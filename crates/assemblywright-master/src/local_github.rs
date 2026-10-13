use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use uuid::Uuid;

const SCHEMA_VERSION: u32 = 1;
const MAX_CAPTURE: usize = 256 * 1024;
const MAX_AUDIT: usize = 500;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const PUBLISH_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const CHECK_POLL_INTERVAL: Duration = Duration::from_millis(750);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Parser)]
#[command(
    name = "assemblywright-github",
    about = "Owner-controlled local GitHub setup and publication"
)]
struct Cli {
    /// Test-only executable injection. Requires ASSEMBLYWRIGHT_GITHUB_TEST_FIXTURE=1.
    #[arg(long, hide = true)]
    fixture_gh: Option<PathBuf>,
    /// Test-only executable injection. Requires ASSEMBLYWRIGHT_GITHUB_TEST_FIXTURE=1.
    #[arg(long, hide = true)]
    fixture_git: Option<PathBuf>,
    /// Test-only required-check deadline. Requires fixture executable injection.
    #[arg(long, hide = true)]
    fixture_check_timeout_ms: Option<u64>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Account {
        #[arg(long)]
        state_dir: PathBuf,
    },
    Status {
        #[arg(long)]
        state_dir: PathBuf,
    },
    Login {
        #[arg(long)]
        state_dir: PathBuf,
    },
    Cancel {
        #[arg(long)]
        state_dir: PathBuf,
    },
    Repositories {
        #[arg(long)]
        state_dir: PathBuf,
    },
    CreateRepository {
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        visibility: Visibility,
    },
    CreateProject {
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        repository: String,
    },
    Connection {
        #[command(subcommand)]
        command: ConnectionCommand,
    },
    Publish {
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        run_id: Uuid,
        #[arg(long)]
        expected_revision: u64,
    },
    Reconcile {
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        run_id: Uuid,
        #[arg(long)]
        expected_revision: u64,
    },
    #[command(hide = true)]
    ProcessLauncher,
}

#[derive(Clone, ValueEnum)]
enum Visibility {
    Private,
    Public,
}

#[derive(Subcommand)]
enum ConnectionCommand {
    Get {
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        project: PathBuf,
    },
    Save {
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        repository: String,
        #[arg(long)]
        base_branch: String,
        #[arg(long)]
        expected_revision: u64,
    },
    Disconnect {
        #[arg(long)]
        state_dir: PathBuf,
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        expected_revision: u64,
    },
}

#[derive(Clone)]
struct Programs {
    gh: PathBuf,
    git: PathBuf,
    check_timeout: Duration,
}

#[derive(Serialize, Deserialize)]
struct ExternalLaunch {
    program: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubState {
    schema_version: u32,
    revision: u64,
    #[serde(default)]
    connections: BTreeMap<String, Connection>,
    #[serde(default)]
    publications: BTreeMap<String, Publication>,
    #[serde(default)]
    audit: Vec<Audit>,
}

impl Default for GithubState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            revision: 0,
            connections: BTreeMap::new(),
            publications: BTreeMap::new(),
            audit: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Connection {
    project: String,
    repository: String,
    repository_url: String,
    base_branch: String,
    source_head: String,
    revision: u64,
    saved_at_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Publication {
    run_id: String,
    project: String,
    repository: String,
    base_branch: String,
    source_head: String,
    connection_revision: u64,
    candidate_commit: String,
    branch: String,
    status: String,
    stage: String,
    #[serde(default)]
    pull_request_number: Option<u64>,
    #[serde(default)]
    pull_request_url: Option<String>,
    #[serde(default)]
    merge_commit: Option<String>,
    #[serde(default)]
    completion_revision: Option<u64>,
    message: String,
    updated_at_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Audit {
    revision: u64,
    action: String,
    at_ms: u128,
    detail: String,
}

#[derive(Deserialize)]
struct LocalReport {
    run_id: String,
    status: String,
    candidate: Option<PathBuf>,
    candidate_commit: Option<String>,
}

#[derive(Deserialize)]
struct SubmittedJob {
    repository: PathBuf,
}

struct Lock(File);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

#[derive(Debug)]
struct Output {
    status: i32,
    stdout: String,
    stderr: String,
}

#[tokio::main]
async fn main() {
    let cli = match Cli::try_parse() {
        Ok(value) => value,
        Err(error) => return emit_error(anyhow!(error.to_string())),
    };
    let programs = match Programs::from_cli(&cli) {
        Ok(value) => value,
        Err(error) => return emit_error(error),
    };
    let result = dispatch(cli.command, programs).await;
    match result {
        Ok(value) => println!(
            "{}",
            serde_json::to_string(&value).expect("JSON serialization")
        ),
        Err(error) => emit_error(error),
    }
}

fn emit_error(error: anyhow::Error) {
    eprintln!("assemblywright-github: {error:#}");
    println!(
        "{}",
        json!({"schema_version":SCHEMA_VERSION,"status":"error","message":format!("{error:#}")})
    );
    std::process::exit(1);
}

impl Programs {
    fn from_cli(cli: &Cli) -> Result<Self> {
        let fixture = cli.fixture_gh.is_some() || cli.fixture_git.is_some();
        let fixture_enabled = std::env::var_os("ASSEMBLYWRIGHT_GITHUB_TEST_FIXTURE").as_deref()
            == Some(std::ffi::OsStr::new("1"));
        if fixture && !fixture_enabled {
            bail!("fixture program overrides are test-only")
        }
        if cli.fixture_check_timeout_ms.is_some() && (!fixture || !fixture_enabled) {
            bail!("fixture check timeout is test-only and requires fixture programs")
        }
        let check_timeout = match cli.fixture_check_timeout_ms {
            Some(0) => bail!("fixture check timeout must be positive"),
            Some(value) if value > PUBLISH_TIMEOUT.as_millis() as u64 => {
                bail!("fixture check timeout exceeds the production bound")
            }
            Some(value) => Duration::from_millis(value),
            None => PUBLISH_TIMEOUT,
        };
        Ok(Self {
            gh: checked_program(
                cli.fixture_gh
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("gh")),
                cli.fixture_gh.is_some(),
            )?,
            git: checked_program(
                cli.fixture_git
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("git")),
                cli.fixture_git.is_some(),
            )?,
            check_timeout,
        })
    }
}

fn checked_program(path: PathBuf, fixture: bool) -> Result<PathBuf> {
    if fixture {
        if !path.is_absolute() || !path.is_file() {
            bail!("fixture program must be an absolute file")
        }
        return fs::canonicalize(path).context("canonicalize fixture program");
    }
    Ok(path)
}

async fn dispatch(command: Commands, programs: Programs) -> Result<Value> {
    match command {
        Commands::Account { state_dir } => account(&state_dir, &programs).await,
        Commands::Status { state_dir } => status(&state_dir),
        Commands::Login { state_dir } => login(&state_dir, &programs).await,
        Commands::Cancel { state_dir } => cancel(&state_dir),
        Commands::Repositories { state_dir } => repositories(&state_dir, &programs).await,
        Commands::CreateRepository {
            state_dir,
            name,
            visibility,
        } => create_repository(&state_dir, &programs, &name, visibility).await,
        Commands::CreateProject {
            state_dir,
            directory,
            repository,
        } => create_project(&state_dir, &programs, &directory, &repository).await,
        Commands::Connection { command } => connection(command, &programs).await,
        Commands::Publish {
            state_dir,
            run_id,
            expected_revision,
        } => publish(&state_dir, &programs, run_id, expected_revision, false).await,
        Commands::Reconcile {
            state_dir,
            run_id,
            expected_revision,
        } => publish(&state_dir, &programs, run_id, expected_revision, true).await,
        Commands::ProcessLauncher => process_launcher(),
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn github_dir(state: &Path) -> PathBuf {
    state.join("github")
}
fn state_path(state: &Path) -> PathBuf {
    github_dir(state).join("connections.json")
}

fn initialize(state: &Path) -> Result<()> {
    if !state.is_absolute() {
        bail!("state directory must be absolute")
    }
    fs::create_dir_all(github_dir(state)).context("create GitHub state directory")?;
    Ok(())
}

fn acquire(path: &Path, label: &str) -> Result<Lock> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    file.try_lock_exclusive()
        .map_err(|_| anyhow!("{label} is busy"))?;
    Ok(Lock(file))
}

fn ensure_unpaused(state: &Path) -> Result<()> {
    if state.join("pause.flag").exists() {
        bail!("local Emergency Pause is active")
    }
    Ok(())
}

fn acquire_runner_guard(state: &Path) -> Result<Lock> {
    acquire(&state.join("runner.lock"), "local runner")
}

fn acquire_control_lock(state: &Path) -> Result<Lock> {
    acquire(&state.join("control.lock"), "local control")
}

fn load_state(state: &Path) -> Result<GithubState> {
    initialize(state)?;
    let path = state_path(state);
    let backup = backup_path(&path);
    if !path.exists() && backup.exists() {
        fs::rename(&backup, &path).context("recover interrupted GitHub state replacement")?;
    }
    if !path.exists() {
        return Ok(GithubState::default());
    }
    let value: GithubState =
        serde_json::from_slice(&fs::read(path)?).context("parse GitHub state")?;
    if value.schema_version != SCHEMA_VERSION {
        bail!("unsupported GitHub state schema")
    }
    Ok(value)
}

fn save_state(state_dir: &Path, state: &GithubState) -> Result<()> {
    atomic_write(&state_path(state_dir), &serde_json::to_vec_pretty(state)?)
}

fn advance(state: &mut GithubState, action: &str, detail: impl Into<String>) {
    state.revision = state.revision.saturating_add(1);
    state.audit.push(Audit {
        revision: state.revision,
        action: action.into(),
        at_ms: now_ms(),
        detail: detail.into(),
    });
    if state.audit.len() > MAX_AUDIT {
        state.audit.drain(0..state.audit.len() - MAX_AUDIT);
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| anyhow!("path has no parent"))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        Uuid::new_v4()
    ));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    let backup = backup_path(path);
    if backup.exists() {
        if path.exists() {
            fs::remove_file(&backup)?;
        } else {
            fs::rename(&backup, path)?;
        }
    }
    if path.exists() {
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(&tmp, path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(error).context("commit durable file replacement");
    }
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        "{}.bak",
        path.file_name().unwrap_or_default().to_string_lossy()
    ))
}

