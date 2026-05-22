#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_PATH="${JARVIS_QA_INSTALLED_APP_PATH:-/Applications/Jarvis.app}"
REPORT_PATH="${JARVIS_QA_REPORT_PATH:-$ROOT_DIR/target/release-live-device-qa-report.json}"
CHECK_ONLY=false
ASSERT_COMPLETE=false
SELF_TEST=false

usage() {
  cat <<'USAGE'
Usage: scripts/release-live-device-qa.sh [--check|--assert-complete|--self-test]

Prepare or assert the live-device release QA gate for Jarvis.

--check validates repo-owned live QA prerequisites and prints the manual checks
that must be performed on a clean Mac profile before any production-ready claim.

--assert-complete verifies that the installed app exists and that the owner has
explicitly recorded each live validation flag below as true:
  JARVIS_QA_CLEAN_PROFILE_VALIDATED=true
  JARVIS_QA_FINDER_LAUNCH_VALIDATED=true
  JARVIS_QA_MICROPHONE_VALIDATED=true
  JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true
  JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true
  JARVIS_QA_NOTIFICATION_VALIDATED=true
  JARVIS_QA_RESTART_VALIDATED=true
  JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true

--self-test builds a fake app fixture in a temporary directory and exercises
the assertion/report mechanics without claiming live-device validation.

Optional:
  JARVIS_QA_INSTALLED_APP_PATH     Defaults to /Applications/Jarvis.app
  JARVIS_QA_REPORT_PATH            Defaults to target/release-live-device-qa-report.json

This script records manual proof boundaries only. It does not perform Developer
ID signing, notarization, App Store review, malware analysis, marketplace
review, or OS-level sandbox validation.
USAGE
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
}

require_file_contains() {
  local label="$1"
  local path="$2"
  local expected="$3"
  [[ -f "$path" ]] || fail "missing $label: $path"
  if ! grep -F "$expected" "$path" >/dev/null 2>&1; then
    fail "$label does not mention required text: $expected"
  fi
}

require_true() {
  local name="$1"
  local value="${!name:-}"
  [[ "$value" == "true" ]] || fail "$name must be set to true after manual validation"
}

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

write_report() {
  local generated_at
  local escaped_app_path
  local escaped_boundary
  require_command python3
  generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  escaped_app_path="$(json_escape "$APP_PATH")"
  escaped_boundary="$(json_escape "Owner-recorded clean-profile install, Finder launch, microphone/Speech, live audio output, notification, restart, and manual release QA flags only; no App Store review, marketplace trust, malware analysis, or OS-level sandbox/egress enforcement.")"

  mkdir -p "$(dirname "$REPORT_PATH")"
  cat >"$REPORT_PATH" <<EOF
{
  "generated_at": "$generated_at",
  "installed_app_path": "$escaped_app_path",
  "validation_flags": {
    "clean_profile": true,
    "finder_launch": true,
    "microphone": true,
    "speech_permission": true,
    "audio_output": true,
    "notification": true,
    "restart": true,
    "manual_release_qa": true
  },
  "proof_boundary": "$escaped_boundary"
}
EOF
  python3 -m json.tool "$REPORT_PATH" >/dev/null
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      CHECK_ONLY=true
      shift
      ;;
    --assert-complete)
      ASSERT_COMPLETE=true
      shift
      ;;
    --self-test)
      SELF_TEST=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

if { [[ "$CHECK_ONLY" == true ]] && { [[ "$ASSERT_COMPLETE" == true ]] || [[ "$SELF_TEST" == true ]]; }; } ||
  { [[ "$ASSERT_COMPLETE" == true ]] && [[ "$SELF_TEST" == true ]]; }; then
  fail "--check, --assert-complete, and --self-test are mutually exclusive"
fi

if [[ "$CHECK_ONLY" != true && "$ASSERT_COMPLETE" != true && "$SELF_TEST" != true ]]; then
  usage
  exit 0
fi

require_command plutil
require_command grep

ENTITLEMENTS="$ROOT_DIR/packaging/Jarvis.entitlements"
INFO_TEMPLATE_HINT="$ROOT_DIR/scripts/package-distribution.sh"

