use assemblywright_master::{MasterError, MasterKernel, MasterProcess, MASTER_SCHEMA_VERSION};
use assemblywright_protocol::{
    AssemblyLineAutoRunRequest, AssemblyLineRepositoryIdentity, BrainstormingAcceptanceCriterion,
    BrainstormingOwnerApprovalBinding, BrainstormingSpecificationDocument, BrainstormingTargetKind,
    CanonicalGitHubRepositoryUrl, FeatureBrainstormingDraft, FrozenBrainstormingSpecification,
    OrchestratorCatalog, ProjectBrainstormingDraft, ProjectVisibility, RepositoryCreationLifecycle,
    RuntimeAvailabilityStatus, FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
};
use rusqlite::{params, Connection};
use tempfile::TempDir;
use uuid::Uuid;

fn repository(url: &str) -> AssemblyLineRepositoryIdentity {
    AssemblyLineRepositoryIdentity {
        repository_id: Uuid::new_v4(),
        git_url: CanonicalGitHubRepositoryUrl::parse(url).unwrap(),
    }
}

fn specification(title: &str) -> BrainstormingSpecificationDocument {
    BrainstormingSpecificationDocument {
        title: title.to_string(),
        outcome: format!("Deliver {title} with native verification."),
        acceptance_criteria: vec![BrainstormingAcceptanceCriterion {
            id: "acceptance-1".to_string(),
            requirement: "The bounded owner workflow succeeds.".to_string(),
        }],
        obligations: vec!["Run focused native tests and update durable documentation.".to_string()],
    }
}

fn project_draft(repository: AssemblyLineRepositoryIdentity) -> ProjectBrainstormingDraft {
    ProjectBrainstormingDraft {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft_id: Uuid::new_v4(),
        draft_revision: 1,
        repository,
        visibility: ProjectVisibility::Public,
        orchestrator_catalog: OrchestratorCatalog::default(),
        orchestrator: Default::default(),
        idea: "Create a small project with an explicit safety boundary.".to_string(),
    }
}

fn frozen_project(draft: &ProjectBrainstormingDraft) -> FrozenBrainstormingSpecification {
    let document = specification("New project");
    FrozenBrainstormingSpecification {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        specification_id: Uuid::new_v4(),
        specification_revision: 1,
        target_kind: BrainstormingTargetKind::Project,
        draft_id: draft.draft_id,
        draft_revision: draft.draft_revision,
        draft_sha256: draft.canonical_sha256().unwrap(),
        repository: draft.repository.clone(),
        visibility: Some(draft.visibility),
        orchestrator_catalog_revision: draft.orchestrator_catalog.catalog_revision,
        orchestrator_catalog_sha256: draft.orchestrator_catalog.catalog_sha256,
        orchestrator_profile_sha256: draft.orchestrator.canonical_sha256().unwrap(),
        specification_sha256: document.canonical_sha256().unwrap(),
        specification: document,
    }
}

fn feature_draft(
    repository: AssemblyLineRepositoryIdentity,
    idea: &str,
) -> FeatureBrainstormingDraft {
    FeatureBrainstormingDraft {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft_id: Uuid::new_v4(),
        draft_revision: 1,
        repository,
        expected_repository_revision: 1,
        orchestrator_catalog: OrchestratorCatalog::default(),
        orchestrator: Default::default(),
        idea: idea.to_string(),
    }
}

fn frozen_feature(
    draft: &FeatureBrainstormingDraft,
    title: &str,
) -> FrozenBrainstormingSpecification {
    let document = specification(title);
    FrozenBrainstormingSpecification {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        specification_id: Uuid::new_v4(),
        specification_revision: 1,
        target_kind: BrainstormingTargetKind::Feature,
        draft_id: draft.draft_id,
        draft_revision: draft.draft_revision,
        draft_sha256: draft.canonical_sha256().unwrap(),
        repository: draft.repository.clone(),
        visibility: None,
        orchestrator_catalog_revision: draft.orchestrator_catalog.catalog_revision,
        orchestrator_catalog_sha256: draft.orchestrator_catalog.catalog_sha256,
        orchestrator_profile_sha256: draft.orchestrator.canonical_sha256().unwrap(),
        specification_sha256: document.canonical_sha256().unwrap(),
        specification: document,
    }
}