fn status(state_dir: &Path) -> Result<Value> {
    initialize(state_dir)?;
    let _lock = acquire(&github_dir(state_dir).join("state.lock"), "GitHub state")?;
    let mut state = load_state(state_dir)?;
    let invalid = state
        .publications
        .iter()
        .filter(|(_, publication)| {
            publication.status == "succeeded" && verify_receipt(state_dir, publication).is_err()
        })
        .map(|(run_id, _)| run_id.clone())
        .collect::<Vec<_>>();
    let changed = !invalid.is_empty();
    for run_id in invalid {
        if let Some(publication) = state.publications.get_mut(&run_id) {
            publication.status = "attention".into();
            publication.stage = "receipt_verification".into();
            publication.message = "Completed publication receipt is unavailable or invalid".into();
            publication.updated_at_ms = now_ms();
        }
        advance(
            &mut state,
            "publication_receipt_attention",
            format!("run_id={run_id}"),
        );
    }
    if changed {
        save_state(state_dir, &state)?;
    }
    let busy = acquire(
        &github_dir(state_dir).join("publication.lock"),
        "publication",
    )
    .is_err();
    let receipts = state
        .publications
        .iter()
        .map(|(run_id, publication)| {
            let verified =
                publication.status == "succeeded" && verify_receipt(state_dir, publication).is_ok();
            (
                run_id.clone(),
                json!({"verified":verified,"revision":publication.completion_revision}),
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok(
        json!({"schema_version":SCHEMA_VERSION,"status":"ok","revision":state.revision,
        "connections":state.connections.values().collect::<Vec<_>>(),"publications":state.publications,
        "publication_receipts":receipts,"paused":state_dir.join("pause.flag").exists(),"busy":busy}),
    )
}

fn cancel(state_dir: &Path) -> Result<Value> {
    initialize(state_dir)?;
    let _control = acquire_control_lock(state_dir)?;
    atomic_write(
        &github_dir(state_dir).join("cancel.flag"),
        format!("{}\n", now_ms()).as_bytes(),
    )?;
    Ok(json!({"schema_version":SCHEMA_VERSION,"status":"ok","cancelled":true}))
}

async fn account(state_dir: &Path, p: &Programs) -> Result<Value> {
    initialize(state_dir)?;
    let result = run(
        p,
        true,
        &["auth", "status", "--hostname", "github.com"],
        state_dir,
        DEFAULT_TIMEOUT,
    )
    .await?;
    if result.status != 0 {
        return Ok(
            json!({"schema_version":SCHEMA_VERSION,"status":"signed_out","account":null,"message":bounded(&redact_external(&result.stderr))}),
        );
    }
    let login = require_success(
        run(
            p,
            true,
            &["api", "user", "--jq", ".login"],
            state_dir,
            DEFAULT_TIMEOUT,
        )
        .await?,
        "read GitHub account",
    )?;
    Ok(json!({"schema_version":SCHEMA_VERSION,"status":"signed_in","account":login.stdout.trim()}))
}

async fn login(state_dir: &Path, p: &Programs) -> Result<Value> {
    initialize(state_dir)?;
    ensure_unpaused(state_dir)?;
    let _setup = acquire(&github_dir(state_dir).join("setup.lock"), "GitHub setup")?;
    let _runner = acquire_runner_guard(state_dir)?;
    clear_cancel(state_dir)?;
    eprintln!("GitHub will print a one-time device code and authorization URL below.");
    let out = run(
        p,
        true,
        &[
            "auth",
            "login",
            "--hostname",
            "github.com",
            "--git-protocol",
            "https",
            "--web",
        ],
        state_dir,
        Duration::from_secs(15 * 60),
    )
    .await?;
    require_success(out, "GitHub sign-in")?;
    account(state_dir, p).await
}

async fn repositories(state_dir: &Path, p: &Programs) -> Result<Value> {
    initialize(state_dir)?;
    let out = require_success(
        run(
            p,
            true,
            &[
                "repo",
                "list",
                "--limit",
                "100",
                "--json",
                "nameWithOwner,url,visibility,defaultBranchRef",
            ],
            state_dir,
            DEFAULT_TIMEOUT,
        )
        .await?,
        "list repositories",
    )?;
    let repositories: Value = serde_json::from_str(out.stdout.trim())
        .context("GitHub returned invalid repository JSON")?;
    for repository in repositories
        .as_array()
        .ok_or_else(|| anyhow!("GitHub repository result was not an array"))?
    {
        validate_repository_value(repository)?;
    }
    Ok(json!({"schema_version":SCHEMA_VERSION,"status":"ok","repositories":repositories}))
}

async fn create_repository(
    state_dir: &Path,
    p: &Programs,
    name: &str,
    visibility: Visibility,
) -> Result<Value> {
    initialize(state_dir)?;
    ensure_unpaused(state_dir)?;
    let slug = validate_slug(name)?;
    let _setup = acquire(&github_dir(state_dir).join("setup.lock"), "GitHub setup")?;
    let _runner = acquire_runner_guard(state_dir)?;
    clear_cancel(state_dir)?;
    let (vis, expected_visibility) = match visibility {
        Visibility::Private => ("--private", "PRIVATE"),
        Visibility::Public => ("--public", "PUBLIC"),
    };
    let existing = run(
        p,
        true,
        &[
            "repo",
            "view",
            &slug,
            "--json",
            "nameWithOwner,url,defaultBranchRef,visibility",
        ],
        state_dir,
        DEFAULT_TIMEOUT,
    )
    .await?;
    if existing.status == 0 {
        let repo: Value = serde_json::from_str(existing.stdout.trim())?;
        validate_repository_choice(&repo, &slug, expected_visibility, "existing")?;
        return Ok(json!({"schema_version":SCHEMA_VERSION,"status":"existing","repository":repo}));
    } else if !is_repository_not_found(&existing, &slug) {
        bail!(
            "could not determine whether repository already exists: {}",
            bounded(&redact_external(&existing.stderr))
        )
    }
    require_success(
        run(
            p,
            true,
            &["repo", "create", &slug, vis, "--add-readme"],
            state_dir,
            DEFAULT_TIMEOUT,
        )
        .await?,
        "create repository",
    )?;
    let view = require_success(
        run(
            p,
            true,
            &[
                "repo",
                "view",
                &slug,
                "--json",
                "nameWithOwner,url,defaultBranchRef,visibility",
            ],
            state_dir,
            DEFAULT_TIMEOUT,
        )
        .await?,
        "verify created repository",
    )?;
    let repository: Value = serde_json::from_str(view.stdout.trim())?;
    validate_repository_choice(&repository, &slug, expected_visibility, "created")?;
    Ok(json!({"schema_version":SCHEMA_VERSION,"status":"created","repository":repository}))
}

async fn create_project(
    state_dir: &Path,
    _p: &Programs,
    directory: &Path,
    repository: &str,
) -> Result<Value> {
    initialize(state_dir)?;
    ensure_unpaused(state_dir)?;
    let slug = validate_slug(repository)?;
    if !directory.is_absolute() {
        bail!("project directory must be absolute")
    }
    if directory.exists() && (!directory.is_dir() || fs::read_dir(directory)?.next().is_some()) {
        bail!("project directory must be new or empty")
    }
    let _setup = acquire(&github_dir(state_dir).join("setup.lock"), "GitHub setup")?;
    let _runner = acquire_runner_guard(state_dir)?;
    clear_cancel(state_dir)?;
    fs::create_dir_all(directory).context("create empty local project")?;
    let canonical = fs::canonicalize(directory)?.to_string_lossy().into_owned();
    Ok(
        json!({"schema_version":SCHEMA_VERSION,"status":"created","project":canonical,
        "repository_draft":slug,"empty":true,"message":"Empty local project created; no clone, download, upload, or connection occurred"}),
    )
}

async fn connection(command: ConnectionCommand, p: &Programs) -> Result<Value> {
    match command {
        ConnectionCommand::Get { state_dir, project } => {
            initialize(&state_dir)?;
            let _state_lock = acquire(&github_dir(&state_dir).join("state.lock"), "GitHub state")?;
            let key = canonical_directory(&project)?;
            let state = load_state(&state_dir)?;
            Ok(
                json!({"schema_version":SCHEMA_VERSION,"status":"ok","revision":state.revision,"connection":state.connections.get(&key)}),
            )
        }
        ConnectionCommand::Save {
            state_dir,
            project,
            repository,
            base_branch,
            expected_revision,
        } => {
            initialize(&state_dir)?;
            ensure_unpaused(&state_dir)?;
            let _state_lock = acquire(&github_dir(&state_dir).join("state.lock"), "GitHub state")?;
            let _publication = acquire(
                &github_dir(&state_dir).join("publication.lock"),
                "publication",
            )?;
            let _runner = acquire_runner_guard(&state_dir)?;
            let repository = validate_slug(&repository)?;
            validate_branch(&base_branch)?;
            let mut state = load_state(&state_dir)?;
            require_revision(&state, expected_revision)?;
            let project_key = canonical_directory(&project)?;
            if state.publications.values().any(|value| {
                value.project == project_key && !publication_is_resolved(&state_dir, value)
            }) {
                bail!("project has an unresolved publication; reconcile it before changing the connection")
            }
            let remote = remote_repository(p, &state_dir, &repository).await?;
            let prepared =
                prepare_project_for_connection(&project, p, &state_dir, &repository, &base_branch)
                    .await?;
            let project = canonical_project(&project, p, &state_dir).await?;
            if project != project_key {
                bail!("project identity changed while preparing its connection")
            }
            let dirty = git_text(
                p,
                &state_dir,
                &project,
                &["status", "--porcelain=v1", "--untracked-files=all"],
            )
            .await?;
            if !dirty.is_empty() {
                bail!("project must be clean before saving its GitHub connection")
            }
            let local_branch = git_text(
                p,
                &state_dir,
                &project,
                &["symbolic-ref", "--short", "HEAD"],
            )
            .await?;
            if local_branch != base_branch {
                bail!("project must be checked out on the selected base branch")
            }
            let origin =
                git_text(p, &state_dir, &project, &["remote", "get-url", "origin"]).await?;
            let expected_url = format!("https://github.com/{repository}");
            if origin != expected_url && origin != format!("{expected_url}.git") {
                bail!("project origin does not match the selected GitHub repository")
            }
            let remote_head = remote_head(p, &state_dir, &repository, &base_branch).await?;
            let local_head = git_text(p, &state_dir, &project, &["rev-parse", "HEAD"]).await?;
            if local_head != remote_head {
                bail!("project HEAD does not match the selected remote base tip")
            }
            let connection = Connection {
                project: project.clone(),
                repository: repository.clone(),
                repository_url: remote.0,
                base_branch,
                source_head: local_head,
                revision: state.revision.saturating_add(1),
                saved_at_ms: now_ms(),
            };
            state
                .connections
                .insert(project.clone(), connection.clone());
            advance(
                &mut state,
                "connection_saved",
                format!("project={project};repository={repository}"),
            );
            save_state(&state_dir, &state)?;
            Ok(
                json!({"schema_version":SCHEMA_VERSION,"status":"saved","revision":state.revision,
                "connection":connection,"prepared_checkout":prepared}),
            )
        }
        ConnectionCommand::Disconnect {
            state_dir,
            project,
            expected_revision,
        } => {
            initialize(&state_dir)?;
            ensure_unpaused(&state_dir)?;
            let _state_lock = acquire(&github_dir(&state_dir).join("state.lock"), "GitHub state")?;
            let _publication = acquire(
                &github_dir(&state_dir).join("publication.lock"),
                "publication",
            )?;
            let _runner = acquire_runner_guard(&state_dir)?;
            let project = canonical_directory(&project)?;
            let mut state = load_state(&state_dir)?;
            require_revision(&state, expected_revision)?;
            if state.publications.values().any(|value| {
                value.project == project && !publication_is_resolved(&state_dir, value)
            }) {
                bail!("project has an unresolved publication; reconcile it before changing the connection")
            }
            if state.connections.remove(&project).is_none() {
                bail!("project is not connected")
            }
            advance(
                &mut state,
                "connection_disconnected",
                format!("project={project}"),
            );
            save_state(&state_dir, &state)?;
            Ok(
                json!({"schema_version":SCHEMA_VERSION,"status":"disconnected","revision":state.revision,"project":project}),
            )
        }
    }
}

fn require_revision(state: &GithubState, expected: u64) -> Result<()> {
    if state.revision != expected {
        bail!(
            "stale revision: expected {expected}, current {}",
            state.revision
        )
    }
    Ok(())
}

async fn prepare_project_for_connection(
    path: &Path,
    p: &Programs,
    state: &Path,
    repository: &str,
    base_branch: &str,
) -> Result<bool> {
    if !path.is_absolute() {
        bail!("project path must be absolute")
    }
    if !path.exists() {
        bail!("create the empty local project before saving its connection")
    }
    if !path.is_dir() {
        bail!("project must be a directory")
    }
    if fs::read_dir(path)?.next().is_none() {
        let directory = path
            .to_str()
            .ok_or_else(|| anyhow!("project path is not Unicode"))?;
        eprintln!("[github] preparing the explicitly connected repository baseline in the empty local project");
        require_success(
            run(
                p,
                true,
                &[
                    "repo",
                    "clone",
                    repository,
                    directory,
                    "--",
                    "--branch",
                    base_branch,
                    "--single-branch",
                    "--origin",
                    "origin",
                ],
                state,
                DEFAULT_TIMEOUT,
            )
            .await?,
            "prepare connected project checkout",
        )?;
        return Ok(true);
    }
    Ok(false)
}

fn checkpoint(
    state_dir: &Path,
    state: &mut GithubState,
    publication: &Publication,
    action: &str,
) -> Result<()> {
    let _state_lock = acquire(&github_dir(state_dir).join("state.lock"), "GitHub state")?;
    state
        .publications
        .insert(publication.run_id.clone(), publication.clone());
    advance(
        state,
        action,
        format!("run_id={};stage={}", publication.run_id, publication.stage),
    );
    save_state(state_dir, state)
}

async fn publish(
    state_dir: &Path,
    p: &Programs,
    run_id: Uuid,
    expected_revision: u64,
    reconcile: bool,
) -> Result<Value> {
    initialize(state_dir)?;
    ensure_unpaused(state_dir)?;
    let _publication = acquire(
        &github_dir(state_dir).join("publication.lock"),
        "publication",
    )?;
    let _runner = acquire_runner_guard(state_dir)?;
    clear_cancel(state_dir)?;
    let mut state = {
        let _state_lock = acquire(&github_dir(state_dir).join("state.lock"), "GitHub state")?;
        let state = load_state(state_dir)?;
        require_revision(&state, expected_revision)?;
        state
    };
    let id = run_id.to_string();
    let retained = state.publications.get(&id).cloned();
    if let Some(done) = &retained {
        if done.status == "succeeded" {
            if let Err(error) = verify_receipt(state_dir, done) {
                let mut attention = done.clone();
                attention.status = "attention".into();
                attention.stage = "receipt_verification".into();
                attention.message =
                    format!("completed publication receipt is unavailable or invalid: {error:#}");
                attention.updated_at_ms = now_ms();
                checkpoint(
                    state_dir,
                    &mut state,
                    &attention,
                    "publication_receipt_attention",
                )?;
                bail!("completed publication receipt is unavailable or invalid")
            }
            return Ok(
                json!({"schema_version":SCHEMA_VERSION,"status":"succeeded","revision":state.revision,"publication":done}),
            );
        }
        if !reconcile {
            bail!("publication already exists and requires the explicit reconcile action")
        }
    } else if reconcile {
        bail!("there is no retained publication to reconcile")
    }
    let run_dir = state_dir.join("runs").join(&id);
    let report: LocalReport = serde_json::from_slice(
        &fs::read(run_dir.join("report.json")).context("read completed report")?,
    )
    .context("parse completed report")?;
    if report.run_id != id || report.status != "completed" {
        bail!("run does not have an exact completed report")
    }
    let job: SubmittedJob = serde_json::from_slice(
        &fs::read(run_dir.join("submitted-job.json")).context("read submitted job")?,
    )
    .context("parse submitted job")?;
    let source_head = fs::read_to_string(run_dir.join("source-head.txt"))?
        .trim()
        .to_owned();
    validate_commit(&source_head)?;
    let project = fs::canonicalize(&job.repository)
        .context("canonicalize submitted project")?
        .to_string_lossy()
        .into_owned();
    let candidate = fs::canonicalize(
        report
            .candidate
            .ok_or_else(|| anyhow!("completed report lacks candidate"))?,
    )
    .context("canonicalize candidate")?;
    let candidate_commit = report
        .candidate_commit
        .ok_or_else(|| anyhow!("completed report lacks candidate commit"))?;
    validate_commit(&candidate_commit)?;
    let candidate_path = candidate.to_string_lossy().into_owned();
    let actual_candidate = git_text(p, state_dir, &candidate_path, &["rev-parse", "HEAD"]).await?;
    if actual_candidate != candidate_commit {
        bail!("candidate HEAD changed after completion")
    }
    let clean = git_text(
        p,
        state_dir,
        &candidate_path,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )
    .await?;
    if !clean.is_empty() {
        bail!("candidate is not clean")
    }
    let expected_candidate =
        fs::canonicalize(run_dir.join("candidate")).context("canonicalize retained candidate")?;
    if candidate != expected_candidate {
        bail!("completed report candidate is not the retained run candidate")
    }
    let candidate_root = git_text(
        p,
        state_dir,
        &candidate_path,
        &["rev-parse", "--show-toplevel"],
    )
    .await?;
    if fs::canonicalize(candidate_root)? != candidate {
        bail!("candidate path must be the Git worktree root")
    }
    let remotes = git_text(p, state_dir, &candidate_path, &["remote"]).await?;
    if !remotes.is_empty() {
        bail!("retained candidate unexpectedly has a Git remote")
    }
    let ancestor = run_git(
        p,
        state_dir,
        &candidate_path,
        &[
            "merge-base",
            "--is-ancestor",
            &source_head,
            &candidate_commit,
        ],
        DEFAULT_TIMEOUT,
    )
    .await?;
    if ancestor.status != 0 {
        bail!("reviewed candidate does not descend from the frozen source HEAD")
    }
    let branch = format!("assemblywright/{id}");
    let mut publication = if let Some(publication) = retained {
        publication
    } else {
        let connection = state
            .connections
            .get(&project)
            .cloned()
            .ok_or_else(|| anyhow!("completed run project is not connected"))?;
        if connection.source_head != source_head || connection.revision == 0 {
            bail!("connection is not bound to the run's frozen source HEAD")
        }
        Publication {
            run_id: id.clone(),
            project: project.clone(),
            repository: connection.repository,
            base_branch: connection.base_branch,
            source_head: source_head.clone(),
            connection_revision: connection.revision,
            candidate_commit: candidate_commit.clone(),
            branch: branch.clone(),
            status: "running".into(),
            stage: "prepare_candidate".into(),
            pull_request_number: None,
            pull_request_url: None,
            merge_commit: None,
            completion_revision: None,
            message: "Preparing exact reviewed candidate".into(),
            updated_at_ms: now_ms(),
        }
    };
    if publication.run_id != id
        || publication.project != project
        || publication.source_head != source_head
        || publication.candidate_commit != candidate_commit
        || publication.branch != branch
        || publication.connection_revision == 0
        || validate_slug(&publication.repository).is_err()
        || validate_branch(&publication.base_branch).is_err()
    {
        bail!("retained publication binding mismatch")
    }
    publication.status = "running".into();
    publication.updated_at_ms = now_ms();
    checkpoint(state_dir, &mut state, &publication, "publication_started")?;

    let outcome = publish_owned(state_dir, p, &candidate_path, &mut state, &mut publication).await;
    match outcome {
        Ok(()) => {
            publication.status = "succeeded".into();
            publication.stage = "complete".into();
            publication.message = "GitHub reports the exact pull request merged and the base branch advanced to its merge commit".into();
            publication.updated_at_ms = now_ms();
            let _control = acquire_control_lock_wait(state_dir).await?;
            stop_check(state_dir)?;
            let final_revision = state.revision.saturating_add(1);
            publication.completion_revision = Some(final_revision);
            if let Err(error) = write_receipt(state_dir, &publication) {
                publication.status = "attention".into();
                publication.stage = "receipt_verification".into();
                publication.message =
                    format!("merged publication receipt could not be committed: {error:#}");
                checkpoint(
                    state_dir,
                    &mut state,
                    &publication,
                    "publication_receipt_attention",
                )?;
                bail!("merged publication receipt could not be committed")
            }
            checkpoint(state_dir, &mut state, &publication, "publication_succeeded")?;
            verify_receipt(state_dir, &publication)?;
            Ok(
                json!({"schema_version":SCHEMA_VERSION,"status":"succeeded","revision":state.revision,"publication":publication}),
            )
        }
        Err(error) => {
            publication.status = if cancelled(state_dir) {
                "cancelled".into()
            } else {
                "attention".into()
            };
            publication.message = format!("{error:#}");
            publication.updated_at_ms = now_ms();
            checkpoint(state_dir, &mut state, &publication, "publication_attention")?;
            Err(anyhow!(
                "publication stopped at {}: {error:#}",
                publication.stage
            ))
        }
    }
}

async fn publish_owned(
    state: &Path,
    p: &Programs,
    candidate: &str,
    github_state: &mut GithubState,
    publication: &mut Publication,
) -> Result<()> {
    stop_check(state)?;
    let existing_pr = find_pr(p, state, publication).await?;
    if let Some(pr) = &existing_pr {
        if pr.get("state").and_then(Value::as_str) == Some("MERGED") {
            publication.stage = "verify_remote_base".into();
            publication.updated_at_ms = now_ms();
            checkpoint(
                state,
                github_state,
                publication,
                "publication_reconcile_merge",
            )?;
            verify_merged(state, p, publication, pr).await?;
            return sync_project_after_merge(state, p, github_state, publication).await;
        }
    }
    let base_tip = remote_head(p, state, &publication.repository, &publication.base_branch).await?;
    if base_tip != publication.source_head {
        bail!("remote base changed from the frozen source HEAD")
    }
    publication.stage = "push_branch".into();
    publication.updated_at_ms = now_ms();
    let remote_branch =
        remote_branch_head(p, state, &publication.repository, &publication.branch).await?;
    match remote_branch {
        Some(head) if head == publication.candidate_commit => {}
        Some(_) => bail!("retained publication branch points to a different commit"),
        None => {
            stop_check(state)?;
            checkpoint(state, github_state, publication, "publication_push_intent")?;
            let remote = format!("https://github.com/{}.git", publication.repository);
            let refspec = format!("HEAD:refs/heads/{}", publication.branch);
            require_success(
                run_git(
                    p,
                    state,
                    candidate,
                    &["push", "--porcelain", &remote, &refspec],
                    PUBLISH_TIMEOUT,
                )
                .await?,
                "push exact candidate branch",
            )?;
        }
    }
    stop_check(state)?;
    publication.stage = "open_pull_request".into();
    publication.updated_at_ms = now_ms();
    let mut pr = existing_pr;
    if pr.is_none() {
        checkpoint(state, github_state, publication, "publication_pr_intent")?;
        let title = format!("Assemblywright run {}", publication.run_id);
        let body = format!(
            "Owner-approved Assemblywright local run `{}`. Candidate `{}`.",
            publication.run_id, publication.candidate_commit
        );
        require_success(
            run(
                p,
                true,
                &[
                    "pr",
                    "create",
                    "--repo",
                    &publication.repository,
                    "--head",
                    &publication.branch,
                    "--base",
                    &publication.base_branch,
                    "--title",
                    &title,
                    "--body",
                    &body,
                ],
                state,
                PUBLISH_TIMEOUT,
            )
            .await?,
            "create pull request",
        )?;
        pr = find_pr(p, state, publication).await?;
    }
    let pr = pr.ok_or_else(|| anyhow!("pull request was not observable after creation"))?;
    bind_pr(publication, &pr)?;
    if pr.get("state").and_then(Value::as_str) == Some("MERGED") {
        verify_merged(state, p, publication, &pr).await?;
        return sync_project_after_merge(state, p, github_state, publication).await;
    }
    publication.stage = "wait_required_checks".into();
    publication.updated_at_ms = now_ms();
    checkpoint(state, github_state, publication, "publication_checks_wait")?;
    let number = publication.pull_request_number.unwrap().to_string();
    wait_for_required_checks(state, p, publication, &number).await?;
    stop_check(state)?;
    let current_pr = find_pr(p, state, publication)
        .await?
        .ok_or_else(|| anyhow!("pull request disappeared"))?;
    bind_pr(publication, &current_pr)?;
    let current_base =
        remote_head(p, state, &publication.repository, &publication.base_branch).await?;
    if current_base != publication.source_head {
        bail!("remote base changed while checks ran")
    }
    publication.stage = "merge_pull_request".into();
    publication.updated_at_ms = now_ms();
    checkpoint(state, github_state, publication, "publication_merge_intent")?;
    require_success(
        run(
            p,
            true,
            &[
                "pr",
                "merge",
                &number,
                "--repo",
                &publication.repository,
                "--merge",
                "--match-head-commit",
                &publication.candidate_commit,
            ],
            state,
            PUBLISH_TIMEOUT,
        )
        .await?,
        "merge pull request",
    )?;
    publication.stage = "verify_remote_base".into();
    publication.updated_at_ms = now_ms();
    checkpoint(state, github_state, publication, "publication_verify_merge")?;
    let merged = find_pr(p, state, publication)
        .await?
        .ok_or_else(|| anyhow!("merged pull request is not observable"))?;
    verify_merged(state, p, publication, &merged).await?;
    sync_project_after_merge(state, p, github_state, publication).await
}

#[derive(Debug, PartialEq, Eq)]
enum RequiredCheckPoll {
    Waiting,
    Passed,
}

async fn wait_for_required_checks(
    state: &Path,
    p: &Programs,
    publication: &Publication,
    number: &str,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + p.check_timeout;
    loop {
        stop_check(state)?;
        let timeout = remaining_check_timeout(deadline)?;
        let output = run(
            p,
            true,
            &[
                "pr",
                "checks",
                number,
                "--repo",
                &publication.repository,
                "--required",
                "--json",
                "bucket,name,state,workflow",
            ],
            state,
            timeout.min(DEFAULT_TIMEOUT),
        )
        .await;
        let output = match output {
            Ok(output) => output,
            Err(error) if tokio::time::Instant::now() >= deadline => {
                bail!("required-check wait exceeded its bounded deadline: {error:#}")
            }
            Err(error) => return Err(error).context("read required checks"),
        };
        match classify_required_checks(&output, &publication.branch)? {
            RequiredCheckPoll::Passed => return Ok(()),
            RequiredCheckPoll::Waiting => {
                eprintln!("[github] waiting for required checks");
                wait_until_next_check_poll(state, deadline).await?;
            }
        }
    }
}

fn remaining_check_timeout(deadline: tokio::time::Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(tokio::time::Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| anyhow!("required-check wait exceeded its bounded deadline"))
}

async fn wait_until_next_check_poll(state: &Path, deadline: tokio::time::Instant) -> Result<()> {
    let poll_deadline = (tokio::time::Instant::now() + CHECK_POLL_INTERVAL).min(deadline);
    loop {
        stop_check(state)?;
        let now = tokio::time::Instant::now();
        if now >= poll_deadline {
            return remaining_check_timeout(deadline).map(|_| ());
        }
        tokio::time::sleep(STOP_POLL_INTERVAL.min(poll_deadline - now)).await;
    }
}

fn classify_required_checks(output: &Output, branch: &str) -> Result<RequiredCheckPoll> {
    let no_checks = format!("no checks reported on the '{branch}' branch");
    let no_required = format!("no required checks reported on the '{branch}' branch");
    if output.status != 0 && output.status != 8 {
        let stderr = output.stderr.trim();
        if stderr == no_checks || stderr == no_required {
            return Ok(RequiredCheckPoll::Waiting);
        }
        bail!(
            "read required checks failed: {}",
            bounded(&redact_external(&output.stderr))
        )
    }
    let checks: Vec<Value> = serde_json::from_str(output.stdout.trim())
        .context("GitHub returned invalid required-check JSON")?;
    if checks.is_empty() {
        bail!("GitHub reported no required checks")
    }
    let mut pending = false;
    for check in checks {
        let name = check
            .get("name")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("GitHub returned an invalid required-check name"))?;
        check
            .get("state")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("GitHub returned an invalid state for required check {name}"))?;
        check
            .get("workflow")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                anyhow!("GitHub returned an invalid workflow for required check {name}")
            })?;
        match check.get("bucket").and_then(Value::as_str) {
            Some("pass") => {}
            Some("pending") => pending = true,
            Some("fail" | "cancel" | "skipping") => {
                bail!("required check {name} did not pass")
            }
            _ => bail!("GitHub returned an unknown bucket for required check {name}"),
        }
    }
    if output.status == 8 && !pending {
        bail!("GitHub reported checks pending without a pending required check")
    }
    Ok(if pending {
        RequiredCheckPoll::Waiting
    } else {
        RequiredCheckPoll::Passed
    })
}

