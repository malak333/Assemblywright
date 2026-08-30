use super::{
    github_effect_idempotency_key, run_brainstorming, run_github_repository_creation,
    BrainstormingAdapter, BrainstormingAdapterBinding, BrainstormingAdapterError,
    BrainstormingDraft, GithubRepositoryCreationAdapter, GithubRepositoryCreationError,
    GithubRepositoryObservation, MasterError, MasterKernel, PlanningEffectAdapterCatalog,
    PlanningEffectControl, WindowsPlanningEffectAuthority,
};
use assemblywright_protocol::{
    AssemblyLineRepositoryIdentity, BrainstormingAcceptanceCriterion,
    BrainstormingOwnerApprovalBinding, BrainstormingSpecificationDocument, BrainstormingTargetKind,
    CanonicalGitHubRepositoryUrl, FeatureBrainstormingDraft, OrchestratorCatalog,
    OrchestratorProfile, ProjectBrainstormingDraft, ProjectVisibility, RepositoryCreationLifecycle,
    RepositoryCreationProjection, FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn control() -> PlanningEffectControl {
    PlanningEffectControl::new(
        Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_secs(30),
    )
}

fn brainstorming_digest() -> [u8; 32] {
    Sha256::digest(b"fixed-test-brainstorming-adapter").into()
}

fn github_digest() -> [u8; 32] {
    Sha256::digest(b"fixed-test-github-adapter").into()
}

fn catalog() -> WindowsPlanningEffectAuthority {
    WindowsPlanningEffectAuthority::for_test(
        PlanningEffectAdapterCatalog::new(
            1,
            vec![BrainstormingAdapterBinding {
                profile: OrchestratorProfile::default(),
                executable_sha256: brainstorming_digest(),
            }],
            vec![github_digest()],
        )
        .unwrap(),
    )
}

fn repository(url: &str) -> AssemblyLineRepositoryIdentity {
    AssemblyLineRepositoryIdentity {
        repository_id: Uuid::new_v4(),
        git_url: CanonicalGitHubRepositoryUrl::parse(url).unwrap(),
    }
}

fn draft(
    repository: AssemblyLineRepositoryIdentity,
    visibility: ProjectVisibility,
) -> ProjectBrainstormingDraft {
    ProjectBrainstormingDraft {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft_id: Uuid::new_v4(),
        draft_revision: 1,
        repository,
        visibility,
        orchestrator_catalog: OrchestratorCatalog::default(),
        orchestrator: OrchestratorProfile::default(),
        idea: "Create a small owner-approved project".to_string(),
    }
}

fn specification() -> BrainstormingSpecificationDocument {
    BrainstormingSpecificationDocument {
        title: "Owner-approved project".to_string(),
        outcome: "Create the exact repository and initialize main.".to_string(),
        acceptance_criteria: vec![BrainstormingAcceptanceCriterion {
            id: "repository-created".to_string(),
            requirement: "The canonical GitHub repository exists with the selected visibility."
                .to_string(),
        }],
        obligations: vec!["Run focused native tests before release.".to_string()],
    }
}

struct FakeBrainstorming {
    profile: OrchestratorProfile,
    result: Result<BrainstormingSpecificationDocument, BrainstormingAdapterError>,
    calls: usize,
    reconcile_calls: usize,
    reconcile_result: Result<Option<BrainstormingSpecificationDocument>, BrainstormingAdapterError>,
    idempotency_keys: Vec<[u8; 32]>,
    cancel_on_generate: Option<Arc<AtomicBool>>,
}

impl FakeBrainstorming {
    fn successful() -> Self {
        Self {
            profile: OrchestratorProfile::default(),
            result: Ok(specification()),
            calls: 0,
            reconcile_calls: 0,
            reconcile_result: Ok(None),
            idempotency_keys: Vec::new(),
            cancel_on_generate: None,
        }
    }
}

impl BrainstormingAdapter for FakeBrainstorming {
    fn binding(&self) -> Option<BrainstormingAdapterBinding> {
        Some(BrainstormingAdapterBinding {
            profile: self.profile.clone(),
            executable_sha256: brainstorming_digest(),
        })
    }

    fn generate(
        &mut self,
        _draft: &BrainstormingDraft,
        idempotency_key: [u8; 32],
        _control: &PlanningEffectControl,
    ) -> Result<BrainstormingSpecificationDocument, BrainstormingAdapterError> {
        self.calls += 1;
        self.idempotency_keys.push(idempotency_key);
        if let Some(cancelled) = &self.cancel_on_generate {
            cancelled.store(true, Ordering::Release);
        }
        self.result.clone()
    }

    fn reconcile(
        &mut self,
        idempotency_key: [u8; 32],
        _control: &PlanningEffectControl,
    ) -> Result<Option<BrainstormingSpecificationDocument>, BrainstormingAdapterError> {
        self.reconcile_calls += 1;
        self.idempotency_keys.push(idempotency_key);
        self.reconcile_result.clone()
    }
}

fn approve_project(
    kernel: &mut MasterKernel,
    draft: &ProjectBrainstormingDraft,
    frozen: &assemblywright_protocol::FrozenBrainstormingSpecification,
) -> RepositoryCreationProjection {
    let owner_control_revision = kernel
        .assembly_line_owner_projection(100)
        .unwrap()
        .owner_control_revision;
    let mut approval = BrainstormingOwnerApprovalBinding {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        approval_id: Uuid::new_v4(),
        approved_at_ms: 101,
        owner_control_revision,
        target_kind: BrainstormingTargetKind::Project,
        repository: draft.repository.clone(),
        visibility: Some(draft.visibility),
        expected_repository_revision: Some(0),
        expected_queue_revision: None,
        draft_id: draft.draft_id,
        draft_revision: draft.draft_revision,
        draft_sha256: draft.canonical_sha256().unwrap(),
        orchestrator_catalog_revision: draft.orchestrator_catalog.catalog_revision,
        orchestrator_catalog_sha256: draft.orchestrator_catalog.catalog_sha256,
        specification_id: frozen.specification_id,
        specification_revision: frozen.specification_revision,
        specification_sha256: frozen.specification_sha256,
        orchestrator_profile_sha256: draft.orchestrator.canonical_sha256().unwrap(),
        owner_approval_sha256: [0; 32],
    };
    approval.owner_approval_sha256 = approval.canonical_approval_sha256().unwrap();
    kernel
        .approve_assembly_line_project(&approval, 102)
        .unwrap()
}

fn pending_project(
    visibility: ProjectVisibility,
) -> (
    MasterKernel,
    ProjectBrainstormingDraft,
    RepositoryCreationProjection,
) {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let draft = draft(
        repository("https://github.com/Example-Owner/New-Repository"),
        visibility,
    );
    let mut provider = FakeBrainstorming::successful();
    let frozen = run_brainstorming(
        &mut kernel,
        BrainstormingDraft::Project(draft.clone()),
        &mut provider,
        &catalog(),
        &control(),
    )
    .unwrap();
    let projection = approve_project(&mut kernel, &draft, &frozen);
    (kernel, draft, projection)
}

#[derive(Clone)]
enum CreateResult {
    Exact,
    Error(GithubRepositoryCreationError),
}

struct FakeGithub {
    binding: Option<[u8; 32]>,
    observations: Vec<Result<Option<GithubRepositoryObservation>, GithubRepositoryCreationError>>,
    create_result: CreateResult,
    inspected: Vec<AssemblyLineRepositoryIdentity>,
    created: Vec<(AssemblyLineRepositoryIdentity, ProjectVisibility)>,
    inspection_keys: Vec<[u8; 32]>,
    creation_keys: Vec<[u8; 32]>,
    cancel_on_inspect_call: Option<(usize, Arc<AtomicBool>)>,
}

impl FakeGithub {
    fn exact() -> Self {
        Self {
            binding: Some(github_digest()),
            observations: vec![Ok(None)],
            create_result: CreateResult::Exact,
            inspected: Vec::new(),
            created: Vec::new(),
            inspection_keys: Vec::new(),
            creation_keys: Vec::new(),
            cancel_on_inspect_call: None,
        }
    }
}

impl GithubRepositoryCreationAdapter for FakeGithub {
    fn binding_sha256(&self) -> Option<[u8; 32]> {
        self.binding
    }

    fn inspect(
        &mut self,
        repository: &AssemblyLineRepositoryIdentity,
        idempotency_key: [u8; 32],
        _control: &PlanningEffectControl,
    ) -> Result<Option<GithubRepositoryObservation>, GithubRepositoryCreationError> {
        self.inspected.push(repository.clone());
        self.inspection_keys.push(idempotency_key);
        if self
            .cancel_on_inspect_call
            .as_ref()
            .is_some_and(|(call, _)| *call == self.inspected.len())
        {
            self.cancel_on_inspect_call
                .as_ref()
                .unwrap()
                .1
                .store(true, Ordering::Release);
        }
        if self.observations.is_empty() {
            return Ok(None);
        }
        self.observations.remove(0)
    }

    fn create(
        &mut self,
        plan: &RepositoryCreationProjection,
        idempotency_key: [u8; 32],
        _control: &PlanningEffectControl,
    ) -> Result<GithubRepositoryObservation, GithubRepositoryCreationError> {
        self.created
            .push((plan.repository.clone(), plan.visibility));
        self.creation_keys.push(idempotency_key);
        match self.create_result {
            CreateResult::Exact => Ok(GithubRepositoryObservation {
                repository: plan.repository.clone(),
                visibility: plan.visibility,
                default_branch: "main".to_string(),
                initialized: true,
            }),
            CreateResult::Error(error) => Err(error),
        }
    }
}

#[test]
fn brainstorming_is_exactly_profile_bound_and_replay_does_not_call_provider_twice() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let draft = draft(
        repository("https://github.com/example-owner/brainstormed"),
        ProjectVisibility::Public,
    );
    let mut provider = FakeBrainstorming::successful();
    let first = run_brainstorming(
        &mut kernel,
        BrainstormingDraft::Project(draft.clone()),
        &mut provider,
        &catalog(),
        &control(),
    )
    .unwrap();
    let replay = run_brainstorming(
        &mut kernel,
        BrainstormingDraft::Project(draft),
        &mut provider,
        &catalog(),
        &control(),
    )
    .unwrap();

    assert_eq!(first, replay);
    assert_eq!(provider.calls, 1);
}

