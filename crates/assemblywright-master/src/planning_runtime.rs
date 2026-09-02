use crate::{
    BrainstormingAdapter, BrainstormingAdapterBinding, BrainstormingAdapterError,
    BrainstormingCloudAuthorization, BrainstormingDraft, GithubRepositoryCreationAdapter,
    GithubRepositoryCreationError, GithubRepositoryObservation, MasterError, MasterKernel,
    PlanningEffectControl, WindowsPlanningEffectAuthority,
};
use assemblywright_protocol::{
    AssemblyLineRepositoryIdentity, BrainstormingOwnerApprovalBinding,
    BrainstormingSpecificationDocument, OrchestratorProfile, ProjectVisibility,
    RepositoryCreationProjection, MAX_BRAINSTORMING_SPECIFICATION_BYTES,
};
#[cfg(windows)]
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
#[cfg(not(windows))]
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
#[cfg(any(windows, test))]
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
#[cfg(any(windows, test))]
use zeroize::Zeroize;

#[cfg(windows)]
mod windows_containment;

const ROOT: &str = "planning-runtime";
#[cfg(not(windows))]
const MASTER_CONFIG: &str = "runtime.json";
#[cfg(windows)]
const MASTER_CONFIG: &str = "runtime-v4.json";
#[cfg(windows)]
const PROVIDER_CONFIG: &str = "runtime.json";
const BRAINSTORMING_PROVIDER: &str = "brainstorming-provider";
const CODEX: &str = "codex";
const OUTPUT_SCHEMA: &str = "brainstorming-output-schema.json";
const CODEX_HOME: &str = "codex-home";
const RECONCILIATION: &str = "reconciliation";
const TEMP_DIRECTORY: &str = "temp";
const LOCAL_APP_DATA_DIRECTORY: &str = "local-app-data";
const GH: &str = "gh";
const GH_CONFIG: &str = "gh-config";
const HOSTS: &str = "hosts.yml";
#[cfg(windows)]
const PROVIDER_ROOT: &str = "provider";
#[cfg(windows)]
const GITHUB_ROOT: &str = "github";
const MAX_CONFIG_BYTES: usize = 16 * 1024;
const MAX_GH_OUTPUT_BYTES: usize = 128 * 1024;
const MAX_PROVIDER_OUTPUT_BYTES: usize = MAX_BRAINSTORMING_SPECIFICATION_BYTES + 4 * 1024;
const PROVIDER_ID: &str = "openai.codex";
const MODEL_ID: &str = "gpt-5.6-sol";
#[cfg(any(windows, test))]
const FILE_BACKED_CODEX_AUTH_CONFIG: &str = "cli_auth_credentials_store=\"file\"";
#[cfg(windows)]
const NATIVE_PROVIDER_PROBE_PROMPT: &str = "Return one public JSON planning specification for the fixed Assemblywright native Windows planning-containment diagnostic. Use the title 'Assemblywright native planning containment probe', describe only the diagnostic outcome, include one acceptance criterion that the structured response was produced without tools, and include one obligation that this diagnostic grants no repository-creation or execution authority.";
#[cfg(windows)]
const NATIVE_PROVIDER_PROBE_MAX_OUTPUT_BYTES: usize = 16 * 1024;
#[cfg(windows)]
const NATIVE_PROVIDER_PROBE_DEADLINE: Duration = Duration::from_secs(300);
#[cfg(any(windows, test))]
const NATIVE_PROVIDER_PROBE_LOGIN_STDERR_MAX_BYTES: usize = 2 * 1024;
#[cfg(any(windows, test))]
const NATIVE_PROVIDER_PROBE_EXEC_STDERR_MAX_BYTES: usize = 4 * 1024;

pub const PLANNING_EFFECT_DEADLINE: Duration = Duration::from_secs(120);

