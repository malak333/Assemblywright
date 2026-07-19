#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOCAL_GATE="scripts/release-local.sh"
BUILD_DOCS="docs/build-test-commands.md"
CHECKLIST="docs/release-checklist.md"
ARCHITECTURE="docs/architecture-map.md"
KB="docs/knowledge-base/jarvis-project-facts.md"
README="README.md"
CORE_IPC="crates/jarvis-core/src/ipc.rs"
DESIGN="DESIGN.md"
SAFETY_RULES="docs/safety-rules.md"
DISTRIBUTED_DESIGN="docs/distributed-developer-mode-design.md"
PROTOCOL_CRATE="crates/jarvis-protocol/src/lib.rs"
MASTER_CRATE="crates/jarvis-master/src/lib.rs"
MASTER_PROCESS="crates/jarvis-master/src/main.rs"
MASTER_IDENTITY="crates/jarvis-master/src/identity.rs"
WINDOWS_PROTOCOL_WORKFLOW=".github/workflows/windows-protocol.yml"
RELEASE_VERSION_SCRIPT="scripts/release-version.sh"
PROTOCOL_E2E="crates/jarvis-protocol/tests/distributed_protocol_contract_e2e.rs"
MASTER_E2E="crates/jarvis-master/tests/master_lifecycle_e2e.rs"
MASTER_PROCESS_E2E="crates/jarvis-master/tests/master_process_e2e.rs"
MASTER_IDENTITY_E2E="crates/jarvis-master/tests/enrollment_identity_e2e.rs"
MASTER_REMOTE_MTLS_E2E="crates/jarvis-master/tests/remote_mtls_e2e.rs"
MASTER_SERVICE_HOST="crates/jarvis-master/src/windows_service_host.rs"
MASTER_SERVICE_E2E="crates/jarvis-master/tests/windows_service_lifecycle_e2e.rs"
MAC_BRIDGE="apps/mac/Sources/JarvisMacCore/DeveloperBridge.swift"
MAC_BRIDGE_KEYCHAIN="apps/mac/Sources/JarvisMacCore/KeychainDeveloperIdentity.swift"
MAC_BRIDGE_NETWORK="apps/mac/Sources/JarvisMacCore/NetworkMTLSBridge.swift"
MAC_BRIDGE_TESTS="apps/mac/Tests/JarvisMacCoreTests/DeveloperBridgeTests.swift"
MAC_BRIDGE_LIVE_E2E="scripts/mac-windows-bridge-live-e2e.sh"

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_text() {
  local label="$1"
  local file="$2"
  local expected="$3"
  if ! grep -Fq -- "$expected" "$file"; then
    fail "$label does not mention required text in $file: $expected"
  fi
}

forbid_text() {
  local label="$1"
  local file="$2"
  local unexpected="$3"
  if grep -Fq -- "$unexpected" "$file"; then
    fail "$label found stale text in $file: $unexpected"
  fi
}

require_file "$LOCAL_GATE"
require_file "$BUILD_DOCS"
require_file "$CHECKLIST"
require_file "$ARCHITECTURE"
require_file "$KB"
require_file "$README"
require_file "$CORE_IPC"
require_file "$DESIGN"
require_file "$SAFETY_RULES"
require_file "$DISTRIBUTED_DESIGN"
require_file "$PROTOCOL_CRATE"
require_file "$MASTER_CRATE"
require_file "$WINDOWS_PROTOCOL_WORKFLOW"
require_file "$RELEASE_VERSION_SCRIPT"
require_file "$PROTOCOL_E2E"
require_file "$MASTER_E2E"
require_file "$MASTER_PROCESS"
require_file "$MASTER_PROCESS_E2E"
require_file "$MASTER_IDENTITY"
require_file "$MASTER_IDENTITY_E2E"
require_file "$MASTER_REMOTE_MTLS_E2E"
require_file "$MASTER_SERVICE_HOST"
require_file "$MASTER_SERVICE_E2E"
require_file "$MAC_BRIDGE"
require_file "$MAC_BRIDGE_KEYCHAIN"
require_file "$MAC_BRIDGE_NETWORK"
require_file "$MAC_BRIDGE_TESTS"
require_file "$MAC_BRIDGE_LIVE_E2E"

for file in "$BUILD_DOCS" "$ARCHITECTURE" "$KB" "$README"; do
  require_text "distributed protocol crate" "$file" "jarvis-protocol"
