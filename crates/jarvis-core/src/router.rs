use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    ApprovalDecision, ApprovalGrant, ApprovalStatus, CapabilityScope, PermissionEngine,
    PolicyRequest, ProviderStatus, RiskTier, Sensitivity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvider {
    Local,
    ChatGpt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteOutcome {
    Selected,
    NeedsApproval,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouteRequest {
    pub task_id: Option<Uuid>,
    pub user_intent: String,
    pub sensitivity: Sensitivity,
    pub required_scopes: Vec<CapabilityScope>,
    pub granted_scopes: Vec<CapabilityScope>,
    pub local_available: bool,
    pub local_sufficient: bool,
    pub provider_status: ProviderStatus,
    pub emergency_paused: bool,
    pub approval: Option<ApprovalGrant>,
    pub context_preview: String,
}

impl ModelRouteRequest {
    pub fn local(intent: impl Into<String>, context_preview: impl Into<String>) -> Self {
        Self {
            task_id: None,
            user_intent: intent.into(),
            sensitivity: Sensitivity::Workspace,
            required_scopes: vec![CapabilityScope::Conversation, CapabilityScope::LocalModel],
            granted_scopes: vec![CapabilityScope::Conversation, CapabilityScope::LocalModel],
            local_available: true,
            local_sufficient: true,
            provider_status: ProviderStatus::from_config(&crate::ProviderConfig::local_only()),
            emergency_paused: false,
            approval: None,
            context_preview: context_preview.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteEvidence {
    pub local_available: bool,
    pub local_sufficient: bool,
    pub chatgpt_enabled: bool,
    pub chatgpt_requires_approval: bool,
    pub required_scopes: Vec<CapabilityScope>,
    pub granted_scopes: Vec<CapabilityScope>,
    pub restricted_cloud_block: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouteRecord {
    pub id: Uuid,
    pub task_id: Option<Uuid>,
    pub outcome: RouteOutcome,
    pub selected_provider: Option<ModelProvider>,
    pub reason: String,
    pub sensitivity: Sensitivity,
    pub approval_status: ApprovalStatus,
    pub redaction_applied: bool,
    pub context_for_model: Option<String>,
    pub local_available: bool,
    pub local_sufficient: bool,
    pub evidence: RouteEvidence,
    pub created_at: DateTime<Utc>,
}

pub struct ModelRouter;

impl ModelRouter {
    pub fn route(request: &ModelRouteRequest) -> ModelRouteRecord {
        if request.emergency_paused {
            return Self::blocked(request, "emergency pause blocks model routing");
        }

        if request.local_available && request.local_sufficient {
            return ModelRouteRecord {
                id: Uuid::new_v4(),
                task_id: request.task_id,
                outcome: RouteOutcome::Selected,
                selected_provider: Some(ModelProvider::Local),
                reason: "local model is available and sufficient".to_string(),
                sensitivity: request.sensitivity,
                approval_status: ApprovalStatus::NotRequired,
                redaction_applied: false,
                context_for_model: Some(request.context_preview.clone()),
                local_available: request.local_available,
                local_sufficient: request.local_sufficient,
                evidence: Self::evidence(request, false),
                created_at: Utc::now(),
            };
        }

        if !request.provider_status.chatgpt_enabled {
            return Self::blocked(
                request,
                "local model is unavailable or insufficient and ChatGPT routing is disabled",
            );
        }

        if request.sensitivity == Sensitivity::Restricted {
            return Self::blocked(request, "restricted data is never routed to ChatGPT");
        }

        let mut required_scopes = request.required_scopes.clone();
        if !required_scopes.contains(&CapabilityScope::CloudModel) {
            required_scopes.push(CapabilityScope::CloudModel);
        }

        let policy_request = PolicyRequest {
            task_id: request.task_id,
            action: "route to ChatGPT".to_string(),
            requested_scopes: required_scopes,
            granted_scopes: request.granted_scopes.clone(),
            risk_tier: Self::cloud_risk_tier(request.sensitivity),
            sensitivity: request.sensitivity,
            emergency_paused: request.emergency_paused,
            approval: request.approval.clone(),
        };
        let policy = PermissionEngine::evaluate(&policy_request);

        match policy.decision {
            ApprovalDecision::AllowSilently | ApprovalDecision::AllowWithNotification => {
                let redacted = redact_for_chatgpt(&request.context_preview);
                ModelRouteRecord {
                    id: Uuid::new_v4(),
                    task_id: request.task_id,
                    outcome: RouteOutcome::Selected,
                    selected_provider: Some(ModelProvider::ChatGpt),
                    reason: "ChatGPT selected after explicit policy approval".to_string(),
                    sensitivity: request.sensitivity,
                    approval_status: policy.approval_status,
                    redaction_applied: redacted != request.context_preview,
                    context_for_model: Some(redacted),
                    local_available: request.local_available,
                    local_sufficient: request.local_sufficient,
                    evidence: Self::evidence(request, false),
                    created_at: Utc::now(),
                }
            }
            ApprovalDecision::RequireConfirmation => ModelRouteRecord {
                id: Uuid::new_v4(),
                task_id: request.task_id,
                outcome: RouteOutcome::NeedsApproval,
                selected_provider: None,
                reason: policy.reason,
                sensitivity: request.sensitivity,
                approval_status: policy.approval_status,
                redaction_applied: false,
                context_for_model: None,
                local_available: request.local_available,
                local_sufficient: request.local_sufficient,
                evidence: Self::evidence(request, false),
                created_at: Utc::now(),
            },
            ApprovalDecision::Blocked => ModelRouteRecord {
                id: Uuid::new_v4(),
                task_id: request.task_id,
                outcome: RouteOutcome::Blocked,
                selected_provider: None,
                reason: policy.reason,
                sensitivity: request.sensitivity,
                approval_status: policy.approval_status,
                redaction_applied: false,
                context_for_model: None,
                local_available: request.local_available,
                local_sufficient: request.local_sufficient,
                evidence: Self::evidence(request, false),
                created_at: Utc::now(),
            },
        }
    }

    fn cloud_risk_tier(sensitivity: Sensitivity) -> RiskTier {
        match sensitivity {
            Sensitivity::Public | Sensitivity::Workspace => RiskTier::Notify,
            Sensitivity::Personal | Sensitivity::Private | Sensitivity::CredentialAdjacent => {
                RiskTier::Confirm
            }
            Sensitivity::Restricted => RiskTier::Block,
        }
    }

    fn blocked(request: &ModelRouteRequest, reason: impl Into<String>) -> ModelRouteRecord {
        let restricted_cloud_block = request.sensitivity == Sensitivity::Restricted
            && request.provider_status.chatgpt_enabled
            && !(request.local_available && request.local_sufficient);
        ModelRouteRecord {
            id: Uuid::new_v4(),
            task_id: request.task_id,
            outcome: RouteOutcome::Blocked,
            selected_provider: None,
            reason: reason.into(),
            sensitivity: request.sensitivity,
            approval_status: ApprovalStatus::Denied,
            redaction_applied: false,
            context_for_model: None,
            local_available: request.local_available,
            local_sufficient: request.local_sufficient,
            evidence: Self::evidence(request, restricted_cloud_block),
            created_at: Utc::now(),
        }
    }

    fn evidence(request: &ModelRouteRequest, restricted_cloud_block: bool) -> RouteEvidence {
        RouteEvidence {
            local_available: request.local_available,
            local_sufficient: request.local_sufficient,
            chatgpt_enabled: request.provider_status.chatgpt_enabled,
            chatgpt_requires_approval: request.provider_status.chatgpt_requires_approval,
            required_scopes: request.required_scopes.clone(),
            granted_scopes: request.granted_scopes.clone(),
            restricted_cloud_block,
        }
    }
}

pub fn redact_for_chatgpt(input: &str) -> String {
    let mut redacted = Vec::new();

    for token in input.split_whitespace() {
        let normalized = token
            .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
            .to_ascii_lowercase();
        let looks_like_secret = normalized.contains("api_key")
            || normalized.contains("token")
            || normalized.contains("secret")
            || normalized.contains("password")
            || token.starts_with("sk-");

        if looks_like_secret {
            redacted.push("[REDACTED]".to_string());
        } else {
            redacted.push(token.to_string());
        }
    }

    redacted.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chatgpt_request(sensitivity: Sensitivity) -> ModelRouteRequest {
        ModelRouteRequest {
            task_id: None,
            user_intent: "complex planning".to_string(),
            sensitivity,
            required_scopes: vec![CapabilityScope::Conversation],
            granted_scopes: vec![
                CapabilityScope::Conversation,
                CapabilityScope::LocalModel,
                CapabilityScope::CloudModel,
            ],
            local_available: true,
            local_sufficient: false,
            provider_status: ProviderStatus::from_config(
                &crate::ProviderConfig::local_only().with_chatgpt_enabled("chatgpt-test"),
            ),
            emergency_paused: false,
            approval: None,
            context_preview: "workspace summary api_key=abc123 token sk-test".to_string(),
        }
    }

    #[test]
    fn routes_to_local_when_local_is_sufficient() {
        let request = ModelRouteRequest::local("summarize", "public workspace notes");

        let record = ModelRouter::route(&request);

        assert_eq!(record.outcome, RouteOutcome::Selected);
        assert_eq!(record.selected_provider, Some(ModelProvider::Local));
        assert_eq!(record.approval_status, ApprovalStatus::NotRequired);
    }

    #[test]
    fn routes_workspace_to_chatgpt_with_redaction_when_cloud_scope_is_granted() {
        let request = chatgpt_request(Sensitivity::Workspace);

        let record = ModelRouter::route(&request);

        assert_eq!(record.outcome, RouteOutcome::Selected);
        assert_eq!(record.selected_provider, Some(ModelProvider::ChatGpt));
        assert!(record.redaction_applied);
        assert_eq!(
            record.context_for_model.as_deref(),
            Some("workspace summary [REDACTED] [REDACTED] [REDACTED]")
        );
    }

    #[test]
    fn private_chatgpt_route_requires_explicit_approval() {
        let request = chatgpt_request(Sensitivity::Private);

        let record = ModelRouter::route(&request);

        assert_eq!(record.outcome, RouteOutcome::NeedsApproval);
        assert_eq!(record.selected_provider, None);
        assert_eq!(record.approval_status, ApprovalStatus::Pending);
    }

    #[test]
    fn private_chatgpt_route_selects_after_approval() {
        let mut request = chatgpt_request(Sensitivity::Private);
        request.approval = Some(ApprovalGrant::approved(vec![
            CapabilityScope::Conversation,
            CapabilityScope::CloudModel,
        ]));

        let record = ModelRouter::route(&request);

        assert_eq!(record.outcome, RouteOutcome::Selected);
        assert_eq!(record.selected_provider, Some(ModelProvider::ChatGpt));
        assert_eq!(record.approval_status, ApprovalStatus::Approved);
    }

    #[test]
    fn restricted_data_is_blocked_from_chatgpt_even_with_approval() {
        let mut request = chatgpt_request(Sensitivity::Restricted);
        request.approval = Some(ApprovalGrant::approved(vec![
            CapabilityScope::Conversation,
            CapabilityScope::CloudModel,
        ]));

        let record = ModelRouter::route(&request);

        assert_eq!(record.outcome, RouteOutcome::Blocked);
        assert_eq!(record.selected_provider, None);
    }

    #[test]
    fn emergency_pause_blocks_routing() {
        let mut request = chatgpt_request(Sensitivity::Workspace);
        request.emergency_paused = true;

        let record = ModelRouter::route(&request);

        assert_eq!(record.outcome, RouteOutcome::Blocked);
        assert!(record.reason.contains("emergency pause"));
    }
}
