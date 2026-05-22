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

select_port() {
  if [[ -n "${JARVIS_PACKAGED_APP_SMOKE_PORT:-}" ]]; then
    printf '%s\n' "$JARVIS_PACKAGED_APP_SMOKE_PORT"
    return
  fi

  for port in 18787 18788 18789 18790 18791; do
    if ! nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
      printf '%s\n' "$port"
      return
    fi
  done

  printf 'error: no packaged app smoke port is available\n' >&2
  exit 1
}

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-packaged-app-smoke.XXXXXX")"
APP_PID=""
PORT="$(select_port)"
ENDPOINT="http://127.0.0.1:$PORT"
APP_PATH="$TMP_DIR/Jarvis.app"
CONTENTS_DIR="$APP_PATH/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
RESOURCES_BIN_DIR="$RESOURCES_DIR/bin"
APP_EXECUTABLE="$MACOS_DIR/JarvisMacApp"
BUNDLED_CORE="$RESOURCES_BIN_DIR/jarvis-cli"
CLEAN_HOME="$TMP_DIR/home"
APP_DB="$CLEAN_HOME/Library/Application Support/Jarvis/jarvis.sqlite"
APP_LOG="$TMP_DIR/JarvisMacApp.log"
SIGNING_STATUS="not attempted"
ENTITLEMENTS="$ROOT_DIR/packaging/Jarvis.entitlements"