done
require_text "build docs Windows protocol workflow" "$BUILD_DOCS" "windows-protocol.yml"
require_text "build docs Windows rustup bootstrap" "$BUILD_DOCS" "rustup toolchain install 1.95.0 --profile minimal --component clippy --component rustfmt"
require_text "build docs Windows Cargo PATH" "$BUILD_DOCS" 'C:\Users\mike\.cargo\bin\cargo.exe'
require_text "architecture dormant distributed feature" "$ARCHITECTURE" "distributed-development"
require_text "architecture distributed non-goal" "$ARCHITECTURE" "This slice does not make Windows the runtime authority yet."
require_text "knowledge base Windows protocol workflow" "$KB" "windows-protocol.yml"
require_text "knowledge base phase completion rule" "$KB" "A feature or phase is not complete"
require_text "knowledge base Windows Cargo PATH" "$KB" 'C:\Users\mike\.cargo\bin\cargo.exe'
require_text "distributed design Windows authority" "$DISTRIBUTED_DESIGN" 'jarvis-master` is the sole authoritative service'
require_text "distributed design Codex account boundary" "$DISTRIBUTED_DESIGN" "Codex account"
require_text "distributed design current-default boundary" "$DISTRIBUTED_DESIGN" "current app-supervised Mac architecture remains the release default"
require_text "protocol version constant" "$PROTOCOL_CRATE" "pub const PROTOCOL_VERSION: u16 = 1;"
require_text "protocol frame bound" "$PROTOCOL_CRATE" "pub const MAX_WIRE_FRAME_BYTES: usize = 1024 * 1024;"
require_text "protocol bound-before-decode API" "$PROTOCOL_CRATE" "pub fn decode_frame(frame: &[u8])"
require_text "protocol nil identity rejection" "$PROTOCOL_CRATE" "NilIdentifier"
require_text "release version protocol package check" "$RELEASE_VERSION_SCRIPT" 'crates/jarvis-protocol/Cargo.toml'
require_text "release version master package check" "$RELEASE_VERSION_SCRIPT" 'crates/jarvis-master/Cargo.toml'
require_text "release version master dependency check" "$RELEASE_VERSION_SCRIPT" "MASTER_PROTOCOL_DEPENDENCY_VERSION"
require_text "release version protocol dependency check" "$RELEASE_VERSION_SCRIPT" "CORE_PROTOCOL_DEPENDENCY_VERSION"
require_text "protocol E2E master-worker story" "$PROTOCOL_E2E" "windows_master_and_mac_worker_complete_one_bounded_protocol_story"
require_text "protocol E2E wrong-lease denial" "$PROTOCOL_E2E" "ResultIdentityMismatch"
require_text "master schema version" "$MASTER_CRATE" "pub const MASTER_SCHEMA_VERSION: i64 = 2;"
require_text "master queue ceiling" "$MASTER_CRATE" "pub const MAX_QUEUED_OR_LEASED_STEPS: u64 = 256;"
require_text "master E2E durable story" "$MASTER_E2E" "windows_master_kernel_accepts_fake_worker_result_durably"
require_text "master E2E restart story" "$MASTER_E2E" "windows_master_kernel_reconciles_fake_worker_across_restart"
require_text "master process loopback restriction" "$MASTER_PROCESS" "Windows master development transport must use a loopback address"
require_text "master process E2E" "$MASTER_PROCESS_E2E" "windows_master_process_owns_state_and_completes_cross_process_fixture"
require_text "master process E2E bearer non-disclosure" "$MASTER_PROCESS_E2E" "setup receipt exposed the development bearer"
require_text "master process E2E body bound" "$MASTER_PROCESS_E2E" "HTTP/1.1 413 Payload Too Large"
require_text "master DPAPI protector" "$MASTER_IDENTITY" "windows_dpapi_current_user"
require_text "master enrollment grant TTL" "$MASTER_IDENTITY" "pub const ENROLLMENT_GRANT_TTL_MS: u64 = 10 * 60 * 1_000;"
require_text "master enrolled device ceiling" "$MASTER_IDENTITY" "pub const MAX_ENROLLED_DEVICES: u64 = 16;"
require_text "master TLS exporter label" "$MASTER_PROCESS" "EXPORTER-Jarvis-Developer-Mode-v1"
require_text "master TLS 1.3 restriction" "$MASTER_PROCESS" "rustls::version::TLS13"
require_text "master remote mTLS E2E" "$MASTER_REMOTE_MTLS_E2E" "remote_listener_requires_enrollment_tls13_and_channel_bound_identity"
require_text "master SCM automatic start" "$MASTER_SERVICE_HOST" "ServiceStartType::AutoStart"
require_text "master SCM bounded recovery" "$MASTER_SERVICE_HOST" "Duration::from_secs(60)"
require_text "master maintenance admission block" "$MASTER_PROCESS" "maintenance_mode_blocks_new_work"
require_text "master service lifecycle E2E" "$MASTER_SERVICE_E2E" "windows_service_install_maintenance_recovery_and_uninstall_preserve_master_state"
require_text "master remote role-boundary E2E" "$MASTER_REMOTE_MTLS_E2E" "worker must not enqueue"
require_text "master service maintenance-restart E2E" "$MASTER_SERVICE_E2E" "maintenance must survive service restart"
require_text "master service direct-stop E2E" "$MASTER_SERVICE_E2E" '"service", "stop"'
require_text "master owner service logon right" "$MASTER_SERVICE_HOST" "SeServiceLogonRight"
require_text "master enrollment E2E" "$MASTER_IDENTITY_E2E" "enrollment_grants_issue_rotate_and_revoke_exact_device_identity"
require_text "master enrollment DPAPI E2E" "$MASTER_IDENTITY_E2E" "windows_dpapi_protector_round_trips_without_plaintext_equivalence"
require_text "master enrollment schema migration E2E" "$MASTER_IDENTITY_E2E" "schema_v1_migrates_transactionally_to_enrollment_identity_v2"
require_text "master secret-free pairing command" "$MASTER_PROCESS" "enrollment_pair"
require_text "protocol enrollment invitation" "$PROTOCOL_CRATE" "pub struct EnrollmentInvitation"
require_text "Mac bridge exporter binding" "$MAC_BRIDGE" "EXPORTER-Jarvis-Developer-Mode-v1"
require_text "Mac bridge device-only Keychain" "$MAC_BRIDGE_KEYCHAIN" "kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly"
require_text "Mac bridge certificate Keychain item" "$MAC_BRIDGE_KEYCHAIN" "kSecClassCertificate"
require_text "Mac bridge supported-SDK identity lookup" "$MAC_BRIDGE_KEYCHAIN" "SecIdentityCreateWithCertificate"
forbid_text "Mac bridge new-SDK-only identity constructor" "$MAC_BRIDGE_KEYCHAIN" "SecIdentityCreate(nil"
require_text "Mac bridge TLS 1.3" "$MAC_BRIDGE_NETWORK" "sec_protocol_options_set_min_tls_protocol_version"
require_text "Mac bridge focused tests" "$MAC_BRIDGE_TESTS" "DeveloperBridgeTests"
require_text "Mac bridge live E2E authenticated marker" "$MAC_BRIDGE_LIVE_E2E" "jarvis_mac_windows_bridge_live_e2e_ok"
require_text "Mac bridge live E2E Keychain precondition" "$MAC_BRIDGE_LIVE_E2E" "not installed in Keychain"
require_text "local release Mac bridge E2E preflight" "$LOCAL_GATE" "./scripts/mac-windows-bridge-live-e2e.sh --check"
for file in "$BUILD_DOCS" "$CHECKLIST" "$ARCHITECTURE" "$KB"; do
  require_text "distributed protocol E2E documentation" "$file" "distributed_protocol_contract_e2e"
  require_text "distributed master E2E documentation" "$file" "master_lifecycle_e2e"
  require_text "distributed master process E2E documentation" "$file" "master_process_e2e"
  require_text "distributed enrollment identity E2E documentation" "$file" "enrollment_identity_e2e"
  require_text "distributed remote mTLS E2E documentation" "$file" "remote_mtls_e2e"
  require_text "distributed Windows service E2E documentation" "$file" "windows_service_lifecycle_e2e"