plutil -lint "$ENTITLEMENTS" >/dev/null
require_file_contains "distribution packaging script" "$INFO_TEMPLATE_HINT" "NSMicrophoneUsageDescription"
require_file_contains "distribution packaging script" "$INFO_TEMPLATE_HINT" "NSSpeechRecognitionUsageDescription"
require_file_contains "release checklist" "$ROOT_DIR/docs/release-checklist.md" "live microphone/Speech"
require_file_contains "release checklist" "$ROOT_DIR/docs/release-checklist.md" "live audio-output"

if [[ "$SELF_TEST" == true ]]; then
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-live-qa-self-test.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT
  fixture_app="$tmp_dir/Jarvis.app"
  fixture_report="$tmp_dir/release-live-device-qa-report.json"
  mkdir -p "$fixture_app/Contents/MacOS" "$fixture_app/Contents/Resources/bin"
  cat >"$fixture_app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>JarvisMacApp</string>
  <key>CFBundleIdentifier</key>
  <string>com.nobiletechnology.jarvis.selftest</string>
</dict>
</plist>
PLIST
  touch "$fixture_app/Contents/MacOS/JarvisMacApp" "$fixture_app/Contents/Resources/bin/jarvis-cli"
  chmod 755 "$fixture_app/Contents/MacOS/JarvisMacApp" "$fixture_app/Contents/Resources/bin/jarvis-cli"

  JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$fixture_report" \
    JARVIS_QA_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_QA_FINDER_LAUNCH_VALIDATED=true \
    JARVIS_QA_MICROPHONE_VALIDATED=true \
    JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true \
    JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true \
    JARVIS_QA_NOTIFICATION_VALIDATED=true \
    JARVIS_QA_RESTART_VALIDATED=true \
    JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true \
    "$0" --assert-complete >/dev/null
  require_file_contains "live QA self-test report" "$fixture_report" '"manual_release_qa": true'
  require_file_contains "live QA self-test report" "$fixture_report" '"proof_boundary"'
  printf 'Jarvis live-device QA self-test: ok\n'
  printf 'Proof boundary: fake app fixture validates assertion/report mechanics only; no live device validation was performed.\n'
  exit 0
fi

if [[ "$CHECK_ONLY" == true ]]; then
  cat <<'CHECKLIST'
Jarvis live-device QA preflight: ok

Manual release checks still required before production-ready language:
- Install the signed, notarized package into /Applications on a clean Mac profile.
- Launch Jarvis through Finder or LaunchServices, not only from Terminal.
- Confirm the app supervises the bundled core and command, audit, memory, scheduler,
  plugin, pause/resume, diagnostics, restart, and release-readiness surfaces work.
- Start voice capture and verify microphone and Speech permission prompts.
- Speak a command and verify transcript handoff reaches the same command path.
- Play speech output and verify live audio output on the device.
- Verify scheduler notification permission and at least one visible notification.
- Record all JARVIS_QA_* flags as true, then rerun this script with
  --assert-complete on the validated release machine.
- Preserve the generated JSON report from --assert-complete as release evidence.

Proof boundary: preflight and runbook only; no live device validation was
performed by --check.
CHECKLIST
  exit 0
fi

[[ -d "$APP_PATH" ]] || fail "installed app is missing: $APP_PATH"
[[ -x "$APP_PATH/Contents/MacOS/JarvisMacApp" ]] || fail "installed app executable is missing or not executable"
[[ -x "$APP_PATH/Contents/Resources/bin/jarvis-cli" ]] || fail "bundled core executable is missing or not executable"
plutil -lint "$APP_PATH/Contents/Info.plist" >/dev/null

require_true JARVIS_QA_CLEAN_PROFILE_VALIDATED
require_true JARVIS_QA_FINDER_LAUNCH_VALIDATED
require_true JARVIS_QA_MICROPHONE_VALIDATED
require_true JARVIS_QA_SPEECH_PERMISSION_VALIDATED
require_true JARVIS_QA_AUDIO_OUTPUT_VALIDATED
require_true JARVIS_QA_NOTIFICATION_VALIDATED
require_true JARVIS_QA_RESTART_VALIDATED
require_true JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED
write_report

cat <<EOF
Jarvis live-device QA assertion: complete
Installed app: $APP_PATH
Report: $REPORT_PATH
Proof boundary: owner-recorded clean-profile install, Finder launch,
microphone/Speech, live audio output, notification, restart, and manual release
QA flags only; this still does not prove App Store review, marketplace trust,
malware analysis, or OS-level sandbox/egress enforcement.
EOF
