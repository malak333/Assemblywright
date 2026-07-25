use std::io::{Read, Write};
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::sync::OnceLock;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use clap::{Parser, Subcommand, ValueEnum};

const MAX_IPC_TOKEN_FILE_BYTES: usize = 1024;
static IPC_BEARER_TOKEN: OnceLock<String> = OnceLock::new();

#[derive(Debug, Parser)]
#[command(name = "assemblywright")]
#[command(about = "Assemblywright release evidence CLI")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    /// Read IPC bearer credentials from a bounded owner-only JSON file.
    #[arg(long, global = true, env = "JARVIS_IPC_TOKEN_FILE")]
    ipc_token_file: Option<PathBuf>,
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct IpcTokenFile {
    version: u16,
    scheme: IpcTokenScheme,
    token: String,
    generation: u64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum IpcTokenScheme {
    Bearer,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Summarize release-readiness evidence and remaining production blockers.
    #[command(
        long_about = "Read-only release operator commands.\n\nThese commands prefer the configured IPC endpoint. When no endpoint is reachable, they fall back to conservative local metadata or local file/report inspection without executing release side effects."
    )]
    Release {
        #[command(subcommand)]
        command: ReleaseCommand,
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
        /// Compatibility alias for machine-readable output. Only `json` is supported.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
        /// Print every recommended verification command in readable output.
        #[arg(long)]
        all_commands: bool,
    },
    /// Print release evidence file/report status.
    #[command(
        long_about = "Print release evidence file/report status.\n\nThis is file/report inventory plus semantic report validation only. It can report whether expected artifact paths and JSON reports are present, missing, or invalid, and checks app bundle metadata, bundled-core version markers, signed-provenance digests, live-device command evidence, owner-asserted plugin-trust review source, host-egress evidence fields, child-report validity, final-bundle archive URI validation, and final-bundle local signature-validation status. It does not prove Developer ID signing, notarization, stapling, installation, Finder launch, live-device QA, marketplace review, malware scanning, OS sandboxing, or host-level egress enforcement. Default output is operator-readable with per-item paths/details and same-line presence-only caveats; use --json for the exact structured payload."
    )]
    EvidenceStatus {
        /// HTTP IPC endpoint. Falls back to local read-only evidence inspection when unavailable.
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print the raw JSON evidence-status payload.
        #[arg(long)]
        json: bool,
        /// Compatibility alias for machine-readable output. Only `json` is supported.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
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
        /// Compatibility alias for machine-readable output. Only `json` is supported.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
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
        /// Compatibility alias for machine-readable output. Only `json` is supported.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Print the final evidence-bundle runbook and current evidence status.
    #[command(
        long_about = "Print the final evidence-bundle runbook and current evidence status.\n\nThis is a read-only operator guide for generating the final release evidence bundle after signed distribution, live-device QA, and plugin-trust QA evidence exist. It combines conservative release readiness with local evidence-status inspection and does not generate the bundle, sign, notarize, staple, install, launch Finder, run live-device QA, perform marketplace review, scan malware, deploy a sandbox, or enforce host-level egress."
    )]
    EvidenceBundleRunbook {
        /// HTTP IPC endpoint. Falls back to local read-only readiness and evidence inspection when unavailable.
        #[arg(long, default_value = "http://127.0.0.1:7787")]
        endpoint: String,
        /// Print a structured JSON runbook summary.
        #[arg(long)]
        json: bool,
        /// Compatibility alias for machine-readable output. Only `json` is supported.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Json,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let cli = Cli::parse();
    if let Some(path) = cli.ipc_token_file.as_deref() {
        let token = read_ipc_token_file(path)?;
        IPC_BEARER_TOKEN
            .set(token)
            .map_err(|_| anyhow::anyhow!("IPC bearer credentials were already configured"))?;
    }

    match cli.command {
        CliCommand::Release { command } => match command {
            ReleaseCommand::Readiness {
                endpoint,
                json,
                format,
                all_commands,
            } => {
                let response = release_readiness(&endpoint)?;
                if json || format == Some(OutputFormat::Json) || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_release_readiness(&response, all_commands)?);
                }
            }
            ReleaseCommand::EvidenceStatus {
                endpoint,
                json,
                format,
            } => {
                let response = release_evidence_status(&endpoint)?;
                if json || format == Some(OutputFormat::Json) || cli_json_requested() {
                    println!("{response}");
                } else {
                    println!("{}", format_release_evidence_status(&response)?);
                }
            }
            ReleaseCommand::LiveDeviceRunbook {
                endpoint,
                json,
                format,
            } => {
                let readiness = release_readiness(&endpoint)?;
                let evidence_status = release_evidence_status(&endpoint)?;
                if json || format == Some(OutputFormat::Json) || cli_json_requested() {
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
            ReleaseCommand::SignedDistributionRunbook {
                endpoint,
                json,
                format,
            } => {
                let readiness = release_readiness(&endpoint)?;
                let evidence_status = release_evidence_status(&endpoint)?;
                if json || format == Some(OutputFormat::Json) || cli_json_requested() {
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
            ReleaseCommand::EvidenceBundleRunbook {
                endpoint,
                json,
                format,
            } => {
                let readiness = release_readiness(&endpoint)?;
                let evidence_status = release_evidence_status(&endpoint)?;
                if json || format == Some(OutputFormat::Json) || cli_json_requested() {
                    println!(
                        "{}",
                        release_evidence_bundle_runbook_json(&readiness, &evidence_status)?
                    );
                } else {
                    println!(
                        "{}",
                        format_release_evidence_bundle_runbook(&readiness, &evidence_status)?
                    );
                }
            }
        },
    }

    Ok(())
}

fn read_ipc_token_file(path: &std::path::Path) -> anyhow::Result<String> {
    use rustix::fs::{fstat, open, FileType, Mode, OFlags};

    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| anyhow::anyhow!("IPC token file could not be opened safely"))?;
    let stat = fstat(&descriptor)
        .map_err(|_| anyhow::anyhow!("IPC token file could not be inspected safely"))?;
    if !FileType::from_raw_mode(stat.st_mode).is_file()
        || stat.st_uid != rustix::process::getuid().as_raw()
        || stat.st_mode & 0o077 != 0
        || stat.st_nlink != 1
        || stat.st_size < 0
        || stat.st_size as usize > MAX_IPC_TOKEN_FILE_BYTES
    {
        anyhow::bail!("IPC token file must be one bounded, owner-only, owner-matched regular file");
    }
    let mut bytes = Vec::with_capacity(stat.st_size as usize);
    std::fs::File::from(descriptor)
        .take((MAX_IPC_TOKEN_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > MAX_IPC_TOKEN_FILE_BYTES {
        anyhow::bail!("IPC token file is empty or exceeds its size limit");
    }
    let document: IpcTokenFile =
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("IPC token file is invalid"))?;
    if document.version != 1 || document.generation == 0 {
        anyhow::bail!("IPC token file version or generation is invalid");
    }
    match document.scheme {
        IpcTokenScheme::Bearer => {}
    }
    if document.token.len() != jarvis_core::IPC_BEARER_TOKEN_LENGTH
        || !document
            .token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        || URL_SAFE_NO_PAD
            .decode(&document.token)
            .map(|decoded| decoded.len() != jarvis_core::IPC_BEARER_TOKEN_BYTES)
            .unwrap_or(true)
    {
        anyhow::bail!("IPC token file contains invalid bearer credentials");
    }
    Ok(document.token)
}

fn request(endpoint: &str, method: &str, path: &str, body: Option<&str>) -> anyhow::Result<String> {
    let target = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| anyhow::anyhow!("only http:// endpoints are supported"))?;
    let host_port = target.trim_end_matches('/');
    let addresses = host_port.to_socket_addrs()?.collect::<Vec<_>>();
    if addresses.is_empty() {
        anyhow::bail!("could not resolve endpoint: {endpoint}");
    }
    if IPC_BEARER_TOKEN.get().is_some() && addresses.iter().any(|value| !value.ip().is_loopback()) {
        anyhow::bail!("authenticated IPC credentials may be sent only to a loopback endpoint");
    }
    let address = addresses[0];
    let mut stream = TcpStream::connect(address)?;
    let body = body.unwrap_or("");
    let authorization = IPC_BEARER_TOKEN
        .get()
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host_port}\r\n{authorization}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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

fn release_readiness(endpoint: &str) -> anyhow::Result<String> {
    match request(endpoint, "GET", "/release/readiness", None) {
        Ok(response) => Ok(response),
        Err(error) if is_transport_unavailable(&error) => {
            let readiness = jarvis_core::release_readiness();
            Ok(serde_json::to_string(&readiness)?)
        }
        Err(error) => Err(error),
    }
}

fn release_evidence_status(endpoint: &str) -> anyhow::Result<String> {
    match request(endpoint, "GET", "/release/evidence-status", None) {
        Ok(response) => Ok(response),
        Err(error) if is_transport_unavailable(&error) => {
            let status = jarvis_core::release_evidence_status();
            Ok(serde_json::to_string(&status)?)
        }
        Err(error) => Err(error),
    }
}

fn cli_json_requested() -> bool {
    std::env::var("JARVIS_CLI_JSON")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn format_release_readiness(response: &str, all_commands: bool) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(response)?;
    let production_ready = value
        .get("production_ready")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let evidence_mode_enabled = value
        .get("evidence_mode_enabled")
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
        "Assemblywright release readiness:".to_string(),
        format!("Production ready: {production_ready}"),
        format!("External evidence mode: {evidence_mode_enabled}"),
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
        "Assemblywright release evidence status:".to_string(),
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
            let detail = item.get("detail").and_then(serde_json::Value::as_str);
            let status_caveat = if status == "present"
                && detail
                    .map(|detail| detail.contains("presence only"))
                    .unwrap_or(false)
            {
                "; presence-only caveat"
            } else {
                ""
            };
            lines.push(format!("- {key}: {status}{status_caveat} ({label})"));
            if let Some(path) = item.get("path").and_then(serde_json::Value::as_str) {
                lines.push(format!("  path: {path}"));
            }
            if let Some(detail) = detail {
                lines.push(format!("  detail: {detail}"));
            }
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
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' in target/release-live-device-qa.env before collecting command evidence",
            "Launch Assemblywright with JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true for this operator evidence session, then confirm JARVIS_IPC_TOKEN_FILE points to the app-owned ipc-session-auth.json path before IPC commands",
            "cargo run -p jarvis-cli -- command \"status check\" --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\" --json",
            "Record the returned task ID as JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid>' or a task-associated audit ID as 'audit:<uuid>' in target/release-live-device-qa.env",
            "set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"",
            "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
        ],
        "manual_checks": [
            "Install the signed, notarized package into /Applications on a clean Mac profile.",
            "Launch Assemblywright through Finder or LaunchServices.",
            "Verify microphone and Speech permission prompts during live voice capture.",
            "Speak the test phrase and confirm the observed transcript reaches the command path.",
            "Verify live speech output, structured scheduler notification kind/title/body/thread evidence, restart behavior, and manual release QA.",
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
        "Assemblywright live-device QA runbook:".to_string(),
        format!("Production ready: {}", readiness.get("production_ready").and_then(serde_json::Value::as_bool).unwrap_or(false)),
        format!("live_voice_loop: {live_voice_status}"),
        format!("live_device_qa_report: {evidence_status}"),
        format!("Evidence detail: {evidence_detail}"),
        "Run on the release machine:".to_string(),
        "- ./scripts/release-live-device-qa.sh --check".to_string(),
        "- ./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env".to_string(),
        "- Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' in target/release-live-device-qa.env before collecting command evidence".to_string(),
        "- Launch Assemblywright with JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true for this operator evidence session, then confirm JARVIS_IPC_TOKEN_FILE points to the app-owned ipc-session-auth.json path before IPC commands".to_string(),
        "- cargo run -p jarvis-cli -- command \"status check\" --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\" --json".to_string(),
        "- Record the returned task ID as JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid>' or a task-associated audit ID as 'audit:<uuid>' in target/release-live-device-qa.env".to_string(),
        "- set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete".to_string(),
        "- JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"".to_string(),
        "- Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external".to_string(),
        "- JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"".to_string(),
        "Manual checks:".to_string(),
        "- Install the signed, notarized package into /Applications on a clean Mac profile.".to_string(),
        "- Launch Assemblywright through Finder or LaunchServices.".to_string(),
        "- Verify microphone and Speech permission prompts during live voice capture.".to_string(),
        "- Speak the test phrase and confirm the observed transcript reaches the command path.".to_string(),
        "- Verify live speech output, structured scheduler notification kind/title/body/thread evidence, restart behavior, and manual release QA.".to_string(),
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
            "./scripts/package-distribution.sh --check",
            "./scripts/package-distribution.sh --unsigned-launch-check",
            "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh",
            "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_APPLE_ID='apple-id@example.com' JARVIS_NOTARYTOOL_TEAM_ID='TEAMID1234' JARVIS_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh",
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks",
            "Launch Assemblywright with JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true, then export JARVIS_IPC_TOKEN_FILE as the app-owned ipc-session-auth.json path before external IPC checks",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"",
            "./scripts/release-evidence-doctor.sh --check",
            "cargo run -p jarvis-cli -- release live-device-runbook"
        ],
        "manual_checks": [
            "Configure Developer ID Application and Installer identities plus either a notarytool keychain profile or Apple ID/team/app-specific password credentials on the release Mac.",
            "Run the full package-distribution lane and preserve the signed zip, signed installer package, signed provenance report, and notarytool logs referenced by that report.",
            "Confirm the signed installer package metadata still targets the Assemblywright package identifier, release version, and /Applications install location.",
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
        "Assemblywright signed distribution runbook:".to_string(),
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
        "- ./scripts/package-distribution.sh --check".to_string(),
        "- ./scripts/package-distribution.sh --unsigned-launch-check".to_string(),
        "- JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh".to_string(),
        "- JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_APPLE_ID='apple-id@example.com' JARVIS_NOTARYTOOL_TEAM_ID='TEAMID1234' JARVIS_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh".to_string(),
        "- Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks".to_string(),
        "- Launch Assemblywright with JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true, then export JARVIS_IPC_TOKEN_FILE as the app-owned ipc-session-auth.json path before external IPC checks".to_string(),
        "- JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"".to_string(),
        "- ./scripts/release-evidence-doctor.sh --check".to_string(),
        "- cargo run -p jarvis-cli -- release live-device-runbook".to_string(),
        "Manual checks:".to_string(),
        "- Configure Developer ID Application and Installer identities plus either a notarytool keychain profile or Apple ID/team/app-specific password credentials on the release Mac.".to_string(),
        "- Preserve the signed zip, signed installer package, signed provenance report, and notarytool logs referenced by that report.".to_string(),
        "- Confirm the signed installer package metadata still targets the Assemblywright package identifier, release version, and /Applications install location.".to_string(),
        "- Confirm notarization and stapling for both app and installer before clean-profile installation.".to_string(),
        "- Continue with live-device QA, plugin-trust QA, final evidence bundle generation, and external evidence-mode readiness.".to_string(),
        "Boundary: runbook and local evidence inspection only; no signing, notarization, stapling, Gatekeeper assessment, installation, live-device QA, or plugin-trust QA was performed.".to_string(),
        "Raw JSON: rerun with --json for a structured runbook summary.".to_string(),
    ]);
    Ok(lines.join("\n"))
}