done
require_text "Windows protocol runner" "$WINDOWS_PROTOCOL_WORKFLOW" "runs-on: windows-latest"
require_text "Windows protocol tests" "$WINDOWS_PROTOCOL_WORKFLOW" "cargo test -p jarvis-protocol --locked"
require_text "Windows master tests" "$WINDOWS_PROTOCOL_WORKFLOW" "cargo test -p jarvis-master --locked"
require_text "Windows service E2E required flag" "$WINDOWS_PROTOCOL_WORKFLOW" 'JARVIS_REQUIRE_WINDOWS_SERVICE_E2E: "1"'
require_text "Windows service E2E command" "$WINDOWS_PROTOCOL_WORKFLOW" "cargo test -p jarvis-master --test windows_service_lifecycle_e2e --locked -- --ignored --nocapture"
require_text "Windows owner service right command" "$WINDOWS_PROTOCOL_WORKFLOW" "service_logon_right_is_ensured_for_current_account"

for file in "$BUILD_DOCS" "$CHECKLIST" "$README"; do
  require_text "atomic approval decision" "$file" "redacted decision audit"
  require_text "approval decision rollback" "$file" "back to pending"
  require_text "approval decision authority chain" "$file" "unaudited grant"
  require_text "atomic approval claim" "$file" "unique durable execution claim"
  require_text "approval replay conflict" "$file" "conflict/HTTP 409"
  require_text "approval ambiguous-effect boundary" "$file" "effect ambiguous"
  require_text "approval no automatic retry boundary" "$file" "automatic retry is forbidden"
  require_text "approval grant-chain evidence" "$file" "approval_granted audit evidence"
  require_text "approval legacy grant compatibility" "$file" "legacy raw-metadata"
