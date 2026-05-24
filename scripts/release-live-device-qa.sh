#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_PATH="${JARVIS_QA_INSTALLED_APP_PATH:-/Applications/Jarvis.app}"
REPORT_PATH="${JARVIS_QA_REPORT_PATH:-$ROOT_DIR/target/release-live-device-qa-report.json}"
EXPECTED_BUNDLE_ID="${JARVIS_QA_EXPECTED_BUNDLE_ID:-com.nobiletechnology.jarvis}"
EXPECTED_VERSION="${JARVIS_QA_EXPECTED_VERSION:-$("$ROOT_DIR/scripts/release-version.sh")}"
CHECK_ONLY=false
ASSERT_COMPLETE=false
SELF_TEST=false
WRITE_TEMPLATE=false
WRITE_TEMPLATE_PATH=""
APP_BUNDLE_ID=""
APP_SHORT_VERSION=""
APP_BUILD_VERSION=""
APP_MICROPHONE_USAGE=""
APP_SPEECH_USAGE=""

usage() {
  cat <<'USAGE'
Usage: scripts/release-live-device-qa.sh [--check|--assert-complete|--self-test|--write-template PATH]

Prepare or assert the live-device release QA gate for Jarvis.

--check validates repo-owned live QA prerequisites and prints the manual checks
that must be performed on a clean Mac profile before any production-ready claim.

--assert-complete verifies that the installed app exists and that the owner has
explicitly recorded each live validation flag below as true:
  JARVIS_QA_CLEAN_PROFILE_VALIDATED=true
  JARVIS_QA_FINDER_LAUNCH_VALIDATED=true
  JARVIS_QA_MICROPHONE_VALIDATED=true
  JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true
  JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true
  JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true
  JARVIS_QA_NOTIFICATION_VALIDATED=true
  JARVIS_QA_RESTART_VALIDATED=true
  JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true

The owner must also record non-empty live voice evidence notes:
  JARVIS_QA_OWNER_NAME
  JARVIS_QA_DEVICE_LABEL
  JARVIS_QA_PROFILE_LABEL
  JARVIS_QA_VOICE_CHECK_STARTED_AT
  JARVIS_QA_VOICE_CHECK_COMPLETED_AT
  JARVIS_QA_MICROPHONE_EVIDENCE_NOTE
  JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE
  JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE
  JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE
  JARVIS_QA_VOICE_TEST_PHRASE
  JARVIS_QA_OBSERVED_TRANSCRIPT
  JARVIS_QA_EXPECTED_COMMAND_TEXT
  JARVIS_QA_OBSERVED_COMMAND_TEXT
  JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID
  JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL

Owner-recorded evidence values must contain non-whitespace text.
The observed transcript must match JARVIS_QA_VOICE_TEST_PHRASE after trimming,
and the observed command text must match JARVIS_QA_EXPECTED_COMMAND_TEXT after
trimming. JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID must be a `task:<uuid>` or
`audit:<uuid>` reference from the live command. This keeps the spoken phrase,
transcript, and command-path evidence bound together.

--self-test builds a fake app fixture in a temporary directory and exercises
the assertion/report mechanics without claiming live-device validation.

--write-template PATH writes a sourceable shell env template containing every
JARVIS_QA_* field required by --assert-complete. Edit the template on the
validated release machine, source it, and then run --assert-complete.

Optional:
  JARVIS_QA_INSTALLED_APP_PATH     Defaults to /Applications/Jarvis.app
  JARVIS_QA_REPORT_PATH            Defaults to target/release-live-device-qa-report.json
  JARVIS_QA_EXPECTED_BUNDLE_ID     Defaults to com.nobiletechnology.jarvis
  JARVIS_QA_EXPECTED_VERSION       Defaults to the Rust package release version

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

require_non_empty_env() {
  local name="$1"
  local value="${!name:-}"
  [[ -n "${value//[[:space:]]/}" ]] || fail "$name must be set to a non-empty owner-recorded evidence value"
}

require_utc_timestamp_env() {
  local name="$1"
  local value="${!name:-}"
  require_non_empty_env "$name"
  require_command python3
  python3 - "$name" "$value" <<'PY'
import datetime
import sys

name, value = sys.argv[1:3]
try:
    datetime.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ")
except ValueError as exc:
    raise SystemExit(f"{name} must be a UTC RFC3339 timestamp like 2026-05-22T16:00:00Z") from exc
PY
}

require_timestamp_order() {
  local started_name="$1"
  local completed_name="$2"
  local started="${!started_name:-}"
  local completed="${!completed_name:-}"
  require_command python3
  python3 - "$started_name" "$started" "$completed_name" "$completed" <<'PY'
import datetime
import sys

started_name, started, completed_name, completed = sys.argv[1:5]
started_at = datetime.datetime.strptime(started, "%Y-%m-%dT%H:%M:%SZ")
completed_at = datetime.datetime.strptime(completed, "%Y-%m-%dT%H:%M:%SZ")
if completed_at < started_at:
    raise SystemExit(f"{completed_name} must be greater than or equal to {started_name}")
PY
}

require_not_future_timestamp_env() {
  local name="$1"
  local value="${!name:-}"
  require_utc_timestamp_env "$name"
  require_command python3
  python3 - "$name" "$value" <<'PY'
import datetime
import sys

name, value = sys.argv[1:3]
timestamp = datetime.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=datetime.timezone.utc)
now = datetime.datetime.now(datetime.timezone.utc)
if timestamp > now:
    raise SystemExit(f"{name} must not be later than the generated report timestamp")
PY
}

