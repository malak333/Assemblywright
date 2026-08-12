#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_PATH="$ROOT_DIR/apps/mac"
AGENT_BIN="$ROOT_DIR/target/debug/assemblywright-agent"
E2E_ROOT="$(mktemp -d -t assemblywright-local-coding-native-e2e)"
DATA_DIR="$E2E_ROOT/data"

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

cleanup() {
  local metadata
  [[ -n "$E2E_ROOT" && -d "$E2E_ROOT" ]] || return 0
  [[ "$(basename "$E2E_ROOT")" == assemblywright-local-coding-native-e2e.* ]] || return 0
  metadata="$(stat -f '%Su:%Lp' "$E2E_ROOT" 2>/dev/null || true)"
  [[ "$metadata" == "$(id -un):700" ]] || return 0
  find "$E2E_ROOT" -depth -delete
}
trap cleanup EXIT

command -v cargo >/dev/null 2>&1 || fail "cargo is required"
command -v codesign >/dev/null 2>&1 || fail "codesign is required"
command -v swift >/dev/null 2>&1 || fail "swift is required"

chmod 700 "$E2E_ROOT"
mkdir "$DATA_DIR"
chmod 700 "$DATA_DIR"

cd "$ROOT_DIR"
cargo build -p assemblywright-agent
[[ -x "$AGENT_BIN" ]] || fail "the debug assemblywright-agent executable is missing"
codesign --verify --strict "$AGENT_BIN" \
  || fail "the debug assemblywright-agent signature is invalid"

if ! output="$(
  ASSEMBLYWRIGHT_MAC_LOCAL_CODING_NATIVE_E2E=true \
  ASSEMBLYWRIGHT_MAC_LOCAL_CODING_AGENT_EXECUTABLE="$AGENT_BIN" \
  ASSEMBLYWRIGHT_MAC_LOCAL_CODING_AGENT_DATA_DIR="$DATA_DIR" \
  swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
    --filter localCodingSnapshotRelayUsesRealSupervisedAgent 2>&1
)"; then
  printf '%s\n' "$output" >&2
  fail "the production Swift-to-Rust local-coding snapshot E2E failed"
fi
printf '%s\n' "$output"
[[ "$output" == *"assemblywright_mac_local_coding_native_e2e_ok"* ]] \
  || fail "the native local-coding snapshot E2E omitted its proof marker"
[[ "$output" == *"assemblywright_mac_local_coding_native_cancellation_e2e_ok"* ]] \
  || fail "the native local-coding cancellation E2E omitted its proof marker"
[[ "$output" == *"general_coding=verified"* \
  && "$output" == *"retained_attempt_pair=verified"* \
  && "$output" == *"final_verification_cancellation=verified"* \
  && "$output" == *"transport_unblock=verified"* \
  && "$output" == *"local_cancel=verified"* \
  && "$output" == *"cleanup_before_ack=verified"* \
  && "$output" == *"no_result=verified"* ]] \
  || fail "the native local-coding cancellation E2E omitted bounded proof claims"

printf 'Assemblywright Mac local-coding snapshot native E2E: ok\n'
