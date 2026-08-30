use assemblywright_protocol::{
    AssemblyLineAutoRunRequest, AssemblyLineOwnerProjection, AssemblyLineRepositoryIdentity,
    BrainstormingAcceptanceCriterion, BrainstormingOwnerApprovalBinding,
    BrainstormingSpecificationDocument, BrainstormingTargetKind, CanonicalGitHubRepositoryUrl,
    FeatureBrainstormingDraft, FrozenBrainstormingSpecification, OrchestratorCatalog,
    ProjectBrainstormingDraft, ProjectVisibility, RepositoryCreationLifecycle,
    FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION, MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use tempfile::tempdir;
use uuid::Uuid;

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn owner_http_planning_surface_is_authenticated_bounded_inert_and_has_no_start_stop_routes() {
    let directory = tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    let setup = Command::new(binary)
        .arg("--data-dir")
        .arg(directory.path())
        .arg("setup")
        .output()
        .unwrap();
    assert!(
        setup.status.success(),
        "setup failed: {}",
        String::from_utf8_lossy(&setup.stderr)
    );
    let endpoint = unused_loopback_addr();
    let mut server = spawn_server(binary, directory.path(), endpoint);
    read_ready(&mut server.0);
    let token = std::fs::read_to_string(directory.path().join("development.token")).unwrap();
    let token = token.trim();

    assert!(
        get_request(endpoint, "/v1/assembly-line", None).starts_with("HTTP/1.1 401 Unauthorized")
    );
    let response = get_request(endpoint, "/v1/assembly-line", Some(token));
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let projection: AssemblyLineOwnerProjection =
        serde_json::from_value(response_json(&response)).unwrap();
    assert!(projection.assembly_line.auto_run);
    assert_eq!(projection.assembly_line.queue_count, 0);
    assert!(projection.queue.is_empty());

    let malformed = post_request(
        endpoint,
        "/v1/assembly-line/project-drafts",
        Some(token),
        r#"{"schema_version":1,"idea":"do-not-reflect","command":"do-not-run"}"#,
    );
    assert!(
        malformed.starts_with("HTTP/1.1 422 Unprocessable Entity"),
        "{malformed}"
    );
    assert_eq!(
        response_json(&malformed),
        serde_json::json!({"error":"assembly_line_request_rejected"})
    );
    assert!(!malformed.contains("do-not-reflect"));
    assert!(!malformed.contains("do-not-run"));

    let repository = AssemblyLineRepositoryIdentity {
        repository_id: Uuid::new_v4(),
        git_url: CanonicalGitHubRepositoryUrl::parse(
            "https://github.com/owner/native-planning-e2e",
        )
        .unwrap(),
    };
    let draft = ProjectBrainstormingDraft {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft_id: Uuid::new_v4(),
        draft_revision: 1,
        repository: repository.clone(),
        visibility: ProjectVisibility::Private,
        orchestrator_catalog: OrchestratorCatalog::default(),
        orchestrator: Default::default(),
        idea: "Define a private project through bounded brainstorming.".to_string(),
    };
    let draft_response = post_json(endpoint, "/v1/assembly-line/project-drafts", token, &draft);
    assert!(
        draft_response.starts_with("HTTP/1.1 200 OK"),
        "{draft_response}"
    );

    let document = BrainstormingSpecificationDocument {
        title: "Native planning E2E".to_string(),
        outcome: "Create the approved private repository intent without an external effect."
            .to_string(),
        acceptance_criteria: vec![BrainstormingAcceptanceCriterion {
            id: "approval-is-inert".to_string(),
            requirement: "Approval records creation_pending and never calls GitHub.".to_string(),
        }],
        obligations: vec!["Run native tests and retain redacted evidence.".to_string()],
    };
    let frozen = FrozenBrainstormingSpecification {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        specification_id: Uuid::new_v4(),
        specification_revision: 1,
        target_kind: BrainstormingTargetKind::Project,
        draft_id: draft.draft_id,
        draft_revision: draft.draft_revision,
        draft_sha256: draft.canonical_sha256().unwrap(),
        repository: repository.clone(),
        visibility: Some(ProjectVisibility::Private),
        orchestrator_catalog_revision: draft.orchestrator_catalog.catalog_revision,
        orchestrator_catalog_sha256: draft.orchestrator_catalog.catalog_sha256,
        orchestrator_profile_sha256: draft.orchestrator.canonical_sha256().unwrap(),
        specification_sha256: document.canonical_sha256().unwrap(),
        specification: document,
    };
    let frozen_response = post_json(
        endpoint,
        "/v1/assembly-line/frozen-specifications",
        token,
        &frozen,
    );
    assert!(
        frozen_response.starts_with("HTTP/1.1 200 OK"),
        "{frozen_response}"
    );
    let planning_projection: AssemblyLineOwnerProjection =
        serde_json::from_value(response_json(&frozen_response)).unwrap();
    let mut approval = BrainstormingOwnerApprovalBinding {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        approval_id: Uuid::new_v4(),
        approved_at_ms: 10_000,
        owner_control_revision: planning_projection.owner_control_revision,
        target_kind: BrainstormingTargetKind::Project,
        repository: repository.clone(),
        visibility: Some(ProjectVisibility::Private),
        expected_repository_revision: Some(0),
        expected_queue_revision: None,
        draft_id: draft.draft_id,
        draft_revision: draft.draft_revision,
        draft_sha256: draft.canonical_sha256().unwrap(),
        orchestrator_catalog_revision: frozen.orchestrator_catalog_revision,
        orchestrator_catalog_sha256: frozen.orchestrator_catalog_sha256,
        specification_id: frozen.specification_id,
        specification_revision: frozen.specification_revision,
        specification_sha256: frozen.specification_sha256,
        orchestrator_profile_sha256: frozen.orchestrator_profile_sha256,
        owner_approval_sha256: [0; 32],
    };
    approval.owner_approval_sha256 = approval.canonical_approval_sha256().unwrap();
    let approved = post_json(
        endpoint,
        "/v1/assembly-line/project-approvals",
        token,
        &approval,
    );
    assert!(approved.starts_with("HTTP/1.1 200 OK"), "{approved}");
    let approved_json = response_json(&approved);
    assert_eq!(approved_json["visibility"], "private");
    assert_eq!(approved_json["lifecycle"], "creation_pending");
    assert_eq!(approved_json["effect_possible"], false);
    assert!(approved_json["creation_evidence_sha256"].is_null());
    let replayed_approval = post_json(
        endpoint,
        "/v1/assembly-line/project-approvals",
        token,
        &approval,
    );
    assert_eq!(response_json(&replayed_approval), approved_json);

    let observed = get_request(endpoint, "/v1/assembly-line", Some(token));
    let observed: AssemblyLineOwnerProjection =
        serde_json::from_value(response_json(&observed)).unwrap();
    assert_eq!(observed.repositories.len(), 1);
    assert_eq!(
        observed.repositories[0].lifecycle,
        RepositoryCreationLifecycle::CreationPending
    );
    assert_eq!(
        observed.repositories[0].visibility,
        ProjectVisibility::Private
    );
    assert!(observed.queue.is_empty());

    let blocked_feature = FeatureBrainstormingDraft {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft_id: Uuid::new_v4(),
        draft_revision: 1,
        repository,
        expected_repository_revision: 1,
        orchestrator_catalog: OrchestratorCatalog::default(),
        orchestrator: Default::default(),
        idea: "Do not reflect this feature idea in an error.".to_string(),
    };
    let blocked = post_json(
        endpoint,
        "/v1/assembly-line/feature-drafts",
        token,
        &blocked_feature,
    );
    assert!(blocked.starts_with("HTTP/1.1 409 Conflict"), "{blocked}");
    assert_eq!(
        response_json(&blocked),
        serde_json::json!({"error":"assembly_line_request_rejected"})
    );
    assert!(!blocked.contains(&blocked_feature.idea));

    let request = AssemblyLineAutoRunRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        expected_state_revision: projection.assembly_line.state_revision,
        auto_run: false,
    };
    let body = serde_json::to_string(&request).unwrap();
    let toggled = post_request(endpoint, "/v1/assembly-line/auto-run", Some(token), &body);
    assert!(toggled.starts_with("HTTP/1.1 200 OK"), "{toggled}");
    assert_eq!(
        response_json(&toggled)["resulting_state"]["auto_run"],
        false
    );
    let replay = post_request(endpoint, "/v1/assembly-line/auto-run", Some(token), &body);
    assert_eq!(response_json(&replay), response_json(&toggled));

    for forbidden in [
        "/v1/assembly-line/start",
        "/v1/assembly-line/stop",
        "/v1/assembly-line/emergency-pause",
    ] {
        let response = post_request(endpoint, forbidden, Some(token), "{}");
        assert!(
            response.starts_with("HTTP/1.1 404 Not Found"),
            "unexpected route {forbidden}: {response}"
        );
    }

    let oversized = "x".repeat(MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES + 1);
    let response = post_request(
        endpoint,
        "/v1/assembly-line/project-drafts",
        Some(token),
        &oversized,
    );
    assert!(
        response.starts_with("HTTP/1.1 413 Payload Too Large"),
        "{response}"
    );
}

