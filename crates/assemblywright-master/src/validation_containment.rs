//! Default-inert Windows containment for Feature Conveyor validation.
//!
//! Callers select only protocol-owned command identifiers. Executables, argv,
//! environment, current directory, output bounds, and termination behavior are
//! owned here. The authoritative frozen candidate is never executable: callers
//! must first prove a distinct, clean, no-remote disposable copy of its exact
//! commit and tree.

use assemblywright_protocol::FeatureConveyorValidationCommandId;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Duration;

#[cfg(windows)]
const MAX_CAPTURE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationFixtureCommand {
    ReadWriteAndEnvironment,
    BoundedOutput,
    TimeoutChildTree,
    DeniedOutsideRoot,
    NetworkDenied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationFixtureResult {
    pub exit_code: u32,
    pub stdout_len: usize,
    pub stderr_len: usize,
    pub stdout_sha256: [u8; 32],
    pub stderr_sha256: [u8; 32],
    pub timed_out: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationCommandExecution {
    ContainedProcess,
    InternalDeterministicCheck,
    ExternalPlatformEvidence,
}

/// Classifies the closed protocol command without authorizing execution.
pub fn validation_command_execution(
    command: FeatureConveyorValidationCommandId,
) -> ValidationCommandExecution {
    use FeatureConveyorValidationCommandId as Command;
    match command {
        Command::FocusedUnitTests
        | Command::Formatting
        | Command::Lint
        | Command::Build
        | Command::RepositoryValidation => ValidationCommandExecution::ContainedProcess,
        Command::RequirementsBinding
        | Command::Documentation
        | Command::KnowledgeBase
        | Command::Safety
        | Command::ChangedPaths
        | Command::SecretScan => ValidationCommandExecution::InternalDeterministicCheck,
        Command::Coverage | Command::NativeE2e => ValidationCommandExecution::ContainedProcess,
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedValidationCopy {
    root: PathBuf,
    candidate_commit: git2::Oid,
    candidate_tree: git2::Oid,
}

impl VerifiedValidationCopy {
    pub fn verify(
        disposable_root: &Path,
        candidate_commit: &str,
        candidate_tree: &str,
    ) -> Result<Self, ValidationContainmentError> {
        let root = canonical_directory_without_link(disposable_root)
            .map_err(|_| ValidationContainmentError::InvalidValidationCopy)?;
        let candidate_commit = git2::Oid::from_str(candidate_commit)
            .map_err(|_| ValidationContainmentError::InvalidValidationCopy)?;
        let candidate_tree = git2::Oid::from_str(candidate_tree)
            .map_err(|_| ValidationContainmentError::InvalidValidationCopy)?;
        let verified = Self {
            root,
            candidate_commit,
            candidate_tree,
        };
        verified.revalidate()?;
        Ok(verified)
    }

    pub fn revalidate(&self) -> Result<(), ValidationContainmentError> {
        let root = canonical_directory_without_link(&self.root)
            .map_err(|_| ValidationContainmentError::CandidateDrift)?;
        if root != self.root {
            return Err(ValidationContainmentError::CandidateDrift);
        }
        let dot_git = root.join(".git");
        let metadata = std::fs::symlink_metadata(&dot_git)
            .map_err(|_| ValidationContainmentError::CandidateDrift)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ValidationContainmentError::CandidateDrift);
        }
        let repository = git2::Repository::open(&root)
            .map_err(|_| ValidationContainmentError::CandidateDrift)?;
        let workdir = repository
            .workdir()
            .and_then(|path| path.canonicalize().ok())
            .ok_or(ValidationContainmentError::CandidateDrift)?;
        if workdir != root
            || !repository
                .remotes()
                .map_err(|_| ValidationContainmentError::CandidateDrift)?
                .is_empty()
        {
            return Err(ValidationContainmentError::CandidateDrift);
        }
        let head = repository
            .head()
            .map_err(|_| ValidationContainmentError::CandidateDrift)?;
        if head.target() != Some(self.candidate_commit) {
            return Err(ValidationContainmentError::CandidateDrift);
        }
        let commit = repository
            .find_commit(self.candidate_commit)
            .map_err(|_| ValidationContainmentError::CandidateDrift)?;
        if commit.tree_id() != self.candidate_tree {
            return Err(ValidationContainmentError::CandidateDrift);
        }
        let mut options = git2::StatusOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);
        if !repository
            .statuses(Some(&mut options))
            .map_err(|_| ValidationContainmentError::CandidateDrift)?
            .is_empty()
        {
            return Err(ValidationContainmentError::CandidateDrift);
        }
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct ValidationToolchainConfig {
    source_root: PathBuf,
    dependency_cache_seed: PathBuf,
}

impl ValidationToolchainConfig {
    /// Resolves a credential-free Rust toolchain and an offline dependency-cache
    /// seed. Both are copied into attempt-private staging before child launch;
    /// the shared inputs are never granted to the AppContainer.
    pub fn resolve(
        source_root: &Path,
        dependency_cache_seed: &Path,
    ) -> Result<Self, ValidationContainmentError> {
        let source_root = canonical_directory_without_link(source_root)
            .map_err(|_| ValidationContainmentError::InvalidToolchain)?;
        let dependency_cache_seed = canonical_directory_without_link(dependency_cache_seed)
            .map_err(|_| ValidationContainmentError::PrivateDependencyCacheUnavailable)?;
        for name in [
            "cargo.exe",
            "cargo-llvm-cov.exe",
            "cargo-clippy.exe",
            "cargo-fmt.exe",
            "rustc.exe",
            "rustfmt.exe",
        ] {
            let path = source_root.join("bin").join(name);
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|_| ValidationContainmentError::InvalidToolchain)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(ValidationContainmentError::InvalidToolchain);
            }
            let canonical = path
                .canonicalize()
                .map_err(|_| ValidationContainmentError::InvalidToolchain)?;
            if !canonical.starts_with(&source_root) {
                return Err(ValidationContainmentError::InvalidToolchain);
            }
        }
        validate_copy_tree(&source_root, 200_000, 4 * 1024 * 1024 * 1024)
            .map_err(|_| ValidationContainmentError::InvalidToolchain)?;
        validate_copy_tree(&dependency_cache_seed, 200_000, 4 * 1024 * 1024 * 1024)
            .map_err(|_| ValidationContainmentError::PrivateDependencyCacheUnavailable)?;
        validate_dependency_cache_seed(&dependency_cache_seed)
            .map_err(|_| ValidationContainmentError::PrivateDependencyCacheUnavailable)?;
        Ok(Self {
            source_root,
            dependency_cache_seed,
        })
    }

    pub fn revalidate(&self) -> Result<(), ValidationContainmentError> {
        let current = Self::resolve(&self.source_root, &self.dependency_cache_seed)?;
        if current.source_root != self.source_root
            || current.dependency_cache_seed != self.dependency_cache_seed
        {
            return Err(ValidationContainmentError::InvalidToolchain);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationProcessResult {
    pub command_id: FeatureConveyorValidationCommandId,
    pub exit_code: u32,
    pub stdout_len: usize,
    pub stderr_len: usize,
    pub stdout_sha256: [u8; 32],
    pub stderr_sha256: [u8; 32],
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalValidationResult {
    pub command_id: FeatureConveyorValidationCommandId,
    pub passed: bool,
    pub result_sha256: [u8; 32],
}

/// Executes one master-owned deterministic check over the exact disposable
/// candidate. Only aggregate pass/digest evidence leaves this boundary.
pub fn run_internal_validation_check(
    command: FeatureConveyorValidationCommandId,
    candidate: &VerifiedValidationCopy,
    base_commit: &str,
    approved_paths: &[String],
    acceptance_criteria_count: u64,
    requirements_sha256: [u8; 32],
) -> Result<InternalValidationResult, ValidationContainmentError> {
    if validation_command_execution(command)
        != ValidationCommandExecution::InternalDeterministicCheck
    {
        return Err(ValidationContainmentError::InternalCheckRequired);
    }
    candidate.revalidate()?;
    let repository = git2::Repository::open(candidate.root())
        .map_err(|_| ValidationContainmentError::CandidateDrift)?;
    let base =
        git2::Oid::from_str(base_commit).map_err(|_| ValidationContainmentError::CandidateDrift)?;
    let base_commit = repository
        .find_commit(base)
        .map_err(|_| ValidationContainmentError::CandidateDrift)?;
    let candidate_commit = repository
        .find_commit(candidate.candidate_commit)
        .map_err(|_| ValidationContainmentError::CandidateDrift)?;
    if candidate_commit.parent_count() != 1 || candidate_commit.parent_id(0).ok() != Some(base) {
        return Err(ValidationContainmentError::CandidateDrift);
    }
    let base_tree = base_commit
        .tree()
        .map_err(|_| ValidationContainmentError::CandidateDrift)?;
    let candidate_tree = candidate_commit
        .tree()
        .map_err(|_| ValidationContainmentError::CandidateDrift)?;
    let diff = repository
        .diff_tree_to_tree(Some(&base_tree), Some(&candidate_tree), None)
        .map_err(|_| ValidationContainmentError::CandidateDrift)?;
    let mut changed = BTreeSet::new();
    for delta in diff.deltas() {
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .ok_or(ValidationContainmentError::CandidateDrift)?;
        let path = path
            .to_str()
            .ok_or(ValidationContainmentError::CandidateDrift)?;
        if path.is_empty()
            || path.starts_with('/')
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == ".." || part == ".git")
        {
            return Err(ValidationContainmentError::CandidateDrift);
        }
        changed.insert(path.to_string());
    }
    let approved: BTreeSet<_> = approved_paths.iter().cloned().collect();
    let docs = changed.iter().any(|path| path.ends_with(".md"));
    let knowledge = changed
        .iter()
        .any(|path| path.starts_with("docs/knowledge-base/") && path.ends_with(".md"));
    let safety = changed.contains("docs/safety-rules.md") || changed.contains("DESIGN.md");
    let secret_matches = if command == FeatureConveyorValidationCommandId::SecretScan {
        scan_changed_files_for_secrets(candidate.root(), &changed)?
    } else {
        0
    };
    let passed = match command {
        FeatureConveyorValidationCommandId::RequirementsBinding => {
            requirements_sha256 != [0; 32]
                && acceptance_criteria_count > 0
                && !changed.is_empty()
                && changed == approved
        }
        FeatureConveyorValidationCommandId::Documentation => docs,
        FeatureConveyorValidationCommandId::KnowledgeBase => knowledge,
        FeatureConveyorValidationCommandId::Safety => safety,
        FeatureConveyorValidationCommandId::ChangedPaths => {
            !changed.is_empty() && changed == approved
        }
        FeatureConveyorValidationCommandId::SecretScan => secret_matches == 0,
        _ => return Err(ValidationContainmentError::InternalCheckRequired),
    };
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.internal-validation.v1\0");
    digest.update(serde_json::to_vec(&command).map_err(|_| ValidationContainmentError::Failed)?);
    digest.update(base.as_bytes());
    digest.update(candidate.candidate_commit.as_bytes());
    digest.update(requirements_sha256);
    digest.update(acceptance_criteria_count.to_be_bytes());
    digest.update((changed.len() as u64).to_be_bytes());
    for path in &changed {
        digest.update((path.len() as u64).to_be_bytes());
        digest.update(path.as_bytes());
    }
    digest.update(secret_matches.to_be_bytes());
    digest.update([u8::from(passed)]);
    candidate.revalidate()?;
    Ok(InternalValidationResult {
        command_id: command,
        passed,
        result_sha256: digest.finalize().into(),
    })
}

fn scan_changed_files_for_secrets(
    root: &Path,
    changed: &BTreeSet<String>,
) -> Result<u64, ValidationContainmentError> {
    const PRIVATE_KEY: &[u8] = &[
        45, 45, 45, 45, 45, 66, 69, 71, 73, 78, 32, 80, 82, 73, 86, 65, 84, 69, 32, 75, 69, 89, 45,
        45, 45, 45, 45,
    ];
    const RSA_PRIVATE_KEY: &[u8] = &[
        45, 45, 45, 45, 45, 66, 69, 71, 73, 78, 32, 82, 83, 65, 32, 80, 82, 73, 86, 65, 84, 69, 32,
        75, 69, 89, 45, 45, 45, 45, 45,
    ];
    const OPENSSH_PRIVATE_KEY: &[u8] = &[
        45, 45, 45, 45, 45, 66, 69, 71, 73, 78, 32, 79, 80, 69, 78, 83, 83, 72, 32, 80, 82, 73, 86,
        65, 84, 69, 32, 75, 69, 89, 45, 45, 45, 45, 45,
    ];
    const GITHUB_CLASSIC: &[u8] = &[103, 104, 112, 95];
    const GITHUB_FINE_GRAINED: &[u8] = &[103, 105, 116, 104, 117, 98, 95, 112, 97, 116, 95];
    const LIVE_SECRET: &[u8] = &[115, 107, 45, 108, 105, 118, 101, 45];
    const PATTERNS: &[&[u8]] = &[
        PRIVATE_KEY,
        RSA_PRIVATE_KEY,
        OPENSSH_PRIVATE_KEY,
        GITHUB_CLASSIC,
        GITHUB_FINE_GRAINED,
        LIVE_SECRET,
    ];
    let mut matches = 0u64;
    for relative in changed {
        let lower_name = relative.to_ascii_lowercase();
        let leaf = lower_name.rsplit('/').next().unwrap_or(&lower_name);
        if matches!(
            leaf,
            ".env" | "credentials" | "credentials.toml" | "id_rsa" | "id_ed25519"
        ) {
            matches = matches
                .checked_add(1)
                .ok_or(ValidationContainmentError::Failed)?;
        }
        let path = root.join(relative);
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(ValidationContainmentError::CandidateDrift),
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(ValidationContainmentError::CandidateDrift)
            }
            Ok(metadata) if metadata.len() > 32 * 1024 * 1024 => {
                return Err(ValidationContainmentError::CandidateDrift)
            }
            Ok(_) => {}
        }
        let bytes = std::fs::read(path).map_err(|_| ValidationContainmentError::CandidateDrift)?;
        for pattern in PATTERNS {
            let count = bytes
                .windows(pattern.len())
                .filter(|window| *window == *pattern)
                .count() as u64;
            matches = matches
                .checked_add(count)
                .ok_or(ValidationContainmentError::Failed)?;
        }
    }
    Ok(matches)
}

#[derive(Debug, Clone)]
pub struct ValidationCancellation(Arc<AtomicBool>);

impl ValidationCancellation {
    pub fn new(signal: Arc<AtomicBool>) -> Self {
        Self(signal)
    }

    fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Acquire)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationContainmentError {
    #[error("validation containment is unsupported on this platform")]
    Unsupported,
    #[error("validation containment rejected its execution root")]
    InvalidExecutionRoot,
    #[error("validation rejected a non-disposable or unverified candidate copy")]
    InvalidValidationCopy,
    #[error("validation candidate identity or worktree state drifted")]
    CandidateDrift,
    #[error("validation rejected its trusted toolchain")]
    InvalidToolchain,
    #[error("validation has no credential-free private offline dependency cache")]
    PrivateDependencyCacheUnavailable,
    #[error("validation command requires a master-internal deterministic check")]
    InternalCheckRequired,
    #[error("validation command requires separately bound platform evidence")]
    ExternalPlatformEvidenceRequired,
    #[error("validation execution was cancelled and its job tree was reaped")]
    Cancelled,
    #[error("validation containment setup or execution failed")]
    Failed,
    #[error("validation containment failed at normalized stage {0}")]
    Stage(&'static str),
    #[error("validation containment failed at normalized stage {0} with platform status {1}")]
    StageStatus(&'static str, i32),
    #[error("validation fixture output exceeded its fixed bound")]
    OutputLimitExceeded,
}

/// Returns the exact protocol-owned Cargo arguments for a contained command.
/// Callers cannot add executable names, arguments, paths, or shell text.
pub fn validation_command_arguments(
    command: FeatureConveyorValidationCommandId,
) -> Result<String, ValidationContainmentError> {
    use FeatureConveyorValidationCommandId as Command;
    let arguments = match command {
        Command::Coverage => format!(
            "llvm-cov --workspace --all-targets --all-features --offline --locked \
             --summary-only --fail-under-lines {}",
            assemblywright_protocol::FEATURE_CONVEYOR_MINIMUM_LINE_COVERAGE_PERCENT
        ),
        Command::FocusedUnitTests => {
            "test --workspace --lib --bins --offline --locked --no-fail-fast".to_string()
        }
        Command::NativeE2e => {
            "test --workspace --tests --all-features --offline --locked --no-fail-fast".to_string()
        }
        Command::Formatting => "fmt --all -- --check".to_string(),
        Command::Lint => {
            "clippy --workspace --all-targets --all-features --offline --locked -- -D warnings"
                .to_string()
        }
        Command::Build => {
            "build --workspace --all-targets --all-features --offline --locked".to_string()
        }
        Command::RepositoryValidation => {
            "test --workspace --all-targets --all-features --offline --locked --no-fail-fast"
                .to_string()
        }
        _ => return Err(ValidationContainmentError::InternalCheckRequired),
    };
    Ok(arguments)
}

/// Runs one exact validation fixture. This is not a general process launcher.
pub fn run_validation_fixture(
    fixture: ValidationFixtureCommand,
    execution_root: &Path,
    timeout: Duration,
) -> Result<ValidationFixtureResult, ValidationContainmentError> {
    platform::run(fixture, execution_root, timeout)
}

/// Test-only boundary probe for the same parent-side cancellation path used by
/// production validation. Cancellation never executes in the child.
pub fn run_validation_fixture_with_cancellation(
    fixture: ValidationFixtureCommand,
    execution_root: &Path,
    timeout: Duration,
    cancellation: &ValidationCancellation,
) -> Result<ValidationFixtureResult, ValidationContainmentError> {
    platform::run_fixture_cancellable(fixture, execution_root, timeout, cancellation)
}

/// Executes one process-backed protocol command with no caller-controlled
/// executable, argv, environment, or working directory.
pub fn run_validation_command(
    command: FeatureConveyorValidationCommandId,
    candidate: &VerifiedValidationCopy,
    toolchain: &ValidationToolchainConfig,
    timeout: Duration,
    cancellation: &ValidationCancellation,
) -> Result<ValidationProcessResult, ValidationContainmentError> {
    match validation_command_execution(command) {
        ValidationCommandExecution::InternalDeterministicCheck => {
            return Err(ValidationContainmentError::InternalCheckRequired)
        }
        ValidationCommandExecution::ExternalPlatformEvidence => {
            return Err(ValidationContainmentError::ExternalPlatformEvidenceRequired)
        }
        ValidationCommandExecution::ContainedProcess => {}
    }
    if cancellation.is_cancelled() {
        return Err(ValidationContainmentError::Cancelled);
    }
    candidate.revalidate()?;
    platform::run_production(command, candidate, toolchain, timeout, cancellation)
}

fn canonical_directory_without_link(path: &Path) -> Result<PathBuf, ()> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| ())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(());
    }
    path.canonicalize().map_err(|_| ())
}

fn validate_copy_tree(root: &Path, max_entries: usize, max_bytes: u64) -> Result<(), ()> {
    let mut stack = vec![root.to_path_buf()];
    let mut entries = 0usize;
    let mut bytes = 0u64;
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(directory).map_err(|_| ())? {
            let entry = entry.map_err(|_| ())?;
            entries = entries.checked_add(1).ok_or(())?;
            if entries > max_entries {
                return Err(());
            }
            let metadata = entry.metadata().map_err(|_| ())?;
            let file_type = entry.file_type().map_err(|_| ())?;
            if file_type.is_symlink() {
                return Err(());
            }
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                bytes = bytes.checked_add(metadata.len()).ok_or(())?;
                if bytes > max_bytes {
                    return Err(());
                }
            } else {
                return Err(());
            }
        }
    }
    Ok(())
}

