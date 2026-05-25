#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

WORKFLOW=".github/workflows/release-local.yml"

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

require_text "name: Jarvis Release Local Gate" "$WORKFLOW"
require_text "pull_request:" "$WORKFLOW"
require_text "push:" "$WORKFLOW"
require_text "workflow_dispatch:" "$WORKFLOW"
require_text "contents: read" "$WORKFLOW"
require_text "runs-on: macos-latest" "$WORKFLOW"
require_text "uses: actions/checkout@v4" "$WORKFLOW"
require_text "uses: dtolnay/rust-toolchain@stable" "$WORKFLOW"
require_text "swift --version" "$WORKFLOW"
require_text "run: ./scripts/release-local.sh" "$WORKFLOW"

printf 'Jarvis release CI workflow smoke: ok\n'