fn spawn_server(binary: &str, data_dir: &Path, endpoint: SocketAddr) -> ChildGuard {
    ChildGuard(
        Command::new(binary)
            .arg("--data-dir")
            .arg(data_dir)
            .arg("serve")
            .arg("--bind")
            .arg(endpoint.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
}

fn read_ready(child: &mut Child) {
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let value: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("bad ready receipt {line:?}: {error}"));
    assert_eq!(value["status"], "ready");
}

fn unused_loopback_addr() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}

fn get_request(endpoint: SocketAddr, path: &str, token: Option<&str>) -> String {
    let mut stream = TcpStream::connect(endpoint).unwrap();
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {endpoint}\r\n{authorization}Connection: close\r\n\r\n"
    )
    .unwrap();
    read_response(&mut stream)
}

fn post_request(endpoint: SocketAddr, path: &str, token: Option<&str>, body: &str) -> String {
    let mut stream = TcpStream::connect(endpoint).unwrap();
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(stream, "POST {path} HTTP/1.1\r\nHost: {endpoint}\r\n{authorization}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    read_response(&mut stream)
}

fn post_json<T: serde::Serialize>(
    endpoint: SocketAddr,
    path: &str,
    token: &str,
    body: &T,
) -> String {
    post_request(
        endpoint,
        path,
        Some(token),
        &serde_json::to_string(body).unwrap(),
    )
}

fn read_response(stream: &mut TcpStream) -> String {
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn response_json(response: &str) -> Value {
    serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
}
