use assemblywright_master::{
    ApprovedFeatureSpecification, DeviceRegistration, FeatureGrantRevisions, MasterProcess,
    RepositoryGrantKind, RepositoryGrantRevision, MAX_CONVEYOR_NONTERMINAL_FEATURES,
};
use assemblywright_protocol::{
    CapabilityDescriptor, DeviceId, DeviceRole, FeatureConveyorOwnerBridgeDesignationRequest,
    FeatureConveyorRepositoryGrantKind, FeatureConveyorRepositoryGrantRequest,
    FeatureConveyorRepositoryGrantRevision, FeatureConveyorRepositoryPreflightRequest,
    FeatureConveyorRepositoryScopeDocument, FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
    MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES, MAX_WIRE_FRAME_BYTES,
};
use rusqlite::Connection;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use tempfile::tempdir;
use uuid::Uuid;

struct ChildGuard {
    child: Child,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn owner_control_bridge_designation_is_owner_authenticated_strict_and_redacted() {
    let directory = tempdir().expect("temporary owner-control directory");
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    assert_success(&run(binary, directory.path(), ["setup"]), "setup");
    let endpoint = unused_loopback_addr();
    let mut server = spawn_server(binary, directory.path(), endpoint);
    read_ready(&mut server.child);
    let token = std::fs::read_to_string(directory.path().join("development.token")).unwrap();
    let token = token.trim();
    let owner = DeviceRegistration {
        device_id: DeviceId::new(Uuid::new_v4()),
        device_name: "owner-control-e2e".to_string(),
        role: DeviceRole::MacBridge,
        registry_revision: 1,
        capabilities: vec![CapabilityDescriptor::mlx_reasoning(
            "owner-control-mlx",
            32 * 1024,
            32 * 1024,
        )],
    };
    let fixture = DeviceRegistration {
        device_id: DeviceId::new(Uuid::new_v4()),
        device_name: "fixture-owner-denied".to_string(),
        role: DeviceRole::MacBridge,
        registry_revision: 1,
        capabilities: vec![CapabilityDescriptor::fixture_reasoning()],
    };
    for registration in [&owner, &fixture] {
        let response = post_request(
            endpoint,
            "/v1/development/devices/register",
            Some(token),
            &serde_json::to_string(registration).unwrap(),
        );
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    }

    let request = FeatureConveyorOwnerBridgeDesignationRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        device_id: owner.device_id,
        expected_designation_revision: 0,
    };
    let request_json = serde_json::to_string(&request).unwrap();
    let unauthorized = post_request(
        endpoint,
        "/v1/feature-conveyor/owner-control-bridge",
        None,
        &request_json,
    );
    assert!(unauthorized.starts_with("HTTP/1.1 401 Unauthorized"));
    let malformed = post_request(
        endpoint,
        "/v1/feature-conveyor/owner-control-bridge",
        Some(token),
        r#"{"schema_version":1,"device_id":"secret malformed value","expected_designation_revision":0}"#,
    );
    assert!(malformed.starts_with("HTTP/1.1 422 Unprocessable Entity"));
    assert_eq!(
        response_json(&malformed)["error"],
        "owner_control_designation_request_rejected"
    );
    assert!(!malformed.contains("secret malformed value"));
    let fixture_request = FeatureConveyorOwnerBridgeDesignationRequest {
        device_id: fixture.device_id,
        ..request
    };
    let fixture_denied = post_request(
        endpoint,
        "/v1/feature-conveyor/owner-control-bridge",
        Some(token),
        &serde_json::to_string(&fixture_request).unwrap(),
    );
    assert!(fixture_denied.starts_with("HTTP/1.1 409 Conflict"));
    assert_eq!(
        response_json(&fixture_denied)["error"],
        "owner_control_designation_rejected"
    );

    let designated = post_request(
        endpoint,
        "/v1/feature-conveyor/owner-control-bridge",
        Some(token),
        &request_json,
    );
    assert!(designated.starts_with("HTTP/1.1 200 OK"), "{designated}");
    let designated_json = response_json(&designated);
    assert_exact_object_keys(
        &designated_json,
        &[
            "schema_version",
            "device_id",
            "registry_revision",
            "designation_revision",
            "status",
        ],
    );
    assert_eq!(designated_json["schema_version"], 1);
    assert_eq!(designated_json["designation_revision"], 1);
    assert_eq!(designated_json["status"], "designated");
    let stale = post_request(
        endpoint,
        "/v1/feature-conveyor/owner-control-bridge",
        Some(token),
        &request_json,
    );
    assert!(stale.starts_with("HTTP/1.1 409 Conflict"));
    assert_eq!(
        response_json(&stale)["error"],
        "owner_control_designation_rejected"
    );
    assert!(!stale.contains("expected"));