#[test]
fn provider_rejection_creates_no_specification_repository_or_github_effect() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let draft = draft(
        repository("https://github.com/example-owner/rejected"),
        ProjectVisibility::Public,
    );
    let mut provider = FakeBrainstorming {
        profile: OrchestratorProfile::default(),
        result: Err(BrainstormingAdapterError::Rejected),
        calls: 0,
        reconcile_calls: 0,
        reconcile_result: Ok(None),
        idempotency_keys: Vec::new(),
        cancel_on_generate: None,
    };

    assert!(matches!(
        run_brainstorming(
            &mut kernel,
            BrainstormingDraft::Project(draft.clone()),
            &mut provider,
            &catalog(),
            &control()
        ),
        Err(MasterError::AssemblyLineBrainstormingRejected)
    ));
    assert!(kernel
        .assembly_line_frozen_specification_for_draft(
            BrainstormingTargetKind::Project,
            draft.draft_id
        )
        .unwrap()
        .is_none());
    assert!(kernel
        .assembly_line_owner_projection(200)
        .unwrap()
        .repositories
        .is_empty());
}

#[test]
fn github_creation_maps_public_and_private_exactly_and_binds_url_owner_repo() {
    for visibility in [ProjectVisibility::Public, ProjectVisibility::Private] {
        let (mut kernel, draft, pending) = pending_project(visibility);
        assert_eq!(
            pending.lifecycle,
            RepositoryCreationLifecycle::CreationPending
        );
        let mut github = FakeGithub::exact();

        let created = run_github_repository_creation(
            &mut kernel,
            draft.repository.repository_id,
            &mut github,
            &catalog(),
            &control(),
        )
        .unwrap();

        assert_eq!(created.lifecycle, RepositoryCreationLifecycle::Created);
        assert_eq!(created.visibility, visibility);
        assert!(created.creation_evidence_sha256.is_some());
        assert_eq!(github.inspected, vec![draft.repository.clone()]);
        assert_eq!(github.created, vec![(draft.repository, visibility)]);
    }
}

