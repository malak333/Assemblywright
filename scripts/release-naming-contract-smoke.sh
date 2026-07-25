#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# The rename to Assemblywright is total. Nothing in this repository is named
# `jarvis`/`Jarvis`/`JARVIS_` any more: not crates, binaries, SwiftPM products or
# targets, environment variables, code-signing identifiers, Keychain services,
# state directories, the Windows service, wire labels, or certificate subjects.
#
# Exactly one legacy identifier survives, and only as a read path: the master
# reads a pre-rename `Jarvis\master` state directory once so an already-enrolled
# Windows host keeps its durable kernel. It is never written and never advertised.
# That single exception is pinned by name below so it cannot quietly grow.

MODE="check"

WORKSPACE_MANIFEST="Cargo.toml"
CLI_MANIFEST="crates/assemblywright-cli/Cargo.toml"
AGENT_MANIFEST="crates/assemblywright-agent/Cargo.toml"
MASTER_MANIFEST="crates/assemblywright-master/Cargo.toml"
CORE_MANIFEST="crates/assemblywright-core/Cargo.toml"
PROTOCOL_MANIFEST="crates/assemblywright-protocol/Cargo.toml"
SWIFT_MANIFEST="apps/mac/Package.swift"
PACKAGING="scripts/package-distribution.sh"
MASTER_PROCESS="crates/assemblywright-master/src/main.rs"
MASTER_IDENTITY="crates/assemblywright-master/src/identity.rs"
PROTOCOL_CRATE="crates/assemblywright-protocol/src/lib.rs"
MAC_BRIDGE="apps/mac/Sources/AssemblywrightMacCore/DeveloperBridge.swift"
MAC_KEYCHAIN="apps/mac/Sources/AssemblywrightMacCore/KeychainDeveloperIdentity.swift"
LIVE_DEVICE_QA="scripts/release-live-device-qa.sh"
BRAND="docs/brand.md"
KB="docs/knowledge-base/assemblywright-project-facts.md"
SELF="scripts/release-naming-contract-smoke.sh"

# The sole permitted legacy reference: the one-time state-directory adoption.
LEGACY_STATE_EXCEPTION='const LEGACY_MASTER_STATE_NAMESPACE: &str = "Jarvis";'

usage() {
  cat <<'USAGE'
Usage: scripts/release-naming-contract-smoke.sh [--check | --self-test]

Validate that the Assemblywright naming contract is total: every product,
identity, state, and wire name is `assemblywright`/`Assemblywright`, and the only
surviving legacy identifier is the one-time Windows state-directory adoption.

  --check      Assert the naming contract (default).
  --self-test  Prove the assertion helpers actually detect drift.

Proof boundary: source text inspection only. No app is built, signed,
notarized, installed, or launched.
USAGE
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_text() {
  local label="$1" file="$2" expected="$3"
  if ! grep -Fq -- "$expected" "$file"; then
    fail "$label is missing required text in $file: $expected"
  fi
}

forbid_pattern() {
  local label="$1" file="$2" pattern="$3"
  if grep -Eq -- "$pattern" "$file"; then
    fail "$label matched a forbidden legacy pattern in $file: $pattern"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check) MODE="check"; shift ;;
    --self-test) MODE="self-test"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown argument: $1" ;;
  esac
done

self_test() {
  FIXTURE_DIR="$(mktemp -d)"
  trap 'rm -rf "$FIXTURE_DIR"' EXIT
  local fixture="$FIXTURE_DIR/fixture.txt"

  printf 'name = "assemblywright-cli"\n' >"$fixture"
  if (require_text "self-test" "$fixture" 'name = "legacy-cli"') 2>/dev/null; then
    fail "self-test: require_text accepted absent text"
  fi
  require_text "self-test" "$fixture" 'name = "assemblywright-cli"'

  printf 'name = "legacy-cli"\n' >"$fixture"
  if (forbid_pattern "self-test" "$fixture" 'legacy') 2>/dev/null; then
    fail "self-test: forbid_pattern accepted a forbidden match"
  fi
  printf 'name = "assemblywright-cli"\n' >"$fixture"
  forbid_pattern "self-test" "$fixture" 'legacy'

  if (require_file "$FIXTURE_DIR/absent.txt") 2>/dev/null; then
    fail "self-test: require_file accepted a missing file"
  fi

  printf 'Assemblywright naming contract self-test: ok\n'
}

if [[ "$MODE" == "self-test" ]]; then
  self_test
  exit 0
fi

for file in \
  "$WORKSPACE_MANIFEST" "$CLI_MANIFEST" "$AGENT_MANIFEST" "$MASTER_MANIFEST" \
  "$CORE_MANIFEST" "$PROTOCOL_MANIFEST" "$SWIFT_MANIFEST" "$PACKAGING" \
  "$MASTER_PROCESS" "$MASTER_IDENTITY" "$PROTOCOL_CRATE" "$MAC_BRIDGE" \
  "$MAC_KEYCHAIN" "$LIVE_DEVICE_QA" "$BRAND" "$KB"; do
  require_file "$file"
done

# ---------------------------------------------------------------------------
# 1. No tracked path is named after the former product.
# ---------------------------------------------------------------------------

legacy_paths="$(git ls-files | grep -i jarvis || true)"
if [[ -n "$legacy_paths" ]]; then
  fail "tracked paths still carry the former name:"$'\n'"$legacy_paths"
fi

# ---------------------------------------------------------------------------
# 2. No tracked file content carries the former product name, except the pinned
#    state-adoption read path and the docs that explain it.
# ---------------------------------------------------------------------------

