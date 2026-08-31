#[test]
fn runtime_has_no_ambient_listener_and_preserves_effect_disablement() {
    let source = include_str!("../src/runtime.rs");
    assert!(!source.contains("TcpListener"));
    assert!(!source.contains("UdpSocket"));
    assert!(source.contains("stdin().lock()") || source.contains("run_stdio"));
    assert!(source.contains("ValidatedEffectDisabled"));
    assert!(source.contains("restart_quarantined"));
    assert!(source.contains("O_NOFOLLOW"));
    assert!(source.contains("FILE_FLAG_OPEN_REPARSE_POINT"));
    assert!(source.contains("current_executable_sha256"));
    assert!(source.contains("request.request_sequence != self.next_request_sequence"));
    assert!(source.contains("request.authority_revision != self.config.bound_authority_revision"));
}
