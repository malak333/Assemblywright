#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOCAL_GATE="scripts/release-local.sh"
BUILD_DOCS="docs/build-test-commands.md"
CHECKLIST="docs/release-checklist.md"
ARCHITECTURE="docs/architecture-map.md"
KB="docs/knowledge-base/assemblywright-project-facts.md"
README="README.md"
BRAND="docs/brand.md"
LICENSE_FILE="LICENSE"
DESIGN="DESIGN.md"
AGENTS="AGENTS.md"
SAFETY_RULES="docs/safety-rules.md"
DISTRIBUTED_DESIGN="docs/distributed-developer-mode-design.md"
FEATURE_CONVEYOR_DESIGN="docs/feature-conveyor-design.md"
AGENT_WORKFLOW="docs/development-agent-workflow.md"

IPC_TRANSPORT="crates/assemblywright-core/src/ipc_transport.rs"
CORE_STARTUP="crates/assemblywright-core/src/startup.rs"
CORE_RELEASE="crates/assemblywright-core/src/release.rs"
PROTOCOL_CRATE="crates/assemblywright-protocol/src/lib.rs"
MASTER_CRATE="crates/assemblywright-master/src/lib.rs"
MASTER_PROCESS="crates/assemblywright-master/src/main.rs"
MASTER_IDENTITY="crates/assemblywright-master/src/identity.rs"
MASTER_SERVICE_HOST="crates/assemblywright-master/src/windows_service_host.rs"
AGENT_CRATE="crates/assemblywright-agent/src/lib.rs"
AGENT_PROCESS="crates/assemblywright-agent/src/main.rs"
AGENT_SNAPSHOT="crates/assemblywright-agent/src/snapshot.rs"
CLI_MAIN="crates/assemblywright-cli/src/main.rs"

PROTOCOL_E2E="crates/assemblywright-protocol/tests/distributed_protocol_contract_e2e.rs"
PROTOCOL_EVENT_E2E="crates/assemblywright-protocol/tests/distributed_event_cursor_contract.rs"
PROTOCOL_MLX_E2E="crates/assemblywright-protocol/tests/mlx_job_contract.rs"
PROTOCOL_LOCAL_CODING_E2E="crates/assemblywright-protocol/tests/local_coding_contract.rs"
PROTOCOL_ARTIFACT_INTEGRATION_E2E="crates/assemblywright-protocol/tests/artifact_integration_contract.rs"
PROTOCOL_OWNER_RESOLUTION_E2E="crates/assemblywright-protocol/tests/owner_resolution_contract.rs"
MASTER_E2E="crates/assemblywright-master/tests/master_lifecycle_e2e.rs"
MASTER_PROCESS_E2E="crates/assemblywright-master/tests/master_process_e2e.rs"
MASTER_IDENTITY_E2E="crates/assemblywright-master/tests/enrollment_identity_e2e.rs"
MASTER_REMOTE_MTLS_E2E="crates/assemblywright-master/tests/remote_mtls_e2e.rs"
MASTER_EVENT_E2E="crates/assemblywright-master/tests/event_cursor_e2e.rs"
MASTER_CONVEYOR_E2E="crates/assemblywright-master/tests/feature_conveyor_kernel.rs"
MASTER_ARTIFACT_INTEGRATION="crates/assemblywright-master/src/integration.rs"
MASTER_ARTIFACT_INTEGRATION_E2E="crates/assemblywright-master/tests/artifact_integration_e2e.rs"
MASTER_SERVICE_E2E="crates/assemblywright-master/tests/windows_service_lifecycle_e2e.rs"
AGENT_E2E="crates/assemblywright-agent/tests/local_relay_e2e.rs"
AGENT_LOCAL_CODING_E2E="crates/assemblywright-agent/tests/local_coding_admission.rs"
CLI_NAMING_E2E="crates/assemblywright-cli/tests/naming_contract_e2e.rs"
CLI_READINESS_E2E="crates/assemblywright-cli/tests/release_readiness_e2e.rs"
AGENT_MAIN="crates/assemblywright-agent/src/main.rs"

MAC_BRIDGE="apps/mac/Sources/AssemblywrightMacCore/DeveloperBridge.swift"
MAC_BRIDGE_CLI="apps/mac/Sources/AssemblywrightMacBridgeCLI/AssemblywrightMacBridgeCLI.swift"
MAC_BRIDGE_KEYCHAIN="apps/mac/Sources/AssemblywrightMacCore/KeychainDeveloperIdentity.swift"
MAC_BRIDGE_NETWORK="apps/mac/Sources/AssemblywrightMacCore/NetworkMTLSBridge.swift"
MAC_BRIDGE_SUPERVISOR="apps/mac/Sources/AssemblywrightMacCore/DeveloperBridgeSupervisor.swift"
MAC_OWNER_CONTROL="apps/mac/Sources/AssemblywrightMacCore/FeatureConveyorOwnerControl.swift"
MAC_BRIDGE_PROCESS="apps/mac/Sources/AssemblywrightMacCore/DeveloperBridgeProcessLifecycle.swift"
MAC_EVENT_RELAY="apps/mac/Sources/AssemblywrightMacCore/DeveloperEventRelay.swift"
MAC_APP="apps/mac/Sources/AssemblywrightMacApp/AssemblywrightMacApp.swift"
MAC_BRIDGE_TESTS="apps/mac/Tests/AssemblywrightMacCoreTests/DeveloperBridgeTests.swift"
MAC_APP_TESTS="apps/mac/Tests/AssemblywrightMacAppTests/AssemblywrightMacAppTests.swift"