async fn sync_project_after_merge(
    state_dir: &Path,
    p: &Programs,
    state: &mut GithubState,
    publication: &mut Publication,
) -> Result<()> {
    publication.stage = "sync_local_project".into();
    publication.updated_at_ms = now_ms();
    checkpoint(state_dir, state, publication, "publication_sync_intent")?;
    let project = canonical_project(Path::new(&publication.project), p, state_dir).await?;
    if project != publication.project {
        bail!("connected project identity changed")
    }
    let dirty = git_text(
        p,
        state_dir,
        &project,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )
    .await?;
    if !dirty.is_empty() {
        bail!("connected project became dirty before post-merge synchronization")
    }
    let branch = git_text(p, state_dir, &project, &["symbolic-ref", "--short", "HEAD"]).await?;
    if branch != publication.base_branch {
        bail!("connected project is no longer on its authorized base branch")
    }
    let origin = git_text(p, state_dir, &project, &["remote", "get-url", "origin"]).await?;
    let expected_url = format!("https://github.com/{}", publication.repository);
    if origin != expected_url && origin != format!("{expected_url}.git") {
        bail!("connected project origin changed before synchronization")
    }
    let merge = publication
        .merge_commit
        .as_deref()
        .ok_or_else(|| anyhow!("confirmed merge commit is unavailable"))?;
    let current = git_text(p, state_dir, &project, &["rev-parse", "HEAD"]).await?;
    if current != merge {
        if current != publication.source_head {
            bail!("connected project HEAD changed from the frozen source before synchronization")
        }
        stop_check(state_dir)?;
        let refspec = format!(
            "refs/heads/{}:refs/remotes/origin/{}",
            publication.base_branch, publication.base_branch
        );
        require_success(
            run_git(
                p,
                state_dir,
                &project,
                &["fetch", "--no-tags", "origin", &refspec],
                PUBLISH_TIMEOUT,
            )
            .await?,
            "fetch confirmed merged base",
        )?;
        let remote_ref = format!("refs/remotes/origin/{}", publication.base_branch);
        let fetched = git_text(p, state_dir, &project, &["rev-parse", &remote_ref]).await?;
        if fetched != merge {
            bail!("fetched base does not equal the confirmed merge commit")
        }
        stop_check(state_dir)?;
        checkpoint(
            state_dir,
            state,
            publication,
            "publication_sync_fast_forward_intent",
        )?;
        require_success(
            run_git(
                p,
                state_dir,
                &project,
                &["merge", "--ff-only", merge],
                DEFAULT_TIMEOUT,
            )
            .await?,
            "fast-forward connected project to confirmed merge",
        )?;
    }
    let final_head = git_text(p, state_dir, &project, &["rev-parse", "HEAD"]).await?;
    let final_dirty = git_text(
        p,
        state_dir,
        &project,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )
    .await?;
    if final_head != merge || !final_dirty.is_empty() {
        bail!("connected project did not reach the exact clean confirmed merge")
    }
    let connection = state
        .connections
        .get_mut(&project)
        .ok_or_else(|| anyhow!("connected project grant disappeared during synchronization"))?;
    if connection.repository != publication.repository
        || connection.base_branch != publication.base_branch
        || (connection.source_head != publication.source_head && connection.source_head != merge)
    {
        bail!("connected project grant changed during synchronization")
    }
    if connection.source_head != merge {
        connection.source_head = merge.to_owned();
        connection.revision = state.revision.saturating_add(1);
        advance(
            state,
            "connection_advanced_after_merge",
            format!("project={project};commit={merge}"),
        );
        save_state(state_dir, state)?;
    }
    Ok(())
}

