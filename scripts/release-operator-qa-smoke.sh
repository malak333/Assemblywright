#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

require_output_contains() {
  local label="$1"
  local output="$2"
  local expected="$3"
  if [[ "$output" != *"$expected"* ]]; then
    printf 'error: %s did not include %q\n' "$label" "$expected" >&2
    printf '%s\n%s\n%s\n' "--- $label output ---" "$output" "--- end $label output ---" >&2
    exit 1
  fi
}

require_output_omits() {
  local label="$1"
  local output="$2"
  local forbidden="$3"
  if [[ "$output" == *"$forbidden"* ]]; then
    printf 'error: %s unexpectedly included %q\n' "$label" "$forbidden" >&2
    printf '%s\n%s\n%s\n' "--- $label output ---" "$output" "--- end $label output ---" >&2
    exit 1
  fi
}

json_string_field() {
  local field="$1"
  local output="$2"
  printf '%s\n' "$output" | sed -n "s/.*\"$field\":\"\\([^\"]*\\)\".*/\\1/p" | head -n 1
}

select_port() {
  if [[ -n "${JARVIS_RELEASE_OPERATOR_QA_PORT:-}" ]]; then
    printf '%s\n' "$JARVIS_RELEASE_OPERATOR_QA_PORT"
    return
  fi

  for port in 18877 18878 18879 18880 18881; do
    if ! nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
      printf '%s\n' "$port"
      return
    fi
  done

  printf 'error: no release operator QA port is available\n' >&2
  exit 1
}

wait_for_health() {
  local label="$1"
  local health_output=""

  for _ in {1..60}; do
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
      printf 'error: %s server exited before health check passed; log follows\n' "$label" >&2
      cat "$SERVER_LOG" >&2 || true
      exit 1
    fi

    if health_output="$("$JARVIS" health --endpoint "$ENDPOINT" 2>/dev/null)"; then
      require_output_contains "$label health" "$health_output" "jarvis-core: ok"
      require_output_contains "$label health" "$health_output" "runtime: routed-fake-local-model+first-party-plugins"
      return
    fi
    sleep 0.25
  done

  printf 'error: %s server did not become healthy; log follows\n' "$label" >&2
  cat "$SERVER_LOG" >&2 || true
  exit 1
}

start_server() {
  local label="$1"
  SERVER_LOG="$TMP_DIR/$label.log"
  printf '\n==> Starting %s repository-backed core at %s\n' "$label" "$ENDPOINT"
  "$JARVIS" serve --bind "127.0.0.1:$PORT" --db-path "$DB_PATH" >"$SERVER_LOG" 2>&1 &
  SERVER_PID="$!"
  wait_for_health "$label"
}

stop_server() {
  if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  SERVER_PID=""
}

cleanup() {
  stop_server
  if command -v lsof >/dev/null 2>&1; then
    while IFS= read -r pid; do
      if [[ -n "$pid" ]]; then
        kill "$pid" 2>/dev/null || true
      fi
    done < <(lsof -ti "tcp:$PORT" 2>/dev/null || true)
  fi
  rm -rf "$TMP_DIR"
}

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-release-operator-qa.XXXXXX")"
PORT="$(select_port)"
ENDPOINT="http://127.0.0.1:$PORT"
DB_PATH="$TMP_DIR/jarvis-release-operator-qa.sqlite"
JARVIS="$ROOT_DIR/target/debug/jarvis"
SERVER_PID=""
SERVER_LOG="$TMP_DIR/server.log"
trap cleanup EXIT

run cargo build -p jarvis-cli
[[ -x "$JARVIS" ]] || {
  printf 'error: jarvis CLI executable missing at %s\n' "$JARVIS" >&2
  exit 1
}

start_server "initial"

PLUGIN_LIST_OUTPUT="$("$JARVIS" plugins list --json --endpoint "$ENDPOINT")"
require_output_contains "plugin manifest list" "$PLUGIN_LIST_OUTPUT" '"id":"system_status"'
require_output_omits "plugin manifest list" "$PLUGIN_LIST_OUTPUT" '"id":"fake_'

COMMAND_OUTPUT="$("$JARVIS" command --json "status" --endpoint "$ENDPOINT")"
require_output_contains "operator QA command" "$COMMAND_OUTPUT" '"accepted":true'
require_output_contains "operator QA command" "$COMMAND_OUTPUT" '"status":"completed"'
require_output_contains "operator QA command" "$COMMAND_OUTPUT" '"event_type":"plugin_completed"'