MAC_BRIDGE_LIVE_E2E="scripts/mac-windows-bridge-live-e2e.sh"
MAC_LOCAL_CODING_SNAPSHOT_E2E="scripts/mac-local-coding-snapshot-e2e.sh"
WINDOWS_FIXTURE_LIVE_CONTROL="scripts/windows-fixture-live-control.ps1"
WINDOWS_MLX_LIVE_CONTROL="scripts/windows-mlx-live-control.ps1"
WINDOWS_LOCAL_CODING_LIVE_CONTROL="scripts/windows-local-coding-live-control.ps1"
WINDOWS_LOCAL_CODING_LIVE_CONTROL_SELF_CHECK="scripts/windows-local-coding-live-control-self-check.sh"
WINDOWS_PROTOCOL_WORKFLOW=".github/workflows/windows-protocol.yml"
RELEASE_VERSION_SCRIPT="scripts/release-version.sh"
NAMING_CONTRACT_SMOKE="scripts/release-naming-contract-smoke.sh"
PROTOCOL_VERSION_CONTRACT_SMOKE="scripts/release-protocol-version-contract-smoke.sh"

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_absent() {
  [[ ! -e "$1" ]] || fail "removed surface reappeared: $1"
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

for file in \
  "$LOCAL_GATE" "$BUILD_DOCS" "$CHECKLIST" "$ARCHITECTURE" "$KB" "$README" \
  "$BRAND" "$LICENSE_FILE" "$DESIGN" "$AGENTS" "$SAFETY_RULES" \
  "$DISTRIBUTED_DESIGN" "$FEATURE_CONVEYOR_DESIGN" "$AGENT_WORKFLOW" \
  "$IPC_TRANSPORT" "$CORE_STARTUP" "$CORE_RELEASE" "$PROTOCOL_CRATE" \
  "$MASTER_CRATE" "$MASTER_PROCESS" "$MASTER_IDENTITY" "$MASTER_SERVICE_HOST" \
  "$AGENT_CRATE" "$AGENT_PROCESS" "$CLI_MAIN" \
  "$PROTOCOL_E2E" "$PROTOCOL_EVENT_E2E" "$PROTOCOL_MLX_E2E" \
  "$PROTOCOL_LOCAL_CODING_E2E" "$PROTOCOL_ARTIFACT_INTEGRATION_E2E" \
  "$PROTOCOL_OWNER_RESOLUTION_E2E" \
  "$MASTER_E2E" "$MASTER_PROCESS_E2E" "$MASTER_IDENTITY_E2E" \
  "$MASTER_REMOTE_MTLS_E2E" "$MASTER_EVENT_E2E" "$MASTER_CONVEYOR_E2E" \
  "$MASTER_ARTIFACT_INTEGRATION" "$MASTER_ARTIFACT_INTEGRATION_E2E" \
  "$MASTER_SERVICE_E2E" "$AGENT_E2E" "$AGENT_LOCAL_CODING_E2E" "$CLI_NAMING_E2E" \
  "$CLI_READINESS_E2E" \
  "$MAC_BRIDGE" "$MAC_BRIDGE_CLI" "$MAC_BRIDGE_KEYCHAIN" "$MAC_BRIDGE_NETWORK" \
  "$MAC_BRIDGE_SUPERVISOR" "$MAC_OWNER_CONTROL" "$MAC_BRIDGE_PROCESS" "$MAC_EVENT_RELAY" \
  "$MAC_APP" "$MAC_BRIDGE_TESTS" "$MAC_APP_TESTS" \
  "$MAC_BRIDGE_LIVE_E2E" "$MAC_LOCAL_CODING_SNAPSHOT_E2E" \
  "$WINDOWS_FIXTURE_LIVE_CONTROL" \
  "$WINDOWS_MLX_LIVE_CONTROL" "$WINDOWS_LOCAL_CODING_LIVE_CONTROL" \
  "$WINDOWS_LOCAL_CODING_LIVE_CONTROL_SELF_CHECK" "$WINDOWS_PROTOCOL_WORKFLOW" \
  "$RELEASE_VERSION_SCRIPT" "$NAMING_CONTRACT_SMOKE"; do
  require_file "$file"
done

# The pre-pivot assistant surface must stay removed. Reintroducing any of these
# paths means a document or a change is describing a product that no longer
# exists.
for path in \
  crates/assemblywright-core/src/ipc.rs \
  crates/assemblywright-core/src/storage.rs \
  crates/assemblywright-core/src/runtime.rs \
  crates/assemblywright-core/src/model.rs \
  crates/assemblywright-core/src/router.rs \
  crates/assemblywright-core/src/policy.rs \
  crates/assemblywright-core/src/plugin.rs \
  crates/assemblywright-core/src/wasm_plugin.rs \
  crates/assemblywright-core/src/memory_index.rs \
  crates/assemblywright-core/src/scheduler.rs \
  crates/assemblywright-core/src/trusted_wake.rs \
  crates/assemblywright-core/src/workspace.rs \
  crates/assemblywright-cli/tests/local_ipc_e2e.rs \
  apps/mac/Sources/AssemblywrightMacCore/VoiceAdapter.swift \
  apps/mac/Sources/AssemblywrightMacCore/SpeechOutputAdapter.swift \
  apps/mac/Sources/AssemblywrightMacCore/TrustedWake.swift \
  apps/mac/Sources/AssemblywrightMacCore/CoreSupervisor.swift \
  apps/mac/Sources/AssemblywrightMacCore/AssemblywrightIPCClient.swift \
  apps/mac/Sources/AssemblywrightMacCore/ManagementModels.swift \
  docs/plugin-contract.md \
  scripts/release-plugin-trust-qa.sh \
  scripts/release-operator-qa-smoke.sh; do
  require_absent "$path"
done

require_text "README product name" "$README" "# Assemblywright"
require_text "README positioning" "$README" "Orchestrated intelligence. Verified software."
require_text "README license" "$README" "Apache License 2.0"
require_text "README conveyor framing" "$README" "owner-approved feature queue"
require_text "README non-claims" "$README" "Autonomous dispatch"

require_text "DESIGN conveyor pointer" "$DESIGN" "docs/feature-conveyor-design.md"
require_text "DESIGN distributed pointer" "$DESIGN" "docs/distributed-developer-mode-design.md"
require_text "DESIGN assistant non-goal" "$DESIGN" "No general-purpose assistant surface."
require_text "DESIGN current master schema" "$DESIGN" "schema-v14"
require_text "DESIGN result artifact boundary" "$DESIGN" \
  "Schema v13 adds bounded general-worker packet"

require_text "conveyor design status" "$FEATURE_CONVEYOR_DESIGN" "default-inert"
require_text "conveyor design approval" "$FEATURE_CONVEYOR_DESIGN" "Approve and Enqueue"
require_text "conveyor loopback status route" "$FEATURE_CONVEYOR_DESIGN" \
  "GET /v1/feature-conveyor/status"
require_text "conveyor MacBridge-only remote route" "$FEATURE_CONVEYOR_DESIGN" \
  "GET /v1/distributed/feature-conveyor/status"
require_text "conveyor advisory observation limit" "$FEATURE_CONVEYOR_DESIGN" \
  "This projection remains advisory observation only"
require_text "conveyor owner designation route" "$FEATURE_CONVEYOR_DESIGN" \
  "POST /v1/distributed/feature-conveyor/approved-features"
require_text "conveyor repository preflight boundary" "$FEATURE_CONVEYOR_DESIGN" \
  "owner-token loopback-only repository preflight"
require_text "conveyor repository snapshot claim boundary" "$FEATURE_CONVEYOR_DESIGN" \
  "repository-snapshot-claims"
require_text "conveyor coding dispatch boundary" "$FEATURE_CONVEYOR_DESIGN" \
  "coding-dispatches"
require_text "conveyor artifact integration boundary" "$FEATURE_CONVEYOR_DESIGN" \
  "artifact-integrations"
require_text "conveyor cancellation boundary" "$FEATURE_CONVEYOR_DESIGN" \
  "cancel-active-feature"
require_text "conveyor abandonment boundary" "$FEATURE_CONVEYOR_DESIGN" \
  "abandon-and-advance"
require_text "conveyor snapshot excludes history" "$FEATURE_CONVEYOR_DESIGN" \
  "never copies parent history or"
require_text "conveyor snapshot singleton reservation" "$FEATURE_CONVEYOR_DESIGN" \
  "fail-fast singleton reservation"
require_text "conveyor claimability limit" "$FEATURE_CONVEYOR_DESIGN" \
  "does not establish"
require_text "conveyor local status implementation" "$MASTER_PROCESS" \
  '"/v1/feature-conveyor/status"'
require_text "conveyor remote status implementation" "$MASTER_PROCESS" \
  '"/v1/distributed/feature-conveyor/status"'
require_text "conveyor owner designation implementation" "$MASTER_PROCESS" \
  '"/v1/feature-conveyor/owner-control-bridge"'
require_text "conveyor repository grant mutation implementation" "$MASTER_PROCESS" \
  '"/v1/feature-conveyor/repository-grants"'
require_text "conveyor repository grant status implementation" "$MASTER_PROCESS" \
  '"/v1/feature-conveyor/repositories/:repository_id/grants"'
require_text "conveyor repository preflight implementation" "$MASTER_PROCESS" \
  '"/v1/feature-conveyor/repository-preflight"'
require_text "conveyor repository snapshot claim implementation" "$MASTER_PROCESS" \
  '"/v1/feature-conveyor/repository-snapshot-claims"'
require_text "conveyor coding dispatch implementation" "$MASTER_PROCESS" \
  '"/v1/feature-conveyor/coding-dispatches"'
require_text "conveyor artifact integration implementation" "$MASTER_PROCESS" \
  '"/v1/feature-conveyor/artifact-integrations"'
require_text "conveyor artifact integration plan implementation" "$MASTER_PROCESS" \
  '"/v1/feature-conveyor/features/:feature_id/integration-plan"'
require_text "conveyor cancellation implementation" "$MASTER_PROCESS" \
  '"/v1/feature-conveyor/cancel-active-feature"'
require_text "conveyor abandonment implementation" "$MASTER_PROCESS" \
  '"/v1/feature-conveyor/abandon-and-advance"'
require_text "conveyor live local-coding lane" "$MAC_BRIDGE_LIVE_E2E" \
  "--run-local-coding"
require_text "conveyor live controller cancellation" "$WINDOWS_LOCAL_CODING_LIVE_CONTROL" \
  '"/v1/feature-conveyor/cancel-active-feature"'
require_text "conveyor live controller Mac cleanup binding" "$WINDOWS_LOCAL_CODING_LIVE_CONTROL" \
  "mac_cleanup_sha256"
require_text "conveyor live controller Git blob CRLF unit regression" \
  "$WINDOWS_LOCAL_CODING_LIVE_CONTROL" "git_blob_crlf_regression"
forbid_text "conveyor live controller stale no-retention criterion" \
  "$WINDOWS_LOCAL_CODING_LIVE_CONTROL" "retain no workspace"
require_text "conveyor live Mac retained-attempt pair-shape proof" "$MAC_BRIDGE_LIVE_E2E" \
  "mac_retained_attempt_pair_shape=verified"
require_text "native relay general-coding proof" "$MAC_LOCAL_CODING_SNAPSHOT_E2E" \
  "general_coding=verified"
require_text "agent current general-coding health boundary" "$AGENT_MAIN" \
  "metadata_cursor_plus_bounded_general_coding_retained_attempt"
forbid_text "agent stale fixed-fixture health boundary" "$AGENT_MAIN" \
  "metadata_cursor_plus_fixed_contained_coding_fixture_ephemeral_workspace"
require_text "release readiness current general-coding boundary" "$CORE_RELEASE" \
  "explicit general-coding admission"
forbid_text "release readiness stale fixed-fixture boundary" "$CORE_RELEASE" \
  "one fixed contained-coding fixture are implemented"
require_text "conveyor live harness-owned retained cleanup proof" "$MAC_BRIDGE_LIVE_E2E" \
  "harness_owned_pair_cleanup=verified"
forbid_text "conveyor live stale empty Mac workspace proof" "$MAC_BRIDGE_LIVE_E2E" \
  "mac_workspace_empty=verified"
require_text "conveyor live controller self-check portable scanner" \
  "$WINDOWS_LOCAL_CODING_LIVE_CONTROL_SELF_CHECK" "grep -Fq --"
forbid_text "conveyor live controller self-check ripgrep dependency" \
  "$WINDOWS_LOCAL_CODING_LIVE_CONTROL_SELF_CHECK" "rg -Fq --"
require_text "conveyor remote owner action implementation" "$MASTER_PROCESS" \
  '"/v1/distributed/feature-conveyor/approved-features"'
require_text "conveyor owner guidance implementation" "$MASTER_CRATE" \
  "owner_guidance"
require_text "conveyor pause revision implementation" "$MASTER_CRATE" \
  "emergency_pause_revision"
require_text "conveyor pre-handshake remote denial" "$MASTER_REMOTE_MTLS_E2E" \
  "pre-handshake client reached Feature Conveyor status"
require_text "conveyor non-MacBridge remote denial" "$MASTER_REMOTE_MTLS_E2E" \
  "non-MacBridge reached Feature Conveyor status"
require_text "conveyor pre-handshake owner-action denial" "$MASTER_REMOTE_MTLS_E2E" \
  "pre-handshake client reached owner-control enqueue"
require_text "conveyor remote owner-resolution absence" "$MASTER_REMOTE_MTLS_E2E" \
  '"/v1/feature-conveyor/cancel-active-feature"'
require_text "conveyor remote owner-action redaction" "$MASTER_REMOTE_MTLS_E2E" \
  "receipt leaked"
require_text "conveyor local owner route remote absence" "$MASTER_REMOTE_MTLS_E2E" \
  "owner-token local status route leaked onto the remote router"
require_text "conveyor local grant routes remote absence" "$MASTER_REMOTE_MTLS_E2E" \
  "repository-grant mutation leaked onto the remote router"
require_text "conveyor local preflight remote absence" "$MASTER_REMOTE_MTLS_E2E" \
  "repository preflight leaked onto the remote router"
require_text "conveyor local snapshot claim remote absence" "$MASTER_REMOTE_MTLS_E2E" \
  "repository snapshot claim leaked onto the remote router"
require_text "conveyor local coding dispatch remote absence" "$MASTER_REMOTE_MTLS_E2E" \
  "coding dispatch leaked onto the remote router"
require_text "conveyor Windows coding dispatch E2E" "$MASTER_REMOTE_MTLS_E2E" \
  "remote_local_coding_dispatch_is_exporter_bound_exact_and_pause_dominant"
require_text "conveyor coding protocol path-free contract" "$PROTOCOL_LOCAL_CODING_E2E" \
  "owner_dispatch_is_strict_bounded_digest_bound_and_path_free"
require_text "conveyor native coding admission" "$AGENT_LOCAL_CODING_E2E" \
  "native_agent_admits_only_path_free_snapshot_bound_metadata_without_executing_it"
require_text "conveyor protocol fixed contained result" "$PROTOCOL_CRATE" \
  'LOCAL_CODING_COMPLETED_STATUS: &str = "contained_coding_completed"'
require_text "conveyor protocol truthful tests-not-run result" "$PROTOCOL_CRATE" \
  'LOCAL_CODING_FIXTURE_TEST_STATUS: &str = "not_run"'
require_text "conveyor bounded general allowlist" "$PROTOCOL_CRATE" \
  'MAX_LOCAL_CODING_EDIT_PATHS: usize = 64'
require_text "conveyor protocol exact admission digest helper" "$PROTOCOL_CRATE" \
  'local_coding_admission_sha256'
require_text "conveyor protocol admission golden transcript" "$PROTOCOL_LOCAL_CODING_E2E" \
  'admission_digest_has_a_fixed_cross_language_transcript'
require_text "conveyor Swift exact admission domain" "$MAC_EVENT_RELAY" \
  'assemblywright.local-coding-admission.v1\0'
require_text "conveyor Swift local-coding projection denial" "$MAC_BRIDGE_SUPERVISOR" \
  "case .localCodingRelay:"
require_text "conveyor Swift admission binds protocol version" "$MAC_EVENT_RELAY" \
  'protocolVersion: AssemblywrightMacMTLSBridgeTransport.protocolVersion'
require_text "conveyor Swift admission binds lease duration" "$MAC_EVENT_RELAY" \
  'leaseDurationMilliseconds: leaseDuration'
require_text "conveyor Swift admission binds deadline" "$MAC_EVENT_RELAY" \
  'deadlineAfterMilliseconds: deadline'
require_text "conveyor Swift admission golden matches Rust" "$MAC_BRIDGE_TESTS" \
  'fb69cef80f0f2a37a886898c25121446a54308b52cb83fd70175c772936874cc'
forbid_text "conveyor Swift result excludes obsolete runner digest" "$MAC_EVENT_RELAY" \
  'runner_sha256'
require_text "conveyor descriptor-relative parent traversal" "$AGENT_SNAPSHOT" \
  'open_verified_parent_chain'
require_text "conveyor atomic replacement" "$AGENT_SNAPSHOT" \
  'libc::RENAME_SWAP'
require_text "conveyor descriptor-relative delete" "$AGENT_SNAPSHOT" \
  'libc::unlinkat'
require_text "conveyor atomic delete capture" "$AGENT_SNAPSHOT" \
  'atomic_delete_at'
require_text "conveyor atomic delete rollback" "$AGENT_SNAPSHOT" \
  'rollback_delete_swap'
require_text "conveyor protocol artifact-to-packet validation" "$PROTOCOL_CRATE" \
  'validate_local_coding_patch_artifact_for_packet'
require_text "conveyor master retained result binding" "$MASTER_CRATE" \
  'workspace_retained = ?18 AND workspace_expires_at_ms = ?19'
require_text "conveyor Swift launches agent with empty environment" "$MAC_EVENT_RELAY" \
  'process.environment = [:]'
require_text "conveyor agent rejects nonempty local-coding parent environment" "$AGENT_PROCESS" \
  'validate_local_coding_parent_environment'
require_text "conveyor external restart recovery record" "$AGENT_SNAPSHOT" \
  'RetainedWorkspaceRecord'
require_text "conveyor restart verifies workspace tree" "$AGENT_SNAPSHOT" \
  'workspace_tree_sha256'
require_text "conveyor unresolved completion blocks admission" "$AGENT_SNAPSHOT" \
  'return Err(LocalCodingSnapshotError::AlreadyActive)'
require_text "conveyor aggregate materialized-output budget" "$AGENT_SNAPSHOT" \
  'max_materialized_bytes'
forbid_text "conveyor fork child does not inspect environment APIs" "$AGENT_SNAPSHOT" \
  'static mut environ'
require_text "conveyor sealed workspace before result" "$AGENT_SNAPSHOT" \
  'format!("{}.sealed"'
require_text "conveyor native Swift-to-Rust snapshot E2E" \
  "$MAC_LOCAL_CODING_SNAPSHOT_E2E" \
  "localCodingSnapshotRelayUsesRealSupervisedAgent"
require_text "conveyor native Swift-to-Rust cancellation proof" \
  "$MAC_LOCAL_CODING_SNAPSHOT_E2E" \
  "assemblywright_mac_local_coding_native_cancellation_e2e_ok"
require_text "conveyor Swift sends local cancellation during final verification" \
  "$MAC_EVENT_RELAY" \
  "let acknowledgement = try await agent.cancelLocalCodingSnapshot"
require_text "conveyor native snapshot E2E release gate" "$LOCAL_GATE" \
  "./scripts/mac-local-coding-snapshot-e2e.sh"
require_text "conveyor Swift strict decoder" "$MAC_BRIDGE_SUPERVISOR" \
  "invalid_feature_conveyor_status"
require_text "conveyor authenticated snapshot only" "$MAC_BRIDGE_SUPERVISOR" \
  'case featureConveyor = "feature_conveyor"'
require_text "conveyor read-only Mac presentation" "$MAC_APP" \
  "Guidance is not an approval or callable action"
require_text "conveyor explicit signed-helper action" "$MAC_BRIDGE_CLI" \
  '"feature-conveyor", "approve-and-enqueue", "--confirm"'
require_text "conveyor strict signed-helper owner action" "$MAC_OWNER_CONTROL" \
  "owner_control_designation_revision"
require_text "conveyor Swift negative-path regression" "$MAC_BRIDGE_TESTS" \
  "Supervisor rejects drifted, inconsistent, duplicate, and oversized Conveyor data"
require_text "conveyor live observer requires signed helper" "$MAC_BRIDGE_LIVE_E2E" \
  'codesign --verify --strict "$BRIDGE_BIN"'
require_text "conveyor live observer requires schema marker" "$MAC_BRIDGE_LIVE_E2E" \
  'feature_conveyor_schema=8'
require_text "conveyor live observer validates schema eight" "$MAC_BRIDGE_LIVE_E2E" \
  'Feature Conveyor schema was not v8'
forbid_text "conveyor live observer rejects stale schema seven" \
  "$MAC_BRIDGE_LIVE_E2E" 'Feature Conveyor schema was not v7'
require_text "conveyor live observer requires repeated monitor samples" \
  "$MAC_BRIDGE_LIVE_E2E" "bridge monitor did not emit exactly two bounded samples"
require_text "conveyor live observer requires reconnect advance" "$MAC_BRIDGE_LIVE_E2E" \
  "bridge reconnect diagnostic did not advance the connection epoch"
require_text "conveyor base live observer command" "$BUILD_DOCS" \
  "./scripts/mac-windows-bridge-live-e2e.sh --run"

require_text "architecture conveyor kernel" "$ARCHITECTURE" "Feature Conveyor repository kernel"
require_text "architecture core reduction" "$ARCHITECTURE" "no longer an assistant runtime"

require_text "brand migration section" "$BRAND" "## Migration From The Former Name"
require_text "knowledge base naming contract gate" "$KB" \
  "release-naming-contract-smoke.sh"
require_text "knowledge base pivot" "$KB" "## The Pivot"
require_text "knowledge base crate boundaries" "$KB" "## Current Crate Boundaries"
require_text "knowledge base proof boundaries" "$KB" "## Proof Boundaries"
require_text "knowledge base conveyor status boundary" "$KB" \
  "owner-token-authenticated loopback-only"
require_text "knowledge base conveyor remote observer" "$KB" \
  "GET /v1/distributed/feature-conveyor/status"
require_text "knowledge base conveyor guidance boundary" "$KB" \
  'fixed-enum `owner_guidance`'
require_text "knowledge base conveyor pause revision" "$KB" \
  '`emergency_pause_revision`'
require_text "knowledge base conveyor live observer proof" "$KB" \
  '`feature_conveyor_schema=8`'
require_text "knowledge base owner designation boundary" "$KB" \
  "POST /v1/feature-conveyor/owner-control-bridge"
require_text "knowledge base remote owner action boundary" "$KB" \
  "POST /v1/distributed/feature-conveyor/approved-features"
require_text "knowledge base repository grant boundary" "$KB" \
  "POST /v1/feature-conveyor/repository-grants"
require_text "knowledge base repository grant mutation invariant" "$KB" \
  "sole public mutation primitive"
require_text "knowledge base repository preflight boundary" "$KB" \
  "point-in-time admission check"
require_text "knowledge base standalone preflight checkout" "$KB" \
  "dedicated standalone checkout"
require_text "knowledge base Windows final path rule" "$KB" \
  "GetFinalPathNameByHandleW"
require_text "knowledge base native HTTP response framing rule" "$KB" \
  'complete declared `Content-Length` body'
require_text "knowledge base feature closeout" "$KB" \
  "Every feature or phase uses the closeout contract"
require_text "knowledge base native E2E boundary" "$KB" \
  "Native Rust/Swift HTTP, process, protocol, service, packaged-app"
require_text "knowledge base shell portability" "$KB" "## Shell Portability"
require_text "knowledge base shell portability gate" "$KB" \
  "release-shell-portability-smoke.sh"

require_text "release checklist live-device QA" "$CHECKLIST" "live-device QA"
require_text "release checklist Developer ID" "$CHECKLIST" "Developer ID"
require_text "release checklist docs gate" "$CHECKLIST" "release-docs-drift-smoke.sh"
require_text "release checklist conveyor observation seam" "$CHECKLIST" \
  "GET /v1/feature-conveyor/status"
require_text "release checklist conveyor remote observation" "$CHECKLIST" \
  "GET /v1/distributed/feature-conveyor/status"
require_text "release checklist conveyor owner designation" "$CHECKLIST" \
  "owner-control MacBridge"
require_text "release checklist repository grant boundary" "$CHECKLIST" \
  "owner-token loopback repository-grant routes"
require_text "release checklist repository preflight boundary" "$CHECKLIST" \
  "loopback repository preflight"
require_text "release checklist preserves worktree metadata" "$CHECKLIST" \
  "do not prune or delete that metadata"
require_text "release checklist feature closeout" "$CHECKLIST" \
  "docs/development-agent-workflow.md"
require_text "release checklist owner resolution" "$CHECKLIST" \
  "abandon-and-advance"
require_text "release checklist schema-v11 migration invariant" "$CHECKLIST" \
  "schema-v11 backup-first"
require_text "release checklist schema-v13 artifact invariant" "$CHECKLIST" \
  "protocol-v5/schema-v13 result-artifact admission"
require_text "release checklist schema-v14 integration invariant" "$CHECKLIST" \
  "schema-v14 artifact integration"
require_text "architecture schema-v13 artifact store" "$ARCHITECTURE" \
  "Private bytes outside SQLite; immutable schema-v13 metadata and redacted audit"
require_text "architecture current master schema" "$ARCHITECTURE" \
  '`assemblywright-master` schema version 14'
require_text "architecture current general worker" "$ARCHITECTURE" \
  "Protocol v5 replaces the historical v4 fixed-child"
require_text "feature design current general worker" "$FEATURE_CONVEYOR_DESIGN" \
  "implemented protocol-v5/schema-v14 kernel"
require_text "readme current general worker" "$README" \
  "packet-bound deterministic writes/deletes"
require_text "knowledge base current protocol" "$KB" \
  '`PROTOCOL_VERSION` is 5'
forbid_text "architecture stale current schema" "$ARCHITECTURE" \
  "schema version 12 preserves"
forbid_text "feature design stale current fixture" "$FEATURE_CONVEYOR_DESIGN" \
  "The implemented schema-v12 kernel reaches one fixed contained-coding fixture"
require_text "knowledge base schema-v13 artifact boundary" "$KB" \
  "Protocol v5/schema v13 replaces the fixed README fixture"
require_text "knowledge base schema-v13 live closeout" "$KB" \
  "seals the attempt workspace for at most one hour"
require_text "knowledge base retained live-attempt pair" "$KB" \
  'one owner-private `<attempt>.sealed` directory'
require_text "knowledge base immutable Git blob CRLF boundary" "$KB" \
  "binds the immutable Git blob bytes"
require_text "knowledge base schema-v14 candidate boundary" "$KB" \
  "Schema v14 deliberately does not widen the protocol-v5 Mac worker contract"
require_text "design schema-v14 candidate boundary" "$DESIGN" \
  "Schema v14 adds artifact integration and candidate freezing"
require_text "safety schema-v14 candidate boundary" "$SAFETY_RULES" \
  "Schema v14 artifact integration is a distinct owner-token-authenticated"
require_text "knowledge base live receipt integrity" "$KB" \
  "one unchanged JSON line on stdin"
require_text "design stable artifact evidence" "$DESIGN" \
  "Terminal result acceptance re-hashes a stable handle"
require_text "safety guarded artifact retry" "$SAFETY_RULES" \
  "Preparation guards make exact crash/concurrent retry"
require_text "release checklist Windows artifact durability proof" "$CHECKLIST" \
  "do not claim portable Windows directory flush"

require_text "agent instructions feature closeout" "$AGENTS" \
  "Close every feature or phase"
require_text "agent workflow closeout section" "$AGENT_WORKFLOW" \
  "## Feature And Phase Closeout"
require_text "agent workflow conversation knowledge audit" "$AGENT_WORKFLOW" \
  "Review the conversation for durable repository facts"
require_text "agent workflow unit skill" "$AGENT_WORKFLOW" \
  "unit-testing-test-generate"
require_text "agent workflow E2E skill" "$AGENT_WORKFLOW" \
  "e2e-testing"
require_text "agent workflow Playwright applicability" "$AGENT_WORKFLOW" \
  "Playwright, screenshots, visual"
require_text "agent workflow native E2E boundary" "$AGENT_WORKFLOW" \
  "native cross-process"
require_text "agent workflow Windows deployment parity" "$AGENT_WORKFLOW" \
  "fast-forward the authoritative"
require_text "agent workflow explicit closeout verdicts" "$AGENT_WORKFLOW" \
  "state explicit verdicts for documentation and safety"

require_text "build docs local gate" "$BUILD_DOCS" "./scripts/release-local.sh"
require_text "build docs local-coding live closeout" "$BUILD_DOCS" \
  "./scripts/mac-windows-bridge-live-e2e.sh --run-local-coding"
require_text "build docs result-artifact live boundary" "$BUILD_DOCS" \
  "terminal success proves the protocol-v5 result"
require_text "build docs retained-attempt live boundary" "$BUILD_DOCS" \
  "private retained-state shape"
require_text "build docs evidence boundary" "$BUILD_DOCS" "## Release Evidence Boundary"
require_text "build docs windows gate" "$BUILD_DOCS" "windows-protocol.yml"
require_text "build docs Windows coding dispatch E2E command" "$BUILD_DOCS" \
  "remote_local_coding_dispatch_is_exporter_bound_exact_and_pause_dominant"
require_text "build docs shell portability gate" "$BUILD_DOCS" \
  "./scripts/release-shell-portability-smoke.sh --check"
require_text "build docs readiness unit test" "$BUILD_DOCS" \
  "cargo test -p assemblywright-core protocol_readiness_proof_is_version_independent"
require_text "build docs readiness E2E" "$BUILD_DOCS" \
  "cargo test -p assemblywright-cli --test release_readiness_e2e"
require_text "build docs snapshot claim process E2E" "$BUILD_DOCS" \
  "repository_snapshot_claim_is_authenticated_path_free_and_durable"
require_text "build docs coding dispatch protocol" "$BUILD_DOCS" \
  "cargo test -p assemblywright-protocol --test local_coding_contract"
require_text "build docs artifact-integration protocol" "$BUILD_DOCS" \
  "cargo test -p assemblywright-protocol --test artifact_integration_contract"
require_text "build docs native artifact-integration E2E" "$BUILD_DOCS" \
  "cargo test -p assemblywright-master --test artifact_integration_e2e"
require_text "build docs owner-resolution protocol" "$BUILD_DOCS" \
  "cargo test -p assemblywright-protocol --test owner_resolution_contract"
require_text "build docs native snapshot relay E2E" "$BUILD_DOCS" \
  "./scripts/mac-local-coding-snapshot-e2e.sh"
require_text "build docs live local-coding E2E" "$BUILD_DOCS" \
  "./scripts/mac-windows-bridge-live-e2e.sh --run-local-coding"
require_text "live harness artifact integration" "$MAC_BRIDGE_LIVE_E2E" \
  "assemblywright_mac_windows_artifact_integration_required"
require_text "Windows live candidate integration" "$WINDOWS_LOCAL_CODING_LIVE_CONTROL" \
  '"Integrate" {'
require_text "build docs Windows live-controller unit regression" "$BUILD_DOCS" \
  "windows-local-coding-live-control.ps1 -Action Check"
require_text "hosted Windows live-controller unit regression" "$WINDOWS_PROTOCOL_WORKFLOW" \
  "windows-local-coding-live-control.ps1 -Action Check"
require_text "build docs Windows package-scoped clippy boundary" "$BUILD_DOCS" \
  "Do not substitute the macOS/Linux workspace-wide clippy command"
require_text "safety snapshot blocking timeout boundary" "$SAFETY_RULES" \
  "Timeout must not pretend to cancel a blocking thread"
require_text "safety snapshot pre-allocation header gate" "$SAFETY_RULES" \
  "header type/declared-size"
require_text "knowledge base shallow snapshot boundary" "$KB" \
  "parent commits and deleted historical objects are absent"
require_text "knowledge base coding dispatch boundary" "$KB" \
  "metadata-only coding-dispatch admission"
require_text "knowledge base owner resolution" "$KB" \
  "cancel-active-feature"
require_text "knowledge base resolution migration invariant" "$KB" \
  "backup-first v10 migration"
require_text "knowledge base production Swift-to-Rust snapshot E2E" "$KB" \
  "production Swift relay and code-identity launcher"
require_text "knowledge base local-coding projection boundary" "$KB" \
  "must not request the MacBridge-only Feature"
require_text "knowledge base contained-coding result" "$KB" \
  'contained_coding_completed'
require_text "safety contained-coding host boundary" "$SAFETY_RULES" \
  "does not claim a host sandbox or host-level egress control"
require_text "release checklist truthful tests-not-run evidence" "$CHECKLIST" \
  'test_status:not_run'
require_text "knowledge base raw manifest path rule" "$KB" \
  "raw UTF-8 bytes before constructing a"

# Documents must not advertise the removed assistant surface.
for file in "$README" "$DESIGN" "$ARCHITECTURE" "$BUILD_DOCS" "$CHECKLIST" "$KB" "$AGENTS"; do
  for phrase in \
    "assemblywright serve" \
    "assemblywright-cli -- serve" \
    "assemblywright-cli -- smoke" \
    "release plugin-trust-runbook" \
    "release-plugin-trust-qa.sh" \
    "release-operator-qa-smoke.sh" \
    "storage-migration-backup-smoke.sh" \
    "NSMicrophoneUsageDescription" \
    "NSSpeechRecognitionUsageDescription"; do
    forbid_text "stale assistant reference" "$file" "$phrase"
  done
done

# The gate script and the canonical command doc must not drift apart.
while IFS= read -r command; do
  require_text "build docs local gate command" "$BUILD_DOCS" "$command"
done < <(grep -E '^run ' "$LOCAL_GATE" | sed -E 's/^run //')

require_text "local gate heartbeat" "$LOCAL_GATE" "still running after"
require_text "local gate completion" "$LOCAL_GATE" "completed in"
require_text "local gate failure" "$LOCAL_GATE" "command failed after"

# The protocol version is declared four times across three languages, while
# user-visible prose must not duplicate its numeric value. That contract has its
# own gate because both rules have to be proven against fixtures, and this
# script has no self-test mode. See
# scripts/release-protocol-version-contract-smoke.sh, which release-local.sh
# runs in both --check and --self-test.
require_file "$PROTOCOL_VERSION_CONTRACT_SMOKE"
require_text "protocol version contract covers Rust" \
  "$PROTOCOL_VERSION_CONTRACT_SMOKE" "$PROTOCOL_CRATE"
require_text "protocol version contract covers Swift" \
  "$PROTOCOL_VERSION_CONTRACT_SMOKE" "$MAC_BRIDGE"
require_text "protocol version contract covers the fixture control plane" \
  "$PROTOCOL_VERSION_CONTRACT_SMOKE" "$WINDOWS_FIXTURE_LIVE_CONTROL"
require_text "protocol version contract covers the MLX control plane" \
  "$PROTOCOL_VERSION_CONTRACT_SMOKE" "$WINDOWS_MLX_LIVE_CONTROL"
require_text "protocol version contract covers the local-coding control plane" \
  "$PROTOCOL_VERSION_CONTRACT_SMOKE" "$WINDOWS_LOCAL_CODING_LIVE_CONTROL"
require_text "protocol version contract covers README prose" \
  "$PROTOCOL_VERSION_CONTRACT_SMOKE" "$README"
require_text "protocol version contract covers architecture prose" \
  "$PROTOCOL_VERSION_CONTRACT_SMOKE" "$ARCHITECTURE"
require_text "protocol version contract covers readiness prose" \
  "$PROTOCOL_VERSION_CONTRACT_SMOKE" "$CORE_RELEASE"

# The TLS exporter label is the same kind of duplicated wire constant.
mac_exporter_label="$(sed -n 's/.*public static let exporterLabel = "\([^"]*\)".*/\1/p' \
  "$MAC_BRIDGE" | head -n 1)"
[[ -n "$mac_exporter_label" ]] || fail "could not read exporterLabel from $MAC_BRIDGE"
require_text "master exporter label matches the Mac" "$MASTER_PROCESS" "$mac_exporter_label"

require_text "safety rules fail closed" "$SAFETY_RULES" "fail"
require_text "agent workflow roles" "$AGENT_WORKFLOW" "assemblywright-"
require_text "agents build commands pointer" "$AGENTS" "docs/build-test-commands.md"

printf 'Assemblywright release docs drift smoke: ok\n'
