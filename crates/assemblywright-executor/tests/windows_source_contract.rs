#[test]
fn windows_held_handle_image_job_and_failure_order_remain_fail_closed() {
    let source = include_str!("../src/lib.rs").replace("\r\n", "\n");
    let windows = source.find("#[cfg(windows)]\nmod platform").unwrap();
    let windows_source = &source[windows..];
    assert!(windows_source.contains("FILE_FLAG_OPEN_REPARSE_POINT"));
    assert!(windows_source.contains("FILE_LIST_DIRECTORY"));
    assert!(windows_source.contains("Attribute-only directory opens are exempt"));
    assert!(windows_source.contains("FILE_SHARE_READ"));
    assert!(!windows_source.contains("FILE_SHARE_WRITE"));
    assert!(!windows_source.contains("FILE_SHARE_DELETE"));
    assert!(windows_source.contains("JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE"));
    assert!(windows_source.contains("windows_object_identity_sha256"));
    assert!(windows_source.contains("verify_signed_identity(&self.signed_executable)"));
    assert!(windows_source.contains("verify_signed_identity(&self.signed_target)"));

    let image_verification = windows_source
        .find("query_process_image_path(guard.process_raw()?)")
        .unwrap();
    let assignment = windows_source
        .find("AssignProcessToJobObject(guard.job_raw()?")
        .unwrap();
    let resume_closure = windows_source[assignment..]
        .find("let mut resume = ||")
        .map(|offset| assignment + offset)
        .unwrap();
    let authority_recheck = windows_source[resume_closure..]
        .find("before_resume(&mut resume)?")
        .map(|offset| resume_closure + offset)
        .unwrap();
    assert!(image_verification < assignment);
    assert!(assignment < resume_closure);
    assert!(resume_closure < authority_recheck);

    let guard_drop = windows_source
        .find("impl Drop for SuspendedChildGuard")
        .unwrap();
    assert!(windows_source[guard_drop..].contains("TerminateJobObject"));
    assert!(windows_source[guard_drop..].contains("TerminateProcess"));
    assert!(windows_source[guard_drop..].contains("WaitForSingleObject"));
    assert!(windows_source[guard_drop..].contains("FAILURE_REAP_TIMEOUT_MS"));
    assert!(windows_source.contains("CreateProcessW success guarantees two valid handles"));
    assert!(source.contains("if block.is_empty()"));
    assert!(source.contains("value.contains('\\0')"));
    assert!(source.contains("key.to_ascii_uppercase()"));
    assert!(source.contains("backslashes * 2 + 1"));
    assert!(source.contains("backslashes * 2"));
    assert!(source.contains("active_execution: HashMap<Uuid, u64>"));
    assert!(source.contains("failed_epochs: HashSet<Uuid>"));
    assert!(source.contains("complete_execution_resume"));
    assert!(source.contains("self.policy.fail_execution(self.envelope)"));
    assert!(source.contains("ExecutorAuthoritySnapshot"));
    assert!(source.contains("update_authority_snapshot"));
    assert!(source.contains("bound_authority_revision"));
    assert!(source.contains("bound_authority_snapshot_sha256"));
    assert!(source.contains("authority_snapshot.sha256()?"));
    assert!(source.contains("Hold the mutable authority snapshot"));
    assert!(source.contains("causal forced-root count remains 0"));
}
