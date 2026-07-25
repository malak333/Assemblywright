#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# The Assemblywright rename has two halves, and drift in either direction is a
# defect:
#
#   1. Product-facing names must be `assemblywright`/`Assemblywright`. Legacy
#      `jarvis*` crates, binary aliases, and SwiftPM product aliases are gone
#      and must not reappear.
#   2. A specific set of `Jarvis`/`JARVIS_` identifiers must NOT be renamed,
#      because each one binds signed identity or installed state. Renaming any
#      of them changes code-signing identity or orphans an installed app.
#
# The second half is the fragile one: it looks like leftover drift, so a future
# cleanup pass is likely to "fix" it. This gate makes that fail, and requires
# the reason to stay documented in the brand doc and the knowledge base.

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
MAC_BRIDGE="apps/mac/Sources/JarvisMacCore/DeveloperBridge.swift"
MAC_KEYCHAIN="apps/mac/Sources/JarvisMacCore/KeychainDeveloperIdentity.swift"
LIVE_DEVICE_QA="scripts/release-live-device-qa.sh"
BRAND="docs/brand.md"
KB="docs/knowledge-base/assemblywright-project-facts.md"

usage() {
  cat <<'USAGE'
Usage: scripts/release-naming-contract-smoke.sh [--check | --self-test]

Validate the Assemblywright naming contract: renamed product surfaces are
current, legacy jarvis* aliases stay removed, and the compatibility identifiers
that bind signed identity or installed state stay unchanged and documented.

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
  local label="$1"
  local file="$2"
  local expected="$3"
  if ! grep -Fq -- "$expected" "$file"; then
    fail "$label is missing required text in $file: $expected"
  fi
}

forbid_pattern() {
  local label="$1"
  local file="$2"
  local pattern="$3"
  if grep -Eq -- "$pattern" "$file"; then
    fail "$label matched a forbidden legacy pattern in $file: $pattern"
  fi
}

# A preserved identifier must exist where the code binds it, and the reason must
# stay written down where a future rename pass will look.
require_preserved_identifier() {
  local identifier="$1"
  local file="$2"
  require_text "preserved identifier $identifier" "$file" "$identifier"
  require_text "brand doc compatibility names" "$BRAND" "$3"
  require_text "knowledge base compatibility names" "$KB" "$3"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      MODE="check"
      shift
      ;;
    --self-test)
      MODE="self-test"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

