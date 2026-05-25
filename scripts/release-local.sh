#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
export CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-$ROOT_DIR/target/clang-module-cache}"
mkdir -p "$CLANG_MODULE_CACHE_PATH"

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

run ./scripts/release-version-consistency.sh --check
run ./scripts/release-ci-workflow-smoke.sh
run cargo fmt --check
run cargo clippy --workspace --all-targets -- -D warnings
run cargo test --workspace
run cargo test --workspace -- --ignored
run ./scripts/storage-migration-backup-smoke.sh
run cargo build --workspace
run cargo run -p jarvis-cli -- smoke
run ./scripts/release-operator-qa-smoke.sh
run cargo package --workspace --allow-dirty
run ./scripts/package-distribution.sh --version-consistency-self-test
run ./scripts/package-distribution.sh --unsigned-launch-check
run cargo run -p jarvis-cli -- release live-device-runbook
run ./scripts/release-live-device-qa.sh --check
run ./scripts/release-live-device-qa.sh --self-test
run ./scripts/release-plugin-trust-qa.sh --check
run ./scripts/release-plugin-trust-qa.sh --self-test
run ./scripts/release-evidence-bundle.sh --check
run ./scripts/release-evidence-bundle.sh --self-test
run ./scripts/release-evidence-doctor.sh --check
run ./scripts/release-evidence-doctor.sh --self-test

if ! command -v swift >/dev/null 2>&1; then
  printf '\nerror: swift is required for the local release gate because apps/mac exists\n' >&2
  exit 1
fi

run swift test --disable-sandbox --package-path apps/mac
run swift build --disable-sandbox --package-path apps/mac

printf '\nJarvis local release verification: ok\n'