enum DraftBinding<'a> {
    Project(&'a ProjectBrainstormingDraft),
    Feature(&'a FeatureBrainstormingDraft),
}

fn approval(
    target_kind: BrainstormingTargetKind,
    repository: AssemblyLineRepositoryIdentity,
    draft: DraftBinding<'_>,
    frozen: &FrozenBrainstormingSpecification,
    owner_control_revision: u64,
    expected_queue_revision: Option<u64>,
) -> BrainstormingOwnerApprovalBinding {
    let (draft_id, draft_revision, draft_sha256) = match draft {
        DraftBinding::Project(draft) => (
            draft.draft_id,
            draft.draft_revision,
            draft.canonical_sha256().unwrap(),
        ),
        DraftBinding::Feature(draft) => (
            draft.draft_id,
            draft.draft_revision,
            draft.canonical_sha256().unwrap(),
        ),
    };
    let mut approval = BrainstormingOwnerApprovalBinding {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        approval_id: Uuid::new_v4(),
        approved_at_ms: 10_000 + owner_control_revision,
        owner_control_revision,
        target_kind,
        repository,
        visibility: frozen.visibility,
        expected_repository_revision: Some(if target_kind == BrainstormingTargetKind::Project {
            0
        } else {
            1
        }),
        expected_queue_revision,
        draft_id,
        draft_revision,
        draft_sha256,
        orchestrator_catalog_revision: frozen.orchestrator_catalog_revision,
        orchestrator_catalog_sha256: frozen.orchestrator_catalog_sha256,
        specification_id: frozen.specification_id,
        specification_revision: frozen.specification_revision,
        specification_sha256: frozen.specification_sha256,
        orchestrator_profile_sha256: frozen.orchestrator_profile_sha256,
        owner_approval_sha256: [0; 32],
    };
    approval.owner_approval_sha256 = approval.canonical_approval_sha256().unwrap();
    approval
}

fn mark_repository_created(path: &std::path::Path, repository_id: Uuid) {
    let connection = Connection::open(path).unwrap();
    connection
        .execute(
            "UPDATE assembly_line_repositories
             SET lifecycle='created',lifecycle_revision=2,effect_possible=1,
                 creation_evidence_sha256=?2
             WHERE repository_id=?1 AND lifecycle='creation_pending'",
            params![repository_id.to_string(), [7_u8; 32].as_slice()],
        )
        .unwrap();
}

fn drop_assembly_line_schema(connection: &Connection) {
    for trigger in [
        "assembly_line_project_drafts_no_update",
        "assembly_line_project_drafts_no_delete",
        "assembly_line_feature_drafts_no_update",
        "assembly_line_feature_drafts_no_delete",
        "assembly_line_frozen_specs_no_update",
        "assembly_line_frozen_specs_no_delete",
        "assembly_line_approvals_no_update",
        "assembly_line_approvals_no_delete",
        "assembly_line_requests_no_update",
        "assembly_line_requests_no_delete",
        "assembly_line_audit_no_update",
        "assembly_line_audit_no_delete",
    ] {
        connection
            .execute_batch(&format!("DROP TRIGGER {trigger};"))
            .unwrap();
    }
    for table in [
        "assembly_line_audit",
        "assembly_line_requests",
        "assembly_line_queue",
        "assembly_line_repositories",
        "assembly_line_owner_approvals",
        "assembly_line_frozen_specifications",
        "assembly_line_feature_drafts",
        "assembly_line_project_drafts",
        "assembly_line_state",
    ] {
        connection
            .execute_batch(&format!("DROP TABLE {table};"))
            .unwrap();
    }
}

#[test]
fn schema_v20_defaults_to_inert_stopped_auto_run_and_unavailable_components() {
    let kernel = MasterKernel::in_memory().unwrap();
    assert_eq!(kernel.schema_version().unwrap(), MASTER_SCHEMA_VERSION);
    let projection = kernel.assembly_line_owner_projection(1).unwrap();
    assert!(projection.assembly_line.auto_run);
    assert_eq!(projection.assembly_line.queue_count, 0);
    assert_eq!(
        format!("{:?}", projection.assembly_line.lifecycle),
        "Stopped"
    );
    for component in [
        projection.availability.brainstorming_provider,
        projection.availability.github_creation,
        projection.availability.windows_executor,
        projection.availability.mac_executor,
        projection.availability.protected_brokers,
    ] {
        assert_eq!(component.status, RuntimeAvailabilityStatus::Unavailable);
    }
}

#[test]
fn project_approval_is_atomic_public_by_default_replay_safe_and_effect_free() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let draft = project_draft(repository("https://github.com/Owner/New-Repo.git"));
    kernel
        .record_assembly_line_project_draft(&draft, 100)
        .unwrap();
    kernel
        .record_assembly_line_project_draft(&draft, 101)
        .unwrap();
    let frozen = frozen_project(&draft);
    kernel
        .record_assembly_line_frozen_specification(&frozen, 102)
        .unwrap();
    let approval = approval(
        BrainstormingTargetKind::Project,
        draft.repository.clone(),
        DraftBinding::Project(&draft),
        &frozen,
        3,
        None,
    );
    let first = kernel
        .approve_assembly_line_project(&approval, 103)
        .unwrap();
    let replay = kernel
        .approve_assembly_line_project(&approval, 104)
        .unwrap();
    assert_eq!(first, replay);
    assert_eq!(first.visibility, ProjectVisibility::Public);
    assert_eq!(
        first.lifecycle,
        RepositoryCreationLifecycle::CreationPending
    );
    assert!(!first.effect_possible);
    assert!(first.creation_evidence_sha256.is_none());
    let projection = kernel.assembly_line_owner_projection(105).unwrap();
    assert_eq!(projection.repositories, vec![first]);
    assert!(projection.queue.is_empty());

    let mut drift = approval.clone();
    drift.visibility = Some(ProjectVisibility::Private);
    drift.owner_approval_sha256 = drift.canonical_approval_sha256().unwrap();
    assert!(matches!(
        kernel.approve_assembly_line_project(&drift, 106),
        Err(MasterError::AssemblyLinePlanningImmutable)
    ));
}