self_test() {
  FIXTURE_DIR="$(mktemp -d)"
  trap 'rm -rf "$FIXTURE_DIR"' EXIT

  local fixture="$FIXTURE_DIR/fixture.txt"
  printf 'name = "assemblywright-cli"\n' >"$fixture"

  if (require_text "self-test" "$fixture" 'name = "jarvis-cli"') 2>/dev/null; then
    fail "self-test: require_text accepted absent text"
  fi
  require_text "self-test" "$fixture" 'name = "assemblywright-cli"'

  printf 'name = "jarvis-cli"\n' >"$fixture"
  if (forbid_pattern "self-test" "$fixture" 'name = "jarvis') 2>/dev/null; then
    fail "self-test: forbid_pattern accepted a forbidden match"
  fi
  printf 'name = "assemblywright-cli"\n' >"$fixture"
  forbid_pattern "self-test" "$fixture" 'name = "jarvis'

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
# 1. Renamed surfaces are current.
# ---------------------------------------------------------------------------

for member in \
  '"crates/assemblywright-protocol"' \
  '"crates/assemblywright-master"' \
  '"crates/assemblywright-agent"' \
  '"crates/assemblywright-core"' \
  '"crates/assemblywright-cli"'; do
  require_text "workspace member" "$WORKSPACE_MANIFEST" "$member"
done
require_text "workspace repository" "$WORKSPACE_MANIFEST" \
  "https://github.com/malak333/Assemblywright"

require_text "cli package name" "$CLI_MANIFEST" 'name = "assemblywright-cli"'
require_text "cli binary name" "$CLI_MANIFEST" 'name = "assemblywright"'
require_text "cli default run" "$CLI_MANIFEST" 'default-run = "assemblywright"'
require_text "agent package name" "$AGENT_MANIFEST" 'name = "assemblywright-agent"'
require_text "agent binary name" "$AGENT_MANIFEST" 'default-run = "assemblywright-agent"'
require_text "master package name" "$MASTER_MANIFEST" 'name = "assemblywright-master"'
require_text "master binary name" "$MASTER_MANIFEST" 'default-run = "assemblywright-master"'
require_text "core package name" "$CORE_MANIFEST" 'name = "assemblywright-core"'
require_text "protocol package name" "$PROTOCOL_MANIFEST" 'name = "assemblywright-protocol"'

require_text "swift package name" "$SWIFT_MANIFEST" 'name: "AssemblywrightMac"'
require_text "swift core product" "$SWIFT_MANIFEST" \
  '.library(name: "AssemblywrightMacCore"'
require_text "swift app product" "$SWIFT_MANIFEST" \
  '.executable(name: "AssemblywrightMacApp"'
require_text "swift bridge product" "$SWIFT_MANIFEST" \
  '.executable(name: "assemblywright-mac-bridge"'

require_text "packaging app name" "$PACKAGING" 'APP_NAME="Assemblywright"'
require_text "packaging swift product" "$PACKAGING" \
  'SWIFT_APP_PRODUCT="AssemblywrightMacApp"'
require_text "packaging bundled core binary" "$PACKAGING" \
  'target/release/assemblywright'

# ---------------------------------------------------------------------------
# 2. Legacy aliases stay removed.
# ---------------------------------------------------------------------------

for legacy_crate in \
  crates/jarvis-protocol crates/jarvis-master crates/jarvis-agent \
  crates/jarvis-core crates/jarvis-cli; do
  [[ ! -e "$legacy_crate" ]] || fail "legacy crate directory reappeared: $legacy_crate"
done

for manifest in \
  "$WORKSPACE_MANIFEST" "$CLI_MANIFEST" "$AGENT_MANIFEST" "$MASTER_MANIFEST" \
  "$CORE_MANIFEST" "$PROTOCOL_MANIFEST"; do
  forbid_pattern "legacy cargo name" "$manifest" 'name *= *"jarvis'
  forbid_pattern "legacy cargo default-run" "$manifest" 'default-run *= *"jarvis'
  forbid_pattern "legacy cargo path dependency" "$manifest" 'path *= *"\.\./jarvis-'
done

# SwiftPM products are the public surface of apps/mac. Targets are internal and
# deliberately still Jarvis*, so only product declarations are policed here.
forbid_pattern "legacy swift product alias" "$SWIFT_MANIFEST" \
  '\.(library|executable)\(name: "[Jj]arvis'

# ---------------------------------------------------------------------------
# 3. Compatibility identifiers stay unchanged, with the reason documented.
# ---------------------------------------------------------------------------

require_text "brand doc compatibility section" "$BRAND" "## Compatibility Names"

# The bundle-internal executable name and the bundled CLI filename are bound by
# signed provenance reports and live-device QA reports.
require_preserved_identifier 'APP_EXECUTABLE_NAME="JarvisMacApp"' "$PACKAGING" \
  '`JarvisMacApp`'
require_preserved_identifier 'CORE_EXECUTABLE_NAME="jarvis-cli"' "$PACKAGING" \
  '`jarvis-cli`'

# Code-signing identity. Renaming these invalidates every signed artifact and
# every Keychain item scoped to them.
require_preserved_identifier 'BUNDLE_ID="com.nobiletechnology.jarvis"' "$PACKAGING" \
  'com.nobiletechnology.jarvis'
require_text "bundled core code identity" "$PACKAGING" 'CORE_CODE_ID="${BUNDLE_ID}.core"'
require_preserved_identifier 'com.nobiletechnology.jarvis.developer-bridge' \
  "$MAC_KEYCHAIN" 'Keychain'

# The Windows service name and its installed state directory.
require_preserved_identifier 'DEFAULT_SERVICE_NAME: &str = "JarvisMaster"' \
  "$MASTER_PROCESS" 'JarvisMaster'
require_text "master installed state directory" "$MASTER_PROCESS" \
  '.join("Jarvis").join("master")'

# The TLS exporter label is a wire contract shared by both sides.
require_preserved_identifier 'EXPORTER-Jarvis-Developer-Mode-v1' \
  "$MASTER_PROCESS" 'EXPORTER-Jarvis-Developer-Mode-v1'
require_text "mac exporter label" "$MAC_BRIDGE" \
  'EXPORTER-Jarvis-Developer-Mode-v1'

# The certificate SAN URI is baked into every already-issued device certificate.
# Renaming it silently voids them.
require_preserved_identifier 'urn:jarvis:device:' "$MASTER_IDENTITY" \
  'urn:jarvis:device:'
require_text "master SAN URI verification" "$MASTER_PROCESS" 'urn:jarvis:device:'

# Protocol version 1 fixture capability identity. Renaming these is a wire
# contract change and needs a protocol version bump, not a rename pass.
require_preserved_identifier 'FIXTURE_REASONING_PROVIDER: &str = "jarvis-fixture"' \
  "$PROTOCOL_CRATE" 'jarvis-fixture'
require_preserved_identifier 'FIXTURE_REASONING_MODEL: &str = "jarvis-fixture-v1"' \
  "$PROTOCOL_CRATE" 'jarvis-fixture-v1'

# Application Support namespace of an already-installed app.
require_preserved_identifier 'Library/Application Support/Jarvis' \
  "$LIVE_DEVICE_QA" 'Application Support'

# Environment variable names are an owner-facing contract; renaming them
# silently breaks existing release and QA shell profiles.
require_text "env var namespace" "$BRAND" '`JARVIS_*` environment variables'
for env_var in \
  JARVIS_MASTER_DATA_DIR \
  JARVIS_RELEASE_READINESS_EVIDENCE_MODE \
  JARVIS_IPC_TOKEN_FILE \
  JARVIS_BUNDLE_ID; do
  if ! grep -rqI --exclude-dir=target --exclude-dir=.git -- "$env_var" .; then
    fail "preserved environment variable disappeared: $env_var"
  fi
done

printf 'Assemblywright naming contract: ok\n'
printf 'Proof boundary: source text inspection only; no app was built, signed, notarized, installed, or launched.\n'
