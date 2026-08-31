#![cfg(unix)]

use assemblywright_master::BrainstormingDraft;
use assemblywright_protocol::{
    AssemblyLineRepositoryIdentity, CanonicalGitHubRepositoryUrl, OrchestratorCatalog,
    ProjectBrainstormingDraft, ProjectVisibility, FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::tempdir;
use uuid::Uuid;

#[derive(Serialize)]
struct ProviderRequest<'a> {
    schema_version: u16,
    operation: &'a str,
    provider_id: &'a str,
    model_id: &'a str,
    idempotency_key_sha256: &'a str,
    information_classification: &'a str,
    owner_cloud_disclosure_sha256: [u8; 32],
    draft: Option<&'a BrainstormingDraft>,
}

#[test]
fn actual_adapter_generates_persists_and_reconciles_without_redisclosure() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    provision(root);
    let draft = BrainstormingDraft::Project(ProjectBrainstormingDraft {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft_id: Uuid::new_v4(),
        draft_revision: 1,
        repository: AssemblyLineRepositoryIdentity {
            repository_id: Uuid::new_v4(),
            git_url: CanonicalGitHubRepositoryUrl::parse(
                "https://github.com/owner/actual-adapter-e2e",
            )
            .unwrap(),
        },
        visibility: ProjectVisibility::Public,
        orchestrator_catalog: OrchestratorCatalog::default(),
        orchestrator: Default::default(),
        idea: "Create a bounded planning-only specification.".to_string(),
    });
    let key = "11".repeat(32);
    let disclosure = [7; 32];
    let generate = ProviderRequest {
        schema_version: 1,
        operation: "generate",
        provider_id: "openai.codex",
        model_id: "gpt-5.6-sol",
        idempotency_key_sha256: &key,
        information_classification: "public",
        owner_cloud_disclosure_sha256: disclosure,
        draft: Some(&draft),
    };
    let first = invoke(root, &generate);
    assert!(
        first.status.success(),
        "adapter generate failed: {}; calls={:?}; reconciliation={:?}",
        String::from_utf8_lossy(&first.stderr),
        std::fs::read_to_string(root.join("codex-home/calls")),
        std::fs::read_dir(root.join("reconciliation"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>()
    );
    let specification: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(specification["title"], "Actual adapter E2E");
    assert_eq!(
        std::fs::read_to_string(root.join("codex-home/calls")).unwrap(),
        "call\n"
    );

    let replay = invoke(root, &generate);
    assert!(replay.status.success());
    assert_eq!(replay.stdout, first.stdout);
    assert_eq!(
        std::fs::read_to_string(root.join("codex-home/calls")).unwrap(),
        "call\n"
    );

    let reconcile = ProviderRequest {
        schema_version: 1,
        operation: "reconcile",
        provider_id: "openai.codex",
        model_id: "gpt-5.6-sol",
        idempotency_key_sha256: &key,
        information_classification: "public",
        owner_cloud_disclosure_sha256: disclosure,
        draft: None,
    };
    let reconciled = invoke(root, &reconcile);
    assert!(reconciled.status.success());
    let value: serde_json::Value = serde_json::from_slice(&reconciled.stdout).unwrap();
    assert_eq!(value["status"], "found");
    assert_eq!(value["specification"], specification);
    assert_eq!(
        std::fs::read_to_string(root.join("codex-home/calls")).unwrap(),
        "call\n"
    );
}

#[cfg(unix)]
#[test]
fn actual_adapter_rejects_a_linked_private_temp_before_codex() {
    assert_linked_private_directory_rejected("temp", "linked-temp-rejected");
}

#[cfg(unix)]
#[test]
fn actual_adapter_rejects_linked_private_local_app_data_before_codex() {
    assert_linked_private_directory_rejected("local-app-data", "linked-local-app-data-rejected");
}

#[cfg(unix)]
fn assert_linked_private_directory_rejected(directory_name: &str, repository_name: &str) {
    use std::os::unix::fs::symlink;

    let directory = tempdir().unwrap();
    let root = directory.path();
    provision(root);
    let external = tempdir().unwrap();
    std::fs::remove_dir(root.join(directory_name)).unwrap();
    symlink(external.path(), root.join(directory_name)).unwrap();
    let draft = BrainstormingDraft::Project(ProjectBrainstormingDraft {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft_id: Uuid::new_v4(),
        draft_revision: 1,
        repository: AssemblyLineRepositoryIdentity {
            repository_id: Uuid::new_v4(),
            git_url: CanonicalGitHubRepositoryUrl::parse(&format!(
                "https://github.com/owner/{repository_name}"
            ))
            .unwrap(),
        },
        visibility: ProjectVisibility::Public,
        orchestrator_catalog: OrchestratorCatalog::default(),
        orchestrator: Default::default(),
        idea: "Reject the linked provider runtime directory before transport.".to_string(),
    });
    let key = "22".repeat(32);
    let request = ProviderRequest {
        schema_version: 1,
        operation: "generate",
        provider_id: "openai.codex",
        model_id: "gpt-5.6-sol",
        idempotency_key_sha256: &key,
        information_classification: "public",
        owner_cloud_disclosure_sha256: [8; 32],
        draft: Some(&draft),
    };

    let output = invoke(root, &request);
    assert_eq!(output.status.code(), Some(11));
    assert!(!root.join("codex-home/calls").exists());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

fn invoke(root: &Path, request: &ProviderRequest<'_>) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-brainstorming-provider"))
        .current_dir(root)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(request).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn provision(root: &Path) {
    let codex = root.join("codex");
    let schema = root.join("brainstorming-output-schema.json");
    let codex_home = root.join("codex-home");
    let reconciliation = root.join("reconciliation");
    let temporary = root.join("temp");
    let local_app_data = root.join("local-app-data");
    std::fs::create_dir(&codex_home).unwrap();
    std::fs::create_dir(&reconciliation).unwrap();
    std::fs::create_dir(&temporary).unwrap();
    std::fs::create_dir(&local_app_data).unwrap();
    std::fs::write(
        &codex,
        r##"#!/bin/sh
/bin/echo call >> codex-home/calls
/bin/echo '{"title":"Actual adapter E2E","outcome":"Return one exact bounded specification.","acceptance_criteria":[{"id":"exact-output","requirement":"The adapter validates and persists the exact response."}],"obligations":["Retain only redacted reconciliation evidence."]}'
"##,
    )
    .unwrap();
    std::fs::write(
        &schema,
        include_bytes!("../resources/brainstorming-output-schema.json"),
    )
    .unwrap();
    let provider = env!("CARGO_BIN_EXE_assemblywright-brainstorming-provider");
    let gh = root.join("gh");
    std::fs::write(&gh, b"not invoked").unwrap();
    let config = serde_json::json!({
        "schema_version": 1,
        "enabled": true,
        "catalog_revision": 1,
        "provider_id": "openai.codex",
        "model_id": "gpt-5.6-sol",
        "adapter_kind": "codex_exec_v1",
        "brainstorming_provider_sha256": file_sha(provider),
        "codex_executable_sha256": file_sha(&codex),
        "output_schema_sha256": file_sha(&schema),
        "gh_executable_sha256": file_sha(&gh),
        "github_owner": "owner"
    });
    std::fs::write(
        root.join("runtime.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    for directory in [
        root,
        codex_home.as_path(),
        reconciliation.as_path(),
        temporary.as_path(),
        local_app_data.as_path(),
    ] {
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o700)).unwrap();
    for file in [root.join("runtime.json"), schema, gh] {
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn file_sha(path: impl AsRef<Path>) -> String {
    let digest = Sha256::digest(std::fs::read(path).unwrap());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