fn bind_pr(publication: &mut Publication, pr: &Value) -> Result<()> {
    if pr.get("isCrossRepository").and_then(Value::as_bool) != Some(false) {
        bail!("pull request head is not owned by the selected repository")
    }
    let head = pr
        .get("headRefOid")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("pull request lacks head commit"))?;
    if head != publication.candidate_commit {
        bail!("pull request head does not match reviewed candidate")
    }
    let head_name = pr
        .get("headRefName")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("pull request lacks head branch"))?;
    if head_name != publication.branch {
        bail!("pull request head branch changed")
    }
    let base = pr
        .get("baseRefName")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("pull request lacks base branch"))?;
    if base != publication.base_branch {
        bail!("pull request base changed")
    }
    let number = pr
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("pull request lacks number"))?;
    let url = pr
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("pull request lacks URL"))?;
    validate_pr_url(url, &publication.repository, number)?;
    if let Some(existing) = publication.pull_request_number {
        if existing != number {
            bail!("publication is bound to a different pull request")
        }
    }
    publication.pull_request_number = Some(number);
    publication.pull_request_url = Some(url.into());
    Ok(())
}

async fn verify_merged(
    state: &Path,
    p: &Programs,
    publication: &mut Publication,
    pr: &Value,
) -> Result<()> {
    bind_pr(publication, pr)?;
    if pr.get("state").and_then(Value::as_str) != Some("MERGED") {
        bail!("GitHub has not confirmed the pull request as merged")
    }
    let merge = pr
        .get("mergeCommit")
        .and_then(|v| v.get("oid"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("merged pull request lacks merge commit"))?
        .to_owned();
    validate_commit(&merge)?;
    let endpoint = format!("repos/{}/git/commits/{merge}", publication.repository);
    let commit = require_success(
        run(p, true, &["api", &endpoint], state, DEFAULT_TIMEOUT).await?,
        "inspect confirmed merge commit",
    )?;
    let commit: Value =
        serde_json::from_str(commit.stdout.trim()).context("invalid merge-commit JSON")?;
    let parents = commit
        .get("parents")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("merge commit lacks parent evidence"))?;
    if parents.len() != 2
        || parents[0].get("sha").and_then(Value::as_str) != Some(&publication.source_head)
        || parents[1].get("sha").and_then(Value::as_str) != Some(&publication.candidate_commit)
    {
        bail!("merge commit parents do not bind the frozen base and reviewed candidate")
    }
    let base = remote_head(p, state, &publication.repository, &publication.base_branch).await?;
    if base != merge {
        bail!("remote base does not equal the confirmed merge commit")
    }
    publication.merge_commit = Some(merge);
    Ok(())
}