#[derive(Debug, thiserror::Error)]
pub enum PlanningRuntimeConfigError {
    #[error("planning runtime configuration is incomplete or invalid")]
    Invalid,
    #[cfg(windows)]
    #[error("planning runtime trust boundary rejected: {0}")]
    Boundary(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Master(#[from] MasterError),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u16,
    enabled: bool,
    catalog_revision: u64,
    provider_id: String,
    model_id: String,
    adapter_kind: String,
    brainstorming_provider_sha256: String,
    codex_executable_sha256: String,
    output_schema_sha256: String,
    gh_executable_sha256: String,
    github_owner: String,
    #[cfg(windows)]
    #[serde(default)]
    provider_profile_name: Option<String>,
    #[cfg(windows)]
    #[serde(default)]
    provider_profile_sid: Option<String>,
    #[cfg(windows)]
    #[serde(default)]
    github_profile_name: Option<String>,
    #[cfg(windows)]
    #[serde(default)]
    github_profile_sid: Option<String>,
    #[cfg(windows)]
    #[serde(default)]
    provisioning_owner_sid: Option<String>,
    #[cfg(windows)]
    #[serde(default)]
    runtime_instance: Option<String>,
}

fn valid_config_schema(schema_version: u16) -> bool {
    schema_version == if cfg!(windows) { 4 } else { 1 }
}

#[derive(Debug, Clone)]
struct Executable {
    path: PathBuf,
    sha256: [u8; 32],
    length: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct PlanningRuntimeStatus {
    pub binding_revision: u64,
    pub brainstorming_sha256: [u8; 32],
    pub github_sha256: [u8; 32],
    pub catalog_sha256: [u8; 32],
}

pub struct PlanningRuntime {
    authority: WindowsPlanningEffectAuthority,
    brainstorming: ProcessBrainstormingAdapter,
    github: ProcessGithubCreationAdapter,
    status: PlanningRuntimeStatus,
}

#[cfg(windows)]
#[derive(Debug, Serialize)]
pub struct PlanningProviderNativeProbeReceipt {
    schema_version: u16,
    receipt_domain: &'static str,
    status: &'static str,
    outcome: &'static str,
    service_name_sha256: String,
    service_runtime_binding_sha256: String,
    binding_revision: u64,
    provider_profile_name: String,
    provider_profile_sid: String,
    service_account_sid: String,
    current_token_sid: String,
    provisioning_owner_sid: String,
    health_endpoint: String,
    brainstorming_binding_sha256: String,
    catalog_sha256: String,
    codex_executable_sha256: String,
    output_schema_sha256: String,
    probe_contract_sha256: String,
    login_exit_code: Option<u32>,
    login_diagnostic_code: Option<u32>,
    exec_exit_code: Option<u32>,
    exec_diagnostic_code: Option<u32>,
    output_sha256: Option<String>,
    live_evidence_required: bool,
}

#[derive(Clone)]
struct ProcessIsolation {
    #[cfg(windows)]
    profile: windows_containment::ProfileBinding,
}

#[cfg(not(windows))]
impl ProcessIsolation {
    fn unrestricted_test_runtime() -> Self {
        Self {}
    }

    fn revalidate(&self) -> Result<(), CommandError> {
        Ok(())
    }
}

impl PlanningRuntime {
    pub fn require_feature_enqueue_provenance(
        &self,
        kernel: &MasterKernel,
        approval: &BrainstormingOwnerApprovalBinding,
    ) -> Result<(), MasterError> {
        self.authority
            .require_feature_enqueue_provenance(kernel, approval)
    }

    pub fn approve_feature_and_enqueue(
        &self,
        kernel: &mut MasterKernel,
        approval: &BrainstormingOwnerApprovalBinding,
        now_ms: u64,
    ) -> Result<assemblywright_protocol::FeatureQueueEntryProjection, MasterError> {
        self.authority
            .approve_feature_and_enqueue(kernel, approval, now_ms)
    }

    pub fn load(data_dir: &Path) -> Result<Option<Self>, PlanningRuntimeConfigError> {
        let locator_root = data_dir.join(ROOT);
        if !locator_root.exists() {
            return Ok(None);
        }
        reject_link(&locator_root)?;
        let locator_root = fs::canonicalize(locator_root)?;
        let config_path = locator_root.join(MASTER_CONFIG);
        reject_link(&config_path)?;
        if config_path.parent() != Some(locator_root.as_path())
            || !fs::metadata(&config_path)?.is_file()
        {
            return Err(PlanningRuntimeConfigError::Invalid);
        }
        validate_private(&locator_root, true)?;
        validate_private(&config_path, false)?;
        let config_bytes = fs::read(&config_path)?;
        if config_bytes.is_empty() || config_bytes.len() > MAX_CONFIG_BYTES {
            return Err(PlanningRuntimeConfigError::Invalid);
        }
        let config: Config = serde_json::from_slice(&config_bytes)?;
        if !valid_config_schema(config.schema_version)
            || !config.enabled
            || config.catalog_revision != 1
            || config.provider_id != PROVIDER_ID
            || config.model_id != MODEL_ID
            || config.adapter_kind != "codex_exec_v1"
            || !valid_github_owner(&config.github_owner)
        {
            return Err(PlanningRuntimeConfigError::Invalid);
        }
        #[cfg(windows)]
        let root = windows_containment::canonical_runtime_root(
            config
                .runtime_instance
                .as_deref()
                .ok_or(PlanningRuntimeConfigError::Invalid)?,
        )
        .map_err(|reason| PlanningRuntimeConfigError::Boundary(reason.code()))?;
        #[cfg(not(windows))]
        let root = locator_root.clone();
        #[cfg(windows)]
        let provider_root = root.join(PROVIDER_ROOT);
        #[cfg(not(windows))]
        let provider_root = root.clone();
        #[cfg(windows)]
        let github_root = root.join(GITHUB_ROOT);
        #[cfg(not(windows))]
        let github_root = root.clone();
        let provider_path = provider_root.join(if cfg!(windows) {
            format!("{BRAINSTORMING_PROVIDER}.exe")
        } else {
            BRAINSTORMING_PROVIDER.to_string()
        });
        let gh_path = github_root.join(if cfg!(windows) {
            format!("{GH}.exe")
        } else {
            GH.to_string()
        });
        let gh_config = github_root.join(GH_CONFIG);
        let codex_path = provider_root.join(if cfg!(windows) {
            format!("{CODEX}.exe")
        } else {
            CODEX.to_string()
        });
        let output_schema = provider_root.join(OUTPUT_SCHEMA);
        #[cfg(windows)]
        let provider_config = provider_root.join(PROVIDER_CONFIG);
        let codex_home = provider_root.join(CODEX_HOME);
        let reconciliation = provider_root.join(RECONCILIATION);
        let temporary = provider_root.join(TEMP_DIRECTORY);
        let local_app_data = provider_root.join(LOCAL_APP_DATA_DIRECTORY);
        for path in [
            &provider_path,
            &codex_path,
            &output_schema,
            &codex_home,
            &reconciliation,
            &temporary,
            &local_app_data,
            &gh_path,
            &gh_config,
        ] {
            reject_link(path)?;
        }
        #[cfg(windows)]
        reject_link(&provider_config)?;
        if provider_path.parent() != Some(provider_root.as_path())
            || gh_path.parent() != Some(github_root.as_path())
            || gh_config.parent() != Some(github_root.as_path())
            || codex_path.parent() != Some(provider_root.as_path())
            || output_schema.parent() != Some(provider_root.as_path())
            || codex_home.parent() != Some(provider_root.as_path())
            || reconciliation.parent() != Some(provider_root.as_path())
            || temporary.parent() != Some(provider_root.as_path())
            || local_app_data.parent() != Some(provider_root.as_path())
            || !fs::metadata(&config_path)?.is_file()
            || !fs::metadata(&provider_path)?.is_file()
            || !fs::metadata(&gh_path)?.is_file()
            || !fs::metadata(&gh_config)?.is_dir()
            || !fs::metadata(&codex_path)?.is_file()
            || !fs::metadata(&output_schema)?.is_file()
            || !fs::metadata(&codex_home)?.is_dir()
            || !fs::metadata(&reconciliation)?.is_dir()
            || !fs::metadata(&temporary)?.is_dir()
            || !fs::metadata(&local_app_data)?.is_dir()
        {
            return Err(PlanningRuntimeConfigError::Invalid);
        }
        #[cfg(windows)]
        if !fs::metadata(&provider_config)?.is_file() {
            return Err(PlanningRuntimeConfigError::Invalid);
        }
        validate_private(&root, true)?;
        validate_private(&gh_config, true)?;
        validate_private(&codex_home, true)?;
        validate_private(&reconciliation, true)?;
        validate_private(&temporary, true)?;
        validate_private(&local_app_data, true)?;
        validate_private(&output_schema, false)?;
        let hosts = gh_config.join(HOSTS);
        reject_link(&hosts)?;
        validate_private(&hosts, false)?;
        let hosts_metadata = fs::metadata(&hosts)?;
        if !hosts_metadata.is_file()
            || hosts_metadata.len() == 0
            || hosts_metadata.len() > MAX_CONFIG_BYTES as u64
        {
            return Err(PlanningRuntimeConfigError::Invalid);
        }
        #[cfg(windows)]
        let (provider_isolation, github_isolation) =
            windows_containment::validate_provisioning(windows_containment::Provisioning {
                data_root: data_dir,
                locator_root: &locator_root,
                planning_root: &root,
                config_path: &config_path,
                provider_root: &provider_root,
                provider_paths: &[
                    &provider_path,
                    &codex_path,
                    &output_schema,
                    &provider_config,
                    &codex_home,
                    &reconciliation,
                    &temporary,
                    &local_app_data,
                ],
                github_root: &github_root,
                github_paths: &[&gh_path, &gh_config, &hosts],
                provider_profile_name: config
                    .provider_profile_name
                    .as_deref()
                    .ok_or(PlanningRuntimeConfigError::Invalid)?,
                provider_profile_sid: config
                    .provider_profile_sid
                    .as_deref()
                    .ok_or(PlanningRuntimeConfigError::Invalid)?,
                github_profile_name: config
                    .github_profile_name
                    .as_deref()
                    .ok_or(PlanningRuntimeConfigError::Invalid)?,
                github_profile_sid: config
                    .github_profile_sid
                    .as_deref()
                    .ok_or(PlanningRuntimeConfigError::Invalid)?,
                provisioning_owner_sid: config
                    .provisioning_owner_sid
                    .as_deref()
                    .ok_or(PlanningRuntimeConfigError::Invalid)?,
            })
            .map_err(|reason| PlanningRuntimeConfigError::Boundary(reason.code()))?;
        #[cfg(not(windows))]
        let (provider_isolation, github_isolation) = (
            ProcessIsolation::unrestricted_test_runtime(),
            ProcessIsolation::unrestricted_test_runtime(),
        );
        let provider = load_executable(&provider_path, &config.brainstorming_provider_sha256)?;
        let codex = load_executable(&codex_path, &config.codex_executable_sha256)?;
        let schema_sha256 = load_trusted_file(&output_schema, &config.output_schema_sha256)?;
        let gh = load_executable(&gh_path, &config.gh_executable_sha256)?;
        let profile = OrchestratorProfile::default();
        let brainstorming_sha256: [u8; 32] = Sha256::digest(
            [
                b"assemblywright.brainstorming-runtime.v1\0".as_slice(),
                provider.sha256.as_slice(),
                codex.sha256.as_slice(),
                schema_sha256.as_slice(),
            ]
            .concat(),
        )
        .into();
        let brainstorming_binding = BrainstormingAdapterBinding {
            profile,
            executable_sha256: brainstorming_sha256,
        };
        let github_sha256: [u8; 32] = Sha256::digest(
            [
                b"assemblywright.github-creation.v1\0".as_slice(),
                gh.sha256.as_slice(),
                config.github_owner.as_bytes(),
            ]
            .concat(),
        )
        .into();
        let authority = WindowsPlanningEffectAuthority::from_loaded_bindings(
            config.catalog_revision,
            brainstorming_binding.clone(),
            github_sha256,
        )?;
        let status = PlanningRuntimeStatus {
            binding_revision: config.catalog_revision,
            brainstorming_sha256,
            github_sha256,
            catalog_sha256: authority.catalog_sha256(),
        };
        Ok(Some(Self {
            authority,
            brainstorming: ProcessBrainstormingAdapter {
                root: provider_root,
                executable: provider,
                codex,
                schema_path: output_schema,
                schema_sha256,
                codex_home,
                reconciliation,
                temporary,
                local_app_data,
                binding: brainstorming_binding,
                isolation: provider_isolation,
            },
            github: ProcessGithubCreationAdapter {
                root: github_root,
                executable: gh,
                config_dir: gh_config,
                owner: config.github_owner,
                binding_sha256: github_sha256,
                isolation: github_isolation,
            },
            status,
        }))
    }

    pub fn status(&self) -> PlanningRuntimeStatus {
        self.status
    }

    #[cfg(windows)]
    pub fn run_provider_native_probe(
        &self,
        data_dir: &Path,
        service_name: &str,
    ) -> Result<PlanningProviderNativeProbeReceipt, PlanningRuntimeConfigError> {
        if self.validated_status().is_none() {
            return Err(PlanningRuntimeConfigError::Boundary(
                "native_probe_boundary",
            ));
        }
        let data_dir = fs::canonicalize(data_dir)?;
        let owner_lock_path = data_dir.join("master.owner.lock");
        reject_link(&owner_lock_path)?;
        let owner_lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&owner_lock_path)?;
        owner_lock
            .try_lock_exclusive()
            .map_err(|_| PlanningRuntimeConfigError::Boundary("native_probe_authority"))?;
        let _owner_lock = owner_lock;
        let database_path = data_dir.join("master.sqlite3");
        reject_link(&database_path)?;
        let kernel = MasterKernel::open_planning_runtime_connection(&database_path)?;
        let (paused, expected_pause_revision) = kernel.planning_effect_pause_snapshot()?;
        if paused {
            return Err(PlanningRuntimeConfigError::Boundary(
                "native_probe_authority",
            ));
        }
        let authority = Arc::new(
            windows_containment::NativeProbeAuthority::open(
                service_name,
                &data_dir,
                &self.brainstorming.isolation.profile,
            )
            .map_err(|reason| PlanningRuntimeConfigError::Boundary(reason.code()))?,
        );
        let bindings = authority.bindings();
        let authority_check = Arc::clone(&authority);
        let authority_failure = Arc::new(AtomicU8::new(0));
        let authority_failure_check = Arc::clone(&authority_failure);
        let continuous_authority: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
            let failed = |stage| {
                let _ = authority_failure_check.compare_exchange(
                    0,
                    stage,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                false
            };
            if let Err(stage) = authority_check.revalidate_service() {
                return failed(10 + stage);
            }
            let kernel = match MasterKernel::open_planning_runtime_connection(&database_path) {
                Ok(kernel) => kernel,
                Err(_) => return failed(2),
            };
            match kernel.planning_effect_pause_snapshot() {
                Ok((paused, revision)) if !paused && revision == expected_pause_revision => true,
                Ok(_) => failed(3),
                Err(_) => failed(4),
            }
        });
        let control = PlanningEffectControl::new(
            Arc::new(AtomicBool::new(false)),
            std::time::Instant::now() + NATIVE_PROVIDER_PROBE_DEADLINE,
        )
        .with_authority(continuous_authority);
        let control_diagnostic_code = || {
            let stage = authority_failure.load(Ordering::Acquire);
            if stage == 0 {
                1030
            } else {
                1030 + u32::from(stage)
            }
        };
        if !control.poll() {
            return Err(PlanningRuntimeConfigError::Boundary(
                "native_probe_authority",
            ));
        }
        let schema_path =
            self.brainstorming
                .schema_path
                .to_str()
                .ok_or(PlanningRuntimeConfigError::Boundary(
                    "native_probe_boundary",
                ))?;
        let codex_home_environment =
            windows_containment::codex_windows_environment_path(&self.brainstorming.codex_home);
        let local_app_data_environment =
            windows_containment::codex_windows_environment_path(&self.brainstorming.local_app_data);
        let temporary_environment =
            windows_containment::codex_windows_environment_path(&self.brainstorming.temporary);
        let environment = [
            (OsStr::new("CODEX_HOME"), codex_home_environment.as_os_str()),
            (
                OsStr::new("LOCALAPPDATA"),
                local_app_data_environment.as_os_str(),
            ),
            (OsStr::new("TEMP"), temporary_environment.as_os_str()),
            (OsStr::new("TMP"), temporary_environment.as_os_str()),
        ];
        let probe_contract_sha256: [u8; 32] = Sha256::digest(
            [
                b"assemblywright.windows-native-provider-probe.v1\0".as_slice(),
                NATIVE_PROVIDER_PROBE_PROMPT.as_bytes(),
                self.brainstorming.schema_sha256.as_slice(),
            ]
            .concat(),
        )
        .into();
        let base = |outcome,
                    login_diagnostic: Option<NativeProbeLoginDiagnostic>,
                    login: ProbePhaseOutcome,
                    exec: ProbePhaseOutcome,
                    output_sha256: Option<[u8; 32]>| {
            PlanningProviderNativeProbeReceipt {
                schema_version: 1,
                receipt_domain: "assemblywright.windows-planning-real-codex-probe.v1",
                status: "planning_provider_native_probe",
                outcome,
                service_name_sha256: hex(&bindings.service_name_sha256),
                service_runtime_binding_sha256: hex(&bindings.service_runtime_binding_sha256),
                binding_revision: self.status.binding_revision,
                provider_profile_name: self.brainstorming.isolation.profile.name().to_string(),
                provider_profile_sid: self.brainstorming.isolation.profile.sid().to_string(),
                service_account_sid: bindings.service_account_sid.clone(),
                current_token_sid: bindings.current_token_sid.clone(),
                provisioning_owner_sid: bindings.provisioning_owner_sid.clone(),
                health_endpoint: bindings.health_endpoint.to_string(),
                brainstorming_binding_sha256: hex(&self.status.brainstorming_sha256),
                catalog_sha256: hex(&self.status.catalog_sha256),
                codex_executable_sha256: hex(&self.brainstorming.codex.sha256),
                output_schema_sha256: hex(&self.brainstorming.schema_sha256),
                probe_contract_sha256: hex(&probe_contract_sha256),
                login_exit_code: probe_phase_exit_code(login),
                login_diagnostic_code: login_diagnostic
                    .map(NativeProbeLoginDiagnostic::code)
                    .or_else(|| probe_phase_diagnostic_code(login)),
                exec_exit_code: probe_phase_exit_code(exec),
                exec_diagnostic_code: probe_phase_diagnostic_code(exec),
                output_sha256: output_sha256.map(|value| hex(&value)),
                live_evidence_required: true,
            }
        };

        let login_args = native_provider_probe_login_args();
        let login_stderr = Arc::new(Mutex::new(Vec::new()));
        let login = run_command(
            &self.brainstorming.codex,
            &control,
            CommandInvocation {
                current_dir: &self.brainstorming.root,
                args: &login_args,
                environment: &environment,
                input: &[],
                max_output: 4 * 1024,
                isolation: &self.brainstorming.isolation,
                stderr: CommandStderrMode::CaptureBounded {
                    max_bytes: NATIVE_PROVIDER_PROBE_LOGIN_STDERR_MAX_BYTES,
                    output: Arc::clone(&login_stderr),
                },
            },
        );
        let login_diagnostic = native_probe_login_diagnostic(&login_stderr);
        if !native_probe_post_command_authority_current(
            self.validated_status().is_some(),
            control.poll(),
        ) {
            return Err(PlanningRuntimeConfigError::Boundary(
                "native_probe_authority",
            ));
        }
        let (login_phase, login_diagnostic_evidence) = match login {
            Err(CommandError::Cancelled) => {
                let mut receipt = base(
                    "cancelled_during_login",
                    None,
                    ProbePhaseOutcome::Error(CommandError::Cancelled),
                    ProbePhaseOutcome::NotRun,
                    None,
                );
                receipt.login_diagnostic_code = Some(control_diagnostic_code());
                return Ok(receipt);
            }
            Err(error)
                if login_diagnostic == NativeProbeLoginDiagnostic::ConfigurationLoadFailed =>
            {
                (
                    ProbePhaseOutcome::Error(error),
                    Some(NativeProbeLoginDiagnostic::ConfigurationLoadFailed),
                )
            }
            Err(error) => {
                return Ok(base(
                    "login_failed",
                    Some(login_diagnostic),
                    ProbePhaseOutcome::Error(error),
                    ProbePhaseOutcome::NotRun,
                    None,
                ));
            }
            Ok(_) => (ProbePhaseOutcome::Success, None),
        };
        if self.validated_status().is_none() {
            return Err(PlanningRuntimeConfigError::Boundary(
                "native_probe_boundary",
            ));
        }
        if !control.poll() {
            return Ok(base(
                "cancelled_before_exec",
                login_diagnostic_evidence,
                login_phase,
                ProbePhaseOutcome::NotRun,
                None,
            ));
        }

        let exec_args = native_provider_probe_exec_args(schema_path);
        let exec_stderr = Arc::new(Mutex::new(Vec::new()));
        let exec = run_command(
            &self.brainstorming.codex,
            &control,
            CommandInvocation {
                current_dir: &self.brainstorming.root,
                args: &exec_args,
                environment: &environment,
                input: NATIVE_PROVIDER_PROBE_PROMPT.as_bytes(),
                max_output: NATIVE_PROVIDER_PROBE_MAX_OUTPUT_BYTES,
                isolation: &self.brainstorming.isolation,
                stderr: CommandStderrMode::CaptureBounded {
                    max_bytes: NATIVE_PROVIDER_PROBE_EXEC_STDERR_MAX_BYTES,
                    output: Arc::clone(&exec_stderr),
                },
            },
        );
        let exec_diagnostic = native_probe_exec_diagnostic(&exec_stderr);
        if !native_probe_post_command_authority_current(
            self.validated_status().is_some(),
            control.poll(),
        ) {
            return Err(PlanningRuntimeConfigError::Boundary(
                "native_probe_authority",
            ));
        }
        match exec {
            Err(CommandError::Cancelled) => {
                let mut receipt = base(
                    "cancelled_during_exec",
                    login_diagnostic_evidence,
                    login_phase,
                    ProbePhaseOutcome::Error(CommandError::Cancelled),
                    None,
                );
                receipt.exec_diagnostic_code = Some(control_diagnostic_code());
                Ok(receipt)
            }
            Err(error) => {
                let mut receipt = base(
                    "exec_failed",
                    login_diagnostic_evidence,
                    login_phase,
                    ProbePhaseOutcome::Error(error),
                    None,
                );
                if let Some(diagnostic) = exec_diagnostic {
                    receipt.exec_diagnostic_code = Some(diagnostic.code());
                }
                Ok(receipt)
            }
            Ok(output) => {
                let output_sha256 = <[u8; 32]>::from(Sha256::digest(&output));
                let specification = strict_decode::<BrainstormingSpecificationDocument>(&output)
                    .and_then(|value| value.validate().map_err(|_| ()));
                if specification.is_err() {
                    return Ok(base(
                        "structured_output_rejected",
                        login_diagnostic_evidence,
                        login_phase,
                        ProbePhaseOutcome::RejectedOutput,
                        Some(output_sha256),
                    ));
                }
                Ok(base(
                    "succeeded",
                    login_diagnostic_evidence,
                    login_phase,
                    ProbePhaseOutcome::Success,
                    Some(output_sha256),
                ))
            }
        }
    }

    pub fn validated_status(&self) -> Option<PlanningRuntimeStatus> {
        verify_executable(&self.brainstorming.executable).ok()?;
        verify_executable(&self.brainstorming.codex).ok()?;
        verify_trusted_file(
            &self.brainstorming.schema_path,
            self.brainstorming.schema_sha256,
        )
        .ok()?;
        validate_private(&self.brainstorming.codex_home, true).ok()?;
        validate_private(&self.brainstorming.reconciliation, true).ok()?;
        validate_private(&self.brainstorming.temporary, true).ok()?;
        validate_private(&self.brainstorming.local_app_data, true).ok()?;
        verify_executable(&self.github.executable).ok()?;
        validate_private(&self.brainstorming.root, true).ok()?;
        validate_private(&self.github.config_dir, true).ok()?;
        self.brainstorming.isolation.revalidate().ok()?;
        self.github.isolation.revalidate().ok()?;
        Some(self.status)
    }

    pub fn run_brainstorming(
        &mut self,
        kernel: &mut crate::MasterKernel,
        draft: BrainstormingDraft,
        authorization: BrainstormingCloudAuthorization,
        control: &PlanningEffectControl,
    ) -> Result<assemblywright_protocol::FrozenBrainstormingSpecification, MasterError> {
        crate::run_brainstorming_authorized(
            kernel,
            draft,
            authorization,
            &mut self.brainstorming,
            &self.authority,
            control,
        )
    }

    pub fn run_github_creation(
        &mut self,
        kernel: &mut crate::MasterKernel,
        repository_id: uuid::Uuid,
        control: &PlanningEffectControl,
    ) -> Result<RepositoryCreationProjection, MasterError> {
        crate::run_github_repository_creation(
            kernel,
            repository_id,
            &mut self.github,
            &self.authority,
            control,
        )
    }
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct ProviderRequest<'a> {
    schema_version: u16,
    operation: &'static str,
    provider_id: &'static str,
    model_id: &'static str,
    idempotency_key_sha256: String,
    information_classification: &'static str,
    owner_cloud_disclosure_sha256: [u8; 32],
    draft: Option<&'a BrainstormingDraft>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconciliationResponse {
    status: String,
    specification: Option<BrainstormingSpecificationDocument>,
}

struct ProcessBrainstormingAdapter {
    root: PathBuf,
    executable: Executable,
    codex: Executable,
    schema_path: PathBuf,
    schema_sha256: [u8; 32],
    codex_home: PathBuf,
    reconciliation: PathBuf,
    temporary: PathBuf,
    local_app_data: PathBuf,
    binding: BrainstormingAdapterBinding,
    isolation: ProcessIsolation,
}

impl BrainstormingAdapter for ProcessBrainstormingAdapter {
    fn binding(&self) -> Option<BrainstormingAdapterBinding> {
        Some(self.binding.clone())
    }