require_trimmed_env_match() {
  local expected_name="$1"
  local actual_name="$2"
  local expected="${!expected_name:-}"
  local actual="${!actual_name:-}"
  require_non_empty_env "$expected_name"
  require_non_empty_env "$actual_name"
  require_command python3
  python3 - "$expected_name" "$expected" "$actual_name" "$actual" <<'PY'
import sys

expected_name, expected, actual_name, actual = sys.argv[1:5]
if expected.strip() != actual.strip():
    raise SystemExit(f"{actual_name} must match {expected_name} after trimming whitespace")
PY
}

require_observed_transcript_matches_phrase() {
  require_trimmed_env_match JARVIS_QA_VOICE_TEST_PHRASE JARVIS_QA_OBSERVED_TRANSCRIPT
}

require_command_result_evidence_id_env() {
  local name="$1"
  local value="${!name:-}"
  require_non_empty_env "$name"
  require_command python3
  python3 - "$name" "$value" <<'PY'
import re
import sys

name, value = sys.argv[1:3]
pattern = re.compile(
    r"^(task|audit):[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
)
if not pattern.fullmatch(value.strip()):
    raise SystemExit(f"{name} must be task:<uuid> or audit:<uuid> from the live command")
PY
}

plist_value() {
  local plist_path="$1"
  local key="$2"
  plutil -extract "$key" raw -o - "$plist_path" 2>/dev/null || true
}

require_plist_value() {
  local label="$1"
  local value="$2"
  [[ -n "$value" ]] || fail "installed app Info.plist is missing $label"
}

validate_installed_app_bundle_metadata() {
  local info_plist="$APP_PATH/Contents/Info.plist"
  plutil -lint "$info_plist" >/dev/null
  APP_BUNDLE_ID="$(plist_value "$info_plist" CFBundleIdentifier)"
  APP_SHORT_VERSION="$(plist_value "$info_plist" CFBundleShortVersionString)"
  APP_BUILD_VERSION="$(plist_value "$info_plist" CFBundleVersion)"
  APP_MICROPHONE_USAGE="$(plist_value "$info_plist" NSMicrophoneUsageDescription)"
  APP_SPEECH_USAGE="$(plist_value "$info_plist" NSSpeechRecognitionUsageDescription)"

  require_plist_value "CFBundleIdentifier" "$APP_BUNDLE_ID"
  require_plist_value "CFBundleShortVersionString" "$APP_SHORT_VERSION"
  require_plist_value "CFBundleVersion" "$APP_BUILD_VERSION"
  require_plist_value "NSMicrophoneUsageDescription" "$APP_MICROPHONE_USAGE"
  require_plist_value "NSSpeechRecognitionUsageDescription" "$APP_SPEECH_USAGE"

  [[ "$APP_BUNDLE_ID" == "$EXPECTED_BUNDLE_ID" ]] ||
    fail "installed app bundle id mismatch: expected $EXPECTED_BUNDLE_ID, got $APP_BUNDLE_ID"
  [[ "$APP_SHORT_VERSION" == "$EXPECTED_VERSION" ]] ||
    fail "installed app short version mismatch: expected $EXPECTED_VERSION, got $APP_SHORT_VERSION"
  [[ "$APP_BUILD_VERSION" == "$EXPECTED_VERSION" ]] ||
    fail "installed app build version mismatch: expected $EXPECTED_VERSION, got $APP_BUILD_VERSION"
}

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

