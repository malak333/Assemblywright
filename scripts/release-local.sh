#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
export CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-$ROOT_DIR/target/clang-module-cache}"
mkdir -p "$CLANG_MODULE_CACHE_PATH"

run() {
  local heartbeat_seconds="${JARVIS_RELEASE_LOCAL_HEARTBEAT_SECONDS:-0}"
  local started_at start_epoch end_epoch duration status
  started_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  start_epoch="$(date -u '+%s')"
  printf '\n==> [%s] %s\n' "$started_at" "$*"

  if [[ "$heartbeat_seconds" =~ ^[0-9]+$ && "$heartbeat_seconds" -gt 0 ]]; then
    local pid heartbeat_pid
    "$@" &
    pid="$!"
    (
      while sleep "$heartbeat_seconds"; do
        if ! kill -0 "$pid" >/dev/null 2>&1; then
          exit 0
        fi
        now="$(date -u '+%s')"
        elapsed=$((now - start_epoch))
        printf '==> [%s] still running after %ss: %s\n' \
          "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$elapsed" "$*"
      done
    ) &
    heartbeat_pid="$!"
    set +e
    wait "$pid"
    status="$?"
    kill "$heartbeat_pid" >/dev/null 2>&1
    wait "$heartbeat_pid" >/dev/null 2>&1
    set -e
  else
    set +e
    "$@"
    status="$?"
    set -e
  fi

  end_epoch="$(date -u '+%s')"
  duration=$((end_epoch - start_epoch))
  if [[ "$status" -ne 0 ]]; then
    printf '==> command failed after %ss with status %s: %s\n' \
      "$duration" "$status" "$*" >&2
    return "$status"
  fi
  printf '==> completed in %ss: %s\n' "$duration" "$*"
}

heartbeat_self_test() {
  local output
  if ! output="$(JARVIS_RELEASE_LOCAL_HEARTBEAT_SECONDS=1 run bash -c 'sleep 2' 2>&1)"; then
    printf '%s\n' "$output" >&2
    printf 'error: release-local heartbeat self-test command failed\n' >&2
    exit 1
  fi
  if [[ "$output" != *"still running after"* ]]; then
    printf '%s\n' "$output" >&2
    printf 'error: release-local heartbeat self-test did not emit heartbeat output\n' >&2
    exit 1
  fi
  if [[ "$output" != *"completed in"* ]]; then
    printf '%s\n' "$output" >&2
    printf 'error: release-local heartbeat self-test did not emit completion output\n' >&2
    exit 1
  fi
  printf 'Assemblywright release-local heartbeat self-test: ok\n'
}

if [[ "${1:-}" == "--heartbeat-self-test" ]]; then
  heartbeat_self_test
  exit 0
fi

run ./scripts/release-version-consistency.sh --check
run ./scripts/release-ci-workflow-smoke.sh
run ./scripts/release-docs-drift-smoke.sh
run ./scripts/mac-windows-bridge-live-e2e.sh --check
run cargo fmt --check
run cargo clippy --workspace --all-targets -- -D warnings
run cargo test --workspace
run cargo test --workspace -- --ignored
run ./scripts/storage-migration-backup-smoke.sh
run cargo build --workspace
run cargo run -p jarvis-cli -- smoke
run ./scripts/release-operator-qa-smoke.sh
run ./scripts/release-cargo-package.sh
run ./scripts/package-distribution.sh --check
run ./scripts/package-distribution.sh --check-guidance-self-test
run ./scripts/package-distribution.sh --entitlements-policy-self-test
run ./scripts/package-distribution.sh --version-consistency-self-test
run ./scripts/package-distribution.sh --provenance-self-test
run ./scripts/package-distribution.sh --running-app-guard-self-test
run ./scripts/package-distribution.sh --running-app-guard-e2e
run ./scripts/package-distribution.sh --unsigned-launch-check
run cargo run -p jarvis-cli -- release signed-distribution-runbook
run cargo run -p jarvis-cli -- release live-device-runbook
run cargo run -p jarvis-cli -- release plugin-trust-runbook
run ./scripts/release-live-device-qa.sh --check
run ./scripts/release-live-device-qa.sh --self-test
run ./scripts/release-plugin-trust-qa.sh --check
run ./scripts/release-plugin-trust-qa.sh --self-test
run ./scripts/release-evidence-bundle.sh --check
run ./scripts/release-evidence-bundle.sh --self-test
run ./scripts/release-evidence-doctor.sh --check
run ./scripts/release-evidence-doctor.sh --self-test
run ./scripts/release-external-handoff.sh --check
run ./scripts/release-external-handoff.sh --self-test

if ! command -v swift >/dev/null 2>&1; then
  printf '\nerror: swift is required for the local release gate because apps/mac exists\n' >&2
  exit 1
fi

run swift test --disable-sandbox --package-path apps/mac
run swift build --disable-sandbox --package-path apps/mac

printf '\nAssemblywright local release verification: ok\n'
