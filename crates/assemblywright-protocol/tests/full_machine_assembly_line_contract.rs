use assemblywright_protocol::{
    AssemblyLineAutoRunReceipt, AssemblyLineAutoRunRequest, AssemblyLineChildEpoch,
    AssemblyLineEmergencyPauseReceipt, AssemblyLineEmergencyPauseRequest,
    AssemblyLineLifecycleState, AssemblyLineOwnerProjection, AssemblyLineRepositoryIdentity,
    AssemblyLineRuntimeAvailabilityProjection, AssemblyLineSessionEpoch, AssemblyLineStartReceipt,
    AssemblyLineStartRequest, AssemblyLineState, AssemblyLineStopReceipt, AssemblyLineStopRequest,
    BrainstormingAcceptanceCriterion, BrainstormingOwnerApprovalBinding,
    BrainstormingSpecificationDocument, BrainstormingTargetKind, CanonicalGitHubRepositoryUrl,
    FeatureBrainstormingCloudRequest, FeatureBrainstormingDraft, FeatureQueueEntryProjection,
    FeatureQueueLifecycle, FrozenBrainstormingSpecification, OrchestratorCatalog,
    OrchestratorProfile, ProcessTerminationEvidenceReference, ProcessTerminationOutcome,
    ProjectBrainstormingCloudRequest, ProjectBrainstormingDraft, ProjectVisibility,
    PublicInformationClassification, RepositoryCreationLifecycle, RepositoryCreationProjection,
    RuntimeAvailabilityStatus, RuntimeComponentAvailability, RuntimeUnavailableReason,
    FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION, MAX_ASSEMBLY_LINE_OWNER_PROJECTION_BYTES,
    MAX_ASSEMBLY_LINE_QUEUE_COUNT, MAX_BRAINSTORMING_INPUT_BYTES,
    MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES, PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use uuid::Uuid;

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).unwrap()
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn repository() -> AssemblyLineRepositoryIdentity {
    AssemblyLineRepositoryIdentity {
        repository_id: id("11111111-1111-4111-8111-111111111111"),
        git_url: CanonicalGitHubRepositoryUrl::parse(
            "https://github.com/Assemblywright/Protocol-Test.git/",
        )
        .unwrap(),
    }
}

fn project_draft() -> ProjectBrainstormingDraft {
    ProjectBrainstormingDraft {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft_id: id("22222222-2222-4222-8222-222222222222"),
        draft_revision: 1,
        repository: repository(),
        visibility: ProjectVisibility::Public,
        orchestrator_catalog: OrchestratorCatalog::default(),
        orchestrator: OrchestratorProfile::default(),
        idea: "Create a bounded project with native tests and durable documentation.".into(),
    }
}

fn feature_draft() -> FeatureBrainstormingDraft {
    FeatureBrainstormingDraft {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft_id: id("33333333-3333-4333-8333-333333333333"),
        draft_revision: 2,
        repository: repository(),
        expected_repository_revision: 4,
        orchestrator_catalog: OrchestratorCatalog::default(),
        orchestrator: OrchestratorProfile::default(),
        idea: "Add one strict owner-approved feature with regression coverage.".into(),
    }
}

fn project_cloud_request() -> ProjectBrainstormingCloudRequest {
    let mut request = ProjectBrainstormingCloudRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft: project_draft(),
        information_classification: PublicInformationClassification::Public,
        owner_cloud_disclosure_sha256: [0; 32],
    };
    request.owner_cloud_disclosure_sha256 = request.canonical_disclosure_sha256().unwrap();
    request
}

fn feature_cloud_request() -> FeatureBrainstormingCloudRequest {
    let mut request = FeatureBrainstormingCloudRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        draft: feature_draft(),
        information_classification: PublicInformationClassification::Public,
        owner_cloud_disclosure_sha256: [0; 32],
    };
    request.owner_cloud_disclosure_sha256 = request.canonical_disclosure_sha256().unwrap();
    request
}

fn specification() -> BrainstormingSpecificationDocument {
    BrainstormingSpecificationDocument {
        title: "Strict project contract".into(),
        outcome: "Produce one owner-reviewable, test-backed change.".into(),
        acceptance_criteria: vec![BrainstormingAcceptanceCriterion {
            id: "acceptance-1".into(),
            requirement: "Malformed input fails before any effect.".into(),
        }],
        obligations: vec!["Run native validation and retain redacted evidence.".into()],
    }
}

fn frozen_project() -> FrozenBrainstormingSpecification {
    let draft = project_draft();
    let specification = specification();
    FrozenBrainstormingSpecification {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        specification_id: id("44444444-4444-4444-8444-444444444444"),
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
        specification_sha256: specification.canonical_sha256().unwrap(),
        specification,
    }
}