    let remote_route_on_loopback = post_request(
        endpoint,
        "/v1/distributed/feature-conveyor/approved-features",
        Some(token),
        "{}",
    );
    assert!(remote_route_on_loopback.starts_with("HTTP/1.1 404 Not Found"));
    let audit: String = Connection::open(directory.path().join("master.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT redacted_metadata_json FROM feature_conveyor_audit
             WHERE event_kind = 'owner_control_bridge_designated'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!audit.contains(&owner.device_id.0.to_string()));
    assert!(!audit.contains(&owner.device_name));
    assert!(!audit.contains("owner-control-mlx"));
}

#[test]
fn repository_grant_routes_are_owner_authenticated_strict_cas_bound_and_redacted() {
    let directory = tempdir().expect("temporary repository-grant directory");
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    assert_success(&run(binary, directory.path(), ["setup"]), "setup");
    let endpoint = unused_loopback_addr();
    let mut server = spawn_server(binary, directory.path(), endpoint);
    read_ready(&mut server.child);
    let token = std::fs::read_to_string(directory.path().join("development.token")).unwrap();
    let token = token.trim();
    let repository_id = Uuid::new_v4();
    let status_path = format!("/v1/feature-conveyor/repositories/{repository_id}/grants");

    let unauthorized = get_request(endpoint, &status_path, None);
    assert!(unauthorized.starts_with("HTTP/1.1 401 Unauthorized"));
    let malformed_status = get_request(
        endpoint,
        "/v1/feature-conveyor/repositories/private-invalid-repository/grants",
        Some(token),
    );
    assert!(malformed_status.starts_with("HTTP/1.1 422 Unprocessable Entity"));
    assert_eq!(
        response_json(&malformed_status),
        serde_json::json!({"error":"repository_grant_status_request_rejected"})
    );
    assert!(!malformed_status.contains("private-invalid-repository"));

    let empty = get_request(endpoint, &status_path, Some(token));
    assert!(empty.starts_with("HTTP/1.1 200 OK"), "{empty}");
    let empty_json = response_json(&empty);
    assert_exact_object_keys(
        &empty_json,
        &[
            "schema_version",
            "repository_id",
            "emergency_paused",
            "emergency_pause_revision",
            "registration",
            "cloud_disclosure",
            "autonomous_publication",
        ],
    );
    assert_eq!(empty_json["schema_version"], 1);
    assert_eq!(empty_json["repository_id"], repository_id.to_string());
    assert_eq!(empty_json["registration"], Value::Null);
    assert_eq!(empty_json["cloud_disclosure"], Value::Null);
    assert_eq!(empty_json["autonomous_publication"], Value::Null);

    let request = FeatureConveyorRepositoryGrantRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        expected_current_revision: 0,
        expected_emergency_pause_revision: 0,
        grant: FeatureConveyorRepositoryGrantRevision {
            repository_id,
            kind: FeatureConveyorRepositoryGrantKind::Registration,
            revision: 1,
            scope_sha256: Sha256::digest("private exact repository scope").into(),
            owner_approval_sha256: Sha256::digest("private exact owner approval").into(),
            expires_at_ms: None,
            revoked: false,
        },
    };
    let request_json = serde_json::to_string(&request).unwrap();
    let unauthorized_post = post_request(
        endpoint,
        "/v1/feature-conveyor/repository-grants",
        None,
        &request_json,
    );
    assert!(unauthorized_post.starts_with("HTTP/1.1 401 Unauthorized"));
    let duplicate = request_json.replacen(
        "\"grant\":{",
        "\"grant\":{\"revision\":1,\"private_scope\":\"must-not-echo\",",
        1,
    );
    let malformed_post = post_request(
        endpoint,
        "/v1/feature-conveyor/repository-grants",
        Some(token),
        &duplicate,
    );
    assert!(malformed_post.starts_with("HTTP/1.1 422 Unprocessable Entity"));
    assert_eq!(
        response_json(&malformed_post),
        serde_json::json!({"error":"repository_grant_request_rejected"})
    );
    assert!(!malformed_post.contains("must-not-echo"));

    let recorded = post_request(
        endpoint,
        "/v1/feature-conveyor/repository-grants",
        Some(token),
        &request_json,
    );
    assert!(recorded.starts_with("HTTP/1.1 200 OK"), "{recorded}");
    let receipt = response_json(&recorded);
    assert_exact_object_keys(
        &receipt,
        &[
            "schema_version",
            "repository_id",
            "kind",
            "revision",
            "scope_sha256",
            "owner_approval_sha256",
            "expires_at_ms",
            "revoked",
            "emergency_pause_revision",
            "status",
        ],
    );
    assert_eq!(receipt["repository_id"], repository_id.to_string());
    assert_eq!(receipt["kind"], "registration");
    assert_eq!(receipt["revision"], 1);
    assert_eq!(receipt["revoked"], false);
    assert_eq!(receipt["status"], "recorded");

    let stale = post_request(
        endpoint,
        "/v1/feature-conveyor/repository-grants",
        Some(token),
        &request_json,
    );
    assert!(stale.starts_with("HTTP/1.1 409 Conflict"));
    assert_eq!(
        response_json(&stale),
        serde_json::json!({"error":"repository_grant_recording_rejected"})
    );

    let populated = get_request(endpoint, &status_path, Some(token));
    assert!(populated.starts_with("HTTP/1.1 200 OK"), "{populated}");
    let populated_json = response_json(&populated);
    let registration = &populated_json["registration"];
    assert_exact_object_keys(
        registration,
        &[
            "revision",
            "scope_sha256",
            "owner_approval_sha256",
            "expires_at_ms",
            "revoked",
            "active",
        ],
    );
    assert_eq!(registration["revision"], 1);
    assert_eq!(registration["active"], true);
    assert_eq!(populated_json["cloud_disclosure"], Value::Null);
    assert_eq!(populated_json["autonomous_publication"], Value::Null);