async fn find_pr(p: &Programs, state: &Path, publication: &Publication) -> Result<Option<Value>> {
    let out = require_success(
        run(
            p,
            true,
            &[
                "pr",
                "list",
                "--repo",
                &publication.repository,
                "--state",
                "all",
                "--head",
                &publication.branch,
                "--base",
                &publication.base_branch,
                "--json",
                "number,url,state,headRefOid,headRefName,baseRefName,mergeCommit,isCrossRepository",
            ],
            state,
            DEFAULT_TIMEOUT,
        )
        .await?,
        "find publication pull request",
    )?;
    let values: Vec<Value> =
        serde_json::from_str(out.stdout.trim()).context("invalid pull-request JSON")?;
    if values.len() > 1 {
        bail!("multiple pull requests match the exact publication branch")
    }
    Ok(values.into_iter().next())
}

fn write_receipt(state: &Path, publication: &Publication) -> Result<()> {
    let dir = github_dir(state)
        .join("publications")
        .join(&publication.run_id);
    let revision = publication
        .completion_revision
        .ok_or_else(|| anyhow!("publication lacks completion revision"))?;
    let receipt = json!({"schema_version":SCHEMA_VERSION,"status":"succeeded","revision":revision,
        "run_id":publication.run_id,"project":publication.project,"repository":publication.repository,
        "connection_revision":publication.connection_revision,"base_branch":publication.base_branch,
        "source_head":publication.source_head,"candidate_commit":publication.candidate_commit,"branch":publication.branch,
        "pull_request_number":publication.pull_request_number,"pull_request_url":publication.pull_request_url,
        "merge_commit":publication.merge_commit,"observed_at_ms":now_ms()});
    atomic_write(
        &dir.join("publication.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )
}

fn verify_receipt(state: &Path, publication: &Publication) -> Result<()> {
    let path = github_dir(state)
        .join("publications")
        .join(&publication.run_id)
        .join("publication.json");
    let backup = backup_path(&path);
    if !path.exists() && backup.exists() {
        fs::rename(&backup, &path)
            .context("recover interrupted publication receipt replacement")?;
    }
    let value: Value = serde_json::from_slice(&fs::read(path).context("read publication receipt")?)
        .context("parse publication receipt")?;
    let revision = publication
        .completion_revision
        .ok_or_else(|| anyhow!("publication lacks completion revision"))?;
    let expected = [
        ("schema_version", json!(SCHEMA_VERSION)),
        ("status", json!("succeeded")),
        ("revision", json!(revision)),
        ("run_id", json!(publication.run_id)),
        ("project", json!(publication.project)),
        ("repository", json!(publication.repository)),
        (
            "connection_revision",
            json!(publication.connection_revision),
        ),
        ("base_branch", json!(publication.base_branch)),
        ("source_head", json!(publication.source_head)),
        ("candidate_commit", json!(publication.candidate_commit)),
        ("branch", json!(publication.branch)),
        (
            "pull_request_number",
            json!(publication.pull_request_number),
        ),
        ("pull_request_url", json!(publication.pull_request_url)),
        ("merge_commit", json!(publication.merge_commit)),
    ];
    for (field, expected) in expected {
        if value.get(field) != Some(&expected) {
            bail!("publication receipt field {field} does not match durable state")
        }
    }
    Ok(())
}

fn publication_is_resolved(state: &Path, publication: &Publication) -> bool {
    publication.status == "succeeded" && verify_receipt(state, publication).is_ok()
}

async fn canonical_project(path: &Path, p: &Programs, state: &Path) -> Result<String> {
    if !path.is_absolute() {
        bail!("project path must be absolute")
    }
    let canonical = fs::canonicalize(path).context("canonicalize project")?;
    if !canonical.is_dir() {
        bail!("project must be a directory")
    }
    let text = canonical.to_string_lossy().into_owned();
    let root = git_text(p, state, &text, &["rev-parse", "--show-toplevel"]).await?;
    let root = fs::canonicalize(root).context("canonicalize Git root")?;
    if root != canonical {
        bail!("project path must be the Git worktree root")
    }
    Ok(canonical.to_string_lossy().into_owned())
}

fn canonical_directory(path: &Path) -> Result<String> {
    if !path.is_absolute() {
        bail!("project path must be absolute")
    }
    let canonical = fs::canonicalize(path).context("canonicalize project")?;
    if !canonical.is_dir() {
        bail!("project must be a directory")
    }
    Ok(canonical.to_string_lossy().into_owned())
}

async fn remote_repository(p: &Programs, state: &Path, slug: &str) -> Result<(String, Value)> {
    let out = require_success(
        run(
            p,
            true,
            &[
                "repo",
                "view",
                slug,
                "--json",
                "nameWithOwner,url,defaultBranchRef",
            ],
            state,
            DEFAULT_TIMEOUT,
        )
        .await?,
        "read repository",
    )?;
    let value: Value =
        serde_json::from_str(out.stdout.trim()).context("invalid repository JSON")?;
    validate_repository_value(&value)?;
    let canonical = value.get("nameWithOwner").and_then(Value::as_str).unwrap();
    if !canonical.eq_ignore_ascii_case(slug) {
        bail!("GitHub returned a different repository")
    }
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("repository lacks URL"))?;
    validate_repo_url(url, canonical)?;
    Ok((url.into(), value))
}

fn validate_repository_value(value: &Value) -> Result<()> {
    let canonical = value
        .get("nameWithOwner")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("repository lacks nameWithOwner"))?;
    validate_slug(canonical)?;
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("repository lacks URL"))?;
    validate_repo_url(url, canonical)
}

