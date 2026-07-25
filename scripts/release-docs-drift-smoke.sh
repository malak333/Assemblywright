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
CLI_MAIN="crates/assemblywright-cli/src/main.rs"

PROTOCOL_E2E="crates/assemblywright-protocol/tests/distributed_protocol_contract_e2e.rs"
PROTOCOL_EVENT_E2E="crates/assemblywright-protocol/tests/distributed_event_cursor_contract.rs"
PROTOCOL_MLX_E2E="crates/assemblywright-protocol/tests/mlx_job_contract.rs"
MASTER_E2E="crates/assemblywright-master/tests/master_lifecycle_e2e.rs"
MASTER_PROCESS_E2E="crates/assemblywright-master/tests/master_process_e2e.rs"
MASTER_IDENTITY_E2E="crates/assemblywright-master/tests/enrollment_identity_e2e.rs"
MASTER_REMOTE_MTLS_E2E="crates/assemblywright-master/tests/remote_mtls_e2e.rs"
MASTER_EVENT_E2E="crates/assemblywright-master/tests/event_cursor_e2e.rs"
MASTER_CONVEYOR_E2E="crates/assemblywright-master/tests/feature_conveyor_kernel.rs"
MASTER_SERVICE_E2E="crates/assemblywright-master/tests/windows_service_lifecycle_e2e.rs"
AGENT_E2E="crates/assemblywright-agent/tests/local_relay_e2e.rs"
CLI_NAMING_E2E="crates/assemblywright-cli/tests/naming_contract_e2e.rs"

MAC_BRIDGE="apps/mac/Sources/AssemblywrightMacCore/DeveloperBridge.swift"
MAC_BRIDGE_CLI="apps/mac/Sources/AssemblywrightMacBridgeCLI/AssemblywrightMacBridgeCLI.swift"
MAC_BRIDGE_KEYCHAIN="apps/mac/Sources/AssemblywrightMacCore/KeychainDeveloperIdentity.swift"
MAC_BRIDGE_NETWORK="apps/mac/Sources/AssemblywrightMacCore/NetworkMTLSBridge.swift"
MAC_BRIDGE_SUPERVISOR="apps/mac/Sources/AssemblywrightMacCore/DeveloperBridgeSupervisor.swift"
MAC_BRIDGE_PROCESS="apps/mac/Sources/AssemblywrightMacCore/DeveloperBridgeProcessLifecycle.swift"
MAC_EVENT_RELAY="apps/mac/Sources/AssemblywrightMacCore/DeveloperEventRelay.swift"
MAC_APP="apps/mac/Sources/AssemblywrightMacApp/AssemblywrightMacApp.swift"
MAC_BRIDGE_TESTS="apps/mac/Tests/AssemblywrightMacCoreTests/DeveloperBridgeTests.swift"
MAC_APP_TESTS="apps/mac/Tests/AssemblywrightMacAppTests/AssemblywrightMacAppTests.swift"

MAC_BRIDGE_LIVE_E2E="scripts/mac-windows-bridge-live-e2e.sh"
WINDOWS_FIXTURE_LIVE_CONTROL="scripts/windows-fixture-live-control.ps1"
WINDOWS_MLX_LIVE_CONTROL="scripts/windows-mlx-live-control.ps1"
WINDOWS_PROTOCOL_WORKFLOW=".github/workflows/windows-protocol.yml"
RELEASE_VERSION_SCRIPT="scripts/release-version.sh"
NAMING_CONTRACT_SMOKE="scripts/release-naming-contract-smoke.sh"

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
  "$MASTER_E2E" "$MASTER_PROCESS_E2E" "$MASTER_IDENTITY_E2E" \
  "$MASTER_REMOTE_MTLS_E2E" "$MASTER_EVENT_E2E" "$MASTER_CONVEYOR_E2E" \
  "$MASTER_SERVICE_E2E" "$AGENT_E2E" "$CLI_NAMING_E2E" \
  "$MAC_BRIDGE" "$MAC_BRIDGE_CLI" "$MAC_BRIDGE_KEYCHAIN" "$MAC_BRIDGE_NETWORK" \
  "$MAC_BRIDGE_SUPERVISOR" "$MAC_BRIDGE_PROCESS" "$MAC_EVENT_RELAY" \
  "$MAC_APP" "$MAC_BRIDGE_TESTS" "$MAC_APP_TESTS" \
  "$MAC_BRIDGE_LIVE_E2E" "$WINDOWS_FIXTURE_LIVE_CONTROL" \
  "$WINDOWS_MLX_LIVE_CONTROL" "$WINDOWS_PROTOCOL_WORKFLOW" \
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

require_text "conveyor design status" "$FEATURE_CONVEYOR_DESIGN" "default-inert"
require_text "conveyor design approval" "$FEATURE_CONVEYOR_DESIGN" "Approve and Enqueue"

require_text "architecture conveyor kernel" "$ARCHITECTURE" "Feature Conveyor repository kernel"
require_text "architecture core reduction" "$ARCHITECTURE" "no longer an assistant runtime"

require_text "brand migration section" "$BRAND" "## Migration From The Former Name"
require_text "knowledge base naming contract gate" "$KB" \
  "release-naming-contract-smoke.sh"
require_text "knowledge base pivot" "$KB" "## The Pivot"
require_text "knowledge base crate boundaries" "$KB" "## Current Crate Boundaries"
require_text "knowledge base proof boundaries" "$KB" "## Proof Boundaries"

require_text "release checklist live-device QA" "$CHECKLIST" "live-device QA"
require_text "release checklist Developer ID" "$CHECKLIST" "Developer ID"
require_text "release checklist docs gate" "$CHECKLIST" "release-docs-drift-smoke.sh"

require_text "build docs local gate" "$BUILD_DOCS" "./scripts/release-local.sh"
require_text "build docs evidence boundary" "$BUILD_DOCS" "## Release Evidence Boundary"
require_text "build docs windows gate" "$BUILD_DOCS" "windows-protocol.yml"

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

require_text "safety rules fail closed" "$SAFETY_RULES" "fail"
require_text "agent workflow roles" "$AGENT_WORKFLOW" "assemblywright-"
require_text "agents build commands pointer" "$AGENTS" "docs/build-test-commands.md"

printf 'Assemblywright release docs drift smoke: ok\n'