TASKS_OUTPUT="$("$JARVIS" tasks list --json --endpoint "$ENDPOINT")"
require_output_contains "operator QA tasks" "$TASKS_OUTPUT" '"status":"completed"'

AUDIT_OUTPUT="$("$JARVIS" tasks audit --json --endpoint "$ENDPOINT")"
require_output_contains "operator QA audit" "$AUDIT_OUTPUT" '"event_type":"plugin_completed"'
require_output_contains "operator QA audit" "$AUDIT_OUTPUT" '"event_type":"task_completed"'

ROUTES_OUTPUT="$("$JARVIS" routes list --json --endpoint "$ENDPOINT")"
require_output_contains "operator QA routes" "$ROUTES_OUTPUT" '"selected_provider":"local"'
require_output_contains "operator QA routes" "$ROUTES_OUTPUT" '"local_model":"fake-local-model"'

MEMORY_CREATE_OUTPUT="$("$JARVIS" memory create release_qa operator_smoke "initial operator QA memory" --provenance "release operator QA smoke" --sensitivity personal --endpoint "$ENDPOINT")"
require_output_contains "operator QA memory create" "$MEMORY_CREATE_OUTPUT" '"category":"release_qa"'
MEMORY_ID="$(json_string_field id "$MEMORY_CREATE_OUTPUT")"
if [[ -z "$MEMORY_ID" ]]; then
  printf 'error: could not parse created memory id\n%s\n' "$MEMORY_CREATE_OUTPUT" >&2
  exit 1
fi

MEMORY_UPDATE_OUTPUT="$("$JARVIS" memory update "$MEMORY_ID" "updated operator QA memory" --provenance "release operator QA smoke update" --sensitivity workspace --endpoint "$ENDPOINT")"
require_output_contains "operator QA memory update" "$MEMORY_UPDATE_OUTPUT" '"value":"updated operator QA memory"'

MEMORY_REVIEW_OUTPUT="$("$JARVIS" memory review "$MEMORY_ID" --endpoint "$ENDPOINT")"
require_output_contains "operator QA memory review" "$MEMORY_REVIEW_OUTPUT" '"reviewed_at":"'

MEMORY_CLASSIFICATION_OUTPUT="$("$JARVIS" memory classification --include-deleted --endpoint "$ENDPOINT")"
require_output_contains "operator QA memory classification" "$MEMORY_CLASSIFICATION_OUTPUT" '"label":"release_qa"'

MEMORY_DELETE_OUTPUT="$("$JARVIS" memory delete "$MEMORY_ID" --endpoint "$ENDPOINT")"
require_output_contains "operator QA memory delete" "$MEMORY_DELETE_OUTPUT" '"deleted_at":"'

MEMORY_RESTORE_OUTPUT="$("$JARVIS" memory restore "$MEMORY_ID" --endpoint "$ENDPOINT")"
require_output_contains "operator QA memory restore" "$MEMORY_RESTORE_OUTPUT" '"deleted_at":null'

SCHEDULER_RUN_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
SCHEDULER_CREATE_OUTPUT="$("$JARVIS" scheduler schedule "release operator QA due job" "plugin status" --once-at "$SCHEDULER_RUN_AT" --endpoint "$ENDPOINT")"
require_output_contains "operator QA scheduler create" "$SCHEDULER_CREATE_OUTPUT" '"name":"release operator QA due job"'

SCHEDULER_ATTENTION_OUTPUT="$("$JARVIS" scheduler attention --endpoint "$ENDPOINT")"
require_output_contains "operator QA scheduler attention" "$SCHEDULER_ATTENTION_OUTPUT" '"attention_required":true'
require_output_contains "operator QA scheduler attention" "$SCHEDULER_ATTENTION_OUTPUT" '"notification_reason"'

SCHEDULER_RUN_OUTPUT="$("$JARVIS" scheduler run-due --limit 4 --endpoint "$ENDPOINT")"
require_output_contains "operator QA scheduler run" "$SCHEDULER_RUN_OUTPUT" '"accepted":false'
require_output_contains "operator QA scheduler run" "$SCHEDULER_RUN_OUTPUT" '"event_type":"scheduler_job_failed"'
require_output_contains "operator QA scheduler run" "$SCHEDULER_RUN_OUTPUT" '"event_type":"scheduler_proactive_policy_checked"'
require_output_contains "operator QA scheduler run" "$SCHEDULER_RUN_OUTPUT" '"event_type":"scheduler_fail_closed_emergency_pause"'