#[test]
fn preexisting_repository_is_a_conflict_and_create_is_never_called() {
    let (mut kernel, draft, pending) = pending_project(ProjectVisibility::Public);
    let mut github = FakeGithub::exact();
    github.observations = vec![Ok(Some(GithubRepositoryObservation {
        repository: draft.repository.clone(),
        visibility: pending.visibility,
        default_branch: "main".to_string(),
        initialized: true,
    }))];

    assert!(matches!(
        run_github_repository_creation(
            &mut kernel,
            draft.repository.repository_id,
            &mut github,
            &catalog(),
            &control()
        ),
        Err(MasterError::AssemblyLineGithubCreationConflict)
    ));
    assert!(github.created.is_empty());
    assert_eq!(
        kernel
            .assembly_line_repository_creation_projection(draft.repository.repository_id)
            .unwrap()
            .lifecycle,
        RepositoryCreationLifecycle::Conflict
    );
}

#[test]
fn lost_creation_response_reconciles_exact_remote_without_second_create() {
    let (mut kernel, draft, pending) = pending_project(ProjectVisibility::Private);
    let exact = GithubRepositoryObservation {
        repository: draft.repository.clone(),
        visibility: pending.visibility,
        default_branch: "main".to_string(),
        initialized: true,
    };
    let mut github = FakeGithub {
        binding: Some(github_digest()),
        observations: vec![Ok(None), Ok(Some(exact))],
        create_result: CreateResult::Error(GithubRepositoryCreationError::Ambiguous),
        inspected: Vec::new(),
        created: Vec::new(),
        inspection_keys: Vec::new(),
        creation_keys: Vec::new(),
        cancel_on_inspect_call: None,
    };

    let created = run_github_repository_creation(
        &mut kernel,
        draft.repository.repository_id,
        &mut github,
        &catalog(),
        &control(),
    )
    .unwrap();
    assert_eq!(created.lifecycle, RepositoryCreationLifecycle::Created);
    assert_eq!(github.created.len(), 1);

    let replay = run_github_repository_creation(
        &mut kernel,
        draft.repository.repository_id,
        &mut github,
        &catalog(),
        &control(),
    )
    .unwrap();
    assert_eq!(replay, created);
    assert_eq!(github.created.len(), 1);
}

