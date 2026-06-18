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
require_text "architecture manual evidence boundary" "$ARCHITECTURE" "Manual external evidence, not local gate proof"
require_text "architecture signed artifact boundary" "$ARCHITECTURE" "Developer ID signed, notarized, and stapled"
require_text "architecture clean-profile QA" "$ARCHITECTURE" "clean-profile install and Finder/LaunchServices"
require_text "architecture manual installed-app QA" "$ARCHITECTURE" "manual installed-app command, audit, memory, scheduler, plugin, pause, diagnostics, notifications, restart QA"
require_text "architecture notification QA" "$ARCHITECTURE" "live macOS notification prompt and delivery QA"
require_text "architecture repository command evidence" "$ARCHITECTURE" "repository-backed task/audit command-result evidence"
require_text "architecture final archive evidence" "$ARCHITECTURE" "archived final release evidence bundle"
require_text "architecture local gate boundary" "$ARCHITECTURE" "does not perform signing,"
require_text "architecture local gate boundary" "$ARCHITECTURE" "notarization, clean-profile install, Finder/LaunchServices launch, live-device"
require_text "architecture local gate boundary" "$ARCHITECTURE" "QA, plugin-trust QA, or final evidence bundling"
require_text "architecture post-merge cleanup audit" "$ARCHITECTURE" "post-merge cleanup audit: open PRs, main workflow runs, worktrees, merged/unmerged codex branches, clean checkout"
for file in "$BUILD_DOCS" "$CHECKLIST" "$KB"; do
  require_text "runbook payload contract boundary" "$file" "operator/snapshot JSON"
  require_text "runbook payload contract boundary" "$file" "ReleaseRunbookResponse"
  require_text "post-merge cleanup audit" "$file" "gh pr list --state open --json number,title,headRefName,baseRefName,url"
  require_text "post-merge cleanup audit" "$file" "gh run list --workflow release-local.yml --branch main --limit 5"
  require_text "post-merge cleanup audit" "$file" "git worktree list --porcelain"
  require_text "post-merge cleanup audit" "$file" "git branch --merged main --list 'codex/*'"
  require_text "post-merge cleanup audit" "$file" "git branch --no-merged main --list 'codex/*'"
  require_text "post-merge cleanup audit" "$file" "git status --short --branch"
done
require_text "release readiness blocker docs" "$CHECKLIST" "live-device QA"
require_text "release readiness blocker docs" "$CHECKLIST" "plugin-trust QA"
require_text "release readiness blocker docs" "$CHECKLIST" "Developer ID"

printf 'Jarvis release docs drift smoke: ok\n'
