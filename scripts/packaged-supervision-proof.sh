#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-packaged-supervision.XXXXXX")"
SERVER_PID=""
cleanup() {
  if [[ -n "$SERVER_PID" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

APP_RESOURCES_BIN="$TMP_DIR/Jarvis.app/Contents/Resources/bin"
BUNDLED_CORE="$APP_RESOURCES_BIN/jarvis-cli"
DB_PATH="$TMP_DIR/packaged-smoke.sqlite"
SERVER_LOG="$TMP_DIR/packaged-core.log"
ENDPOINT=""

run cargo build -p jarvis-cli

mkdir -p "$APP_RESOURCES_BIN"
cp "$ROOT_DIR/target/debug/jarvis" "$BUNDLED_CORE"
chmod 755 "$BUNDLED_CORE"

if [[ ! -x "$BUNDLED_CORE" ]]; then
  printf 'error: packaged supervision proof did not create an executable core at %s\n' "$BUNDLED_CORE" >&2
  exit 1
fi

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

start_packaged_core() {
  local endpoint
  local health_output
  local ports=()

  if [[ -n "${JARVIS_PACKAGED_SMOKE_PORT:-}" ]]; then
    ports+=("$JARVIS_PACKAGED_SMOKE_PORT")
  fi
  ports+=(17787 17788 17789 17790 17791)

  for port in "${ports[@]}"; do
    endpoint="http://127.0.0.1:$port"
    printf '\n==> %s serve --bind 127.0.0.1:%s --db-path %s\n' "$BUNDLED_CORE" "$port" "$DB_PATH"
    "$BUNDLED_CORE" serve --bind "127.0.0.1:$port" --db-path "$DB_PATH" >"$SERVER_LOG" 2>&1 &
    SERVER_PID="$!"

    for _ in {1..40}; do
      if ! kill -0 "$SERVER_PID" 2>/dev/null; then
        break
      fi
      if health_output="$("$BUNDLED_CORE" health --endpoint "$endpoint" 2>/dev/null)"; then
        require_output_contains "packaged health" "$health_output" "jarvis-core: ok"
        require_output_contains "packaged health" "$health_output" "runtime: routed-fake-local-model+first-party-plugins"
        ENDPOINT="$endpoint"
        return 0
      fi
      sleep 0.2
    done

    if kill -0 "$SERVER_PID" 2>/dev/null; then
      kill "$SERVER_PID" 2>/dev/null || true
      wait "$SERVER_PID" 2>/dev/null || true
    fi
    SERVER_PID=""
  done

  printf 'error: packaged core did not become healthy; last server log follows\n' >&2
  cat "$SERVER_LOG" >&2 || true
  exit 1
}

run env JARVIS_MAC_CORE_EXECUTABLE="$BUNDLED_CORE" swift test --package-path apps/mac

start_packaged_core

COMMAND_OUTPUT="$("$BUNDLED_CORE" command --json "status" --endpoint "$ENDPOINT")"
require_output_contains "packaged command" "$COMMAND_OUTPUT" '"accepted":true'
require_output_contains "packaged command" "$COMMAND_OUTPUT" '"status":"completed"'
require_output_contains "packaged command" "$COMMAND_OUTPUT" '"event_type":"plugin_completed"'

AUDIT_OUTPUT="$("$BUNDLED_CORE" tasks audit --json --endpoint "$ENDPOINT")"
require_output_contains "packaged audit" "$AUDIT_OUTPUT" '"event_type":"plugin_completed"'
require_output_contains "packaged audit" "$AUDIT_OUTPUT" '"event_type":"task_completed"'

DIAGNOSTICS_OUTPUT="$("$BUNDLED_CORE" diagnostics export --endpoint "$ENDPOINT")"
require_output_contains "packaged diagnostics" "$DIAGNOSTICS_OUTPUT" '"repository_backed":true'
require_output_contains "packaged diagnostics" "$DIAGNOSTICS_OUTPUT" '"redaction":"diagnostics export omits command bodies'
require_output_contains "packaged diagnostics" "$DIAGNOSTICS_OUTPUT" '"task_count":1'

PAUSE_OUTPUT="$("$BUNDLED_CORE" pause --endpoint "$ENDPOINT" --reason "packaged-layout smoke")"
require_output_contains "packaged pause" "$PAUSE_OUTPUT" '"paused":true'

BLOCKED_OUTPUT="$("$BUNDLED_CORE" command --json "status" --endpoint "$ENDPOINT" --dry-run)"
require_output_contains "packaged blocked command" "$BLOCKED_OUTPUT" '"accepted":false'
require_output_contains "packaged blocked command" "$BLOCKED_OUTPUT" '"status":"blocked"'

PAUSE_STATUS_OUTPUT="$("$BUNDLED_CORE" pause-status --endpoint "$ENDPOINT")"
require_output_contains "packaged pause status" "$PAUSE_STATUS_OUTPUT" '"paused":true'

RESUME_OUTPUT="$("$BUNDLED_CORE" resume --endpoint "$ENDPOINT")"
require_output_contains "packaged resume" "$RESUME_OUTPUT" '"paused":false'

run cargo run -p jarvis-cli -- smoke

printf '\nJarvis packaged supervision proof: ok (%s)\n' "$BUNDLED_CORE"
printf 'Proof boundary: temporary packaged layout only; not signed, notarized, or a clean-profile app release.\n'
