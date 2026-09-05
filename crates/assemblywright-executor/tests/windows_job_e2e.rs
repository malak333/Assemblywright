#![cfg(windows)]

use assemblywright_executor::{
    ExecutorAuthoritySnapshot, ExecutorError, ExecutorIdentity, ExecutorPolicy,
    UnprivilegedProcessOperation,
};
use assemblywright_protocol::*;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::mem::zeroed;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;
use uuid::Uuid;
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT,
};

#[test]
fn suspended_child_and_descendant_are_reaped_by_kill_on_close_job() {
    let temp = tempdir().unwrap();
    let root = plain_path(&temp.path().canonicalize().unwrap());
    let working_directory = root.join("feature");
    let protected = root.join("control-plane");
    fs::create_dir(&working_directory).unwrap();
    fs::create_dir(&protected).unwrap();

    let helper = PathBuf::from(env!("CARGO_BIN_EXE_assemblywright-executor-windows-helper"));
    let executable = root.join("worker-helper.exe");
    fs::copy(helper, &executable).unwrap();
    let executable_text = executable.to_str().unwrap().to_string();
    let cwd_text = working_directory.to_str().unwrap().to_string();
    let operation = UnprivilegedProcessOperation {
        executable: executable_text.clone(),
        arguments: Vec::new(),
        working_directory: cwd_text.clone(),
        environment: BTreeMap::new(),
    };

    let authority = SigningKey::from_bytes(&[41; 32]);
    let receipt = SigningKey::from_bytes(&[42; 32]);
    let executor_id = Uuid::new_v4();
    let broker_id = Uuid::new_v4();
    let child_epoch_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let protected_text = protected.to_str().unwrap().to_string();
    let manifest = ProtectedControlPlanePathManifest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        platform: ExecutionHostPlatform::Windows,
        master_binary: protected_text.clone(),
        broker_binary: protected_text.clone(),
        service_configuration: protected_text.clone(),
        authority_database: protected_text.clone(),
        database_backups: protected_text.clone(),
        audit: protected_text.clone(),
        owner_tokens_and_signing: protected_text.clone(),
        trust_and_update_roots: protected_text.clone(),
        ipc_and_enforcement_state: protected_text.clone(),
        release_evidence: protected_text.clone(),
        resource_reservations: protected_text,
    };
    let protected_digest = manifest.canonical_sha256().unwrap();
    let policy = executor_policy(
        &authority,
        &receipt,
        executor_id,
        broker_id,
        child_epoch_id,
        session_id,
        protected_digest,
        manifest.clone(),
    );
    let mut envelope = ExecutionActionEnvelope {
        schema_version: EXECUTION_ACTION_ENVELOPE_SCHEMA_VERSION,
        action_id: Uuid::new_v4(),
        action_sequence: 1,
        feature_id: Uuid::new_v4(),
        repository_id: Uuid::new_v4(),
        session_id,
        session_revision: 1,
        child_epoch_id,
        child_epoch_revision: 1,
        feature_lifecycle_revision: 1,
        authority_revision: 1,
        executor_id,
        executor_revision: 1,
        executor_executable_sha256: [1; 32],
        broker_id,
        broker_revision: 1,
        broker_executable_sha256: [2; 32],
        protected_control_plane_sha256: protected_digest,
        host_platform: ExecutionHostPlatform::Windows,
        action_type: ExecutionActionType::RunUnprivilegedProcess,
        targets: vec![
            ExecutionTargetIdentity {
                platform: ExecutionHostPlatform::Windows,
                canonical_path: cwd_text.clone(),
                canonical_path_sha256: execution_path_sha256(
                    ExecutionHostPlatform::Windows,
                    &cwd_text,
                )
                .unwrap(),
                canonical_parent_sha256: object_identity_sha256(
                    working_directory.parent().unwrap(),
                ),
                expected_object_sha256: Some(object_identity_sha256(&working_directory)),
                expected_single_link: true,
            },
            ExecutionTargetIdentity {
                platform: ExecutionHostPlatform::Windows,
                canonical_path: executable_text.clone(),
                canonical_path_sha256: execution_path_sha256(
                    ExecutionHostPlatform::Windows,
                    &executable_text,
                )
                .unwrap(),
                canonical_parent_sha256: object_identity_sha256(executable.parent().unwrap()),
                expected_object_sha256: Some(object_identity_sha256(&executable)),
                expected_single_link: true,
            },
        ],
        operation_sha256: operation.sha256().unwrap(),
        working_directory_sha256: execution_path_sha256(ExecutionHostPlatform::Windows, &cwd_text)
            .unwrap(),
        environment_keys: Vec::new(),
        effect_classification: ExecutionEffectClassification::LocalReversible,
        deadline_ms: now_ms() + 60_000,
        cancellation_behavior: ExecutionCancellationBehavior::ImmediateTerminate,
        reconciliation_strategy: ExecutionReconciliationStrategy::NoEffectRetry,
        issued_at_ms: now_ms() - 1_000,
        nonce: Uuid::new_v4(),
        signer_key_id: "master-action-v1".into(),
        signature: Vec::new(),
    };
    envelope.sign(&authority).unwrap();

    let marker = working_directory.join("job-started.txt");
    let historical_snapshot =
        authority_snapshot(&authority, session_id, child_epoch_id, 1, false, false);
    let durable_revoked_snapshot =
        authority_snapshot(&authority, session_id, child_epoch_id, 3, true, true);
    assert_eq!(
        executor_policy_for_checkpoint(
            &authority,
            &receipt,
            executor_id,
            broker_id,
            child_epoch_id,
            session_id,
            protected_digest,
            manifest.clone(),
            &durable_revoked_snapshot,
            historical_snapshot,
        )
        .err()
        .unwrap(),
        ExecutorError::InvalidIdentity
    );
    let restarted_revoked_policy = executor_policy_for_checkpoint(
        &authority,
        &receipt,
        executor_id,
        broker_id,
        child_epoch_id,
        session_id,
        protected_digest,
        manifest.clone(),
        &durable_revoked_snapshot,
        durable_revoked_snapshot.clone(),
    )
    .unwrap();
    assert!(matches!(
        restarted_revoked_policy.admit(&envelope, &operation),
        Err(ExecutorError::InvalidIdentity)
    ));
    assert!(!marker.exists());

    let stale_unpresented_policy = executor_policy(
        &authority,
        &receipt,
        executor_id,
        broker_id,
        child_epoch_id,
        session_id,
        protected_digest,
        manifest.clone(),
    );
    stale_unpresented_policy
        .update_authority_snapshot(authority_snapshot(
            &authority,
            session_id,
            child_epoch_id,
            2,
            true,
            false,
        ))
        .unwrap();
    stale_unpresented_policy
        .update_authority_snapshot(authority_snapshot(
            &authority,
            session_id,
            child_epoch_id,
            3,
            false,
            false,
        ))
        .unwrap();
    assert!(matches!(
        stale_unpresented_policy.admit(&envelope, &operation),
        Err(ExecutorError::InvalidIdentity)
    ));
    assert!(!marker.exists());

    let concurrent_policy = executor_policy(
        &authority,
        &receipt,
        executor_id,
        broker_id,
        child_epoch_id,
        session_id,
        protected_digest,
        manifest.clone(),
    );
    let concurrent_admission = concurrent_policy.admit(&envelope, &operation).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let concurrent_result = thread::scope(|scope| {
        let update_barrier = barrier.clone();
        let concurrent_policy = &concurrent_policy;
        let authority = &authority;
        let updater = scope.spawn(move || {
            update_barrier.wait();
            concurrent_policy.update_authority_snapshot(authority_snapshot(
                authority,
                session_id,
                child_epoch_id,
                2,
                true,
                false,
            ))
        });
        barrier.wait();
        let spawn = concurrent_admission.spawn();
        updater.join().unwrap().unwrap();
        spawn
    });
    match concurrent_result {
        Err(error) => {
            assert_eq!(error, ExecutorError::InvalidIdentity);
            assert!(!marker.exists());
        }
        Ok(execution) => {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !marker.exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert!(marker.exists());
            let receipt = execution
                .terminate(
                    ExecutionTerminationMode::EmergencyPause,
                    [10; 32],
                    Duration::from_millis(100),
                    Duration::from_secs(5),
                )
                .unwrap();
            assert_eq!(receipt.outcome, ExecutionTerminationOutcome::Reaped);
            assert!(receipt.descendants_reaped);
            fs::remove_file(&marker).unwrap();
        }
    }

    let paused_policy = executor_policy(
        &authority,
        &receipt,
        executor_id,
        broker_id,
        child_epoch_id,
        session_id,
        protected_digest,
        manifest,
    );
    let paused_admission = paused_policy.admit(&envelope, &operation).unwrap();
    paused_policy
        .update_authority_snapshot(authority_snapshot(
            &authority,
            session_id,
            child_epoch_id,
            2,
            true,
            false,
        ))
        .unwrap();
    paused_policy
        .update_authority_snapshot(authority_snapshot(
            &authority,
            session_id,
            child_epoch_id,
            3,
            false,
            false,
        ))
        .unwrap();
    assert_eq!(
        paused_admission.spawn().err().unwrap(),
        ExecutorError::InvalidIdentity
    );
    paused_policy
        .update_authority_snapshot(authority_snapshot(
            &authority,
            session_id,
            child_epoch_id,
            4,
            false,
            true,
        ))
        .unwrap();
    assert_eq!(
        paused_policy.update_authority_snapshot(authority_snapshot(
            &authority,
            session_id,
            child_epoch_id,
            5,
            false,
            false,
        )),
        Err(ExecutorError::InvalidIdentity)
    );
    assert!(!marker.exists());
    let mut mismatched_identity = envelope.clone();
    mismatched_identity.targets[1].expected_object_sha256 = Some([91; 32]);
    mismatched_identity.signature.clear();
    mismatched_identity.sign(&authority).unwrap();
    assert!(matches!(
        policy.admit(&mismatched_identity, &operation),
        Err(ExecutorError::UnsafePath)
    ));
    assert!(!marker.exists());

    let admission = policy.admit(&envelope, &operation).unwrap();
    assert!(fs::rename(&executable, root.join("attacker-worker.exe")).is_err());
    assert!(fs::rename(&working_directory, root.join("attacker-cwd")).is_err());
    let process = admission.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let marker_value = fs::read_to_string(&marker).unwrap();
    assert!(marker_value.contains("root="));
    assert!(marker_value.contains("descendant="));
    let resource_limits = process.attest_windows_job_resource_limits().unwrap();
    assert!(matches!(resource_limits.cpu_rate_hard_cap, 5_000 | 9_000));
    assert!(resource_limits.commit_limit_bytes >= 1024 * 1024 * 1024);
    assert_eq!(resource_limits.active_process_limit, 128);
    let termination = process
        .terminate(
            ExecutionTerminationMode::EmergencyPause,
            [11; 32],
            Duration::from_millis(100),
            Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(termination.outcome, ExecutionTerminationOutcome::Reaped);
    assert_eq!(termination.tracked_root_process_count, 1);
    assert_eq!(termination.reaped_root_process_count, 1);
    assert_eq!(termination.survivor_root_process_count, 0);
    assert!(termination.descendants_reaped);
    termination
        .verify_signature(&receipt.verifying_key())
        .unwrap();
}