fn project_approval() -> BrainstormingOwnerApprovalBinding {
    let frozen = frozen_project();
    let mut approval = BrainstormingOwnerApprovalBinding {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        approval_id: id("55555555-5555-4555-8555-555555555555"),
        approved_at_ms: 1_800_000_000_000,
        owner_control_revision: 7,
        target_kind: BrainstormingTargetKind::Project,
        repository: repository(),
        visibility: Some(ProjectVisibility::Public),
        expected_repository_revision: Some(0),
        expected_queue_revision: None,
        draft_id: frozen.draft_id,
        draft_revision: frozen.draft_revision,
        draft_sha256: frozen.draft_sha256,
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

#[test]
fn github_url_is_canonical_and_separate_from_internal_identity() {
    assert_eq!(PROTOCOL_VERSION, 5);
    let canonical =
        CanonicalGitHubRepositoryUrl::parse("HTTPS://GITHUB.COM/Assemblywright/Protocol-Test.git/")
            .unwrap();
    assert_eq!(
        canonical.url,
        "https://github.com/assemblywright/protocol-test"
    );
    canonical.validate().unwrap();

    let identity = AssemblyLineRepositoryIdentity {
        repository_id: id("11111111-1111-4111-8111-111111111111"),
        git_url: canonical,
    };
    identity.validate().unwrap();
    assert!(!serde_json::to_string(&identity).unwrap().contains("path"));

    let mut nil = identity.clone();
    nil.repository_id = Uuid::nil();
    assert!(nil.validate().is_err());
    let mut noncanonical = identity;
    noncanonical.git_url.url = "https://github.com/Assemblywright/protocol-test".into();
    assert!(noncanonical.validate().is_err());
}

#[test]
fn repository_and_orchestrator_canonical_digests_are_fixed() {
    assert_eq!(
        hex(repository().canonical_sha256().unwrap()),
        "39f2f4a280157749f16e6ce0f3601a421e8a6f0e6a048fe01d0efa3e5d09b3cf"
    );
    assert_eq!(
        hex(OrchestratorProfile::default().canonical_sha256().unwrap()),
        "6f4eafc8125fd3762accf9628f22280f2cc21640572c48b36463162177a483ee"
    );
}

#[test]
fn production_brainstorming_cloud_disclosure_is_public_and_digest_bound() {
    let project = project_cloud_request();
    project.validate().unwrap();
    assert_eq!(
        hex(project.owner_cloud_disclosure_sha256),
        "295ec0847c198b1463ff9f4b0b5318f1b2276acf9c0e715bef585b3f3c06a914"
    );
    let project_bytes = serde_json::to_vec(&project).unwrap();
    assert_eq!(
        ProjectBrainstormingCloudRequest::decode_frame(&project_bytes).unwrap(),
        project
    );

    let feature = feature_cloud_request();
    feature.validate().unwrap();
    assert_eq!(
        hex(feature.owner_cloud_disclosure_sha256),
        "905215bf4e48f9768026ac013b66f48076b5737019c846544a7c81c4fad60dae"
    );
    let feature_bytes = serde_json::to_vec(&feature).unwrap();
    assert_eq!(
        FeatureBrainstormingCloudRequest::decode_frame(&feature_bytes).unwrap(),
        feature
    );
}

#[test]
fn production_brainstorming_cloud_disclosure_rejects_minting_and_ambiguity() {
    let mut forged = project_cloud_request();
    forged.draft.idea.push_str(" altered");
    assert!(forged.validate().is_err());

    let mut zero = feature_cloud_request();
    zero.owner_cloud_disclosure_sha256 = [0; 32];
    assert!(zero.validate().is_err());

    let mut value = serde_json::to_value(project_cloud_request()).unwrap();
    value["information_classification"] = json!("private");
    assert!(
        ProjectBrainstormingCloudRequest::decode_frame(&serde_json::to_vec(&value).unwrap())
            .is_err()
    );

    value = serde_json::to_value(project_cloud_request()).unwrap();
    value["unexpected"] = json!(true);
    assert!(
        ProjectBrainstormingCloudRequest::decode_frame(&serde_json::to_vec(&value).unwrap())
            .is_err()
    );

    let bytes = serde_json::to_string(&project_cloud_request()).unwrap();
    let duplicate = bytes.replacen("{", "{\"schema_version\":1,", 1);
    assert!(ProjectBrainstormingCloudRequest::decode_frame(duplicate.as_bytes()).is_err());
}

#[test]
fn github_url_rejects_non_https_non_github_and_url_ambiguity() {
    for value in [
        "http://github.com/owner/repo",
        "ssh://github.com/owner/repo",
        "git@github.com:owner/repo.git",
        "file:///tmp/repo",
        "https://user@github.com/owner/repo",
        "https://github.com:443/owner/repo",
        "https://github.com/owner/repo?token=secret",
        "https://github.com/owner/repo#fragment",
        "https://github.com/owner/repo/extra",
        "https://github.com/owner%2frepo",
        "https://github.com/-owner/repo",
        "https://github.com/owner/.repo",
    ] {
        assert!(
            CanonicalGitHubRepositoryUrl::parse(value).is_err(),
            "accepted malformed URL: {value}"
        );
    }
}

#[test]
fn project_visibility_and_orchestrator_defaults_are_explicit_and_no_fallback_is_accepted() {
    assert_eq!(ProjectVisibility::default(), ProjectVisibility::Public);
    let profile = OrchestratorProfile::default();
    assert_eq!(profile.provider_id, "openai.codex");
    assert_eq!(profile.model_id, "gpt-5.6-sol");
    profile.validate().unwrap();

    let draft = project_draft();
    let mut value = serde_json::to_value(&draft).unwrap();
    value.as_object_mut().unwrap().remove("visibility");
    let decoded =
        ProjectBrainstormingDraft::decode_frame(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(decoded.visibility, ProjectVisibility::Public);

    let mut private = decoded;
    private.visibility = ProjectVisibility::Private;
    private.validate().unwrap();

    let mut fallback = serde_json::to_value(&private).unwrap();
    fallback["orchestrator"]["fallback_model_id"] = json!("another-model");
    assert!(
        ProjectBrainstormingDraft::decode_frame(&serde_json::to_vec(&fallback).unwrap()).is_err()
    );
}

#[test]
fn orchestrator_selection_requires_exact_strict_catalog_membership() {
    let catalog = OrchestratorCatalog::default();
    catalog.validate().unwrap();
    catalog
        .validate_selection(&OrchestratorProfile::default())
        .unwrap();
    assert_eq!(
        catalog.default_profile_sha256,
        OrchestratorProfile::default().canonical_sha256().unwrap()
    );
    assert_eq!(
        OrchestratorCatalog::decode_frame(&serde_json::to_vec(&catalog).unwrap()).unwrap(),
        catalog
    );

    let mut absent = catalog.clone();
    absent.profiles.clear();
    assert!(absent.validate().is_err());
    let mut drifted = catalog.clone();
    drifted.catalog_revision += 1;
    assert!(drifted.validate().is_err());
    let mut digest_drift = catalog.clone();
    digest_drift.catalog_sha256 = [9; 32];
    assert!(digest_drift.validate().is_err());
    let mut duplicate = catalog.clone();
    duplicate.profiles.push(duplicate.profiles[0].clone());
    assert!(duplicate.validate().is_err());
    let unlisted = OrchestratorProfile {
        configuration_revision: catalog.catalog_revision,
        provider_id: "local.planner".into(),
        model_id: "planner-v1".into(),
    };
    assert!(catalog.validate_selection(&unlisted).is_err());

    let mut fallback = serde_json::to_value(&catalog).unwrap();
    fallback["fallback_profile_sha256"] = json!(vec![1; 32]);
    assert!(OrchestratorCatalog::decode_frame(&serde_json::to_vec(&fallback).unwrap()).is_err());
    let mut missing_catalog = serde_json::to_value(project_draft()).unwrap();
    missing_catalog
        .as_object_mut()
        .unwrap()
        .remove("orchestrator_catalog");
    assert!(ProjectBrainstormingDraft::decode_frame(
        &serde_json::to_vec(&missing_catalog).unwrap()
    )
    .is_err());
}

#[test]
fn caller_catalog_cannot_self_authorize_against_windows_catalog() {
    let authoritative = OrchestratorCatalog::default();
    let custom_profile = OrchestratorProfile {
        configuration_revision: authoritative.catalog_revision,
        provider_id: "local.planner".into(),
        model_id: "planner-v1".into(),
    };
    let mut custom_catalog = OrchestratorCatalog {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        catalog_revision: authoritative.catalog_revision,
        profiles: vec![custom_profile.clone()],
        default_profile_sha256: custom_profile.canonical_sha256().unwrap(),
        catalog_sha256: [0; 32],
    };
    custom_catalog.catalog_sha256 = custom_catalog.canonical_catalog_sha256().unwrap();
    custom_catalog.validate().unwrap();

    let mut draft = project_draft();
    draft.orchestrator_catalog = custom_catalog.clone();
    draft.orchestrator = custom_profile;
    draft.validate().unwrap();
    assert!(draft
        .validate_against_authoritative_catalog(&authoritative)
        .is_err());

    let mut frozen = frozen_project();
    frozen.draft_sha256 = draft.canonical_sha256().unwrap();
    frozen.orchestrator_catalog_revision = custom_catalog.catalog_revision;
    frozen.orchestrator_catalog_sha256 = custom_catalog.catalog_sha256;
    frozen.orchestrator_profile_sha256 = draft.orchestrator.canonical_sha256().unwrap();
    frozen.validate().unwrap();
    frozen
        .validate_for_project_draft(&draft, &custom_catalog)
        .unwrap();
    assert!(frozen
        .validate_for_project_draft(&draft, &authoritative)
        .is_err());

    let mut approval = project_approval();
    approval.draft_sha256 = frozen.draft_sha256;
    approval.orchestrator_catalog_revision = frozen.orchestrator_catalog_revision;
    approval.orchestrator_catalog_sha256 = frozen.orchestrator_catalog_sha256;
    approval.orchestrator_profile_sha256 = frozen.orchestrator_profile_sha256;
    approval.owner_approval_sha256 = approval.canonical_approval_sha256().unwrap();
    approval
        .validate_for_project(&draft, &frozen, &custom_catalog)
        .unwrap();
    assert!(approval
        .validate_for_project(&draft, &frozen, &authoritative)
        .is_err());
}

#[test]
fn brainstorming_drafts_are_strict_bounded_and_secret_free() {
    let project = project_draft();
    project.validate().unwrap();
    let encoded = serde_json::to_vec(&project).unwrap();
    assert_eq!(
        ProjectBrainstormingDraft::decode_frame(&encoded).unwrap(),
        project
    );

    let duplicate = String::from_utf8(encoded.clone()).unwrap().replacen(
        "\"draft_revision\":1",
        "\"draft_revision\":1,\"draft_revision\":2",
        1,
    );
    assert!(ProjectBrainstormingDraft::decode_frame(duplicate.as_bytes()).is_err());
    let mut unknown: Value = serde_json::from_slice(&encoded).unwrap();
    unknown["credential"] = json!("forbidden");
    assert!(
        ProjectBrainstormingDraft::decode_frame(&serde_json::to_vec(&unknown).unwrap()).is_err()
    );

    for idea in [
        "Read (/Users/Owner/.ssh/id_ed25519), now",
        "Read [ / home / owner / .ssh / id_ed25519 ]",
        "Use C:\\Users\\owner\\secret.txt",
        "Use \\\\server\\share\\secret.txt",
        "Authorization:Bearer abcdefghijklmnop",
        "AUTHORIZATION : BASIC dXNlcjpwYXNzd29yZA==",
        "client_secret: forbidden",
        "client-secret = forbidden",
        "client.secret: forbidden",
        "client secret = forbidden",
        "API_KEY: forbidden",
        "api-key = forbidden",
        "access_token: forbidden",
        "access-token = forbidden",
        "token=github_pat_forbidden",
        "embedded GHP_12345678901234567890123456789012",
        "embedded SK-1234567890abcdefg",
        "access AKIA1234567890ABCDEF12",
        "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.signature1234",
        "endpoint https://owner:password@example.invalid/path",
        "read FILE:///tmp/secret",
        "clone SSH://github.com/owner/repo",
        "clone git://github.com/owner/repo",
        "clone git@github.com:owner/repo",
        "-----BEGIN PRIVATE KEY----- abc",
    ] {
        let mut invalid = project.clone();
        invalid.idea = idea.into();
        assert!(
            invalid.validate().is_err(),
            "accepted secret/path input: {idea}"
        );
    }
    for safe in [
        "Use token-free authorization metadata.",
        "The basic-authentication lane remains disabled.",
        "The bearer-token header must be redacted.",
        "Reject the literal short example sk-example.",
        "Keep the risk-management-requirement test readable.",
        "The client-secret field must always be redacted.",
        "Use the canonical public GitHub repository identity.",
    ] {
        let mut valid = project.clone();
        valid.idea = safe.into();
        valid.validate().unwrap();
    }
    let mut oversized = project.clone();
    oversized.idea = "a".repeat(MAX_BRAINSTORMING_INPUT_BYTES);
    assert!(oversized.validate().is_err());
    assert!(ProjectBrainstormingDraft::decode_frame(&vec![
        b' ';
        MAX_BRAINSTORMING_INPUT_BYTES + 1
    ])
    .is_err());

    let mut feature = feature_draft();
    feature.validate().unwrap();
    feature.expected_repository_revision = 0;
    assert!(feature.validate().is_err());
    feature = feature_draft();
    feature.draft_id = Uuid::nil();
    assert!(feature.validate().is_err());
}

#[test]
fn frozen_specification_rejects_duplicates_drift_zero_and_digest_mismatch() {
    let authoritative_catalog = OrchestratorCatalog::default();
    let frozen = frozen_project();
    frozen.validate().unwrap();
    frozen
        .validate_for_project_draft(&project_draft(), &authoritative_catalog)
        .unwrap();
    let bytes = serde_json::to_vec(&frozen).unwrap();
    assert_eq!(
        FrozenBrainstormingSpecification::decode_frame(&bytes).unwrap(),
        frozen
    );

    let mut duplicate = frozen.clone();
    duplicate
        .specification
        .acceptance_criteria
        .push(duplicate.specification.acceptance_criteria[0].clone());
    assert!(duplicate.validate().is_err());
    let mut duplicate_obligation = frozen.clone();
    duplicate_obligation
        .specification
        .obligations
        .push(duplicate_obligation.specification.obligations[0].clone());
    assert!(duplicate_obligation.validate().is_err());
    let mut wrong_digest = frozen.clone();
    wrong_digest.specification_sha256 = [9; 32];
    assert!(wrong_digest.validate().is_err());
    let mut zero = frozen.clone();
    zero.draft_sha256 = [0; 32];
    assert!(zero.validate().is_err());
    let mut stale_draft = project_draft();
    stale_draft.draft_revision += 1;
    assert!(frozen
        .validate_for_project_draft(&stale_draft, &authoritative_catalog)
        .is_err());
    let mut stale_profile = project_draft();
    stale_profile.orchestrator.configuration_revision += 1;
    assert!(frozen
        .validate_for_project_draft(&stale_profile, &authoritative_catalog)
        .is_err());
    let mut drifted_url = project_draft();
    drifted_url.repository.git_url =
        CanonicalGitHubRepositoryUrl::parse("https://github.com/assemblywright/other").unwrap();
    assert!(frozen
        .validate_for_project_draft(&drifted_url, &authoritative_catalog)
        .is_err());
    let mut drifted_visibility = project_draft();
    drifted_visibility.visibility = ProjectVisibility::Private;
    assert!(frozen
        .validate_for_project_draft(&drifted_visibility, &authoritative_catalog)
        .is_err());
}

#[test]
fn owner_approval_is_digest_bound_to_exact_project_or_feature_shape() {
    let authoritative_catalog = OrchestratorCatalog::default();
    let approval = project_approval();
    let frozen = frozen_project();
    approval.validate().unwrap();
    approval
        .validate_for_project(&project_draft(), &frozen, &authoritative_catalog)
        .unwrap();
    let bytes = serde_json::to_vec(&approval).unwrap();
    assert_eq!(
        BrainstormingOwnerApprovalBinding::decode_frame(&bytes).unwrap(),
        approval
    );

    let mut stale = approval.clone();
    stale.specification_revision += 1;
    stale.owner_approval_sha256 = stale.canonical_approval_sha256().unwrap();
    assert!(stale
        .validate_for_project(&project_draft(), &frozen, &authoritative_catalog)
        .is_err());
    let mut stale_draft = approval.clone();
    stale_draft.draft_revision += 1;
    stale_draft.owner_approval_sha256 = stale_draft.canonical_approval_sha256().unwrap();
    assert!(stale_draft
        .validate_for_project(&project_draft(), &frozen, &authoritative_catalog)
        .is_err());
    let mut wrong_visibility = approval.clone();
    wrong_visibility.visibility = Some(ProjectVisibility::Private);
    wrong_visibility.owner_approval_sha256 = wrong_visibility.canonical_approval_sha256().unwrap();
    wrong_visibility.validate().unwrap();
    assert!(wrong_visibility
        .validate_for_project(&project_draft(), &frozen, &authoritative_catalog)
        .is_err());
    let mut wrong_url = approval.clone();
    wrong_url.repository.git_url =
        CanonicalGitHubRepositoryUrl::parse("https://github.com/assemblywright/other").unwrap();
    wrong_url.owner_approval_sha256 = wrong_url.canonical_approval_sha256().unwrap();
    wrong_url.validate().unwrap();
    assert!(wrong_url
        .validate_for_project(&project_draft(), &frozen, &authoritative_catalog)
        .is_err());

    let mut coordinated_url_frozen = frozen.clone();
    coordinated_url_frozen.repository.git_url =
        CanonicalGitHubRepositoryUrl::parse("https://github.com/assemblywright/other").unwrap();
    coordinated_url_frozen.validate().unwrap();
    let mut coordinated_url_approval = approval.clone();
    coordinated_url_approval.repository = coordinated_url_frozen.repository.clone();
    coordinated_url_approval.owner_approval_sha256 = coordinated_url_approval
        .canonical_approval_sha256()
        .unwrap();
    coordinated_url_approval.validate().unwrap();
    assert!(coordinated_url_approval
        .validate_for_project(
            &project_draft(),
            &coordinated_url_frozen,
            &authoritative_catalog,
        )
        .is_err());

    let mut coordinated_visibility_frozen = frozen.clone();
    coordinated_visibility_frozen.visibility = Some(ProjectVisibility::Private);
    coordinated_visibility_frozen.validate().unwrap();
    let mut coordinated_visibility_approval = approval.clone();
    coordinated_visibility_approval.visibility = coordinated_visibility_frozen.visibility;
    coordinated_visibility_approval.owner_approval_sha256 = coordinated_visibility_approval
        .canonical_approval_sha256()
        .unwrap();
    coordinated_visibility_approval.validate().unwrap();
    assert!(coordinated_visibility_approval
        .validate_for_project(
            &project_draft(),
            &coordinated_visibility_frozen,
            &authoritative_catalog,
        )
        .is_err());
    let mut zero_revision = approval.clone();
    zero_revision.owner_control_revision = 0;
    assert!(zero_revision.validate().is_err());
    let mut zero_digest = approval;
    zero_digest.owner_approval_sha256 = [0; 32];
    assert!(zero_digest.validate().is_err());

    let feature = feature_draft();
    let specification = specification();
    let frozen_feature = FrozenBrainstormingSpecification {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        specification_id: id("66666666-6666-4666-8666-666666666666"),
        specification_revision: 3,
        target_kind: BrainstormingTargetKind::Feature,
        draft_id: feature.draft_id,
        draft_revision: feature.draft_revision,
        draft_sha256: feature.canonical_sha256().unwrap(),
        repository: feature.repository.clone(),
        visibility: None,
        orchestrator_catalog_revision: feature.orchestrator_catalog.catalog_revision,
        orchestrator_catalog_sha256: feature.orchestrator_catalog.catalog_sha256,
        orchestrator_profile_sha256: feature.orchestrator.canonical_sha256().unwrap(),
        specification_sha256: specification.canonical_sha256().unwrap(),
        specification,
    };
    frozen_feature
        .validate_for_feature_draft(&feature, &authoritative_catalog)
        .unwrap();
    let mut feature_url_drift = feature.clone();
    feature_url_drift.repository.git_url =
        CanonicalGitHubRepositoryUrl::parse("https://github.com/assemblywright/other").unwrap();
    assert!(frozen_feature
        .validate_for_feature_draft(&feature_url_drift, &authoritative_catalog)
        .is_err());
    let mut feature_approval = BrainstormingOwnerApprovalBinding {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        approval_id: id("77777777-7777-4777-8777-777777777777"),
        approved_at_ms: 1_800_000_000_001,
        owner_control_revision: 8,
        target_kind: BrainstormingTargetKind::Feature,
        repository: repository(),
        visibility: None,
        expected_repository_revision: Some(4),
        expected_queue_revision: Some(0),
        draft_id: frozen_feature.draft_id,
        draft_revision: frozen_feature.draft_revision,
        draft_sha256: frozen_feature.draft_sha256,
        orchestrator_catalog_revision: frozen_feature.orchestrator_catalog_revision,
        orchestrator_catalog_sha256: frozen_feature.orchestrator_catalog_sha256,
        specification_id: frozen_feature.specification_id,
        specification_revision: frozen_feature.specification_revision,
        specification_sha256: frozen_feature.specification_sha256,
        orchestrator_profile_sha256: frozen_feature.orchestrator_profile_sha256,
        owner_approval_sha256: [0; 32],
    };
    feature_approval.owner_approval_sha256 = feature_approval.canonical_approval_sha256().unwrap();
    feature_approval
        .validate_for_feature(&feature, &frozen_feature, &authoritative_catalog)
        .unwrap();
}

fn start_request() -> AssemblyLineStartRequest {
    let mut request = AssemblyLineStartRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: id("88888888-8888-4888-8888-888888888888"),
        expected_state_revision: 3,
        expected_queue_revision: 5,
        expected_emergency_pause_revision: 0,
        queue_count: 1,
        windows_executor_id: id("cccccccc-cccc-4ccc-8ccc-cccccccccccc"),
        windows_executor_revision: 3,
        mac_executor_id: id("dddddddd-dddd-4ddd-8ddd-dddddddddddd"),
        mac_executor_revision: 6,
        auto_run: true,
        owner_start_approval_sha256: [0; 32],
    };
    request.owner_start_approval_sha256 = request.canonical_owner_start_approval_sha256().unwrap();
    request
}

#[test]
fn assembly_line_defaults_auto_run_and_rejects_empty_start() {
    let mut value = serde_json::to_value(start_request()).unwrap();
    value.as_object_mut().unwrap().remove("auto_run");
    let decoded =
        AssemblyLineStartRequest::decode_frame(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(decoded.auto_run);

    let mut empty = start_request();
    empty.queue_count = 0;
    assert!(empty.validate().is_err());
    let mut too_many = start_request();
    too_many.queue_count = MAX_ASSEMBLY_LINE_QUEUE_COUNT + 1;
    assert!(too_many.validate().is_err());
    let mut zero_revision = start_request();
    zero_revision.expected_queue_revision = 0;
    assert!(zero_revision.validate().is_err());
    let mut zero_digest = start_request();
    zero_digest.owner_start_approval_sha256 = [0; 32];
    assert!(zero_digest.validate().is_err());

    let mut unknown = serde_json::to_value(start_request()).unwrap();
    unknown["owner_token"] = json!("forbidden");
    assert!(
        AssemblyLineStartRequest::decode_frame(&serde_json::to_vec(&unknown).unwrap()).is_err()
    );
    assert!(AssemblyLineStartRequest::decode_frame(&vec![
        b' ';
        MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES
            + 1
    ])
    .is_err());
}

#[test]
fn start_approval_and_session_bind_every_start_field_exactly() {
    let start = start_request();
    start.validate().unwrap();
    let session = session();
    session.validate_for_start(&start).unwrap();

    let mutations: [fn(&mut AssemblyLineStartRequest); 11] = [
        |request| request.schema_version += 1,
        |request| request.request_id = id("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"),
        |request| request.expected_state_revision += 1,
        |request| request.expected_queue_revision += 1,
        |request| request.expected_emergency_pause_revision += 1,
        |request| request.queue_count += 1,
        |request| request.windows_executor_id = id("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"),
        |request| request.windows_executor_revision += 1,
        |request| request.mac_executor_id = id("ffffffff-ffff-4fff-8fff-ffffffffffff"),
        |request| request.mac_executor_revision += 1,
        |request| request.auto_run = !request.auto_run,
    ];
    for mutate in mutations {
        let mut drifted = start.clone();
        mutate(&mut drifted);
        drifted.owner_start_approval_sha256 =
            drifted.canonical_owner_start_approval_sha256().unwrap();
        assert!(session.validate_for_start(&drifted).is_err());
    }

    let mut forged_digest = start;
    forged_digest.owner_start_approval_sha256 = [9; 32];
    assert!(forged_digest.validate().is_err());
    assert!(session.validate_for_start(&forged_digest).is_err());
}

#[test]
fn assembly_line_state_is_strict_and_identity_consistent() {
    let state = AssemblyLineState {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        state_revision: 4,
        queue_revision: 5,
        queue_count: 1,
        auto_run: true,
        lifecycle: AssemblyLineLifecycleState::Running,
        session_id: Some(id("99999999-9999-4999-8999-999999999999")),
        active_child_epoch_id: Some(id("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")),
        active_feature_id: Some(id("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")),
    };
    state.validate().unwrap();
    let mut missing_auto_run = serde_json::to_value(&state).unwrap();
    missing_auto_run.as_object_mut().unwrap().remove("auto_run");
    assert!(
        AssemblyLineState::decode_frame(&serde_json::to_vec(&missing_auto_run).unwrap())
            .unwrap()
            .auto_run
    );

    let mut no_feature = state.clone();
    no_feature.active_feature_id = None;
    assert!(no_feature.validate().is_err());
    let mut empty_running = state.clone();
    empty_running.queue_count = 0;
    assert!(empty_running.validate().is_err());
    let mut zero_queue_revision = state.clone();
    zero_queue_revision.queue_revision = 0;
    assert!(zero_queue_revision.validate().is_err());
    let mut stopped_with_session = state;
    stopped_with_session.lifecycle = AssemblyLineLifecycleState::Stopped;
    assert!(stopped_with_session.validate().is_err());
}

fn session() -> AssemblyLineSessionEpoch {
    let start = start_request();
    AssemblyLineSessionEpoch {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        session_id: id("99999999-9999-4999-8999-999999999999"),
        session_revision: 2,
        start_request_id: start.request_id,
        started_queue_count: start.queue_count,
        state_revision: start.expected_state_revision + 1,
        queue_revision: start.expected_queue_revision,
        emergency_pause_revision: start.expected_emergency_pause_revision,
        owner_start_approval_sha256: start.owner_start_approval_sha256,
        windows_executor_id: start.windows_executor_id,
        windows_executor_revision: start.windows_executor_revision,
        mac_executor_id: start.mac_executor_id,
        mac_executor_revision: start.mac_executor_revision,
        auto_run: start.auto_run,
    }
}

fn child() -> AssemblyLineChildEpoch {
    let session = session();
    AssemblyLineChildEpoch {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        child_epoch_id: id("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
        child_epoch_revision: 1,
        session_id: session.session_id,
        session_revision: session.session_revision,
        feature_id: id("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
        repository_id: repository().repository_id,
        feature_lifecycle_revision: 2,
        queue_revision: session.queue_revision,
        windows_executor_id: session.windows_executor_id,
        windows_executor_revision: session.windows_executor_revision,
        mac_executor_id: session.mac_executor_id,
        mac_executor_revision: session.mac_executor_revision,
    }
}

fn running_state() -> AssemblyLineState {
    let session = session();
    let child = child();
    AssemblyLineState {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        state_revision: session.state_revision,
        queue_revision: session.queue_revision,
        queue_count: session.started_queue_count,
        auto_run: session.auto_run,
        lifecycle: AssemblyLineLifecycleState::Running,
        session_id: Some(session.session_id),
        active_child_epoch_id: Some(child.child_epoch_id),
        active_feature_id: Some(child.feature_id),
    }
}

fn repository_projection() -> RepositoryCreationProjection {
    let frozen = frozen_project();
    RepositoryCreationProjection {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        repository: repository(),
        repository_revision: 1,
        lifecycle_revision: 3,
        visibility: ProjectVisibility::Public,
        approved_specification_id: frozen.specification_id,
        approved_specification_revision: frozen.specification_revision,
        approved_specification_sha256: frozen.specification_sha256,
        owner_approval_sha256: project_approval().owner_approval_sha256,
        lifecycle: RepositoryCreationLifecycle::Created,
        effect_possible: true,
        creation_evidence_sha256: Some([7; 32]),
    }
}

fn queue_entry() -> FeatureQueueEntryProjection {
    FeatureQueueEntryProjection {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        feature_id: child().feature_id,
        repository_id: repository().repository_id,
        specification_id: id("66666666-6666-4666-8666-666666666666"),
        specification_revision: 3,
        specification_sha256: [6; 32],
        owner_approval_sha256: [7; 32],
        position: 1,
        lifecycle_revision: child().feature_lifecycle_revision,
        lifecycle: FeatureQueueLifecycle::Active,
    }
}

fn available_component(seed: u8) -> RuntimeComponentAvailability {
    RuntimeComponentAvailability {
        binding_revision: u64::from(seed),
        binding_sha256: [seed; 32],
        status: RuntimeAvailabilityStatus::Available,
        unavailable_reason: None,
    }
}

fn availability_projection() -> AssemblyLineRuntimeAvailabilityProjection {
    AssemblyLineRuntimeAvailabilityProjection {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        availability_revision: 2,
        observed_at_ms: 1_800_000_000_100,
        brainstorming_provider: available_component(1),
        github_creation: available_component(2),
        windows_executor: available_component(3),
        mac_executor: available_component(4),
        protected_brokers: available_component(5),
    }
}

fn owner_projection() -> AssemblyLineOwnerProjection {
    AssemblyLineOwnerProjection {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        owner_control_revision: 9,
        emergency_pause_revision: 0,
        emergency_paused: false,
        orchestrator_catalog: OrchestratorCatalog::default(),
        repositories: vec![repository_projection()],
        queue: vec![queue_entry()],
        assembly_line: running_state(),
        availability: availability_projection(),
    }
}

fn termination_evidence(outcome: ProcessTerminationOutcome) -> ProcessTerminationEvidenceReference {
    let mut evidence = ProcessTerminationEvidenceReference {
        evidence_id: id("12121212-1212-4212-8212-121212121212"),
        evidence_sha256: [0; 32],
        observed_at_ms: 1_800_000_000_200,
        outcome,
    };
    evidence.evidence_sha256 = evidence.canonical_evidence_sha256().unwrap();
    evidence
}

#[test]
fn repository_creation_projection_is_lifecycle_exact_and_metadata_only() {
    let created = repository_projection();
    created.validate().unwrap();
    assert_eq!(
        RepositoryCreationProjection::decode_frame(&serde_json::to_vec(&created).unwrap()).unwrap(),
        created
    );
    for (lifecycle, effect_possible) in [
        (RepositoryCreationLifecycle::CreationPending, false),
        (RepositoryCreationLifecycle::Conflict, false),
        (RepositoryCreationLifecycle::Failed, false),
        (RepositoryCreationLifecycle::Reconciling, true),
        (RepositoryCreationLifecycle::ReconciliationRequired, true),
    ] {
        let mut projection = created.clone();
        projection.lifecycle = lifecycle;
        projection.effect_possible = effect_possible;
        projection.creation_evidence_sha256 = None;
        projection.validate().unwrap();
    }
    let mut created_without_evidence = created.clone();
    created_without_evidence.creation_evidence_sha256 = None;
    assert!(created_without_evidence.validate().is_err());
    let mut pending_with_evidence = created.clone();
    pending_with_evidence.lifecycle = RepositoryCreationLifecycle::CreationPending;
    pending_with_evidence.effect_possible = false;
    assert!(pending_with_evidence.validate().is_err());
    let mut unknown = serde_json::to_value(&created).unwrap();
    unknown["repository_path"] = json!("C:\\private");
    assert!(
        RepositoryCreationProjection::decode_frame(&serde_json::to_vec(&unknown).unwrap()).is_err()
    );
    let encoded = serde_json::to_string(&created).unwrap();
    for forbidden in [
        "repository_path",
        "credential",
        "owner_token",
        "raw_evidence",
    ] {
        assert!(!encoded.contains(forbidden));
    }
}

#[test]
fn runtime_availability_is_explicit_and_reason_bound() {
    let available = availability_projection();
    available.validate().unwrap();
    AssemblyLineRuntimeAvailabilityProjection::decode_frame(
        &serde_json::to_vec(&available).unwrap(),
    )
    .unwrap();

    let mut unavailable = available;
    unavailable.github_creation.status = RuntimeAvailabilityStatus::Unavailable;
    unavailable.github_creation.unavailable_reason =
        Some(RuntimeUnavailableReason::NotAuthenticated);
    unavailable.validate().unwrap();
    let mut missing_reason = unavailable;
    missing_reason.github_creation.unavailable_reason = None;
    assert!(missing_reason.validate().is_err());
    let mut available_with_reason = availability_projection();
    available_with_reason.windows_executor.unavailable_reason =
        Some(RuntimeUnavailableReason::Disconnected);
    assert!(available_with_reason.validate().is_err());
}

#[test]
fn owner_projection_enforces_fifo_repository_and_active_state_consistency() {
    let owner = owner_projection();
    owner.validate().unwrap();
    let encoded = serde_json::to_vec(&owner).unwrap();
    assert_eq!(
        AssemblyLineOwnerProjection::decode_frame(&encoded).unwrap(),
        owner
    );
    let mut zero_position = owner.clone();
    zero_position.queue[0].position = 0;
    assert!(zero_position.validate().is_err());
    let mut missing_repository = owner.clone();
    missing_repository.queue[0].repository_id = Uuid::new_v4();
    assert!(missing_repository.validate().is_err());
    let mut lifecycle_drift = owner.clone();
    lifecycle_drift.queue[0].lifecycle = FeatureQueueLifecycle::Queued;
    assert!(lifecycle_drift.validate().is_err());
    let mut active_not_at_head = owner.clone();
    let mut queued_head = active_not_at_head.queue[0].clone();
    queued_head.feature_id = id("18181818-1818-4818-8818-181818181818");
    queued_head.position = 1;
    queued_head.lifecycle = FeatureQueueLifecycle::Queued;
    active_not_at_head.queue[0].position = 2;
    active_not_at_head.queue.insert(0, queued_head);
    active_not_at_head.assembly_line.queue_count = 2;
    assert!(active_not_at_head.validate().is_err());
    let mut uncreated_repository = owner.clone();
    uncreated_repository.repositories[0].lifecycle = RepositoryCreationLifecycle::Conflict;
    uncreated_repository.repositories[0].effect_possible = false;
    uncreated_repository.repositories[0].creation_evidence_sha256 = None;
    assert!(uncreated_repository.repositories[0].validate().is_ok());
    assert!(uncreated_repository.validate().is_err());
    let mut duplicate_repository = owner.clone();
    duplicate_repository
        .repositories
        .push(owner.repositories[0].clone());
    assert!(duplicate_repository.validate().is_err());
    let mut unknown: Value = serde_json::from_slice(&encoded).unwrap();
    unknown["raw_evidence"] = json!("forbidden");
    assert!(
        AssemblyLineOwnerProjection::decode_frame(&serde_json::to_vec(&unknown).unwrap()).is_err()
    );
    assert!(AssemblyLineOwnerProjection::decode_frame(&vec![
        b' ';
        MAX_ASSEMBLY_LINE_OWNER_PROJECTION_BYTES
            + 1
    ])
    .is_err());
}

#[test]
fn owner_projection_allows_an_idle_emergency_pause_without_inventing_an_active_session() {
    let mut owner = owner_projection();
    owner.emergency_pause_revision = 1;
    owner.emergency_paused = true;
    owner.assembly_line.lifecycle = AssemblyLineLifecycleState::Stopped;
    owner.assembly_line.session_id = None;
    owner.assembly_line.active_child_epoch_id = None;
    owner.assembly_line.active_feature_id = None;
    owner.queue[0].lifecycle = FeatureQueueLifecycle::Queued;

    owner.validate().unwrap();
    assert_eq!(
        AssemblyLineOwnerProjection::decode_frame(&serde_json::to_vec(&owner).unwrap()).unwrap(),
        owner
    );

    let mut falsely_active = owner;
    falsely_active.assembly_line.active_feature_id = Some(falsely_active.queue[0].feature_id);
    assert!(falsely_active.validate().is_err());
}

#[test]
fn owner_projection_binds_execution_availability_to_safe_lifecycle() {
    for component in ["windows", "mac", "brokers"] {
        let mut owner = owner_projection();
        let unavailable = match component {
            "windows" => &mut owner.availability.windows_executor,
            "mac" => &mut owner.availability.mac_executor,
            _ => &mut owner.availability.protected_brokers,
        };
        unavailable.status = RuntimeAvailabilityStatus::Unavailable;
        unavailable.unavailable_reason = Some(RuntimeUnavailableReason::Disconnected);
        assert!(
            owner.validate().is_err(),
            "{component} cannot remain running"
        );

        owner.assembly_line.lifecycle = AssemblyLineLifecycleState::WaitingForHostReconnect;
        owner.queue[0].lifecycle = FeatureQueueLifecycle::WaitingForHostReconnect;
        owner.validate().unwrap();
    }

    let mut planning_unavailable = owner_projection();
    planning_unavailable
        .availability
        .brainstorming_provider
        .status = RuntimeAvailabilityStatus::Unavailable;
    planning_unavailable
        .availability
        .brainstorming_provider
        .unavailable_reason = Some(RuntimeUnavailableReason::NotAuthenticated);
    planning_unavailable.availability.github_creation.status =
        RuntimeAvailabilityStatus::Unavailable;
    planning_unavailable
        .availability
        .github_creation
        .unavailable_reason = Some(RuntimeUnavailableReason::NotAuthenticated);
    planning_unavailable.validate().unwrap();
}

#[test]
fn termination_evidence_digest_binds_identity_time_and_outcome() {
    let evidence = termination_evidence(ProcessTerminationOutcome::AllTerminated);
    evidence.validate().unwrap();
    for drifted in [
        ProcessTerminationEvidenceReference {
            evidence_id: Uuid::new_v4(),
            ..evidence
        },
        ProcessTerminationEvidenceReference {
            observed_at_ms: evidence.observed_at_ms + 1,
            ..evidence
        },
        ProcessTerminationEvidenceReference {
            outcome: ProcessTerminationOutcome::SurvivorsDetected,
            ..evidence
        },
    ] {
        assert!(drifted.validate().is_err());
    }

    let rebound = termination_evidence(ProcessTerminationOutcome::SurvivorsDetected);
    assert_ne!(evidence.evidence_sha256, rebound.evidence_sha256);
    rebound.validate().unwrap();
}

#[test]
fn start_receipt_binds_request_resulting_state_session_and_child() {
    let request = start_request();
    let mut starting_state = running_state();
    starting_state.lifecycle = AssemblyLineLifecycleState::Starting;
    let receipt = AssemblyLineStartReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: request.request_id,
        owner_start_approval_sha256: request.owner_start_approval_sha256,
        resulting_state: starting_state,
        session: session(),
        child: child(),
    };
    receipt.validate_for_request(&request).unwrap();
    assert_eq!(
        AssemblyLineStartReceipt::decode_frame(&serde_json::to_vec(&receipt).unwrap()).unwrap(),
        receipt
    );
    let mut drifted_child = receipt.clone();
    drifted_child.child.feature_id = Uuid::new_v4();
    assert!(drifted_child.validate().is_err());
    let mut drifted_state = receipt.clone();
    drifted_state.resulting_state.queue_revision += 1;
    assert!(drifted_state.validate().is_err());
    let mut drifted_auto_run = receipt.clone();
    drifted_auto_run.resulting_state.auto_run = !drifted_auto_run.resulting_state.auto_run;
    assert!(drifted_auto_run.validate().is_err());
    assert!(drifted_auto_run.validate_for_request(&request).is_err());
    let mut wrong_request = request;
    wrong_request.request_id = Uuid::new_v4();
    wrong_request.owner_start_approval_sha256 = wrong_request
        .canonical_owner_start_approval_sha256()
        .unwrap();
    assert!(receipt.validate_for_request(&wrong_request).is_err());
}

#[test]
fn control_receipts_require_termination_evidence_and_exact_resulting_state() {
    let session = session();
    let child = child();
    let stop_request = AssemblyLineStopRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: id("13131313-1313-4313-8313-131313131313"),
        session_id: session.session_id,
        expected_state_revision: session.state_revision,
        expected_child_epoch_id: child.child_epoch_id,
    };
    let mut paused_state = running_state();
    paused_state.state_revision += 1;
    paused_state.lifecycle = AssemblyLineLifecycleState::PausedAtCheckpoint;
    let stop_receipt = AssemblyLineStopReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: stop_request.request_id,
        session_id: session.session_id,
        child_epoch_id: child.child_epoch_id,
        checkpoint_id: id("14141414-1414-4414-8414-141414141414"),
        checkpoint_sha256: [14; 32],
        resulting_state: paused_state,
        termination_evidence: termination_evidence(ProcessTerminationOutcome::AllTerminated),
    };
    let prior_state = running_state();
    stop_receipt
        .validate_for_request_and_prior_state(&stop_request, &prior_state)
        .unwrap();
    AssemblyLineStopReceipt::decode_frame(&serde_json::to_vec(&stop_receipt).unwrap()).unwrap();
    let mut unsupported_claim = stop_receipt.clone();
    unsupported_claim.termination_evidence.outcome = ProcessTerminationOutcome::SurvivorsDetected;
    assert!(unsupported_claim.validate().is_err());
    let mut missing_evidence = serde_json::to_value(&stop_receipt).unwrap();
    missing_evidence
        .as_object_mut()
        .unwrap()
        .remove("termination_evidence");
    assert!(
        AssemblyLineStopReceipt::decode_frame(&serde_json::to_vec(&missing_evidence).unwrap())
            .is_err()
    );
    for drift in 0..7 {
        let mut changed = stop_receipt.clone();
        match drift {
            0 => changed.resulting_state.state_revision += 1,
            1 => changed.resulting_state.queue_revision += 1,
            2 => changed.resulting_state.queue_count += 1,
            3 => changed.resulting_state.auto_run = !changed.resulting_state.auto_run,
            4 => changed.resulting_state.session_id = Some(Uuid::new_v4()),
            5 => changed.resulting_state.active_child_epoch_id = Some(Uuid::new_v4()),
            _ => changed.resulting_state.active_feature_id = Some(Uuid::new_v4()),
        }
        assert!(changed
            .validate_for_request_and_prior_state(&stop_request, &prior_state)
            .is_err());
    }

    let pause_request = AssemblyLineEmergencyPauseRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: id("15151515-1515-4515-8515-151515151515"),
        session_id: session.session_id,
        expected_child_epoch_id: child.child_epoch_id,
        expected_state_revision: session.state_revision,
        expected_emergency_pause_revision: session.emergency_pause_revision,
    };
    let mut emergency_state = running_state();
    emergency_state.state_revision += 1;
    emergency_state.lifecycle = AssemblyLineLifecycleState::EmergencyPaused;
    let pause_receipt = AssemblyLineEmergencyPauseReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: pause_request.request_id,
        session_id: session.session_id,
        child_epoch_id: child.child_epoch_id,
        emergency_pause_revision: pause_request.expected_emergency_pause_revision + 1,
        checkpoint_id: id("16161616-1616-4616-8616-161616161616"),
        checkpoint_sha256: [16; 32],
        resulting_state: emergency_state,
        termination_evidence: termination_evidence(ProcessTerminationOutcome::AllTerminated),
    };
    pause_receipt
        .validate_for_request_and_prior_state(&pause_request, &prior_state)
        .unwrap();
    AssemblyLineEmergencyPauseReceipt::decode_frame(&serde_json::to_vec(&pause_receipt).unwrap())
        .unwrap();
    for drift in 0..7 {
        let mut changed = pause_receipt.clone();
        match drift {
            0 => changed.resulting_state.state_revision += 1,
            1 => changed.resulting_state.queue_revision += 1,
            2 => changed.resulting_state.queue_count += 1,
            3 => changed.resulting_state.auto_run = !changed.resulting_state.auto_run,
            4 => changed.resulting_state.session_id = Some(Uuid::new_v4()),
            5 => changed.resulting_state.active_child_epoch_id = Some(Uuid::new_v4()),
            _ => changed.resulting_state.active_feature_id = Some(Uuid::new_v4()),
        }
        assert!(changed
            .validate_for_request_and_prior_state(&pause_request, &prior_state)
            .is_err());
    }

    let auto_request = AssemblyLineAutoRunRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: id("17171717-1717-4717-8717-171717171717"),
        expected_state_revision: session.state_revision,
        auto_run: false,
    };
    let mut auto_state = running_state();
    auto_state.state_revision += 1;
    auto_state.auto_run = false;
    let auto_receipt = AssemblyLineAutoRunReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: auto_request.request_id,
        resulting_state: auto_state,
    };
    auto_receipt
        .validate_for_request_and_prior_state(&auto_request, &prior_state)
        .unwrap();
    let mut stale_auto = auto_receipt.clone();
    stale_auto.resulting_state.auto_run = true;
    assert!(stale_auto
        .validate_for_request_and_prior_state(&auto_request, &prior_state)
        .is_err());
    for drift in 0..7 {
        let mut changed = auto_receipt.clone();
        match drift {
            0 => changed.resulting_state.state_revision += 1,
            1 => changed.resulting_state.queue_revision += 1,
            2 => changed.resulting_state.queue_count += 1,
            3 => changed.resulting_state.lifecycle = AssemblyLineLifecycleState::Stopping,
            4 => changed.resulting_state.session_id = Some(Uuid::new_v4()),
            5 => changed.resulting_state.active_child_epoch_id = Some(Uuid::new_v4()),
            _ => changed.resulting_state.active_feature_id = Some(Uuid::new_v4()),
        }
        assert!(changed
            .validate_for_request_and_prior_state(&auto_request, &prior_state)
            .is_err());
    }
}

