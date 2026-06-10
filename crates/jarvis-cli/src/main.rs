use std::io::{Read, Write};
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use jarvis_core::{
    ApprovalDecisionRequest, CommandRequest, CreateMemoryItemRequest, CreateSchedulerJobRequest,
    EmergencyPauseRequest, InstallPluginRequest, InstalledPluginExecutionGrant,
    InstalledPluginExecutionRequest, InstalledPluginPublisherSignatureVerificationRequest,
    InstalledPluginPublisherVerificationRequest, Sensitivity, TriggerKind, UpdateMemoryItemRequest,
};
use tokio::net::TcpListener;

#[derive(Debug, Parser)]
#[command(name = "jarvis")]
#[command(about = "Local-first Jarvis core CLI")]
#[command(version = env!("CARGO_PKG_VERSION"))]
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
        #[arg(long)]
        scheduler_recover_stale_on_startup: bool,
        #[arg(long, default_value_t = jarvis_core::DEFAULT_SCHEDULER_STALE_RECOVERY_OLDER_THAN_SECONDS)]
        scheduler_stale_older_than_seconds: u64,
        #[arg(long, default_value_t = jarvis_core::DEFAULT_SCHEDULER_STALE_RECOVERY_LIMIT)]
        scheduler_stale_recovery_limit: usize,
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
    /// Summarize release-readiness evidence and remaining production blockers.
    #[command(
        long_about = "Read-only release operator commands.\n\nThese commands prefer the configured IPC endpoint. When the core is not running and transport is unavailable, they fall back to conservative local metadata or local file/report inspection without executing release side effects."
    )]
    Release {
        #[command(subcommand)]
        command: ReleaseCommand,
    },
    /// Run a local IPC smoke test against an ephemeral core server.
    Smoke,
    /// Submit a command to the core command endpoint.
    #[command(visible_alias = "ask")]
    Command {
        input: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        sensitivity: Option<String>,
        /// Print the raw JSON command response.
        #[arg(long)]
        json: bool,
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
    /// Inspect current task and audit activity summary.
    Activity {
        #[command(subcommand)]
        command: ActivityCommand,
    },
    /// Inspect persisted model route evidence.
    Routes {
        #[command(subcommand)]
        command: RoutesCommand,
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
    /// Inspect model-visible registered first-party tools.
    Tools {
        #[command(subcommand)]
        command: ToolsCommand,
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
enum ReleaseCommand {
    /// Print conservative release-readiness evidence.
    #[command(
        long_about = "Print conservative release-readiness evidence.\n\nThis is a read-only operator summary of implemented repo-owned proof, pending features, recommended verification commands, and manual production blockers. By default it remains conservative even if local evidence files exist; set JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external only after owner-recorded external evidence has been collected. The production_ready field stays false until signed distribution, notarization/stapling, plugin-trust QA, and final evidence bundle checks validate."
    )]
    Readiness {
        /// HTTP IPC endpoint. Falls back to local read-only readiness metadata when unavailable.
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print the raw JSON readiness payload.
        #[arg(long)]
        json: bool,
        /// Print every recommended verification command in readable output.
        #[arg(long)]
        all_commands: bool,
    },
    /// Print release evidence file/report status.
    #[command(
        long_about = "Print release evidence file/report status.\n\nThis is file/report inventory plus semantic report validation only. It can report whether expected artifact paths and JSON reports are present, missing, or invalid, and checks app bundle metadata, bundled-core version markers, signed-provenance digests, live-device command evidence, owner-asserted plugin-trust review source, host-egress evidence fields, child-report validity, and final-bundle local signature-validation status. It does not prove Developer ID signing, notarization, stapling, installation, Finder launch, live-device QA, marketplace review, malware scanning, OS sandboxing, or host-level egress enforcement. Default output is operator-readable; use --json for the exact structured payload."
    )]
    EvidenceStatus {
        /// HTTP IPC endpoint. Falls back to local read-only evidence inspection when unavailable.
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print the raw JSON evidence-status payload.
        #[arg(long)]
        json: bool,
    },
    /// Print the live-device QA runbook and current evidence status.
    #[command(
        long_about = "Print the live-device QA runbook and current evidence status.\n\nThis is a read-only operator guide for clearing the live_voice_loop manual validation gate. It combines conservative release readiness with local evidence-status inspection and does not perform live microphone, Speech permission, transcript handoff, audio-output, notification, Finder launch, signing, notarization, or installation validation."
    )]
    LiveDeviceRunbook {
        /// HTTP IPC endpoint. Falls back to local read-only readiness and evidence inspection when unavailable.
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print a structured JSON runbook summary.
        #[arg(long)]
        json: bool,
    },
    /// Print the signed distribution runbook and current evidence status.
    #[command(
        long_about = "Print the signed distribution runbook and current evidence status.\n\nThis is a read-only operator guide for clearing the Developer ID signing, notarization, stapling, and signed-provenance evidence gates. It combines conservative release readiness with local evidence-status inspection and does not perform signing, notarization, stapling, Gatekeeper assessment, installation, Finder launch, live-device QA, or plugin-trust QA."
    )]
    SignedDistributionRunbook {
        /// HTTP IPC endpoint. Falls back to local read-only readiness and evidence inspection when unavailable.
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print a structured JSON runbook summary.
        #[arg(long)]
        json: bool,
    },
    /// Print the plugin-trust QA runbook and current evidence status.
    #[command(
        long_about = "Print the plugin-trust QA runbook and current evidence status.\n\nThis is a read-only operator guide for clearing plugin marketplace review, malware scanning, signed publisher policy, OS sandbox, host-level egress enforcement, and manual trust-review evidence gates. It combines conservative release readiness with local evidence-status inspection and does not perform marketplace review, malware scanning, sandbox deployment, network egress enforcement, signing, notarization, live-device QA, or final evidence bundling."
    )]
    PluginTrustRunbook {
        /// HTTP IPC endpoint. Falls back to local read-only readiness and evidence inspection when unavailable.
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print a structured JSON runbook summary.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SchedulerCommand {
    /// List local scheduler jobs.
    List {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Inspect redacted scheduler attention signals for the app notification surface.
    Attention {
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
    /// Mark stale running scheduler jobs failed after explicit operator review.
    RecoverStale {
        #[arg(long, default_value_t = 3600)]
        older_than_seconds: u64,
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
        /// Print the raw JSON task list.
        #[arg(long)]
        json: bool,
    },
    /// Fetch one persisted task by id.
    Get {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print the raw JSON task payload.
        #[arg(long)]
        json: bool,
    },
    /// List audit entries, optionally scoped to one task id.
    Audit {
        #[arg(long)]
        task_id: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print the raw JSON audit entries.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ActivityCommand {
    /// Summarize current task statuses and recent audit progress.
    Summary {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print the raw JSON activity summary.
        #[arg(long)]
        json: bool,
    },
    /// Stream bounded activity summary events.
    Watch {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        #[arg(long, default_value_t = jarvis_core::DEFAULT_ACTIVITY_EVENT_LIMIT)]
        max_events: usize,
        #[arg(long, default_value_t = jarvis_core::DEFAULT_ACTIVITY_EVENT_INTERVAL_MS)]
        interval_ms: u64,
    },
}

#[derive(Debug, Subcommand)]
enum RoutesCommand {
    /// List persisted model route records, optionally scoped to one task id.
    List {
        #[arg(long)]
        task_id: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print the raw JSON route list.
        #[arg(long)]
        json: bool,
    },
    /// Fetch one persisted model route record by id.
    Get {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print the raw JSON route payload.
        #[arg(long)]
        json: bool,
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
    /// Summarize memory items by sensitivity and category.
    Classification {
        #[arg(long)]
        include_deleted: bool,
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
    /// Restore a soft-deleted memory item.
    Restore {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
}

#[derive(Debug, Subcommand)]
enum PluginsCommand {
    /// List registered plugin manifests.
    #[command(visible_alias = "available")]
    List {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print the raw JSON plugin manifest list.
        #[arg(long)]
        json: bool,
    },
    /// Fetch one registered plugin manifest by id.
    Get {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print the raw JSON plugin manifest.
        #[arg(long)]
        json: bool,
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
    /// Enable an installed local subprocess plugin with an explicit execution grant.
    /// Network-declaring actions require --grant subprocess_stdio_network.
    EnableInstalled {
        id: String,
        #[arg(long, default_value = "subprocess_stdio")]
        grant: InstalledPluginExecutionGrant,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Disable an installed plugin and reset it to metadata-only.
    DisableInstalled {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Verify local installed plugin files against the install-time provenance snapshot.
    VerifyInstalled {
        id: String,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Mark an installed plugin publisher origin claim as operator-verified.
    VerifyPublisher {
        id: String,
        #[arg(long)]
        trusted_origin: String,
        #[arg(long, default_value = "cli")]
        decided_by: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
    },
    /// Verify an installed plugin publisher signature with an explicit trusted public key.
    VerifyPublisherSignature {
        id: String,
        #[arg(long)]
        trusted_public_key: String,
        #[arg(long, default_value = "cli")]
        decided_by: String,
        #[arg(long)]
        reason: Option<String>,
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
enum ToolsCommand {
    /// List the registered first-party tools that models may request.
    #[command(visible_aliases = ["model", "catalog"])]
    List {
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print the raw JSON tool catalog.
        #[arg(long)]
        json: bool,
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
    /// Execute an already-approved first-party action.
    Execute {
        id: String,
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
    /// Show permission policy review items that need operator attention.
    Review {
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
            scheduler_recover_stale_on_startup,
            scheduler_stale_older_than_seconds,
            scheduler_stale_recovery_limit,
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
            if scheduler_recover_stale_on_startup {
                state.recover_stale_scheduler_jobs_automatically(
                    scheduler_stale_older_than_seconds,
                    scheduler_stale_recovery_limit,
                )?;
            }
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
            let response = server_required_request(&endpoint, "GET", "/health", None)?;
            println!("{}", format_health(&response)?);
        }
        CliCommand::Contract { endpoint } => {
            println!("{}", contract(&endpoint)?);
        }
        CliCommand::Release { command } => match command {
            ReleaseCommand::Readiness {
                endpoint,
                json,
                all_commands,
            } => {
                let response = release_readiness(&endpoint)?;
                if json || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_release_readiness(&response, all_commands)?);
                }
            }
            ReleaseCommand::EvidenceStatus { endpoint, json } => {
                let response = release_evidence_status(&endpoint)?;
                if json || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_release_evidence_status(&response)?);
                }
            }
            ReleaseCommand::LiveDeviceRunbook { endpoint, json } => {
                let readiness = release_readiness(&endpoint)?;
                let evidence_status = release_evidence_status(&endpoint)?;
                if json || cli_json_requested() {
                    println!(
                        "{}",
                        release_live_device_runbook_json(&readiness, &evidence_status)?
                    );
                } else {
                    println!(
                        "{}",
                        format_release_live_device_runbook(&readiness, &evidence_status)?
                    );
                }
            }
            ReleaseCommand::SignedDistributionRunbook { endpoint, json } => {
                let readiness = release_readiness(&endpoint)?;
                let evidence_status = release_evidence_status(&endpoint)?;
                if json || cli_json_requested() {
                    println!(
                        "{}",
                        release_signed_distribution_runbook_json(&readiness, &evidence_status)?
                    );
                } else {
                    println!(
                        "{}",
                        format_release_signed_distribution_runbook(&readiness, &evidence_status)?
                    );
                }
            }
            ReleaseCommand::PluginTrustRunbook { endpoint, json } => {
                let readiness = release_readiness(&endpoint)?;
                let evidence_status = release_evidence_status(&endpoint)?;
                if json || cli_json_requested() {
                    println!(
                        "{}",
                        release_plugin_trust_runbook_json(&readiness, &evidence_status)?
                    );
                } else {
                    println!(
                        "{}",
                        format_release_plugin_trust_runbook(&readiness, &evidence_status)?
                    );
                }
            }
        },
        CliCommand::Smoke => {
            run_smoke().await?;
        }
        CliCommand::Command {
            input,
            endpoint,
            dry_run,
            sensitivity,
            json,
        } => {
            let body = serde_json::to_string(&CommandRequest {
                input,
                session_id: None,
                context: serde_json::Value::Null,
                dry_run,
                proactive: false,
                sensitivity: sensitivity.as_deref().map(parse_sensitivity).transpose()?,
            })?;
            let response = server_required_request(&endpoint, "POST", "/commands", Some(&body))?;
            if json || cli_json_requested() {
                println!("{response}");
            } else {
                println!("{}", format_command_response(&response)?);
            }
        }
        CliCommand::Pause { reason, endpoint } => {
            let body = serde_json::to_string(&EmergencyPauseRequest { reason })?;
            println!(
                "{}",
                server_required_request(&endpoint, "POST", "/emergency-pause", Some(&body))?
            );
        }
        CliCommand::Resume { endpoint } => {
            println!(
                "{}",
                server_required_request(&endpoint, "DELETE", "/emergency-pause", None)?
            );
        }
        CliCommand::PauseStatus { endpoint } => {
            println!(
                "{}",
                server_required_request(&endpoint, "GET", "/emergency-pause", None)?
            );
        }
        CliCommand::Scheduler { command } => match command {
            SchedulerCommand::List { endpoint } => {
                println!(
                    "{}",
                    server_required_request(&endpoint, "GET", "/scheduler/jobs", None)?
                );
            }
            SchedulerCommand::Attention { endpoint } => {
                println!(
                    "{}",
                    server_required_request(&endpoint, "GET", "/scheduler/attention", None)?
                );
            }
            SchedulerCommand::Get { id, endpoint } => {
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "GET",
                        &format!("/scheduler/jobs/{id}"),
                        None
                    )?
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
                    server_required_request(&endpoint, "POST", "/scheduler/jobs", Some(&body))?
                );
            }
            SchedulerCommand::RunDue { limit, endpoint } => {
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!("/scheduler/run-due?limit={limit}"),
                        None,
                    )?
                );
            }
            SchedulerCommand::RecoverStale {
                older_than_seconds,
                limit,
                endpoint,
            } => {
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!(
                            "/scheduler/recover-stale?older_than_seconds={older_than_seconds}&limit={limit}"
                        ),
                        None,
                    )?
                );
            }
            SchedulerCommand::Cancel { id, endpoint } => {
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "DELETE",
                        &format!("/scheduler/jobs/{id}"),
                        None
                    )?
                );
            }
        },
        CliCommand::Diagnostics { command } => match command {
            DiagnosticsCommand::Export { endpoint } => {
                println!(
                    "{}",
                    server_required_request(&endpoint, "GET", "/diagnostics/export", None)?
                );
            }
        },
        CliCommand::Tasks { command } => match command {
            TasksCommand::List { endpoint, json } => {
                let response = server_required_request(&endpoint, "GET", "/tasks", None)?;
                if json || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_task_list(&response)?);
                }
            }
            TasksCommand::Get { id, endpoint, json } => {
                let response =
                    server_required_request(&endpoint, "GET", &format!("/tasks/{id}"), None)?;
                if json || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_task_detail(&response)?);
                }
            }
            TasksCommand::Audit {
                task_id,
                endpoint,
                json,
            } => {
                let path = task_id
                    .map(|id| format!("/tasks/{id}/audit"))
                    .unwrap_or_else(|| "/audit".to_string());
                let response = server_required_request(&endpoint, "GET", &path, None)?;
                if json || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_audit_entries(&response)?);
                }
            }
        },
        CliCommand::Activity { command } => {
            match command {
                ActivityCommand::Summary { endpoint, json } => {
                    let response =
                        server_required_request(&endpoint, "GET", "/activity/summary", None)?;
                    if json || cli_json_requested() {
                        println!("{response}");
                    } else {
                        println!("{}", format_activity_summary(&response)?);
                    }
                }
                ActivityCommand::Watch {
                    endpoint,
                    max_events,
                    interval_ms,
                } => {
                    println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "GET",
                        &format!("/activity/events?max_events={max_events}&interval_ms={interval_ms}"),
                        None,
                    )?
                );
                }
            }
        }
        CliCommand::Routes { command } => match command {
            RoutesCommand::List {
                task_id,
                endpoint,
                json,
            } => {
                let path = task_id
                    .map(|id| format!("/model-routes?task_id={id}"))
                    .unwrap_or_else(|| "/model-routes".to_string());
                let response = server_required_request(&endpoint, "GET", &path, None)?;
                if json || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_route_list(&response)?);
                }
            }
            RoutesCommand::Get { id, endpoint, json } => {
                let response = server_required_request(
                    &endpoint,
                    "GET",
                    &format!("/model-routes/{id}"),
                    None,
                )?;
                if json || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_route_detail(&response)?);
                }
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
                println!("{}", server_required_request(&endpoint, "GET", path, None)?);
            }
            MemoryCommand::Get { id, endpoint } => {
                println!(
                    "{}",
                    server_required_request(&endpoint, "GET", &format!("/memory/{id}"), None)?
                );
            }
            MemoryCommand::Classification {
                include_deleted,
                endpoint,
            } => {
                let path = if include_deleted {
                    "/memory/classification?include_deleted=true"
                } else {
                    "/memory/classification"
                };
                println!("{}", server_required_request(&endpoint, "GET", path, None)?);
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
                println!(
                    "{}",
                    server_required_request(&endpoint, "POST", "/memory", Some(&body))?
                );
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
                    server_required_request(
                        &endpoint,
                        "PATCH",
                        &format!("/memory/{id}"),
                        Some(&body)
                    )?
                );
            }
            MemoryCommand::Review { id, endpoint } => {
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!("/memory/{id}/review"),
                        None
                    )?
                );
            }
            MemoryCommand::Delete { id, endpoint } => {
                println!(
                    "{}",
                    server_required_request(&endpoint, "DELETE", &format!("/memory/{id}"), None)?
                );
            }
            MemoryCommand::Restore { id, endpoint } => {
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!("/memory/{id}/restore"),
                        None
                    )?
                );
            }
        },
        CliCommand::Plugins { command } => match command {
            PluginsCommand::List { endpoint, json } => {
                let response = plugin_manifests(&endpoint)?;
                if json || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_plugin_manifest_list(&response)?);
                }
            }
            PluginsCommand::Get { id, endpoint, json } => {
                let response = plugin_manifest(&endpoint, &id)?;
                if json || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_plugin_manifest_detail(&response)?);
                }
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
                    server_required_request(&endpoint, "POST", "/plugins/installed", Some(&body))?
                );
            }
            PluginsCommand::Installed { endpoint } => {
                println!(
                    "{}",
                    server_required_request(&endpoint, "GET", "/plugins/installed", None)?
                );
            }
            PluginsCommand::InstalledGet { id, endpoint } => {
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "GET",
                        &format!("/plugins/installed/{id}"),
                        None
                    )?
                );
            }
            PluginsCommand::EnableInstalled {
                id,
                grant,
                endpoint,
            } => {
                let body = serde_json::to_string(&InstalledPluginExecutionRequest {
                    execution_enabled: true,
                    execution_grant: grant,
                })?;
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!("/plugins/installed/{id}/execution"),
                        Some(&body)
                    )?
                );
            }
            PluginsCommand::DisableInstalled { id, endpoint } => {
                let body = serde_json::to_string(&InstalledPluginExecutionRequest {
                    execution_enabled: false,
                    execution_grant: InstalledPluginExecutionGrant::MetadataOnly,
                })?;
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!("/plugins/installed/{id}/execution"),
                        Some(&body)
                    )?
                );
            }
            PluginsCommand::VerifyInstalled { id, endpoint } => {
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!("/plugins/installed/{id}/provenance/verify"),
                        None
                    )?
                );
            }
            PluginsCommand::VerifyPublisher {
                id,
                trusted_origin,
                decided_by,
                reason,
                endpoint,
            } => {
                let body = serde_json::to_string(&InstalledPluginPublisherVerificationRequest {
                    trusted_origin,
                    decided_by,
                    reason,
                })?;
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!("/plugins/installed/{id}/publisher/verify"),
                        Some(&body)
                    )?
                );
            }
            PluginsCommand::VerifyPublisherSignature {
                id,
                trusted_public_key,
                decided_by,
                reason,
                endpoint,
            } => {
                let body =
                    serde_json::to_string(&InstalledPluginPublisherSignatureVerificationRequest {
                        trusted_public_key,
                        decided_by,
                        reason,
                    })?;
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!("/plugins/installed/{id}/publisher/signature/verify"),
                        Some(&body)
                    )?
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
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!("/plugins/installed/{id}/run"),
                        Some(&body)
                    )?
                );
            }
        },
        CliCommand::Tools { command } => match command {
            ToolsCommand::List { endpoint, json } => {
                let response = model_tool_catalog(&endpoint)?;
                if json || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_model_tool_catalog(&response)?);
                }
            }
        },
        CliCommand::Approvals { command } => match command {
            ApprovalsCommand::List { status, endpoint } => {
                let path = status
                    .map(|status| format!("/approvals?status={status}"))
                    .unwrap_or_else(|| "/approvals".to_string());
                println!(
                    "{}",
                    server_required_request(&endpoint, "GET", &path, None)?
                );
            }
            ApprovalsCommand::Get { id, endpoint } => {
                println!(
                    "{}",
                    server_required_request(&endpoint, "GET", &format!("/approvals/{id}"), None)?
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
                    server_required_request(
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
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!("/approvals/{id}/deny"),
                        Some(&body)
                    )?
                );
            }
            ApprovalsCommand::Execute { id, endpoint } => {
                println!(
                    "{}",
                    server_required_request(
                        &endpoint,
                        "POST",
                        &format!("/approvals/{id}/execute"),
                        None
                    )?
                );
            }
        },
        CliCommand::Permissions { command } => match command {
            PermissionsCommand::Grants { endpoint } => {
                println!(
                    "{}",
                    server_required_request(&endpoint, "GET", "/permissions/grants", None)?
                );
            }
            PermissionsCommand::Review { endpoint } => {
                println!(
                    "{}",
                    server_required_request(&endpoint, "GET", "/permissions/policy-review", None)?
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

fn server_required_request(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> anyhow::Result<String> {
    request(endpoint, method, path, body).map_err(|error| {
        if is_transport_unavailable(&error) {
            server_required_unavailable_error(endpoint, &error)
        } else {
            error
        }
    })
}

fn release_readiness(endpoint: &str) -> anyhow::Result<String> {
    match request(endpoint, "GET", "/release/readiness", None) {
        Ok(response) => Ok(response),
        Err(error) if is_transport_unavailable(&error) => {
            let readiness = jarvis_core::IpcState::new().release_readiness();
            Ok(serde_json::to_string(&readiness)?)
        }
        Err(error) => Err(error),
    }
}

fn release_evidence_status(endpoint: &str) -> anyhow::Result<String> {
    match request(endpoint, "GET", "/release/evidence-status", None) {
        Ok(response) => Ok(response),
        Err(error) if is_transport_unavailable(&error) => {
            let status = jarvis_core::IpcState::new().release_evidence_status();
            Ok(serde_json::to_string(&status)?)
        }
        Err(error) => Err(error),
    }
}

fn contract(endpoint: &str) -> anyhow::Result<String> {
    match request(endpoint, "GET", "/contract", None) {
        Ok(response) => Ok(response),
        Err(error) if is_transport_unavailable(&error) => {
            let contract = jarvis_core::IpcState::new().contract();
            Ok(serde_json::to_string(&contract)?)
        }
        Err(error) => Err(error),
    }
}

fn plugin_manifests(endpoint: &str) -> anyhow::Result<String> {
    match request(endpoint, "GET", "/plugins/manifests", None) {
        Ok(response) => Ok(response),
        Err(error) if is_transport_unavailable(&error) => {
            let host = jarvis_core::PluginHost::with_first_party_plugins()?;
            Ok(serde_json::to_string(&host.manifests()?)?)
        }
        Err(error) => Err(error),
    }
}

fn plugin_manifest(endpoint: &str, id: &str) -> anyhow::Result<String> {
    match request(endpoint, "GET", &format!("/plugins/manifests/{id}"), None) {
        Ok(response) => Ok(response),
        Err(error) if is_transport_unavailable(&error) => {
            let host = jarvis_core::PluginHost::with_first_party_plugins()?;
            Ok(serde_json::to_string(&host.manifest(id)?)?)
        }
        Err(error) => Err(error),
    }
}

fn model_tool_catalog(endpoint: &str) -> anyhow::Result<String> {
    match request(endpoint, "GET", "/tools/model", None) {
        Ok(response) => Ok(response),
        Err(error) if is_transport_unavailable(&error) => {
            let catalog = jarvis_core::IpcState::new().model_tool_catalog()?;
            Ok(serde_json::to_string(&catalog)?)
        }
        Err(error) => Err(error),
    }
}

fn cli_json_requested() -> bool {
    std::env::var("JARVIS_CLI_JSON")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn format_command_response(response: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let status = value
        .pointer("/task/status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let accepted = value
        .get("accepted")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let task_id = value
        .pointer("/task/id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let mut lines = vec![
        format!("Jarvis command: {status}"),
        format!("Accepted: {accepted}"),
        format!("Task: {task_id}"),
    ];

    if let Some(route) = value.get("route").and_then(serde_json::Value::as_object) {
        let provider = route
            .get("provider")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let model = route
            .get("model")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let reason = route
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("no route reason provided");
        lines.push(format!("Route: {provider} / {model}"));
        lines.push(format!("Route reason: {reason}"));
    }

    if let Some(message) = value.get("message").and_then(serde_json::Value::as_str) {
        lines.push("Message:".to_string());
        lines.push(message.to_string());
    }

    let mut tool_lines = Vec::new();
    if let Some(steps) = value.get("steps").and_then(serde_json::Value::as_array) {
        for step in steps {
            let Some(results) = step
                .get("tool_results")
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            for result in results {
                let plugin_id = result
                    .get("plugin_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown_plugin");
                let action = result
                    .get("action")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown_action");
                let result_status = result
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown");
                let mut line = format!("- {plugin_id}.{action}: {result_status}");
                if let Some(error) = result
                    .pointer("/output/error")
                    .and_then(serde_json::Value::as_str)
                {
                    line.push_str(&format!(" ({error})"));
                }
                if let Some(guidance) = result
                    .pointer("/output/guidance")
                    .and_then(serde_json::Value::as_str)
                {
                    line.push_str(&format!(" Guidance: {guidance}"));
                }
                tool_lines.push(line);
            }
        }
    }
    if let Some(results) = value
        .get("plugin_results")
        .and_then(serde_json::Value::as_array)
    {
        for result in results {
            let plugin_id = result
                .get("plugin_id")
                .or_else(|| result.pointer("/metadata/plugin_id"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown_plugin");
            let action = result
                .get("action")
                .or_else(|| result.pointer("/metadata/action"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown_action");
            let result_status = result
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let mut line = format!("- {plugin_id}.{action}: {result_status}");
            if let Some(error) = result
                .pointer("/output/error")
                .and_then(serde_json::Value::as_str)
            {
                line.push_str(&format!(" ({error})"));
            }
            if let Some(guidance) = result
                .pointer("/output/guidance")
                .and_then(serde_json::Value::as_str)
            {
                line.push_str(&format!(" Guidance: {guidance}"));
            }
            tool_lines.push(line);
        }
    }
    if !tool_lines.is_empty() {
        lines.push("Tools:".to_string());
        lines.extend(tool_lines);
    }

    let latest_audit_entry = value
        .get("audit_entry")
        .and_then(serde_json::Value::as_object)
        .or_else(|| {
            value
                .get("audit_entries")
                .and_then(serde_json::Value::as_array)
                .and_then(|entries| entries.last())
                .and_then(serde_json::Value::as_object)
        });
    if let Some(entry) = latest_audit_entry {
        let event_type = entry
            .get("event_type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown_event");
        let summary = entry
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("no audit summary");
        lines.push(format!("Latest audit: {event_type} - {summary}"));
        if let Some(error) = entry
            .get("payload")
            .and_then(|payload| payload.get("error"))
            .and_then(serde_json::Value::as_str)
        {
            lines.push(format!("Latest audit detail: {error}"));
        }
    }

    lines.push("Raw JSON: rerun with --json for full audit and route evidence.".to_string());
    Ok(lines.join("\n"))
}

fn format_model_tool_catalog(response: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let mut lines = vec!["Registered first-party model tools:".to_string()];
    if let Some(tools) = value.get("tools").and_then(serde_json::Value::as_array) {
        for tool in tools {
            let plugin_id = tool
                .get("plugin_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown_plugin");
            let action = tool
                .get("action")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown_action");
            let description = tool
                .get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("no description");
            lines.push(format!("- {plugin_id}.{action}: {description}"));
        }
    }
    if let Some(proof_boundary) = value
        .get("proof_boundary")
        .and_then(serde_json::Value::as_str)
    {
        lines.push(format!("Boundary: {proof_boundary}"));
    }
    lines.push("Raw JSON: rerun with --json for the exact catalog payload.".to_string());
    Ok(lines.join("\n"))
}

fn format_plugin_manifest_list(response: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let manifests = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("plugin manifest list response was not an array"))?;
    let mut lines = vec![
        "Registered first-party plugins:".to_string(),
        format!("Total plugins: {}", manifests.len()),
    ];
    for manifest in manifests {
        lines.push(format!("- {}", format_plugin_manifest_summary(manifest)));
        if let Some(actions) = manifest
            .get("actions")
            .and_then(serde_json::Value::as_array)
        {
            let action_names = actions
                .iter()
                .filter_map(|action| action.get("name").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>();
            if !action_names.is_empty() {
                lines.push(format!("  Actions: {}", action_names.join(", ")));
            }
        }
    }
    lines.push("Model-visible tools: use `jarvis tools list` for exact plugin_id.action pairs models may request.".to_string());
    lines.push("Raw JSON: rerun with --json for full manifest schemas.".to_string());
    Ok(lines.join("\n"))
}

fn format_plugin_manifest_detail(response: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let mut lines = vec![
        "Registered first-party plugin:".to_string(),
        format!("- {}", format_plugin_manifest_summary(&value)),
    ];
    if let Some(actions) = value.get("actions").and_then(serde_json::Value::as_array) {
        lines.push("Actions:".to_string());
        for action in actions {
            let name = action
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown_action");
            let description = action
                .get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("no description");
            let risk = action
                .get("risk_tier")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let proactive = action
                .get("proactive")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            lines.push(format!(
                "- {name}: {description} (risk: {risk}, proactive: {proactive})"
            ));
        }
    }
    lines.push("Model-visible tools: use `jarvis tools list` for exact plugin_id.action pairs models may request.".to_string());
    lines.push("Raw JSON: rerun with --json for full manifest schemas.".to_string());
    Ok(lines.join("\n"))
}

fn format_plugin_manifest_summary(manifest: &serde_json::Value) -> String {
    let id = manifest
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown_plugin");
    let name = manifest
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Unnamed plugin");
    let version = manifest
        .get("version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown_version");
    let source = manifest
        .get("source")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown_source");
    format!("{id} ({name}, {version}, {source})")
}

fn format_task_list(response: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let tasks = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("task list response was not an array"))?;
    let mut lines = vec![
        "Jarvis tasks:".to_string(),
        format!("Total tasks: {}", tasks.len()),
    ];
    for task in tasks {
        lines.push(format!("- {}", format_task_summary(task)));
    }
    lines.push("Raw JSON: rerun with --json for exact task records.".to_string());
    Ok(lines.join("\n"))
}

fn format_task_detail(response: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let mut lines = vec![
        "Jarvis task:".to_string(),
        format!("- {}", format_task_summary(&value)),
    ];
    if let Some(session_id) = json_string(&value, "session_id") {
        lines.push(format!("Session: {session_id}"));
    }
    lines.push("Input: omitted from human output; rerun with --json if you need the exact stored task record.".to_string());
    lines.push("Raw JSON: rerun with --json for the exact task payload.".to_string());
    Ok(lines.join("\n"))
}

fn format_audit_entries(response: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let entries = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("audit response was not an array"))?;
    let mut lines = vec![
        "Jarvis audit entries:".to_string(),
        format!("Total entries: {}", entries.len()),
    ];
    for entry in entries {
        let event_type = json_string(entry, "event_type").unwrap_or("unknown_event");
        let summary = json_string(entry, "summary").unwrap_or("no summary");
        let task_id = json_string(entry, "task_id").unwrap_or("system");
        let created_at = json_string(entry, "created_at").unwrap_or("unknown time");
        lines.push(format!(
            "- {created_at} {event_type} [{task_id}]: {summary}"
        ));
    }
    lines.push("Raw JSON: rerun with --json for exact audit payloads.".to_string());
    Ok(lines.join("\n"))
}

fn format_activity_summary(response: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let repository_backed = value
        .get("repository_backed")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let task_count = json_u64(&value, "task_count");
    let audit_count = json_u64(&value, "audit_entry_count");
    let active_count = json_u64(&value, "active_task_count");
    let mut lines = vec![
        "Jarvis activity summary:".to_string(),
        format!("Repository backed: {repository_backed}"),
        format!("Tasks: {task_count} total, {active_count} active"),
        format!("Audit entries: {audit_count}"),
    ];

    if let Some(status_counts) = value
        .get("status_counts")
        .and_then(serde_json::Value::as_array)
        .filter(|counts| !counts.is_empty())
    {
        lines.push("Task statuses:".to_string());
        for status in status_counts {
            let label = json_string(status, "status").unwrap_or("unknown");
            let count = json_u64(status, "count");
            lines.push(format!("- {label}: {count}"));
        }
    }

    if let Some(recent_tasks) = value
        .get("recent_tasks")
        .and_then(serde_json::Value::as_array)
        .filter(|tasks| !tasks.is_empty())
    {
        lines.push("Recent tasks:".to_string());
        for task in recent_tasks.iter().take(5) {
            lines.push(format!("- {}", format_task_summary(task)));
        }
    }

    if let Some(entries) = value
        .get("recent_audit_entries")
        .and_then(serde_json::Value::as_array)
        .filter(|entries| !entries.is_empty())
    {
        lines.push("Recent audit:".to_string());
        for entry in entries.iter().take(5) {
            let event_type = json_string(entry, "event_type").unwrap_or("unknown_event");
            let summary = json_string(entry, "summary").unwrap_or("no summary");
            lines.push(format!("- {event_type}: {summary}"));
        }
    }

    lines.push("Raw JSON: rerun with --json for exact activity details.".to_string());
    Ok(lines.join("\n"))
}

fn format_route_list(response: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let routes = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("route list response was not an array"))?;
    let mut lines = vec![
        "Jarvis model routes:".to_string(),
        format!("Total routes: {}", routes.len()),
    ];
    for route in routes {
        lines.push(format!("- {}", format_route_summary(route)));
    }
    lines.push("Raw JSON: rerun with --json for exact route evidence.".to_string());
    Ok(lines.join("\n"))
}

fn format_route_detail(response: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let mut lines = vec![
        "Jarvis model route:".to_string(),
        format!("- {}", format_route_summary(&value)),
    ];
    if let Some(reason) = json_string(&value, "reason") {
        lines.push(format!("Reason: {reason}"));
    }
    if let Some(approval_status) = json_string(&value, "approval_status") {
        lines.push(format!("Approval: {approval_status}"));
    }
    if value
        .get("context_for_model")
        .is_some_and(|context| !context.is_null())
    {
        lines.push("Model context: retained in raw JSON for this route.".to_string());
    } else {
        lines.push("Model context: redacted from persisted route inspection.".to_string());
    }
    lines.push("Raw JSON: rerun with --json for exact route evidence.".to_string());
    Ok(lines.join("\n"))
}

fn format_task_summary(task: &serde_json::Value) -> String {
    let id = json_string(task, "id").unwrap_or("unknown_task");
    let status = json_string(task, "status").unwrap_or("unknown");
    let created_at = json_string(task, "created_at").unwrap_or("unknown time");
    let updated_at = json_string(task, "updated_at").unwrap_or("unknown update");
    format!("{id}: {status} (created {created_at}, updated {updated_at})")
}

fn format_route_summary(route: &serde_json::Value) -> String {
    let id = json_string(route, "id").unwrap_or("unknown_route");
    let provider = json_string(route, "selected_provider").unwrap_or("unknown_provider");
    let outcome = json_string(route, "outcome").unwrap_or("unknown");
    let sensitivity = json_string(route, "sensitivity").unwrap_or("unknown_sensitivity");
    let created_at = json_string(route, "created_at").unwrap_or("unknown time");
    let task_id = json_string(route, "task_id").unwrap_or("unknown_task");
    format!("{id}: {outcome} {provider} for task {task_id} ({sensitivity}, {created_at})")
}

fn json_string<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

fn json_u64(value: &serde_json::Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

fn format_release_readiness(response: &str, all_commands: bool) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let production_ready = value
        .get("production_ready")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let verified_feature_count = value
        .get("verified_feature_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let pending_feature_count = value
        .get("pending_feature_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let blocker_count = value
        .get("blocking_manual_gates")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);

    let mut lines = vec![
        "Jarvis release readiness:".to_string(),
        format!("Production ready: {production_ready}"),
        format!("Verified features: {verified_feature_count}"),
        format!("Pending features: {pending_feature_count}"),
        format!("Blocking manual gates: {blocker_count}"),
    ];

    if let Some(scope) = value
        .get("readiness_scope")
        .and_then(serde_json::Value::as_str)
    {
        lines.push(format!("Scope: {scope}"));
    }

    if let Some(features) = value
        .get("pending_features")
        .and_then(serde_json::Value::as_array)
        .filter(|features| !features.is_empty())
    {
        lines.push("Pending features:".to_string());
        for feature in features.iter().take(5) {
            let key = feature
                .get("key")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown_feature");
            let status = feature
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            lines.push(format!("- {key}: {status}"));
        }
    }

    if let Some(gates) = value
        .get("blocking_manual_gates")
        .and_then(serde_json::Value::as_array)
        .filter(|gates| !gates.is_empty())
    {
        lines.push("Top manual gates:".to_string());
        for gate in gates.iter().filter_map(serde_json::Value::as_str).take(5) {
            lines.push(format!("- {gate}"));
        }
    }

    if let Some(commands) = value
        .get("recommended_verification_commands")
        .and_then(serde_json::Value::as_array)
        .filter(|commands| !commands.is_empty())
    {
        let command_count = commands.len();
        let shown_command_count = if all_commands {
            command_count
        } else {
            command_count.min(4)
        };
        if all_commands {
            lines.push("Recommended verification commands:".to_string());
        } else {
            lines.push("Next verification commands:".to_string());
        }
        for command in commands
            .iter()
            .filter_map(serde_json::Value::as_str)
            .take(shown_command_count)
        {
            lines.push(format!("- {command}"));
        }
        if !all_commands && command_count > shown_command_count {
            lines.push(format!(
                "Showing {shown_command_count} of {command_count} commands; rerun with --all-commands for the full readable runbook."
            ));
        }
    }

    if let Some(boundary) = value
        .get("proof_boundary")
        .and_then(serde_json::Value::as_str)
    {
        lines.push(format!("Boundary: {boundary}"));
    }

    if all_commands {
        lines.push("Raw JSON: rerun with --json for full readiness evidence.".to_string());
    } else {
        lines.push(
            "Raw JSON: rerun with --json for full readiness evidence, or --all-commands for the full readable runbook."
                .to_string(),
        );
    }
    Ok(lines.join("\n"))
}

fn format_release_evidence_status(response: &str) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let complete = value
        .get("complete")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let missing_count = value
        .get("missing_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let invalid_count = value
        .get("invalid_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let mut lines = vec![
        "Jarvis release evidence status:".to_string(),
        format!("Complete: {complete}"),
        format!("Missing evidence: {missing_count}"),
        format!("Invalid evidence: {invalid_count}"),
    ];

    if let Some(items) = value
        .get("items")
        .and_then(serde_json::Value::as_array)
        .filter(|items| !items.is_empty())
    {
        lines.push("Evidence items:".to_string());
        for item in items {
            let key = item
                .get("key")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown_item");
            let status = item
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let label = item
                .get("label")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(key);
            lines.push(format!("- {key}: {status} ({label})"));
        }
    }

    if let Some(boundary) = value
        .get("proof_boundary")
        .and_then(serde_json::Value::as_str)
    {
        lines.push(format!("Boundary: {boundary}"));
    }

    lines.push("Raw JSON: rerun with --json for exact evidence inventory.".to_string());
    Ok(lines.join("\n"))
}

fn release_live_device_runbook_json(
    readiness_response: &str,
    evidence_status_response: &str,
) -> anyhow::Result<String> {
    let readiness: serde_json::Value = serde_json::from_str(readiness_response)?;
    let evidence_status: serde_json::Value = serde_json::from_str(evidence_status_response)?;
    let live_voice_feature = readiness
        .get("pending_features")
        .and_then(serde_json::Value::as_array)
        .and_then(|features| {
            features.iter().find(|feature| {
                feature.get("key").and_then(serde_json::Value::as_str) == Some("live_voice_loop")
            })
        })
        .cloned();
    let live_device_evidence = evidence_status
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("key").and_then(serde_json::Value::as_str) == Some("live_device_qa_report")
            })
        })
        .cloned();
    let payload = serde_json::json!({
        "generated_from": "release readiness plus evidence-status",
        "production_ready": readiness.get("production_ready").cloned().unwrap_or(serde_json::Value::Bool(false)),
        "live_voice_feature": live_voice_feature,
        "live_device_evidence": live_device_evidence,
        "commands": [
            "./scripts/release-live-device-qa.sh --check",
            "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env",
            "set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete",
            "cargo run -p jarvis-cli -- release evidence-status",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness"
        ],
        "manual_checks": [
            "Install the signed, notarized package into /Applications on a clean Mac profile.",
            "Launch Jarvis through Finder or LaunchServices.",
            "Verify microphone and Speech permission prompts during live voice capture.",
            "Speak the test phrase and confirm the observed transcript reaches the command path.",
            "Verify live speech output, notification delivery, restart behavior, and manual release QA.",
            "Preserve target/release-live-device-qa-report.json for final release evidence bundling."
        ],
        "proof_boundary": "Runbook and local evidence inspection only; this command does not perform live-device validation."
    });
    Ok(serde_json::to_string_pretty(&payload)?)
}

fn format_release_live_device_runbook(
    readiness_response: &str,
    evidence_status_response: &str,
) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(evidence_status_response)?;
    let readiness: serde_json::Value = serde_json::from_str(readiness_response)?;
    let live_voice_status = readiness
        .get("pending_features")
        .and_then(serde_json::Value::as_array)
        .and_then(|features| {
            features.iter().find(|feature| {
                feature.get("key").and_then(serde_json::Value::as_str) == Some("live_voice_loop")
            })
        })
        .and_then(|feature| feature.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("cleared_or_not_reported");
    let live_device_item = value
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("key").and_then(serde_json::Value::as_str) == Some("live_device_qa_report")
            })
        });
    let evidence_status = live_device_item
        .and_then(|item| item.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let evidence_detail = live_device_item
        .and_then(|item| item.get("detail"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("No live-device QA report detail was available.");

    Ok([
        "Jarvis live-device QA runbook:".to_string(),
        format!("Production ready: {}", readiness.get("production_ready").and_then(serde_json::Value::as_bool).unwrap_or(false)),
        format!("live_voice_loop: {live_voice_status}"),
        format!("live_device_qa_report: {evidence_status}"),
        format!("Evidence detail: {evidence_detail}"),
        "Run on the release machine:".to_string(),
        "- ./scripts/release-live-device-qa.sh --check".to_string(),
        "- ./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env".to_string(),
        "- set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete".to_string(),
        "- cargo run -p jarvis-cli -- release evidence-status".to_string(),
        "- JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness".to_string(),
        "Manual checks:".to_string(),
        "- Install the signed, notarized package into /Applications on a clean Mac profile.".to_string(),
        "- Launch Jarvis through Finder or LaunchServices.".to_string(),
        "- Verify microphone and Speech permission prompts during live voice capture.".to_string(),
        "- Speak the test phrase and confirm the observed transcript reaches the command path.".to_string(),
        "- Verify live speech output, notification delivery, restart behavior, and manual release QA.".to_string(),
        "- Preserve target/release-live-device-qa-report.json for final release evidence bundling.".to_string(),
        "Boundary: runbook and local evidence inspection only; no live-device validation was performed.".to_string(),
        "Raw JSON: rerun with --json for a structured runbook summary.".to_string(),
    ]
    .join("\n"))
}

fn release_signed_distribution_runbook_json(
    readiness_response: &str,
    evidence_status_response: &str,
) -> anyhow::Result<String> {
    let readiness: serde_json::Value = serde_json::from_str(readiness_response)?;
    let evidence_status: serde_json::Value = serde_json::from_str(evidence_status_response)?;
    let distribution_keys = [
        "signed_app_bundle",
        "app_executable",
        "bundled_core_executable",
        "signed_app_zip",
        "signed_installer_package",
        "signed_distribution_provenance_report",
    ];
    let distribution_evidence = evidence_status
        .get("items")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("key")
                        .and_then(serde_json::Value::as_str)
                        .map(|key| distribution_keys.contains(&key))
                        .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let payload = serde_json::json!({
        "generated_from": "release readiness plus evidence-status",
        "production_ready": readiness.get("production_ready").cloned().unwrap_or(serde_json::Value::Bool(false)),
        "distribution_evidence": distribution_evidence,
        "commands": [
            "./scripts/package-distribution.sh --unsigned-launch-check",
            "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh",
            "cargo run -p jarvis-cli -- release evidence-status",
            "./scripts/release-evidence-doctor.sh --check",
            "cargo run -p jarvis-cli -- release live-device-runbook"
        ],
        "manual_checks": [
            "Configure Developer ID Application and Installer identities plus the notarytool profile on the release Mac.",
            "Run the full package-distribution lane and preserve the signed zip, signed installer package, and signed provenance report.",
            "Confirm the signed app zip and installer package are notarized and stapled before clean-profile installation.",
            "Rerun evidence-status and evidence-doctor so missing or invalid signed artifact paths are visible before final bundling.",
            "Continue with live-device QA, plugin-trust QA, final evidence bundle generation, and external evidence-mode readiness."
        ],
        "proof_boundary": "Runbook and local evidence inspection only; this command does not perform signing, notarization, stapling, Gatekeeper assessment, installation, live-device QA, or plugin-trust QA."
    });
    Ok(serde_json::to_string_pretty(&payload)?)
}

fn format_release_signed_distribution_runbook(
    readiness_response: &str,
    evidence_status_response: &str,
) -> anyhow::Result<String> {
    let readiness: serde_json::Value = serde_json::from_str(readiness_response)?;
    let evidence_status: serde_json::Value = serde_json::from_str(evidence_status_response)?;
    let distribution_keys = [
        "signed_app_bundle",
        "app_executable",
        "bundled_core_executable",
        "signed_app_zip",
        "signed_installer_package",
        "signed_distribution_provenance_report",
    ];
    let mut lines = vec![
        "Jarvis signed distribution runbook:".to_string(),
        format!(
            "Production ready: {}",
            readiness
                .get("production_ready")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        ),
        "Signed distribution evidence:".to_string(),
    ];
    let items = evidence_status
        .get("items")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for key in distribution_keys {
        let item = items.iter().find(|item| {
            item.get("key")
                .and_then(serde_json::Value::as_str)
                .map(|candidate| candidate == key)
                .unwrap_or(false)
        });
        let status = item
            .and_then(|item| item.get("status"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let detail = item
            .and_then(|item| item.get("detail"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("No evidence detail was available.");
        lines.push(format!("- {key}: {status} ({detail})"));
    }
    lines.extend([
        "Run on the release machine:".to_string(),
        "- ./scripts/package-distribution.sh --unsigned-launch-check".to_string(),
        "- JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh".to_string(),
        "- cargo run -p jarvis-cli -- release evidence-status".to_string(),
        "- ./scripts/release-evidence-doctor.sh --check".to_string(),
        "- cargo run -p jarvis-cli -- release live-device-runbook".to_string(),
        "Manual checks:".to_string(),
        "- Configure Developer ID Application and Installer identities plus the notarytool profile on the release Mac.".to_string(),
        "- Preserve the signed zip, signed installer package, and signed provenance report generated by package-distribution.sh.".to_string(),
        "- Confirm notarization and stapling for both app and installer before clean-profile installation.".to_string(),
        "- Continue with live-device QA, plugin-trust QA, final evidence bundle generation, and external evidence-mode readiness.".to_string(),
        "Boundary: runbook and local evidence inspection only; no signing, notarization, stapling, Gatekeeper assessment, installation, live-device QA, or plugin-trust QA was performed.".to_string(),
        "Raw JSON: rerun with --json for a structured runbook summary.".to_string(),
    ]);
    Ok(lines.join("\n"))
}

fn release_plugin_trust_runbook_json(
    readiness_response: &str,
    evidence_status_response: &str,
) -> anyhow::Result<String> {
    let readiness: serde_json::Value = serde_json::from_str(readiness_response)?;
    let evidence_status: serde_json::Value = serde_json::from_str(evidence_status_response)?;
    let plugin_trust_evidence = evidence_status
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("key").and_then(serde_json::Value::as_str)
                    == Some("plugin_trust_qa_report")
            })
        })
        .cloned();
    let payload = serde_json::json!({
        "generated_from": "release readiness plus evidence-status",
        "production_ready": readiness.get("production_ready").cloned().unwrap_or(serde_json::Value::Bool(false)),
        "plugin_trust_evidence": plugin_trust_evidence,
        "commands": [
            "./scripts/release-plugin-trust-qa.sh --check",
            "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env",
            "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete",
            "cargo run -p jarvis-cli -- release evidence-status",
            "./scripts/release-evidence-doctor.sh --check",
            "cargo run -p jarvis-cli -- release signed-distribution-runbook"
        ],
        "manual_checks": [
            "Run the marketplace review workflow for every public plugin listing.",
            "Preserve malware scan evidence for distributed plugin archives and updates.",
            "Validate signed publisher policy for trusted publisher keys and revocation.",
            "Validate the macOS sandbox profile or equivalent OS-level confinement.",
            "Validate host-level egress enforcement with deny and declared-host allow fixtures.",
            "Preserve target/release-plugin-trust-qa-report.json for final release evidence bundling."
        ],
        "proof_boundary": "Runbook and local evidence inspection only; this command does not perform marketplace review, malware scanning, sandbox deployment, host-level egress enforcement, signing, notarization, live-device QA, or final evidence bundling."
    });
    Ok(serde_json::to_string_pretty(&payload)?)
}

fn format_release_plugin_trust_runbook(
    readiness_response: &str,
    evidence_status_response: &str,
) -> anyhow::Result<String> {
    let readiness: serde_json::Value = serde_json::from_str(readiness_response)?;
    let evidence_status: serde_json::Value = serde_json::from_str(evidence_status_response)?;
    let plugin_trust_item = evidence_status
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("key").and_then(serde_json::Value::as_str)
                    == Some("plugin_trust_qa_report")
            })
        });
    let evidence_status = plugin_trust_item
        .and_then(|item| item.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let evidence_detail = plugin_trust_item
        .and_then(|item| item.get("detail"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("No plugin-trust QA report detail was available.");

    Ok([
        "Jarvis plugin-trust QA runbook:".to_string(),
        format!(
            "Production ready: {}",
            readiness
                .get("production_ready")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        ),
        format!("plugin_trust_qa_report: {evidence_status}"),
        format!("Evidence detail: {evidence_detail}"),
        "Run on the release machine:".to_string(),
        "- ./scripts/release-plugin-trust-qa.sh --check".to_string(),
        "- ./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env".to_string(),
        "- set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete".to_string(),
        "- cargo run -p jarvis-cli -- release evidence-status".to_string(),
        "- ./scripts/release-evidence-doctor.sh --check".to_string(),
        "- cargo run -p jarvis-cli -- release signed-distribution-runbook".to_string(),
        "Manual checks:".to_string(),
        "- Run the marketplace review workflow for every public plugin listing.".to_string(),
        "- Preserve malware scan evidence for distributed plugin archives and updates.".to_string(),
        "- Validate signed publisher policy for trusted publisher keys and revocation.".to_string(),
        "- Validate the macOS sandbox profile or equivalent OS-level confinement.".to_string(),
        "- Validate host-level egress enforcement with deny and declared-host allow fixtures.".to_string(),
        "- Preserve target/release-plugin-trust-qa-report.json for final release evidence bundling.".to_string(),
        "Boundary: runbook and local evidence inspection only; no marketplace review, malware scanning, sandbox deployment, host-level egress enforcement, signing, notarization, live-device QA, or final evidence bundling was performed.".to_string(),
        "Raw JSON: rerun with --json for a structured runbook summary.".to_string(),
    ]
    .join("\n"))
}

fn is_transport_unavailable(error: &anyhow::Error) -> bool {
    let Some(error) = error.downcast_ref::<std::io::Error>() else {
        return false;
    };

    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::AddrNotAvailable
            | std::io::ErrorKind::PermissionDenied
            | std::io::ErrorKind::NotFound
    )
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
    require_array_contains_object_field(&contract_json["endpoints"], "path", "/tools/model")?;
    require_array_contains_object_field(&contract_json["endpoints"], "path", "/model-routes")?;

    let command_body = serde_json::to_string(&CommandRequest {
        input: "smoke command".to_string(),
        session_id: None,
        context: serde_json::json!({ "surface": "cli-smoke" }),
        dry_run: true,
        proactive: false,
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

    let tools = request(&endpoint, "GET", "/tools/model", None)?;
    let tools_json: serde_json::Value = serde_json::from_str(&tools)?;
    require_json_field(&tools_json, "source", "registered_first_party_plugins")?;
    require_array_contains_object_field(&tools_json["tools"], "plugin_id", "fake_status")?;

    let diagnostics = request(&endpoint, "GET", "/diagnostics/export", None)?;
    let diagnostics_json: serde_json::Value = serde_json::from_str(&diagnostics)?;
    require_json_field(
        &diagnostics_json,
        "redaction",
        "diagnostics export omits command bodies, scheduler commands, model route contexts, audit payloads, memory values, and cancellation reason text",
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

    let activity = request(&persistent_endpoint, "GET", "/activity/summary", None)?;
    let activity_json: serde_json::Value = serde_json::from_str(&activity)?;
    require_bool_field(&activity_json, "repository_backed", true)?;
    require_number_at_least(&activity_json, "task_count", 1)?;
    require_number_at_least(&activity_json, "audit_entry_count", 1)?;
    require_array_field(&activity_json, "recent_tasks")?;
    require_array_field(&activity_json, "recent_audit_entries")?;

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
    require_number_at_least(&persistent_diagnostics_json, "model_route_record_count", 1)?;

    let routes = request(&persistent_endpoint, "GET", "/model-routes", None)?;
    let routes_json: serde_json::Value = serde_json::from_str(&routes)?;
    require_array_field(&routes_json, "root")?;

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

fn server_required_unavailable_error(endpoint: &str, source: &anyhow::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "jarvis-core is unavailable at {endpoint}. Start the IPC server with `cargo run -p jarvis-cli -- serve`, or run `cargo run -p jarvis-cli -- smoke` for an offline ephemeral health smoke. This command requires a running repository-backed core. Read-only inspection commands such as `jarvis release readiness`, `jarvis plugins list`, and `jarvis tools list` can still fall back to local metadata when the server is not running. Detail: {source}"
    )
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

#[cfg(test)]
mod tests {
    use super::is_transport_unavailable;

    #[test]
    fn transport_unavailable_includes_restricted_loopback_errors() {
        for kind in [
            std::io::ErrorKind::ConnectionRefused,
            std::io::ErrorKind::ConnectionReset,
            std::io::ErrorKind::ConnectionAborted,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::AddrNotAvailable,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::NotFound,
        ] {
            let error = anyhow::Error::from(std::io::Error::from(kind));
            assert!(
                is_transport_unavailable(&error),
                "expected {kind:?} to trigger read-only local fallback"
            );
        }
    }

    #[test]
    fn transport_unavailable_excludes_protocol_and_http_failures() {
        let invalid_endpoint = anyhow::anyhow!("only http:// endpoints are supported");
        assert!(!is_transport_unavailable(&invalid_endpoint));

        let server_error = anyhow::anyhow!("HTTP/1.1 500 Internal Server Error");
        assert!(!is_transport_unavailable(&server_error));
    }
}
