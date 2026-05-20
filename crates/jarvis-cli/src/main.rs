use std::io::{Read, Write};
use std::net::TcpStream;
use std::net::ToSocketAddrs;

use clap::{Parser, Subcommand};
use jarvis_core::{CommandRequest, CreateSchedulerJobRequest, EmergencyPauseRequest, TriggerKind};

#[derive(Debug, Parser)]
#[command(name = "jarvis")]
#[command(about = "Local-first Jarvis core CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the local HTTP IPC server.
    Serve {
        #[arg(long, default_value = "127.0.0.1:7787")]
        bind: String,
    },
    /// Query core health over HTTP IPC.
    Health {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
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
        Command::Serve { bind } => {
            jarvis_core::serve(bind.parse()?, jarvis_core::IpcState::new()).await?;
        }
        Command::Health { endpoint } => {
            println!("{}", request(&endpoint, "GET", "/health", None)?);
        }
        Command::Command {
            input,
            endpoint,
            dry_run,
        } => {
            let body = serde_json::to_string(&CommandRequest {
                input,
                session_id: None,
                context: serde_json::Value::Null,
                dry_run,
            })?;
            println!("{}", request(&endpoint, "POST", "/commands", Some(&body))?);
        }
        Command::Pause { reason, endpoint } => {
            let body = serde_json::to_string(&EmergencyPauseRequest { reason })?;
            println!(
                "{}",
                request(&endpoint, "POST", "/emergency-pause", Some(&body))?
            );
        }
        Command::Resume { endpoint } => {
            println!(
                "{}",
                request(&endpoint, "DELETE", "/emergency-pause", None)?
            );
        }
        Command::PauseStatus { endpoint } => {
            println!("{}", request(&endpoint, "GET", "/emergency-pause", None)?);
        }
        Command::Scheduler { command } => match command {
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