fn release_evidence_bundle_runbook_json(
    readiness_response: &str,
    evidence_status_response: &str,
) -> anyhow::Result<String> {
    let readiness: serde_json::Value = serde_json::from_str(readiness_response)?;
    let evidence_status: serde_json::Value = serde_json::from_str(evidence_status_response)?;
    let child_keys = [
        "signed_distribution_provenance_report",
        "live_device_qa_report",
        "plugin_trust_qa_report",
    ];
    let child_evidence = evidence_status
        .get("items")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("key")
                        .and_then(serde_json::Value::as_str)
                        .map(|key| child_keys.contains(&key))
                        .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let final_bundle_evidence = evidence_status
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("key").and_then(serde_json::Value::as_str)
                    == Some("release_evidence_bundle")
            })
        })
        .cloned();
    let payload = serde_json::json!({
        "generated_from": "release readiness plus evidence-status",
        "production_ready": readiness.get("production_ready").cloned().unwrap_or(serde_json::Value::Bool(false)),
        "child_evidence": child_evidence,
        "final_bundle_evidence": final_bundle_evidence,
        "commands": [
            "./scripts/release-evidence-bundle.sh --check",
            "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env",
            "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle",
            "./scripts/release-evidence-doctor.sh --check",
            "./scripts/release-evidence-doctor.sh --assert-complete",
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks",
            "Launch Assemblywright with JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true, then export JARVIS_IPC_TOKEN_FILE as the app-owned ipc-session-auth.json path before external IPC checks",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"",
            "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
        ],
        "manual_checks": [
            "Generate the final evidence bundle only after signed-distribution, live-device QA, and plugin-trust QA reports exist and have been archived.",
            "Use a durable reports archive URI and preserve the signed zip, installer package, signed provenance report, live-device QA report, plugin-trust QA report, final bundle, and supporting logs.",
            "Confirm release-evidence-doctor --assert-complete reports every required evidence item present before enabling external evidence-mode readiness.",
            "Restart or start the release core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external before the final readiness check.",
            "Confirm production_ready remains false if any required evidence item is missing, invalid, or stale."
        ],
        "proof_boundary": "Runbook and local evidence inspection only; this command does not generate the final bundle, sign, notarize, staple, install, Finder-launch, run live-device QA, perform marketplace review, scan malware, deploy a sandbox, or enforce host-level egress."
    });
    Ok(serde_json::to_string_pretty(&payload)?)
}

