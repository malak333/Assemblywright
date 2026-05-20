use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApprovalStatus, RiskTier, Sensitivity};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityScope {
    Conversation,
    MemoryRead,
    MemoryWrite,
    FileRead,
    FileWrite,
    NetworkAccess,
    LocalModel,
    CloudModel,
    PluginRun,
    SchedulerRun,
    ExternalCommunication,
    Purchase,
    SystemControl,
    PluginManagement,
    EmergencyControl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    AllowSilently,
    AllowWithNotification,
    RequireConfirmation,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGrant {
    pub status: ApprovalStatus,
    pub approved_scopes: Vec<CapabilityScope>,
    pub approved_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl ApprovalGrant {
    pub fn approved(scopes: Vec<CapabilityScope>) -> Self {
        Self {
            status: ApprovalStatus::Approved,
            approved_scopes: scopes,
            approved_at: Utc::now(),
            expires_at: None,
        }
    }

    fn is_valid_for(&self, requested_scopes: &[CapabilityScope], now: DateTime<Utc>) -> bool {
        if self.status != ApprovalStatus::Approved {
            return false;
        }

        if self.expires_at.is_some_and(|expires_at| expires_at <= now) {
            return false;
        }

        let approved: BTreeSet<_> = self.approved_scopes.iter().collect();
        requested_scopes
            .iter()
            .all(|scope| approved.contains(scope) || *scope == CapabilityScope::Conversation)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRequest {
    pub task_id: Option<Uuid>,
    pub action: String,
    pub requested_scopes: Vec<CapabilityScope>,
    pub granted_scopes: Vec<CapabilityScope>,
    pub risk_tier: RiskTier,
    pub sensitivity: Sensitivity,
    pub emergency_paused: bool,
    pub approval: Option<ApprovalGrant>,
}

impl PolicyRequest {
    pub fn new(
        action: impl Into<String>,
        requested_scopes: Vec<CapabilityScope>,
        granted_scopes: Vec<CapabilityScope>,
        risk_tier: RiskTier,
        sensitivity: Sensitivity,
    ) -> Self {
        Self {
            task_id: None,
            action: action.into(),
            requested_scopes,
            granted_scopes,
            risk_tier,
            sensitivity,
            emergency_paused: false,
            approval: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub decision: ApprovalDecision,
    pub reason: String,
    pub risk_tier: RiskTier,
    pub sensitivity: Sensitivity,
    pub missing_scopes: Vec<CapabilityScope>,
    pub approval_status: ApprovalStatus,
    pub emergency_paused: bool,
    pub audit_required: bool,
    pub decided_at: DateTime<Utc>,
}

pub struct PermissionEngine;

impl PermissionEngine {
    pub fn evaluate(request: &PolicyRequest) -> PolicyDecision {
        let now = Utc::now();
        let audit_required = request.risk_tier != RiskTier::Low
            || request.sensitivity != Sensitivity::Public
            || request
                .requested_scopes
                .iter()
                .any(Self::scope_requires_audit);

        if request.emergency_paused
            && !request
                .requested_scopes
                .contains(&CapabilityScope::EmergencyControl)
        {
            return PolicyDecision {
                decision: ApprovalDecision::Blocked,
                reason: "emergency pause blocks new non-emergency actions".to_string(),
                risk_tier: request.risk_tier,
                sensitivity: request.sensitivity,
                missing_scopes: Vec::new(),
                approval_status: ApprovalStatus::Denied,
                emergency_paused: true,
                audit_required: true,
                decided_at: now,
            };
        }

        let missing_scopes = Self::missing_scopes(request);
        if !missing_scopes.is_empty() {
            return PolicyDecision {
                decision: ApprovalDecision::Blocked,
                reason: "requested capability scope is not granted".to_string(),
                risk_tier: request.risk_tier,
                sensitivity: request.sensitivity,
                missing_scopes,
                approval_status: ApprovalStatus::Denied,
                emergency_paused: request.emergency_paused,
                audit_required: true,
                decided_at: now,
            };
        }

        if request.risk_tier == RiskTier::Block {
            return PolicyDecision {
                decision: ApprovalDecision::Blocked,
                reason: "risk tier is blocked by policy".to_string(),
                risk_tier: request.risk_tier,
                sensitivity: request.sensitivity,
                missing_scopes: Vec::new(),
                approval_status: ApprovalStatus::Denied,
                emergency_paused: request.emergency_paused,
                audit_required: true,
                decided_at: now,
            };
        }

        if Self::requires_confirmation(request) {
            let approval_status = request
                .approval
                .as_ref()
                .map(|approval| approval.status)
                .unwrap_or(ApprovalStatus::Pending);

            if request
                .approval
                .as_ref()
                .is_some_and(|approval| approval.is_valid_for(&request.requested_scopes, now))
            {
                return PolicyDecision {
                    decision: ApprovalDecision::AllowWithNotification,
                    reason: "explicit approval satisfies confirmation policy".to_string(),
                    risk_tier: request.risk_tier,
                    sensitivity: request.sensitivity,
                    missing_scopes: Vec::new(),
                    approval_status: ApprovalStatus::Approved,
                    emergency_paused: request.emergency_paused,
                    audit_required: true,
                    decided_at: now,
                };
            }

            return PolicyDecision {
                decision: ApprovalDecision::RequireConfirmation,
                reason: "action requires explicit user confirmation".to_string(),
                risk_tier: request.risk_tier,
                sensitivity: request.sensitivity,
                missing_scopes: Vec::new(),
                approval_status,
                emergency_paused: request.emergency_paused,
                audit_required: true,
                decided_at: now,
            };
        }

        match request.risk_tier {
            RiskTier::Low => PolicyDecision {
                decision: ApprovalDecision::AllowSilently,
                reason: "low-risk action is inside granted capability scopes".to_string(),
                risk_tier: request.risk_tier,
                sensitivity: request.sensitivity,
                missing_scopes: Vec::new(),
                approval_status: ApprovalStatus::NotRequired,
                emergency_paused: request.emergency_paused,
                audit_required,
                decided_at: now,
            },
            RiskTier::Notify => PolicyDecision {
                decision: ApprovalDecision::AllowWithNotification,
                reason: "notify-tier action is inside granted capability scopes".to_string(),
                risk_tier: request.risk_tier,
                sensitivity: request.sensitivity,
                missing_scopes: Vec::new(),
                approval_status: ApprovalStatus::NotRequired,
                emergency_paused: request.emergency_paused,
                audit_required: true,
                decided_at: now,
            },
            RiskTier::Confirm | RiskTier::Block => unreachable!("handled above"),
        }
    }

    fn missing_scopes(request: &PolicyRequest) -> Vec<CapabilityScope> {
        let granted: BTreeSet<_> = request.granted_scopes.iter().collect();
        request
            .requested_scopes
            .iter()
            .filter(|scope| !granted.contains(scope))
            .cloned()
            .collect()
    }

    fn requires_confirmation(request: &PolicyRequest) -> bool {
        request.risk_tier == RiskTier::Confirm
            || matches!(
                request.sensitivity,
                Sensitivity::Private | Sensitivity::CredentialAdjacent | Sensitivity::Restricted
            )
            || request.requested_scopes.iter().any(|scope| {
                matches!(
                    scope,
                    CapabilityScope::ExternalCommunication
                        | CapabilityScope::Purchase
                        | CapabilityScope::SystemControl
                        | CapabilityScope::PluginManagement
                )
            })
    }

    fn scope_requires_audit(scope: &CapabilityScope) -> bool {
        !matches!(
            scope,
            CapabilityScope::Conversation | CapabilityScope::LocalModel
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_low_risk_inside_granted_scope() {
        let request = PolicyRequest::new(
            "summarize local note",
            vec![CapabilityScope::Conversation, CapabilityScope::LocalModel],
            vec![CapabilityScope::Conversation, CapabilityScope::LocalModel],
            RiskTier::Low,
            Sensitivity::Workspace,
        );

        let decision = PermissionEngine::evaluate(&request);

        assert_eq!(decision.decision, ApprovalDecision::AllowSilently);
        assert_eq!(decision.approval_status, ApprovalStatus::NotRequired);
    }

    #[test]
    fn blocks_missing_capability_scope() {
        let request = PolicyRequest::new(
            "write file",
            vec![CapabilityScope::FileWrite],
            vec![CapabilityScope::FileRead],
            RiskTier::Notify,
            Sensitivity::Workspace,
        );

        let decision = PermissionEngine::evaluate(&request);

        assert_eq!(decision.decision, ApprovalDecision::Blocked);
        assert_eq!(decision.missing_scopes, vec![CapabilityScope::FileWrite]);
    }

    #[test]
    fn emergency_pause_blocks_non_emergency_actions() {
        let mut request = PolicyRequest::new(
            "run scheduled job",
            vec![CapabilityScope::SchedulerRun],
            vec![CapabilityScope::SchedulerRun],
            RiskTier::Low,
            Sensitivity::Public,
        );
        request.emergency_paused = true;

        let decision = PermissionEngine::evaluate(&request);

        assert_eq!(decision.decision, ApprovalDecision::Blocked);
        assert!(decision.emergency_paused);
    }

    #[test]
    fn confirm_risk_requires_approval_then_allows_with_notification() {
        let mut request = PolicyRequest::new(
            "change system setting",
            vec![CapabilityScope::SystemControl],
            vec![CapabilityScope::SystemControl],
            RiskTier::Confirm,
            Sensitivity::Workspace,
        );

        let pending = PermissionEngine::evaluate(&request);
        assert_eq!(pending.decision, ApprovalDecision::RequireConfirmation);

        request.approval = Some(ApprovalGrant::approved(vec![
            CapabilityScope::SystemControl,
        ]));
        let approved = PermissionEngine::evaluate(&request);
        assert_eq!(approved.decision, ApprovalDecision::AllowWithNotification);
        assert_eq!(approved.approval_status, ApprovalStatus::Approved);
    }

    #[test]
    fn private_data_requires_confirmation_even_at_low_risk() {
        let request = PolicyRequest::new(
            "read personal memory",
            vec![CapabilityScope::MemoryRead],
            vec![CapabilityScope::MemoryRead],
            RiskTier::Low,
            Sensitivity::Private,
        );

        let decision = PermissionEngine::evaluate(&request);

        assert_eq!(decision.decision, ApprovalDecision::RequireConfirmation);
    }
}