fn plain_path(path: &Path) -> PathBuf {
    PathBuf::from(
        path.to_string_lossy()
            .strip_prefix(r"\\?\")
            .unwrap_or(&path.to_string_lossy()),
    )
}

fn object_identity_sha256(path: &Path) -> [u8; 32] {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .unwrap();
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    assert_ne!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) },
        0
    );
    let canonical = plain_path(&path.canonicalize().unwrap());
    let comparison = canonical
        .to_str()
        .unwrap()
        .replace('/', "\\")
        .to_ascii_lowercase();
    let mut hasher = Sha256::new();
    hasher.update(comparison.as_bytes());
    hasher.update([0]);
    hasher.update(information.dwVolumeSerialNumber.to_le_bytes());
    hasher.update(information.nFileIndexHigh.to_le_bytes());
    hasher.update(information.nFileIndexLow.to_le_bytes());
    hasher.finalize().into()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[allow(clippy::too_many_arguments)]
fn executor_policy(
    authority: &SigningKey,
    receipt: &SigningKey,
    executor_id: Uuid,
    broker_id: Uuid,
    child_epoch_id: Uuid,
    session_id: Uuid,
    protected_digest: [u8; 32],
    manifest: ProtectedControlPlanePathManifest,
) -> ExecutorPolicy {
    let snapshot = authority_snapshot(authority, session_id, child_epoch_id, 1, false, false);
    executor_policy_for_checkpoint(
        authority,
        receipt,
        executor_id,
        broker_id,
        child_epoch_id,
        session_id,
        protected_digest,
        manifest,
        &snapshot,
        snapshot.clone(),
    )
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn executor_policy_for_checkpoint(
    authority: &SigningKey,
    receipt: &SigningKey,
    executor_id: Uuid,
    broker_id: Uuid,
    child_epoch_id: Uuid,
    session_id: Uuid,
    protected_digest: [u8; 32],
    manifest: ProtectedControlPlanePathManifest,
    checkpoint: &ExecutorAuthoritySnapshot,
    supplied_snapshot: ExecutorAuthoritySnapshot,
) -> Result<ExecutorPolicy, ExecutorError> {
    ExecutorPolicy::new(
        ExecutorIdentity {
            platform: ExecutionHostPlatform::Windows,
            executor_id,
            executor_revision: 1,
            executor_executable_sha256: [1; 32],
            broker_id,
            broker_revision: 1,
            broker_executable_sha256: [2; 32],
            protected_control_plane_sha256: protected_digest,
            authority_key_id: "master-action-v1".into(),
            authority_verifying_key: authority.verifying_key(),
            receipt_key_id: "executor-receipt-v1".into(),
            receipt_signing_key: receipt.clone(),
            bound_child_epoch_id: child_epoch_id,
            bound_session_id: session_id,
            bound_session_revision: 1,
            bound_child_epoch_revision: 1,
            bound_feature_lifecycle_revision: 1,
            bound_authority_revision: checkpoint.authority_revision,
            bound_authority_snapshot_sha256: checkpoint.sha256().unwrap(),
            next_action_sequence: 1,
        },
        manifest,
        supplied_snapshot,
    )
}

fn authority_snapshot(
    authority: &SigningKey,
    session_id: Uuid,
    child_epoch_id: Uuid,
    authority_revision: u64,
    emergency_paused: bool,
    revoked: bool,
) -> ExecutorAuthoritySnapshot {
    let mut snapshot = ExecutorAuthoritySnapshot {
        authority_revision,
        session_id,
        session_revision: 1,
        child_epoch_id,
        child_epoch_revision: 1,
        feature_lifecycle_revision: 1,
        emergency_paused,
        revoked,
        signer_key_id: "master-action-v1".into(),
        signature: Vec::new(),
    };
    snapshot.sign(authority).unwrap();
    snapshot
}
