use jarvis_core::{
    ApprovalStatus, AuditEntry, JarvisError, RiskTier, Sensitivity, TaskRecord, TaskStatus,
};
use serde_json::json;
use uuid::Uuid;

#[test]
fn public_task_and_audit_types_support_an_end_to_end_task_story() {
    let session_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let task = TaskRecord {
        id: task_id,
        session_id,
        user_input: "summarize today's local project status".to_owned(),
        status: TaskStatus::Created,
        created_at: now,
        updated_at: now,
    };

    let audit = AuditEntry::new(
        Some(task.id),
        "task.created",
        "Created a local-first assistant task",
        json!({
            "session_id": task.session_id,
            "sensitivity": Sensitivity::Workspace,
            "risk_tier": RiskTier::Low,
            "approval_status": ApprovalStatus::NotRequired
        }),
    );

    assert_eq!(task.status, TaskStatus::Created);
    assert_eq!(audit.task_id, Some(task.id));
    assert_eq!(audit.event_type, "task.created");
    assert_eq!(audit.payload["sensitivity"], "workspace");
    assert_eq!(audit.payload["risk_tier"], "low");
    assert_eq!(audit.payload["approval_status"], "not_required");
}

#[test]
fn safety_enums_serialize_with_stable_contract_names() {
    assert_eq!(json!(Sensitivity::Public), "public");
    assert_eq!(json!(Sensitivity::Workspace), "workspace");
    assert_eq!(json!(Sensitivity::Personal), "personal");
    assert_eq!(json!(Sensitivity::Private), "private");
    assert_eq!(
        json!(Sensitivity::CredentialAdjacent),
        "credential_adjacent"
    );
    assert_eq!(json!(Sensitivity::Restricted), "restricted");

    assert_eq!(json!(RiskTier::Low), "low");
    assert_eq!(json!(RiskTier::Notify), "notify");
    assert_eq!(json!(RiskTier::Confirm), "confirm");
    assert_eq!(json!(RiskTier::Block), "block");

    assert_eq!(json!(ApprovalStatus::NotRequired), "not_required");
    assert_eq!(json!(ApprovalStatus::Pending), "pending");
    assert_eq!(json!(ApprovalStatus::Approved), "approved");
    assert_eq!(json!(ApprovalStatus::Denied), "denied");
}

#[test]
fn risk_tiers_order_from_least_to_most_restrictive() {
    assert!(RiskTier::Low < RiskTier::Notify);
    assert!(RiskTier::Notify < RiskTier::Confirm);
    assert!(RiskTier::Confirm < RiskTier::Block);
}

#[test]
fn policy_errors_are_human_readable_for_ipc_and_cli_boundaries() {
    let blocked = JarvisError::PolicyBlocked("restricted data cannot route to cloud".to_owned());
    let approval = JarvisError::ApprovalRequired("confirm file write".to_owned());
    let validation = JarvisError::Validation("missing plugin action schema".to_owned());

    assert_eq!(
        blocked.to_string(),
        "blocked by policy: restricted data cannot route to cloud"
    );
    assert_eq!(
        approval.to_string(),
        "approval required: confirm file write"
    );
    assert_eq!(
        validation.to_string(),
        "validation failed: missing plugin action schema"
    );
}

#[test]
#[ignore = "Enable after the command runtime, fake model, fake plugin, and audit store APIs land."]
fn command_pipeline_blocks_confirm_tier_plugin_actions_without_approval() {
    // Future integration shape:
    // 1. Create a temporary local Jarvis core store.
    // 2. Register a fake local model that requests a fake plugin action.
    // 3. Register a fake plugin action with RiskTier::Confirm.
    // 4. Submit a command through the public command runtime API.
    // 5. Assert the task moves to WaitingForApproval.
    // 6. Assert no plugin side effect ran.
    // 7. Assert an audit entry records the approval requirement.
}

#[test]
#[ignore = "Enable after plugin manifest validation is exposed as a public API."]
fn plugin_manifest_rejects_actions_outside_declared_scopes() {
    // Future integration shape:
    // 1. Build a plugin manifest with a narrow workspace-read scope.
    // 2. Ask the plugin host to run a workspace-write action.
    // 3. Assert the call returns JarvisError::PolicyBlocked.
    // 4. Assert the audit log records the rejected scope escalation.
}