fn validate_dependency_cache_seed(root: &Path) -> Result<(), ()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(directory).map_err(|_| ())? {
            let entry = entry.map_err(|_| ())?;
            let file_type = entry.file_type().map_err(|_| ())?;
            if file_type.is_dir() {
                stack.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                return Err(());
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if matches!(
                name.as_str(),
                "credentials"
                    | "credentials.toml"
                    | ".git-credentials"
                    | ".netrc"
                    | "config"
                    | "config.toml"
                    | "id_rsa"
                    | "id_ed25519"
            ) {
                return Err(());
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub(super) fn run(
        _fixture: ValidationFixtureCommand,
        _execution_root: &Path,
        _timeout: Duration,
    ) -> Result<ValidationFixtureResult, ValidationContainmentError> {
        Err(ValidationContainmentError::Unsupported)
    }

    pub(super) fn run_production(
        _command: FeatureConveyorValidationCommandId,
        _candidate: &VerifiedValidationCopy,
        _toolchain: &ValidationToolchainConfig,
        _timeout: Duration,
        _cancellation: &ValidationCancellation,
    ) -> Result<ValidationProcessResult, ValidationContainmentError> {
        Err(ValidationContainmentError::Unsupported)
    }

    pub(super) fn run_fixture_cancellable(
        _fixture: ValidationFixtureCommand,
        _execution_root: &Path,
        _timeout: Duration,
        _cancellation: &ValidationCancellation,
    ) -> Result<ValidationFixtureResult, ValidationContainmentError> {
        Err(ValidationContainmentError::Unsupported)
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::ffi::{c_void, OsStr};
    use std::fs::{self, File};
    use std::io::Read;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use std::ptr::{null, null_mut};
    use std::thread;
    use std::time::Instant;
    use windows_sys::Win32::Foundation::SetHandleInformation;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_INSUFFICIENT_BUFFER, HANDLE, HANDLE_FLAG_INHERIT,
        INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::Security::Isolation::{
        CreateAppContainerProfile, DeleteAppContainerProfile,
        DeriveAppContainerSidFromAppContainerName,
    };
    use windows_sys::Win32::Security::{
        CreateRestrictedToken, FreeSid, DISABLE_MAX_PRIVILEGE, PSID, SECURITY_ATTRIBUTES,
        SECURITY_CAPABILITIES, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
        JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
        TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
        JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateProcessAsUserW, DeleteProcThreadAttributeList, GetCurrentProcess,
        GetExitCodeProcess, InitializeProcThreadAttributeList, OpenProcessToken, ResumeThread,
        UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED,
        CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
        PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
        STARTF_USESTDHANDLES, STARTUPINFOEXW,
    };

    // AppContainer profile names are capped at 64 UTF-16 code units.
    const PROFILE_PREFIX: &str = "Assemblywright.Validation.";
    const TERMINATED_EXIT_CODE: u32 = 0xA55E_0001;
    const CANCELLED_EXIT_CODE: u32 = 0xA55E_0002;
    const HRESULT_PROFILE_ALREADY_EXISTS: i32 = 0x8007_00B7u32 as i32;
    const PROCESS_MEMORY_LIMIT: usize = 256 * 1024 * 1024;
    const JOB_MEMORY_LIMIT: usize = 512 * 1024 * 1024;

    pub(super) fn run_fixture_cancellable(
        fixture: ValidationFixtureCommand,
        execution_root: &Path,
        timeout: Duration,
        cancellation: &ValidationCancellation,
    ) -> Result<ValidationFixtureResult, ValidationContainmentError> {
        let root = execution_root
            .canonicalize()
            .map_err(|_| ValidationContainmentError::InvalidExecutionRoot)?;
        if !root.is_dir() || timeout.is_zero() || timeout.as_millis() > u32::MAX as u128 {
            return Err(ValidationContainmentError::InvalidExecutionRoot);
        }
        if cancellation.is_cancelled() {
            return Err(ValidationContainmentError::Cancelled);
        }
        let source_executable = std::env::current_exe()
            .map_err(|_| ValidationContainmentError::Stage("current_exe"))?
            .canonicalize()
            .map_err(|_| ValidationContainmentError::Stage("canonical_exe"))?;
        let mut profile = create_profile()?;
        let mut root_access = ExecutionRootAccess::grant(&root, profile.sid)
            .map_err(|_| ValidationContainmentError::Stage("root_acl"))?;
        let executable = root.join("assemblywright-validation-fixture.exe");
        let execution_result = (|| {
            fs::copy(&source_executable, &executable)
                .map_err(|_| ValidationContainmentError::Stage("copy_fixture"))?;
            fs::write(root.join("inheritance-probe.txt"), b"parent-only")
                .map_err(|_| ValidationContainmentError::Stage("inheritance_probe"))?;
            launch_process(
                &executable,
                exact_command_line(&executable, fixture),
                &root,
                minimal_environment(&root)?,
                timeout,
                Some(cancellation),
                profile.sid,
            )
        })();
        let acl_restored = root_access.restore_checked().is_ok();
        let profile_deleted = profile.delete_checked().is_ok();
        if !acl_restored {
            return Err(ValidationContainmentError::Stage("acl_restore"));
        }
        if !profile_deleted {
            return Err(ValidationContainmentError::Stage("profile_delete"));
        }
        execution_result
    }

    pub(super) fn run_production(
        command: FeatureConveyorValidationCommandId,
        candidate: &VerifiedValidationCopy,
        toolchain: &ValidationToolchainConfig,
        timeout: Duration,
        cancellation: &ValidationCancellation,
    ) -> Result<ValidationProcessResult, ValidationContainmentError> {
        if timeout.is_zero() || timeout.as_millis() > u32::MAX as u128 {
            return Err(ValidationContainmentError::InvalidExecutionRoot);
        }
        candidate.revalidate()?;
        revalidate_toolchain(toolchain)?;
        if cancellation.is_cancelled() {
            return Err(ValidationContainmentError::Cancelled);
        }

        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let stage = candidate
            .root
            .parent()
            .ok_or(ValidationContainmentError::InvalidValidationCopy)?
            .join(format!(".assemblywright-validation-toolchain-{nonce}"));
        if stage.exists() {
            return Err(ValidationContainmentError::InvalidToolchain);
        }
        fs::create_dir(&stage).map_err(|_| ValidationContainmentError::Stage("stage_create"))?;
        let mut stage_guard = PrivateDirectoryGuard::new(stage.clone());
        let cargo_home = candidate
            .root
            .join(format!(".assemblywright-validation-cargo-home-{nonce}"));
        let target = candidate
            .root
            .join(format!(".assemblywright-validation-target-{nonce}"));

        let mut profile = create_profile()?;
        let mut root_access = ExecutionRootAccess::grant(&candidate.root, profile.sid)
            .map_err(|_| ValidationContainmentError::Stage("root_acl"))?;
        let mut stage_access = ExecutionRootAccess::grant_permissions(
            &stage,
            profile.sid,
            windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ
                | windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_EXECUTE,
        )
        .map_err(|_| ValidationContainmentError::Stage("toolchain_acl"))?;

        let execution_result = (|| {
            copy_tree(
                &toolchain.source_root,
                &stage,
                200_000,
                4 * 1024 * 1024 * 1024,
            )
            .map_err(|_| ValidationContainmentError::Stage("toolchain_stage"))?;
            fs::create_dir(&cargo_home)
                .map_err(|_| ValidationContainmentError::Stage("cargo_home_create"))?;
            copy_tree(
                &toolchain.dependency_cache_seed,
                &cargo_home,
                200_000,
                4 * 1024 * 1024 * 1024,
            )
            .map_err(|_| ValidationContainmentError::PrivateDependencyCacheUnavailable)?;
            fs::create_dir(&target)
                .map_err(|_| ValidationContainmentError::Stage("target_create"))?;

            let executable = stage.join("bin").join("cargo.exe");
            let command_line = validation_command_line(&executable, command)?;
            let environment =
                validation_environment(&candidate.root, &stage, &cargo_home, &target)?;
            launch_process(
                &executable,
                command_line,
                &candidate.root,
                environment,
                timeout,
                Some(cancellation),
                profile.sid,
            )
            .map(|result| ValidationProcessResult {
                command_id: command,
                exit_code: result.exit_code,
                stdout_len: result.stdout_len,
                stderr_len: result.stderr_len,
                stdout_sha256: result.stdout_sha256,
                stderr_sha256: result.stderr_sha256,
                timed_out: result.timed_out,
            })
        })();

        let cargo_home_removed = remove_private_tree(&cargo_home).is_ok();
        let target_removed = remove_private_tree(&target).is_ok();
        let stage_acl_restored = stage_access.restore_checked().is_ok();
        let stage_removed = stage_guard.remove_checked().is_ok();
        let root_acl_restored = root_access.restore_checked().is_ok();
        let profile_deleted = profile.delete_checked().is_ok();
        if !cargo_home_removed || !target_removed || !stage_removed {
            return Err(ValidationContainmentError::Stage("private_stage_cleanup"));
        }
        if !stage_acl_restored || !root_acl_restored {
            return Err(ValidationContainmentError::Stage("acl_restore"));
        }
        if !profile_deleted {
            return Err(ValidationContainmentError::Stage("profile_delete"));
        }
        candidate.revalidate()?;
        execution_result
    }

    fn create_profile() -> Result<ProfileGuard, ValidationContainmentError> {
        let profile_name = format!("{PROFILE_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let profile_name_w = wide(&profile_name);
        let mut appcontainer_sid: PSID = null_mut();
        let created = unsafe {
            CreateAppContainerProfile(
                profile_name_w.as_ptr(),
                profile_name_w.as_ptr(),
                profile_name_w.as_ptr(),
                null(),
                0,
                &mut appcontainer_sid,
            )
        };
        if created == HRESULT_PROFILE_ALREADY_EXISTS {
            let derived = unsafe {
                DeriveAppContainerSidFromAppContainerName(
                    profile_name_w.as_ptr(),
                    &mut appcontainer_sid,
                )
            };
            if derived < 0 {
                return Err(ValidationContainmentError::StageStatus(
                    "derive_profile",
                    derived,
                ));
            }
        } else if created < 0 {
            return Err(ValidationContainmentError::StageStatus(
                "create_profile",
                created,
            ));
        }
        Ok(ProfileGuard {
            name: profile_name_w,
            sid: appcontainer_sid,
            delete: created >= 0,
        })
    }

    fn revalidate_toolchain(
        toolchain: &ValidationToolchainConfig,
    ) -> Result<(), ValidationContainmentError> {
        if canonical_directory_without_link(&toolchain.source_root)
            .map_err(|_| ValidationContainmentError::InvalidToolchain)?
            != toolchain.source_root
        {
            return Err(ValidationContainmentError::InvalidToolchain);
        }
        if canonical_directory_without_link(&toolchain.dependency_cache_seed)
            .map_err(|_| ValidationContainmentError::PrivateDependencyCacheUnavailable)?
            != toolchain.dependency_cache_seed
        {
            return Err(ValidationContainmentError::PrivateDependencyCacheUnavailable);
        }
        validate_copy_tree(&toolchain.source_root, 200_000, 4 * 1024 * 1024 * 1024)
            .map_err(|_| ValidationContainmentError::InvalidToolchain)?;
        validate_copy_tree(
            &toolchain.dependency_cache_seed,
            200_000,
            4 * 1024 * 1024 * 1024,
        )
        .map_err(|_| ValidationContainmentError::PrivateDependencyCacheUnavailable)?;
        validate_dependency_cache_seed(&toolchain.dependency_cache_seed)
            .map_err(|_| ValidationContainmentError::PrivateDependencyCacheUnavailable)
    }

    fn copy_tree(
        source: &Path,
        destination: &Path,
        max_entries: usize,
        max_bytes: u64,
    ) -> Result<(), ()> {
        let source = source.canonicalize().map_err(|_| ())?;
        let mut stack = vec![(source.clone(), destination.to_path_buf())];
        let mut entries = 0usize;
        let mut bytes = 0u64;
        while let Some((source_directory, destination_directory)) = stack.pop() {
            for entry in fs::read_dir(&source_directory).map_err(|_| ())? {
                let entry = entry.map_err(|_| ())?;
                entries = entries.checked_add(1).ok_or(())?;
                if entries > max_entries {
                    return Err(());
                }
                let file_type = entry.file_type().map_err(|_| ())?;
                if file_type.is_symlink() {
                    return Err(());
                }
                let canonical = entry.path().canonicalize().map_err(|_| ())?;
                if !canonical.starts_with(&source) {
                    return Err(());
                }
                let destination_entry = destination_directory.join(entry.file_name());
                if file_type.is_dir() {
                    fs::create_dir(&destination_entry).map_err(|_| ())?;
                    stack.push((canonical, destination_entry));
                } else if file_type.is_file() {
                    let metadata = entry.metadata().map_err(|_| ())?;
                    bytes = bytes.checked_add(metadata.len()).ok_or(())?;
                    if bytes > max_bytes {
                        return Err(());
                    }
                    fs::copy(&canonical, &destination_entry).map_err(|_| ())?;
                } else {
                    return Err(());
                }
            }
        }
        Ok(())
    }

    fn remove_private_tree(path: &Path) -> Result<(), ()> {
        crate::integration::remove_validation_private_tree(path).map_err(|_| ())
    }

    struct PrivateDirectoryGuard {
        path: Option<PathBuf>,
    }

    impl PrivateDirectoryGuard {
        fn new(path: PathBuf) -> Self {
            Self { path: Some(path) }
        }

        fn remove_checked(&mut self) -> Result<(), ()> {
            if let Some(path) = self.path.as_deref() {
                remove_private_tree(path)?;
                self.path = None;
            }
            Ok(())
        }
    }

    impl Drop for PrivateDirectoryGuard {
        fn drop(&mut self) {
            let _ = self.remove_checked();
        }
    }

    fn validation_command_line(
        executable: &Path,
        command: FeatureConveyorValidationCommandId,
    ) -> Result<Vec<u16>, ValidationContainmentError> {
        let arguments = validation_command_arguments(command)?;
        Ok(wide(format!("\"{}\" {arguments}", executable.display())))
    }

    fn validation_environment(
        root: &Path,
        toolchain: &Path,
        cargo_home: &Path,
        target: &Path,
    ) -> Result<Vec<u16>, ValidationContainmentError> {
        let windows = windows_directory()?;
        let system_drive = windows
            .get(..2)
            .filter(|prefix| prefix.ends_with(':'))
            .ok_or(ValidationContainmentError::Stage("windows_directory"))?;
        let bin = toolchain.join("bin");
        let mut variables = vec![
            format!("APPDATA={}", root.display()),
            format!("CARGO_HOME={}", cargo_home.display()),
            "CARGO_NET_OFFLINE=true".to_string(),
            "CARGO_TERM_COLOR=never".to_string(),
            format!("CARGO_TARGET_DIR={}", target.display()),
            format!("ComSpec={windows}\\System32\\cmd.exe"),
            format!("HOME={}", root.display()),
            format!("LOCALAPPDATA={}", root.display()),
            format!("PATH={};{}\\System32", bin.display(), windows),
            format!("RUSTC={}", bin.join("rustc.exe").display()),
            format!("RUSTFMT={}", bin.join("rustfmt.exe").display()),
            "RUSTUP_TOOLCHAIN=assemblywright-private".to_string(),
            format!("SystemDrive={system_drive}"),
            format!("SystemRoot={windows}"),
            format!("TEMP={}", root.display()),
            format!("TMP={}", root.display()),
            format!("USERPROFILE={}", root.display()),
            format!("WINDIR={windows}"),
        ];
        variables.sort_by_key(|value| value.to_ascii_uppercase());
        environment_block(variables)
    }

    fn windows_directory() -> Result<String, ValidationContainmentError> {
        let mut windows = vec![0u16; 32_768];
        let count = unsafe { GetWindowsDirectoryW(windows.as_mut_ptr(), windows.len() as u32) };
        if count == 0 || count as usize >= windows.len() {
            return Err(ValidationContainmentError::Stage("windows_directory"));
        }
        windows.truncate(count as usize);
        String::from_utf16(&windows)
            .map_err(|_| ValidationContainmentError::Stage("windows_directory"))
    }

    fn environment_block(variables: Vec<String>) -> Result<Vec<u16>, ValidationContainmentError> {
        if variables.iter().any(|value| value.contains('\0')) {
            return Err(ValidationContainmentError::Failed);
        }
        let mut block = Vec::new();
        for variable in variables {
            block.extend(variable.encode_utf16());
            block.push(0);
        }
        block.push(0);
        Ok(block)
    }

    pub(super) fn run(
        fixture: ValidationFixtureCommand,
        execution_root: &Path,
        timeout: Duration,
    ) -> Result<ValidationFixtureResult, ValidationContainmentError> {
        let root = execution_root
            .canonicalize()
            .map_err(|_| ValidationContainmentError::InvalidExecutionRoot)?;
        if !root.is_dir() || timeout.is_zero() || timeout.as_millis() > u32::MAX as u128 {
            return Err(ValidationContainmentError::InvalidExecutionRoot);
        }
        let source_executable = std::env::current_exe()
            .map_err(|_| ValidationContainmentError::Stage("current_exe"))?;
        let source_executable = source_executable
            .canonicalize()
            .map_err(|_| ValidationContainmentError::Stage("canonical_exe"))?;

        let profile_name = format!("{PROFILE_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let profile_name_w = wide(&profile_name);
        let mut appcontainer_sid: PSID = null_mut();
        let created = unsafe {
            CreateAppContainerProfile(
                profile_name_w.as_ptr(),
                profile_name_w.as_ptr(),
                profile_name_w.as_ptr(),
                null(),
                0,
                &mut appcontainer_sid,
            )
        };
        if created == HRESULT_PROFILE_ALREADY_EXISTS {
            let derived = unsafe {
                DeriveAppContainerSidFromAppContainerName(
                    profile_name_w.as_ptr(),
                    &mut appcontainer_sid,
                )
            };
            if derived < 0 {
                return Err(ValidationContainmentError::StageStatus(
                    "derive_profile",
                    derived,
                ));
            }
        } else if created < 0 {
            return Err(ValidationContainmentError::StageStatus(
                "create_profile",
                created,
            ));
        }
        let mut profile = ProfileGuard {
            name: profile_name_w,
            sid: appcontainer_sid,
            delete: created >= 0,
        };
        let mut root_access = ExecutionRootAccess::grant(&root, profile.sid)
            .map_err(|_| ValidationContainmentError::Stage("root_acl"))?;
        let execution_result = (|| {
            let executable = root.join("assemblywright-validation-fixture.exe");
            fs::copy(&source_executable, &executable)
                .map_err(|_| ValidationContainmentError::Stage("copy_fixture"))?;
            let inheritance_event = inheritable_event()
                .map_err(|_| ValidationContainmentError::Stage("inheritance_event"))?;
            fs::write(
                root.join("inheritance-probe.txt"),
                (inheritance_event.raw() as usize).to_string(),
            )
            .map_err(|_| ValidationContainmentError::Stage("inheritance_probe"))?;

            let token = restricted_primary_token()
                .map_err(|_| ValidationContainmentError::Stage("restricted_token"))?;
            let (stdout_read, stdout_write) =
                pipe().map_err(|_| ValidationContainmentError::Stage("stdout_pipe"))?;
            let (stderr_read, stderr_write) =
                pipe().map_err(|_| ValidationContainmentError::Stage("stderr_pipe"))?;
            let mut inherited = [stdout_write.raw(), stderr_write.raw()];

            let mut attributes = AttributeList::new(2)
                .map_err(|_| ValidationContainmentError::Stage("attribute_list"))?;
            let mut security_capabilities = SECURITY_CAPABILITIES {
                AppContainerSid: profile.sid,
                Capabilities: null_mut(),
                CapabilityCount: 0,
                Reserved: 0,
            };
            attributes
                .update(
                    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                    (&mut security_capabilities as *mut SECURITY_CAPABILITIES).cast(),
                    size_of::<SECURITY_CAPABILITIES>(),
                )
                .map_err(|_| ValidationContainmentError::Stage("security_capabilities"))?;
            attributes
                .update(
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    inherited.as_mut_ptr().cast(),
                    size_of_val(&inherited),
                )
                .map_err(|_| ValidationContainmentError::Stage("handle_list"))?;
            let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
            startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
            startup.StartupInfo.hStdOutput = stdout_write.raw();
            startup.StartupInfo.hStdError = stderr_write.raw();
            startup.lpAttributeList = attributes.ptr;

            let mut command = exact_command_line(&executable, fixture);
            let executable_w = wide(executable.as_os_str());
            let root_w = wide(root.as_os_str());
            let environment = minimal_environment(&root)?;
            let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
            let launched = unsafe {
                CreateProcessAsUserW(
                    token.raw(),
                    executable_w.as_ptr(),
                    command.as_mut_ptr(),
                    null(),
                    null(),
                    1,
                    CREATE_SUSPENDED
                        | CREATE_NO_WINDOW
                        | CREATE_UNICODE_ENVIRONMENT
                        | EXTENDED_STARTUPINFO_PRESENT,
                    environment.as_ptr().cast(),
                    root_w.as_ptr(),
                    &startup.StartupInfo,
                    &mut process,
                )
            };
            if launched == 0 {
                return Err(ValidationContainmentError::StageStatus(
                    "create_process",
                    unsafe { GetLastError() } as i32,
                ));
            }
            let process_handle = OwnedHandle::new(process.hProcess)
                .map_err(|_| ValidationContainmentError::Stage("process_handle"))?;
            let thread_handle = OwnedHandle::new(process.hThread)
                .map_err(|_| ValidationContainmentError::Stage("thread_handle"))?;
            drop(stdout_write);
            drop(stderr_write);

            let job = create_job().map_err(|_| ValidationContainmentError::Stage("job_create"))?;
            if unsafe { AssignProcessToJobObject(job.raw(), process_handle.raw()) } == 0 {
                unsafe { TerminateJobObject(job.raw(), TERMINATED_EXIT_CODE) };
                unsafe { WaitForSingleObject(process_handle.raw(), 5_000) };
                return Err(ValidationContainmentError::Stage("job_assign"));
            }
            if unsafe { ResumeThread(thread_handle.raw()) } == u32::MAX {
                unsafe { TerminateJobObject(job.raw(), TERMINATED_EXIT_CODE) };
                unsafe { WaitForSingleObject(process_handle.raw(), 5_000) };
                return Err(ValidationContainmentError::Stage("resume"));
            }

            let stdout_drain = thread::spawn(move || drain(stdout_read));
            let stderr_drain = thread::spawn(move || drain(stderr_read));
            let wait =
                unsafe { WaitForSingleObject(process_handle.raw(), timeout.as_millis() as u32) };
            let timed_out = wait == WAIT_TIMEOUT;
            if timed_out {
                if unsafe { TerminateJobObject(job.raw(), TERMINATED_EXIT_CODE) } == 0 {
                    return Err(ValidationContainmentError::Failed);
                }
                if unsafe { WaitForSingleObject(process_handle.raw(), 5_000) } != WAIT_OBJECT_0 {
                    return Err(ValidationContainmentError::Failed);
                }
            } else if wait != WAIT_OBJECT_0 {
                return Err(ValidationContainmentError::Stage("process_wait"));
            }
            if !wait_job_empty(job.raw(), Duration::from_secs(5))? {
                if unsafe { TerminateJobObject(job.raw(), TERMINATED_EXIT_CODE) } == 0
                    || !wait_job_empty(job.raw(), Duration::from_secs(5))?
                {
                    return Err(ValidationContainmentError::Stage("job_reap"));
                }
                if !timed_out {
                    return Err(ValidationContainmentError::Stage("descendant_remained"));
                }
            }
            let mut exit_code = 0;
            if unsafe { GetExitCodeProcess(process_handle.raw(), &mut exit_code) } == 0 {
                return Err(ValidationContainmentError::Stage("exit_code"));
            }
            if unsafe { WaitForSingleObject(inheritance_event.raw(), 0) } != WAIT_TIMEOUT {
                return Err(ValidationContainmentError::Stage("handle_inherited"));
            }
            drop(job);
            let stdout = stdout_drain
                .join()
                .map_err(|_| ValidationContainmentError::Failed)??;
            let stderr = stderr_drain
                .join()
                .map_err(|_| ValidationContainmentError::Failed)??;
            Ok(ValidationFixtureResult {
                exit_code,
                stdout_len: stdout.len,
                stderr_len: stderr.len,
                stdout_sha256: stdout.sha256,
                stderr_sha256: stderr.sha256,
                timed_out,
            })
        })();
        let acl_restored = root_access.restore_checked().is_ok();
        let profile_deleted = profile.delete_checked().is_ok();
        if !acl_restored {
            return Err(ValidationContainmentError::Stage("acl_restore"));
        }
        if !profile_deleted {
            return Err(ValidationContainmentError::Stage("profile_delete"));
        }
        execution_result
    }

    fn launch_process(
        executable: &Path,
        mut command: Vec<u16>,
        root: &Path,
        environment: Vec<u16>,
        timeout: Duration,
        cancellation: Option<&ValidationCancellation>,
        appcontainer_sid: PSID,
    ) -> Result<ValidationFixtureResult, ValidationContainmentError> {
        let inheritance_event = inheritable_event()
            .map_err(|_| ValidationContainmentError::Stage("inheritance_event"))?;
        let token = restricted_primary_token()
            .map_err(|_| ValidationContainmentError::Stage("restricted_token"))?;
        let (stdout_read, stdout_write) =
            pipe().map_err(|_| ValidationContainmentError::Stage("stdout_pipe"))?;
        let (stderr_read, stderr_write) =
            pipe().map_err(|_| ValidationContainmentError::Stage("stderr_pipe"))?;
        let mut inherited = [stdout_write.raw(), stderr_write.raw()];
        let mut attributes = AttributeList::new(2)
            .map_err(|_| ValidationContainmentError::Stage("attribute_list"))?;
        let mut security_capabilities = SECURITY_CAPABILITIES {
            AppContainerSid: appcontainer_sid,
            Capabilities: null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        };
        attributes
            .update(
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
                (&mut security_capabilities as *mut SECURITY_CAPABILITIES).cast(),
                size_of::<SECURITY_CAPABILITIES>(),
            )
            .map_err(|_| ValidationContainmentError::Stage("security_capabilities"))?;
        attributes
            .update(
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                inherited.as_mut_ptr().cast(),
                size_of_val(&inherited),
            )
            .map_err(|_| ValidationContainmentError::Stage("handle_list"))?;
        let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdOutput = stdout_write.raw();
        startup.StartupInfo.hStdError = stderr_write.raw();
        startup.lpAttributeList = attributes.ptr;
        let executable_w = wide(executable.as_os_str());
        let root_w = wide(root.as_os_str());
        let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
        let launched = unsafe {
            CreateProcessAsUserW(
                token.raw(),
                executable_w.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                1,
                CREATE_SUSPENDED
                    | CREATE_NO_WINDOW
                    | CREATE_UNICODE_ENVIRONMENT
                    | EXTENDED_STARTUPINFO_PRESENT,
                environment.as_ptr().cast(),
                root_w.as_ptr(),
                &startup.StartupInfo,
                &mut process,
            )
        };
        if launched == 0 {
            return Err(ValidationContainmentError::StageStatus(
                "create_process",
                unsafe { GetLastError() } as i32,
            ));
        }
        let process_handle = OwnedHandle::new(process.hProcess)
            .map_err(|_| ValidationContainmentError::Stage("process_handle"))?;
        let thread_handle = OwnedHandle::new(process.hThread)
            .map_err(|_| ValidationContainmentError::Stage("thread_handle"))?;
        drop(stdout_write);
        drop(stderr_write);
        let job = create_job().map_err(|_| ValidationContainmentError::Stage("job_create"))?;
        if unsafe { AssignProcessToJobObject(job.raw(), process_handle.raw()) } == 0 {
            unsafe { TerminateJobObject(job.raw(), TERMINATED_EXIT_CODE) };
            unsafe { WaitForSingleObject(process_handle.raw(), 5_000) };
            return Err(ValidationContainmentError::Stage("job_assign"));
        }
        if unsafe { ResumeThread(thread_handle.raw()) } == u32::MAX {
            unsafe { TerminateJobObject(job.raw(), TERMINATED_EXIT_CODE) };
            unsafe { WaitForSingleObject(process_handle.raw(), 5_000) };
            return Err(ValidationContainmentError::Stage("resume"));
        }
        let stdout_drain = thread::spawn(move || drain(stdout_read));
        let stderr_drain = thread::spawn(move || drain(stderr_read));
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(ValidationContainmentError::Failed)?;
        let mut cancelled = false;
        let mut timed_out = false;
        loop {
            if cancellation.is_some_and(ValidationCancellation::is_cancelled) {
                cancelled = true;
                break;
            }
            let now = Instant::now();
            if now >= deadline {
                timed_out = true;
                break;
            }
            let remaining = deadline.saturating_duration_since(now);
            let poll = remaining.min(Duration::from_millis(25)).as_millis() as u32;
            match unsafe { WaitForSingleObject(process_handle.raw(), poll.max(1)) } {
                WAIT_OBJECT_0 => break,
                WAIT_TIMEOUT => {}
                _ => return Err(ValidationContainmentError::Stage("process_wait")),
            }
        }
        if cancelled || timed_out {
            let exit_code = if cancelled {
                CANCELLED_EXIT_CODE
            } else {
                TERMINATED_EXIT_CODE
            };
            if unsafe { TerminateJobObject(job.raw(), exit_code) } == 0
                || unsafe { WaitForSingleObject(process_handle.raw(), 5_000) } != WAIT_OBJECT_0
            {
                return Err(ValidationContainmentError::Stage("job_terminate"));
            }
        }
        if !wait_job_empty(job.raw(), Duration::from_secs(5))? {
            if unsafe { TerminateJobObject(job.raw(), TERMINATED_EXIT_CODE) } == 0
                || !wait_job_empty(job.raw(), Duration::from_secs(5))?
            {
                return Err(ValidationContainmentError::Stage("job_reap"));
            }
            if !timed_out && !cancelled {
                return Err(ValidationContainmentError::Stage("descendant_remained"));
            }
        }
        let mut exit_code = 0;
        if unsafe { GetExitCodeProcess(process_handle.raw(), &mut exit_code) } == 0 {
            return Err(ValidationContainmentError::Stage("exit_code"));
        }
        if unsafe { WaitForSingleObject(inheritance_event.raw(), 0) } != WAIT_TIMEOUT {
            return Err(ValidationContainmentError::Stage("handle_inherited"));
        }
        drop(job);
        let stdout = stdout_drain
            .join()
            .map_err(|_| ValidationContainmentError::Failed)??;
        let stderr = stderr_drain
            .join()
            .map_err(|_| ValidationContainmentError::Failed)??;
        if cancelled {
            return Err(ValidationContainmentError::Cancelled);
        }
        Ok(ValidationFixtureResult {
            exit_code,
            stdout_len: stdout.len,
            stderr_len: stderr.len,
            stdout_sha256: stdout.sha256,
            stderr_sha256: stderr.sha256,
            timed_out,
        })
    }

    fn exact_command_line(executable: &Path, fixture: ValidationFixtureCommand) -> Vec<u16> {
        let test = match fixture {
            ValidationFixtureCommand::ReadWriteAndEnvironment => {
                "fixture_appcontainer_can_read_write_only_granted_root_and_has_exact_environment"
            }
            ValidationFixtureCommand::BoundedOutput => {
                "fixture_appcontainer_output_is_bounded_by_parent"
            }
            ValidationFixtureCommand::TimeoutChildTree => {
                "fixture_timeout_spawns_child_tree_that_must_be_killed"
            }
            ValidationFixtureCommand::DeniedOutsideRoot => {
                "fixture_appcontainer_is_denied_outside_execution_root"
            }
            ValidationFixtureCommand::NetworkDenied => {
                "fixture_zero_capability_appcontainer_has_no_network"
            }
        };
        wide(format!(
            "\"{}\" --exact {test} --ignored --nocapture",
            executable.display()
        ))
    }

    fn minimal_environment(root: &Path) -> Result<Vec<u16>, ValidationContainmentError> {
        let mut windows = vec![0u16; 32_768];
        let count = unsafe { GetWindowsDirectoryW(windows.as_mut_ptr(), windows.len() as u32) };
        if count == 0 || count as usize >= windows.len() {
            return Err(ValidationContainmentError::Stage("windows_directory"));
        }
        windows.truncate(count as usize);
        let windows = String::from_utf16(&windows)
            .map_err(|_| ValidationContainmentError::Stage("windows_directory"))?;
        let root = root.as_os_str().to_string_lossy();
        let system_drive = windows
            .get(..2)
            .filter(|prefix| prefix.ends_with(':'))
            .ok_or(ValidationContainmentError::Stage("windows_directory"))?;
        let mut variables = vec![
            format!("APPDATA={root}"),
            format!("ComSpec={windows}\\System32\\cmd.exe"),
            format!("LOCALAPPDATA={root}"),
            format!("PATH={windows}\\System32"),
            format!("SystemDrive={system_drive}"),
            format!("SystemRoot={windows}"),
            format!("TEMP={root}"),
            format!("TMP={root}"),
            format!("USERPROFILE={root}"),
            format!("WINDIR={windows}"),
        ];
        variables.sort_by_key(|value| value.to_ascii_uppercase());
        let mut block = Vec::new();
        for variable in variables {
            block.extend(variable.encode_utf16());
            block.push(0);
        }
        block.push(0);
        Ok(block)
    }

    fn restricted_primary_token() -> Result<OwnedHandle, ValidationContainmentError> {
        let mut current: HANDLE = null_mut();
        if unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ASSIGN_PRIMARY | TOKEN_DUPLICATE | TOKEN_QUERY,
                &mut current,
            )
        } == 0
        {
            return Err(ValidationContainmentError::Failed);
        }
        let current = OwnedHandle::new(current)?;
        let mut restricted: HANDLE = null_mut();
        if unsafe {
            CreateRestrictedToken(
                current.raw(),
                DISABLE_MAX_PRIVILEGE,
                0,
                null(),
                0,
                null(),
                0,
                null(),
                &mut restricted,
            )
        } == 0
        {
            return Err(ValidationContainmentError::Failed);
        }
        OwnedHandle::new(restricted)
    }

    fn pipe() -> Result<(OwnedHandle, OwnedHandle), ValidationContainmentError> {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        let mut read: HANDLE = null_mut();
        let mut write: HANDLE = null_mut();
        if unsafe { CreatePipe(&mut read, &mut write, &attributes, 0) } == 0 {
            return Err(ValidationContainmentError::Failed);
        }
        let read = OwnedHandle::new(read)?;
        let write = OwnedHandle::new(write)?;
        if unsafe { SetHandleInformation(read.raw(), HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(ValidationContainmentError::Failed);
        }
        Ok((read, write))
    }

    fn inheritable_event() -> Result<OwnedHandle, ValidationContainmentError> {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        OwnedHandle::new(unsafe { CreateEventW(&attributes, 1, 0, null()) })
    }

    fn create_job() -> Result<OwnedHandle, ValidationContainmentError> {
        let job = OwnedHandle::new(unsafe { CreateJobObjectW(null(), null()) })?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_JOB_MEMORY;
        limits.BasicLimitInformation.ActiveProcessLimit = 8;
        limits.ProcessMemoryLimit = PROCESS_MEMORY_LIMIT;
        limits.JobMemoryLimit = JOB_MEMORY_LIMIT;
        if unsafe {
            SetInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(ValidationContainmentError::Failed);
        }
        Ok(job)
    }

    fn wait_job_empty(job: HANDLE, timeout: Duration) -> Result<bool, ValidationContainmentError> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(ValidationContainmentError::Failed)?;
        loop {
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
            if unsafe {
                QueryInformationJobObject(
                    job,
                    JobObjectBasicAccountingInformation,
                    (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    null_mut(),
                )
            } == 0
            {
                return Err(ValidationContainmentError::Failed);
            }
            if accounting.ActiveProcesses == 0 {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    struct DrainResult {
        len: usize,
        sha256: [u8; 32],
    }

    fn drain(handle: OwnedHandle) -> Result<DrainResult, ValidationContainmentError> {
        let raw = handle.into_raw();
        let mut file = unsafe { File::from_raw_handle(raw as RawHandle) };
        let mut digest = Sha256::new();
        let mut total = 0usize;
        let mut buffer = [0u8; 4096];
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|_| ValidationContainmentError::Failed)?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count)
                .ok_or(ValidationContainmentError::OutputLimitExceeded)?;
            if total > MAX_CAPTURE_BYTES {
                return Err(ValidationContainmentError::OutputLimitExceeded);
            }
            digest.update(&buffer[..count]);
        }
        Ok(DrainResult {
            len: total,
            sha256: digest.finalize().into(),
        })
    }

    struct OwnedHandle(HANDLE);

    impl OwnedHandle {
        fn new(handle: HANDLE) -> Result<Self, ValidationContainmentError> {
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                Err(ValidationContainmentError::Failed)
            } else {
                Ok(Self(handle))
            }
        }
        fn raw(&self) -> HANDLE {
            self.0
        }
        fn into_raw(mut self) -> HANDLE {
            let raw = self.0;
            self.0 = null_mut();
            raw
        }
    }

    unsafe impl Send for OwnedHandle {}

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    struct AttributeList {
        storage: Vec<usize>,
        ptr: windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST,
    }

    impl AttributeList {
        fn new(count: u32) -> Result<Self, ValidationContainmentError> {
            let mut bytes = 0usize;
            unsafe { InitializeProcThreadAttributeList(null_mut(), count, 0, &mut bytes) };
            if bytes == 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
                return Err(ValidationContainmentError::Failed);
            }
            let words = bytes.div_ceil(size_of::<usize>());
            let mut storage = vec![0usize; words];
            let ptr = storage.as_mut_ptr().cast();
            if unsafe { InitializeProcThreadAttributeList(ptr, count, 0, &mut bytes) } == 0 {
                return Err(ValidationContainmentError::Failed);
            }
            Ok(Self { storage, ptr })
        }

        fn update(
            &mut self,
            attribute: usize,
            value: *const c_void,
            bytes: usize,
        ) -> Result<(), ValidationContainmentError> {
            if unsafe {
                UpdateProcThreadAttribute(self.ptr, 0, attribute, value, bytes, null_mut(), null())
            } == 0
            {
                return Err(ValidationContainmentError::Failed);
            }
            Ok(())
        }
    }

    impl Drop for AttributeList {
        fn drop(&mut self) {
            unsafe { DeleteProcThreadAttributeList(self.ptr) };
            let _ = self.storage.len();
        }
    }

    struct ProfileGuard {
        name: Vec<u16>,
        sid: PSID,
        delete: bool,
    }

    impl ProfileGuard {
        fn delete_checked(&mut self) -> Result<(), ValidationContainmentError> {
            let mut failed = false;
            if self.delete {
                if unsafe { DeleteAppContainerProfile(self.name.as_ptr()) } < 0 {
                    failed = true;
                } else {
                    self.delete = false;
                }
            }
            if !self.sid.is_null() {
                if !unsafe { FreeSid(self.sid.cast()) }.is_null() {
                    failed = true;
                }
                self.sid = null_mut();
            }
            if failed {
                Err(ValidationContainmentError::Failed)
            } else {
                Ok(())
            }
        }
    }

    impl Drop for ProfileGuard {
        fn drop(&mut self) {
            let _ = self.delete_checked();
        }
    }

    struct ExecutionRootAccess {
        path: Vec<u16>,
        original_descriptor: *mut c_void,
        original_dacl: *mut windows_sys::Win32::Security::ACL,
        restored: bool,
    }

    impl ExecutionRootAccess {
        fn grant(path: &Path, sid: PSID) -> Result<Self, ValidationContainmentError> {
            use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
            Self::grant_permissions(path, sid, FILE_ALL_ACCESS)
        }

        fn grant_permissions(
            path: &Path,
            sid: PSID,
            permissions: u32,
        ) -> Result<Self, ValidationContainmentError> {
            use windows_sys::Win32::Security::Authorization::{
                GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W,
                GRANT_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP,
            };
            use windows_sys::Win32::Security::{
                CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, OBJECT_INHERIT_ACE,
            };
            let path = wide(path.as_os_str());
            let mut original_dacl = null_mut();
            let mut descriptor = null_mut();
            let status = unsafe {
                GetNamedSecurityInfoW(
                    path.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    &mut original_dacl,
                    null_mut(),
                    &mut descriptor,
                )
            };
            if status != 0 {
                return Err(ValidationContainmentError::Failed);
            }
            let mut entry: EXPLICIT_ACCESS_W = unsafe { zeroed() };
            entry.grfAccessPermissions = permissions;
            entry.grfAccessMode = GRANT_ACCESS;
            entry.grfInheritance = CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE;
            entry.Trustee.TrusteeForm = TRUSTEE_IS_SID;
            entry.Trustee.TrusteeType = TRUSTEE_IS_WELL_KNOWN_GROUP;
            entry.Trustee.ptstrName = sid.cast();
            let mut updated_dacl = null_mut();
            let status = unsafe { SetEntriesInAclW(1, &entry, original_dacl, &mut updated_dacl) };
            if status != 0 {
                unsafe { windows_sys::Win32::Foundation::LocalFree(descriptor) };
                return Err(ValidationContainmentError::Failed);
            }
            let status = unsafe {
                SetNamedSecurityInfoW(
                    path.as_ptr() as *mut _,
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    updated_dacl,
                    null_mut(),
                )
            };
            unsafe { windows_sys::Win32::Foundation::LocalFree(updated_dacl.cast()) };
            if status != 0 {
                unsafe { windows_sys::Win32::Foundation::LocalFree(descriptor) };
                return Err(ValidationContainmentError::Failed);
            }
            Ok(Self {
                path,
                original_descriptor: descriptor,
                original_dacl,
                restored: false,
            })
        }

        fn restore_checked(&mut self) -> Result<(), ValidationContainmentError> {
            use windows_sys::Win32::Security::Authorization::{
                SetNamedSecurityInfoW, SE_FILE_OBJECT,
            };
            use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
            if self.restored {
                return Ok(());
            }
            let status = unsafe {
                SetNamedSecurityInfoW(
                    self.path.as_mut_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    self.original_dacl,
                    null_mut(),
                )
            };
            if status != 0 {
                return Err(ValidationContainmentError::Failed);
            }
            unsafe {
                windows_sys::Win32::Foundation::LocalFree(self.original_descriptor);
            }
            self.original_descriptor = null_mut();
            self.original_dacl = null_mut();
            self.restored = true;
            Ok(())
        }
    }

    impl Drop for ExecutionRootAccess {
        fn drop(&mut self) {
            let _ = self.restore_checked();
        }
    }

    fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
        value.as_ref().encode_wide().chain(Some(0)).collect()
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn unsupported_platform_fails_closed_before_process_execution() {
        let result = run_validation_fixture(
            ValidationFixtureCommand::ReadWriteAndEnvironment,
            Path::new("."),
            Duration::from_secs(1),
        );
        assert!(matches!(
            result,
            Err(ValidationContainmentError::Unsupported)
        ));
    }
}
