use std::io::{Read, Write};
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use jarvis_core::{CommandRequest, CreateSchedulerJobRequest, EmergencyPauseRequest, TriggerKind};
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
    },
    /// Query core health over HTTP IPC.
    Health {
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
}

#[derive(Debug, Subcommand)]
enum SchedulerCommand {
    /// List local scheduler jobs.
    List {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Create an inspectable manual scheduler job.
    Schedule {
        name: String,
        command: String,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    match Cli::parse().command {
        CliCommand::Serve { bind, db_path } => {
            let state = match db_path {
                Some(path) => {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    jarvis_core::IpcState::with_repository(jarvis_core::SqliteRepository::open(
                        path,
                    )?)?
                }
                None => jarvis_core::IpcState::new(),
            };
            jarvis_core::serve(bind.parse()?, state).await?;
        }
        CliCommand::Health { endpoint } => {
            println!(
                "{}",
                format_health(&request(&endpoint, "GET", "/health", None)?)?
            );
        }
        CliCommand::Smoke => {
            run_smoke().await?;
        }
        CliCommand::Command {
            input,
            endpoint,
            dry_run,
        } => {
            let body = serde_json::to_string(&CommandRequest {
                input,
                session_id: None,
                context: serde_json::Value::Null,
                dry_run,
                sensitivity: None,
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
            SchedulerCommand::Schedule {
                name,
                command,
                endpoint,
            } => {
                let body = serde_json::to_string(&CreateSchedulerJobRequest {
                    name,
                    command,
                    trigger: TriggerKind::Manual,
                })?;
                println!(
                    "{}",
                    request(&endpoint, "POST", "/scheduler/jobs", Some(&body))?
                );
            }
            SchedulerCommand::Cancel { id, endpoint } => {
                println!(
                    "{}",
                    request(&endpoint, "DELETE", &format!("/scheduler/jobs/{id}"), None)?
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

async fn run_smoke() -> anyhow::Result<()> {
    let state = jarvis_core::IpcState::new();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = tokio::spawn(jarvis_core::serve_listener(listener, state));

    let health = request_with_retry(&endpoint, "GET", "/health", None)?;
    let health_json: serde_json::Value = serde_json::from_str(&health)?;
    require_json_field(&health_json, "status", "ok")?;
    require_json_field(
        &health_json,
        "command_runtime",
        "routed-fake-local-model+first-party-plugins",
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

    Ok(format!(
        "jarvis-core: {status} (runtime: {runtime}, paused: {paused}, scheduler_jobs: {scheduler_jobs})"
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
    value
        .get(field)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("missing array field `{field}`"))?;
    Ok(())
}