write_report() {
  local generated_at
  local escaped_app_path
  local escaped_bundle_id
  local escaped_short_version
  local escaped_build_version
  local escaped_microphone_usage
  local escaped_speech_usage
  local escaped_owner_name
  local escaped_device_label
  local escaped_profile_label
  local escaped_started_at
  local escaped_completed_at
  local escaped_microphone_note
  local escaped_speech_note
  local escaped_transcript_note
  local escaped_audio_note
  local escaped_voice_test_phrase
  local escaped_observed_transcript
  local escaped_expected_command_text
  local escaped_observed_command_text
  local escaped_command_result_evidence_id
  local escaped_audio_output_device_label
  local escaped_boundary
  local self_test_fixture
  require_command python3
  generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  self_test_fixture="${JARVIS_QA_SELF_TEST_FIXTURE:-false}"
  case "$self_test_fixture" in
    true|false) ;;
    *) fail "JARVIS_QA_SELF_TEST_FIXTURE must be true or false" ;;
  esac
  if [[ "$self_test_fixture" == true && "${JARVIS_QA_INTERNAL_SELF_TEST:-false}" != true ]]; then
    fail "JARVIS_QA_SELF_TEST_FIXTURE is reserved for --self-test and cannot be used for release evidence"
  fi
  escaped_app_path="$(json_escape "$APP_PATH")"
  escaped_bundle_id="$(json_escape "$APP_BUNDLE_ID")"
  escaped_short_version="$(json_escape "$APP_SHORT_VERSION")"
  escaped_build_version="$(json_escape "$APP_BUILD_VERSION")"
  escaped_microphone_usage="$(json_escape "$APP_MICROPHONE_USAGE")"
  escaped_speech_usage="$(json_escape "$APP_SPEECH_USAGE")"
  escaped_owner_name="$(json_escape "$JARVIS_QA_OWNER_NAME")"
  escaped_device_label="$(json_escape "$JARVIS_QA_DEVICE_LABEL")"
  escaped_profile_label="$(json_escape "$JARVIS_QA_PROFILE_LABEL")"
  escaped_started_at="$(json_escape "$JARVIS_QA_VOICE_CHECK_STARTED_AT")"
  escaped_completed_at="$(json_escape "$JARVIS_QA_VOICE_CHECK_COMPLETED_AT")"
  escaped_microphone_note="$(json_escape "$JARVIS_QA_MICROPHONE_EVIDENCE_NOTE")"
  escaped_speech_note="$(json_escape "$JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE")"
  escaped_transcript_note="$(json_escape "$JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE")"
  escaped_audio_note="$(json_escape "$JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE")"
  escaped_voice_test_phrase="$(json_escape "$JARVIS_QA_VOICE_TEST_PHRASE")"
  escaped_observed_transcript="$(json_escape "$JARVIS_QA_OBSERVED_TRANSCRIPT")"
  escaped_expected_command_text="$(json_escape "$JARVIS_QA_EXPECTED_COMMAND_TEXT")"
  escaped_observed_command_text="$(json_escape "$JARVIS_QA_OBSERVED_COMMAND_TEXT")"
  escaped_command_result_evidence_id="$(json_escape "$JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID")"
  escaped_audio_output_device_label="$(json_escape "$JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL")"
  escaped_boundary="$(json_escape "Owner-recorded clean-profile install, Finder launch, microphone/Speech permission prompts, spoken transcript handoff into the command path, live audio output, notification, restart, and manual release QA flags only; no App Store review, marketplace trust, malware analysis, or OS-level sandbox/egress enforcement.")"

  mkdir -p "$(dirname "$REPORT_PATH")"
  cat >"$REPORT_PATH" <<EOF
{
  "schema_version": 1,
  "evidence_type": "owner_recorded_live_device_qa",
  "self_test_fixture": $self_test_fixture,
  "generated_at": "$generated_at",
  "installed_app_path": "$escaped_app_path",
  "app_bundle": {
    "bundle_identifier": "$escaped_bundle_id",
    "short_version": "$escaped_short_version",
    "build_version": "$escaped_build_version",
    "microphone_usage_description": "$escaped_microphone_usage",
    "speech_recognition_usage_description": "$escaped_speech_usage"
  },
  "validation_flags": {
    "clean_profile": true,
    "finder_launch": true,
    "microphone": true,
    "speech_permission": true,
    "transcript_handoff": true,
    "audio_output": true,
    "notification": true,
    "restart": true,
    "manual_release_qa": true
  },
  "voice_loop": {
    "microphone_permission_prompt": true,
    "speech_permission_prompt": true,
    "spoken_transcript_handoff": true,
    "same_command_path": true,
    "speech_output_playback": true
  },
  "owner_recorded_live_voice_evidence": {
    "owner_name": "$escaped_owner_name",
    "device_label": "$escaped_device_label",
    "profile_label": "$escaped_profile_label",
    "voice_check_started_at": "$escaped_started_at",
    "voice_check_completed_at": "$escaped_completed_at",
    "microphone_evidence_note": "$escaped_microphone_note",
    "speech_permission_evidence_note": "$escaped_speech_note",
    "transcript_handoff_evidence_note": "$escaped_transcript_note",
    "audio_output_evidence_note": "$escaped_audio_note"
  },
  "voice_command_observation": {
    "test_phrase": "$escaped_voice_test_phrase",
    "observed_transcript": "$escaped_observed_transcript",
    "expected_command_text": "$escaped_expected_command_text",
    "observed_command_text": "$escaped_observed_command_text",
    "command_result_evidence_id": "$escaped_command_result_evidence_id",
    "audio_output_device_label": "$escaped_audio_output_device_label"
  },
  "proof_boundary": "$escaped_boundary"
}
EOF
  python3 -m json.tool "$REPORT_PATH" >/dev/null
}