fn validate_repository_choice(
    value: &Value,
    requested_slug: &str,
    requested_visibility: &str,
    disposition: &str,
) -> Result<()> {
    validate_repository_value(value)?;
    let canonical = value
        .get("nameWithOwner")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("repository lacks nameWithOwner"))?;
    if !canonical.eq_ignore_ascii_case(requested_slug) {
        bail!("GitHub returned a different repository")
    }
    if value
        .get("visibility")
        .and_then(Value::as_str)
        .is_none_or(|visibility| !visibility.eq_ignore_ascii_case(requested_visibility))
    {
        bail!("{disposition} repository visibility does not match the owner's explicit choice")
    }
    Ok(())
}

async fn remote_head(p: &Programs, state: &Path, slug: &str, branch: &str) -> Result<String> {
    let endpoint = format!("repos/{slug}/git/ref/heads/{branch}");
    let out = require_success(
        run(
            p,
            true,
            &["api", &endpoint, "--jq", ".object.sha"],
            state,
            DEFAULT_TIMEOUT,
        )
        .await?,
        "read remote branch",
    )?;
    let head = out.stdout.trim().to_owned();
    validate_commit(&head)?;
    Ok(head)
}

async fn remote_branch_head(
    p: &Programs,
    state: &Path,
    slug: &str,
    branch: &str,
) -> Result<Option<String>> {
    let endpoint = format!("repos/{slug}/git/ref/heads/{branch}");
    let out = run(
        p,
        true,
        &["api", &endpoint, "--jq", ".object.sha"],
        state,
        DEFAULT_TIMEOUT,
    )
    .await?;
    if out.status != 0 {
        if is_not_found(&out) {
            return Ok(None);
        }
        bail!(
            "could not determine whether publication branch exists: {}",
            bounded(&redact_external(&out.stderr))
        )
    }
    let head = out.stdout.trim().to_owned();
    validate_commit(&head)?;
    Ok(Some(head))
}

