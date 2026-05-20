use jarvis_core::{
    plugin_permission_scopes, ApprovalDecision, ApprovalGrant, ApprovalStatus, AuditEntry,
    CancellationBehavior, CancellationSignal, CapabilityScope, InProcessPlugin, JarvisError,
    JarvisResult, JsonSchema, PermissionEngine, PluginAccess, PluginActionManifest,
    PluginCallRequest, PluginCallStatus, PluginHost, PluginManifest, PluginPermission,
    PluginSource, PluginTimeout, PolicyRequest, RiskTier, Sensitivity, TaskRecord, TaskStatus,
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
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
fn command_pipeline_blocks_confirm_tier_plugin_actions_without_approval() {
    let side_effect_ran = Arc::new(AtomicBool::new(false));
    let mut host = PluginHost::new();
    host.register(ConfirmWritePlugin {
        side_effect_ran: Arc::clone(&side_effect_ran),
    })
    .expect("confirm plugin should register");

    let result = host
        .execute(
            PluginCallRequest::reactive(
                "confirm_write",
                "write_note",
                json!({ "note": "prepare external side effect" }),
            )
            .with_granted_scopes(plugin_permission_scopes(&[
                PluginPermission::WriteWorkspace,
            ])),
        )
        .expect("host should return approval requirement");

    assert_eq!(result.status, PluginCallStatus::ApprovalRequired);
    assert!(result.metadata.approval_required);
    assert_eq!(result.metadata.approval_status, ApprovalStatus::Pending);
    assert_eq!(result.metadata.risk_tier, RiskTier::Confirm);
    assert!(
        !side_effect_ran.load(Ordering::SeqCst),
        "confirm-tier plugin must not execute before approval"
    );
}

#[test]
fn command_pipeline_executes_confirm_tier_plugin_action_after_approval() {
    let side_effect_ran = Arc::new(AtomicBool::new(false));
    let mut host = PluginHost::new();
    host.register(ConfirmWritePlugin {
        side_effect_ran: Arc::clone(&side_effect_ran),
    })
    .expect("confirm plugin should register");

    let request = PluginCallRequest::reactive(
        "confirm_write",
        "write_note",
        json!({ "note": "approved side effect" }),
    )
    .with_granted_scopes(plugin_permission_scopes(&[
        PluginPermission::WriteWorkspace,
    ]))
    .with_approval(ApprovalGrant::approved(plugin_permission_scopes(&[
        PluginPermission::WriteWorkspace,
    ])));

    let result = host
        .execute(request)
        .expect("approved confirm-tier plugin should execute");

    assert_eq!(result.status, PluginCallStatus::Completed);
    assert_eq!(result.output, json!({ "note": "approved side effect" }));
    assert_eq!(result.metadata.approval_status, ApprovalStatus::Approved);
    assert!(
        side_effect_ran.load(Ordering::SeqCst),
        "approved confirm-tier plugin should execute exactly once"
    );
}

#[test]
fn plugin_manifest_rejects_actions_outside_declared_scopes() {
    let manifest = PluginManifest {
        manifest_schema_version: 1,
        id: "bad_scope_plugin".to_string(),
        name: "Bad Scope Plugin".to_string(),
        version: "0.1.0".to_string(),
        source: PluginSource::FirstParty,
        author: "Jarvis".to_string(),
        source_path: None,
        actions: vec![PluginActionManifest {
            name: "write_memory_without_scope".to_string(),
            description: "Invalid action that claims write memory access without permission."
                .to_string(),
            permissions: vec![PluginPermission::ReadMemory],
            risk_tier: RiskTier::Notify,
            input_schema: JsonSchema::empty_object(),
            output_schema: JsonSchema::empty_object(),
            proactive: false,
            memory_access: PluginAccess::Write,
            model_access: PluginAccess::None,
            audit_fields: vec!["memory_access".to_string()],
            timeout: PluginTimeout::default_for_action(),
            cancellation: CancellationBehavior::Cooperative,
        }],
    };

    let error = manifest
        .validate()
        .expect_err("manifest should reject undeclared memory write scope");
    assert!(error
        .to_string()
        .contains("memory write access requires write_memory permission"));
}

#[test]
fn policy_blocks_plugin_scope_escalation_before_host_execution() {
    let side_effect_ran = Arc::new(AtomicBool::new(false));
    let mut host = PluginHost::new();
    host.register(ConfirmWritePlugin {
        side_effect_ran: Arc::clone(&side_effect_ran),
    })
    .expect("confirm plugin should register");

    let request = PolicyRequest::new(
        "confirm_write.write_note",
        vec![CapabilityScope::PluginRun, CapabilityScope::FileWrite],
        vec![CapabilityScope::PluginRun, CapabilityScope::FileRead],
        RiskTier::Notify,
        Sensitivity::Workspace,
    );

    let decision = PermissionEngine::evaluate(&request);
    let audit = AuditEntry::new(
        None,
        "plugin_scope_escalation_blocked",
        "plugin action requested a capability scope that was not granted",
        json!({
            "action": request.action,
            "decision": decision.decision,
            "risk_tier": decision.risk_tier,
            "missing_scopes": decision.missing_scopes,
            "approval_status": decision.approval_status,
        }),
    );

    assert_eq!(decision.decision, ApprovalDecision::Blocked);
    assert_eq!(decision.missing_scopes, vec![CapabilityScope::FileWrite]);
    assert_eq!(decision.approval_status, ApprovalStatus::Denied);
    assert!(
        !side_effect_ran.load(Ordering::SeqCst),
        "policy must block the escalated plugin request before host execution"
    );
    assert_eq!(audit.event_type, "plugin_scope_escalation_blocked");
    assert_eq!(audit.payload["decision"], "blocked");
    assert_eq!(audit.payload["missing_scopes"], json!(["file_write"]));
    assert_eq!(audit.payload["approval_status"], "denied");
}

struct ConfirmWritePlugin {
    side_effect_ran: Arc<AtomicBool>,
}

impl InProcessPlugin for ConfirmWritePlugin {
    fn manifest(&self) -> PluginManifest {
        let schema = JsonSchema::new(json!({
            "type": "object",
            "properties": {
                "note": { "type": "string" }
            },
            "required": ["note"],
            "additionalProperties": false
        }));

        PluginManifest {
            manifest_schema_version: 1,
            id: "confirm_write".to_string(),
            name: "Confirm Write".to_string(),
            version: "0.1.0".to_string(),
            source: PluginSource::FirstParty,
            author: "Jarvis".to_string(),
            source_path: None,
            actions: vec![PluginActionManifest {
                name: "write_note".to_string(),
                description: "A fake side-effecting action that must require approval.".to_string(),
                permissions: vec![PluginPermission::WriteWorkspace],
                risk_tier: RiskTier::Confirm,
                input_schema: schema.clone(),
                output_schema: schema,
                proactive: false,
                memory_access: PluginAccess::None,
                model_access: PluginAccess::None,
                audit_fields: vec!["note".to_string()],
                timeout: PluginTimeout::default_for_action(),
                cancellation: CancellationBehavior::Cooperative,
            }],
        }
    }

    fn execute(
        &self,
        _action: &PluginActionManifest,
        input: serde_json::Value,
        _cancellation: CancellationSignal,
    ) -> JarvisResult<serde_json::Value> {
        self.side_effect_ran.store(true, Ordering::SeqCst);
        Ok(input)
    }
}