#[test]
fn caller_catalog_cannot_authorize_itself_and_creation_pending_cannot_queue() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let mut forged = project_draft(repository("https://github.com/owner/catalog-test"));
    forged.orchestrator_catalog.profiles[0].provider_id = "caller.provider".to_string();
    forged.orchestrator_catalog.profiles[0].model_id = "caller-model".to_string();
    forged.orchestrator_catalog.default_profile_sha256 = forged.orchestrator_catalog.profiles[0]
        .canonical_sha256()
        .unwrap();
    forged.orchestrator_catalog.catalog_sha256 = forged
        .orchestrator_catalog
        .canonical_catalog_sha256()
        .unwrap();
    forged.orchestrator = forged.orchestrator_catalog.profiles[0].clone();
    assert!(matches!(
        kernel.record_assembly_line_project_draft(&forged, 100),
        Err(MasterError::Protocol(_))
    ));

    let draft = project_draft(repository("https://github.com/owner/pending-test"));
    kernel
        .record_assembly_line_project_draft(&draft, 110)
        .unwrap();
    let frozen = frozen_project(&draft);
    kernel
        .record_assembly_line_frozen_specification(&frozen, 111)
        .unwrap();
    let project_approval = approval(
        BrainstormingTargetKind::Project,
        draft.repository.clone(),
        DraftBinding::Project(&draft),
        &frozen,
        3,
        None,
    );
    kernel
        .approve_assembly_line_project(&project_approval, 112)
        .unwrap();
    let feature = feature_draft(draft.repository.clone(), "Add a bounded feature.");
    assert!(matches!(
        kernel.record_assembly_line_feature_draft(&feature, 113),
        Err(MasterError::AssemblyLineRepositoryUnavailable)
    ));
    assert!(kernel
        .assembly_line_owner_projection(114)
        .unwrap()
        .queue
        .is_empty());
}

