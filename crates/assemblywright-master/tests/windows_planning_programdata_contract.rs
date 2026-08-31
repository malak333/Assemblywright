#[test]
fn windows_runtime_is_schema_v4_program_data_bound_and_revalidated_per_effect() {
    let runtime = include_str!("../src/planning_runtime.rs");
    let containment = include_str!("../src/planning_runtime/windows_containment.rs");

    assert!(runtime.contains("const MASTER_CONFIG: &str = \"runtime-v4.json\";"));
    assert!(runtime.contains("schema_version == if cfg!(windows) { 4 } else { 1 }"));
    assert!(runtime.contains("runtime_instance: Option<String>"));
    assert!(runtime.contains("windows_containment::canonical_runtime_root("));
    assert!(containment.contains("SHGetKnownFolderPath("));
    assert!(containment.contains("&FOLDERID_ProgramData"));
    assert!(containment.contains("RUNTIME_VENDOR_DIRECTORY: &str = \"Assemblywright\""));
    assert!(containment.contains("RUNTIME_NAMESPACE_DIRECTORY: &str = \"planning-runtime\""));
    assert!(containment.contains("(program_data, RuntimeDirectoryAcl::SharedTraverse)"));
    assert!(containment.contains("runtime_ancestors: Vec<DirectoryBinding>"));
    assert!(containment.contains("handle: Arc<File>"));
    let directory_open = &containment[containment.find("fn open_bound_directory(").unwrap()
        ..containment
            .find("fn validate_runtime_directory_acl(")
            .unwrap()];
    assert!(directory_open.contains("share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)"));
    assert!(!directory_open.contains("FILE_SHARE_DELETE"));
    assert!(containment.contains("GetSecurityInfo("));
    assert!(containment.contains("revalidate_runtime_ancestors("));
    assert!(containment.contains("&self.runtime_ancestors"));
    assert!(containment.contains("file_identity(&binding.handle)"));
    assert!(containment.contains("validate_runtime_directory_acl(binding"));
    assert!(containment.contains("let _runtime_directory_guards = profile"));
    let launch = &containment[containment.find("pub(super) fn run_command(").unwrap()
        ..containment.find("fn complete_signaled_process(").unwrap()];
    let final_revalidation = launch.rfind("if profile.revalidate().is_err()").unwrap();
    let resume = launch.find("ResumeThread(suspended.thread())").unwrap();
    assert!(final_revalidation < resume);
}

#[test]
fn provisioning_merges_only_exact_noninheriting_traverse_on_held_parent_handles() {
    let provision = include_str!("../../../scripts/provision-planning-runtime.ps1");

    assert!(provision.contains("ContractVersion=10"));
    assert!(provision.contains("ContractVersion -ne 10"));
    assert!(provision.contains("READ_CONTROL=0x00020000"));
    assert!(provision.contains(
        "FILE_READ_ATTRIBUTES|READ_CONTROL|WRITE_DAC|WRITE_OWNER,FILE_SHARE_READ|FILE_SHARE_WRITE"
    ));
    assert!(provision.contains("Open(path,FILE_READ_ATTRIBUTES,FILE_SHARE_READ|FILE_SHARE_WRITE"));
    assert!(provision.contains(
        "Open(path,FILE_READ_ATTRIBUTES|READ_CONTROL|WRITE_DAC,FILE_SHARE_READ|FILE_SHARE_WRITE"
    ));
    assert!(provision.contains("OpenSharedAclDirectory([IO.Path]::GetFullPath($programDataPath))"));
    assert!(provision.contains("CommonApplicationData"));
    assert!(provision.contains("$runtimeNamespace = Join-Path $runtimeVendor 'planning-runtime'"));
    assert!(provision.contains("$planning = Join-Path $runtimeNamespace $ServiceName"));
    assert!(provision.contains("$programDataProof.MergeProfileTraverse($providerSid,$githubSid)"));
    assert!(provision.contains("$runtimeVendorProof.MergeProfileTraverse($providerSid,$githubSid)"));
    assert!(
        provision.contains("$runtimeNamespaceProof.MergeProfileTraverse($providerSid,$githubSid)")
    );
    assert!(provision.contains("SetEntriesInAclW((uint)entries.Length,entries,oldAcl"));
    assert!(
        provision.contains("AccessPermissions=FILE_TRAVERSE,AccessMode=SET_ACCESS,Inheritance=0")
    );
    assert!(provision.contains("SetSecurityInfo(handle,SE_FILE_OBJECT,DACL_SECURITY_INFORMATION"));
    assert!(provision.contains("if(descriptor==IntPtr.Zero||oldAcl==IntPtr.Zero)"));
    assert!(!provision.contains("Set-Acl"));
    assert!(provision.contains("schema_version=4"));
    assert!(provision.contains("runtime_instance=$ServiceName"));
    assert!(provision.contains("Join-Path $locator 'runtime-v4.json'"));
    assert!(provision.contains("Set-HeldAcl $targetManifest.RootProof $true (MasterRules $true)"));
}

#[test]
fn native_owner_checks_follow_the_same_external_runtime_binding() {
    let check = include_str!("../../../scripts/check-planning-runtime.ps1");
    let native = include_str!("../../../scripts/planning-runtime-native-proof.ps1");

    assert!(check.contains("planning-runtime\\runtime-v4.json"));
    assert!(check.contains("$locatorConfig.schema_version -ne 4"));
    assert!(check.contains("CommonApplicationData"));
    assert!(check.contains("$locatorConfig.runtime_instance"));
    assert!(check.contains("$runtimeRoot 'master-check\\assemblywright-master.exe'"));
    assert!(native.contains("CommonApplicationData"));
    assert!(native.contains("$runtimeRoot 'provider'"));
    assert!(!native.contains("$data 'planning-runtime\\provider'"));
}
