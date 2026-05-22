#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

run cargo fmt --check
run cargo clippy --workspace --all-targets -- -D warnings
run cargo test --workspace
run cargo test --workspace -- --ignored
run ./scripts/storage-migration-backup-smoke.sh
run cargo build --workspace
run cargo run -p jarvis-cli -- smoke
run ./scripts/release-operator-qa-smoke.sh
run cargo package --workspace --allow-dirty
run ./scripts/package-distribution.sh --unsigned-launch-check
run ./scripts/release-live-device-qa.sh --check

if ! command -v swift >/dev/null 2>&1; then
  printf '\nerror: swift is required for the local release gate because apps/mac exists\n' >&2
  exit 1
fi

run swift test --package-path apps/mac
run swift build --package-path apps/mac

printf '\nJarvis local release verification: ok\n'