async fn git_text(p: &Programs, state: &Path, cwd: &str, args: &[&str]) -> Result<String> {
    let out = require_success(
        run_git(p, state, cwd, args, DEFAULT_TIMEOUT).await?,
        "inspect Git repository",
    )?;
    Ok(out.stdout.trim().to_owned())
}

async fn run_git(
    p: &Programs,
    state: &Path,
    cwd: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Output> {
    let mut all = vec![
        "-c",
        "core.hooksPath=NUL",
        "-c",
        "core.fsmonitor=false",
        "-C",
        cwd,
    ];
    all.extend_from_slice(args);
    run(p, false, &all, state, timeout).await
}

fn process_launcher() -> Result<Value> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::io::stdin().take(1024 * 1024).read_to_end(&mut bytes)?;
    let launch: ExternalLaunch =
        serde_json::from_slice(&bytes).context("decode external-command launch permit")?;
    if !launch.cwd.is_absolute() {
        bail!("launcher working directory must be absolute")
    }
    let status = std::process::Command::new(&launch.program)
        .args(&launch.args)
        .current_dir(&launch.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("launch permitted external command")?;
    std::process::exit(status.code().unwrap_or(1))
}

async fn run(
    p: &Programs,
    gh: bool,
    args: &[&str],
    state: &Path,
    timeout: Duration,
) -> Result<Output> {
    let program = if gh { &p.gh } else { &p.git };
    eprintln!(
        "[{}] {}",
        if gh { "github" } else { "git" },
        args.first().copied().unwrap_or("command")
    );
    let launch = ExternalLaunch {
        program: program.clone(),
        args: args.iter().map(|value| (*value).to_owned()).collect(),
        cwd: state.to_path_buf(),
    };
    let launch_bytes = serde_json::to_vec(&launch)?;
    let job = ExternalJob::new().context("create external-command Job Object")?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("process-launcher")
        .current_dir(state)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .kill_on_drop(true);
    for key in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GH_ENTERPRISE_TOKEN",
        "GH_HOST",
        "GH_REPO",
        "GH_CONFIG_DIR",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_COUNT",
        "GIT_EXEC_PATH",
        "GIT_SSH",
        "GIT_SSH_COMMAND",
        "GIT_ASKPASS",
        "SSH_ASKPASS",
        "GIT_PROXY_COMMAND",
        "GIT_EXTERNAL_DIFF",
    ] {
        command.env_remove(key);
    }
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if name.starts_with("GIT_CONFIG_KEY_") || name.starts_with("GIT_CONFIG_VALUE_") {
            command.env_remove(key);
        }
    }
    let mut child = command
        .spawn()
        .context("launch inert external-command process")?;
    if let Err(error) = job.assign(&child) {
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Err(error).context("assign external command to Job Object");
    }
    let control = acquire_control_lock_wait(state).await?;
    if let Err(error) = stop_check(state) {
        drop(control);
        terminate(&mut child, &job).await;
        bail!("{error:#}")
    }
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("launcher stdin unavailable"))?;
    if stdin.write_all(&launch_bytes).await.is_err() || stdin.shutdown().await.is_err() {
        drop(control);
        terminate(&mut child, &job).await;
        bail!("could not deliver the external-command launch permit")
    }
    drop(stdin);
    drop(control);
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mirror_login_stdout = gh && args.starts_with(&["auth", "login"]);
    let stdout_task =
        tokio::spawn(async move { read_bounded(&mut stdout, mirror_login_stdout).await });
    let mirror_login_stderr = mirror_login_stdout;
    let stderr_task =
        tokio::spawn(async move { read_bounded(&mut stderr, mirror_login_stderr).await });
    let deadline = tokio::time::Instant::now() + timeout;
    let status = loop {
        tokio::select! {
            value = child.wait() => break value?,
            _ = tokio::time::sleep(Duration::from_millis(150)) => {
                if cancelled(state) || state.join("pause.flag").exists() { terminate(&mut child, &job).await; bail!("operation cancelled or paused") }
                if tokio::time::Instant::now() >= deadline { terminate(&mut child, &job).await; bail!("external command timed out") }
            }
        }
    };
    if !job.wait_empty(Duration::from_millis(500)).await? {
        terminate(&mut child, &job).await;
        bail!("external command left a descendant process")
    }
    let _control = acquire_control_lock_wait(state).await?;
    stop_check(state)?;
    let stdout = stdout_task.await??;
    let stderr = stderr_task.await??;
    Ok(Output {
        status: status.code().unwrap_or(-1),
        stdout,
        stderr,
    })
}

async fn read_bounded<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut R,
    mirror_stderr: bool,
) -> Result<String> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut mirror_buffer = String::new();
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        if mirror_stderr {
            mirror_buffer.push_str(&String::from_utf8_lossy(&buffer[..n]));
            while let Some(end) = mirror_buffer.find('\n') {
                let line = mirror_buffer.drain(..=end).collect::<String>();
                eprint!("{}", redact_external(&line));
            }
        }
        let remaining = MAX_CAPTURE.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..n.min(remaining)]);
    }
    if mirror_stderr && !mirror_buffer.is_empty() {
        eprint!("{}", redact_external(&mirror_buffer));
    }
    Ok(String::from_utf8_lossy(&output).into_owned())
}