    fn generate(
        &mut self,
        draft: &BrainstormingDraft,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<BrainstormingSpecificationDocument, BrainstormingAdapterError> {
        let _ = (draft, idempotency_key, control);
        Err(BrainstormingAdapterError::Rejected)
    }

    fn generate_authorized(
        &mut self,
        draft: &BrainstormingDraft,
        authorization: &BrainstormingCloudAuthorization,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<BrainstormingSpecificationDocument, BrainstormingAdapterError> {
        let request = ProviderRequest {
            schema_version: 1,
            operation: "generate",
            provider_id: PROVIDER_ID,
            model_id: MODEL_ID,
            idempotency_key_sha256: hex(&idempotency_key),
            information_classification: "public",
            owner_cloud_disclosure_sha256: authorization.owner_cloud_disclosure_sha256,
            draft: Some(draft),
        };
        let output = self.invoke(&request, control)?;
        let specification: BrainstormingSpecificationDocument =
            strict_decode(&output).map_err(|_| BrainstormingAdapterError::MalformedOutput)?;
        specification
            .validate()
            .map_err(|_| BrainstormingAdapterError::MalformedOutput)?;
        Ok(specification)
    }

    fn reconcile(
        &mut self,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<Option<BrainstormingSpecificationDocument>, BrainstormingAdapterError> {
        let _ = (idempotency_key, control);
        Err(BrainstormingAdapterError::Rejected)
    }

    fn reconcile_authorized(
        &mut self,
        authorization: &BrainstormingCloudAuthorization,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<Option<BrainstormingSpecificationDocument>, BrainstormingAdapterError> {
        let request = ProviderRequest {
            schema_version: 1,
            operation: "reconcile",
            provider_id: PROVIDER_ID,
            model_id: MODEL_ID,
            idempotency_key_sha256: hex(&idempotency_key),
            information_classification: "public",
            owner_cloud_disclosure_sha256: authorization.owner_cloud_disclosure_sha256,
            draft: None,
        };
        let response: ReconciliationResponse = strict_decode(&self.invoke(&request, control)?)
            .map_err(|_| BrainstormingAdapterError::MalformedOutput)?;
        match (response.status.as_str(), response.specification) {
            ("not_found", None) => Ok(None),
            ("found", Some(specification)) => {
                specification
                    .validate()
                    .map_err(|_| BrainstormingAdapterError::MalformedOutput)?;
                Ok(Some(specification))
            }
            _ => Err(BrainstormingAdapterError::MalformedOutput),
        }
    }
}

impl ProcessBrainstormingAdapter {
    fn invoke<T: Serialize>(
        &self,
        request: &T,
        control: &PlanningEffectControl,
    ) -> Result<Vec<u8>, BrainstormingAdapterError> {
        let _codex_guard =
            verify_executable(&self.codex).map_err(|_| BrainstormingAdapterError::Unavailable)?;
        let _schema_guard = verify_trusted_file(&self.schema_path, self.schema_sha256)
            .map_err(|_| BrainstormingAdapterError::Unavailable)?;
        validate_private(&self.codex_home, true)
            .map_err(|_| BrainstormingAdapterError::Unavailable)?;
        validate_private(&self.reconciliation, true)
            .map_err(|_| BrainstormingAdapterError::Unavailable)?;
        validate_private(&self.temporary, true)
            .map_err(|_| BrainstormingAdapterError::Unavailable)?;
        validate_private(&self.local_app_data, true)
            .map_err(|_| BrainstormingAdapterError::Unavailable)?;
        let input = serde_json::to_vec(request).map_err(|_| BrainstormingAdapterError::Rejected)?;
        run_command(
            &self.executable,
            control,
            CommandInvocation {
                current_dir: &self.root,
                args: &[],
                environment: &[],
                input: &input,
                max_output: MAX_PROVIDER_OUTPUT_BYTES,
                isolation: &self.isolation,
                stderr: CommandStderrMode::default(),
            },
        )
        .map_err(|error| {
            let Some(diagnostic_code) = planning_provider_diagnostic_code(error) else {
                return BrainstormingAdapterError::Cancelled;
            };
            tracing::warn!(
                target: "assemblywright::planning_provider",
                diagnostic_code,
                "planning provider process failed"
            );
            match error {
                CommandError::Malformed => BrainstormingAdapterError::MalformedOutput,
                CommandError::Failed | CommandError::Exited(_) => {
                    BrainstormingAdapterError::Unavailable
                }
                CommandError::Cancelled => unreachable!("handled above"),
            }
        })
    }
}

struct ProcessGithubCreationAdapter {
    root: PathBuf,
    executable: Executable,
    config_dir: PathBuf,
    owner: String,
    binding_sha256: [u8; 32],
    isolation: ProcessIsolation,
}

impl GithubRepositoryCreationAdapter for ProcessGithubCreationAdapter {
    fn binding_sha256(&self) -> Option<[u8; 32]> {
        Some(self.binding_sha256)
    }

    fn inspect(
        &mut self,
        repository: &AssemblyLineRepositoryIdentity,
        _idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<Option<GithubRepositoryObservation>, GithubRepositoryCreationError> {
        self.revalidate()?;
        let (owner, name) =
            repository_parts(repository).ok_or(GithubRepositoryCreationError::Rejected)?;
        if owner != self.owner {
            return Err(GithubRepositoryCreationError::Rejected);
        }
        let target = format!("{owner}/{name}");
        let args = [
            "repo",
            "view",
            target.as_str(),
            "--json",
            "nameWithOwner,visibility,defaultBranchRef,isEmpty,url",
        ];
        let env = [(OsStr::new("GH_CONFIG_DIR"), self.config_dir.as_os_str())];
        match run_command(
            &self.executable,
            control,
            CommandInvocation {
                current_dir: &self.root,
                args: &args,
                environment: &env,
                input: &[],
                max_output: MAX_GH_OUTPUT_BYTES,
                isolation: &self.isolation,
                stderr: CommandStderrMode::default(),
            },
        ) {
            Ok(output) => parse_github_observation(repository, &output).map(Some),
            Err(CommandError::Cancelled) => Err(GithubRepositoryCreationError::Cancelled),
            Err(CommandError::Malformed) => Err(GithubRepositoryCreationError::MalformedOutput),
            Err(CommandError::Failed | CommandError::Exited(_)) => {
                let identity = run_command(
                    &self.executable,
                    control,
                    CommandInvocation {
                        current_dir: &self.root,
                        args: &["api", "user"],
                        environment: &env,
                        input: &[],
                        max_output: 16 * 1024,
                        isolation: &self.isolation,
                        stderr: CommandStderrMode::default(),
                    },
                )
                .map_err(|_| GithubRepositoryCreationError::Unavailable)?;
                let value: Value = strict_decode(&identity)
                    .map_err(|_| GithubRepositoryCreationError::MalformedOutput)?;
                if value.get("login").and_then(Value::as_str) == Some(self.owner.as_str()) {
                    Ok(None)
                } else {
                    Err(GithubRepositoryCreationError::Unavailable)
                }
            }
        }
    }

    fn create(
        &mut self,
        plan: &RepositoryCreationProjection,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<GithubRepositoryObservation, GithubRepositoryCreationError> {
        self.revalidate()?;
        let (owner, name) =
            repository_parts(&plan.repository).ok_or(GithubRepositoryCreationError::Rejected)?;
        if owner != self.owner {
            return Err(GithubRepositoryCreationError::Rejected);
        }
        let target = format!("{owner}/{name}");
        let visibility = match plan.visibility {
            ProjectVisibility::Public => "--public",
            ProjectVisibility::Private => "--private",
        };
        let env = [(OsStr::new("GH_CONFIG_DIR"), self.config_dir.as_os_str())];
        let result = run_command(
            &self.executable,
            control,
            CommandInvocation {
                current_dir: &self.root,
                args: &[
                    "repo",
                    "create",
                    target.as_str(),
                    visibility,
                    "--add-readme",
                ],
                environment: &env,
                input: &[],
                max_output: MAX_GH_OUTPUT_BYTES,
                isolation: &self.isolation,
                stderr: CommandStderrMode::default(),
            },
        );
        if matches!(result, Err(CommandError::Cancelled)) {
            return Err(GithubRepositoryCreationError::Cancelled);
        }
        match self.reconcile_creation(plan, idempotency_key, control)? {
            Some(observation) => Ok(observation),
            None => Err(GithubRepositoryCreationError::Ambiguous),
        }
    }

    fn reconcile_creation(
        &mut self,
        plan: &RepositoryCreationProjection,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<Option<GithubRepositoryObservation>, GithubRepositoryCreationError> {
        let Some(observation) = self.inspect(&plan.repository, idempotency_key, control)? else {
            return Ok(None);
        };
        if observation.visibility != plan.visibility || !observation.initialized {
            return Ok(Some(observation));
        }
        if observation.default_branch == "main" {
            return Ok(Some(observation));
        }
        let (owner, name) =
            repository_parts(&plan.repository).ok_or(GithubRepositoryCreationError::Rejected)?;
        let branch = github_path_component(&observation.default_branch)
            .ok_or(GithubRepositoryCreationError::MalformedOutput)?;
        let endpoint = format!("repos/{owner}/{name}/branches/{branch}/rename");
        let env = [(OsStr::new("GH_CONFIG_DIR"), self.config_dir.as_os_str())];
        run_command(
            &self.executable,
            control,
            CommandInvocation {
                current_dir: &self.root,
                args: &[
                    "api",
                    "--method",
                    "POST",
                    endpoint.as_str(),
                    "-f",
                    "new_name=main",
                ],
                environment: &env,
                input: &[],
                max_output: MAX_GH_OUTPUT_BYTES,
                isolation: &self.isolation,
                stderr: CommandStderrMode::default(),
            },
        )
        .map_err(|error| match error {
            CommandError::Cancelled => GithubRepositoryCreationError::Cancelled,
            CommandError::Malformed => GithubRepositoryCreationError::MalformedOutput,
            CommandError::Failed | CommandError::Exited(_) => {
                GithubRepositoryCreationError::Ambiguous
            }
        })?;
        self.inspect(&plan.repository, idempotency_key, control)
    }
}

impl ProcessGithubCreationAdapter {
    fn revalidate(&self) -> Result<(), GithubRepositoryCreationError> {
        verify_executable(&self.executable)
            .map_err(|_| GithubRepositoryCreationError::Unavailable)?;
        validate_private(&self.config_dir, true)
            .map_err(|_| GithubRepositoryCreationError::Unavailable)?;
        let hosts = self.config_dir.join(HOSTS);
        reject_link(&hosts).map_err(|_| GithubRepositoryCreationError::Unavailable)?;
        validate_private(&hosts, false).map_err(|_| GithubRepositoryCreationError::Unavailable)?;
        Ok(())
    }
}

fn parse_github_observation(
    repository: &AssemblyLineRepositoryIdentity,
    bytes: &[u8],
) -> Result<GithubRepositoryObservation, GithubRepositoryCreationError> {
    let value: Value =
        strict_decode(bytes).map_err(|_| GithubRepositoryCreationError::MalformedOutput)?;
    let (owner, name) =
        repository_parts(repository).ok_or(GithubRepositoryCreationError::MalformedOutput)?;
    let expected = format!("{owner}/{name}");
    let visibility = match value.get("visibility").and_then(Value::as_str) {
        Some("PUBLIC") => ProjectVisibility::Public,
        Some("PRIVATE") => ProjectVisibility::Private,
        _ => return Err(GithubRepositoryCreationError::MalformedOutput),
    };
    let branch = value
        .get("defaultBranchRef")
        .and_then(|branch| branch.get("name"))
        .and_then(Value::as_str)
        .ok_or(GithubRepositoryCreationError::MalformedOutput)?;
    if value.get("nameWithOwner").and_then(Value::as_str) != Some(expected.as_str())
        || value.get("url").and_then(Value::as_str)
            != Some(repository.git_url.url.trim_end_matches(".git"))
        || value.get("isEmpty").and_then(Value::as_bool).is_none()
    {
        return Err(GithubRepositoryCreationError::MalformedOutput);
    }
    Ok(GithubRepositoryObservation {
        repository: repository.clone(),
        visibility,
        default_branch: branch.to_string(),
        initialized: !value["isEmpty"].as_bool().unwrap_or(true),
    })
}

fn repository_parts(repository: &AssemblyLineRepositoryIdentity) -> Option<(&str, &str)> {
    let value = repository.git_url.url.strip_prefix("https://github.com/")?;
    let value = value.strip_suffix(".git").unwrap_or(value);
    let mut parts = value.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    if parts.next().is_some() || !valid_github_owner(owner) || !valid_repository_name(name) {
        return None;
    }
    Some((owner, name))
}

fn valid_github_owner(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 39
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_repository_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn github_path_component(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 255 || value.bytes().any(|byte| byte.is_ascii_control()) {
        return None;
    }
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            write!(&mut encoded, "%{byte:02X}").ok()?;
        }
    }
    Some(encoded)
}

#[derive(Debug, Clone, Copy)]
enum CommandError {
    Failed,
    Exited(u32),
    Cancelled,
    Malformed,
}

#[derive(Default)]
enum CommandStderrMode {
    #[default]
    Discard,
    #[cfg(any(windows, test))]
    CaptureBounded {
        max_bytes: usize,
        output: Arc<Mutex<Vec<u8>>>,
    },
}

impl CommandStderrMode {
    fn valid_for(&self, _args: &[&str]) -> bool {
        match self {
            Self::Discard => true,
            #[cfg(any(windows, test))]
            Self::CaptureBounded { max_bytes, output } => {
                ((*max_bytes == NATIVE_PROVIDER_PROBE_LOGIN_STDERR_MAX_BYTES
                    && _args == native_provider_probe_login_args())
                    || (*max_bytes == NATIVE_PROVIDER_PROBE_EXEC_STDERR_MAX_BYTES
                        && native_provider_probe_exec_args_match(_args)))
                    && Arc::strong_count(output) > 0
            }
        }
    }

    #[cfg(any(windows, test))]
    fn allows_empty_stdout(&self, args: &[&str]) -> bool {
        matches!(
            self,
            Self::CaptureBounded { max_bytes, .. }
                if *max_bytes == NATIVE_PROVIDER_PROBE_LOGIN_STDERR_MAX_BYTES
                    && args == native_provider_probe_login_args()
        )
    }

    #[cfg(windows)]
    fn record_containment_failure(&self, stage: u8, status: u32) {
        if let Self::CaptureBounded { max_bytes, output } = self {
            let mut message =
                format!("containment setup failed stage={stage} status={status}").into_bytes();
            message.truncate(*max_bytes);
            let mut guard = output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.zeroize();
            *guard = message;
        }
    }
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeProbeLoginDiagnostic {
    RequirementsFileReadFailed,
    ConfigFileReadFailed,
    AuthenticationRequirementsNoUsableLoginMethod,
    AccessDeniedIo,
    SystemCodexPathAccessDenied,
    ProviderCodexHomeAccessDenied,
    ProviderLocalAppDataAccessDenied,
    ProviderTempAccessDenied,
    ProviderRootOrCurrentDirectoryAccessDenied,
    CodexHomeCanonicalizationFailed,
    CodexHomeReadFailed,
    ProcessLaunchAccessDenied,
    ProcessLaunchFailed,
    ContainmentStage(u8),
    NotLoggedIn,
    ConfigurationLoadFailed,
    LoginStatusFailed,
    Other,
}

#[cfg(any(windows, test))]
impl NativeProbeLoginDiagnostic {
    const fn code(self) -> u32 {
        match self {
            Self::RequirementsFileReadFailed => 924,
            Self::ConfigFileReadFailed => 925,
            Self::AuthenticationRequirementsNoUsableLoginMethod => 926,
            Self::AccessDeniedIo => 927,
            Self::SystemCodexPathAccessDenied => 928,
            Self::ProviderCodexHomeAccessDenied => 929,
            Self::ProviderLocalAppDataAccessDenied => 930,
            Self::ProviderTempAccessDenied => 931,
            Self::ProviderRootOrCurrentDirectoryAccessDenied => 932,
            Self::CodexHomeCanonicalizationFailed => 933,
            Self::CodexHomeReadFailed => 934,
            Self::ProcessLaunchAccessDenied => 935,
            Self::ProcessLaunchFailed => 936,
            Self::ContainmentStage(stage) => 960 + stage as u32,
            Self::NotLoggedIn => 920,
            Self::ConfigurationLoadFailed => 921,
            Self::LoginStatusFailed => 922,
            Self::Other => 923,
        }
    }
}

#[cfg(any(windows, test))]
fn native_probe_login_diagnostic(stderr: &Arc<Mutex<Vec<u8>>>) -> NativeProbeLoginDiagnostic {
    let mut guard = stderr
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut bytes = std::mem::take(&mut *guard);
    drop(guard);
    let diagnostic = classify_native_probe_login_stderr(&bytes);
    bytes.zeroize();
    diagnostic
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeProbeExecDiagnostic {
    ConfigurationLoadFailed,
    NotLoggedIn,
    AuthenticationFailed,
    NetworkUnavailable,
    CliArgumentRejected,
    OutputSchemaRejected,
    ModelUnavailable,
    SystemCodexPathAccessDenied,
    ProviderCodexHomeAccessDenied,
    ProviderLocalAppDataAccessDenied,
    ProviderTempAccessDenied,
    ProviderRootOrCurrentDirectoryAccessDenied,
    CodexHomeCanonicalizationFailed,
    ExplicitCwdCanonicalizationFailed,
    ProcessLaunchAccessDenied,
    ProcessLaunchFailed,
    ContainmentStage(u8),
    AccessDeniedIo,
    Other,
}

#[cfg(any(windows, test))]
impl NativeProbeExecDiagnostic {
    const fn code(self) -> u32 {
        match self {
            Self::ConfigurationLoadFailed => 940,
            Self::NotLoggedIn => 941,
            Self::AuthenticationFailed => 942,
            Self::NetworkUnavailable => 943,
            Self::CliArgumentRejected => 944,
            Self::OutputSchemaRejected => 945,
            Self::ModelUnavailable => 946,
            Self::SystemCodexPathAccessDenied => 947,
            Self::ProviderCodexHomeAccessDenied => 948,
            Self::ProviderLocalAppDataAccessDenied => 949,
            Self::ProviderTempAccessDenied => 950,
            Self::ProviderRootOrCurrentDirectoryAccessDenied => 951,
            Self::CodexHomeCanonicalizationFailed => 954,
            Self::ExplicitCwdCanonicalizationFailed => 955,
            Self::ProcessLaunchAccessDenied => 956,
            Self::ProcessLaunchFailed => 957,
            Self::ContainmentStage(stage) => 990 + stage as u32,
            Self::AccessDeniedIo => 952,
            Self::Other => 953,
        }
    }
}

#[cfg(any(windows, test))]
fn native_probe_exec_diagnostic(stderr: &Arc<Mutex<Vec<u8>>>) -> Option<NativeProbeExecDiagnostic> {
    let mut guard = stderr
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut bytes = std::mem::take(&mut *guard);
    drop(guard);
    let diagnostic = classify_native_probe_exec_stderr(&bytes);
    bytes.zeroize();
    diagnostic
}

#[cfg(any(windows, test))]
fn classify_native_probe_exec_stderr(stderr: &[u8]) -> Option<NativeProbeExecDiagnostic> {
    if stderr.is_empty() {
        None
    } else if let Some(stage) = native_probe_containment_stage(stderr) {
        Some(NativeProbeExecDiagnostic::ContainmentStage(stage))
    } else if ascii_contains_ignore_case(stderr, b"process launch failed status=5") {
        Some(NativeProbeExecDiagnostic::ProcessLaunchAccessDenied)
    } else if ascii_contains_ignore_case(stderr, b"process launch failed status=") {
        Some(NativeProbeExecDiagnostic::ProcessLaunchFailed)
    } else if native_probe_stderr_contains_access_denied(stderr)
        && ascii_contains_ignore_case(stderr, b"failed to canonicalize codex_home")
    {
        Some(NativeProbeExecDiagnostic::CodexHomeCanonicalizationFailed)
    } else if native_probe_stderr_contains_access_denied(stderr)
        && (ascii_contains_ignore_case(stderr, b"failed to canonicalize current directory")
            || ascii_contains_ignore_case(stderr, b"failed to canonicalize working directory")
            || ascii_contains_ignore_case(stderr, b"failed to canonicalize cwd"))
    {
        Some(NativeProbeExecDiagnostic::ExplicitCwdCanonicalizationFailed)
    } else if ascii_contains_ignore_case(stderr, b"failed to load configuration")
        || ascii_contains_ignore_case(stderr, b"error loading configuration")
        || ascii_contains_ignore_case(stderr, b"configuration load failed")
    {
        Some(NativeProbeExecDiagnostic::ConfigurationLoadFailed)
    } else if ascii_contains_ignore_case(stderr, b"not logged in")
        || ascii_contains_ignore_case(stderr, b"not authenticated")
        || ascii_contains_ignore_case(stderr, b"login required")
    {
        Some(NativeProbeExecDiagnostic::NotLoggedIn)
    } else if ascii_contains_ignore_case(stderr, b"unauthorized")
        || ascii_contains_ignore_case(stderr, b"authentication failed")
        || ascii_contains_ignore_case(stderr, b"invalid api key")
        || ascii_contains_ignore_case(stderr, b"status: 401")
    {
        Some(NativeProbeExecDiagnostic::AuthenticationFailed)
    } else if ascii_contains_ignore_case(stderr, b"failed to connect")
        || ascii_contains_ignore_case(stderr, b"connection error")
        || ascii_contains_ignore_case(stderr, b"network is unreachable")
        || ascii_contains_ignore_case(stderr, b"dns error")
    {
        Some(NativeProbeExecDiagnostic::NetworkUnavailable)
    } else if ascii_contains_ignore_case(stderr, b"unexpected argument")
        || ascii_contains_ignore_case(stderr, b"unrecognized option")
        || ascii_contains_ignore_case(stderr, b"invalid value")
    {
        Some(NativeProbeExecDiagnostic::CliArgumentRejected)
    } else if ascii_contains_ignore_case(stderr, b"output schema")
        || ascii_contains_ignore_case(stderr, b"json schema")
    {
        Some(NativeProbeExecDiagnostic::OutputSchemaRejected)
    } else if ascii_contains_ignore_case(stderr, b"model not found")
        || ascii_contains_ignore_case(stderr, b"model is not available")
        || ascii_contains_ignore_case(stderr, b"unsupported model")
    {
        Some(NativeProbeExecDiagnostic::ModelUnavailable)
    } else if native_probe_stderr_contains_access_denied(stderr)
        && ascii_contains_ignore_case(stderr, b"\\ProgramData\\OpenAI\\Codex\\")
    {
        Some(NativeProbeExecDiagnostic::SystemCodexPathAccessDenied)
    } else if native_probe_stderr_contains_access_denied(stderr)
        && ascii_contains_ignore_case(stderr, b"\\planning-runtime\\provider\\codex-home")
    {
        Some(NativeProbeExecDiagnostic::ProviderCodexHomeAccessDenied)
    } else if native_probe_stderr_contains_access_denied(stderr)
        && ascii_contains_ignore_case(stderr, b"\\planning-runtime\\provider\\local-app-data")
    {
        Some(NativeProbeExecDiagnostic::ProviderLocalAppDataAccessDenied)
    } else if native_probe_stderr_contains_access_denied(stderr)
        && ascii_contains_ignore_case(stderr, b"\\planning-runtime\\provider\\temp")
    {
        Some(NativeProbeExecDiagnostic::ProviderTempAccessDenied)
    } else if native_probe_stderr_contains_access_denied(stderr)
        && (ascii_contains_ignore_case(stderr, b"\\planning-runtime\\provider")
            || ascii_contains_ignore_case(stderr, b"current working directory"))
    {
        Some(NativeProbeExecDiagnostic::ProviderRootOrCurrentDirectoryAccessDenied)
    } else if native_probe_stderr_contains_access_denied(stderr) {
        Some(NativeProbeExecDiagnostic::AccessDeniedIo)
    } else {
        Some(NativeProbeExecDiagnostic::Other)
    }
}

#[cfg(any(windows, test))]
fn classify_native_probe_login_stderr(stderr: &[u8]) -> NativeProbeLoginDiagnostic {
    let access_denied = native_probe_stderr_contains_access_denied(stderr);
    if let Some(stage) = native_probe_containment_stage(stderr) {
        NativeProbeLoginDiagnostic::ContainmentStage(stage)
    } else if ascii_contains_ignore_case(stderr, b"process launch failed status=5") {
        NativeProbeLoginDiagnostic::ProcessLaunchAccessDenied
    } else if ascii_contains_ignore_case(stderr, b"process launch failed status=") {
        NativeProbeLoginDiagnostic::ProcessLaunchFailed
    } else if access_denied
        && ascii_contains_ignore_case(stderr, b"failed to canonicalize codex_home")
    {
        NativeProbeLoginDiagnostic::CodexHomeCanonicalizationFailed
    } else if access_denied && ascii_contains_ignore_case(stderr, b"failed to read codex_home") {
        NativeProbeLoginDiagnostic::CodexHomeReadFailed
    } else if ascii_contains_ignore_case(stderr, b"failed to read requirements file") {
        NativeProbeLoginDiagnostic::RequirementsFileReadFailed
    } else if ascii_contains_ignore_case(stderr, b"failed to read config file") {
        NativeProbeLoginDiagnostic::ConfigFileReadFailed
    } else if ascii_contains_ignore_case(
        stderr,
        b"authentication requirements do not permit any usable login method",
    ) {
        NativeProbeLoginDiagnostic::AuthenticationRequirementsNoUsableLoginMethod
    } else if access_denied && ascii_contains_ignore_case(stderr, b"\\ProgramData\\OpenAI\\Codex\\")
    {
        NativeProbeLoginDiagnostic::SystemCodexPathAccessDenied
    } else if access_denied
        && ascii_contains_ignore_case(stderr, b"\\planning-runtime\\provider\\codex-home")
    {
        NativeProbeLoginDiagnostic::ProviderCodexHomeAccessDenied
    } else if access_denied
        && ascii_contains_ignore_case(stderr, b"\\planning-runtime\\provider\\local-app-data")
    {
        NativeProbeLoginDiagnostic::ProviderLocalAppDataAccessDenied
    } else if access_denied
        && ascii_contains_ignore_case(stderr, b"\\planning-runtime\\provider\\temp")
    {
        NativeProbeLoginDiagnostic::ProviderTempAccessDenied
    } else if access_denied
        && (ascii_contains_ignore_case(stderr, b"\\planning-runtime\\provider")
            || ascii_contains_ignore_case(stderr, b"current working directory"))
    {
        NativeProbeLoginDiagnostic::ProviderRootOrCurrentDirectoryAccessDenied
    } else if ascii_contains_ignore_case(stderr, b"failed to load configuration")
        || ascii_contains_ignore_case(stderr, b"error loading configuration")
        || ascii_contains_ignore_case(stderr, b"configuration load failed")
    {
        NativeProbeLoginDiagnostic::ConfigurationLoadFailed
    } else if ascii_contains_ignore_case(stderr, b"error checking login status")
        || ascii_contains_ignore_case(stderr, b"login status request failed")
        || ascii_contains_ignore_case(stderr, b"authentication status")
    {
        NativeProbeLoginDiagnostic::LoginStatusFailed
    } else if access_denied {
        NativeProbeLoginDiagnostic::AccessDeniedIo
    } else if ascii_contains_ignore_case(stderr, b"not logged in")
        || ascii_contains_ignore_case(stderr, b"not authenticated")
        || ascii_contains_ignore_case(stderr, b"login required")
    {
        NativeProbeLoginDiagnostic::NotLoggedIn
    } else {
        NativeProbeLoginDiagnostic::Other
    }
}

#[cfg(any(windows, test))]
fn native_probe_containment_stage(stderr: &[u8]) -> Option<u8> {
    (1_u8..=32).find(|stage| {
        ascii_contains_ignore_case(
            stderr,
            format!("containment setup failed stage={stage} status=").as_bytes(),
        )
    })
}

#[cfg(any(windows, test))]
fn native_probe_stderr_contains_access_denied(stderr: &[u8]) -> bool {
    ascii_contains_ignore_case(stderr, b"access is denied")
        || ascii_contains_ignore_case(stderr, b"permission denied")
        || ascii_contains_ignore_case(stderr, b"os error 5")
}

#[cfg(any(windows, test))]
fn ascii_contains_ignore_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

fn planning_provider_diagnostic_code(error: CommandError) -> Option<u32> {
    match error {
        CommandError::Failed => Some(900),
        CommandError::Malformed => Some(901),
        CommandError::Exited(code) => Some(1_000_u32.saturating_add(code.min(255))),
        CommandError::Cancelled => None,
    }
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy)]
enum ProbePhaseOutcome {
    NotRun,
    Success,
    RejectedOutput,
    Error(CommandError),
}

#[cfg(any(windows, test))]
fn probe_phase_exit_code(outcome: ProbePhaseOutcome) -> Option<u32> {
    match outcome {
        ProbePhaseOutcome::Success | ProbePhaseOutcome::RejectedOutput => Some(0),
        ProbePhaseOutcome::Error(CommandError::Exited(code)) => Some(code),
        ProbePhaseOutcome::NotRun
        | ProbePhaseOutcome::Error(
            CommandError::Failed | CommandError::Malformed | CommandError::Cancelled,
        ) => None,
    }
}

#[cfg(any(windows, test))]
fn probe_phase_diagnostic_code(outcome: ProbePhaseOutcome) -> Option<u32> {
    match outcome {
        ProbePhaseOutcome::RejectedOutput => Some(901),
        ProbePhaseOutcome::Error(error) => planning_provider_diagnostic_code(error),
        ProbePhaseOutcome::NotRun | ProbePhaseOutcome::Success => None,
    }
}

#[cfg(any(windows, test))]
fn native_probe_post_command_authority_current(runtime_valid: bool, control_current: bool) -> bool {
    runtime_valid && control_current
}

#[cfg(any(windows, test))]
fn native_provider_probe_login_args() -> [&'static str; 6] {
    [
        "--config",
        FILE_BACKED_CODEX_AUTH_CONFIG,
        "--config",
        "project_root_markers=[]",
        "login",
        "status",
    ]
}

#[cfg(any(windows, test))]
fn native_provider_probe_exec_args(output_schema: &str) -> Vec<&str> {
    vec![
        "exec",
        "--strict-config",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--skip-git-repo-check",
        "--sandbox",
        "danger-full-access",
        "--model",
        MODEL_ID,
        "--config",
        FILE_BACKED_CODEX_AUTH_CONFIG,
        "--config",
        "project_root_markers=[]",
        "--config",
        "model_reasoning_effort=\"high\"",
        "--config",
        "model_reasoning_summary=\"none\"",
        "--config",
        "model_verbosity=\"low\"",
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
        output_schema,
        "-",
    ]
}

#[cfg(any(windows, test))]
fn native_provider_probe_exec_args_match(args: &[&str]) -> bool {
    let Some(output_schema_index) = args
        .iter()
        .position(|argument| *argument == "--output-schema")
    else {
        return false;
    };
    let Some(output_schema) = args.get(output_schema_index + 1) else {
        return false;
    };
    args == native_provider_probe_exec_args(output_schema)
}

struct CommandInvocation<'a> {
    current_dir: &'a Path,
    args: &'a [&'a str],
    environment: &'a [(&'a OsStr, &'a OsStr)],
    input: &'a [u8],
    max_output: usize,
    isolation: &'a ProcessIsolation,
    stderr: CommandStderrMode,
}

fn run_command(
    executable: &Executable,
    control: &PlanningEffectControl,
    invocation: CommandInvocation<'_>,
) -> Result<Vec<u8>, CommandError> {
    if !invocation.stderr.valid_for(invocation.args) {
        return Err(CommandError::Failed);
    }
    #[cfg(windows)]
    if let Err(error) = invocation.isolation.revalidate() {
        invocation.stderr.record_containment_failure(30, 0);
        return Err(error);
    }
    #[cfg(not(windows))]
    invocation.isolation.revalidate()?;
    #[cfg(windows)]
    {
        windows_containment::run_command(
            executable,
            control,
            &invocation,
            &invocation.isolation.profile,
        )
    }
    #[cfg(not(windows))]
    {
        run_command_portable(
            executable,
            invocation.current_dir,
            invocation.args,
            invocation.environment,
            invocation.input,
            invocation.max_output,
            control,
        )
    }
}

#[cfg(not(windows))]
fn run_command_portable(
    executable: &Executable,
    current_dir: &Path,
    args: &[&str],
    environment: &[(&OsStr, &OsStr)],
    input: &[u8],
    max_output: usize,
    control: &PlanningEffectControl,
) -> Result<Vec<u8>, CommandError> {
    if !control.poll() {
        return Err(CommandError::Failed);
    }
    let mut executable_guard = verify_executable(executable)?;
    executable_guard
        .rewind()
        .map_err(|_| CommandError::Failed)?;
    #[cfg(target_os = "linux")]
    let (spawn_path, descriptor_flags) = {
        use std::os::fd::AsRawFd;
        let descriptor = executable_guard.as_raw_fd();
        let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
        if flags < 0
            || unsafe { libc::fcntl(descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) } < 0
        {
            return Err(CommandError::Failed);
        }
        (PathBuf::from(format!("/dev/fd/{descriptor}")), flags)
    };
    #[cfg(not(target_os = "linux"))]
    let spawn_path = executable.path.clone();
    let mut command = Command::new(&spawn_path);
    command
        .args(args)
        .current_dir(current_dir)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (name, value) in environment {
        command.env(name, value);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let spawned = command.spawn();
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        unsafe {
            libc::fcntl(
                executable_guard.as_raw_fd(),
                libc::F_SETFD,
                descriptor_flags,
            );
        }
    }
    let mut child = spawned.map_err(|_| CommandError::Failed)?;
    let containment = ChildContainment::bind(&mut child)?;
    let mut stdin = child.stdin.take().ok_or(CommandError::Failed)?;
    stdin.write_all(input).map_err(|_| CommandError::Failed)?;
    drop(stdin);
    let stdout = child.stdout.take().ok_or(CommandError::Failed)?;
    let output_thread = bounded_reader(stdout, max_output, false);
    loop {
        if !control.poll() {
            containment.terminate(&mut child);
            let _ = output_thread.join();
            return Err(CommandError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                containment.terminate_after_parent_exit();
                let output = output_thread.join().map_err(|_| CommandError::Failed)?;
                if !status.success() {
                    let code = status
                        .code()
                        .and_then(|code| u32::try_from(code).ok())
                        .unwrap_or(255);
                    return Err(CommandError::Exited(code));
                }
                return output;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                containment.terminate(&mut child);
                let _ = output_thread.join();
                return Err(CommandError::Failed);
            }
        }
    }
}

fn bounded_reader(
    mut reader: impl Read + Send + 'static,
    max: usize,
    allow_empty: bool,
) -> thread::JoinHandle<Result<Vec<u8>, CommandError>> {
    thread::spawn(move || {
        let mut retained = Vec::new();
        let mut oversized = false;
        let mut buffer = [0_u8; 8192];
        loop {
            let count = reader.read(&mut buffer).map_err(|_| CommandError::Failed)?;
            if count == 0 {
                break;
            }
            if retained.len().saturating_add(count) <= max {
                retained.extend_from_slice(&buffer[..count]);
            } else {
                oversized = true;
            }
        }
        if oversized || (!allow_empty && retained.is_empty()) {
            Err(CommandError::Malformed)
        } else {
            Ok(retained)
        }
    })
}

#[cfg(unix)]
struct ChildContainment(i32);

#[cfg(unix)]
impl ChildContainment {
    fn bind(child: &mut std::process::Child) -> Result<Self, CommandError> {
        Ok(Self(
            i32::try_from(child.id()).map_err(|_| CommandError::Failed)?,
        ))
    }

    fn terminate(&self, child: &mut std::process::Child) {
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }

    fn terminate_after_parent_exit(&self) {
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}

#[cfg(unix)]
impl Drop for ChildContainment {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}

#[cfg(not(any(unix, windows)))]
struct ChildContainment;

#[cfg(not(any(unix, windows)))]
impl ChildContainment {
    fn bind(_child: &mut std::process::Child) -> Result<Self, CommandError> {
        Ok(Self)
    }

    fn terminate(&self, child: &mut std::process::Child) {
        let _ = child.kill();
        let _ = child.wait();
    }

    fn terminate_after_parent_exit(&self) {}
}

fn load_executable(path: &Path, expected: &str) -> Result<Executable, PlanningRuntimeConfigError> {
    let expected = decode_sha256(expected).ok_or(PlanningRuntimeConfigError::Invalid)?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 384 * 1024 * 1024 {
        return Err(PlanningRuntimeConfigError::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.permissions().mode() & 0o111 == 0
            || metadata.permissions().mode() & 0o022 != 0
            || metadata.nlink() != 1
        {
            return Err(PlanningRuntimeConfigError::Invalid);
        }
    }
    let bytes = fs::read(path)?;
    let sha256: [u8; 32] = Sha256::digest(&bytes).into();
    if sha256 != expected {
        return Err(PlanningRuntimeConfigError::Invalid);
    }
    Ok(Executable {
        path: fs::canonicalize(path)?,
        sha256,
        length: metadata.len(),
    })
}

fn load_trusted_file(path: &Path, expected: &str) -> Result<[u8; 32], PlanningRuntimeConfigError> {
    let expected = decode_sha256(expected).ok_or(PlanningRuntimeConfigError::Invalid)?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_CONFIG_BYTES as u64 {
        return Err(PlanningRuntimeConfigError::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.permissions().mode() & 0o022 != 0 || metadata.nlink() != 1 {
            return Err(PlanningRuntimeConfigError::Invalid);
        }
    }
    let actual: [u8; 32] = Sha256::digest(fs::read(path)?).into();
    if actual != expected {
        return Err(PlanningRuntimeConfigError::Invalid);
    }
    Ok(actual)
}

fn verify_trusted_file(path: &Path, expected: [u8; 32]) -> Result<File, CommandError> {
    reject_link(path).map_err(|_| CommandError::Failed)?;
    validate_private(path, false).map_err(|_| CommandError::Failed)?;
    #[cfg(windows)]
    let mut file = {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).share_mode(FILE_SHARE_READ);
        options.open(path).map_err(|_| CommandError::Failed)?
    };
    #[cfg(not(windows))]
    let mut file = File::open(path).map_err(|_| CommandError::Failed)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| CommandError::Failed)?;
    let actual: [u8; 32] = Sha256::digest(bytes).into();
    if actual != expected {
        return Err(CommandError::Failed);
    }
    Ok(file)
}

fn verify_executable(executable: &Executable) -> Result<File, CommandError> {
    #[cfg(windows)]
    let mut file = {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).share_mode(FILE_SHARE_READ);
        options
            .open(&executable.path)
            .map_err(|_| CommandError::Failed)?
    };
    #[cfg(not(windows))]
    let mut file = File::open(&executable.path).map_err(|_| CommandError::Failed)?;
    let metadata = file.metadata().map_err(|_| CommandError::Failed)?;
    if metadata.len() != executable.length {
        return Err(CommandError::Failed);
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| CommandError::Failed)?;
    if <[u8; 32]>::from(Sha256::digest(&bytes)) != executable.sha256 {
        return Err(CommandError::Failed);
    }
    Ok(file)
}

fn reject_link(path: &Path) -> Result<(), PlanningRuntimeConfigError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(PlanningRuntimeConfigError::Invalid);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(PlanningRuntimeConfigError::Invalid);
        }
    }
    Ok(())
}

fn validate_private(path: &Path, directory: bool) -> Result<(), PlanningRuntimeConfigError> {
    let metadata = fs::metadata(path)?;
    if metadata.is_dir() != directory {
        return Err(PlanningRuntimeConfigError::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(PlanningRuntimeConfigError::Invalid);
        }
    }
    Ok(())
}

fn decode_sha256(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return None;
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(output)
}

struct StrictJsonValue(Value);

impl<'de> Deserialize<'de> for StrictJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = StrictJsonValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::Bool(value)))
            }
            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::Number(value.into())))
            }
            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::Number(value.into())))
            }
            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                serde_json::Number::from_f64(value)
                    .map(Value::Number)
                    .map(StrictJsonValue)
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }
            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::String(value.to_string())))
            }
            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::String(value)))
            }
            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::Null))
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::Null))
            }
            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<StrictJsonValue>()? {
                    values.push(value.0);
                }
                Ok(StrictJsonValue(Value::Array(values)))
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate JSON object key"));
                    }
                    values.insert(key, map.next_value::<StrictJsonValue>()?.0);
                }
                Ok(StrictJsonValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

fn strict_decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, ()> {
    let value = serde_json::from_slice::<StrictJsonValue>(bytes).map_err(|_| ())?;
    serde_json::from_value(value.0).map_err(|_| ())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_allows_empty_output_only_when_explicitly_selected() {
        assert_eq!(
            bounded_reader(std::io::Cursor::new(Vec::new()), 1, true)
                .join()
                .unwrap()
                .unwrap(),
            Vec::<u8>::new(),
        );
        assert!(bounded_reader(std::io::Cursor::new(Vec::new()), 1, false)
            .join()
            .unwrap()
            .is_err());
    }
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn provider_process_diagnostics_are_numeric_bounded_and_suppress_cancellation() {
        assert_eq!(
            planning_provider_diagnostic_code(CommandError::Failed),
            Some(900)
        );
        assert_eq!(
            planning_provider_diagnostic_code(CommandError::Malformed),
            Some(901)
        );
        assert_eq!(
            planning_provider_diagnostic_code(CommandError::Exited(34)),
            Some(1034)
        );
        assert_eq!(
            planning_provider_diagnostic_code(CommandError::Exited(u32::MAX)),
            Some(1255)
        );
        assert_eq!(
            planning_provider_diagnostic_code(CommandError::Cancelled),
            None
        );
    }

    #[test]
    fn native_probe_phase_receipt_codes_distinguish_not_run_success_and_rejection() {
        assert_eq!(probe_phase_exit_code(ProbePhaseOutcome::NotRun), None);
        assert_eq!(probe_phase_diagnostic_code(ProbePhaseOutcome::NotRun), None);
        assert_eq!(probe_phase_exit_code(ProbePhaseOutcome::Success), Some(0));
        assert_eq!(
            probe_phase_diagnostic_code(ProbePhaseOutcome::Success),
            None
        );
        assert_eq!(
            probe_phase_exit_code(ProbePhaseOutcome::RejectedOutput),
            Some(0)
        );
        assert_eq!(
            probe_phase_diagnostic_code(ProbePhaseOutcome::RejectedOutput),
            Some(901)
        );
        assert_eq!(
            probe_phase_exit_code(ProbePhaseOutcome::Error(CommandError::Exited(17))),
            Some(17)
        );
        assert_eq!(
            probe_phase_diagnostic_code(ProbePhaseOutcome::Error(CommandError::Exited(17))),
            Some(1017)
        );
        assert_eq!(
            probe_phase_diagnostic_code(ProbePhaseOutcome::Error(CommandError::Cancelled)),
            None
        );
    }

    #[test]
    fn native_probe_post_command_poll_dominates_every_receipt_path() {
        assert!(native_probe_post_command_authority_current(true, true));
        for (runtime_valid, control_current) in [(false, true), (true, false), (false, false)] {
            assert!(!native_probe_post_command_authority_current(
                runtime_valid,
                control_current
            ));
        }
    }

    #[test]
    fn native_probe_codex_invocations_disable_cwd_ancestor_project_discovery() {
        let login = native_provider_probe_login_args();
        assert_eq!(
            login
                .windows(2)
                .filter(|pair| pair == &["--config", "project_root_markers=[]"])
                .count(),
            1
        );
        assert_eq!(&login[4..], &["login", "status"]);

        let exec = native_provider_probe_exec_args("schema.json");
        assert_eq!(
            exec.windows(2)
                .filter(|pair| pair == &["--config", "project_root_markers=[]"])
                .count(),
            1
        );
        assert!(!exec.contains(&"--cd"));
    }

    #[test]
    fn native_login_stderr_classification_is_closed_numeric_and_content_free() {
        for (stderr, expected) in [
            (
                b"FAILED TO READ REQUIREMENTS FILE C:\\ProgramData\\OpenAI\\Codex\\requirements.toml token=private; not logged in"
                    .as_slice(),
                NativeProbeLoginDiagnostic::RequirementsFileReadFailed,
            ),
            (
                b"Failed to read config file C:\\ProgramData\\OpenAI\\Codex\\config.toml?secret=private; permission denied; failed to load configuration"
                    .as_slice(),
                NativeProbeLoginDiagnostic::ConfigFileReadFailed,
            ),
            (
                b"AUTHENTICATION REQUIREMENTS DO NOT PERMIT ANY USABLE LOGIN METHOD token=private; not logged in"
                    .as_slice(),
                NativeProbeLoginDiagnostic::AuthenticationRequirementsNoUsableLoginMethod,
            ),
            (
                b"failed to canonicalize CODEX_HOME: Access is denied. (os error 5)"
                    .as_slice(),
                NativeProbeLoginDiagnostic::CodexHomeCanonicalizationFailed,
            ),
            (
                b"failed to read CODEX_HOME: permission denied".as_slice(),
                NativeProbeLoginDiagnostic::CodexHomeReadFailed,
            ),
            (
                b"C:\\ProgramData\\OpenAI\\Codex\\managed.toml?token=secret: ACCESS IS DENIED"
                    .as_slice(),
                NativeProbeLoginDiagnostic::SystemCodexPathAccessDenied,
            ),
            (
                b"C:\\ProgramData\\Assemblywright\\planning-runtime\\provider\\codex-home\\auth.json?token=secret: permission denied"
                    .as_slice(),
                NativeProbeLoginDiagnostic::ProviderCodexHomeAccessDenied,
            ),
            (
                b"C:\\ProgramData\\Assemblywright\\planning-runtime\\provider\\local-app-data\\state?token=secret: os error 5"
                    .as_slice(),
                NativeProbeLoginDiagnostic::ProviderLocalAppDataAccessDenied,
            ),
            (
                b"C:\\ProgramData\\Assemblywright\\planning-runtime\\provider\\temp\\state?token=secret: access is denied"
                    .as_slice(),
                NativeProbeLoginDiagnostic::ProviderTempAccessDenied,
            ),
            (
                b"C:\\ProgramData\\Assemblywright\\planning-runtime\\provider\\other?token=secret: permission denied"
                    .as_slice(),
                NativeProbeLoginDiagnostic::ProviderRootOrCurrentDirectoryAccessDenied,
            ),
            (
                b"Current working directory contained token=secret: os error 5".as_slice(),
                NativeProbeLoginDiagnostic::ProviderRootOrCurrentDirectoryAccessDenied,
            ),
            (
                b"Failed to load configuration C:\\private\\config.toml?token=secret: ACCESS IS DENIED (os error 5)"
                    .as_slice(),
                NativeProbeLoginDiagnostic::ConfigurationLoadFailed,
            ),
            (
                b"Failed to load configuration C:\\ProgramData\\OpenAI\\Codex\\config.toml?token=secret"
                    .as_slice(),
                NativeProbeLoginDiagnostic::ConfigurationLoadFailed,
            ),
            (
                b"Not logged in. Run codex login.".as_slice(),
                NativeProbeLoginDiagnostic::NotLoggedIn,
            ),
            (
                b"Failed to load configuration: access denied".as_slice(),
                NativeProbeLoginDiagnostic::ConfigurationLoadFailed,
            ),
            (
                b"Login status request failed".as_slice(),
                NativeProbeLoginDiagnostic::LoginStatusFailed,
            ),
            (
                b"unrecognized failure with secret-shaped material".as_slice(),
                NativeProbeLoginDiagnostic::Other,
            ),
            (
                b"process launch failed status=5".as_slice(),
                NativeProbeLoginDiagnostic::ProcessLaunchAccessDenied,
            ),
            (
                b"process launch failed status=193".as_slice(),
                NativeProbeLoginDiagnostic::ProcessLaunchFailed,
            ),
        ] {
            assert_eq!(classify_native_probe_login_stderr(stderr), expected);
        }
        assert_eq!(
            classify_native_probe_login_stderr(
                b"containment setup failed stage=6 status=5 private material is absent",
            ),
            NativeProbeLoginDiagnostic::ContainmentStage(6),
        );
        assert_eq!(NativeProbeLoginDiagnostic::ContainmentStage(6).code(), 966);
        assert_eq!(
            [
                NativeProbeLoginDiagnostic::NotLoggedIn.code(),
                NativeProbeLoginDiagnostic::ConfigurationLoadFailed.code(),
                NativeProbeLoginDiagnostic::LoginStatusFailed.code(),
                NativeProbeLoginDiagnostic::Other.code(),
                NativeProbeLoginDiagnostic::RequirementsFileReadFailed.code(),
                NativeProbeLoginDiagnostic::ConfigFileReadFailed.code(),
                NativeProbeLoginDiagnostic::AuthenticationRequirementsNoUsableLoginMethod.code(),
                NativeProbeLoginDiagnostic::AccessDeniedIo.code(),
                NativeProbeLoginDiagnostic::SystemCodexPathAccessDenied.code(),
                NativeProbeLoginDiagnostic::ProviderCodexHomeAccessDenied.code(),
                NativeProbeLoginDiagnostic::ProviderLocalAppDataAccessDenied.code(),
                NativeProbeLoginDiagnostic::ProviderTempAccessDenied.code(),
                NativeProbeLoginDiagnostic::ProviderRootOrCurrentDirectoryAccessDenied.code(),
                NativeProbeLoginDiagnostic::CodexHomeCanonicalizationFailed.code(),
                NativeProbeLoginDiagnostic::CodexHomeReadFailed.code(),
                NativeProbeLoginDiagnostic::ProcessLaunchAccessDenied.code(),
                NativeProbeLoginDiagnostic::ProcessLaunchFailed.code(),
            ],
            [
                920, 921, 922, 923, 924, 925, 926, 927, 928, 929, 930, 931, 932, 933, 934, 935,
                936,
            ]
        );

        let captured = Arc::new(Mutex::new(
            b"C:\\ProgramData\\Assemblywright\\planning-runtime\\provider\\temp\\private?token=secret: permission denied"
                .to_vec(),
        ));
        assert_eq!(
            native_probe_login_diagnostic(&captured),
            NativeProbeLoginDiagnostic::ProviderTempAccessDenied
        );
        assert!(captured.lock().unwrap().is_empty());
    }

    #[test]
    fn only_exact_native_probe_commands_may_capture_bounded_stderr() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let mode = CommandStderrMode::CaptureBounded {
            max_bytes: NATIVE_PROVIDER_PROBE_LOGIN_STDERR_MAX_BYTES,
            output: Arc::clone(&output),
        };
        assert!(mode.valid_for(&native_provider_probe_login_args()));
        assert!(mode.allows_empty_stdout(&native_provider_probe_login_args()));
        assert!(!mode.valid_for(&native_provider_probe_exec_args("schema.json")));
        assert!(!mode.allows_empty_stdout(&native_provider_probe_exec_args("schema.json")));

        let exec_mode = CommandStderrMode::CaptureBounded {
            max_bytes: NATIVE_PROVIDER_PROBE_EXEC_STDERR_MAX_BYTES,
            output: Arc::clone(&output),
        };
        let exec_args = native_provider_probe_exec_args("schema.json");
        assert!(exec_mode.valid_for(&exec_args));
        assert!(!exec_mode.allows_empty_stdout(&exec_args));
        assert!(!exec_mode.valid_for(&native_provider_probe_login_args()));
        assert!(!exec_mode.valid_for(&["exec", "-"]));
        assert!(CommandStderrMode::default().valid_for(&[]));

        let oversized = CommandStderrMode::CaptureBounded {
            max_bytes: NATIVE_PROVIDER_PROBE_LOGIN_STDERR_MAX_BYTES + 1,
            output,
        };
        assert!(!oversized.valid_for(&native_provider_probe_login_args()));
    }

    #[test]
    fn native_exec_stderr_classification_is_closed_numeric_and_content_free() {
        assert_eq!(
            classify_native_probe_exec_stderr(
                b"containment setup failed stage=15 status=87 private material is absent",
            ),
            Some(NativeProbeExecDiagnostic::ContainmentStage(15)),
        );
        assert_eq!(NativeProbeExecDiagnostic::ContainmentStage(15).code(), 1005);
        for (stderr, expected) in [
            (
                b"Failed to load configuration: private path".as_slice(),
                Some(NativeProbeExecDiagnostic::ConfigurationLoadFailed),
            ),
            (
                b"Not logged in. Run codex login.".as_slice(),
                Some(NativeProbeExecDiagnostic::NotLoggedIn),
            ),
            (
                b"request failed: status: 401 token=private".as_slice(),
                Some(NativeProbeExecDiagnostic::AuthenticationFailed),
            ),
            (
                b"failed to connect: dns error".as_slice(),
                Some(NativeProbeExecDiagnostic::NetworkUnavailable),
            ),
            (
                b"error: unexpected argument '--private'".as_slice(),
                Some(NativeProbeExecDiagnostic::CliArgumentRejected),
            ),
            (
                b"invalid output schema at private path".as_slice(),
                Some(NativeProbeExecDiagnostic::OutputSchemaRejected),
            ),
            (
                b"model is not available for private account".as_slice(),
                Some(NativeProbeExecDiagnostic::ModelUnavailable),
            ),
            (
                b"failed to canonicalize CODEX_HOME: Access is denied. (os error 5)"
                    .as_slice(),
                Some(NativeProbeExecDiagnostic::CodexHomeCanonicalizationFailed),
            ),
            (
                b"failed to canonicalize current directory: permission denied".as_slice(),
                Some(NativeProbeExecDiagnostic::ExplicitCwdCanonicalizationFailed),
            ),
            (
                b"C:\\ProgramData\\OpenAI\\Codex\\requirements.toml: access is denied"
                    .as_slice(),
                Some(NativeProbeExecDiagnostic::SystemCodexPathAccessDenied),
            ),
            (
                b"C:\\ProgramData\\Assemblywright\\planning-runtime\\provider\\codex-home\\auth.json: access is denied"
                    .as_slice(),
                Some(NativeProbeExecDiagnostic::ProviderCodexHomeAccessDenied),
            ),
            (
                b"C:\\ProgramData\\Assemblywright\\planning-runtime\\provider\\local-app-data\\state: access is denied"
                    .as_slice(),
                Some(NativeProbeExecDiagnostic::ProviderLocalAppDataAccessDenied),
            ),
            (
                b"C:\\ProgramData\\Assemblywright\\planning-runtime\\provider\\temp\\state: access is denied"
                    .as_slice(),
                Some(NativeProbeExecDiagnostic::ProviderTempAccessDenied),
            ),
            (
                b"C:\\ProgramData\\Assemblywright\\planning-runtime\\provider: access is denied"
                    .as_slice(),
                Some(NativeProbeExecDiagnostic::ProviderRootOrCurrentDirectoryAccessDenied),
            ),
            (
                b"private path: access is denied (os error 5)".as_slice(),
                Some(NativeProbeExecDiagnostic::AccessDeniedIo),
            ),
            (
                b"unrecognized failure with secret-shaped material".as_slice(),
                Some(NativeProbeExecDiagnostic::Other),
            ),
            (
                b"process launch failed status=5".as_slice(),
                Some(NativeProbeExecDiagnostic::ProcessLaunchAccessDenied),
            ),
            (
                b"process launch failed status=193".as_slice(),
                Some(NativeProbeExecDiagnostic::ProcessLaunchFailed),
            ),
            (b"".as_slice(), None),
        ] {
            assert_eq!(classify_native_probe_exec_stderr(stderr), expected);
        }
        assert_eq!(
            [
                NativeProbeExecDiagnostic::ConfigurationLoadFailed.code(),
                NativeProbeExecDiagnostic::NotLoggedIn.code(),
                NativeProbeExecDiagnostic::AuthenticationFailed.code(),
                NativeProbeExecDiagnostic::NetworkUnavailable.code(),
                NativeProbeExecDiagnostic::CliArgumentRejected.code(),
                NativeProbeExecDiagnostic::OutputSchemaRejected.code(),
                NativeProbeExecDiagnostic::ModelUnavailable.code(),
                NativeProbeExecDiagnostic::SystemCodexPathAccessDenied.code(),
                NativeProbeExecDiagnostic::ProviderCodexHomeAccessDenied.code(),
                NativeProbeExecDiagnostic::ProviderLocalAppDataAccessDenied.code(),
                NativeProbeExecDiagnostic::ProviderTempAccessDenied.code(),
                NativeProbeExecDiagnostic::ProviderRootOrCurrentDirectoryAccessDenied.code(),
                NativeProbeExecDiagnostic::CodexHomeCanonicalizationFailed.code(),
                NativeProbeExecDiagnostic::ExplicitCwdCanonicalizationFailed.code(),
                NativeProbeExecDiagnostic::AccessDeniedIo.code(),
                NativeProbeExecDiagnostic::Other.code(),
                NativeProbeExecDiagnostic::ProcessLaunchAccessDenied.code(),
                NativeProbeExecDiagnostic::ProcessLaunchFailed.code(),
            ],
            [
                940, 941, 942, 943, 944, 945, 946, 947, 948, 949, 950, 951, 954, 955, 952, 953,
                956, 957,
            ]
        );

        let captured = Arc::new(Mutex::new(b"failed to connect with token=private".to_vec()));
        assert_eq!(
            native_probe_exec_diagnostic(&captured),
            Some(NativeProbeExecDiagnostic::NetworkUnavailable)
        );
        assert!(captured.lock().unwrap().is_empty());
    }

    #[test]
    fn exited_parent_cannot_leave_stdout_inheriting_descendant_or_block_return() {
        let directory = tempfile::tempdir().unwrap();
        let executable_path = directory.path().join("adversarial-provider");
        fs::write(
            &executable_path,
            b"#!/usr/bin/perl\nuse strict; use warnings; while (<STDIN>) {} my $pid = fork(); die 'fork' unless defined $pid; if ($pid == 0) { sleep 30; exit 0; } open(my $file, '>', 'descendant.pid') or die 'pid'; print {$file} $pid; close($file); print STDOUT 'complete'; exit 0;\n",
        )
        .unwrap();
        fs::set_permissions(&executable_path, fs::Permissions::from_mode(0o700)).unwrap();
        let expected = hex(&Sha256::digest(fs::read(&executable_path).unwrap()));
        let executable = load_executable(&executable_path, &expected).unwrap();
        let control = PlanningEffectControl::new(
            Arc::new(AtomicBool::new(false)),
            Instant::now() + Duration::from_secs(5),
        );

        let working_directory = directory.path().to_path_buf();
        let descendant_path = working_directory.join("descendant.pid");
        let (completed_tx, completed_rx) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let output = run_command(
                &executable,
                &control,
                CommandInvocation {
                    current_dir: &working_directory,
                    args: &[],
                    environment: &[],
                    input: b"untrusted planning request",
                    max_output: 64,
                    isolation: &ProcessIsolation::unrestricted_test_runtime(),
                    stderr: CommandStderrMode::default(),
                },
            );
            completed_tx.send(()).unwrap();
            output
        });
        if completed_rx.recv_timeout(Duration::from_secs(10)).is_err() {
            if let Ok(pid) = fs::read_to_string(&descendant_path) {
                if let Ok(pid) = pid.parse::<i32>() {
                    unsafe { libc::kill(pid, libc::SIGKILL) };
                }
            }
            let _ = worker.join();
            panic!("run_command did not return while the 30-second descendant held stdout");
        }
        let output = worker.join().unwrap().unwrap();
        assert_eq!(output, b"complete");

        let descendant: i32 = fs::read_to_string(descendant_path)
            .unwrap()
            .parse()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while unsafe { libc::kill(descendant, 0) } == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(unsafe { libc::kill(descendant, 0) }, -1);
    }
}
