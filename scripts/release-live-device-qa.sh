#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_PATH="${JARVIS_QA_INSTALLED_APP_PATH:-/Applications/Jarvis.app}"
CHECK_ONLY=false
ASSERT_COMPLETE=false

usage() {
  cat <<'USAGE'
Usage: scripts/release-live-device-qa.sh [--check|--assert-complete]

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

Optional:
  JARVIS_QA_INSTALLED_APP_PATH     Defaults to /Applications/Jarvis.app

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
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

if [[ "$CHECK_ONLY" == true && "$ASSERT_COMPLETE" == true ]]; then
  fail "--check and --assert-complete are mutually exclusive"
fi

if [[ "$CHECK_ONLY" != true && "$ASSERT_COMPLETE" != true ]]; then
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

cat <<EOF
Jarvis live-device QA assertion: complete
Installed app: $APP_PATH
Proof boundary: owner-recorded clean-profile install, Finder launch,
microphone/Speech, live audio output, notification, restart, and manual release
QA flags only; this still does not prove App Store review, marketplace trust,
malware analysis, or OS-level sandbox/egress enforcement.
EOF