#[test]
fn created_repository_feature_approvals_are_fifo_cas_bound_and_never_dispatch() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let project = project_draft(repository("https://github.com/owner/fifo-test"));
    kernel
        .record_assembly_line_project_draft(&project, 100)
        .unwrap();
    let frozen = frozen_project(&project);
    kernel
        .record_assembly_line_frozen_specification(&frozen, 101)
        .unwrap();
    let project_approval = approval(
        BrainstormingTargetKind::Project,
        project.repository.clone(),
        DraftBinding::Project(&project),
        &frozen,
        3,
        None,
    );
    kernel
        .approve_assembly_line_project(&project_approval, 102)
        .unwrap();
    mark_repository_created(&database, project.repository.repository_id);

    for (index, title) in ["First feature", "Second feature"].into_iter().enumerate() {
        let draft = feature_draft(project.repository.clone(), &format!("Plan {title}."));
        kernel
            .record_assembly_line_feature_draft(&draft, 200 + index as u64 * 10)
            .unwrap();
        let frozen = frozen_feature(&draft, title);
        kernel
            .record_assembly_line_frozen_specification(&frozen, 201 + index as u64 * 10)
            .unwrap();
        let projection = kernel
            .assembly_line_owner_projection(202 + index as u64 * 10)
            .unwrap();
        let feature_approval = approval(
            BrainstormingTargetKind::Feature,
            draft.repository.clone(),
            DraftBinding::Feature(&draft),
            &frozen,
            projection.owner_control_revision,
            Some(index as u64),
        );
        let entry = kernel
            .approve_assembly_line_feature_and_enqueue(&feature_approval, 203 + index as u64 * 10)
            .unwrap();
        assert_eq!(entry.position, index as u16 + 1);
        assert_eq!(
            kernel
                .approve_assembly_line_feature_and_enqueue(
                    &feature_approval,
                    204 + index as u64 * 10
                )
                .unwrap(),
            entry
        );
    }
    let projection = kernel.assembly_line_owner_projection(300).unwrap();
    assert_eq!(
        projection
            .queue
            .iter()
            .map(|entry| entry.position)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(projection.assembly_line.queue_count, 2);
    assert_eq!(projection.assembly_line.queue_revision, 2);
    let connection = Connection::open(&database).unwrap();
    let external_rows: i64 = connection
        .query_row("SELECT COUNT(*) FROM master_steps", [], |row| row.get(0))
        .unwrap();
    let legacy_queue: i64 = connection
        .query_row("SELECT COUNT(*) FROM feature_conveyor_queue", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!((external_rows, legacy_queue), (0, 0));
}

#[test]
fn auto_run_toggle_is_cas_bound_and_exact_replay_does_not_mutate_twice() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let prior = kernel
        .assembly_line_owner_projection(1)
        .unwrap()
        .assembly_line;
    let request = AssemblyLineAutoRunRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        expected_state_revision: prior.state_revision,
        auto_run: false,
    };
    let first = kernel.set_assembly_line_auto_run(&request, 2).unwrap();
    let replay = kernel.set_assembly_line_auto_run(&request, 3).unwrap();
    assert_eq!(first, replay);
    assert!(!first.resulting_state.auto_run);
    assert_eq!(
        first.resulting_state.state_revision,
        prior.state_revision + 1
    );

    let stale = AssemblyLineAutoRunRequest {
        request_id: Uuid::new_v4(),
        ..request
    };
    assert!(matches!(
        kernel.set_assembly_line_auto_run(&stale, 4),
        Err(MasterError::StaleAssemblyLineStateRevision { .. })
    ));
}

