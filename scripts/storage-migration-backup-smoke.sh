#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

run cargo test -p jarvis-core storage::tests::migrating_legacy_file_database_creates_backup_snapshot
run cargo test -p jarvis-core storage::tests::migration_failure_restores_preflight_backup
run cargo test -p jarvis-core storage::tests::newer_schema_version_fails_with_explicit_upgrade_message
run cargo test -p jarvis-core storage::tests::historical_fixture_matrix_migrates_supported_versions

printf '\nJarvis storage migration backup smoke: ok\n'
printf 'Proof boundary: focused Rust storage tests for legacy DB backup, restore after migration-open failure, newer-schema diagnostics, and schema v1-v8 fixture preservation; no installer upgrade or Finder/LaunchServices recovery proof.\n'