done

for file in "$BUILD_DOCS" "$CHECKLIST" "$ARCHITECTURE" "$KB" "$README"; do
  require_text "diagnostics pause-reason redaction contract" "$file" "emergency_pause_reason_present"
done
require_text "core diagnostics pause-reason redaction contract" "$CORE_IPC" "emergency_pause_reason_present"
forbid_text "core stale diagnostics redaction statement" "$CORE_IPC" "memory values, and cancellation reason text"

for file in "$ARCHITECTURE" "$KB"; do
  require_text "atomic approval decision" "$file" "redacted decision audit"
  require_text "approval decision rollback" "$file" "back to pending"
  require_text "approval decision authority chain" "$file" "unaudited grant"
  require_text "approval grant-chain evidence" "$file" "approval_granted"
  require_text "approval legacy grant compatibility" "$file" "legacy raw-metadata"
done
require_text "approval decision storage proof" "$BUILD_DOCS" "approval_decision_and_redacted_audit_commit_or_roll_back_together"
require_text "approval decision IPC proof" "$BUILD_DOCS" "approval_decision_audit_failure_rolls_back_across_cli_ipc_and_restart"
require_text "approval grant-chain storage proof" "$BUILD_DOCS" "approved_row_without_matching_grant_audit_cannot_be_claimed"
require_text "approval legacy grant storage proof" "$BUILD_DOCS" "matching_legacy_raw_metadata_grant_audit_remains_claimable"
require_text "approval grant-chain IPC proof" "$BUILD_DOCS" "approved_row_without_grant_audit_cannot_claim_or_enter_plugin_across_restart"

for file in "$BUILD_DOCS" "$CHECKLIST" "$ARCHITECTURE" "$KB"; do
  require_text "active command cancellation handle" "$file" "cancellation_id"
  require_text "active command cancellation endpoint" "$file" "/runtime/cancellations/:id"
  require_text "active command cancellation active evidence" "$file" "cancellation_requested"
  require_text "active command cancellation not-found evidence" "$file" "not_found"
  require_text "active command cancellation tombstones" "$file" "1,024"
  require_text "active command cancellation tombstone lifecycle" "$file" "process-local"
done
require_text "active command cancellation safety" "$SAFETY_RULES" "result-acceptance"
require_text "active command cancellation design" "$DESIGN" "client-generated cancellation handle"
require_text "active command cancellation core test" "$BUILD_DOCS" "explicit_command_handle_cancels_only_its_active_model_transport"
require_text "active command cancellation E2E" "$BUILD_DOCS" "active_command_cancellation_is_end_to_end_and_finalized_handles_report_not_found"
require_text "active command cancellation Swift test" "$BUILD_DOCS" "commandConsoleCancelsItsActiveSubmission"
require_text "active command cancellation concurrent Swift test" "$CHECKLIST" "commandConsoleSerializesConcurrentSubmissions"

require_text "core app-supervised IPC audit-token proof" "$CORE_IPC" "LOCAL_PEERTOKEN"
require_text "core app-supervised IPC Security.framework proof" "$CORE_IPC" "Security.framework designated requirement"
require_text "core app-supervised IPC wrong-code proof" "$CORE_IPC" "same-EUID wrong-code pre-frame rejection"
require_text "core app-supervised IPC default transport proof" "$CORE_IPC" "default owner-only Unix socket plus memory-only bearer path has no TCP listener or credential handoff file"
for file in "$BUILD_DOCS" "$CHECKLIST" "$KB"; do
  require_text "app-supervised parent PID contract" "$file" "--supervised-parent-pid"