fn format_release_evidence_bundle_runbook(
    readiness_response: &str,
    evidence_status_response: &str,
) -> anyhow::Result<String> {
    let readiness: serde_json::Value = serde_json::from_str(readiness_response)?;
    let evidence_status: serde_json::Value = serde_json::from_str(evidence_status_response)?;
    let item_keys = [
        "signed_distribution_provenance_report",
        "live_device_qa_report",
        "plugin_trust_qa_report",
        "release_evidence_bundle",
    ];
    let items = evidence_status
        .get("items")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut lines = vec![
        "Assemblywright final evidence-bundle runbook:".to_string(),
        format!(
            "Production ready: {}",
            readiness
                .get("production_ready")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        ),
        "Final bundle evidence:".to_string(),
    ];
    for key in item_keys {
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
        "- ./scripts/release-evidence-bundle.sh --check".to_string(),
        "- ./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env".to_string(),
        "- set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle".to_string(),
        "- ./scripts/release-evidence-doctor.sh --check".to_string(),
        "- ./scripts/release-evidence-doctor.sh --assert-complete".to_string(),
        "- Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks".to_string(),
        "- Launch Assemblywright with JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true, then export JARVIS_IPC_TOKEN_FILE as the app-owned ipc-session-auth.json path before external IPC checks".to_string(),
        "- JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"".to_string(),
        "- Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external".to_string(),
        "- JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"".to_string(),
        "Manual checks:".to_string(),
        "- Generate the final evidence bundle only after signed-distribution, live-device QA, and plugin-trust QA reports exist and have been archived.".to_string(),
        "- Use a durable reports archive URI and preserve the signed zip, installer package, signed provenance report, live-device QA report, plugin-trust QA report, final bundle, and supporting logs.".to_string(),
        "- Confirm release-evidence-doctor --assert-complete reports every required evidence item present before enabling external evidence-mode readiness.".to_string(),
        "- Restart or start the release core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external before the final readiness check.".to_string(),
        "- Confirm production_ready remains false if any required evidence item is missing, invalid, or stale.".to_string(),
        "Boundary: runbook and local evidence inspection only; no final bundle was generated and no signing, notarization, stapling, installation, Finder launch, live-device QA, marketplace review, malware scan, sandbox deployment, or host-level egress enforcement was performed.".to_string(),
        "Raw JSON: rerun with --json for a structured runbook summary.".to_string(),
    ]);
    Ok(lines.join("\n"))
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