async fn terminate(child: &mut tokio::process::Child, job: &ExternalJob) {
    let terminated = job.terminate().is_ok();
    #[cfg(windows)]
    if !terminated {
        if let Some(pid) = child.id() {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[cfg(windows)]
struct ExternalJob(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl ExternalJob {
    fn new() -> Result<Self> {
        use std::mem::{size_of, zeroed};
        use std::ptr::null;
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        let handle = unsafe { CreateJobObjectW(null(), null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error()).context("CreateJobObjectW");
        }
        let job = Self(handle);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("SetInformationJobObject");
        }
        Ok(job)
    }

    fn assign(&self, child: &tokio::process::Child) -> Result<()> {
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
        let handle = child
            .raw_handle()
            .ok_or_else(|| anyhow!("launcher exited before Job Object assignment"))?;
        if unsafe { AssignProcessToJobObject(self.0, handle) } == 0 {
            return Err(std::io::Error::last_os_error()).context("AssignProcessToJobObject");
        }
        Ok(())
    }

    fn terminate(&self) -> Result<()> {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;
        if unsafe { TerminateJobObject(self.0, 0xA55E_0002) } == 0 {
            return Err(std::io::Error::last_os_error()).context("TerminateJobObject");
        }
        Ok(())
    }

    async fn wait_empty(&self, timeout: Duration) -> Result<bool> {
        use std::mem::{size_of, zeroed};
        use std::ptr::null_mut;
        use windows_sys::Win32::System::JobObjects::{
            JobObjectBasicAccountingInformation, QueryInformationJobObject,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        };
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
            if unsafe {
                QueryInformationJobObject(
                    self.0,
                    JobObjectBasicAccountingInformation,
                    (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    null_mut(),
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error()).context("QueryInformationJobObject");
            }
            if accounting.ActiveProcesses == 0 {
                return Ok(true);
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(false);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[cfg(windows)]
impl Drop for ExternalJob {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(not(windows))]
struct ExternalJob;
#[cfg(not(windows))]
impl ExternalJob {
    fn new() -> Result<Self> {
        Ok(Self)
    }
    fn assign(&self, _child: &tokio::process::Child) -> Result<()> {
        Ok(())
    }
    fn terminate(&self) -> Result<()> {
        Ok(())
    }
    async fn wait_empty(&self, _timeout: Duration) -> Result<bool> {
        Ok(true)
    }
}

fn require_success(output: Output, label: &str) -> Result<Output> {
    if output.status != 0 {
        bail!(
            "{label} failed: {}",
            bounded(&redact_external(&output.stderr))
        )
    }
    Ok(output)
}

fn is_not_found(output: &Output) -> bool {
    output.status != 0 && output.stderr.to_ascii_lowercase().contains("http 404")
}

fn is_repository_not_found(output: &Output, requested_slug: &str) -> bool {
    if output.status == 0 {
        return false;
    }
    let stderr = output.stderr.trim();
    if stderr.eq_ignore_ascii_case("HTTP 404") {
        return true;
    }
    let expected = format!(
        "GraphQL: Could not resolve to a Repository with the name '{requested_slug}'. (repository)"
    );
    stderr.eq_ignore_ascii_case(&expected)
}

fn bounded(value: &str) -> String {
    let value = value.trim();
    if value.chars().count() <= 4096 {
        return value.into();
    }
    format!(
        "{}...[truncated]",
        value.chars().take(4096).collect::<String>()
    )
}

fn redact_external(value: &str) -> String {
    value
        .split_inclusive(char::is_whitespace)
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if [
                "ghp_",
                "gho_",
                "ghu_",
                "ghs_",
                "ghr_",
                "github_pat_",
                "access_token",
                "refresh_token",
                "authorization:",
                "bearer ",
            ]
            .iter()
            .any(|marker| lower.contains(marker))
            {
                if part.ends_with(char::is_whitespace) {
                    "[REDACTED]\n"
                } else {
                    "[REDACTED]"
                }
            } else {
                part
            }
        })
        .collect()
}

fn clear_cancel(state: &Path) -> Result<()> {
    let _control = acquire_control_lock(state)?;
    let path = github_dir(state).join("cancel.flag");
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

async fn acquire_control_lock_wait(state: &Path) -> Result<Lock> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match acquire_control_lock(state) {
            Ok(lock) => return Ok(lock),
            Err(error)
                if error.to_string() == "local control is busy"
                    && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(error) => return Err(error),
        }
    }
}
fn cancelled(state: &Path) -> bool {
    github_dir(state).join("cancel.flag").exists()
}
fn stop_check(state: &Path) -> Result<()> {
    if cancelled(state) {
        bail!("operation cancelled")
    }
    ensure_unpaused(state)
}

fn validate_slug(value: &str) -> Result<String> {
    if value.trim() != value || value.len() > 201 {
        bail!("repository must be owner/name")
    }
    let parts: Vec<_> = value.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|part| !valid_name(part)) {
        bail!("repository must be owner/name using GitHub-safe characters")
    }
    Ok(value.into())
}
fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
fn validate_branch(value: &str) -> Result<()> {
    if value.is_empty()
        || value.trim() != value
        || value.starts_with('/')
        || value.ends_with('/')
        || value.ends_with('.')
        || value.contains("..")
        || value.contains("//")
        || value.contains("@{")
        || value.contains('\\')
        || value.split('/').any(|part| {
            part.is_empty() || part.starts_with('.') || part.to_ascii_lowercase().ends_with(".lock")
        })
        || value
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control() || b"~^:?*[".contains(&b))
    {
        bail!("invalid base branch")
    }
    Ok(())
}
fn validate_commit(value: &str) -> Result<()> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        bail!("invalid commit ID")
    }
    Ok(())
}
fn validate_repo_url(url: &str, slug: &str) -> Result<()> {
    if url != format!("https://github.com/{slug}") {
        bail!("GitHub returned an unsafe repository URL")
    }
    Ok(())
}
fn validate_pr_url(url: &str, slug: &str, number: u64) -> Result<()> {
    if url != format!("https://github.com/{slug}/pull/{number}") {
        bail!("GitHub returned an unsafe pull request URL")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_repository_names() {
        assert!(validate_slug("owner/repo").is_ok());
        assert!(validate_slug("owner/repo/extra").is_err());
        assert!(validate_slug("owner /repo").is_err());
    }
    #[test]
    fn rejects_unsafe_branches_and_urls() {
        assert!(validate_branch("main").is_ok());
        assert!(validate_branch("refs/../main").is_err());
        assert!(validate_repo_url("https://github.com/o/r", "o/r").is_ok());
        assert!(validate_repo_url("http://github.com/o/r", "o/r").is_err());
    }
    #[test]
    fn state_defaults_are_inert() {
        let state = GithubState::default();
        assert_eq!(state.revision, 0);
        assert!(state.connections.is_empty());
        assert!(state.publications.is_empty());
    }
    #[test]
    fn repository_absence_requires_the_exact_requested_slug() {
        let missing = Output {
            status: 1,
            stdout: String::new(),
            stderr: "GraphQL: Could not resolve to a Repository with the name 'owner/repo'. (repository)\n".into(),
        };
        assert!(is_repository_not_found(&missing, "owner/repo"));
        assert!(!is_repository_not_found(&missing, "owner/other"));
        let denied = Output {
            status: 1,
            stdout: String::new(),
            stderr: "GraphQL: Resource not accessible by personal access token (repository)".into(),
        };
        assert!(!is_repository_not_found(&denied, "owner/repo"));
    }
    #[test]
    fn structured_required_checks_fail_closed_and_allow_known_wait_states() {
        let no_checks = Output {
            status: 1,
            stdout: String::new(),
            stderr: "no required checks reported on the 'assemblywright/run' branch\n".into(),
        };
        assert_eq!(
            classify_required_checks(&no_checks, "assemblywright/run").unwrap(),
            RequiredCheckPoll::Waiting
        );
        assert!(classify_required_checks(&no_checks, "assemblywright/other").is_err());
        let pending = Output {
            status: 8,
            stdout: r#"[{"bucket":"pending","name":"gate","state":"IN_PROGRESS","workflow":"CI"}]"#
                .into(),
            stderr: String::new(),
        };
        assert_eq!(
            classify_required_checks(&pending, "assemblywright/run").unwrap(),
            RequiredCheckPoll::Waiting
        );
        let unknown = Output {
            status: 0,
            stdout: r#"[{"bucket":"unknown","name":"gate","state":"ALIEN","workflow":"CI"}]"#
                .into(),
            stderr: String::new(),
        };
        assert!(classify_required_checks(&unknown, "assemblywright/run").is_err());
    }
    #[test]
    fn required_check_wait_has_a_hard_deadline() {
        let expired = tokio::time::Instant::now() - Duration::from_millis(1);
        assert!(remaining_check_timeout(expired).is_err());
        let remaining =
            remaining_check_timeout(tokio::time::Instant::now() + Duration::from_secs(1)).unwrap();
        assert!(remaining <= Duration::from_secs(1));
    }
}