done
require_text "app-supervised parent lifetime design" "$DESIGN" "watches the relationship"
require_text "app-supervised parent lifetime safety" "$SAFETY_RULES" "database owner lease"
require_text "app-supervised parent lifetime architecture" "$ARCHITECTURE" "core self-exit; UDS and database lease release; same-DB relaunch"
require_text "app-supervised crash E2E" "$BUILD_DOCS" "Abrupt app termination"
require_text "orphaned core diagnostic" "$KB" "parent PID 1"
require_text "core approval grant-chain proof" "$CORE_IPC" "matching approval_granted audit evidence"
require_text "core approval no-fabrication boundary" "$CORE_IPC" "never fabricates grant evidence"
forbid_text "obsolete same-EUID code-sign boundary" "$CORE_IPC" "Same-EUID plus bearer defense in depth does not prove peer PID, intended process or code-sign identity"
forbid_text "stale schema fixture range" "$README" "schema v1-v11 fixtures"
forbid_text "stale schema fixture range" "$BUILD_DOCS" "schema v1-v11"
forbid_text "stale schema fixture range" "$CHECKLIST" "schema v1-v11"

while IFS= read -r command; do
  require_text "build/test command docs" "$BUILD_DOCS" "$command"
  require_text "release checklist" "$CHECKLIST" "$command"
done < <(grep -E '^[[:space:]]*run ' "$LOCAL_GATE" | sed -E 's/^[[:space:]]*run //')

for file in "$BUILD_DOCS" "$CHECKLIST" "$ARCHITECTURE" "$KB" "$README"; do
  require_text "release evidence-mode boundary" "$file" "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external"
  require_text "release evidence-mode field" "$file" "evidence_mode_enabled"
  require_text "release command evidence reference" "$file" "task:<uuid>"
  require_text "release command evidence reference" "$file" "audit:<uuid>"
  require_text "owner evidence boundary" "$file" "owner-recorded external evidence"
  require_text "external handoff script" "$file" "release-external-handoff.sh"
done

for file in "$BUILD_DOCS" "$CHECKLIST" "$ARCHITECTURE" "$KB" "$README"; do
  require_text "external handoff checklist" "$file" "release-evidence-checklist.md"
  require_text "external handoff manifest" "$file" "release-handoff-manifest.json"
done