    let audit: String = Connection::open(directory.path().join("master.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT redacted_metadata_json FROM feature_conveyor_audit
             WHERE event_kind = 'repository_grant_revision_recorded'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!audit.contains(&repository_id.to_string()));
    assert!(!audit.contains("private"));
    assert!(audit.contains("\"side_effect_executed\":false"));
}

#[test]
fn repository_preflight_is_owner_only_filesystem_identity_observation_and_redacted() {
    let directory = tempdir().expect("temporary repository-preflight master directory");
    let repository = tempdir().expect("temporary repository-preflight Git directory");
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    assert_success(&run(binary, directory.path(), ["setup"]), "setup");
    git(repository.path(), &["init", "-b", "main"]);
    git(
        repository.path(),
        &["config", "user.name", "Assemblywright Test"],
    );
    git(
        repository.path(),
        &["config", "user.email", "assemblywright@example.invalid"],
    );
    std::fs::write(repository.path().join("README.md"), "bounded fixture\n").unwrap();
    git(repository.path(), &["add", "README.md"]);
    git(repository.path(), &["commit", "-m", "bounded fixture"]);
    let head = git_stdout(repository.path(), &["rev-parse", "HEAD"]);
    let repository_path = std::fs::canonicalize(repository.path()).unwrap();

    let endpoint = unused_loopback_addr();
    let mut server = spawn_server(binary, directory.path(), endpoint);
    read_ready(&mut server.child);
    let token = std::fs::read_to_string(directory.path().join("development.token")).unwrap();
    let token = token.trim();
    let repository_id = Uuid::new_v4();
    let scope = FeatureConveyorRepositoryScopeDocument {
        repository_id,
        repository_path: repository_path.to_string_lossy().into_owned(),
        expected_base_branch: "main".to_string(),
        expected_head_commit: head.clone(),
    };
    let mut request = FeatureConveyorRepositoryPreflightRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        scope_sha256: scope.canonical_scope_sha256().unwrap(),
        scope,
        registration_grant_revision: 1,
        expected_emergency_pause_revision: 0,
    };
    record_preflight_registration_grant(endpoint, token, &request, 0);

    let request_json = serde_json::to_string(&request).unwrap();
    let unauthorized = post_request(
        endpoint,
        "/v1/feature-conveyor/repository-preflight",
        None,
        &request_json,
    );
    assert!(unauthorized.starts_with("HTTP/1.1 401 Unauthorized"));
    let malformed = post_request(
        endpoint,
        "/v1/feature-conveyor/repository-preflight",
        Some(token),
        r#"{"schema_version":1,"private_path":"must-not-leak"}"#,
    );
    assert!(malformed.starts_with("HTTP/1.1 422 Unprocessable Entity"));
    assert_eq!(
        response_json(&malformed),
        serde_json::json!({"error":"repository_preflight_request_rejected"})
    );
    assert!(!malformed.contains("must-not-leak"));
    for forbidden_path in [
        "//server/share/repository",
        r"\\server\share\repository",
        r"\\?\C:\repository",
        r"\\.\C:\repository",
    ] {
        let mut forbidden = serde_json::to_value(&request).unwrap();
        forbidden["scope"]["repository_path"] = Value::String(forbidden_path.to_string());
        let response = post_request(
            endpoint,
            "/v1/feature-conveyor/repository-preflight",
            Some(token),
            &serde_json::to_string(&forbidden).unwrap(),
        );
        assert!(
            response.starts_with("HTTP/1.1 422 Unprocessable Entity"),
            "accepted {forbidden_path}: {response}"
        );
        assert!(!response.contains(forbidden_path));
    }
    let oversized = post_bytes_request(
        endpoint,
        "/v1/feature-conveyor/repository-preflight",
        Some(token),
        &vec![b'x'; MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES + 1],
    );
    assert!(
        oversized.starts_with("HTTP/1.1 422 Unprocessable Entity"),
        "{oversized}"
    );
    assert_eq!(
        response_json(&oversized),
        serde_json::json!({"error":"repository_preflight_request_rejected"})
    );

    let eligible = post_request(
        endpoint,
        "/v1/feature-conveyor/repository-preflight",
        Some(token),
        &request_json,
    );
    assert!(eligible.starts_with("HTTP/1.1 200 OK"), "{eligible}");
    let receipt = response_json(&eligible);
    assert_exact_object_keys(
        &receipt,
        &[
            "schema_version",
            "repository_id",
            "registration_grant_revision",
            "scope_sha256",
            "emergency_pause_revision",
            "base_branch",
            "head_commit",
            "preflight_fingerprint_sha256",
            "observed_at_ms",
            "status",
        ],
    );
    assert_eq!(receipt["repository_id"], repository_id.to_string());
    assert_eq!(receipt["base_branch"], "main");
    assert_eq!(receipt["head_commit"], head);
    assert!(receipt.get("clean").is_none());
    assert_eq!(receipt["status"], "identity_eligible");
    assert!(!eligible.contains(&request.scope.repository_path));

