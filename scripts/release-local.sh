#!/usr/bin/env bash
set -euo pipefail

INTERNAL_STDIN_MARKER="${ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_STDIN_V1:-}"
INTERNAL_ROOT_MARKER="${ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_ROOT:-}"
unset ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_STDIN_V1
unset ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_ROOT

if [[ "$INTERNAL_STDIN_MARKER" == "assemblywright.repository-gate-proof.v1" ]]; then
  [[ -z "${BASH_SOURCE[0]-}" && "$0" == "bash" && "$#" -eq 0 ]] || {
    printf 'error: repository-gate internal mode requires argument-free bash stdin execution\n' >&2
    exit 1
  }
  [[ "$INTERNAL_ROOT_MARKER" == /* && -d "$INTERNAL_ROOT_MARKER" && ! -L "$INTERNAL_ROOT_MARKER" ]] || {
    printf 'error: repository-gate internal root is invalid\n' >&2
    exit 1
  }
  ROOT_DIR="$(cd "$INTERNAL_ROOT_MARKER" && pwd -P)"
  [[ "$ROOT_DIR" == "$INTERNAL_ROOT_MARKER" ]] || {
    printf 'error: repository-gate internal root is ambiguous\n' >&2
    exit 1
  }
else
  ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
fi
unset INTERNAL_STDIN_MARKER
unset INTERNAL_ROOT_MARKER
cd "$ROOT_DIR"
export CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-$ROOT_DIR/target/clang-module-cache}"
mkdir -p "$CLANG_MODULE_CACHE_PATH"

run() {
  local heartbeat_seconds="${ASSEMBLYWRIGHT_RELEASE_LOCAL_HEARTBEAT_SECONDS:-0}"
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
  if ! output="$(ASSEMBLYWRIGHT_RELEASE_LOCAL_HEARTBEAT_SECONDS=1 run bash -c 'sleep 2' 2>&1)"; then
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
run ./scripts/repository-gate-proof-controller.sh --check
run ./scripts/repository-gate-proof-controller.sh --self-test
run ./scripts/restricted-worker-proof-controller.sh --check
run ./scripts/restricted-worker-proof-controller.sh --self-test
run ./scripts/review-provider-proof-controller.sh --check
run ./scripts/review-provider-proof-controller.sh --self-test
run ./scripts/github-publication-live-e2e.sh --check
run ./scripts/github-publication-proof-controller.sh --check
run ./scripts/github-publication-proof-controller.sh --self-test
run ./scripts/restart-recovery-live-e2e.sh --check
run ./scripts/restart-recovery-proof-controller.sh --check
run ./scripts/restart-recovery-proof-controller.sh --self-test
run ./scripts/mac-windows-control-streaming-proof-controller.sh --check
run ./scripts/mac-windows-control-streaming-proof-controller.sh --self-test
run ./scripts/release-naming-contract-smoke.sh --check
run ./scripts/release-naming-contract-smoke.sh --self-test
run ./scripts/release-shell-portability-smoke.sh --check
run ./scripts/release-shell-portability-smoke.sh --self-test
run ./scripts/release-protocol-version-contract-smoke.sh --check
run ./scripts/release-protocol-version-contract-smoke.sh --self-test
run ./scripts/mac-windows-bridge-live-e2e.sh --check
run ./scripts/windows-repository-onboarding-self-check.sh
run cargo fmt --check
run cargo clippy --workspace --all-targets -- -D warnings
run cargo test --workspace
run cargo test --workspace -- --ignored
run cargo build --workspace
run ./scripts/mac-local-coding-snapshot-e2e.sh
run ./scripts/release-cargo-package.sh
run ./scripts/package-distribution.sh --check
run ./scripts/package-distribution.sh --check-guidance-self-test
run ./scripts/package-distribution.sh --entitlements-policy-self-test
run ./scripts/package-distribution.sh --version-consistency-self-test
run ./scripts/package-distribution.sh --provenance-self-test
run ./scripts/package-distribution.sh --running-app-guard-self-test
run ./scripts/package-distribution.sh --running-app-guard-e2e
run ./scripts/package-distribution.sh --unsigned-launch-check
run cargo run -p assemblywright-cli -- release signed-distribution-runbook
run cargo run -p assemblywright-cli -- release live-device-runbook
run ./scripts/release-live-device-qa.sh --check
run ./scripts/release-live-device-qa.sh --self-test
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
