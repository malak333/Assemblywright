#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

WORKFLOW=".github/workflows/release-local.yml"
WINDOWS_PROTOCOL_WORKFLOW=".github/workflows/windows-protocol.yml"
LOCAL_GATE="scripts/release-local.sh"

require_file() {
  if [[ ! -f "$1" ]]; then
    printf 'error: missing %s\n' "$1" >&2
    exit 1
  fi
}

require_text() {
  local needle="$1"
  local file="$2"
  if ! grep -Fq -- "$needle" "$file"; then
    printf 'error: expected %s to contain: %s\n' "$file" "$needle" >&2
    exit 1
  fi
}

require_file "$WORKFLOW"
require_file "$WINDOWS_PROTOCOL_WORKFLOW"
require_file "$LOCAL_GATE"

require_text "name: Assemblywright Release Local Gate" "$WORKFLOW"
require_text "pull_request:" "$WORKFLOW"
require_text "push:" "$WORKFLOW"
require_text "workflow_dispatch:" "$WORKFLOW"
require_text "contents: read" "$WORKFLOW"
require_text "runs-on: macos-15" "$WORKFLOW"
require_text "uses: actions/checkout@93cb6efe18208431cddfb8368fd83d5badbf9bfd" "$WORKFLOW"
require_text "uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8" "$WORKFLOW"
require_text "toolchain: 1.95.0" "$WORKFLOW"
require_text "components: clippy,rustfmt" "$WORKFLOW"
require_text "swift --version" "$WORKFLOW"
require_text "JARVIS_RELEASE_LOCAL_HEARTBEAT_SECONDS: \"60\"" "$WORKFLOW"
require_text "run: ./scripts/release-local.sh" "$WORKFLOW"
require_text "name: Assemblywright Windows Distributed Gate" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "pull_request:" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "push:" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "workflow_dispatch:" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "contents: read" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "runs-on: windows-latest" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "uses: actions/checkout@93cb6efe18208431cddfb8368fd83d5badbf9bfd" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "toolchain: 1.95.0" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "components: clippy,rustfmt" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "cargo fmt --all --check" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "cargo clippy -p jarvis-protocol --all-targets --locked -- -D warnings" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "cargo clippy -p jarvis-master --all-targets --locked -- -D warnings" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "cargo test -p jarvis-protocol --locked" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "cargo test -p jarvis-master --locked" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text 'JARVIS_REQUIRE_WINDOWS_SERVICE_E2E: "1"' "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "cargo test -p jarvis-master --test windows_service_lifecycle_e2e --locked -- --ignored --nocapture" "$WINDOWS_PROTOCOL_WORKFLOW"
require_text "still running after" "$LOCAL_GATE"
require_text "completed in" "$LOCAL_GATE"
require_text "command failed after" "$LOCAL_GATE"
require_text "--heartbeat-self-test" "$LOCAL_GATE"

"$ROOT_DIR/$LOCAL_GATE" --heartbeat-self-test

expected_local_gate_commands=(
  "run ./scripts/release-version-consistency.sh --check"
  "run ./scripts/release-ci-workflow-smoke.sh"
  "run ./scripts/release-docs-drift-smoke.sh"
  "run ./scripts/mac-windows-bridge-live-e2e.sh --check"
  "run cargo fmt --check"
  "run cargo clippy --workspace --all-targets -- -D warnings"
  "run cargo test --workspace"
  "run cargo test --workspace -- --ignored"
  "run cargo build --workspace"
  "run ./scripts/release-cargo-package.sh"
  "run ./scripts/package-distribution.sh --check"
  "run ./scripts/package-distribution.sh --check-guidance-self-test"
  "run ./scripts/package-distribution.sh --entitlements-policy-self-test"
  "run ./scripts/package-distribution.sh --version-consistency-self-test"
  "run ./scripts/package-distribution.sh --provenance-self-test"
  "run ./scripts/package-distribution.sh --running-app-guard-self-test"
  "run ./scripts/package-distribution.sh --running-app-guard-e2e"
  "run ./scripts/package-distribution.sh --unsigned-launch-check"
  "run cargo run -p jarvis-cli -- release signed-distribution-runbook"
  "run cargo run -p jarvis-cli -- release live-device-runbook"
  "run ./scripts/release-live-device-qa.sh --check"
  "run ./scripts/release-live-device-qa.sh --self-test"
  "run ./scripts/release-evidence-bundle.sh --check"
  "run ./scripts/release-evidence-bundle.sh --self-test"
  "run ./scripts/release-evidence-doctor.sh --check"
  "run ./scripts/release-evidence-doctor.sh --self-test"
  "run ./scripts/release-external-handoff.sh --check"
  "run ./scripts/release-external-handoff.sh --self-test"
  "run swift test --disable-sandbox --package-path apps/mac"
  "run swift build --disable-sandbox --package-path apps/mac"
)

actual_local_gate_commands=()
while IFS= read -r command; do
  actual_local_gate_commands+=("$command")
done < <(grep -E '^[[:space:]]*run ' "$LOCAL_GATE" | sed -E 's/^[[:space:]]*//')

if [[ "${#actual_local_gate_commands[@]}" -ne "${#expected_local_gate_commands[@]}" ]]; then
  printf 'error: expected %s release-local run commands, found %s\n' \
    "${#expected_local_gate_commands[@]}" "${#actual_local_gate_commands[@]}" >&2
  printf 'actual release-local run command manifest:\n' >&2
  printf '  %s\n' "${actual_local_gate_commands[@]}" >&2
  exit 1
fi

for index in "${!expected_local_gate_commands[@]}"; do
  if [[ "${actual_local_gate_commands[$index]}" != "${expected_local_gate_commands[$index]}" ]]; then
    printf 'error: release-local command %s mismatch\n' "$((index + 1))" >&2
    printf '  expected: %s\n' "${expected_local_gate_commands[$index]}" >&2
    printf '  actual:   %s\n' "${actual_local_gate_commands[$index]}" >&2
    exit 1
  fi
done

printf 'Assemblywright release CI workflow smoke: ok\n'
