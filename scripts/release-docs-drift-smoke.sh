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

forbid_text() {
  local label="$1"
  local file="$2"
  local unexpected="$3"
  if grep -Fq -- "$unexpected" "$file"; then
    fail "$label found stale text in $file: $unexpected"
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
  require_text "release evidence-mode field" "$file" "evidence_mode_enabled"
  require_text "release command evidence reference" "$file" "task:<uuid>"
  require_text "release command evidence reference" "$file" "audit:<uuid>"
  require_text "owner evidence boundary" "$file" "owner-recorded external evidence"
  require_text "external handoff script" "$file" "release-external-handoff.sh"
done

for file in "$BUILD_DOCS" "$CHECKLIST" "$ARCHITECTURE" "$KB" "$README"; do
  require_text "external handoff checklist" "$file" "release-evidence-checklist.md"
  require_text "external handoff manifest" "$file" "release-handoff-manifest.json"
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
require_text "architecture current readiness baseline" "$ARCHITECTURE" 'latest verified main baseline'
require_text "architecture current readiness commit" "$ARCHITECTURE" '`main` commit `8d61ad7`'
require_text "architecture current readiness run" "$ARCHITECTURE" '27849385053'
require_text "architecture current readiness refresh command" "$ARCHITECTURE" 'cargo run -p jarvis-cli -- release readiness --json'
require_text "architecture current readiness refresh command" "$ARCHITECTURE" 'gh run list --branch main --workflow "Jarvis Release Local Gate"'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #312 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `4b36c14`'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `73dbd54`'
forbid_text "architecture stale readiness run" "$ARCHITECTURE" '27847887169'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #311 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `4417187`'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #310 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `27c33f5`'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #309 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `38bd79e`'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #308 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `af943e8`'
forbid_text "architecture stale readiness baseline" "$ARCHITECTURE" 'current post-PR #301 baseline at'
forbid_text "architecture stale readiness commit" "$ARCHITECTURE" '`main` commit `155ccd4`'
require_text "architecture model-step progress boundary" "$ARCHITECTURE" "model-step, and redacted model-output chunk progress frames"
require_text "architecture model-output progress boundary" "$ARCHITECTURE" "redacted model-output chunk progress frames"
require_text "architecture handoff manifest digest" "$ARCHITECTURE" "handoff manifest digest-binding"
require_text "architecture handoff manifest self-test" "$ARCHITECTURE" "--self-test\` verifies the expected file list"
require_text "architecture release-local heartbeat" "$ARCHITECTURE" "release-local command heartbeat"
require_text "architecture final bundle output collision guard" "$ARCHITECTURE" "final bundle writer must also reject"
require_text "architecture final bundle output collision guard" "$ARCHITECTURE" "output paths that collide with signed-provenance"
require_text "knowledge base current readiness baseline" "$KB" 'latest verified main baseline at `8d61ad7`'
require_text "knowledge base current readiness run" "$KB" '27849385053'
require_text "knowledge base current readiness refresh command" "$KB" 'cargo run -p jarvis-cli -- release readiness --json'
require_text "knowledge base current readiness refresh command" "$KB" 'gh run list --branch main --workflow "Jarvis Release Local Gate"'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #312 baseline at `4b36c14`'
forbid_text "knowledge base stale readiness baseline" "$KB" 'latest verified main baseline at `73dbd54`'
forbid_text "knowledge base stale readiness run" "$KB" '27847887169'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #311 baseline at `4417187`'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #310 baseline at `27c33f5`'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #309 baseline at `38bd79e`'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #308 baseline at `af943e8`'
forbid_text "knowledge base stale readiness baseline" "$KB" 'current post-PR #301 baseline at `155ccd4`'
require_text "knowledge base model-step progress boundary" "$KB" "model-step completion/failure audit evidence"
require_text "knowledge base model-output progress boundary" "$KB" "content_redacted: true"
require_text "knowledge base model-step progress proof" "$KB" "installed-plugin plus model-step/model-output"
require_text "knowledge base model-output progress proof" "$KB" "model-step/model-output"
require_text "knowledge base handoff manifest digest" "$KB" "manifest digests match"
require_text "knowledge base handoff manifest self-test" "$KB" "shell self-test verifies the expected file list"
require_text "knowledge base handoff evidence-status parity" "$KB" "external-mode direct CLI evidence-status query"
require_text "knowledge base missing live-device field E2E" "$KB" "removes required"
require_text "knowledge base missing live-device field E2E" "$KB" "notification-observation fields"
require_text "knowledge base plugin-trust script E2E" "$KB" "release-plugin-trust-qa.sh --assert-complete"
require_text "knowledge base plugin-trust script E2E" "$KB" "plugin-trust QA report"
require_text "knowledge base release-local heartbeat" "$KB" "release-local command heartbeat"
require_text "knowledge base final bundle output collision guard" "$KB" "final bundle output path must also be distinct"
require_text "architecture plugin-trust script E2E" "$ARCHITECTURE" "rebinds the generated"
require_text "architecture plugin-trust script E2E" "$ARCHITECTURE" "bundle as present"
require_text "build docs model-step progress command" "$BUILD_DOCS" "repository_backed_state_endpoints_expose_tasks_and_audit"
require_text "build docs handoff manifest digest" "$BUILD_DOCS" "per-file SHA-256 digests"
require_text "build docs handoff manifest self-test" "$BUILD_DOCS" "handoff shell self-test verifies the expected manifest file list"
require_text "build docs handoff evidence-status parity" "$BUILD_DOCS" "fresh external-mode direct CLI evidence-status query"
require_text "build docs missing live-device field E2E" "$BUILD_DOCS" "removes required live voice"
require_text "build docs missing live-device field E2E" "$BUILD_DOCS" "external-mode"
require_text "build docs missing live-device field E2E" "$BUILD_DOCS" "readiness fail closed"
require_text "build docs plugin-trust script E2E" "$BUILD_DOCS" "release_plugin_trust_qa_assertion_report_is_accepted_by_evidence_status"
require_text "build docs plugin-trust script E2E" "$BUILD_DOCS" "plugin-trust QA report"
require_text "build docs release-local heartbeat" "$BUILD_DOCS" "JARVIS_RELEASE_LOCAL_HEARTBEAT_SECONDS"
require_text "build docs release-local heartbeat self-test" "$BUILD_DOCS" "--heartbeat-self-test"
require_text "build docs final bundle output collision guard" "$BUILD_DOCS" "final bundle output path must also be distinct"
require_text "release checklist handoff manifest digest" "$CHECKLIST" "byte counts"
require_text "release checklist model-output chunk boundary" "$CHECKLIST" "model_output_chunk"
require_text "release checklist model-output redaction boundary" "$CHECKLIST" "content_redacted: true"
require_text "release checklist final bundle output collision guard" "$CHECKLIST" "bundle output path is distinct from the signed-distribution provenance"
require_text "release checklist missing live-device field E2E" "$CHECKLIST" "Missing required live voice evidence"
require_text "release checklist missing live-device field E2E" "$CHECKLIST" "external-mode readiness fail closed"
require_text "release checklist plugin-trust script E2E" "$CHECKLIST" "generated plugin-trust QA report and bundle as present"
require_text "build docs external handoff snapshot command" "$BUILD_DOCS" "release_external_handoff_snapshots_match_live_runbook_commands"
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
