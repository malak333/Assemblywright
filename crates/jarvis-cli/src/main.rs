use std::io::{Read, Write};
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use jarvis_core::{
    ApprovalDecisionRequest, CommandRequest, CreateMemoryItemRequest, CreateSchedulerJobRequest,
    EmergencyPauseRequest, InstallPluginRequest, Sensitivity, TriggerKind, UpdateMemoryItemRequest,
};
use tokio::net::TcpListener;

#[derive(Debug, Parser)]
#[command(name = "jarvis")]
#[command(about = "Local-first Jarvis core CLI")]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Start the local HTTP IPC server.
    Serve {
        #[arg(long, default_value = "127.0.0.1:7787")]
        bind: String,
        #[arg(long, env = "JARVIS_DB_PATH")]
        db_path: Option<PathBuf>,
        #[arg(long)]
        scheduler_background: bool,
        #[arg(long, default_value_t = jarvis_core::DEFAULT_SCHEDULER_BACKGROUND_INTERVAL_MS)]
        scheduler_interval_ms: u64,
        #[arg(long, default_value_t = jarvis_core::DEFAULT_SCHEDULER_BACKGROUND_LIMIT)]
        scheduler_limit: usize,
    },
    /// Query core health over HTTP IPC.
    Health {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Print supported IPC contract metadata and endpoint inventory as JSON.
    Contract {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Run a local IPC smoke test against an ephemeral core server.
    Smoke,
    /// Submit a command to the core command endpoint.
    Command {
        input: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        sensitivity: Option<String>,
    },
    /// Activate emergency pause.
    Pause {
        #[arg(long, default_value = "user requested")]
        reason: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Resume after emergency pause.
    Resume {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Inspect emergency pause state.
    PauseStatus {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Inspect and mutate scheduler jobs.
    Scheduler {
        #[command(subcommand)]
        command: SchedulerCommand,
    },
    /// Export redacted local diagnostics.
    Diagnostics {
        #[command(subcommand)]
        command: DiagnosticsCommand,
    },
    /// Inspect persisted tasks and audit entries.
    Tasks {
        #[command(subcommand)]
        command: TasksCommand,
    },
    /// Inspect or update persisted memory items.
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
    /// Inspect registered first-party plugin manifests.
    Plugins {
        #[command(subcommand)]
        command: PluginsCommand,
    },
    /// Inspect and decide approval-required actions.
    Approvals {
        #[command(subcommand)]
        command: ApprovalsCommand,
    },
    /// Inspect persisted permission grants and approval history.
    Permissions {
        #[command(subcommand)]
        command: PermissionsCommand,
    },
}

#[derive(Debug, Subcommand)]
enum SchedulerCommand {
    /// List local scheduler jobs.
    List {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Fetch one scheduler job by id.
    Get {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Create an inspectable manual scheduler job.
    Schedule {
        name: String,
        command: String,
        #[arg(long, conflicts_with = "interval_seconds")]
        once_at: Option<String>,
        #[arg(long, conflicts_with = "once_at")]
        interval_seconds: Option<u64>,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Execute currently due scheduler jobs once.
    RunDue {
        #[arg(long, default_value_t = 16)]
        limit: usize,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Cancel a scheduler job by id.
    Cancel {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
}

#[derive(Debug, Subcommand)]
enum DiagnosticsCommand {
    /// Export redacted health, scheduler, and persistence counters.
    Export {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
}

#[derive(Debug, Subcommand)]
enum TasksCommand {
    /// List persisted tasks.
    List {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Fetch one persisted task by id.
    Get {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// List audit entries, optionally scoped to one task id.
    Audit {
        #[arg(long)]
        task_id: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
}

#[derive(Debug, Subcommand)]
enum MemoryCommand {
    /// List persisted memory items.
    List {
        #[arg(long)]
        include_deleted: bool,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Fetch one persisted memory item by id.
    Get {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Create a persisted memory item.
    Create {
        category: String,
        key: String,
        value: String,
        #[arg(long)]
        provenance: String,
        #[arg(long, default_value = "personal")]
        sensitivity: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Update a persisted memory item.
    Update {
        id: String,
        value: String,
        #[arg(long)]
        provenance: String,
        #[arg(long, default_value = "personal")]
        sensitivity: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Mark a memory item reviewed.
    Review {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Soft-delete a memory item.
    Delete {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
}

#[derive(Debug, Subcommand)]
enum PluginsCommand {
    /// List registered plugin manifests.
    List {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Fetch one registered plugin manifest by id.
    Get {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Validate and store local plugin manifest metadata without enabling execution.
    Install {
        manifest_path: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// List locally installed plugin metadata.
    Installed {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Fetch one locally installed plugin metadata record by id.
    InstalledGet {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Request an installed plugin run through the fail-closed runner boundary.
    RunInstalled {
        id: String,
        action: String,
        #[arg(long, default_value = "null")]
        input: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
}

#[derive(Debug, Subcommand)]
enum ApprovalsCommand {
    /// List approval decisions, optionally filtered by status.
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Fetch one approval decision by id.
    Get {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Grant an approval decision without executing the side effect.
    Approve {
        id: String,
        #[arg(long, default_value = "cli")]
        decided_by: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Deny an approval decision.
    Deny {
        id: String,
        #[arg(long, default_value = "cli")]
        decided_by: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
}

#[derive(Debug, Subcommand)]
enum PermissionsCommand {
    /// Show approval history and installed-plugin execution grant state.
    Grants {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    match Cli::parse().command {
        CliCommand::Serve {
            bind,
            db_path,
            scheduler_background,
            scheduler_interval_ms,
            scheduler_limit,
        } => {
            let provider_config = jarvis_core::ProviderConfig::from_env()?;
            let state = match db_path {
                Some(path) => {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    jarvis_core::IpcState::with_repository_and_provider_config(
                        jarvis_core::SqliteRepository::open(path)?,
                        provider_config,
                    )?
                }
                None => jarvis_core::IpcState::with_provider_config(provider_config),
            };
            let _scheduler_loop = if scheduler_background {
                let config = jarvis_core::SchedulerBackgroundConfig::new(
                    std::time::Duration::from_millis(scheduler_interval_ms),
                    scheduler_limit,
                )?;
                Some(state.spawn_scheduler_background_loop(config))
            } else {
                None
            };
            jarvis_core::serve(bind.parse()?, state).await?;
        }
        CliCommand::Health { endpoint } => {
            println!(
                "{}",
                format_health(&request(&endpoint, "GET", "/health", None)?)?
            );
        }
        CliCommand::Contract { endpoint } => {
            println!("{}", request(&endpoint, "GET", "/contract", None)?);
        }
        CliCommand::Smoke => {
            run_smoke().await?;
        }
        CliCommand::Command {
            input,
            endpoint,
            dry_run,
            sensitivity,
        } => {
            let body = serde_json::to_string(&CommandRequest {
                input,
                session_id: None,
                context: serde_json::Value::Null,
                dry_run,
                sensitivity: sensitivity.as_deref().map(parse_sensitivity).transpose()?,
            })?;
            println!("{}", request(&endpoint, "POST", "/commands", Some(&body))?);
        }
        CliCommand::Pause { reason, endpoint } => {
            let body = serde_json::to_string(&EmergencyPauseRequest { reason })?;
            println!(
                "{}",
                request(&endpoint, "POST", "/emergency-pause", Some(&body))?
            );
        }
        CliCommand::Resume { endpoint } => {
            println!(
                "{}",
                request(&endpoint, "DELETE", "/emergency-pause", None)?
            );
        }
        CliCommand::PauseStatus { endpoint } => {
            println!("{}", request(&endpoint, "GET", "/emergency-pause", None)?);
        }
        CliCommand::Scheduler { command } => match command {
            SchedulerCommand::List { endpoint } => {
                println!("{}", request(&endpoint, "GET", "/scheduler/jobs", None)?);
            }
            SchedulerCommand::Get { id, endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "GET", &format!("/scheduler/jobs/{id}"), None)?
                );
            }
            SchedulerCommand::Schedule {
                name,
                command,
                once_at,
                interval_seconds,
                endpoint,
            } => {
                let body = serde_json::to_string(&CreateSchedulerJobRequest {
                    name,
                    command,
                    trigger: parse_scheduler_trigger(once_at, interval_seconds)?,
                })?;
                println!(
                    "{}",
                    request(&endpoint, "POST", "/scheduler/jobs", Some(&body))?
                );
            }
            SchedulerCommand::RunDue { limit, endpoint } => {
                println!(
                    "{}",
                    request(
                        &endpoint,
                        "POST",
                        &format!("/scheduler/run-due?limit={limit}"),
                        None,
                    )?
                );
            }
            SchedulerCommand::Cancel { id, endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "DELETE", &format!("/scheduler/jobs/{id}"), None)?
                );
            }
        },
        CliCommand::Diagnostics { command } => match command {
            DiagnosticsCommand::Export { endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "GET", "/diagnostics/export", None)?
                );
            }
        },
        CliCommand::Tasks { command } => match command {
            TasksCommand::List { endpoint } => {
                println!("{}", request(&endpoint, "GET", "/tasks", None)?);
            }
            TasksCommand::Get { id, endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "GET", &format!("/tasks/{id}"), None)?
                );
            }
            TasksCommand::Audit { task_id, endpoint } => {
                let path = task_id
                    .map(|id| format!("/tasks/{id}/audit"))
                    .unwrap_or_else(|| "/audit".to_string());
                println!("{}", request(&endpoint, "GET", &path, None)?);
            }
        },
        CliCommand::Memory { command } => match command {
            MemoryCommand::List {
                include_deleted,
                endpoint,
            } => {
                let path = if include_deleted {
                    "/memory?include_deleted=true"
                } else {
                    "/memory"
                };
                println!("{}", request(&endpoint, "GET", path, None)?);
            }
            MemoryCommand::Get { id, endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "GET", &format!("/memory/{id}"), None)?
                );
            }
            MemoryCommand::Create {
                category,
                key,
                value,
                provenance,
                sensitivity,
                endpoint,
            } => {
                let body = serde_json::to_string(&CreateMemoryItemRequest {
                    category,
                    key,
                    value,
                    provenance,
                    sensitivity: parse_sensitivity(&sensitivity)?,
                })?;
                println!("{}", request(&endpoint, "POST", "/memory", Some(&body))?);
            }
            MemoryCommand::Update {
                id,
                value,
                provenance,
                sensitivity,
                endpoint,
            } => {
                let body = serde_json::to_string(&UpdateMemoryItemRequest {
                    value,
                    provenance,
                    sensitivity: parse_sensitivity(&sensitivity)?,
                })?;
                println!(
                    "{}",
                    request(&endpoint, "PATCH", &format!("/memory/{id}"), Some(&body))?
                );
            }
            MemoryCommand::Review { id, endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "POST", &format!("/memory/{id}/review"), None)?
                );
            }
            MemoryCommand::Delete { id, endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "DELETE", &format!("/memory/{id}"), None)?
                );
            }
        },
        CliCommand::Plugins { command } => match command {
            PluginsCommand::List { endpoint } => {
                println!("{}", request(&endpoint, "GET", "/plugins/manifests", None)?);
            }
            PluginsCommand::Get { id, endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "GET", &format!("/plugins/manifests/{id}"), None)?
                );
            }
            PluginsCommand::Install {
                manifest_path,
                endpoint,
            } => {
                let manifest_path = std::fs::canonicalize(manifest_path)?;
                let body = serde_json::to_string(&InstallPluginRequest {
                    manifest_path: manifest_path.display().to_string(),
                })?;
                println!(
                    "{}",
                    request(&endpoint, "POST", "/plugins/installed", Some(&body))?
                );
            }
            PluginsCommand::Installed { endpoint } => {
                println!("{}", request(&endpoint, "GET", "/plugins/installed", None)?);
            }
            PluginsCommand::InstalledGet { id, endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "GET", &format!("/plugins/installed/{id}"), None)?
                );
            }
            PluginsCommand::RunInstalled {
                id,
                action,
                input,
                dry_run,
                endpoint,
            } => {
                let input: serde_json::Value = serde_json::from_str(&input)?;
                let body = serde_json::to_string(&serde_json::json!({
                    "action": action,
                    "input": input,
                    "session_id": null,
                    "dry_run": dry_run,
                }))?;
                println!(
                    "{}",
                    request(
                        &endpoint,
                        "POST",
                        &format!("/plugins/installed/{id}/run"),
                        Some(&body)
                    )?
                );
            }
        },
        CliCommand::Approvals { command } => match command {
            ApprovalsCommand::List { status, endpoint } => {
                let path = status
                    .map(|status| format!("/approvals?status={status}"))
                    .unwrap_or_else(|| "/approvals".to_string());
                println!("{}", request(&endpoint, "GET", &path, None)?);
            }
            ApprovalsCommand::Get { id, endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "GET", &format!("/approvals/{id}"), None)?
                );
            }
            ApprovalsCommand::Approve {
                id,
                decided_by,
                reason,
                endpoint,
            } => {
                let body = serde_json::to_string(&ApprovalDecisionRequest { decided_by, reason })?;
                println!(
                    "{}",
                    request(
                        &endpoint,
                        "POST",
                        &format!("/approvals/{id}/approve"),
                        Some(&body)
                    )?
                );
            }
            ApprovalsCommand::Deny {
                id,
                decided_by,
                reason,
                endpoint,
            } => {
                let body = serde_json::to_string(&ApprovalDecisionRequest { decided_by, reason })?;
                println!(
                    "{}",
                    request(
                        &endpoint,
                        "POST",
                        &format!("/approvals/{id}/deny"),
                        Some(&body)
                    )?
                );
            }
        },
        CliCommand::Permissions { command } => match command {
            PermissionsCommand::Grants { endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "GET", "/permissions/grants", None)?
                );
            }
        },
    }

    Ok(())
}

fn request(endpoint: &str, method: &str, path: &str, body: Option<&str>) -> anyhow::Result<String> {
    let target = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| anyhow::anyhow!("only http:// endpoints are supported"))?;
    let host_port = target.trim_end_matches('/');
    let address = host_port
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("could not resolve endpoint: {endpoint}"))?;
    let mut stream = TcpStream::connect(address)?;
    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    stream.write_all(request.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    let Some((headers, response_body)) = response.split_once("\r\n\r\n") else {
        return Ok(response);
    };

    if !headers.starts_with("HTTP/1.1 2") {
        return Err(anyhow::anyhow!("{headers}\n\n{response_body}"));
    }

    Ok(response_body.to_string())
}

fn parse_sensitivity(value: &str) -> anyhow::Result<Sensitivity> {
    match value {
        "public" => Ok(Sensitivity::Public),
        "workspace" => Ok(Sensitivity::Workspace),
        "personal" => Ok(Sensitivity::Personal),
        "private" => Ok(Sensitivity::Private),
        "credential_adjacent" => Ok(Sensitivity::CredentialAdjacent),
        "restricted" => Ok(Sensitivity::Restricted),
        _ => Err(anyhow::anyhow!(
            "sensitivity must be one of public, workspace, personal, private, credential_adjacent, restricted"
        )),
    }
}

fn parse_scheduler_trigger(
    once_at: Option<String>,
    interval_seconds: Option<u64>,
) -> anyhow::Result<TriggerKind> {
    match (once_at, interval_seconds) {
        (Some(value), None) => {
            let run_at = DateTime::parse_from_rfc3339(&value)
                .map_err(|error| anyhow::anyhow!("--once-at must be RFC3339: {error}"))?
                .with_timezone(&Utc);
            Ok(TriggerKind::OnceAt { run_at })
        }
        (None, Some(0)) => Err(anyhow::anyhow!(
            "--interval-seconds must be greater than zero"
        )),
        (None, Some(every_seconds)) => Ok(TriggerKind::Interval { every_seconds }),
        (None, None) => Ok(TriggerKind::Manual),
        (Some(_), Some(_)) => unreachable!("clap conflicts prevent both trigger flags"),
    }
}

async fn run_smoke() -> anyhow::Result<()> {
    let state = jarvis_core::IpcState::new();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = tokio::spawn(jarvis_core::serve_listener(listener, state));

    let health = request_with_retry(&endpoint, "GET", "/health", None)?;
    let health_json: serde_json::Value = serde_json::from_str(&health)?;
    require_json_field(&health_json, "status", "ok")?;
    require_nested_field(&health_json, &["contract", "name"], "jarvis.local-ipc")?;
    require_json_field(
        &health_json,
        "command_runtime",
        "routed-fake-local-model+first-party-plugins",
    )?;

    let contract = request(&endpoint, "GET", "/contract", None)?;
    let contract_json: serde_json::Value = serde_json::from_str(&contract)?;
    require_nested_field(&contract_json, &["contract", "name"], "jarvis.local-ipc")?;
    require_array_contains_object_field(
        &contract_json["endpoints"],
        "path",
        "/diagnostics/export",
    )?;

    let command_body = serde_json::to_string(&CommandRequest {
        input: "smoke command".to_string(),
        session_id: None,
        context: serde_json::json!({ "surface": "cli-smoke" }),
        dry_run: true,
        sensitivity: None,
    })?;
    let command = request(&endpoint, "POST", "/commands", Some(&command_body))?;
    let command_json: serde_json::Value = serde_json::from_str(&command)?;
    require_bool_field(&command_json, "accepted", true)?;
    require_nested_field(&command_json, &["task", "status"], "completed")?;
    require_nested_field(&command_json, &["route", "model"], "fake-local-model")?;
    require_array_field(&command_json, "audit_entries")?;

    let manifests = request(&endpoint, "GET", "/plugins/manifests", None)?;
    let manifests_json: serde_json::Value = serde_json::from_str(&manifests)?;
    require_array_contains_object_field(&manifests_json, "id", "fake_echo")?;

    let diagnostics = request(&endpoint, "GET", "/diagnostics/export", None)?;
    let diagnostics_json: serde_json::Value = serde_json::from_str(&diagnostics)?;
    require_json_field(
        &diagnostics_json,
        "redaction",
        "diagnostics export omits command bodies, scheduler commands, audit payloads, memory values, and cancellation reason text",
    )?;
    require_nested_field(&diagnostics_json, &["health", "status"], "ok")?;

    let pause_body = serde_json::to_string(&EmergencyPauseRequest {
        reason: "cli smoke".to_string(),
    })?;
    let pause = request(&endpoint, "POST", "/emergency-pause", Some(&pause_body))?;
    let pause_json: serde_json::Value = serde_json::from_str(&pause)?;
    require_bool_field(&pause_json, "paused", true)?;

    let blocked = request(&endpoint, "POST", "/commands", Some(&command_body))?;
    let blocked_json: serde_json::Value = serde_json::from_str(&blocked)?;
    require_bool_field(&blocked_json, "accepted", false)?;
    require_nested_field(&blocked_json, &["task", "status"], "blocked")?;

    let resume = request(&endpoint, "DELETE", "/emergency-pause", None)?;
    let resume_json: serde_json::Value = serde_json::from_str(&resume)?;
    require_bool_field(&resume_json, "paused", false)?;

    server.abort();

    let db_path = std::env::temp_dir().join(format!(
        "jarvis-smoke-{}.sqlite",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let state =
        jarvis_core::IpcState::with_repository(jarvis_core::SqliteRepository::open(&db_path)?)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let persistent_endpoint = format!("http://{}", listener.local_addr()?);
    let persistent_server = tokio::spawn(jarvis_core::serve_listener(listener, state));

    request_with_retry(&persistent_endpoint, "GET", "/health", None)?;
    let persisted_command = request(
        &persistent_endpoint,
        "POST",
        "/commands",
        Some(&command_body),
    )?;
    let persisted_command_json: serde_json::Value = serde_json::from_str(&persisted_command)?;
    require_nested_field(&persisted_command_json, &["task", "status"], "completed")?;

    let tasks = request(&persistent_endpoint, "GET", "/tasks", None)?;
    let tasks_json: serde_json::Value = serde_json::from_str(&tasks)?;
    require_array_field(&tasks_json, "root")?;

    let memory_body = serde_json::to_string(&CreateMemoryItemRequest {
        category: "smoke".to_string(),
        key: "release-gate".to_string(),
        value: "local smoke covers persisted state".to_string(),
        provenance: "jarvis-cli smoke".to_string(),
        sensitivity: Sensitivity::Workspace,
    })?;
    let memory = request(&persistent_endpoint, "POST", "/memory", Some(&memory_body))?;
    let memory_json: serde_json::Value = serde_json::from_str(&memory)?;
    require_json_field(&memory_json, "key", "release-gate")?;

    let memory_list = request(&persistent_endpoint, "GET", "/memory", None)?;
    let memory_list_json: serde_json::Value = serde_json::from_str(&memory_list)?;
    require_array_contains_object_field(&memory_list_json, "key", "release-gate")?;

    let persistent_diagnostics = request(&persistent_endpoint, "GET", "/diagnostics/export", None)?;
    let persistent_diagnostics_json: serde_json::Value =
        serde_json::from_str(&persistent_diagnostics)?;
    require_bool_field(&persistent_diagnostics_json, "repository_backed", true)?;
    require_number_at_least(&persistent_diagnostics_json, "task_count", 1)?;

    persistent_server.abort();
    let _ = std::fs::remove_file(db_path);

    println!("jarvis smoke: ok");
    Ok(())
}

fn request_with_retry(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> anyhow::Result<String> {
    let mut last_error = None;
    for _ in 0..20 {
        match request(endpoint, method, path, body) {
            Ok(response) => return Ok(response),
            Err(error) => {
                last_error = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("request did not run")))
}

fn format_health(response_body: &str) -> anyhow::Result<String> {
    let health: serde_json::Value = serde_json::from_str(response_body)?;
    let status = health
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let runtime = health
        .get("command_runtime")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let paused = health
        .get("emergency_paused")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let scheduler_jobs = health
        .get("scheduler_jobs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let contract_version = health
        .get("contract")
        .and_then(|contract| contract.get("version"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    Ok(format!(
        "jarvis-core: {status} (runtime: {runtime}, paused: {paused}, scheduler_jobs: {scheduler_jobs}, contract: v{contract_version})"
    ))
}

fn require_json_field(
    value: &serde_json::Value,
    field: &str,
    expected: &str,
) -> anyhow::Result<()> {
    let actual = value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing string field `{field}`"))?;
    anyhow::ensure!(
        actual == expected,
        "expected `{field}` to be `{expected}`, got `{actual}`"
    );
    Ok(())
}

fn require_nested_field(
    value: &serde_json::Value,
    path: &[&str],
    expected: &str,
) -> anyhow::Result<()> {
    let mut cursor = value;
    for segment in path {
        cursor = cursor
            .get(segment)
            .ok_or_else(|| anyhow::anyhow!("missing field `{}`", path.join(".")))?;
    }
    let actual = cursor
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("field `{}` is not a string", path.join(".")))?;
    anyhow::ensure!(
        actual == expected,
        "expected `{}` to be `{expected}`, got `{actual}`",
        path.join(".")
    );
    Ok(())
}

fn require_bool_field(
    value: &serde_json::Value,
    field: &str,
    expected: bool,
) -> anyhow::Result<()> {
    let actual = value
        .get(field)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| anyhow::anyhow!("missing boolean field `{field}`"))?;
    anyhow::ensure!(
        actual == expected,
        "expected `{field}` to be `{expected}`, got `{actual}`"
    );
    Ok(())
}

fn require_array_field(value: &serde_json::Value, field: &str) -> anyhow::Result<()> {
    if field == "root" {
        value
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("response root is not an array"))?;
    } else {
        value
            .get(field)
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("missing array field `{field}`"))?;
    }
    Ok(())
}

fn require_array_contains_object_field(
    value: &serde_json::Value,
    field: &str,
    expected: &str,
) -> anyhow::Result<()> {
    let array = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("response root is not an array"))?;
    anyhow::ensure!(
        array
            .iter()
            .any(|item| item.get(field).and_then(serde_json::Value::as_str) == Some(expected)),
        "expected array to contain object with `{field}` = `{expected}`"
    );
    Ok(())
}

fn require_number_at_least(
    value: &serde_json::Value,
    field: &str,
    minimum: u64,
) -> anyhow::Result<()> {
    let actual = value
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("missing numeric field `{field}`"))?;
    anyhow::ensure!(
        actual >= minimum,
        "expected `{field}` to be at least `{minimum}`, got `{actual}`"
    );
    Ok(())
}