require_text "architecture current diagram" "$ARCHITECTURE" "Current Implementation And Evidence Boundary Diagram"
require_text "architecture end-state diagram" "$ARCHITECTURE" "End-Goal Production Architecture"
require_text "architecture plugin update preview" "$ARCHITECTURE" "/plugins/installed/:id/update/preview"
require_text "architecture plugin update apply" "$ARCHITECTURE" "/plugins/installed/:id/update/apply"
require_text "architecture plugin lifecycle history" "$ARCHITECTURE" "/plugins/installed/:id/history"
require_text "architecture plugin candidate binding" "$ARCHITECTURE" "candidate_update_contract_sha256"
require_text "architecture plugin lifecycle binding" "$ARCHITECTURE" "current_lifecycle_contract_sha256"
require_text "knowledge base plugin candidate binding" "$KB" "candidate_update_contract_sha256"
require_text "knowledge base plugin update trust boundary" "$KB" "not a raw component provenance hash"
require_text "knowledge base plugin legacy version transition" "$KB" "persisted pre-SemVer record may cross once"
require_text "build docs plugin update preview CLI" "$BUILD_DOCS" "plugins update-preview"
require_text "build docs plugin update apply CLI" "$BUILD_DOCS" "plugins update-apply"
require_text "build docs plugin update lifecycle flag" "$BUILD_DOCS" "--expected-lifecycle-contract-sha256"
require_text "build docs plugin update candidate flag" "$BUILD_DOCS" "--expected-candidate-update-contract-sha256"
require_text "build docs plugin history CLI" "$BUILD_DOCS" "plugins history"
require_text "build docs plugin update client proof" "$BUILD_DOCS" "pluginUpdateClientUsesTypedRedactedContracts"
require_text "build docs plugin update manager proof" "$BUILD_DOCS" "pluginManagerUpdateRequiresPreviewAndConfirmation"
require_text "build docs plugin history failure proof" "$BUILD_DOCS" "pluginLifecycleHistoryFailureDoesNotStaleRegistry"
require_text "build docs plugin update cross-process E2E" "$BUILD_DOCS" "installed_plugin_update_preview_apply_history_is_cas_bound_redacted_and_persistent"
require_text "checklist plugin update cross-process E2E" "$CHECKLIST" "installed_plugin_update_preview_apply_history_is_cas_bound_redacted_and_persistent"
require_text "knowledge base plugin update cross-process E2E" "$KB" "installed_plugin_update_preview_apply_history_is_cas_bound_redacted_and_persistent"
require_text "checklist plugin update preview CLI" "$CHECKLIST" "plugins update-preview"
require_text "checklist plugin update apply CLI" "$CHECKLIST" "plugins update-apply"
require_text "checklist plugin history CLI" "$CHECKLIST" "plugins history"
require_text "architecture manual evidence boundary" "$ARCHITECTURE" "Manual external evidence, not local gate proof"
require_text "architecture signed artifact boundary" "$ARCHITECTURE" "Developer ID signed, notarized, and stapled"
require_text "architecture clean-profile QA" "$ARCHITECTURE" "clean-profile install and Finder/LaunchServices"
require_text "architecture manual installed-app QA" "$ARCHITECTURE" "manual installed-app command, audit, memory, scheduler, plugin, pause, diagnostics, notifications, restart QA"
require_text "architecture notification QA" "$ARCHITECTURE" "live macOS notification prompt and delivery QA"
require_text "architecture repository command evidence" "$ARCHITECTURE" "repository-backed task/audit command-result evidence"
require_text "architecture final archive evidence" "$ARCHITECTURE" "archived final release evidence bundle"
require_text "architecture local gate boundary" "$ARCHITECTURE" "does not perform signing,"
require_text "architecture local gate boundary" "$ARCHITECTURE" "notarization, clean-profile install, Finder/LaunchServices launch, live-device"
require_text "architecture local gate boundary" "$ARCHITECTURE" "QA, plugin-trust QA, or final evidence bundling"
require_text "architecture post-merge cleanup audit" "$ARCHITECTURE" "post-merge cleanup audit: open PRs, main workflow runs, worktrees, merged/unmerged codex branches, clean checkout"
require_text "architecture current readiness baseline" "$ARCHITECTURE" 'latest verified main baseline'
require_text "architecture current readiness commit" "$ARCHITECTURE" '`main` commit `042c60e`'
require_text "architecture current readiness run" "$ARCHITECTURE" '29344743720'
require_text "architecture current readiness job" "$ARCHITECTURE" '87125361398'
require_text "architecture current readiness refresh command" "$ARCHITECTURE" 'cargo run -p jarvis-cli -- release readiness --json'
require_text "architecture current readiness refresh command" "$ARCHITECTURE" 'gh run list --branch main --workflow "Jarvis Release Local Gate"'
require_text "architecture microphone privacy prompt" "$ARCHITECTURE" 'Jarvis uses microphone input only when you explicitly start local voice capture.'
require_text "architecture Speech privacy prompt" "$ARCHITECTURE" 'Jarvis uses speech recognition only to turn your spoken command into a local assistant request.'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `051ec49`'
forbid_text "architecture stale readiness run" "$ARCHITECTURE" '28041417362'
forbid_text "architecture stale readiness job" "$ARCHITECTURE" '83008348690'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `7f7b543`'
forbid_text "architecture stale readiness run" "$ARCHITECTURE" '28037502202'
forbid_text "architecture stale readiness job" "$ARCHITECTURE" '82994684590'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `8d61ad7`'
forbid_text "architecture stale readiness run" "$ARCHITECTURE" '27849385053'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #312 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `4b36c14`'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `73dbd54`'
forbid_text "architecture stale readiness run" "$ARCHITECTURE" '27847887169'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #311 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `4417187`'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #310 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `27c33f5`'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #309 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `38bd79e`'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #308 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `af943e8`'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #301 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `155ccd4`'
require_text "architecture model-step progress boundary" "$ARCHITECTURE" "model-step, and redacted model-output chunk progress frames"
require_text "architecture model-output progress boundary" "$ARCHITECTURE" "redacted model-output chunk progress frames"
require_text "architecture handoff manifest digest" "$ARCHITECTURE" "handoff manifest digest-binding"
require_text "architecture handoff manifest self-test" "$ARCHITECTURE" "--self-test\` verifies the expected file list"
require_text "architecture release-local heartbeat" "$ARCHITECTURE" "release-local command heartbeat"
require_text "architecture final bundle output collision guard" "$ARCHITECTURE" "final bundle writer must also reject"
require_text "architecture final bundle output collision guard" "$ARCHITECTURE" "output paths that collide with signed-provenance"
require_text "knowledge base current readiness baseline" "$KB" 'latest verified main baseline at `042c60e`'
require_text "knowledge base current readiness run" "$KB" '29344743720'
require_text "knowledge base current readiness job" "$KB" '87125361398'
require_text "knowledge base current readiness refresh command" "$KB" 'cargo run -p jarvis-cli -- release readiness --json'
require_text "knowledge base current readiness refresh command" "$KB" 'gh run list --branch main --workflow "Jarvis Release Local Gate"'
require_text "knowledge base microphone privacy prompt" "$KB" 'Jarvis uses microphone input only when you explicitly start local voice capture.'
require_text "knowledge base Speech privacy prompt" "$KB" 'Jarvis uses speech recognition only to turn your spoken command into a local assistant request.'
forbid_text "knowledge base stale readiness baseline" "$KB" 'latest verified main baseline at `051ec49`'
forbid_text "knowledge base stale readiness run" "$KB" '28041417362'
forbid_text "knowledge base stale readiness job" "$KB" '83008348690'
forbid_text "knowledge base stale readiness baseline" "$KB" 'latest verified main baseline at `7f7b543`'
forbid_text "knowledge base stale readiness run" "$KB" '28037502202'
forbid_text "knowledge base stale readiness job" "$KB" '82994684590'
forbid_text "knowledge base stale readiness baseline" "$KB" 'latest verified main baseline at `8d61ad7`'
forbid_text "knowledge base stale readiness run" "$KB" '27849385053'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #312 baseline at `4b36c14`'
forbid_text "knowledge base stale readiness baseline" "$KB" 'latest verified main baseline at `73dbd54`'
forbid_text "knowledge base stale readiness run" "$KB" '27847887169'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #311 baseline at `4417187`'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #310 baseline at `27c33f5`'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #309 baseline at `38bd79e`'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #308 baseline at `af943e8`'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #301 baseline at `155ccd4`'
require_text "knowledge base model-step progress boundary" "$KB" "model-step completion/failure audit evidence"
require_text "knowledge base model-output progress boundary" "$KB" "content_redacted: true"
require_text "knowledge base model-step progress proof" "$KB" "installed-plugin plus model-step/model-output"
require_text "knowledge base model-output progress proof" "$KB" "model-step/model-output"
require_text "knowledge base handoff manifest digest" "$KB" "manifest digests match"
require_text "knowledge base handoff manifest self-test" "$KB" "shell self-test verifies the expected file list"
require_text "knowledge base handoff evidence-status parity" "$KB" "external-mode direct CLI evidence-status query"
require_text "knowledge base missing live-device field E2E" "$KB" "removes required"
require_text "knowledge base missing live-device field E2E" "$KB" "notification-observation fields"
require_text "knowledge base live-device script E2E" "$KB" "release-live-device-qa.sh --assert-complete"
require_text "knowledge base live-device script E2E" "$KB" "script-generated live-device QA"
require_text "knowledge base plugin-trust script E2E" "$KB" "release-plugin-trust-qa.sh --assert-complete"
require_text "knowledge base plugin-trust script E2E" "$KB" "plugin-trust QA report"
require_text "knowledge base release-local heartbeat" "$KB" "release-local command heartbeat"
require_text "knowledge base final bundle output collision guard" "$KB" "final bundle output path must also be distinct"
require_text "architecture live-device script E2E" "$ARCHITECTURE" "script-generated live-device QA report"
require_text "architecture live-device script E2E" "$ARCHITECTURE" "live-device evidence, not automated"
require_text "architecture plugin-trust script E2E" "$ARCHITECTURE" "rebinds the generated"
require_text "architecture plugin-trust script E2E" "$ARCHITECTURE" "bundle as present"
require_text "build docs model-step progress command" "$BUILD_DOCS" "repository_backed_state_endpoints_expose_tasks_and_audit"
require_text "build docs handoff manifest digest" "$BUILD_DOCS" "per-file SHA-256 digests"
require_text "build docs handoff manifest self-test" "$BUILD_DOCS" "handoff shell self-test verifies the expected manifest file list"
require_text "build docs handoff evidence-status parity" "$BUILD_DOCS" "fresh external-mode direct CLI evidence-status query"
require_text "build docs missing live-device field E2E" "$BUILD_DOCS" "removes required live voice"
require_text "build docs missing live-device field E2E" "$BUILD_DOCS" "external-mode"
require_text "build docs missing live-device field E2E" "$BUILD_DOCS" "readiness fail closed"
require_text "build docs live-device script E2E" "$BUILD_DOCS" "release_live_device_qa_script_generated_report_clears_evidence_status"
require_text "build docs live-device script E2E" "$BUILD_DOCS" "script-generated live-device QA"
require_text "build docs plugin-trust script E2E" "$BUILD_DOCS" "release_plugin_trust_qa_assertion_report_is_accepted_by_evidence_status"
require_text "build docs plugin-trust script E2E" "$BUILD_DOCS" "plugin-trust QA report"
require_text "build docs downstream plugin URI hardening" "$BUILD_DOCS" "evidence_artifacts.*.uri"
require_text "knowledge base downstream plugin URI hardening" "$KB" "temporary plugin artifact URIs"
require_text "architecture downstream plugin URI hardening" "$ARCHITECTURE" "per-category artifact URI bindings"
require_text "build docs evidence-bundle runbook E2E" "$BUILD_DOCS" "release_evidence_bundle_runbook_summarizes_next_operator_steps"
require_text "build docs normalized runbook endpoint E2E" "$BUILD_DOCS" "release_runbook_ipc_endpoints_emit_normalized_core_json"
require_text "build docs evidence-bundle runbook snapshot" "$BUILD_DOCS" "evidence-bundle-runbook.json"
require_text "knowledge base evidence-bundle runbook" "$KB" "/release/evidence-bundle-runbook"
require_text "knowledge base evidence-bundle handoff snapshot" "$KB" "evidence-bundle-runbook.json"
require_text "architecture evidence-bundle runbook endpoint" "$ARCHITECTURE" "/release/evidence-bundle-runbook"
require_text "architecture evidence-bundle handoff snapshot" "$ARCHITECTURE" "evidence-bundle-runbook.json"
require_text "release checklist evidence-bundle runbook endpoint" "$CHECKLIST" "/release/evidence-bundle-runbook"
require_text "release checklist evidence-bundle handoff snapshot" "$CHECKLIST" "evidence-bundle-runbook.json"
require_text "build docs release-local heartbeat" "$BUILD_DOCS" "JARVIS_RELEASE_LOCAL_HEARTBEAT_SECONDS"
require_text "build docs release-local heartbeat self-test" "$BUILD_DOCS" "--heartbeat-self-test"
require_text "build docs final bundle output collision guard" "$BUILD_DOCS" "final bundle output path must also be distinct"
require_text "release checklist handoff manifest digest" "$CHECKLIST" "byte counts"
require_text "release checklist model-output chunk boundary" "$CHECKLIST" "model_output_chunk"
require_text "release checklist model-output redaction boundary" "$CHECKLIST" "content_redacted: true"
require_text "release checklist final bundle output collision guard" "$CHECKLIST" "bundle output path is distinct from the signed-distribution provenance"
require_text "release checklist missing live-device field E2E" "$CHECKLIST" "Missing required live voice evidence"
require_text "release checklist missing live-device field E2E" "$CHECKLIST" "external-mode readiness fail closed"
require_text "release checklist live-device script E2E" "$CHECKLIST" "script-generated live-device QA report"
require_text "release checklist live-device script E2E" "$CHECKLIST" "script/status/readiness compatibility"
require_text "release checklist plugin-trust script E2E" "$CHECKLIST" "generated plugin-trust QA report and bundle as present"
require_text "build docs external handoff snapshot command" "$BUILD_DOCS" "release_external_handoff_snapshots_match_live_runbook_commands"
for file in "$BUILD_DOCS" "$CHECKLIST" "$KB"; do
  require_text "runbook payload contract boundary" "$file" "operator/snapshot JSON"
  require_text "runbook payload contract boundary" "$file" "ReleaseRunbookResponse"
  require_text "post-merge cleanup audit" "$file" "gh pr list --state open --json number,title,headRefName,baseRefName,url"
  require_text "post-merge cleanup audit" "$file" "gh run list --workflow release-local.yml --branch main --limit 5"
  require_text "post-merge cleanup audit" "$file" "git worktree list --porcelain"
  require_text "post-merge cleanup audit" "$file" "git branch --merged main --list 'codex/*'"
  require_text "post-merge cleanup audit" "$file" "git branch --no-merged main --list 'codex/*'"
  require_text "post-merge cleanup audit" "$file" "git status --short --branch"
