#[test]
fn provider_writable_roots_have_an_exact_low_integrity_boundary() {
    let containment = include_str!("../src/planning_runtime/windows_containment.rs");

    for required in [
        "LABEL_SECURITY_INFORMATION",
        "SYSTEM_MANDATORY_LABEL_ACE",
        "SYSTEM_MANDATORY_LABEL_ACE_TYPE",
        "SYSTEM_MANDATORY_LABEL_NO_WRITE_UP",
        "S-1-16-4096",
        "fn install_provider_low_integrity_labels(",
        "fn validate_provider_integrity_scope(",
        "SetSecurityInfo(",
        "GetSecurityInfo(",
    ] {
        assert!(
            containment.contains(required),
            "missing provider integrity-label contract: {required}"
        );
    }

    let provisioning_start = containment
        .find("pub(super) fn validate_provisioning(")
        .unwrap();
    let provisioning_end = containment[provisioning_start..]
        .find("impl ProfileBinding {")
        .unwrap()
        + provisioning_start;
    let provisioning = &containment[provisioning_start..provisioning_end];
    let provider_dacl = provisioning.find("provisioning.provider_root,").unwrap();
    let install = provisioning
        .find("install_provider_low_integrity_labels(")
        .unwrap();
    let final_validation = provisioning
        .find("validate_provider_integrity_scope(")
        .unwrap();
    assert!(provider_dacl < install && install < final_validation);
}

#[test]
fn immutable_provider_objects_are_not_downgraded_to_low_integrity() {
    let containment = include_str!("../src/planning_runtime/windows_containment.rs");

    assert!(containment.contains("IntegrityLabelScope::Unlabeled"));
    assert!(containment.contains("IntegrityLabelScope::WritableRoot"));
    assert!(containment.contains("IntegrityLabelScope::WritableChild"));
    assert!(containment.contains("IntegrityObject::open(root, true, false)"));
    assert!(containment.contains("&root_guard, IntegrityLabelScope::Unlabeled"));
    assert!(containment.contains("if *writable"));
    assert!(containment.contains("validate_writable_integrity_tree("));
    assert!(containment.contains("ProviderIntegrityState::Incomplete"));

    let open_start = containment.find("fn open_integrity_file(").unwrap();
    let open_end = containment[open_start..]
        .find("fn validate_root_allowlist(")
        .unwrap()
        + open_start;
    let open = &containment[open_start..open_end];
    assert!(open.contains("share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)"));
    assert!(!open.contains("FILE_SHARE_DELETE"));
    assert!(open.contains("let access = READ_CONTROL | if write { WRITE_OWNER } else { 0 };"));
    assert!(containment.contains("let tree = IntegrityTree::collect(path)?"));
    assert!(containment.contains("tree.revalidate_paths()?"));
    assert!(containment.contains("object.revalidate_path()?"));

    let launch = &containment[containment.find("pub(super) fn run_command(").unwrap()
        ..containment.find("fn complete_signaled_process(").unwrap()];
    assert!(launch.matches(".revalidate()").count() >= 4);
}

#[test]
fn successful_output_requires_post_command_containment_revalidation() {
    let containment = include_str!("../src/planning_runtime/windows_containment.rs");
    let completion_start = containment.find("fn complete_signaled_process(").unwrap();
    let completion_end = containment[completion_start..]
        .find("fn discard_reader(")
        .unwrap()
        + completion_start;
    let completion = &containment[completion_start..completion_end];

    let post_command_revalidation = completion.find("profile.revalidate().is_err()").unwrap();
    let output_acceptance = completion
        .find("classify_completed_output(exit_code, output)")
        .unwrap();
    assert!(post_command_revalidation < output_acceptance);
    assert!(completion.contains("bytes.zeroize()"));
}