    std::fs::write(repository.path().join("README.md"), "dirty fixture\n").unwrap();
    assert_preflight_identity_eligible(endpoint, token, &request);
    git(repository.path(), &["checkout", "--", "README.md"]);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let marker = repository.path().join("hostile-filter-executed");
        let hostile_filter = repository.path().join("hostile-filter.sh");
        std::fs::write(
            &hostile_filter,
            format!(
                "#!/bin/sh\nprintf executed > '{}'\n(sleep 30) &\n",
                marker.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&hostile_filter).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&hostile_filter, permissions).unwrap();
        git(
            repository.path(),
            &[
                "config",
                "filter.hostile.process",
                hostile_filter.to_str().unwrap(),
            ],
        );
        git(
            repository.path(),
            &["config", "filter.hostile.required", "true"],
        );
        std::fs::write(
            repository.path().join(".gitattributes"),
            "* filter=hostile\n",
        )
        .unwrap();
        assert_preflight_identity_eligible(endpoint, token, &request);
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!marker.exists(), "repository filter executable was invoked");
        std::fs::remove_file(repository.path().join(".gitattributes")).unwrap();
        std::fs::remove_file(hostile_filter).unwrap();
        git(
            repository.path(),
            &["config", "--unset", "filter.hostile.process"],
        );
        git(
            repository.path(),
            &["config", "--unset", "filter.hostile.required"],
        );
    }

    git(repository.path(), &["checkout", "-b", "other"]);
    assert_preflight_rejected(endpoint, token, &request);
    git(repository.path(), &["checkout", "main"]);
    std::fs::write(repository.path().join("README.md"), "new clean head\n").unwrap();
    git(repository.path(), &["add", "README.md"]);
    git(repository.path(), &["commit", "-m", "new head"]);
    assert_preflight_rejected(endpoint, token, &request);
    git(repository.path(), &["reset", "--hard", &head]);
    git(repository.path(), &["checkout", "--detach", &head]);
    assert_preflight_rejected(endpoint, token, &request);
    git(repository.path(), &["checkout", "main"]);
    std::fs::create_dir(repository.path().join(".git").join("worktrees")).unwrap();
    assert_preflight_rejected(endpoint, token, &request);
    std::fs::remove_dir(repository.path().join(".git").join("worktrees")).unwrap();
    std::fs::create_dir(repository.path().join(".git").join("modules")).unwrap();
    assert_preflight_rejected(endpoint, token, &request);
    std::fs::remove_dir(repository.path().join(".git").join("modules")).unwrap();
    std::fs::write(
        repository.path().join(".git").join("config.worktree"),
        "[core]\n\tworktree = forbidden\n",
    )
    .unwrap();
    assert_preflight_rejected(endpoint, token, &request);
    std::fs::remove_file(repository.path().join(".git").join("config.worktree")).unwrap();
    std::fs::write(
        repository.path().join(".gitmodules"),
        "forbidden submodule\n",
    )
    .unwrap();
    assert_preflight_rejected(endpoint, token, &request);
    std::fs::remove_file(repository.path().join(".gitmodules")).unwrap();

    #[cfg(unix)]
    {
        let symlink_parent = tempdir().unwrap();
        let symlink = symlink_parent.path().join("repository-link");
        std::os::unix::fs::symlink(repository.path(), &symlink).unwrap();
        request.scope.repository_path = symlink.to_string_lossy().into_owned();
        request.scope_sha256 = request.scope.canonical_scope_sha256().unwrap();
        request.registration_grant_revision = 2;
        record_preflight_registration_grant(endpoint, token, &request, 1);
        assert_preflight_rejected(endpoint, token, &request);
    }

    let non_git = tempdir().unwrap();
    request.scope.repository_path = std::fs::canonicalize(non_git.path())
        .unwrap()
        .to_string_lossy()
        .into_owned();
    request.scope_sha256 = request.scope.canonical_scope_sha256().unwrap();
    request.registration_grant_revision += 1;
    record_preflight_registration_grant(
        endpoint,
        token,
        &request,
        request.registration_grant_revision - 1,
    );
    assert_preflight_rejected(endpoint, token, &request);

    let audit: String = Connection::open(directory.path().join("master.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT redacted_metadata_json FROM feature_conveyor_audit
             WHERE event_kind = 'repository_identity_preflight_eligible'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    for forbidden in [
        repository_id.to_string(),
        repository_path.to_string_lossy().into_owned(),
        "main".to_string(),
        head,
        "error".to_string(),
    ] {
        assert!(
            !audit.contains(&forbidden),
            "audit leaked {forbidden}: {audit}"
        );
    }
    assert!(audit.contains("\"identity_only\":true"));
    assert!(!audit.contains("clean"));
}

#[test]
fn feature_conveyor_status_is_owner_authenticated_bounded_and_redacted() {
    let directory = tempdir().expect("temporary feature status directory");
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    assert_success(&run(binary, directory.path(), ["setup"]), "setup");

    let empty_endpoint = unused_loopback_addr();
    let mut empty_server = spawn_server(binary, directory.path(), empty_endpoint);
    read_ready(&mut empty_server.child);
    let token = std::fs::read_to_string(directory.path().join("development.token"))
        .expect("read development bearer");
    let token = token.trim();
    let unauthorized = get_request(empty_endpoint, "/v1/feature-conveyor/status", None);
    assert!(
        unauthorized.starts_with("HTTP/1.1 401 Unauthorized"),
        "unauthenticated feature status was reachable: {unauthorized}"
    );
    let empty = get_request(empty_endpoint, "/v1/feature-conveyor/status", Some(token));
    assert!(empty.starts_with("HTTP/1.1 200 OK"), "{empty}");
    let empty_json = response_json(&empty);
    assert_eq!(empty_json["schema_version"], 8);
    assert_eq!(empty_json["queue_revision"], 0);
    assert_eq!(empty_json["startup_quarantine_count"], 0);
    assert_eq!(empty_json["visible_feature_count"], 0);
    assert_eq!(empty_json["features_truncated"], false);
    assert_eq!(empty_json["features"], serde_json::json!([]));
    assert_eq!(empty_json["counts_by_status"]["queued"], 0);
    assert_eq!(empty_json["counts_by_status"]["quarantined"], 0);
    assert_eq!(empty_json["owner_guidance"]["state"], "idle");
    assert_eq!(empty_json["owner_guidance"]["reason_code"], "queue_empty");
    assert_eq!(
        empty_json["owner_guidance"]["next_owner_action"],
        "prepare_approved_feature"
    );
    assert_eq!(empty_json["owner_guidance"]["feature_id"], Value::Null);
    assert_eq!(empty_json["owner_guidance"]["queue_revision"], 0);
    assert_eq!(empty_json["owner_guidance"]["emergency_pause_revision"], 0);
    assert_status_json_allowlist(&empty_json);

    empty_server.child.kill().expect("stop empty master");
    empty_server.child.wait().expect("reap empty master");
    seed_bounded_feature_status(directory.path());

    let populated_endpoint = unused_loopback_addr();
    let mut populated_server = spawn_server(binary, directory.path(), populated_endpoint);
    read_ready(&mut populated_server.child);
    let populated = get_request(
        populated_endpoint,
        "/v1/feature-conveyor/status",
        Some(token),
    );
    assert!(populated.starts_with("HTTP/1.1 200 OK"), "{populated}");
    let populated_json = response_json(&populated);
    assert_eq!(
        populated_json["visible_feature_count"],
        MAX_CONVEYOR_NONTERMINAL_FEATURES + 1
    );
    assert_eq!(populated_json["features_truncated"], true);
    assert_eq!(
        populated_json["features"]
            .as_array()
            .expect("bounded feature metadata")
            .len(),
        MAX_CONVEYOR_NONTERMINAL_FEATURES as usize
    );
    assert_eq!(
        populated_json["counts_by_status"]["queued"],
        MAX_CONVEYOR_NONTERMINAL_FEATURES
    );
    assert_eq!(populated_json["counts_by_status"]["cancelled"], 1);
    assert_eq!(populated_json["features"][0]["status"], "cancelled");
    assert_eq!(populated_json["features"][0]["lease_present"], true);
    assert_eq!(populated_json["features"][0]["effect_possible"], true);
    assert_eq!(populated_json["owner_guidance"]["state"], "blocked");
    assert_eq!(
        populated_json["owner_guidance"]["reason_code"],
        "active_requires_reconciliation"
    );
    assert_eq!(
        populated_json["owner_guidance"]["next_owner_action"],
        "reconcile_active_feature"
    );
    assert_eq!(
        populated_json["owner_guidance"]["feature_id"],
        populated_json["features"][0]["feature_id"]
    );
    assert_eq!(
        populated_json["owner_guidance"]["queue_revision"],
        populated_json["queue_revision"]
    );
    assert_eq!(
        populated_json["owner_guidance"]["emergency_pause_revision"],
        0
    );
    assert_status_json_allowlist(&populated_json);
}

#[test]
fn windows_master_process_owns_state_and_completes_cross_process_fixture() {
    let directory = tempdir().expect("temporary master process directory");
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");

    let setup = run(binary, directory.path(), ["setup"]);
    assert_success(&setup, "setup");
    let setup_receipt: Value = serde_json::from_slice(&setup.stdout).expect("setup JSON receipt");
    assert_eq!(setup_receipt["status"], "setup_complete");
    assert_eq!(setup_receipt["protocol_version"], 2);
    assert_eq!(setup_receipt["schema_version"], 8);
    assert!(directory.path().join("master.sqlite3").is_file());
    assert!(directory.path().join("development.token").is_file());
    let development_token = std::fs::read_to_string(directory.path().join("development.token"))
        .expect("read generated development token");
    let development_token = development_token.trim();
    assert!(!development_token.is_empty());
    assert!(
        !String::from_utf8_lossy(&setup.stdout).contains(development_token),
        "setup receipt exposed the development bearer"
    );

    let endpoint = unused_loopback_addr();
    let mut server = spawn_server(binary, directory.path(), endpoint);
    let ready = read_ready(&mut server.child);
    assert_eq!(ready["status"], "ready");
    assert_eq!(ready["endpoint"], endpoint.to_string());
    assert_unauthorized_without_bearer(endpoint);
    let unauthorized_events = post_request(
        endpoint,
        "/v1/development/events/next",
        None,
        r#"{"protocol_version":2,"connection_epoch":1,"after":null,"limit":64}"#,
    );
    assert!(
        unauthorized_events.starts_with("HTTP/1.1 401 Unauthorized"),
        "unauthenticated local event metadata was reachable: {unauthorized_events}"
    );
    assert_oversized_body_is_rejected(endpoint, development_token);
    let unauthorized_pause = post_request(
        endpoint,
        "/v1/development/emergency-pause/activate",
        None,
        "{}",
    );
    assert!(
        unauthorized_pause.starts_with("HTTP/1.1 401 Unauthorized"),
        "unauthenticated owner pause control was reachable: {unauthorized_pause}"
    );

    let health = run(
        binary,
        directory.path(),
        ["health", "--endpoint", &endpoint.to_string()],
    );
    assert_success(&health, "initial health");
    let health_json: Value = serde_json::from_slice(&health.stdout).expect("health JSON");
    assert_eq!(health_json["status"], "ok");
    assert_eq!(health_json["emergency_paused"], false);
    assert_eq!(health_json["state"]["terminal_steps"], 0);

    let mixed_plan_action = post_request(
        endpoint,
        "/v1/development/emergency-pause/activate",
        Some(development_token),
        r#"{"step":{"capability_id":"fixture.reasoning"}}"#,
    );
    assert!(
        mixed_plan_action.starts_with("HTTP/1.1 422 Unprocessable Entity"),
        "pause action accepted a mixed planning payload: {mixed_plan_action}"
    );
    let still_unpaused = run(
        binary,
        directory.path(),
        ["health", "--endpoint", &endpoint.to_string()],
    );
    assert_success(&still_unpaused, "health after rejected mixed action");
    let still_unpaused_json: Value =
        serde_json::from_slice(&still_unpaused.stdout).expect("unpaused health JSON");
    assert_eq!(still_unpaused_json["emergency_paused"], false);

    let pause = post_request(
        endpoint,
        "/v1/development/emergency-pause/activate",
        Some(development_token),
        "{}",
    );
    assert!(pause.starts_with("HTTP/1.1 200 OK"), "{pause}");
    assert_eq!(response_json(&pause)["emergency_paused"], true);
    let paused_health = run(
        binary,
        directory.path(),
        ["health", "--endpoint", &endpoint.to_string()],
    );
    assert_success(&paused_health, "paused health");
    let paused_health_json: Value =
        serde_json::from_slice(&paused_health.stdout).expect("paused health JSON");
    assert_eq!(paused_health_json["status"], "paused");
    assert_eq!(paused_health_json["emergency_paused"], true);
    let blocked_work = post_request(
        endpoint,
        "/v1/development/leases/next",
        Some(development_token),
        r#"{"device_id":"11111111-1111-4111-8111-111111111111","connection_epoch":1}"#,
    );
    assert!(
        blocked_work.starts_with("HTTP/1.1 503 Service Unavailable")
            && blocked_work.contains("emergency_pause_blocks_work"),
        "pause did not dominate live process work admission: {blocked_work}"
    );
    let resume = post_request(
        endpoint,
        "/v1/development/emergency-pause/resume",
        Some(development_token),
        "{}",
    );
    assert!(resume.starts_with("HTTP/1.1 200 OK"), "{resume}");
    assert_eq!(response_json(&resume)["emergency_paused"], false);
    let resumed_health = run(
        binary,
        directory.path(),
        ["health", "--endpoint", &endpoint.to_string()],
    );
    assert_success(&resumed_health, "resumed health");
    let resumed_health_json: Value =
        serde_json::from_slice(&resumed_health.stdout).expect("resumed health JSON");
    assert_eq!(resumed_health_json["status"], "ok");
    assert_eq!(resumed_health_json["emergency_paused"], false);

    let second_endpoint = unused_loopback_addr();
    let second_owner = run(
        binary,
        directory.path(),
        ["serve", "--bind", &second_endpoint.to_string()],
    );
    assert!(
        !second_owner.status.success(),
        "second owner unexpectedly started"
    );
    assert!(
        String::from_utf8_lossy(&second_owner.stderr).contains("already owns"),
        "unexpected second-owner error: {}",
        String::from_utf8_lossy(&second_owner.stderr)
    );

    let fixture = run(
        binary,
        directory.path(),
        [
            "fixture-worker",
            "--endpoint",
            &endpoint.to_string(),
            "--prompt",
            "complete the child-process fixture",
        ],
    );
    assert_success(&fixture, "fixture worker");
    let fixture_json: Value = serde_json::from_slice(&fixture.stdout).expect("fixture JSON");
    assert_eq!(fixture_json["status"], "fixture_complete");
    assert_eq!(fixture_json["accepted_result"]["status"], "succeeded");
    let fixture_task_id = fixture_json["task_id"]
        .as_str()
        .expect("fixture receipt task identifier");
    let fixture_step_id = fixture_json["step_id"]
        .as_str()
        .expect("fixture receipt step identifier");
    let event_response = post_request(
        endpoint,
        "/v1/development/events/next",
        Some(development_token),
        r#"{"protocol_version":2,"connection_epoch":1,"after":null,"limit":64}"#,
    );
    assert!(
        event_response.starts_with("HTTP/1.1 200 OK"),
        "authenticated local event query failed: {event_response}"
    );
    let event_batch = response_json(&event_response);
    let events = event_batch["events"]
        .as_array()
        .expect("local event metadata array");
    let fixture_kinds = events
        .iter()
        .filter(|event| event["task_id"] == fixture_task_id && event["step_id"] == fixture_step_id)
        .map(|event| event["kind"].as_str().expect("event kind"))
        .collect::<Vec<_>>();
    assert_eq!(
        fixture_kinds,
        ["step_queued", "step_leased", "step_succeeded"],
        "local event metadata did not bind the exact fixture lifecycle"
    );
    let event_body = event_response
        .split_once("\r\n\r\n")
        .expect("event response body delimiter")
        .1;
    for forbidden in ["context", "payload", "result", "prompt", "input", "output"] {
        assert!(
            !event_body.contains(forbidden),
            "local event metadata exposed forbidden field: {forbidden}"
        );
    }

    let completed_health = run(
        binary,
        directory.path(),
        ["health", "--endpoint", &endpoint.to_string()],
    );
    assert_success(&completed_health, "completed health");
    let completed_json: Value =
        serde_json::from_slice(&completed_health.stdout).expect("completed health JSON");
    assert_eq!(completed_json["state"]["registered_devices"], 1);
    assert_eq!(completed_json["state"]["active_connections"], 1);
    assert_eq!(completed_json["state"]["terminal_steps"], 1);
    assert_eq!(completed_json["state"]["active_attempts"], 0);

    server.child.kill().expect("stop first master process");
    server.child.wait().expect("reap first master process");

    let restart_endpoint = unused_loopback_addr();
    let mut restarted = spawn_server(binary, directory.path(), restart_endpoint);
    let restarted_ready = read_ready(&mut restarted.child);
    assert_eq!(restarted_ready["status"], "ready");

    let restarted_health = run(
        binary,
        directory.path(),
        ["health", "--endpoint", &restart_endpoint.to_string()],
    );
    assert_success(&restarted_health, "restarted health");
    let restarted_json: Value =
        serde_json::from_slice(&restarted_health.stdout).expect("restarted health JSON");
    assert_eq!(
        restarted_json["startup_reconciliation"]["disconnected_connections"],
        1
    );
    assert_eq!(restarted_json["state"]["active_connections"], 0);
    assert_eq!(restarted_json["state"]["terminal_steps"], 1);
}

fn run<const N: usize>(binary: &str, data_dir: &Path, arguments: [&str; N]) -> Output {
    Command::new(binary)
        .arg("--data-dir")
        .arg(data_dir)
        .args(arguments)
        .output()
        .expect("run assemblywright-master command")
}

fn spawn_server(binary: &str, data_dir: &Path, endpoint: SocketAddr) -> ChildGuard {
    let child = Command::new(binary)
        .arg("--data-dir")
        .arg(data_dir)
        .arg("serve")
        .arg("--bind")
        .arg(endpoint.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn assemblywright-master serve");
    ChildGuard { child }
}

fn read_ready(child: &mut Child) -> Value {
    let stdout = child.stdout.take().expect("master stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read ready receipt");
    assert!(!line.is_empty(), "master exited without a ready receipt");
    serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("invalid ready receipt {line:?}: {error}"))
}

fn unused_loopback_addr() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .expect("reserve loopback address")
        .local_addr()
        .expect("read loopback address")
}

