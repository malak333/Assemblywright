#[test]
fn runtime_has_no_ambient_listener_and_keeps_dispatch_effect_disabled() {
    let source = include_str!("../src/runtime.rs");
    let broker = include_str!("../src/lib.rs");
    assert!(source.contains("RUNTIME_SCHEMA_VERSION: u16 = 1"));
    assert!(!source.contains("TcpListener"));
    assert!(!source.contains("UdpSocket"));
    assert!(source.contains("stdin().lock()") || source.contains("run_stdio"));
    assert!(source.contains("ValidatedEffectDisabled"));
    assert!(!source.contains("admission.execute()"));
    assert!(source.contains("dedicated one-shot proof seam"));
    assert!(source.contains("restart_quarantined"));
    assert!(source.contains("O_NOFOLLOW"));
    assert!(source.contains("FILE_FLAG_OPEN_REPARSE_POINT"));
    assert!(source.contains("current_executable_sha256"));
    assert!(source.contains("request.request_sequence != self.next_request_sequence"));
    assert!(source.contains("request.authority_revision != self.config.bound_authority_revision"));
    assert!(broker.contains("NtCreateFile"));
    assert!(broker.contains("RootDirectory"));
    assert!(broker.contains("WindowsRetainedAncestor"));
    assert!(broker.contains("EffectPossibleReconciliationRequired"));
    assert!(broker.contains("execute_windows_create_directory_once"));
    assert!(broker.contains("FILE_OPEN_REPARSE_POINT"));
    assert!(broker.contains("FILE_CREATE"));
    assert!(!broker.contains("FILE_SHARE_DELETE"));
    let production_broker = broker
        .split("#[cfg(all(test, windows))]")
        .next()
        .expect("production broker source");
    assert!(!production_broker.contains("fs::create_dir("));
}
