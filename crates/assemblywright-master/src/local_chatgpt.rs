use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio as StdStdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use uuid::Uuid;

use super::{current_stop, terminate_tree, Stop, ValidatorJob, CONTROL_POLL_MILLISECONDS};

const MAX_REQUEST_BYTES: usize = 512 * 1024;
const MAX_LAUNCH_BYTES: usize = 4 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_CATALOG_BYTES: usize = 1024 * 1024;
const MAX_CATALOG_TOTAL_BYTES: usize = 4 * 1024 * 1024;
const MAX_CATALOG_MODELS: usize = 512;
const CATALOG_TIMEOUT_SECONDS: u64 = 45;
const CHAT_TIMEOUT_SECONDS: u64 = 900;
const MAX_CODEX_EXECUTABLE_BYTES: u64 = 512 * 1024 * 1024;
const LAUNCH_GATE: u8 = 0xA7;
const RESPONSE_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "properties": { "text": { "type": "string", "minLength": 1, "maxLength": 524288 } },
  "required": ["text"]
}"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RoleProvider {
    Local,
    Chatgpt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChatgptModel {
    pub id: String,
    pub name: String,
    pub reasoning_efforts: Vec<String>,
    pub default_reasoning_effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChatgptCatalog {
    pub status: String,
    pub account_type: String,
    pub models: Vec<ChatgptModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChatgptChatRequest {
    pub model: String,
    pub reasoning_effort: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChatgptChatResponse {
    pub status: String,
    pub text: String,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuntimeIdentity {
    pub executable: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum LauncherRequest {
    Catalog {
        runtime: RuntimeIdentity,
    },
    Chat {
        runtime: RuntimeIdentity,
        state_dir: PathBuf,
        model: String,
        reasoning_effort: String,
        prompt: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexText {
    text: String,
}

#[cfg(feature = "local-chatgpt-test-fixture")]
const TEST_FIXTURE_ENV: &str = "ASSEMBLYWRIGHT_CHATGPT_TEST_FIXTURE";

#[cfg(feature = "local-chatgpt-test-fixture")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestFixture {
    catalog: ChatgptCatalog,
    plan_text: String,
    review_text: String,
    chat_text: String,
    #[serde(default)]
    malformed_chat: bool,
    #[serde(default)]
    malformed_stage: Option<String>,
    #[serde(default)]
    delay_stage: Option<String>,
    #[serde(default)]
    delay_ms: u64,
}

pub(crate) async fn catalog(state: &Path) -> Result<ChatgptCatalog> {
    let runtime = runtime_identity()?;
    catalog_with_identity(state, &runtime).await
}

pub(crate) async fn catalog_with_identity(
    state: &Path,
    runtime: &RuntimeIdentity,
) -> Result<ChatgptCatalog> {
    let bytes = invoke_launcher(
        state,
        LauncherRequest::Catalog {
            runtime: runtime.clone(),
        },
        CATALOG_TIMEOUT_SECONDS,
    )
    .await?;
    let catalog: ChatgptCatalog =
        serde_json::from_slice(&bytes).context("ChatGPT catalog response was malformed")?;
    validate_catalog(&catalog)?;
    Ok(catalog)
}

pub(crate) async fn chat(state: &Path, request: ChatgptChatRequest) -> Result<ChatgptChatResponse> {
    validate_chat_request(&request)?;
    let runtime = runtime_identity()?;
    let catalog = catalog_with_identity(state, &runtime).await?;
    require_catalog_selection(&catalog, &request.model, &request.reasoning_effort)?;
    chat_with_validated_selection(state, request, Some(&runtime)).await
}

pub(crate) async fn chat_with_validated_selection(
    state: &Path,
    request: ChatgptChatRequest,
    expected_runtime: Option<&RuntimeIdentity>,
) -> Result<ChatgptChatResponse> {
    validate_chat_request(&request)?;
    let expected_model = request.model.clone();
    let expected_effort = request.reasoning_effort.clone();
    let runtime = match expected_runtime {
        Some(expected) => {
            if &runtime_identity()? != expected {
                bail!("Codex executable identity changed after role selection was frozen")
            }
            expected.clone()
        }
        None => runtime_identity()?,
    };
    let launch = LauncherRequest::Chat {
        runtime,
        state_dir: state.to_path_buf(),
        model: request.model,
        reasoning_effort: request.reasoning_effort,
        prompt: request.prompt,
    };
    let bytes = invoke_launcher(state, launch, CHAT_TIMEOUT_SECONDS).await?;
    let response: ChatgptChatResponse =
        serde_json::from_slice(&bytes).context("ChatGPT response was malformed")?;
    if response.status != "completed"
        || response.provider != "chatgpt"
        || response.model != expected_model
        || response.reasoning_effort != expected_effort
        || response.text.trim().is_empty()
        || response.text.len() > MAX_RESPONSE_BYTES
    {
        bail!("ChatGPT response did not preserve the selected role binding")
    }
    Ok(response)
}

pub(crate) fn require_catalog_selection(
    catalog: &ChatgptCatalog,
    model: &str,
    reasoning_effort: &str,
) -> Result<()> {
    let entry = catalog
        .models
        .iter()
        .find(|entry| entry.id == model)
        .ok_or_else(|| anyhow!("ChatGPT model {model} is not in the authenticated catalog"))?;
    if !entry
        .reasoning_efforts
        .iter()
        .any(|effort| effort == reasoning_effort)
    {
        bail!("ChatGPT model {model} does not advertise reasoning effort {reasoning_effort}")
    }
    Ok(())
}

pub(crate) fn runtime_identity() -> Result<RuntimeIdentity> {
    #[cfg(feature = "local-chatgpt-test-fixture")]
    if fixture_path_from_env(env::var_os(TEST_FIXTURE_ENV)).is_some() {
        return Ok(RuntimeIdentity {
            executable: PathBuf::from("debug-test-fixture"),
            sha256: "0000000000000000000000000000000000000000000000000000000000000000".into(),
        });
    }
    let executable = resolve_codex_executable()?;
    let metadata = fs::metadata(&executable).context("read Codex executable metadata")?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_CODEX_EXECUTABLE_BYTES {
        bail!("Codex executable is not a bounded ordinary file")
    }
    let mut file = fs::File::open(&executable).context("open Codex executable")?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(RuntimeIdentity {
        executable,
        sha256: digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    })
}

fn validate_catalog(catalog: &ChatgptCatalog) -> Result<()> {
    if catalog.status != "ok" || catalog.account_type != "chatgpt" || catalog.models.is_empty() {
        bail!("Codex is not authenticated with a usable ChatGPT model catalog")
    }
    let mut ids = BTreeSet::new();
    for model in &catalog.models {
        if model.id.trim().is_empty()
            || model.name.trim().is_empty()
            || model.id.len() > 128
            || model.name.len() > 256
            || model.reasoning_efforts.is_empty()
            || model.reasoning_efforts.len() > 16
            || !ids.insert(&model.id)
        {
            bail!("ChatGPT catalog contains an invalid model entry")
        }
        let efforts = model.reasoning_efforts.iter().collect::<BTreeSet<_>>();
        if efforts.len() != model.reasoning_efforts.len()
            || !efforts.contains(&model.default_reasoning_effort)
            || model
                .reasoning_efforts
                .iter()
                .any(|effort| !valid_reasoning_effort(effort))
        {
            bail!("ChatGPT catalog contains invalid reasoning metadata")
        }
    }
    Ok(())
}

fn validate_chat_request(request: &ChatgptChatRequest) -> Result<()> {
    if request.model.trim().is_empty() || request.model.len() > 128 {
        bail!("ChatGPT model must be nonempty and at most 128 bytes")
    }
    if !valid_reasoning_effort(&request.reasoning_effort) {
        bail!("ChatGPT reasoning effort is invalid")
    }
    if request.prompt.trim().is_empty() || request.prompt.len() > MAX_REQUEST_BYTES {
        bail!("ChatGPT prompt must be nonempty and at most {MAX_REQUEST_BYTES} bytes")
    }
    Ok(())
}

fn valid_reasoning_effort(value: &str) -> bool {
    matches!(
        value,
        "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
    )
}

async fn invoke_launcher(
    state: &Path,
    request: LauncherRequest,
    timeout_seconds: u64,
) -> Result<Vec<u8>> {
    fs::create_dir_all(state).context("create ChatGPT state directory")?;
    let executable = env::current_exe().context("resolve assemblywright-local executable")?;
    let job = ValidatorJob::new().context("create ChatGPT process containment")?;
    let mut command = Command::new(executable);
    command
        .arg("chatgpt-launcher")
        .stdin(StdStdio::piped())
        .stdout(StdStdio::piped())
        .stderr(StdStdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .context("start contained ChatGPT launcher")?;
    if let Err(error) = job.assign(&child) {
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Err(error).context("assign ChatGPT launcher to Job Object");
    }
    let request = serde_json::to_vec(&request)?;
    if request.len() > MAX_LAUNCH_BYTES {
        terminate_tree(&mut child, &job).await;
        let _ = child.wait().await;
        bail!("ChatGPT launcher request is oversized")
    }
    let delivered = if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&[LAUNCH_GATE]).await.is_ok()
            && stdin.write_all(&request).await.is_ok()
            && stdin.shutdown().await.is_ok()
    } else {
        false
    };
    if !delivered {
        terminate_tree(&mut child, &job).await;
        let _ = child.wait().await;
        bail!("failed to deliver ChatGPT request after process containment")
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_task = tokio::spawn(read_bounded_bytes(stdout, MAX_RESPONSE_BYTES));
    let err_task = tokio::spawn(read_bounded_bytes(stderr, 32 * 1024));
    let deadline = tokio::time::sleep(Duration::from_secs(timeout_seconds));
    tokio::pin!(deadline);
    let status = loop {
        tokio::select! {
            status = child.wait() => break status.context("wait for ChatGPT launcher")?,
            _ = &mut deadline => {
                terminate_tree(&mut child, &job).await;
                let _ = child.wait().await;
                bail!("ChatGPT request timed out")
            },
            _ = tokio::signal::ctrl_c() => {
                terminate_tree(&mut child, &job).await;
                let _ = child.wait().await;
                return Err(anyhow!("ChatGPT request cancelled by console interrupt"));
            },
            _ = tokio::time::sleep(Duration::from_millis(CONTROL_POLL_MILLISECONDS)) => {
                if let Some(stop) = current_stop(state) {
                    terminate_tree(&mut child, &job).await;
                    let _ = child.wait().await;
                    return match stop {
                        Stop::Cancelled => Err(anyhow!("ChatGPT request cancelled by owner")),
                        Stop::Paused => Err(anyhow!("Emergency Pause stopped ChatGPT request")),
                    };
                }
            }
        }
    };
    let stdout = out_task
        .await
        .unwrap_or_else(|_| Err(anyhow!("read ChatGPT stdout")))?;
    let stderr = err_task
        .await
        .unwrap_or_else(|_| Err(anyhow!("read ChatGPT stderr")))?;
    if !job
        .wait_empty(Duration::from_millis(500))
        .await
        .unwrap_or(false)
    {
        let _ = job.terminate();
        bail!("ChatGPT launcher left a descendant process")
    }
    if !status.success() {
        bail!(
            "ChatGPT request failed{}",
            if stderr.is_empty() {
                String::new()
            } else {
                format!(
                    ": {}",
                    String::from_utf8_lossy(&stderr)
                        .chars()
                        .take(512)
                        .collect::<String>()
                )
            }
        )
    }
    if stdout.is_empty() || stdout.len() >= MAX_RESPONSE_BYTES {
        bail!("ChatGPT response was empty or oversized")
    }
    Ok(stdout)
}

async fn read_bounded_bytes<T: tokio::io::AsyncRead + Unpin>(
    stream: Option<T>,
    limit: usize,
) -> Result<Vec<u8>> {
    let mut stream = stream.ok_or_else(|| anyhow!("missing child output stream"))?;
    let mut result = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        if result.len().saturating_add(count) > limit {
            bail!("child output exceeded its limit")
        }
        result.extend_from_slice(&buffer[..count]);
    }
    Ok(result)
}

pub(crate) fn launcher() -> Result<()> {
    let mut input = Vec::new();
    std::io::stdin()
        .take((MAX_LAUNCH_BYTES + 1) as u64)
        .read_to_end(&mut input)?;
    if input.first() != Some(&LAUNCH_GATE) {
        bail!("missing ChatGPT process-containment launch gate")
    }
    let request: LauncherRequest =
        serde_json::from_slice(&input[1..]).context("invalid ChatGPT launcher request")?;
    #[cfg(feature = "local-chatgpt-test-fixture")]
    if let Some(path) = fixture_path_from_env(env::var_os(TEST_FIXTURE_ENV)) {
        return fixture_launcher(&PathBuf::from(path), &request);
    }
    let supplied_runtime = match &request {
        LauncherRequest::Catalog { runtime } | LauncherRequest::Chat { runtime, .. } => runtime,
    };
    if &runtime_identity()? != supplied_runtime {
        bail!("Codex executable identity changed before contained launch")
    }
    let output = match request {
        LauncherRequest::Catalog { runtime } => serde_json::to_vec(&codex_catalog(&runtime)?)?,
        LauncherRequest::Chat {
            runtime,
            state_dir,
            model,
            reasoning_effort,
            prompt,
        } => serde_json::to_vec(&codex_chat(
            &runtime,
            &state_dir,
            &model,
            &reasoning_effort,
            &prompt,
        )?)?,
    };
    std::io::stdout().write_all(&output)?;
    Ok(())
}

#[cfg(feature = "local-chatgpt-test-fixture")]
fn fixture_launcher(path: &Path, request: &LauncherRequest) -> Result<()> {
    let fixture: TestFixture = serde_json::from_slice(&fs::read(path)?)?;
    let stage = match request {
        LauncherRequest::Catalog { .. } => "catalog",
        LauncherRequest::Chat { prompt, .. }
            if prompt.contains("Prepare concise implementation guidance") =>
        {
            "planning"
        }
        LauncherRequest::Chat { prompt, .. }
            if prompt.contains("You are an independent reviewer with no tools") =>
        {
            "review"
        }
        LauncherRequest::Chat { .. } => "chat",
    };
    if fixture.delay_stage.as_deref() == Some(stage) {
        std::thread::sleep(Duration::from_millis(fixture.delay_ms.min(30_000)));
    }
    match request {
        LauncherRequest::Catalog { .. } => {
            validate_catalog(&fixture.catalog)?;
            std::io::stdout().write_all(&serde_json::to_vec(&fixture.catalog)?)?;
        }
        LauncherRequest::Chat {
            model,
            reasoning_effort,
            ..
        } => {
            if fixture.malformed_chat || fixture.malformed_stage.as_deref() == Some(stage) {
                std::io::stdout().write_all(b"{malformed")?;
            } else {
                let text = match stage {
                    "planning" => &fixture.plan_text,
                    "review" => &fixture.review_text,
                    _ => &fixture.chat_text,
                };
                let response = ChatgptChatResponse {
                    status: "completed".into(),
                    text: text.clone(),
                    provider: "chatgpt".into(),
                    model: model.clone(),
                    reasoning_effort: reasoning_effort.clone(),
                };
                std::io::stdout().write_all(&serde_json::to_vec(&response)?)?;
            }
        }
    }
    Ok(())
}

#[cfg(feature = "local-chatgpt-test-fixture")]
fn fixture_path_from_env(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

#[cfg(all(not(feature = "local-chatgpt-test-fixture"), test))]
fn fixture_path_from_env(_value: Option<OsString>) -> Option<OsString> {
    None
}

fn codex_catalog(runtime: &RuntimeIdentity) -> Result<ChatgptCatalog> {
    let executable = &runtime.executable;
    let codex_home = codex_home()?;
    let mut command = StdCommand::new(executable);
    command
        .args(["app-server", "--listen", "stdio://"])
        .env_clear()
        .env("CODEX_HOME", codex_home)
        .stdin(StdStdio::piped())
        .stdout(StdStdio::piped())
        .stderr(StdStdio::null());
    configure_windows_environment(&mut command)?;
    let mut child = command.spawn().context("start Codex app-server")?;
    let mut stdin = child.stdin.take().context("open Codex app-server stdin")?;
    let stdout = child
        .stdout
        .take()
        .context("open Codex app-server stdout")?;
    let mut reader = BufReader::new(stdout);
    rpc_send(
        &mut stdin,
        &json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"assemblywright_local","title":"Assemblywright","version":"0.1.4"}}}),
    )?;
    let mut response_bytes = 0_usize;
    let _ = rpc_read(&mut reader, 1, &mut response_bytes)?;
    rpc_send(&mut stdin, &json!({"method":"initialized","params":{}}))?;
    rpc_send(
        &mut stdin,
        &json!({"id":2,"method":"account/read","params":{"refreshToken":false}}),
    )?;
    let account = rpc_read(&mut reader, 2, &mut response_bytes)?;
    if account.pointer("/account/type").and_then(Value::as_str) != Some("chatgpt") {
        let _ = child.kill();
        bail!("Codex is not signed in with ChatGPT")
    }
    let mut models = Vec::new();
    let mut cursor: Option<String> = None;
    for page_index in 0..10_u64 {
        let id = 10 + page_index;
        let mut params = json!({"limit":100,"includeHidden":false});
        if let Some(value) = &cursor {
            params["cursor"] = Value::String(value.clone());
        }
        rpc_send(
            &mut stdin,
            &json!({"id":id,"method":"model/list","params":params}),
        )?;
        let page = rpc_read(&mut reader, id, &mut response_bytes)?;
        let entries = page
            .get("data")
            .and_then(Value::as_array)
            .context("Codex model/list omitted data")?;
        for entry in entries {
            let id = entry
                .get("model")
                .or_else(|| entry.get("id"))
                .and_then(Value::as_str)
                .context("Codex model/list omitted model id")?;
            let name = entry
                .get("displayName")
                .and_then(Value::as_str)
                .context("Codex model/list omitted display name")?;
            let default = entry
                .get("defaultReasoningEffort")
                .and_then(Value::as_str)
                .context("Codex model/list omitted default reasoning effort")?;
            let effort_values = entry
                .get("supportedReasoningEfforts")
                .and_then(Value::as_array)
                .context("Codex model/list omitted reasoning efforts")?;
            let mut efforts = Vec::new();
            for effort in effort_values {
                efforts.push(
                    effort
                        .get("reasoningEffort")
                        .and_then(Value::as_str)
                        .context("Codex model/list contained malformed reasoning effort")?
                        .to_string(),
                );
            }
            models.push(ChatgptModel {
                id: id.to_string(),
                name: name.to_string(),
                reasoning_efforts: efforts,
                default_reasoning_effort: default.to_string(),
            });
            if models.len() > MAX_CATALOG_MODELS {
                let _ = child.kill();
                bail!("Codex model catalog contains too many entries")
            }
        }
        cursor = page
            .get("nextCursor")
            .and_then(Value::as_str)
            .map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    drop(stdin);
    let _ = child.wait();
    let catalog = ChatgptCatalog {
        status: "ok".into(),
        account_type: "chatgpt".into(),
        models,
    };
    validate_catalog(&catalog)?;
    Ok(catalog)
}

fn rpc_send(writer: &mut impl Write, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > 64 * 1024 {
        bail!("Codex app-server request is oversized")
    }
    writer.write_all(&bytes)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn rpc_read(reader: &mut impl BufRead, expected_id: u64, total_bytes: &mut usize) -> Result<Value> {
    for _ in 0..1000 {
        let mut line = String::new();
        let count = reader.read_line(&mut line)?;
        if count == 0 || count > MAX_CATALOG_BYTES {
            bail!("Codex app-server ended or returned an oversized message")
        }
        *total_bytes = total_bytes
            .checked_add(count)
            .ok_or_else(|| anyhow!("Codex app-server response budget overflowed"))?;
        if *total_bytes > MAX_CATALOG_TOTAL_BYTES {
            bail!("Codex app-server exceeded the cumulative catalog response limit")
        }
        let value: Value =
            serde_json::from_str(&line).context("Codex app-server returned malformed JSON")?;
        if value.get("id").and_then(Value::as_u64) != Some(expected_id) {
            continue;
        }
        if value.get("error").is_some() {
            bail!("Codex app-server rejected the bounded request")
        }
        return value
            .get("result")
            .cloned()
            .context("Codex app-server response omitted result");
    }
    bail!("Codex app-server did not return the expected response")
}

fn codex_chat(
    runtime: &RuntimeIdentity,
    state: &Path,
    model: &str,
    effort: &str,
    prompt: &str,
) -> Result<ChatgptChatResponse> {
    let request = ChatgptChatRequest {
        model: model.to_string(),
        reasoning_effort: effort.to_string(),
        prompt: prompt.to_string(),
    };
    validate_chat_request(&request)?;
    let executable = &runtime.executable;
    let codex_home = codex_home()?;
    let working = state.join("chatgpt").join(Uuid::new_v4().to_string());
    fs::create_dir_all(&working).context("create private ChatGPT request directory")?;
    let working = fs::canonicalize(&working).context("canonicalize ChatGPT request directory")?;
    let schema = working.join("response-schema.json");
    let response_path = working.join("response.json");
    fs::write(&schema, RESPONSE_SCHEMA).context("write ChatGPT response schema")?;
    let mut command = StdCommand::new(executable);
    command
        .args(codex_exec_arguments(
            model,
            effort,
            &schema,
            &response_path,
            &working,
        ))
        .current_dir(&working)
        .env_clear()
        .env("CODEX_HOME", codex_home)
        .stdin(StdStdio::piped())
        .stdout(StdStdio::null())
        .stderr(StdStdio::null());
    configure_windows_environment(&mut command)?;
    let mut child = command
        .spawn()
        .context("start response-only Codex request")?;
    child
        .stdin
        .take()
        .context("open Codex stdin")?
        .write_all(prompt.as_bytes())?;
    let output = child
        .wait_with_output()
        .context("wait for Codex response")?;
    let response_bytes = fs::read(&response_path).unwrap_or_default();
    let _ = fs::remove_dir_all(&working);
    if !output.status.success()
        || response_bytes.is_empty()
        || response_bytes.len() > MAX_RESPONSE_BYTES
    {
        bail!("Codex did not complete the bounded response-only request")
    }
    let body: CodexText = serde_json::from_slice(&response_bytes)
        .context("Codex returned malformed structured output")?;
    if body.text.trim().is_empty() || body.text.len() > MAX_RESPONSE_BYTES {
        bail!("Codex returned an empty or oversized response")
    }
    Ok(ChatgptChatResponse {
        status: "completed".into(),
        text: body.text,
        provider: "chatgpt".into(),
        model: model.into(),
        reasoning_effort: effort.into(),
    })
}

fn codex_exec_arguments(
    model: &str,
    effort: &str,
    schema: &Path,
    response: &Path,
    working: &Path,
) -> Vec<OsString> {
    [
        "exec",
        "--strict-config",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--skip-git-repo-check",
        "--sandbox",
        "read-only",
        "--model",
    ]
    .into_iter()
    .map(OsString::from)
    .chain(std::iter::once(model.into()))
    .chain(
        [
            "--config",
            &format!("model_reasoning_effort=\"{effort}\""),
            "--config",
            "model_reasoning_summary=\"none\"",
            "--config",
            "model_verbosity=\"low\"",
            "--config",
            "project_doc_max_bytes=0",
            "--config",
            "features.shell_tool=false",
            "--config",
            "features.shell_snapshot=false",
            "--config",
            "features.skill_mcp_dependency_install=false",
            "--config",
            "features.skill_search=false",
            "--config",
            "features.plugins=false",
            "--config",
            "features.plugin_sharing=false",
            "--config",
            "features.remote_plugin=false",
            "--config",
            "features.multi_agent=false",
            "--config",
            "features.apps=false",
            "--config",
            "features.browser_use=false",
            "--config",
            "features.browser_use_external=false",
            "--config",
            "features.browser_use_full_cdp_access=false",
            "--config",
            "features.in_app_browser=false",
            "--config",
            "features.computer_use=false",
            "--config",
            "features.image_generation=false",
            "--config",
            "features.view_image=false",
            "--config",
            "features.hooks=false",
            "--config",
            "features.unified_exec=false",
            "--config",
            "features.code_mode_host=false",
            "--config",
            "features.goals=false",
            "--config",
            "features.tool_suggest=false",
            "--config",
            "features.tool_call_mcp_elicitation=false",
            "--config",
            "skills.include_instructions=false",
            "--config",
            "skills.bundled.enabled=false",
            "--config",
            "web_search=\"disabled\"",
            "--config",
            "tools.web_search=false",
            "--output-schema",
        ]
        .into_iter()
        .map(OsString::from),
    )
    .chain(std::iter::once(schema.as_os_str().to_owned()))
    .chain([
        OsString::from("--output-last-message"),
        response.as_os_str().to_owned(),
    ])
    .chain([
        OsString::from("--cd"),
        working.as_os_str().to_owned(),
        OsString::from("-"),
    ])
    .collect()
}

fn codex_home() -> Result<PathBuf> {
    let profile = env::var_os("USERPROFILE").context("USERPROFILE is unavailable")?;
    let home = PathBuf::from(profile).join(".codex");
    if !home.is_absolute() || !home.is_dir() {
        bail!("Codex authentication home is unavailable")
    }
    Ok(home)
}

fn resolve_codex_executable() -> Result<PathBuf> {
    let path = env::var_os("PATH").context("PATH is unavailable")?;
    for directory in env::split_paths(&path) {
        let candidate = directory.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        if candidate.is_file() {
            return fs::canonicalize(candidate).context("canonicalize Codex executable");
        }
    }
    bail!("the authenticated native Codex executable was not found")
}

#[cfg(windows)]
fn configure_windows_environment(command: &mut StdCommand) -> Result<()> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::{
        GetSystemDirectoryW, GetWindowsDirectoryW,
    };
    let mut root = [0_u16; 32_768];
    let mut system = [0_u16; 32_768];
    let root_len = unsafe { GetWindowsDirectoryW(root.as_mut_ptr(), root.len() as u32) } as usize;
    let system_len =
        unsafe { GetSystemDirectoryW(system.as_mut_ptr(), system.len() as u32) } as usize;
    if root_len == 0 || root_len >= root.len() || system_len == 0 || system_len >= system.len() {
        bail!("Windows system directories are unavailable")
    }
    command
        .env("SystemRoot", OsString::from_wide(&root[..root_len]))
        .env("PATH", OsString::from_wide(&system[..system_len]));
    Ok(())
}

#[cfg(not(windows))]
fn configure_windows_environment(_command: &mut StdCommand) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_selection_is_exact_and_effort_bound() {
        let catalog = ChatgptCatalog {
            status: "ok".into(),
            account_type: "chatgpt".into(),
            models: vec![ChatgptModel {
                id: "gpt-test".into(),
                name: "GPT Test".into(),
                reasoning_efforts: vec!["low".into(), "high".into()],
                default_reasoning_effort: "low".into(),
            }],
        };
        validate_catalog(&catalog).unwrap();
        require_catalog_selection(&catalog, "gpt-test", "high").unwrap();
        assert!(require_catalog_selection(&catalog, "gpt-other", "high").is_err());
        assert!(require_catalog_selection(&catalog, "gpt-test", "medium").is_err());
    }

    #[test]
    fn strict_request_rejects_extra_fields() {
        let result = serde_json::from_str::<ChatgptChatRequest>(
            r#"{"model":"gpt-test","reasoning_effort":"high","prompt":"hi","extra":true}"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn codex_exec_has_no_prompt_and_disables_tool_surfaces() {
        let args = codex_exec_arguments(
            "gpt-test",
            "high",
            Path::new("C:/state/schema.json"),
            Path::new("C:/state/response.json"),
            Path::new("C:/state/request"),
        );
        let args = args
            .iter()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>();
        assert_eq!(args.last().map(|value| value.as_ref()), Some("-"));
        assert!(args
            .iter()
            .any(|value| value == "features.shell_tool=false"));
        assert!(args.iter().any(|value| value == "features.plugins=false"));
        assert!(args.iter().any(|value| value == "features.apps=false"));
        assert!(args.iter().any(|value| value == "web_search=\"disabled\""));
        assert!(!args.iter().any(|value| value.contains("user prompt")));
    }

    #[cfg(not(feature = "local-chatgpt-test-fixture"))]
    #[test]
    fn fixture_environment_is_inert_in_default_build() {
        assert!(fixture_path_from_env(Some(OsString::from("forged.json"))).is_none());
    }
}