cleanup() {
  if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
    kill "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
  fi

  if command -v lsof >/dev/null 2>&1; then
    while IFS= read -r pid; do
      if [[ -n "$pid" ]]; then
        kill "$pid" 2>/dev/null || true
      fi
    done < <(lsof -ti "tcp:$PORT" 2>/dev/null || true)
  fi

  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

run cargo build -p jarvis-cli
run swift build --package-path apps/mac

SWIFT_BIN_DIR="$(swift build --package-path apps/mac --show-bin-path)"
SWIFT_EXECUTABLE="$SWIFT_BIN_DIR/JarvisMacApp"
if [[ ! -x "$SWIFT_EXECUTABLE" ]]; then
  printf 'error: SwiftPM did not build executable at %s\n' "$SWIFT_EXECUTABLE" >&2
  exit 1
fi

if [[ ! -x "$ROOT_DIR/target/debug/jarvis" ]]; then
  printf 'error: cargo did not build jarvis CLI at %s\n' "$ROOT_DIR/target/debug/jarvis" >&2
  exit 1
fi

mkdir -p "$MACOS_DIR" "$RESOURCES_BIN_DIR" "$CLEAN_HOME"
cp "$SWIFT_EXECUTABLE" "$APP_EXECUTABLE"
cp "$ROOT_DIR/target/debug/jarvis" "$BUNDLED_CORE"
chmod 755 "$APP_EXECUTABLE" "$BUNDLED_CORE"

cat >"$CONTENTS_DIR/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>JarvisMacApp</string>
  <key>CFBundleIdentifier</key>
  <string>com.nobiletechnology.jarvis.local-release-smoke</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Jarvis</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.4-local</string>
  <key>CFBundleVersion</key>
  <string>0.1.4</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSMicrophoneUsageDescription</key>
  <string>Jarvis uses microphone input only when you explicitly start local voice capture.</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>Jarvis uses speech recognition only to turn your spoken command into a local assistant request.</string>
</dict>
</plist>
PLIST

if command -v plutil >/dev/null 2>&1; then
  run plutil -lint "$CONTENTS_DIR/Info.plist"
fi
INFO_PLIST_CONTENTS="$(cat "$CONTENTS_DIR/Info.plist")"
require_output_contains "packaged app Info.plist" "$INFO_PLIST_CONTENTS" "NSMicrophoneUsageDescription"
require_output_contains "packaged app Info.plist" "$INFO_PLIST_CONTENTS" "NSSpeechRecognitionUsageDescription"

if command -v codesign >/dev/null 2>&1; then
  [[ -f "$ENTITLEMENTS" ]] || {
    printf 'error: missing entitlements file: %s\n' "$ENTITLEMENTS" >&2
    exit 1
  }
  run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$BUNDLED_CORE"
  run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_EXECUTABLE"
  run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_PATH"
  run codesign --verify --deep --strict "$APP_PATH"
  APP_ENTITLEMENTS_OUTPUT="$(codesign -d --entitlements :- "$APP_PATH" 2>/dev/null)"
  require_output_contains \
    "packaged app entitlements" \
    "$APP_ENTITLEMENTS_OUTPUT" \
    "com.apple.security.device.audio-input"
  SIGNING_STATUS="ad-hoc signed with codesign - and packaging/Jarvis.entitlements"
else
  SIGNING_STATUS="codesign unavailable; unsigned local bundle"
fi

printf '\n==> Launching %s with HOME=%s and endpoint %s\n' "$APP_PATH" "$CLEAN_HOME" "$ENDPOINT"
env \
  HOME="$CLEAN_HOME" \
  JARVIS_MAC_CORE_BIND_ADDRESS="127.0.0.1:$PORT" \
  JARVIS_MAC_CORE_ENDPOINT="$ENDPOINT" \
  JARVIS_MAC_CORE_DATABASE="$APP_DB" \
  "$APP_EXECUTABLE" >"$APP_LOG" 2>&1 &
APP_PID="$!"

HEALTH_OUTPUT=""
for _ in {1..60}; do
  if ! kill -0 "$APP_PID" 2>/dev/null; then
    printf 'error: packaged app exited before core became healthy; app log follows\n' >&2
    cat "$APP_LOG" >&2 || true
    exit 1
  fi

  if HEALTH_OUTPUT="$("$BUNDLED_CORE" health --endpoint "$ENDPOINT" 2>/dev/null)"; then
    require_output_contains "packaged app health" "$HEALTH_OUTPUT" "jarvis-core: ok"
    require_output_contains "packaged app health" "$HEALTH_OUTPUT" "runtime: routed-fake-local-model+first-party-plugins"
    break
  fi
  sleep 0.25
done

if [[ -z "$HEALTH_OUTPUT" ]]; then
  printf 'error: packaged app did not supervise a healthy core; app log follows\n' >&2
  cat "$APP_LOG" >&2 || true
  exit 1
fi

COMMAND_OUTPUT="$("$BUNDLED_CORE" command "plugin echo packaged app release smoke" --endpoint "$ENDPOINT")"
require_output_contains "packaged app command" "$COMMAND_OUTPUT" '"accepted":true'
require_output_contains "packaged app command" "$COMMAND_OUTPUT" '"status":"completed"'
require_output_contains "packaged app command" "$COMMAND_OUTPUT" '"event_type":"plugin_completed"'

AUDIT_OUTPUT="$("$BUNDLED_CORE" tasks audit --endpoint "$ENDPOINT")"
require_output_contains "packaged app audit" "$AUDIT_OUTPUT" '"event_type":"plugin_completed"'
require_output_contains "packaged app audit" "$AUDIT_OUTPUT" '"event_type":"task_completed"'

DIAGNOSTICS_OUTPUT="$("$BUNDLED_CORE" diagnostics export --endpoint "$ENDPOINT")"
require_output_contains "packaged app diagnostics" "$DIAGNOSTICS_OUTPUT" '"repository_backed":true'
require_output_contains "packaged app diagnostics" "$DIAGNOSTICS_OUTPUT" '"task_count":1'
require_output_contains "packaged app diagnostics" "$DIAGNOSTICS_OUTPUT" '"redaction":"diagnostics export omits command bodies'

PAUSE_OUTPUT="$("$BUNDLED_CORE" pause --endpoint "$ENDPOINT" --reason "packaged app release smoke")"
require_output_contains "packaged app pause" "$PAUSE_OUTPUT" '"paused":true'

BLOCKED_OUTPUT="$("$BUNDLED_CORE" command "plugin echo blocked by packaged app release smoke" --endpoint "$ENDPOINT" --dry-run)"
require_output_contains "packaged app blocked command" "$BLOCKED_OUTPUT" '"accepted":false'
require_output_contains "packaged app blocked command" "$BLOCKED_OUTPUT" '"status":"blocked"'

PAUSE_STATUS_OUTPUT="$("$BUNDLED_CORE" pause-status --endpoint "$ENDPOINT")"
require_output_contains "packaged app pause status" "$PAUSE_STATUS_OUTPUT" '"paused":true'

RESUME_OUTPUT="$("$BUNDLED_CORE" resume --endpoint "$ENDPOINT")"
require_output_contains "packaged app resume" "$RESUME_OUTPUT" '"paused":false'

if [[ ! -s "$APP_DB" ]]; then
  printf 'error: clean HOME database was not created at %s\n' "$APP_DB" >&2
  exit 1
fi

printf '\nJarvis packaged app release smoke: ok\n'
printf 'Bundle: %s\n' "$APP_PATH"
printf 'Signing: %s\n' "$SIGNING_STATUS"
printf 'Clean HOME database: %s\n' "$APP_DB"
printf 'Proof boundary: locally assembled SwiftPM Jarvis.app with bundled jarvis-cli, usage strings, and ad-hoc entitlement evidence; no Developer ID signing, notarization, installer, live microphone/Speech/audio-output, or Finder/LaunchServices validation.\n'