fn assert_unauthorized_without_bearer(endpoint: SocketAddr) {
    let mut stream = TcpStream::connect(endpoint).expect("connect without bearer");
    write!(
        stream,
        "GET /health HTTP/1.1\r\nHost: {endpoint}\r\nConnection: close\r\n\r\n"
    )
    .expect("write unauthenticated health request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read unauthenticated health response");
    assert!(
        response.starts_with("HTTP/1.1 401 Unauthorized"),
        "unexpected unauthenticated response: {response}"
    );
}

fn assert_oversized_body_is_rejected(endpoint: SocketAddr, token: &str) {
    let body = vec![b'x'; MAX_WIRE_FRAME_BYTES + 1];
    let mut stream = TcpStream::connect(endpoint).expect("connect for oversized request");
    write!(
        stream,
        "POST /v1/development/steps HTTP/1.1\r\nHost: {endpoint}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .expect("write oversized request headers");
    stream
        .write_all(&body)
        .expect("write oversized request body");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read oversized response");
    assert!(
        response.starts_with("HTTP/1.1 413 Payload Too Large"),
        "unexpected oversized response status"
    );
}

fn post_request(endpoint: SocketAddr, path: &str, token: Option<&str>, body: &str) -> String {
    let mut stream = TcpStream::connect(endpoint).expect("connect owner action");
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {endpoint}\r\n{authorization}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("write owner action request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read owner action response");
    response
}

fn post_bytes_request(
    endpoint: SocketAddr,
    path: &str,
    token: Option<&str>,
    body: &[u8],
) -> String {
    let mut stream = TcpStream::connect(endpoint).expect("connect binary owner action");
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {endpoint}\r\n{authorization}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .expect("write binary owner action request headers");
    stream
        .write_all(body)
        .expect("write binary owner action request body");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read binary owner action response");
    response
}

fn record_preflight_registration_grant(
    endpoint: SocketAddr,
    token: &str,
    request: &FeatureConveyorRepositoryPreflightRequest,
    expected_current_revision: u64,
) {
    let grant = FeatureConveyorRepositoryGrantRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        expected_current_revision,
        expected_emergency_pause_revision: request.expected_emergency_pause_revision,
        grant: FeatureConveyorRepositoryGrantRevision {
            repository_id: request.scope.repository_id,
            kind: FeatureConveyorRepositoryGrantKind::Registration,
            revision: request.registration_grant_revision,
            scope_sha256: request.scope_sha256,
            owner_approval_sha256: Sha256::digest(format!(
                "owner-approved-preflight-revision-{}",
                request.registration_grant_revision
            ))
            .into(),
            expires_at_ms: None,
            revoked: false,
        },
    };
    let response = post_request(
        endpoint,
        "/v1/feature-conveyor/repository-grants",
        Some(token),
        &serde_json::to_string(&grant).unwrap(),
    );
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
}

