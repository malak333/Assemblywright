#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOCAL_GATE="scripts/release-local.sh"
BUILD_DOCS="docs/build-test-commands.md"
CHECKLIST="docs/release-checklist.md"
ARCHITECTURE="docs/architecture-map.md"
KB="docs/knowledge-base/jarvis-project-facts.md"
README="README.md"

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
    fail "$label does not mention required text in $file: $expected"
  fi
}

require_file "$LOCAL_GATE"
require_file "$BUILD_DOCS"
require_file "$CHECKLIST"
require_file "$ARCHITECTURE"
require_file "$KB"
require_file "$README"

while IFS= read -r command; do
  require_text "build/test command docs" "$BUILD_DOCS" "$command"
  require_text "release checklist" "$CHECKLIST" "$command"
done < <(grep -E '^[[:space:]]*run ' "$LOCAL_GATE" | sed -E 's/^[[:space:]]*run //')

for file in "$BUILD_DOCS" "$CHECKLIST" "$ARCHITECTURE" "$KB" "$README"; do
  require_text "release evidence-mode boundary" "$file" "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external"
  require_text "release command evidence reference" "$file" "task:<uuid>"
  require_text "release command evidence reference" "$file" "audit:<uuid>"
  require_text "owner evidence boundary" "$file" "owner-recorded external evidence"
  require_text "external handoff script" "$file" "release-external-handoff.sh"
done

for file in "$BUILD_DOCS" "$CHECKLIST" "$ARCHITECTURE" "$KB" "$README"; do
  require_text "external handoff checklist" "$file" "release-evidence-checklist.md"
done

require_text "architecture current diagram" "$ARCHITECTURE" "Current Implementation And Evidence Boundary Diagram"
require_text "architecture end-state diagram" "$ARCHITECTURE" "End-Goal Production Architecture"
require_text "release readiness blocker docs" "$CHECKLIST" "live-device QA"
require_text "release readiness blocker docs" "$CHECKLIST" "plugin-trust QA"
require_text "release readiness blocker docs" "$CHECKLIST" "Developer ID"

printf 'Jarvis release docs drift smoke: ok\n'
