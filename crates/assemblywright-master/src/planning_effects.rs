use crate::{
    advance_assembly_line_owner_revision_tx, append_assembly_line_audit_tx,
    assembly_line_repository_projection_tx, canonical_json, current_time_ms,
    project_visibility_str, u64_to_i64, MasterError, MasterKernel,
};
use assemblywright_protocol::{
    AssemblyLineRepositoryIdentity, BrainstormingOwnerApprovalBinding,
    BrainstormingSpecificationDocument, BrainstormingTargetKind, FeatureBrainstormingDraft,
    FrozenBrainstormingSpecification, OrchestratorProfile, ProjectBrainstormingDraft,
    ProjectVisibility, RepositoryCreationLifecycle, RepositoryCreationProjection,
    FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

#[derive(Clone)]
pub struct PlanningEffectControl {
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    authority_current: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    #[cfg(test)]
    cancel_on_poll: Option<(Arc<AtomicUsize>, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrainstormingCloudAuthorization {
    pub owner_cloud_disclosure_sha256: [u8; 32],
}

impl BrainstormingCloudAuthorization {
    fn validate(&self) -> Result<(), MasterError> {
        if self.owner_cloud_disclosure_sha256 == [0; 32] {
            return Err(MasterError::InvalidAssemblyLinePlanningInput(
                "owner cloud disclosure digest is invalid".to_string(),
            ));
        }
        Ok(())
    }
}

impl PlanningEffectControl {
    pub fn new(cancelled: Arc<AtomicBool>, deadline: Instant) -> Self {
        Self {
            cancelled,
            deadline,
            authority_current: None,
            #[cfg(test)]
            cancel_on_poll: None,
        }
    }

    #[cfg(test)]
    fn cancel_on_poll(deadline: Instant, poll_number: usize) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline,
            authority_current: None,
            cancel_on_poll: Some((Arc::new(AtomicUsize::new(0)), poll_number)),
        }
    }

    pub fn poll(&self) -> bool {
        #[cfg(test)]
        if let Some((count, cancel_on)) = &self.cancel_on_poll {
            if count.fetch_add(1, Ordering::AcqRel) + 1 == *cancel_on {
                self.cancelled.store(true, Ordering::Release);
            }
        }
        !self.cancelled.load(Ordering::Acquire)
            && Instant::now() < self.deadline
            && self
                .authority_current
                .as_ref()
                .is_none_or(|authority_current| authority_current())
    }

    pub fn with_authority(
        mut self,
        authority_current: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        self.authority_current = Some(authority_current);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrainstormingAdapterBinding {
    pub profile: OrchestratorProfile,
    pub executable_sha256: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanningEffectAdapterCatalog {
    catalog_revision: u64,
    brainstorming: Vec<BrainstormingAdapterBinding>,
    github_creation: Vec<[u8; 32]>,
    catalog_sha256: [u8; 32],
}

impl PlanningEffectAdapterCatalog {
    fn new(
        catalog_revision: u64,
        mut brainstorming: Vec<BrainstormingAdapterBinding>,
        mut github_creation: Vec<[u8; 32]>,
    ) -> Result<Self, MasterError> {
        brainstorming.sort_by(|left, right| {
            left.profile
                .provider_id
                .cmp(&right.profile.provider_id)
                .then(left.profile.model_id.cmp(&right.profile.model_id))
                .then(left.executable_sha256.cmp(&right.executable_sha256))
        });
        github_creation.sort();
        let mut catalog = Self {
            catalog_revision,
            brainstorming,
            github_creation,
            catalog_sha256: [0; 32],
        };
        catalog.catalog_sha256 = catalog.compute_sha256()?;
        catalog.validate()?;
        Ok(catalog)
    }

    fn compute_sha256(&self) -> Result<[u8; 32], MasterError> {
        let json = canonical_json(&serde_json::json!({
            "catalog_revision": self.catalog_revision,
            "brainstorming": self.brainstorming,
            "github_creation": self.github_creation,
        }))?;
        Ok(Sha256::digest(json.as_bytes()).into())
    }

    fn validate(&self) -> Result<(), MasterError> {
        if self.catalog_revision == 0
            || self.brainstorming.is_empty()
            || self.github_creation.is_empty()
            || self.catalog_sha256 == [0; 32]
            || self.compute_sha256()? != self.catalog_sha256
        {
            return Err(MasterError::InvalidAssemblyLinePlanningInput(
                "planning-effect adapter catalog is invalid".to_string(),
            ));
        }
        let mut prior_brainstorming = None;
        for binding in &self.brainstorming {
            binding.profile.validate()?;
            if binding.executable_sha256 == [0; 32] {
                return Err(MasterError::InvalidAssemblyLinePlanningInput(
                    "brainstorming adapter binding is invalid".to_string(),
                ));
            }
            let identity = (
                binding.profile.provider_id.as_str(),
                binding.profile.model_id.as_str(),
                binding.executable_sha256,
            );
            if prior_brainstorming.is_some_and(|prior| prior >= identity) {
                return Err(MasterError::InvalidAssemblyLinePlanningInput(
                    "brainstorming adapter catalog is not unique and ordered".to_string(),
                ));
            }
            prior_brainstorming = Some(identity);
        }
        if self.github_creation.contains(&[0; 32])
            || self
                .github_creation
                .windows(2)
                .any(|window| window[0] >= window[1])
        {
            return Err(MasterError::InvalidAssemblyLinePlanningInput(
                "GitHub adapter catalog is not unique and ordered".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_brainstorming_binding(
        &self,
        binding: &BrainstormingAdapterBinding,
    ) -> Result<(), MasterError> {
        self.validate()?;
        if self
            .brainstorming
            .iter()
            .filter(|configured| *configured == binding)
            .count()
            != 1
        {
            return Err(MasterError::AssemblyLineBrainstormingUnavailable);
        }
        Ok(())
    }

    fn validate_github_binding(&self, binding_sha256: [u8; 32]) -> Result<(), MasterError> {
        self.validate()?;
        if self
            .github_creation
            .iter()
            .filter(|configured| **configured == binding_sha256)
            .count()
            != 1
        {
            return Err(MasterError::AssemblyLineGithubCreationUnavailable);
        }
        Ok(())
    }
}

/// Opaque capability minted only after Windows-owned configuration has been
/// authenticated and converted to an exact adapter catalog. This slice has no
/// production minting path, so runtime planning effects remain unavailable.
pub struct WindowsPlanningEffectAuthority {
    catalog: PlanningEffectAdapterCatalog,
}

impl WindowsPlanningEffectAuthority {
    #[cfg(test)]
    fn for_test(catalog: PlanningEffectAdapterCatalog) -> Self {
        Self { catalog }
    }

    pub(crate) fn from_loaded_bindings(
        catalog_revision: u64,
        brainstorming: BrainstormingAdapterBinding,
        github_creation: [u8; 32],
    ) -> Result<Self, MasterError> {
        Ok(Self {
            catalog: PlanningEffectAdapterCatalog::new(
                catalog_revision,
                vec![brainstorming],
                vec![github_creation],
            )?,
        })
    }

    pub(crate) fn catalog_sha256(&self) -> [u8; 32] {
        self.catalog.catalog_sha256
    }

    pub(crate) fn require_feature_enqueue_provenance(
        &self,
        kernel: &MasterKernel,
        approval: &BrainstormingOwnerApprovalBinding,
    ) -> Result<(), MasterError> {
        self.catalog.validate()?;
        let frozen = kernel
            .assembly_line_frozen_specification_for_draft(
                BrainstormingTargetKind::Feature,
                approval.draft_id,
            )?
            .ok_or(MasterError::AssemblyLineBrainstormingUnavailable)?;
        if frozen.specification_id != approval.specification_id
            || frozen.specification_sha256 != approval.specification_sha256
        {
            return Err(MasterError::AssemblyLineBrainstormingUnavailable);
        }
        let binding = self
            .catalog
            .brainstorming
            .iter()
            .find(|binding| {
                binding.profile.canonical_sha256().ok() == Some(frozen.orchestrator_profile_sha256)
            })
            .ok_or(MasterError::AssemblyLineBrainstormingUnavailable)?;
        kernel.require_assembly_line_provider_accepted_specification(
            &frozen,
            binding,
            self.catalog.catalog_sha256,
        )
    }

    pub(crate) fn approve_feature_and_enqueue(
        &self,
        kernel: &mut MasterKernel,
        approval: &BrainstormingOwnerApprovalBinding,
        now_ms: u64,
    ) -> Result<assemblywright_protocol::FeatureQueueEntryProjection, MasterError> {
        self.catalog.validate()?;
        let frozen = kernel
            .assembly_line_frozen_specification_for_draft(
                BrainstormingTargetKind::Feature,
                approval.draft_id,
            )?
            .ok_or(MasterError::AssemblyLineBrainstormingUnavailable)?;
        let binding = self
            .catalog
            .brainstorming
            .iter()
            .find(|binding| {
                binding.profile.canonical_sha256().ok() == Some(frozen.orchestrator_profile_sha256)
            })
            .ok_or(MasterError::AssemblyLineBrainstormingUnavailable)?;
        kernel.approve_assembly_line_feature_and_enqueue_with_provider(
            approval,
            binding,
            self.catalog.catalog_sha256,
            now_ms,
        )
    }
}

impl BrainstormingAdapterBinding {
    fn validate_for(&self, expected: &OrchestratorProfile) -> Result<(), MasterError> {
        expected.validate()?;
        self.profile.validate()?;
        if &self.profile != expected || self.executable_sha256 == [0; 32] {
            return Err(MasterError::AssemblyLineBrainstormingUnavailable);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "target_kind", content = "draft")]
pub enum BrainstormingDraft {
    Project(ProjectBrainstormingDraft),
    Feature(FeatureBrainstormingDraft),
}

impl BrainstormingDraft {
    fn canonical_sha256(&self) -> Result<[u8; 32], MasterError> {
        match self {
            Self::Project(draft) => Ok(draft.canonical_sha256()?),
            Self::Feature(draft) => Ok(draft.canonical_sha256()?),
        }
    }

    fn draft_id(&self) -> Uuid {
        match self {
            Self::Project(draft) => draft.draft_id,
            Self::Feature(draft) => draft.draft_id,
        }
    }

    fn target_kind(&self) -> BrainstormingTargetKind {
        match self {
            Self::Project(_) => BrainstormingTargetKind::Project,
            Self::Feature(_) => BrainstormingTargetKind::Feature,
        }
    }

    fn orchestrator(&self) -> &OrchestratorProfile {
        match self {
            Self::Project(draft) => &draft.orchestrator,
            Self::Feature(draft) => &draft.orchestrator,
        }
    }

    fn record(&self, kernel: &mut MasterKernel, now_ms: u64) -> Result<(), MasterError> {
        match self {
            Self::Project(draft) => kernel.record_assembly_line_project_draft(draft, now_ms),
            Self::Feature(draft) => kernel.record_assembly_line_feature_draft(draft, now_ms),
        }
    }

    fn frozen(
        &self,
        specification: BrainstormingSpecificationDocument,
    ) -> Result<FrozenBrainstormingSpecification, MasterError> {
        specification.validate()?;
        let specification_sha256 = specification.canonical_sha256()?;
        let (
            draft_id,
            draft_revision,
            draft_sha256,
            repository,
            visibility,
            catalog_revision,
            catalog_sha256,
            profile_sha256,
        ) = match self {
            Self::Project(draft) => (
                draft.draft_id,
                draft.draft_revision,
                draft.canonical_sha256()?,
                draft.repository.clone(),
                Some(draft.visibility),
                draft.orchestrator_catalog.catalog_revision,
                draft.orchestrator_catalog.catalog_sha256,
                draft.orchestrator.canonical_sha256()?,
            ),
            Self::Feature(draft) => (
                draft.draft_id,
                draft.draft_revision,
                draft.canonical_sha256()?,
                draft.repository.clone(),
                None,
                draft.orchestrator_catalog.catalog_revision,
                draft.orchestrator_catalog.catalog_sha256,
                draft.orchestrator.canonical_sha256()?,
            ),
        };
        let mut identity_material = Vec::with_capacity(96);
        identity_material.extend_from_slice(b"assemblywright.brainstorming-specification.v1\0");
        identity_material.extend_from_slice(draft_id.as_bytes());
        identity_material.extend_from_slice(&draft_sha256);
        identity_material.extend_from_slice(&specification_sha256);
        let identity_sha256: [u8; 32] = Sha256::digest(identity_material).into();
        let mut identity = [0_u8; 16];
        identity.copy_from_slice(&identity_sha256[..16]);
        identity[6] = (identity[6] & 0x0f) | 0x40;
        identity[8] = (identity[8] & 0x3f) | 0x80;
        let frozen = FrozenBrainstormingSpecification {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            specification_id: Uuid::from_bytes(identity),
            specification_revision: 1,
            target_kind: self.target_kind(),
            draft_id,
            draft_revision,
            draft_sha256,
            repository,
            visibility,
            orchestrator_catalog_revision: catalog_revision,
            orchestrator_catalog_sha256: catalog_sha256,
            orchestrator_profile_sha256: profile_sha256,
            specification,
            specification_sha256,
        };
        frozen.validate()?;
        Ok(frozen)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrainstormingAdapterError {
    Unavailable,
    Rejected,
    Cancelled,
    Timeout,
    MalformedOutput,
}

pub trait BrainstormingAdapter: Send {
    fn binding(&self) -> Option<BrainstormingAdapterBinding>;

    fn generate(
        &mut self,
        draft: &BrainstormingDraft,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<BrainstormingSpecificationDocument, BrainstormingAdapterError>;

    fn generate_authorized(
        &mut self,
        draft: &BrainstormingDraft,
        authorization: &BrainstormingCloudAuthorization,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<BrainstormingSpecificationDocument, BrainstormingAdapterError> {
        let _ = authorization;
        self.generate(draft, idempotency_key, control)
    }

    fn reconcile(
        &mut self,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<Option<BrainstormingSpecificationDocument>, BrainstormingAdapterError>;

    fn reconcile_authorized(
        &mut self,
        authorization: &BrainstormingCloudAuthorization,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<Option<BrainstormingSpecificationDocument>, BrainstormingAdapterError> {
        let _ = authorization;
        self.reconcile(idempotency_key, control)
    }
}

pub fn run_brainstorming<A: BrainstormingAdapter + ?Sized>(
    kernel: &mut MasterKernel,
    draft: BrainstormingDraft,
    adapter: &mut A,
    authority: &WindowsPlanningEffectAuthority,
    control: &PlanningEffectControl,
) -> Result<FrozenBrainstormingSpecification, MasterError> {
    run_brainstorming_inner(kernel, draft, None, adapter, authority, control)
}

pub fn run_brainstorming_authorized<A: BrainstormingAdapter + ?Sized>(
    kernel: &mut MasterKernel,
    draft: BrainstormingDraft,
    authorization: BrainstormingCloudAuthorization,
    adapter: &mut A,
    authority: &WindowsPlanningEffectAuthority,
    control: &PlanningEffectControl,
) -> Result<FrozenBrainstormingSpecification, MasterError> {
    authorization.validate()?;
    run_brainstorming_inner(
        kernel,
        draft,
        Some(authorization),
        adapter,
        authority,
        control,
    )
}

fn run_brainstorming_inner<A: BrainstormingAdapter + ?Sized>(
    kernel: &mut MasterKernel,
    draft: BrainstormingDraft,
    authorization: Option<BrainstormingCloudAuthorization>,
    adapter: &mut A,
    authority: &WindowsPlanningEffectAuthority,
    control: &PlanningEffectControl,
) -> Result<FrozenBrainstormingSpecification, MasterError> {
    let now_ms = current_time_ms()?;
    draft.record(kernel, now_ms)?;
    let binding = adapter
        .binding()
        .ok_or(MasterError::AssemblyLineBrainstormingUnavailable)?;
    binding.validate_for(draft.orchestrator())?;
    authority.catalog.validate_brainstorming_binding(&binding)?;
    if let Some(existing) = kernel
        .assembly_line_frozen_specification_for_draft(draft.target_kind(), draft.draft_id())?
    {
        kernel.require_assembly_line_provider_accepted_specification(
            &existing,
            &binding,
            authority.catalog.catalog_sha256,
        )?;
        return Ok(existing);
    }
    if !control.poll() {
        return Err(MasterError::AssemblyLineBrainstormingUnavailable);
    }
    let (idempotency_key, existing_intent) =
        kernel.prepare_assembly_line_brainstorming_intent(BrainstormingIntentParameters {
            target_kind: draft.target_kind(),
            draft_id: draft.draft_id(),
            draft_sha256: draft.canonical_sha256()?,
            binding: &binding,
            adapter_catalog_sha256: authority.catalog.catalog_sha256,
            authorization: authorization.as_ref(),
            now_ms,
        })?;

    let specification_result = if existing_intent {
        if !control.poll() {
            return Err(MasterError::AssemblyLineBrainstormingUnavailable);
        }
        let result = match authorization.as_ref() {
            Some(authorization) => {
                adapter.reconcile_authorized(authorization, idempotency_key, control)
            }
            None => adapter.reconcile(idempotency_key, control),
        };
        if !control.poll() {
            kernel.record_assembly_line_brainstorming_failure(
                draft.target_kind(),
                draft.draft_id(),
                BrainstormingAdapterError::Cancelled,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineBrainstormingUnavailable);
        }
        match result {
            Ok(Some(specification)) => Ok(specification),
            Ok(None) => Err(BrainstormingAdapterError::Unavailable),
            Err(error) => Err(error),
        }
    } else {
        if !control.poll() {
            return Err(MasterError::AssemblyLineBrainstormingUnavailable);
        }
        let result = match authorization.as_ref() {
            Some(authorization) => {
                adapter.generate_authorized(&draft, authorization, idempotency_key, control)
            }
            None => adapter.generate(&draft, idempotency_key, control),
        };
        if !control.poll() {
            kernel.record_assembly_line_brainstorming_failure(
                draft.target_kind(),
                draft.draft_id(),
                BrainstormingAdapterError::Cancelled,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineBrainstormingUnavailable);
        }
        result
    };

    let specification = match specification_result {
        Ok(specification) => specification,
        Err(error) => {
            if !existing_intent && control.poll() {
                let reconciled = match authorization.as_ref() {
                    Some(authorization) => {
                        adapter.reconcile_authorized(authorization, idempotency_key, control)
                    }
                    None => adapter.reconcile(idempotency_key, control),
                };
                if !control.poll() {
                    kernel.record_assembly_line_brainstorming_failure(
                        draft.target_kind(),
                        draft.draft_id(),
                        BrainstormingAdapterError::Cancelled,
                        current_time_ms()?,
                    )?;
                    return Err(MasterError::AssemblyLineBrainstormingUnavailable);
                }
                if let Ok(Some(specification)) = reconciled {
                    specification
                } else {
                    kernel.record_assembly_line_brainstorming_failure(
                        draft.target_kind(),
                        draft.draft_id(),
                        error,
                        current_time_ms()?,
                    )?;
                    return Err(match error {
                        BrainstormingAdapterError::Rejected => {
                            MasterError::AssemblyLineBrainstormingRejected
                        }
                        _ => MasterError::AssemblyLineBrainstormingUnavailable,
                    });
                }
            } else {
                kernel.record_assembly_line_brainstorming_failure(
                    draft.target_kind(),
                    draft.draft_id(),
                    error,
                    current_time_ms()?,
                )?;
                return Err(match error {
                    BrainstormingAdapterError::Rejected => {
                        MasterError::AssemblyLineBrainstormingRejected
                    }
                    _ => MasterError::AssemblyLineBrainstormingUnavailable,
                });
            }
        }
    };
    if !control.poll() {
        kernel.record_assembly_line_brainstorming_failure(
            draft.target_kind(),
            draft.draft_id(),
            BrainstormingAdapterError::Cancelled,
            current_time_ms()?,
        )?;
        return Err(MasterError::AssemblyLineBrainstormingUnavailable);
    }
    let frozen = match draft.frozen(specification) {
        Ok(frozen) => frozen,
        Err(_) => {
            kernel.record_assembly_line_brainstorming_failure(
                draft.target_kind(),
                draft.draft_id(),
                BrainstormingAdapterError::MalformedOutput,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineBrainstormingRejected);
        }
    };
    kernel.record_assembly_line_frozen_specification_with_provider(
        &frozen,
        &binding,
        authority.catalog.catalog_sha256,
        current_time_ms()?,
    )?;
    Ok(frozen)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubRepositoryObservation {
    pub repository: AssemblyLineRepositoryIdentity,
    pub visibility: ProjectVisibility,
    pub default_branch: String,
    pub initialized: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrainstormingStartedAudit {
    target_kind: String,
    draft_id: Uuid,
    draft_sha256: [u8; 32],
    provider_id: String,
    model_id: String,
    adapter_sha256: [u8; 32],
    adapter_catalog_sha256: [u8; 32],
    idempotency_key_sha256: [u8; 32],
    information_classification: Option<String>,
    owner_cloud_disclosure_sha256: Option<[u8; 32]>,
    planning_only: bool,
    provider_output_retained_in_audit: bool,
    external_effect_possible: bool,
}

struct BrainstormingIntentParameters<'a> {
    target_kind: BrainstormingTargetKind,
    draft_id: Uuid,
    draft_sha256: [u8; 32],
    binding: &'a BrainstormingAdapterBinding,
    adapter_catalog_sha256: [u8; 32],
    authorization: Option<&'a BrainstormingCloudAuthorization>,
    now_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrainstormingAcceptedAudit {
    pub(crate) target_kind: String,
    pub(crate) draft_id: Uuid,
    pub(crate) specification_id: Uuid,
    pub(crate) specification_sha256: [u8; 32],
    pub(crate) provider_id: String,
    pub(crate) model_id: String,
    pub(crate) adapter_sha256: [u8; 32],
    pub(crate) adapter_catalog_sha256: [u8; 32],
    pub(crate) planning_only: bool,
    pub(crate) provider_output_retained_in_audit: bool,
    pub(crate) external_effect_authorized: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GithubInspectionStartedAudit {
    repository_id: Uuid,
    owner_approval_sha256: [u8; 32],
    adapter_sha256: [u8; 32],
    adapter_catalog_sha256: [u8; 32],
    idempotency_key_sha256: [u8; 32],
    visibility: String,
    effect_possible: bool,
    external_read_possible: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GithubCreationStartedAudit {
    repository_id: Uuid,
    owner_approval_sha256: [u8; 32],
    adapter_sha256: [u8; 32],
    adapter_catalog_sha256: [u8; 32],
    idempotency_key_sha256: [u8; 32],
    visibility: String,
    effect_possible: bool,
    automatic_retry_authorized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GithubCreationIntentBinding {
    adapter_sha256: [u8; 32],
    adapter_catalog_sha256: [u8; 32],
    idempotency_key_sha256: [u8; 32],
    owner_approval_sha256: [u8; 32],
    visibility: ProjectVisibility,
}

impl GithubRepositoryObservation {
    fn matches(&self, plan: &RepositoryCreationProjection) -> bool {
        self.repository == plan.repository
            && self.visibility == plan.visibility
            && self.default_branch == "main"
            && self.initialized
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GithubRepositoryCreationError {
    Unavailable,
    Rejected,
    Cancelled,
    Ambiguous,
    MalformedOutput,
}

pub trait GithubRepositoryCreationAdapter: Send {
    fn binding_sha256(&self) -> Option<[u8; 32]>;

    fn inspect(
        &mut self,
        repository: &AssemblyLineRepositoryIdentity,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<Option<GithubRepositoryObservation>, GithubRepositoryCreationError>;

    fn create(
        &mut self,
        plan: &RepositoryCreationProjection,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<GithubRepositoryObservation, GithubRepositoryCreationError>;

    fn reconcile_creation(
        &mut self,
        plan: &RepositoryCreationProjection,
        idempotency_key: [u8; 32],
        control: &PlanningEffectControl,
    ) -> Result<Option<GithubRepositoryObservation>, GithubRepositoryCreationError> {
        self.inspect(&plan.repository, idempotency_key, control)
    }
}

fn github_effect_idempotency_key(
    operation: &str,
    plan: &RepositoryCreationProjection,
    adapter_sha256: [u8; 32],
    adapter_catalog_sha256: [u8; 32],
) -> Result<[u8; 32], MasterError> {
    let git_url_sha256: [u8; 32] = Sha256::digest(plan.repository.git_url.url.as_bytes()).into();
    Ok(Sha256::digest(canonical_json(&serde_json::json!({
        "schema_version": 1,
        "operation": operation,
        "repository_id": plan.repository.repository_id,
        "git_url_sha256": git_url_sha256,
        "visibility": project_visibility_str(plan.visibility),
        "owner_approval_sha256": plan.owner_approval_sha256,
        "adapter_sha256": adapter_sha256,
        "adapter_catalog_sha256": adapter_catalog_sha256
    }))?)
    .into())
}

pub fn run_github_repository_creation<A: GithubRepositoryCreationAdapter + ?Sized>(
    kernel: &mut MasterKernel,
    repository_id: Uuid,
    adapter: &mut A,
    authority: &WindowsPlanningEffectAuthority,
    control: &PlanningEffectControl,
) -> Result<RepositoryCreationProjection, MasterError> {
    let binding_sha256 = adapter
        .binding_sha256()
        .filter(|digest| *digest != [0; 32])
        .ok_or(MasterError::AssemblyLineGithubCreationUnavailable)?;
    authority.catalog.validate_github_binding(binding_sha256)?;
    let plan = kernel.assembly_line_repository_creation_projection(repository_id)?;
    kernel
        .require_assembly_line_repository_provider_provenance(repository_id, &authority.catalog)?;
    if plan.lifecycle == RepositoryCreationLifecycle::Created {
        return Ok(plan);
    }
    if matches!(
        plan.lifecycle,
        RepositoryCreationLifecycle::Conflict | RepositoryCreationLifecycle::Failed
    ) {
        return Err(MasterError::AssemblyLineGithubCreationConflict);
    }
    let creation_idempotency_key = github_effect_idempotency_key(
        "create",
        &plan,
        binding_sha256,
        authority.catalog.catalog_sha256,
    )?;
    let inspection_idempotency_key = github_effect_idempotency_key(
        "inspect",
        &plan,
        binding_sha256,
        authority.catalog.catalog_sha256,
    )?;
    if matches!(
        plan.lifecycle,
        RepositoryCreationLifecycle::Reconciling
            | RepositoryCreationLifecycle::ReconciliationRequired
    ) && kernel.assembly_line_repository_creation_adapter_binding(repository_id)?
        != Some(GithubCreationIntentBinding {
            adapter_sha256: binding_sha256,
            adapter_catalog_sha256: authority.catalog.catalog_sha256,
            idempotency_key_sha256: creation_idempotency_key,
            owner_approval_sha256: plan.owner_approval_sha256,
            visibility: plan.visibility,
        })
    {
        return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
    }
    if !control.poll() {
        if plan.lifecycle == RepositoryCreationLifecycle::Reconciling {
            kernel.mark_assembly_line_repository_reconciliation_required(
                repository_id,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        return Err(MasterError::AssemblyLineGithubCreationUnavailable);
    }
    kernel.prepare_assembly_line_github_inspection_intent(
        &plan,
        binding_sha256,
        authority.catalog.catalog_sha256,
        inspection_idempotency_key,
        current_time_ms()?,
    )?;
    if !control.poll() {
        if matches!(
            plan.lifecycle,
            RepositoryCreationLifecycle::Reconciling
                | RepositoryCreationLifecycle::ReconciliationRequired
        ) {
            kernel.mark_assembly_line_repository_reconciliation_required(
                repository_id,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        return Err(MasterError::AssemblyLineGithubCreationUnavailable);
    }

    let inspection = if plan.lifecycle == RepositoryCreationLifecycle::CreationPending {
        adapter.inspect(&plan.repository, inspection_idempotency_key, control)
    } else {
        adapter.reconcile_creation(&plan, creation_idempotency_key, control)
    };
    if !control.poll() {
        if matches!(
            plan.lifecycle,
            RepositoryCreationLifecycle::Reconciling
                | RepositoryCreationLifecycle::ReconciliationRequired
        ) {
            kernel.mark_assembly_line_repository_reconciliation_required(
                repository_id,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        return Err(MasterError::AssemblyLineGithubCreationUnavailable);
    }
    let observed = match inspection {
        Ok(observed) => observed,
        Err(_) if plan.lifecycle == RepositoryCreationLifecycle::Reconciling => {
            kernel.mark_assembly_line_repository_reconciliation_required(
                repository_id,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        Err(_) if plan.lifecycle == RepositoryCreationLifecycle::ReconciliationRequired => {
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        Err(GithubRepositoryCreationError::Unavailable)
        | Err(GithubRepositoryCreationError::Rejected)
        | Err(GithubRepositoryCreationError::Cancelled)
        | Err(GithubRepositoryCreationError::Ambiguous)
        | Err(GithubRepositoryCreationError::MalformedOutput) => {
            return Err(MasterError::AssemblyLineGithubCreationUnavailable);
        }
    };
    if plan.lifecycle == RepositoryCreationLifecycle::CreationPending {
        if observed.is_some() {
            kernel.mark_assembly_line_repository_creation_conflict(
                repository_id,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineGithubCreationConflict);
        }
        kernel.begin_assembly_line_repository_creation(
            repository_id,
            binding_sha256,
            authority.catalog.catalog_sha256,
            creation_idempotency_key,
            current_time_ms()?,
        )?;
        if !control.poll() {
            kernel.mark_assembly_line_repository_reconciliation_required(
                repository_id,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        let creation = adapter.create(&plan, creation_idempotency_key, control);
        if !control.poll() {
            kernel.mark_assembly_line_repository_reconciliation_required(
                repository_id,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        match creation {
            Ok(observation) if observation.matches(&plan) => {
                if !control.poll() {
                    kernel.mark_assembly_line_repository_reconciliation_required(
                        repository_id,
                        current_time_ms()?,
                    )?;
                    return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
                }
                return kernel.complete_assembly_line_repository_creation(
                    repository_id,
                    &observation,
                    binding_sha256,
                    authority.catalog.catalog_sha256,
                    creation_idempotency_key,
                    current_time_ms()?,
                );
            }
            Ok(_) | Err(GithubRepositoryCreationError::MalformedOutput) => {
                kernel.mark_assembly_line_repository_reconciliation_required(
                    repository_id,
                    current_time_ms()?,
                )?;
                return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
            }
            Err(GithubRepositoryCreationError::Unavailable)
            | Err(GithubRepositoryCreationError::Rejected)
            | Err(GithubRepositoryCreationError::Cancelled)
            | Err(GithubRepositoryCreationError::Ambiguous) => {}
        }

        if !control.poll() {
            kernel.mark_assembly_line_repository_reconciliation_required(
                repository_id,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        let reconciliation = adapter.inspect(&plan.repository, inspection_idempotency_key, control);
        if !control.poll() {
            kernel.mark_assembly_line_repository_reconciliation_required(
                repository_id,
                current_time_ms()?,
            )?;
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        match reconciliation {
            Ok(Some(observation)) if observation.matches(&plan) => {
                if !control.poll() {
                    kernel.mark_assembly_line_repository_reconciliation_required(
                        repository_id,
                        current_time_ms()?,
                    )?;
                    return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
                }
                return kernel.complete_assembly_line_repository_creation(
                    repository_id,
                    &observation,
                    binding_sha256,
                    authority.catalog.catalog_sha256,
                    creation_idempotency_key,
                    current_time_ms()?,
                );
            }
            _ => {
                kernel.mark_assembly_line_repository_reconciliation_required(
                    repository_id,
                    current_time_ms()?,
                )?;
                return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
            }
        }
    }

    match observed {
        Some(observation) if observation.matches(&plan) => {
            if !control.poll() {
                kernel.mark_assembly_line_repository_reconciliation_required(
                    repository_id,
                    current_time_ms()?,
                )?;
                return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
            }
            kernel.complete_assembly_line_repository_creation(
                repository_id,
                &observation,
                binding_sha256,
                authority.catalog.catalog_sha256,
                creation_idempotency_key,
                current_time_ms()?,
            )
        }
        _ if plan.lifecycle == RepositoryCreationLifecycle::Reconciling => {
            kernel.mark_assembly_line_repository_reconciliation_required(
                repository_id,
                current_time_ms()?,
            )?;
            Err(MasterError::AssemblyLineGithubCreationReconciliationRequired)
        }
        _ => Err(MasterError::AssemblyLineGithubCreationReconciliationRequired),
    }
}

impl MasterKernel {
    fn require_assembly_line_repository_provider_provenance(
        &self,
        repository_id: Uuid,
        catalog: &PlanningEffectAdapterCatalog,
    ) -> Result<(), MasterError> {
        catalog.validate()?;
        let frozen_json: String = self
            .connection
            .query_row(
                "SELECT frozen.canonical_json
                 FROM assembly_line_repositories repository
                 JOIN assembly_line_frozen_specifications frozen
                   ON frozen.specification_id=repository.approved_specification_id
                 WHERE repository.repository_id=?1 AND frozen.target_kind='project'",
                [repository_id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(MasterError::AssemblyLineGithubCreationUnavailable)?;
        let frozen: FrozenBrainstormingSpecification = serde_json::from_str(&frozen_json)?;
        frozen.validate()?;
        for binding in &catalog.brainstorming {
            if self.has_assembly_line_provider_accepted_specification(&frozen, binding)? {
                return Ok(());
            }
        }
        Err(MasterError::AssemblyLineGithubCreationUnavailable)
    }

    fn has_assembly_line_provider_accepted_specification(
        &self,
        frozen: &FrozenBrainstormingSpecification,
        binding: &BrainstormingAdapterBinding,
    ) -> Result<bool, MasterError> {
        let mut statement = self.connection.prepare(
            "SELECT redacted_metadata_json FROM assembly_line_audit
             WHERE event_kind='brainstorming_provider_output_accepted' ORDER BY audit_id ASC",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut matches = 0_usize;
        for row in rows {
            let audit: BrainstormingAcceptedAudit = serde_json::from_str(&row?)?;
            if audit.target_kind == brainstorming_target(frozen.target_kind)
                && audit.draft_id == frozen.draft_id
                && audit.specification_id == frozen.specification_id
                && audit.specification_sha256 == frozen.specification_sha256
                && audit.provider_id == binding.profile.provider_id
                && audit.model_id == binding.profile.model_id
                && audit.adapter_sha256 == binding.executable_sha256
            {
                if audit.adapter_catalog_sha256 == [0; 32]
                    || !audit.planning_only
                    || audit.provider_output_retained_in_audit
                    || audit.external_effect_authorized
                {
                    return Err(MasterError::InvalidStoredState(
                        "accepted brainstorming provenance audit is malformed".to_string(),
                    ));
                }
                matches += 1;
            }
        }
        Ok(matches == 1)
    }

    pub(crate) fn require_assembly_line_provider_accepted_specification(
        &self,
        frozen: &FrozenBrainstormingSpecification,
        binding: &BrainstormingAdapterBinding,
        adapter_catalog_sha256: [u8; 32],
    ) -> Result<(), MasterError> {
        let expected = canonical_json(&serde_json::json!({
            "target_kind": brainstorming_target(frozen.target_kind),
            "draft_id": frozen.draft_id,
            "specification_id": frozen.specification_id,
            "specification_sha256": frozen.specification_sha256,
            "provider_id": binding.profile.provider_id,
            "model_id": binding.profile.model_id,
            "adapter_sha256": binding.executable_sha256,
            "adapter_catalog_sha256": adapter_catalog_sha256,
            "planning_only": true,
            "provider_output_retained_in_audit": false,
            "external_effect_authorized": false
        }))?;
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM assembly_line_audit
             WHERE event_kind='brainstorming_provider_output_accepted'
               AND redacted_metadata_json=?1",
            [expected],
            |row| row.get(0),
        )?;
        if count != 1 {
            return Err(MasterError::AssemblyLineBrainstormingUnavailable);
        }
        Ok(())
    }

    pub fn assembly_line_frozen_specification_for_draft(
        &self,
        target_kind: BrainstormingTargetKind,
        draft_id: Uuid,
    ) -> Result<Option<FrozenBrainstormingSpecification>, MasterError> {
        let json = self
            .connection
            .query_row(
                "SELECT canonical_json FROM assembly_line_frozen_specifications
                 WHERE target_kind=?1 AND draft_id=?2",
                params![
                    match target_kind {
                        BrainstormingTargetKind::Project => "project",
                        BrainstormingTargetKind::Feature => "feature",
                    },
                    draft_id.to_string()
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        json.map(|json| {
            let frozen: FrozenBrainstormingSpecification = serde_json::from_str(&json)?;
            frozen.validate()?;
            Ok(frozen)
        })
        .transpose()
    }

    fn prepare_assembly_line_brainstorming_intent(
        &mut self,
        parameters: BrainstormingIntentParameters<'_>,
    ) -> Result<([u8; 32], bool), MasterError> {
        let BrainstormingIntentParameters {
            target_kind,
            draft_id,
            draft_sha256,
            binding,
            adapter_catalog_sha256,
            authorization,
            now_ms,
        } = parameters;
        let target_kind = match target_kind {
            BrainstormingTargetKind::Project => "project",
            BrainstormingTargetKind::Feature => "feature",
        };
        let idempotency_key_sha256: [u8; 32] =
            Sha256::digest(canonical_json(&serde_json::json!({
                "schema_version": 1,
                "target_kind": target_kind,
                "draft_id": draft_id,
                "draft_sha256": draft_sha256,
                "provider_id": binding.profile.provider_id,
                "model_id": binding.profile.model_id,
                "adapter_sha256": binding.executable_sha256,
                "adapter_catalog_sha256": adapter_catalog_sha256,
                "information_classification": authorization.map(|_| "public"),
                "owner_cloud_disclosure_sha256": authorization.map(|value| value.owner_cloud_disclosure_sha256)
            }))?)
            .into();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut statement = tx.prepare(
            "SELECT redacted_metadata_json FROM assembly_line_audit
             WHERE event_kind='brainstorming_provider_call_started' ORDER BY audit_id ASC",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut found = false;
        for row in rows {
            let audit: BrainstormingStartedAudit = serde_json::from_str(&row?)?;
            if audit.draft_id == draft_id && audit.target_kind == target_kind {
                if found
                    || audit.draft_sha256 != draft_sha256
                    || audit.provider_id != binding.profile.provider_id
                    || audit.model_id != binding.profile.model_id
                    || audit.adapter_sha256 != binding.executable_sha256
                    || audit.adapter_catalog_sha256 != adapter_catalog_sha256
                    || audit.idempotency_key_sha256 != idempotency_key_sha256
                    || audit.information_classification.as_deref()
                        != authorization.map(|_| "public")
                    || audit.owner_cloud_disclosure_sha256
                        != authorization.map(|value| value.owner_cloud_disclosure_sha256)
                    || !audit.planning_only
                    || audit.provider_output_retained_in_audit
                    || !audit.external_effect_possible
                {
                    return Err(MasterError::InvalidStoredState(
                        "brainstorming provider intent audit is malformed, duplicated, or drifted"
                            .to_string(),
                    ));
                }
                found = true;
            }
        }
        drop(statement);
        if found {
            tx.commit()?;
            return Ok((idempotency_key_sha256, true));
        }
        append_assembly_line_audit_tx(
            &tx,
            "brainstorming_provider_call_started",
            now_ms,
            serde_json::json!({
                "target_kind": target_kind,
                "draft_id": draft_id,
                "draft_sha256": draft_sha256,
                "provider_id": binding.profile.provider_id,
                "model_id": binding.profile.model_id,
                "adapter_sha256": binding.executable_sha256,
                "adapter_catalog_sha256": adapter_catalog_sha256,
                "idempotency_key_sha256": idempotency_key_sha256,
                "information_classification": authorization.map(|_| "public"),
                "owner_cloud_disclosure_sha256": authorization.map(|value| value.owner_cloud_disclosure_sha256),
                "planning_only": true,
                "provider_output_retained_in_audit": false,
                "external_effect_possible": true
            }),
        )?;
        tx.commit()?;
        Ok((idempotency_key_sha256, false))
    }

    fn record_assembly_line_brainstorming_failure(
        &mut self,
        target_kind: BrainstormingTargetKind,
        draft_id: Uuid,
        error: BrainstormingAdapterError,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        append_assembly_line_audit_tx(
            &tx,
            "brainstorming_provider_call_failed",
            now_ms,
            serde_json::json!({
                "target_kind": match target_kind {
                    BrainstormingTargetKind::Project => "project",
                    BrainstormingTargetKind::Feature => "feature",
                },
                "draft_id": draft_id,
                "reason": match error {
                    BrainstormingAdapterError::Unavailable => "provider_unavailable",
                    BrainstormingAdapterError::Rejected => "provider_rejected",
                    BrainstormingAdapterError::Cancelled => "provider_cancelled",
                    BrainstormingAdapterError::Timeout => "provider_timeout",
                    BrainstormingAdapterError::MalformedOutput => "malformed_output",
                },
                "provider_output_retained_in_audit": false,
                "repository_created": false,
                "feature_created": false,
                "reconciliation_required": true
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn assembly_line_repository_creation_projection(
        &self,
        repository_id: Uuid,
    ) -> Result<RepositoryCreationProjection, MasterError> {
        let tx = self.connection.unchecked_transaction()?;
        let projection = assembly_line_repository_projection_tx(&tx, repository_id)?;
        projection.validate()?;
        Ok(projection)
    }

    fn assembly_line_repository_creation_adapter_binding(
        &self,
        repository_id: Uuid,
    ) -> Result<Option<GithubCreationIntentBinding>, MasterError> {
        let tx = self.connection.unchecked_transaction()?;
        github_creation_adapter_binding_tx(&tx, repository_id)
    }

    fn prepare_assembly_line_github_inspection_intent(
        &mut self,
        plan: &RepositoryCreationProjection,
        adapter_sha256: [u8; 32],
        adapter_catalog_sha256: [u8; 32],
        idempotency_key_sha256: [u8; 32],
        now_ms: u64,
    ) -> Result<(), MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut statement = tx.prepare(
            "SELECT redacted_metadata_json FROM assembly_line_audit
             WHERE event_kind='github_repository_inspection_started' ORDER BY audit_id ASC",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut found = false;
        for row in rows {
            let audit: GithubInspectionStartedAudit = serde_json::from_str(&row?)?;
            if audit.repository_id == plan.repository.repository_id {
                if found
                    || audit.owner_approval_sha256 != plan.owner_approval_sha256
                    || audit.adapter_sha256 != adapter_sha256
                    || audit.adapter_catalog_sha256 != adapter_catalog_sha256
                    || audit.idempotency_key_sha256 != idempotency_key_sha256
                    || audit.visibility != project_visibility_str(plan.visibility)
                    || audit.effect_possible
                    || !audit.external_read_possible
                {
                    return Err(MasterError::InvalidStoredState(
                        "GitHub inspection intent audit is malformed, duplicated, or drifted"
                            .to_string(),
                    ));
                }
                found = true;
            }
        }
        drop(statement);
        if !found {
            append_assembly_line_audit_tx(
                &tx,
                "github_repository_inspection_started",
                now_ms,
                serde_json::json!({
                    "repository_id": plan.repository.repository_id,
                    "owner_approval_sha256": plan.owner_approval_sha256,
                    "adapter_sha256": adapter_sha256,
                    "adapter_catalog_sha256": adapter_catalog_sha256,
                    "idempotency_key_sha256": idempotency_key_sha256,
                    "visibility": project_visibility_str(plan.visibility),
                    "effect_possible": false,
                    "external_read_possible": true
                }),
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    fn begin_assembly_line_repository_creation(
        &mut self,
        repository_id: Uuid,
        binding_sha256: [u8; 32],
        adapter_catalog_sha256: [u8; 32],
        idempotency_key_sha256: [u8; 32],
        now_ms: u64,
    ) -> Result<(), MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prior = assembly_line_repository_projection_tx(&tx, repository_id)?;
        if prior.lifecycle != RepositoryCreationLifecycle::CreationPending
            || prior.effect_possible
            || prior.creation_evidence_sha256.is_some()
        {
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        let next_revision = prior
            .lifecycle_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let changed = tx.execute(
            "UPDATE assembly_line_repositories
             SET lifecycle='reconciling',lifecycle_revision=?1,effect_possible=1
             WHERE repository_id=?2 AND lifecycle_revision=?3
               AND lifecycle='creation_pending' AND effect_possible=0
               AND creation_evidence_sha256 IS NULL",
            params![
                u64_to_i64(next_revision)?,
                repository_id.to_string(),
                u64_to_i64(prior.lifecycle_revision)?
            ],
        )?;
        if changed != 1 {
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        advance_assembly_line_owner_revision_tx(&tx)?;
        append_assembly_line_audit_tx(
            &tx,
            "github_repository_creation_started",
            now_ms,
            serde_json::json!({
                "repository_id": repository_id,
                "owner_approval_sha256": prior.owner_approval_sha256,
                "adapter_sha256": binding_sha256,
                "adapter_catalog_sha256": adapter_catalog_sha256,
                "idempotency_key_sha256": idempotency_key_sha256,
                "visibility": project_visibility_str(prior.visibility),
                "effect_possible": true,
                "automatic_retry_authorized": false
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    fn complete_assembly_line_repository_creation(
        &mut self,
        repository_id: Uuid,
        observation: &GithubRepositoryObservation,
        binding_sha256: [u8; 32],
        adapter_catalog_sha256: [u8; 32],
        idempotency_key_sha256: [u8; 32],
        now_ms: u64,
    ) -> Result<RepositoryCreationProjection, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prior = assembly_line_repository_projection_tx(&tx, repository_id)?;
        if prior.lifecycle == RepositoryCreationLifecycle::Created {
            return Ok(prior);
        }
        if !matches!(
            prior.lifecycle,
            RepositoryCreationLifecycle::Reconciling
                | RepositoryCreationLifecycle::ReconciliationRequired
        ) || !observation.matches(&prior)
            || github_creation_adapter_binding_tx(&tx, repository_id)?
                != Some(GithubCreationIntentBinding {
                    adapter_sha256: binding_sha256,
                    adapter_catalog_sha256,
                    idempotency_key_sha256,
                    owner_approval_sha256: prior.owner_approval_sha256,
                    visibility: prior.visibility,
                })
        {
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        let evidence_json = canonical_json(&serde_json::json!({
            "schema_version": 1,
            "repository": observation.repository,
            "visibility": observation.visibility,
            "default_branch": observation.default_branch,
            "initialized": observation.initialized,
            "owner_approval_sha256": prior.owner_approval_sha256,
            "approved_specification_sha256": prior.approved_specification_sha256,
            "adapter_sha256": binding_sha256
            ,"adapter_catalog_sha256": adapter_catalog_sha256
            ,"idempotency_key_sha256": idempotency_key_sha256
        }))?;
        let evidence_sha256: [u8; 32] = Sha256::digest(evidence_json.as_bytes()).into();
        let next_revision = prior
            .lifecycle_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let changed = tx.execute(
            "UPDATE assembly_line_repositories
             SET lifecycle='created',lifecycle_revision=?1,effect_possible=1,
                 creation_evidence_sha256=?2
             WHERE repository_id=?3 AND lifecycle_revision=?4
               AND lifecycle IN('reconciling','reconciliation_required')
               AND effect_possible=1 AND creation_evidence_sha256 IS NULL",
            params![
                u64_to_i64(next_revision)?,
                evidence_sha256.as_slice(),
                repository_id.to_string(),
                u64_to_i64(prior.lifecycle_revision)?
            ],
        )?;
        if changed != 1 {
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        advance_assembly_line_owner_revision_tx(&tx)?;
        append_assembly_line_audit_tx(
            &tx,
            "github_repository_creation_reconciled",
            now_ms,
            serde_json::json!({
                "repository_id": repository_id,
                "creation_evidence_sha256": evidence_sha256,
                "adapter_sha256": binding_sha256,
                "visibility": project_visibility_str(prior.visibility),
                "default_branch": "main",
                "effect_possible": true,
                "exact_reconciliation": true
            }),
        )?;
        let projection = assembly_line_repository_projection_tx(&tx, repository_id)?;
        tx.commit()?;
        Ok(projection)
    }

    fn mark_assembly_line_repository_creation_conflict(
        &mut self,
        repository_id: Uuid,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        self.mark_assembly_line_repository_creation_state(
            repository_id,
            "creation_pending",
            "conflict",
            false,
            "github_repository_preexisting_conflict",
            now_ms,
        )
    }

    fn mark_assembly_line_repository_reconciliation_required(
        &mut self,
        repository_id: Uuid,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prior = assembly_line_repository_projection_tx(&tx, repository_id)?;
        if prior.lifecycle == RepositoryCreationLifecycle::ReconciliationRequired {
            return Ok(());
        }
        if prior.lifecycle != RepositoryCreationLifecycle::Reconciling {
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        let next_revision = prior
            .lifecycle_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let changed = tx.execute(
            "UPDATE assembly_line_repositories
             SET lifecycle='reconciliation_required',lifecycle_revision=?1
             WHERE repository_id=?2 AND lifecycle_revision=?3
               AND lifecycle='reconciling' AND effect_possible=1
               AND creation_evidence_sha256 IS NULL",
            params![
                u64_to_i64(next_revision)?,
                repository_id.to_string(),
                u64_to_i64(prior.lifecycle_revision)?
            ],
        )?;
        if changed != 1 {
            return Err(MasterError::AssemblyLineGithubCreationReconciliationRequired);
        }
        advance_assembly_line_owner_revision_tx(&tx)?;
        append_assembly_line_audit_tx(
            &tx,
            "github_repository_creation_reconciliation_required",
            now_ms,
            serde_json::json!({
                "repository_id": repository_id,
                "effect_possible": true,
                "automatic_retry_authorized": false,
                "raw_error_present": false
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    fn mark_assembly_line_repository_creation_state(
        &mut self,
        repository_id: Uuid,
        from: &str,
        to: &str,
        effect_possible: bool,
        event: &str,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let prior = assembly_line_repository_projection_tx(&tx, repository_id)?;
        let next_revision = prior
            .lifecycle_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let changed = tx.execute(
            "UPDATE assembly_line_repositories
             SET lifecycle=?1,lifecycle_revision=?2,effect_possible=?3
             WHERE repository_id=?4 AND lifecycle_revision=?5 AND lifecycle=?6
               AND creation_evidence_sha256 IS NULL",
            params![
                to,
                u64_to_i64(next_revision)?,
                if effect_possible { 1_i64 } else { 0_i64 },
                repository_id.to_string(),
                u64_to_i64(prior.lifecycle_revision)?,
                from
            ],
        )?;
        if changed != 1 {
            return Err(MasterError::AssemblyLineGithubCreationConflict);
        }
        advance_assembly_line_owner_revision_tx(&tx)?;
        append_assembly_line_audit_tx(
            &tx,
            event,
            now_ms,
            serde_json::json!({
                "repository_id": repository_id,
                "effect_possible": effect_possible,
                "automatic_retry_authorized": false,
                "raw_error_present": false
            }),
        )?;
        tx.commit()?;
        Ok(())
    }
}

fn brainstorming_target(target: BrainstormingTargetKind) -> &'static str {
    match target {
        BrainstormingTargetKind::Project => "project",
        BrainstormingTargetKind::Feature => "feature",
    }
}

fn github_creation_adapter_binding_tx(
    tx: &Transaction<'_>,
    repository_id: Uuid,
) -> Result<Option<GithubCreationIntentBinding>, MasterError> {
    let mut statement = tx.prepare(
        "SELECT redacted_metadata_json FROM assembly_line_audit
         WHERE event_kind='github_repository_creation_started'
         ORDER BY audit_id ASC",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut binding = None;
    for row in rows {
        let audit: GithubCreationStartedAudit = serde_json::from_str(&row?)?;
        if audit.repository_id == repository_id {
            if binding.is_some()
                || audit.owner_approval_sha256 == [0; 32]
                || audit.adapter_sha256 == [0; 32]
                || audit.adapter_catalog_sha256 == [0; 32]
                || audit.idempotency_key_sha256 == [0; 32]
                || !matches!(audit.visibility.as_str(), "public" | "private")
                || !audit.effect_possible
                || audit.automatic_retry_authorized
            {
                return Err(MasterError::InvalidStoredState(
                    "GitHub creation adapter binding audit is malformed or duplicated".to_string(),
                ));
            }
            let visibility = match audit.visibility.as_str() {
                "public" => ProjectVisibility::Public,
                "private" => ProjectVisibility::Private,
                _ => {
                    return Err(MasterError::InvalidStoredState(
                        "GitHub creation adapter binding visibility is malformed".to_string(),
                    ));
                }
            };
            binding = Some(GithubCreationIntentBinding {
                adapter_sha256: audit.adapter_sha256,
                adapter_catalog_sha256: audit.adapter_catalog_sha256,
                idempotency_key_sha256: audit.idempotency_key_sha256,
                owner_approval_sha256: audit.owner_approval_sha256,
                visibility,
            });
        }
    }
    Ok(binding)
}

// A credential-owning production process adapter is intentionally not present in
// this slice. It must use the existing pre-spawn Windows Job Object gate; assigning
// containment after spawn would permit a child-delegation race.

#[cfg(test)]
#[path = "planning_effects_tests.rs"]
mod tests;