offenders=""
while IFS= read -r file; do
  [[ -f "$file" ]] || continue
  case "$file" in
    "$MASTER_PROCESS"|"$BRAND"|"$KB"|"$SELF") continue ;;
  esac
  hits="$(grep -nIi 'jarvis' -- "$file" 2>/dev/null || true)"
  [[ -z "$hits" ]] || offenders+="$file:$hits"$'\n'
done < <(git ls-files)

if [[ -n "$offenders" ]]; then
  fail "the former product name survives in tracked content:"$'\n'"$offenders"
fi

# The exception must exist exactly where it is declared, and must stay read-only.
require_text "legacy state adoption constant" "$MASTER_PROCESS" "$LEGACY_STATE_EXCEPTION"
legacy_in_master="$(grep -cI 'Jarvis' -- "$MASTER_PROCESS" || true)"
if [[ "$legacy_in_master" -gt 3 ]]; then
  fail "the master mentions the former namespace on $legacy_in_master lines; only the \
LEGACY_MASTER_STATE_NAMESPACE constant and its doc comment may"
fi

# ---------------------------------------------------------------------------
# 3. Current names are present where they matter.
# ---------------------------------------------------------------------------

for member in \
  '"crates/assemblywright-protocol"' '"crates/assemblywright-master"' \
  '"crates/assemblywright-agent"' '"crates/assemblywright-core"' \
  '"crates/assemblywright-cli"'; do
  require_text "workspace member" "$WORKSPACE_MANIFEST" "$member"
done
require_text "workspace repository" "$WORKSPACE_MANIFEST" \
  "https://github.com/malak333/Assemblywright"

require_text "cli package name" "$CLI_MANIFEST" 'name = "assemblywright-cli"'
require_text "cli binary name" "$CLI_MANIFEST" 'name = "assemblywright"'
require_text "agent package name" "$AGENT_MANIFEST" 'name = "assemblywright-agent"'
require_text "master package name" "$MASTER_MANIFEST" 'name = "assemblywright-master"'
require_text "core package name" "$CORE_MANIFEST" 'name = "assemblywright-core"'
require_text "protocol package name" "$PROTOCOL_MANIFEST" 'name = "assemblywright-protocol"'

require_text "swift package name" "$SWIFT_MANIFEST" 'name: "AssemblywrightMac"'
require_text "swift core product" "$SWIFT_MANIFEST" '.library(name: "AssemblywrightMacCore"'
require_text "swift app product" "$SWIFT_MANIFEST" '.executable(name: "AssemblywrightMacApp"'
require_text "swift bridge product" "$SWIFT_MANIFEST" \
  '.executable(name: "assemblywright-mac-bridge"'

require_text "packaging app name" "$PACKAGING" 'APP_NAME="Assemblywright"'
require_text "packaging bundle identity" "$PACKAGING" \
  'BUNDLE_ID="com.nobiletechnology.assemblywright"'
require_text "packaging bundled core identity" "$PACKAGING" \
  'CORE_CODE_ID="${BUNDLE_ID}.core"'
require_text "packaging app executable" "$PACKAGING" \
  'APP_EXECUTABLE_NAME="AssemblywrightMacApp"'
require_text "packaging bundled core binary" "$PACKAGING" \
  'CORE_EXECUTABLE_NAME="assemblywright-cli"'

require_text "windows service name" "$MASTER_PROCESS" \
  'DEFAULT_SERVICE_NAME: &str = "AssemblywrightMaster"'
require_text "master state namespace" "$MASTER_PROCESS" \
  'MASTER_STATE_NAMESPACE: &str = "Assemblywright"'
require_text "master exporter label" "$MASTER_PROCESS" \
  'EXPORTER-Assemblywright-Developer-Mode-v1'
require_text "mac exporter label" "$MAC_BRIDGE" \
  'EXPORTER-Assemblywright-Developer-Mode-v1'
require_text "certificate SAN namespace" "$MASTER_IDENTITY" 'urn:assemblywright:device:'
require_text "keychain namespace" "$MAC_KEYCHAIN" \
  'com.nobiletechnology.assemblywright.developer-bridge'

# The wire contract moved, so the version must move with it. A former-name peer
# and a current peer disagree on the exporter label, the fixture capability
# identity, and the certificate subject, and must reject each other on version
# rather than failing somewhere less obvious.
require_text "protocol version bump" "$PROTOCOL_CRATE" 'PROTOCOL_VERSION: u16 = 2'
require_text "fixture capability provider" "$PROTOCOL_CRATE" \
  'FIXTURE_REASONING_PROVIDER: &str = "assemblywright-fixture"'
require_text "fixture capability model" "$PROTOCOL_CRATE" \
  'FIXTURE_REASONING_MODEL: &str = "assemblywright-fixture-v1"'

# ---------------------------------------------------------------------------
# 4. Migration guidance stays documented, because the rename orphans state.
# ---------------------------------------------------------------------------

require_text "brand migration section" "$BRAND" "## Migration From The Former Name"
require_text "knowledge base migration section" "$KB" "## Migrating A Host Past The Rename"
require_text "knowledge base re-enrollment" "$KB" "re-enroll"
require_text "knowledge base state adoption" "$KB" "adopt"

for env_var in \
  ASSEMBLYWRIGHT_MASTER_DATA_DIR \
  ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE \
  ASSEMBLYWRIGHT_IPC_TOKEN_FILE \
  ASSEMBLYWRIGHT_BUNDLE_ID; do
  if ! grep -rqI --exclude-dir=target --exclude-dir=.git -- "$env_var" .; then
    fail "expected environment variable is missing: $env_var"
  fi
done

printf 'Assemblywright naming contract: ok (rename is total; one pinned legacy read path)\n'
printf 'Proof boundary: source text inspection only; no app was built, signed, notarized, installed, or launched.\n'