#[test]
fn unavailable_or_rejected_adapters_do_not_create_or_mark_remote_success() {
    let (mut kernel, draft, _) = pending_project(ProjectVisibility::Public);
    let mut unavailable = FakeGithub::exact();
    unavailable.binding = None;
    assert!(matches!(
        run_github_repository_creation(
            &mut kernel,
            draft.repository.repository_id,
            &mut unavailable,
            &catalog(),
            &control()
        ),
        Err(MasterError::AssemblyLineGithubCreationUnavailable)
    ));
    assert!(unavailable.created.is_empty());

    let mut rejected = FakeGithub::exact();
    rejected.observations = vec![Err(GithubRepositoryCreationError::Rejected)];
    assert!(matches!(
        run_github_repository_creation(
            &mut kernel,
            draft.repository.repository_id,
            &mut rejected,
            &catalog(),
            &control()
        ),
        Err(MasterError::AssemblyLineGithubCreationUnavailable)
    ));
    assert!(rejected.created.is_empty());
    assert_eq!(
        kernel
            .assembly_line_repository_creation_projection(draft.repository.repository_id)
            .unwrap()
            .lifecycle,
        RepositoryCreationLifecycle::CreationPending
    );
}

#[test]
fn cancellation_before_an_effect_keeps_draft_and_creation_intent_inert() {
    let cancelled = Arc::new(AtomicBool::new(true));
    let cancelled_control = PlanningEffectControl::new(
        Arc::clone(&cancelled),
        Instant::now() + Duration::from_secs(30),
    );
    let mut kernel = MasterKernel::in_memory().unwrap();
    let rejected_draft = draft(
        repository("https://github.com/example-owner/cancelled-brainstorm"),
        ProjectVisibility::Public,
    );
    let mut provider = FakeBrainstorming::successful();
    assert!(matches!(
        run_brainstorming(
            &mut kernel,
            BrainstormingDraft::Project(rejected_draft.clone()),
            &mut provider,
            &catalog(),
            &cancelled_control,
        ),
        Err(MasterError::AssemblyLineBrainstormingUnavailable)
    ));
    assert_eq!(provider.calls, 0);
    assert!(kernel
        .assembly_line_frozen_specification_for_draft(
            BrainstormingTargetKind::Project,
            rejected_draft.draft_id,
        )
        .unwrap()
        .is_none());

    cancelled.store(false, Ordering::Release);
    let (mut kernel, draft, _) = pending_project(ProjectVisibility::Public);
    cancelled.store(true, Ordering::Release);
    let mut github = FakeGithub::exact();
    assert!(matches!(
        run_github_repository_creation(
            &mut kernel,
            draft.repository.repository_id,
            &mut github,
            &catalog(),
            &cancelled_control,
        ),
        Err(MasterError::AssemblyLineGithubCreationUnavailable)
    ));
    assert!(github.inspected.is_empty());
    assert!(github.created.is_empty());
    assert_eq!(
        kernel
            .assembly_line_repository_creation_projection(draft.repository.repository_id)
            .unwrap()
            .lifecycle,
        RepositoryCreationLifecycle::CreationPending
    );
}