done
require_text "release readiness blocker docs" "$CHECKLIST" "live-device QA"
require_text "release readiness blocker docs" "$CHECKLIST" "plugin-trust QA"
require_text "release readiness blocker docs" "$CHECKLIST" "Developer ID"
require_text "design app executable identity binding" "$DESIGN" "TeamIdentifier, and CDHash"
require_text "safety app executable identity binding" "$SAFETY_RULES" "continuous runtime integrity"
require_text "readme signed app executable identity" "$README" "structured code Identifier, TeamIdentifier"
require_text "build docs signed provenance live QA binding" "$BUILD_DOCS" "JARVIS_QA_SIGNED_PROVENANCE_REPORT"
require_text "release checklist app identity drift" "$CHECKLIST" "reject cross-report artifact or identity drift"
require_text "architecture current app identity binding" "$ARCHITECTURE" "app executable path/SHA-256 plus Identifier, TeamIdentifier, and CDHash"
require_text "architecture target app identity binding" "$ARCHITECTURE" "point-in-time installed executable and signed-provenance binding"
require_text "knowledge base app identity binding" "$KB" "Signed distribution and live-device evidence are now joined by the exact app"
require_text "core release proof boundary app identity" "$CORE_IPC" "exact app-executable SHA-256/code-identity"

printf 'Jarvis release docs drift smoke: ok\n'