fn assert_preflight_rejected(
    endpoint: SocketAddr,
    token: &str,
    request: &FeatureConveyorRepositoryPreflightRequest,
) {
    let response = post_request(
        endpoint,
        "/v1/feature-conveyor/repository-preflight",
        Some(token),
        &serde_json::to_string(request).unwrap(),
    );
    assert!(response.starts_with("HTTP/1.1 409 Conflict"), "{response}");
    assert_eq!(
        response_json(&response),
        serde_json::json!({"error":"repository_preflight_rejected"})
    );
    assert!(!response.contains(&request.scope.repository_path));
}

fn assert_preflight_identity_eligible(
    endpoint: SocketAddr,
    token: &str,
    request: &FeatureConveyorRepositoryPreflightRequest,
) {
    let response = post_request(
        endpoint,
        "/v1/feature-conveyor/repository-preflight",
        Some(token),
        &serde_json::to_string(request).unwrap(),
    );
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    let receipt = response_json(&response);
    assert_eq!(receipt["status"], "identity_eligible");
    assert!(receipt.get("clean").is_none());
}

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .expect("run disposable Git fixture command");
    assert!(
        output.status.success(),
        "Git fixture command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .expect("run disposable Git fixture observation");
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end_matches(['\r', '\n'])
        .to_string()
}