#[test]
fn reconciliation_rejects_adapter_binding_drift_before_remote_observation() {
    let (mut kernel, draft, pending) = pending_project(ProjectVisibility::Public);
    let first_digest: [u8; 32] = Sha256::digest(b"first-fixed-github-adapter").into();
    let drifted_digest: [u8; 32] = Sha256::digest(b"different-fixed-github-adapter").into();
    let drift_catalog = WindowsPlanningEffectAuthority::for_test(
        PlanningEffectAdapterCatalog::new(
            1,
            vec![BrainstormingAdapterBinding {
                profile: OrchestratorProfile::default(),
                executable_sha256: brainstorming_digest(),
            }],
            vec![first_digest, drifted_digest],
        )
        .unwrap(),
    );
    let mut first = FakeGithub {
        binding: Some(first_digest),
        observations: vec![Ok(None), Ok(None)],
        create_result: CreateResult::Error(GithubRepositoryCreationError::Ambiguous),
        inspected: Vec::new(),
        created: Vec::new(),
        inspection_keys: Vec::new(),
        creation_keys: Vec::new(),
        cancel_on_inspect_call: None,
    };
    assert!(matches!(
        run_github_repository_creation(
            &mut kernel,
            draft.repository.repository_id,
            &mut first,
            &drift_catalog,
            &control(),
        ),
        Err(MasterError::AssemblyLineGithubCreationReconciliationRequired)
    ));
    assert_eq!(first.created.len(), 1);

    let exact = GithubRepositoryObservation {
        repository: draft.repository.clone(),
        visibility: pending.visibility,
        default_branch: "main".to_string(),
        initialized: true,
    };
    let mut drifted = FakeGithub {
        binding: Some(drifted_digest),
        observations: vec![Ok(Some(exact))],
        create_result: CreateResult::Exact,
        inspected: Vec::new(),
        created: Vec::new(),
        inspection_keys: Vec::new(),
        creation_keys: Vec::new(),
        cancel_on_inspect_call: None,
    };
    assert!(matches!(
        run_github_repository_creation(
            &mut kernel,
            draft.repository.repository_id,
            &mut drifted,
            &drift_catalog,
            &control(),
        ),
        Err(MasterError::AssemblyLineGithubCreationReconciliationRequired)
    ));
    assert!(drifted.inspected.is_empty());
    assert_eq!(
        kernel
            .assembly_line_repository_creation_projection(draft.repository.repository_id)
            .unwrap()
            .lifecycle,
        RepositoryCreationLifecycle::ReconciliationRequired
    );
}