SCHEDULER_RESUME_OUTPUT="$("$JARVIS" resume --endpoint "$ENDPOINT")"
require_output_contains "operator QA scheduler fail-closed resume" "$SCHEDULER_RESUME_OUTPUT" '"paused":false'

ACTIVITY_OUTPUT="$("$JARVIS" activity summary --json --endpoint "$ENDPOINT")"
require_output_contains "operator QA activity summary" "$ACTIVITY_OUTPUT" '"active_task_count":0'
require_output_contains "operator QA activity summary" "$ACTIVITY_OUTPUT" '"completed"'

WATCH_OUTPUT="$("$JARVIS" activity watch --max-events 1 --interval-ms 10 --endpoint "$ENDPOINT")"
require_output_contains "operator QA activity watch" "$WATCH_OUTPUT" 'event: activity_summary'

PERMISSIONS_OUTPUT="$("$JARVIS" permissions review --endpoint "$ENDPOINT")"
require_output_contains "operator QA permissions review" "$PERMISSIONS_OUTPUT" '"generated_at":"'
require_output_contains "operator QA permissions review" "$PERMISSIONS_OUTPUT" '"items"'

DIAGNOSTICS_OUTPUT="$("$JARVIS" diagnostics export --endpoint "$ENDPOINT")"
require_output_contains "operator QA diagnostics" "$DIAGNOSTICS_OUTPUT" '"repository_backed":true'
require_output_contains "operator QA diagnostics" "$DIAGNOSTICS_OUTPUT" '"redaction":"diagnostics export omits command bodies'

PAUSE_OUTPUT="$("$JARVIS" pause --endpoint "$ENDPOINT" --reason "release operator QA smoke")"
require_output_contains "operator QA pause" "$PAUSE_OUTPUT" '"paused":true'

BLOCKED_OUTPUT="$("$JARVIS" command --json "status" --endpoint "$ENDPOINT" --dry-run)"
require_output_contains "operator QA blocked command" "$BLOCKED_OUTPUT" '"accepted":false'
require_output_contains "operator QA blocked command" "$BLOCKED_OUTPUT" '"status":"blocked"'

RESUME_OUTPUT="$("$JARVIS" resume --endpoint "$ENDPOINT")"
require_output_contains "operator QA resume" "$RESUME_OUTPUT" '"paused":false'

RELEASE_READINESS_OUTPUT="$("$JARVIS" release readiness --json --endpoint "$ENDPOINT")"
require_output_contains "operator QA release readiness" "$RELEASE_READINESS_OUTPUT" '"production_ready":false'
require_output_contains "operator QA release readiness" "$RELEASE_READINESS_OUTPUT" './scripts/release-operator-qa-smoke.sh'

stop_server
start_server "restart"

RESTART_MEMORY_OUTPUT="$("$JARVIS" memory get "$MEMORY_ID" --endpoint "$ENDPOINT")"
require_output_contains "operator QA restart memory" "$RESTART_MEMORY_OUTPUT" '"value":"updated operator QA memory"'

RESTART_TASKS_OUTPUT="$("$JARVIS" tasks list --json --endpoint "$ENDPOINT")"
require_output_contains "operator QA restart tasks" "$RESTART_TASKS_OUTPUT" '"status":"completed"'

RESTART_SCHEDULER_OUTPUT="$("$JARVIS" scheduler list --endpoint "$ENDPOINT")"
require_output_contains "operator QA restart scheduler" "$RESTART_SCHEDULER_OUTPUT" '"status":"failed"'

RESTART_DIAGNOSTICS_OUTPUT="$("$JARVIS" diagnostics export --endpoint "$ENDPOINT")"
require_output_contains "operator QA restart diagnostics" "$RESTART_DIAGNOSTICS_OUTPUT" '"repository_backed":true'
require_output_contains "operator QA restart diagnostics" "$RESTART_DIAGNOSTICS_OUTPUT" '"active_memory_item_count":1'

printf '\nJarvis release operator QA smoke: ok\n'
printf 'Endpoint: %s\n' "$ENDPOINT"
printf 'Database: %s\n' "$DB_PATH"
printf 'Proof boundary: repository-backed CLI operator smoke for command, audit, model routes, memory create/update/review/delete/restore, scheduler attention/run-due, activity, permissions review, diagnostics, emergency pause, release readiness, and restart recovery; no Developer ID signing, notarization, installer/Finder validation, live microphone/Speech/audio-output validation, live OS notification delivery, App Store review, marketplace trust, malware analysis, or OS sandbox enforcement.\n'