fn get_request(endpoint: SocketAddr, path: &str, token: Option<&str>) -> String {
    let mut stream = TcpStream::connect(endpoint).expect("connect owner observation");
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {endpoint}\r\n{authorization}Connection: close\r\n\r\n"
    )
    .expect("write owner observation request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read owner observation response");
    response
}

fn seed_bounded_feature_status(data_dir: &Path) {
    let mut process = MasterProcess::acquire(data_dir).expect("acquire feature status seed");
    let repository_id = Uuid::new_v4();
    for (index, kind) in [
        RepositoryGrantKind::Registration,
        RepositoryGrantKind::CloudDisclosure,
        RepositoryGrantKind::AutonomousPublication,
    ]
    .into_iter()
    .enumerate()
    {
        process
            .kernel_mut()
            .record_repository_grant_revision(
                &RepositoryGrantRevision {
                    repository_id,
                    kind,
                    revision: 1,
                    scope_sha256: Sha256::digest(format!("scope-{index}")).into(),
                    owner_approval_sha256: Sha256::digest(format!("approval-{index}")).into(),
                    expires_at_ms: None,
                    revoked: false,
                },
                0,
                0,
                1,
            )
            .expect("record status seed grant");
    }
    let features = (0..=MAX_CONVEYOR_NONTERMINAL_FEATURES)
        .map(|index| {
            let feature_id = Uuid::new_v4();
            let manifest = serde_json::json!({"feature_id": feature_id, "index": index});
            let canonical = format!(r#"{{"feature_id":"{feature_id}","index":{index}}}"#);
            ApprovedFeatureSpecification {
                feature_id,
                revision: 1,
                repository_id,
                manifest,
                manifest_sha256: Sha256::digest(canonical).into(),
                design_sha256: Sha256::digest(format!("design-{index}")).into(),
                brainstorming_sha256: Sha256::digest(format!("brainstorming-{index}")).into(),
                owner_approval_sha256: Sha256::digest(format!("owner-{index}")).into(),
                grants: FeatureGrantRevisions {
                    registration: 1,
                    cloud_disclosure: 1,
                    autonomous_publication: 1,
                },
                provider_id: "local.review".to_string(),
                model_id: "review-v1".to_string(),
                dependencies: vec![],
            }
        })
        .collect::<Vec<_>>();
    for (index, feature) in features
        .iter()
        .take(MAX_CONVEYOR_NONTERMINAL_FEATURES as usize)
        .enumerate()
    {
        process
            .kernel_mut()
            .enqueue_approved_feature(feature, index as u64, 10 + index as u64)
            .expect("enqueue status seed");
    }
    let claim = process
        .kernel_mut()
        .claim_next_feature(MAX_CONVEYOR_NONTERMINAL_FEATURES, 200)
        .expect("claim status blocker");
    process
        .kernel_mut()
        .cancel_active_feature(claim.feature_id, claim.lifecycle_revision, 201)
        .expect("cancel status blocker");
    process
        .kernel_mut()
        .enqueue_approved_feature(
            features.last().expect("overflow status feature"),
            MAX_CONVEYOR_NONTERMINAL_FEATURES + 1,
            202,
        )
        .expect("enqueue bounded overflow feature");
}

fn response_json(response: &str) -> Value {
    let (_, body) = response
        .split_once("\r\n\r\n")
        .expect("HTTP response body delimiter");
    serde_json::from_str(body).expect("decode response JSON")
}

fn assert_exact_object_keys(value: &Value, expected: &[&str]) {
    let object = value.as_object().expect("JSON object");
    let mut actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

fn assert_status_json_allowlist(value: &Value) {
    assert_exact_object_keys(
        value,
        &[
            "schema_version",
            "queue_revision",
            "startup_quarantine_count",
            "counts_by_status",
            "visible_feature_count",
            "features_truncated",
            "features",
            "owner_guidance",
        ],
    );
    assert_exact_object_keys(
        &value["counts_by_status"],
        &[
            "queued",
            "implementing",
            "validating",
            "reviewing",
            "publishing",
            "verifying_main",
            "succeeded",
            "cancelled",
            "abandoned",
            "quarantined",
        ],
    );
    for feature in value["features"].as_array().expect("status feature array") {
        assert_exact_object_keys(
            feature,
            &[
                "feature_id",
                "specification_revision",
                "lifecycle_revision",
                "queue_position",
                "status",
                "lease_present",
                "effect_possible",
            ],
        );
    }
    assert_exact_object_keys(
        &value["owner_guidance"],
        &[
            "state",
            "reason_code",
            "next_owner_action",
            "feature_id",
            "specification_revision",
            "lifecycle_revision",
            "queue_revision",
            "emergency_pause_revision",
        ],
    );
}

fn assert_success(output: &Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