#[test]
fn feature_brainstorming_uses_the_created_git_url_but_does_not_enqueue() {
    let (mut kernel, project, _) = pending_project(ProjectVisibility::Public);
    let mut github = FakeGithub::exact();
    let created = run_github_repository_creation(
        &mut kernel,
        project.repository.repository_id,
        &mut github,
        &catalog(),
        &control(),
    )
    .unwrap();
    let feature = FeatureBrainstormingDraft {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft_id: Uuid::new_v4(),
        draft_revision: 1,
        repository: project.repository.clone(),
        expected_repository_revision: created.repository_revision,
        orchestrator_catalog: OrchestratorCatalog::default(),
        orchestrator: OrchestratorProfile::default(),
        idea: "Add one owner-visible feature".to_string(),
    };
    let mut feature_specification = specification();
    feature_specification.title = "Owner-approved feature".to_string();
    feature_specification.outcome = "Add the exact feature without enqueueing it yet.".to_string();
    let mut provider = FakeBrainstorming {
        profile: OrchestratorProfile::default(),
        result: Ok(feature_specification),
        calls: 0,
        reconcile_calls: 0,
        reconcile_result: Ok(None),
        idempotency_keys: Vec::new(),
        cancel_on_generate: None,
    };
    let frozen = run_brainstorming(
        &mut kernel,
        BrainstormingDraft::Feature(feature),
        &mut provider,
        &catalog(),
        &control(),
    )
    .unwrap();

    assert_eq!(frozen.target_kind, BrainstormingTargetKind::Feature);
    assert_eq!(frozen.repository, project.repository);
    assert_eq!(frozen.visibility, None);
    assert!(kernel
        .assembly_line_owner_projection(300)
        .unwrap()
        .queue
        .is_empty());
}

#[test]
fn accepted_brainstorming_crash_retries_by_exact_reconciliation_without_redisclosure() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let effect_control = PlanningEffectControl::new(
        Arc::clone(&cancelled),
        Instant::now() + Duration::from_secs(30),
    );
    let mut kernel = MasterKernel::in_memory().unwrap();
    let project = draft(
        repository("https://github.com/example-owner/provider-crash"),
        ProjectVisibility::Public,
    );
    let mut provider = FakeBrainstorming {
        profile: OrchestratorProfile::default(),
        result: Err(BrainstormingAdapterError::Timeout),
        calls: 0,
        reconcile_calls: 0,
        reconcile_result: Ok(Some(specification())),
        idempotency_keys: Vec::new(),
        cancel_on_generate: Some(Arc::clone(&cancelled)),
    };

    assert!(matches!(
        run_brainstorming(
            &mut kernel,
            BrainstormingDraft::Project(project.clone()),
            &mut provider,
            &catalog(),
            &effect_control,
        ),
        Err(MasterError::AssemblyLineBrainstormingUnavailable)
    ));
    assert_eq!(provider.calls, 1);
    assert_eq!(provider.reconcile_calls, 0);

    cancelled.store(false, Ordering::Release);
    provider.cancel_on_generate = None;
    let frozen = run_brainstorming(
        &mut kernel,
        BrainstormingDraft::Project(project),
        &mut provider,
        &catalog(),
        &effect_control,
    )
    .unwrap();
    assert_eq!(frozen.specification, specification());
    assert_eq!(provider.calls, 1);
    assert_eq!(provider.reconcile_calls, 1);
    assert_eq!(provider.idempotency_keys[0], provider.idempotency_keys[1]);
}

#[test]
fn cancellation_after_post_create_inspection_never_marks_repository_created() {
    let (mut kernel, project, pending) = pending_project(ProjectVisibility::Private);
    let cancelled = Arc::new(AtomicBool::new(false));
    let effect_control = PlanningEffectControl::new(
        Arc::clone(&cancelled),
        Instant::now() + Duration::from_secs(30),
    );
    let exact = GithubRepositoryObservation {
        repository: project.repository.clone(),
        visibility: pending.visibility,
        default_branch: "main".to_string(),
        initialized: true,
    };
    let mut github = FakeGithub {
        binding: Some(github_digest()),
        observations: vec![Ok(None), Ok(Some(exact))],
        create_result: CreateResult::Error(GithubRepositoryCreationError::Ambiguous),
        inspected: Vec::new(),
        created: Vec::new(),
        inspection_keys: Vec::new(),
        creation_keys: Vec::new(),
        cancel_on_inspect_call: Some((2, cancelled)),
    };

    assert!(matches!(
        run_github_repository_creation(
            &mut kernel,
            project.repository.repository_id,
            &mut github,
            &catalog(),
            &effect_control,
        ),
        Err(MasterError::AssemblyLineGithubCreationReconciliationRequired)
    ));
    assert_eq!(github.created.len(), 1);
    assert_eq!(github.inspection_keys[0], github.inspection_keys[1]);
    assert_eq!(
        kernel
            .assembly_line_repository_creation_projection(project.repository.repository_id)
            .unwrap()
            .lifecycle,
        RepositoryCreationLifecycle::ReconciliationRequired
    );
}