write_env_template() {
  local template_path="$1"
  mkdir -p "$(dirname "$template_path")"
  cat >"$template_path" <<'EOF'
# Jarvis live-device QA evidence template.
# Edit this file on the validated release machine, then run:
#   set -a
#   source ./target/release-live-device-qa.env
#   set +a
#   ./scripts/release-live-device-qa.sh --assert-complete
#
# Keep all validation flags false until that check has actually been observed
# on the signed, notarized app installed in a clean macOS profile.

JARVIS_QA_INSTALLED_APP_PATH="/Applications/Jarvis.app"
JARVIS_QA_REPORT_PATH="target/release-live-device-qa-report.json"
JARVIS_QA_EXPECTED_BUNDLE_ID="com.nobiletechnology.jarvis"
JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION"

JARVIS_QA_CLEAN_PROFILE_VALIDATED=false
JARVIS_QA_FINDER_LAUNCH_VALIDATED=false
JARVIS_QA_MICROPHONE_VALIDATED=false
JARVIS_QA_SPEECH_PERMISSION_VALIDATED=false
JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=false
JARVIS_QA_AUDIO_OUTPUT_VALIDATED=false
JARVIS_QA_NOTIFICATION_VALIDATED=false
JARVIS_QA_RESTART_VALIDATED=false
JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=false

JARVIS_QA_OWNER_NAME=""
JARVIS_QA_DEVICE_LABEL=""
JARVIS_QA_PROFILE_LABEL=""
JARVIS_QA_VOICE_CHECK_STARTED_AT=""
JARVIS_QA_VOICE_CHECK_COMPLETED_AT=""
JARVIS_QA_MICROPHONE_EVIDENCE_NOTE=""
JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE=""
JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE=""
JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE=""
JARVIS_QA_VOICE_TEST_PHRASE=""
JARVIS_QA_OBSERVED_TRANSCRIPT=""
JARVIS_QA_EXPECTED_COMMAND_TEXT=""
JARVIS_QA_OBSERVED_COMMAND_TEXT=""
JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="" # task:<uuid> or audit:<uuid> from the live command
JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL=""
EOF
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
    --write-template)
      WRITE_TEMPLATE=true
      [[ $# -ge 2 ]] || fail "--write-template requires a path"
      WRITE_TEMPLATE_PATH="$2"
      shift 2
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
  { [[ "$CHECK_ONLY" == true ]] && [[ "$WRITE_TEMPLATE" == true ]]; } ||
  { [[ "$ASSERT_COMPLETE" == true ]] && { [[ "$SELF_TEST" == true ]] || [[ "$WRITE_TEMPLATE" == true ]]; }; } ||
  { [[ "$SELF_TEST" == true ]] && [[ "$WRITE_TEMPLATE" == true ]]; }; then
  fail "--check, --assert-complete, --self-test, and --write-template are mutually exclusive"
fi

if [[ "$CHECK_ONLY" != true && "$ASSERT_COMPLETE" != true && "$SELF_TEST" != true && "$WRITE_TEMPLATE" != true ]]; then
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

if [[ "$WRITE_TEMPLATE" == true ]]; then
  write_env_template "$WRITE_TEMPLATE_PATH"
  printf 'Jarvis live-device QA env template written: %s\n' "$WRITE_TEMPLATE_PATH"
  printf 'Proof boundary: template generation only; no live device validation was performed.\n'
  exit 0
fi

if [[ "$SELF_TEST" == true ]]; then
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-live-qa-self-test.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT
  export JARVIS_QA_INTERNAL_SELF_TEST=true
  fixture_app="$tmp_dir/Jarvis.app"
  fixture_report="$tmp_dir/release-live-device-qa-report.json"
  fixture_template="$tmp_dir/release-live-device-qa.env"
  mkdir -p "$fixture_app/Contents/MacOS" "$fixture_app/Contents/Resources/bin"
  cat >"$fixture_app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>JarvisMacApp</string>
  <key>CFBundleIdentifier</key>
  <string>com.nobiletechnology.jarvis.selftest</string>
  <key>CFBundleShortVersionString</key>
  <string>$EXPECTED_VERSION</string>
  <key>CFBundleVersion</key>
  <string>$EXPECTED_VERSION</string>
  <key>NSMicrophoneUsageDescription</key>
  <string>Jarvis uses microphone input only when you explicitly start local voice capture.</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>Jarvis uses speech recognition only to turn your spoken command into a local assistant request.</string>
</dict>
</plist>
PLIST
  touch "$fixture_app/Contents/MacOS/JarvisMacApp" "$fixture_app/Contents/Resources/bin/jarvis-cli"
  chmod 755 "$fixture_app/Contents/MacOS/JarvisMacApp" "$fixture_app/Contents/Resources/bin/jarvis-cli"

  "$0" --write-template "$fixture_template" >/dev/null
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_CLEAN_PROFILE_VALIDATED=false'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=false'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE=""'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_EXPECTED_COMMAND_TEXT=""'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_OBSERVED_COMMAND_TEXT=""'
  require_file_contains "live QA env template" "$fixture_template" './scripts/release-live-device-qa.sh --assert-complete'
  if grep -F 'JARVIS_QA_CLEAN_PROFILE_VALIDATED=true' "$fixture_template" >/dev/null 2>&1; then
    fail "live QA self-test expected env template validation flags to default false"
  fi

  JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$fixture_report" \
    JARVIS_QA_EXPECTED_BUNDLE_ID="com.nobiletechnology.jarvis.selftest" \
    JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION" \
    JARVIS_QA_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_QA_FINDER_LAUNCH_VALIDATED=true \
    JARVIS_QA_MICROPHONE_VALIDATED=true \
    JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true \
    JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true \
    JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true \
    JARVIS_QA_NOTIFICATION_VALIDATED=true \
    JARVIS_QA_RESTART_VALIDATED=true \
    JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true \
    JARVIS_QA_OWNER_NAME="Jarvis QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the fake fixture." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the fake fixture." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the fake fixture." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the fake fixture." \
    JARVIS_QA_VOICE_TEST_PHRASE="Jarvis status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Jarvis status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null
  require_file_contains "live QA self-test report" "$fixture_report" '"manual_release_qa": true'
  require_file_contains "live QA self-test report" "$fixture_report" '"schema_version": 1'
  require_file_contains "live QA self-test report" "$fixture_report" '"evidence_type": "owner_recorded_live_device_qa"'
  require_file_contains "live QA self-test report" "$fixture_report" '"self_test_fixture": true'
  require_file_contains "live QA self-test report" "$fixture_report" '"bundle_identifier": "com.nobiletechnology.jarvis.selftest"'
  require_file_contains "live QA self-test report" "$fixture_report" '"microphone_usage_description"'
  require_file_contains "live QA self-test report" "$fixture_report" '"transcript_handoff": true'
  require_file_contains "live QA self-test report" "$fixture_report" '"same_command_path": true'
  require_file_contains "live QA self-test report" "$fixture_report" '"owner_recorded_live_voice_evidence"'
  require_file_contains "live QA self-test report" "$fixture_report" '"owner_name": "Jarvis QA Self-Test"'
  require_file_contains "live QA self-test report" "$fixture_report" '"voice_command_observation"'
  require_file_contains "live QA self-test report" "$fixture_report" '"expected_command_text": "status check"'
  require_file_contains "live QA self-test report" "$fixture_report" '"observed_command_text": "status check"'
  require_file_contains "live QA self-test report" "$fixture_report" '"audio_output_evidence_note": "Observed speech output playback in the fake fixture."'
  require_file_contains "live QA self-test report" "$fixture_report" '"proof_boundary"'

  if env -u JARVIS_QA_INTERNAL_SELF_TEST \
    JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$tmp_dir/operator-self-test-fixture.json" \
    JARVIS_QA_EXPECTED_BUNDLE_ID="com.nobiletechnology.jarvis.selftest" \
    JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION" \
    JARVIS_QA_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_QA_FINDER_LAUNCH_VALIDATED=true \
    JARVIS_QA_MICROPHONE_VALIDATED=true \
    JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true \
    JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true \
    JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true \
    JARVIS_QA_NOTIFICATION_VALIDATED=true \
    JARVIS_QA_RESTART_VALIDATED=true \
    JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true \
    JARVIS_QA_OWNER_NAME="Jarvis QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the fake fixture." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the fake fixture." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the fake fixture." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the fake fixture." \
    JARVIS_QA_VOICE_TEST_PHRASE="Jarvis status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Jarvis status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected operator-authored self-test fixture reports to fail"
  fi

  if JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$tmp_dir/blank-owner-evidence.json" \
    JARVIS_QA_EXPECTED_BUNDLE_ID="com.nobiletechnology.jarvis.selftest" \
    JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION" \
    JARVIS_QA_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_QA_FINDER_LAUNCH_VALIDATED=true \
    JARVIS_QA_MICROPHONE_VALIDATED=true \
    JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true \
    JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true \
    JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true \
    JARVIS_QA_NOTIFICATION_VALIDATED=true \
    JARVIS_QA_RESTART_VALIDATED=true \
    JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true \
    JARVIS_QA_OWNER_NAME="Jarvis QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the fake fixture." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the fake fixture." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the fake fixture." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="   " \
    JARVIS_QA_VOICE_TEST_PHRASE="Jarvis status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Jarvis status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected whitespace-only owner evidence to fail"
  fi

  if JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$tmp_dir/missing-transcript-handoff.json" \
    JARVIS_QA_EXPECTED_BUNDLE_ID="com.nobiletechnology.jarvis.selftest" \
    JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION" \
    JARVIS_QA_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_QA_FINDER_LAUNCH_VALIDATED=true \
    JARVIS_QA_MICROPHONE_VALIDATED=true \
    JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true \
    JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true \
    JARVIS_QA_NOTIFICATION_VALIDATED=true \
    JARVIS_QA_RESTART_VALIDATED=true \
    JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true \
    JARVIS_QA_OWNER_NAME="Jarvis QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the fake fixture." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the fake fixture." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the fake fixture." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the fake fixture." \
    JARVIS_QA_VOICE_TEST_PHRASE="Jarvis status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Jarvis status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected missing transcript handoff validation to fail"
  fi

  if JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$tmp_dir/missing-owner-note.json" \
    JARVIS_QA_EXPECTED_BUNDLE_ID="com.nobiletechnology.jarvis.selftest" \
    JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION" \
    JARVIS_QA_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_QA_FINDER_LAUNCH_VALIDATED=true \
    JARVIS_QA_MICROPHONE_VALIDATED=true \
    JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true \
    JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true \
    JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true \
    JARVIS_QA_NOTIFICATION_VALIDATED=true \
    JARVIS_QA_RESTART_VALIDATED=true \
    JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true \
    JARVIS_QA_OWNER_NAME="Jarvis QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the fake fixture." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the fake fixture." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the fake fixture." \
    JARVIS_QA_VOICE_TEST_PHRASE="Jarvis status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Jarvis status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected missing owner-recorded audio evidence note to fail"
  fi

  if JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$tmp_dir/missing-structured-transcript.json" \
    JARVIS_QA_EXPECTED_BUNDLE_ID="com.nobiletechnology.jarvis.selftest" \
    JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION" \
    JARVIS_QA_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_QA_FINDER_LAUNCH_VALIDATED=true \
    JARVIS_QA_MICROPHONE_VALIDATED=true \
    JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true \
    JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true \
    JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true \
    JARVIS_QA_NOTIFICATION_VALIDATED=true \
    JARVIS_QA_RESTART_VALIDATED=true \
    JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true \
    JARVIS_QA_OWNER_NAME="Jarvis QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the fake fixture." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the fake fixture." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the fake fixture." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the fake fixture." \
    JARVIS_QA_VOICE_TEST_PHRASE="Jarvis status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Jarvis status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected missing structured command observation to fail"
  fi

  if JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$tmp_dir/mismatched-observed-transcript.json" \
    JARVIS_QA_EXPECTED_BUNDLE_ID="com.nobiletechnology.jarvis.selftest" \
    JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION" \
    JARVIS_QA_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_QA_FINDER_LAUNCH_VALIDATED=true \
    JARVIS_QA_MICROPHONE_VALIDATED=true \
    JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true \
    JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true \
    JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true \
    JARVIS_QA_NOTIFICATION_VALIDATED=true \
    JARVIS_QA_RESTART_VALIDATED=true \
    JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true \
    JARVIS_QA_OWNER_NAME="Jarvis QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the fake fixture." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the fake fixture." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the fake fixture." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the fake fixture." \
    JARVIS_QA_VOICE_TEST_PHRASE="Jarvis status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Jarvis stats check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected mismatched observed transcript to fail"
  fi

  if JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$tmp_dir/mismatched-command-observation.json" \
    JARVIS_QA_EXPECTED_BUNDLE_ID="com.nobiletechnology.jarvis.selftest" \
    JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION" \
    JARVIS_QA_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_QA_FINDER_LAUNCH_VALIDATED=true \
    JARVIS_QA_MICROPHONE_VALIDATED=true \
    JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true \
    JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true \
    JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true \
    JARVIS_QA_NOTIFICATION_VALIDATED=true \
    JARVIS_QA_RESTART_VALIDATED=true \
    JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true \
    JARVIS_QA_OWNER_NAME="Jarvis QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the fake fixture." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the fake fixture." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the fake fixture." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the fake fixture." \
    JARVIS_QA_VOICE_TEST_PHRASE="Jarvis status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Jarvis status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="different command" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected mismatched command observation to fail"
  fi

  if JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$tmp_dir/malformed-command-result-evidence-id.json" \
    JARVIS_QA_EXPECTED_BUNDLE_ID="com.nobiletechnology.jarvis.selftest" \
    JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION" \
    JARVIS_QA_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_QA_FINDER_LAUNCH_VALIDATED=true \
    JARVIS_QA_MICROPHONE_VALIDATED=true \
    JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true \
    JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true \
    JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true \
    JARVIS_QA_NOTIFICATION_VALIDATED=true \
    JARVIS_QA_RESTART_VALIDATED=true \
    JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true \
    JARVIS_QA_OWNER_NAME="Jarvis QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the fake fixture." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the fake fixture." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the fake fixture." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the fake fixture." \
    JARVIS_QA_VOICE_TEST_PHRASE="Jarvis status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Jarvis status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="looked good" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected malformed command result evidence id to fail"
  fi

  if JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$tmp_dir/bad-timestamp-order.json" \
    JARVIS_QA_EXPECTED_BUNDLE_ID="com.nobiletechnology.jarvis.selftest" \
    JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION" \
    JARVIS_QA_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_QA_FINDER_LAUNCH_VALIDATED=true \
    JARVIS_QA_MICROPHONE_VALIDATED=true \
    JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true \
    JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true \
    JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true \
    JARVIS_QA_NOTIFICATION_VALIDATED=true \
    JARVIS_QA_RESTART_VALIDATED=true \
    JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true \
    JARVIS_QA_OWNER_NAME="Jarvis QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the fake fixture." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the fake fixture." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the fake fixture." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the fake fixture." \
    JARVIS_QA_VOICE_TEST_PHRASE="Jarvis status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Jarvis status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected timestamp order validation to fail"
  fi

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
- Record all JARVIS_QA_* flags as true, add owner/device/profile/timestamp and
  voice evidence notes, or run --write-template target/release-live-device-qa.env
  to generate the complete fillable environment file. Then rerun this script
  with --assert-complete on the validated release machine.
- Preserve the generated JSON report from --assert-complete as release evidence.

Proof boundary: preflight and runbook only; no live device validation was
performed by --check.
CHECKLIST
  exit 0
fi

[[ -d "$APP_PATH" ]] || fail "installed app is missing: $APP_PATH"
[[ -x "$APP_PATH/Contents/MacOS/JarvisMacApp" ]] || fail "installed app executable is missing or not executable"
[[ -x "$APP_PATH/Contents/Resources/bin/jarvis-cli" ]] || fail "bundled core executable is missing or not executable"
validate_installed_app_bundle_metadata

require_true JARVIS_QA_CLEAN_PROFILE_VALIDATED
require_true JARVIS_QA_FINDER_LAUNCH_VALIDATED
require_true JARVIS_QA_MICROPHONE_VALIDATED
require_true JARVIS_QA_SPEECH_PERMISSION_VALIDATED
require_true JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED
require_true JARVIS_QA_AUDIO_OUTPUT_VALIDATED
require_true JARVIS_QA_NOTIFICATION_VALIDATED
require_true JARVIS_QA_RESTART_VALIDATED
require_true JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED
require_non_empty_env JARVIS_QA_OWNER_NAME
require_non_empty_env JARVIS_QA_DEVICE_LABEL
require_non_empty_env JARVIS_QA_PROFILE_LABEL
require_utc_timestamp_env JARVIS_QA_VOICE_CHECK_STARTED_AT
require_utc_timestamp_env JARVIS_QA_VOICE_CHECK_COMPLETED_AT
require_timestamp_order JARVIS_QA_VOICE_CHECK_STARTED_AT JARVIS_QA_VOICE_CHECK_COMPLETED_AT
require_not_future_timestamp_env JARVIS_QA_VOICE_CHECK_COMPLETED_AT
require_non_empty_env JARVIS_QA_MICROPHONE_EVIDENCE_NOTE
require_non_empty_env JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE
require_non_empty_env JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE
require_non_empty_env JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE
require_non_empty_env JARVIS_QA_VOICE_TEST_PHRASE
require_non_empty_env JARVIS_QA_OBSERVED_TRANSCRIPT
require_observed_transcript_matches_phrase
require_trimmed_env_match JARVIS_QA_EXPECTED_COMMAND_TEXT JARVIS_QA_OBSERVED_COMMAND_TEXT
require_non_empty_env JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID
require_command_result_evidence_id_env JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID
require_non_empty_env JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL
write_report

cat <<EOF
Jarvis live-device QA assertion: complete
Installed app: $APP_PATH
Bundle: $APP_BUNDLE_ID $APP_SHORT_VERSION ($APP_BUILD_VERSION)
Report: $REPORT_PATH
Proof boundary: owner-recorded clean-profile install, Finder launch,
microphone/Speech permission prompts, spoken transcript handoff into the command
path, live audio output, notification, restart, and manual release QA flags
only; this still does not prove App Store review, marketplace trust, malware
analysis, or OS-level sandbox/egress enforcement.
EOF