#[test]
fn feature_enqueue_revision_overflow_rolls_back_every_durable_record() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let project = project_draft(repository("https://github.com/owner/overflow-test"));
    kernel
        .record_assembly_line_project_draft(&project, 100)
        .unwrap();
    let frozen_project = frozen_project(&project);
    kernel
        .record_assembly_line_frozen_specification(&frozen_project, 101)
        .unwrap();
    let project_approval = approval(
        BrainstormingTargetKind::Project,
        project.repository.clone(),
        DraftBinding::Project(&project),
        &frozen_project,
        3,
        None,
    );
    kernel
        .approve_assembly_line_project(&project_approval, 102)
        .unwrap();
    mark_repository_created(&database, project.repository.repository_id);

    let feature = feature_draft(project.repository.clone(), "Exercise overflow rollback.");
    kernel
        .record_assembly_line_feature_draft(&feature, 110)
        .unwrap();
    let frozen_feature = frozen_feature(&feature, "Overflow rollback");
    kernel
        .record_assembly_line_frozen_specification(&frozen_feature, 111)
        .unwrap();
    let projection = kernel.assembly_line_owner_projection(112).unwrap();
    let mut feature_approval = approval(
        BrainstormingTargetKind::Feature,
        feature.repository.clone(),
        DraftBinding::Feature(&feature),
        &frozen_feature,
        projection.owner_control_revision,
        Some(projection.assembly_line.queue_revision),
    );
    let connection = Connection::open(&database).unwrap();
    connection
        .execute(
            "UPDATE assembly_line_state
             SET owner_control_revision=?1,state_revision=?1,queue_revision=?1
             WHERE singleton=1",
            [i64::MAX],
        )
        .unwrap();
    feature_approval.owner_control_revision = i64::MAX as u64;
    feature_approval.expected_queue_revision = Some(i64::MAX as u64);
    feature_approval.owner_approval_sha256 = feature_approval.canonical_approval_sha256().unwrap();
    let before_counts: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM assembly_line_owner_approvals),
               (SELECT COUNT(*) FROM assembly_line_queue),
               (SELECT COUNT(*) FROM assembly_line_requests),
               (SELECT COUNT(*) FROM assembly_line_audit)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    let before_state: (i64, i64, i64) = connection
        .query_row(
            "SELECT owner_control_revision,state_revision,queue_revision
             FROM assembly_line_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    drop(connection);

    assert!(matches!(
        kernel.approve_assembly_line_feature_and_enqueue(&feature_approval, 113),
        Err(MasterError::IntegerOutOfRange)
    ));
    let connection = Connection::open(&database).unwrap();
    let after_counts: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM assembly_line_owner_approvals),
               (SELECT COUNT(*) FROM assembly_line_queue),
               (SELECT COUNT(*) FROM assembly_line_requests),
               (SELECT COUNT(*) FROM assembly_line_audit)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    let after_state: (i64, i64, i64) = connection
        .query_row(
            "SELECT owner_control_revision,state_revision,queue_revision
             FROM assembly_line_state WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(after_counts, before_counts);
    assert_eq!(after_state, before_state);
    assert_eq!(after_state, (i64::MAX, i64::MAX, i64::MAX));
}

#[test]
fn schema_v19_file_upgrade_is_backup_first_and_preserves_legacy_tables() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    drop_assembly_line_schema(&connection);
    connection.execute_batch("PRAGMA user_version=19;").unwrap();
    drop(connection);

    let process = MasterProcess::acquire(temp.path()).unwrap();
    assert_eq!(process.kernel().schema_version().unwrap(), 20);
    let backup = process.migration_backup_path().unwrap();
    assert!(backup.exists());
    let backup_connection = Connection::open(backup).unwrap();
    assert_eq!(
        backup_connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        19
    );
    assert!(
        process
            .kernel()
            .assembly_line_owner_projection(1)
            .unwrap()
            .assembly_line
            .auto_run
    );
}

#[test]
fn forged_v20_names_at_schema_v19_fail_closed_and_restore_the_backup() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    drop_assembly_line_schema(&connection);
    connection
        .execute_batch(
            "CREATE TABLE assembly_line_project_drafts(draft_id TEXT PRIMARY KEY);
             CREATE TRIGGER assembly_line_project_drafts_no_update
               BEFORE UPDATE ON assembly_line_project_drafts
               BEGIN SELECT 1; END;
             PRAGMA user_version=19;",
        )
        .unwrap();
    drop(connection);

    assert!(matches!(
        MasterProcess::acquire(temp.path()),
        Err(MasterError::Storage(_))
    ));
    let restored = Connection::open(&database).unwrap();
    assert_eq!(
        restored
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        19
    );
    let trigger_sql: String = restored
        .query_row(
            "SELECT sql FROM sqlite_master
             WHERE type='trigger' AND name='assembly_line_project_drafts_no_update'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(trigger_sql.contains("SELECT 1"));
    assert_eq!(
        restored
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table' AND name='assembly_line_requests'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn planning_audit_and_request_records_are_digest_only_and_content_never_enters_legacy_effect_tables(
) {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let draft = project_draft(repository("https://github.com/owner/redaction-test"));
    kernel
        .record_assembly_line_project_draft(&draft, 100)
        .unwrap();
    let connection = Connection::open(&database).unwrap();
    let metadata: String = connection
        .query_row(
            "SELECT redacted_metadata_json FROM assembly_line_audit",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!metadata.contains(&draft.idea));
    assert!(!metadata.contains("github.com"));
    let request_columns: Vec<String> = connection
        .prepare("PRAGMA table_info(assembly_line_requests)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!request_columns.iter().any(|name| matches!(
        name.as_str(),
        "idea" | "content" | "provider_output" | "git_url"
    )));
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM master_steps", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM feature_conveyor_queue", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