#[test]
fn cancellation_immediately_after_creation_intent_audit_requires_reconciliation() {
    let (mut kernel, project, _) = pending_project(ProjectVisibility::Public);
    let effect_control =
        PlanningEffectControl::cancel_on_poll(Instant::now() + Duration::from_secs(30), 4);
    let mut github = FakeGithub::exact();

    assert!(matches!(
        run_github_repository_creation(
            &mut kernel,
            project.repository.repository_id,
            &mut github,
            &catalog(),
            &effect_control,
        ),
        Err(MasterError::AssemblyLineGithubCreationReconciliationRequired)
    ));
    assert!(github.created.is_empty());
    assert_eq!(github.inspected.len(), 1);
    assert_eq!(
        kernel
            .assembly_line_repository_creation_projection(project.repository.repository_id)
            .unwrap()
            .lifecycle,
        RepositoryCreationLifecycle::ReconciliationRequired
    );
}

#[test]
fn cancellation_after_recovery_inspection_audit_quarantines_reconciling_state() {
    let (mut kernel, project, pending) = pending_project(ProjectVisibility::Public);
    let authority = catalog();
    let creation_key = github_effect_idempotency_key(
        "create",
        &pending,
        github_digest(),
        authority.catalog.catalog_sha256,
    )
    .unwrap();
    kernel
        .begin_assembly_line_repository_creation(
            project.repository.repository_id,
            github_digest(),
            authority.catalog.catalog_sha256,
            creation_key,
            super::current_time_ms().unwrap(),
        )
        .unwrap();
    let effect_control =
        PlanningEffectControl::cancel_on_poll(Instant::now() + Duration::from_secs(30), 2);
    let mut github = FakeGithub::exact();

    assert!(matches!(
        run_github_repository_creation(
            &mut kernel,
            project.repository.repository_id,
            &mut github,
            &authority,
            &effect_control,
        ),
        Err(MasterError::AssemblyLineGithubCreationReconciliationRequired)
    ));
    assert!(github.inspected.is_empty());
    assert!(github.created.is_empty());
    assert_eq!(
        kernel
            .assembly_line_repository_creation_projection(project.repository.repository_id)
            .unwrap()
            .lifecycle,
        RepositoryCreationLifecycle::ReconciliationRequired
    );
}

#[test]
fn failed_initial_inspection_has_durable_redacted_pre_call_audit() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("master.sqlite");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let project = draft(
        repository("https://github.com/example-owner/inspection-failure"),
        ProjectVisibility::Public,
    );
    let mut provider = FakeBrainstorming::successful();
    let frozen = run_brainstorming(
        &mut kernel,
        BrainstormingDraft::Project(project.clone()),
        &mut provider,
        &catalog(),
        &control(),
    )
    .unwrap();
    approve_project(&mut kernel, &project, &frozen);
    let mut github = FakeGithub::exact();
    github.observations = vec![Err(GithubRepositoryCreationError::Rejected)];
    assert!(matches!(
        run_github_repository_creation(
            &mut kernel,
            project.repository.repository_id,
            &mut github,
            &catalog(),
            &control(),
        ),
        Err(MasterError::AssemblyLineGithubCreationUnavailable)
    ));
    drop(kernel);

    let connection = Connection::open(database).unwrap();
    let metadata: String = connection
        .query_row(
            "SELECT redacted_metadata_json FROM assembly_line_audit
             WHERE event_kind='github_repository_inspection_started'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(metadata.contains(&project.repository.repository_id.to_string()));
    assert!(!metadata.contains(&project.repository.git_url.url));
    assert!(!metadata.to_ascii_lowercase().contains("credential"));
}

#[test]
fn self_asserted_adapter_digest_outside_windows_catalog_has_no_effect() {
    let (mut kernel, project, _) = pending_project(ProjectVisibility::Public);
    let mut github = FakeGithub::exact();
    github.binding = Some(Sha256::digest(b"unconfigured-adapter").into());

    assert!(matches!(
        run_github_repository_creation(
            &mut kernel,
            project.repository.repository_id,
            &mut github,
            &catalog(),
            &control(),
        ),
        Err(MasterError::AssemblyLineGithubCreationUnavailable)
    ));
    assert!(github.inspected.is_empty());
    assert!(github.created.is_empty());
}