#[test]
fn session_child_epochs_and_control_requests_reject_nil_duplicate_stale_and_zero() {
    let session = session();
    session.validate().unwrap();
    let child = child();
    child.validate_for_session(&session).unwrap();

    let mut duplicate_host = session.clone();
    duplicate_host.mac_executor_id = duplicate_host.windows_executor_id;
    assert!(duplicate_host.validate().is_err());
    let mut nil_child = child.clone();
    nil_child.child_epoch_id = Uuid::nil();
    assert!(nil_child.validate().is_err());
    let mut zero_revision = child.clone();
    zero_revision.feature_lifecycle_revision = 0;
    assert!(zero_revision.validate().is_err());
    let mut stale_session = session.clone();
    stale_session.session_revision += 1;
    assert!(child.validate_for_session(&stale_session).is_err());
    let mut stale_executor = session.clone();
    stale_executor.mac_executor_revision += 1;
    assert!(child.validate_for_session(&stale_executor).is_err());

    let stop = AssemblyLineStopRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        session_id: session.session_id,
        expected_state_revision: session.state_revision,
        expected_child_epoch_id: child.child_epoch_id,
    };
    stop.validate().unwrap();
    AssemblyLineStopRequest::decode_frame(&serde_json::to_vec(&stop).unwrap()).unwrap();
    let pause = AssemblyLineEmergencyPauseRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        session_id: session.session_id,
        expected_child_epoch_id: child.child_epoch_id,
        expected_state_revision: session.state_revision,
        expected_emergency_pause_revision: session.emergency_pause_revision,
    };
    pause.validate().unwrap();
    let toggle = AssemblyLineAutoRunRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        expected_state_revision: session.state_revision,
        auto_run: false,
    };
    toggle.validate().unwrap();

    let duplicate_json = serde_json::to_string(&child).unwrap().replacen(
        "\"session_revision\":2",
        "\"session_revision\":2,\"session_revision\":3",
        1,
    );
    assert!(AssemblyLineChildEpoch::decode_frame(duplicate_json.as_bytes()).is_err());
}
