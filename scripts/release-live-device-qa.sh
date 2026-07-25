#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_PATH="${JARVIS_QA_INSTALLED_APP_PATH:-/Applications/Assemblywright.app}"
REPORT_PATH="${JARVIS_QA_REPORT_PATH:-$ROOT_DIR/target/release-live-device-qa-report.json}"
EXPECTED_BUNDLE_ID="${JARVIS_QA_EXPECTED_BUNDLE_ID:-com.nobiletechnology.jarvis}"
EXPECTED_VERSION="${JARVIS_QA_EXPECTED_VERSION:-$("$ROOT_DIR/scripts/release-version.sh")}"
SIGNED_PROVENANCE_PATH="${JARVIS_QA_SIGNED_PROVENANCE_REPORT:-$ROOT_DIR/target/distribution/Assemblywright-$EXPECTED_VERSION-signed-provenance.json}"
EXPECTED_MICROPHONE_USAGE_DESCRIPTION="Assemblywright uses microphone input only when you explicitly start local voice capture."
EXPECTED_SPEECH_RECOGNITION_USAGE_DESCRIPTION="Assemblywright uses speech recognition only to turn your spoken command into a local assistant request."
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
APP_BUNDLED_CORE_PATH=""
APP_BUNDLED_CORE_VERSION=""
APP_BUNDLED_CORE_SHA256=""
APP_EXECUTABLE_PATH=""
APP_EXECUTABLE_SHA256=""
APP_EXECUTABLE_IDENTIFIER=""
APP_EXECUTABLE_TEAM_IDENTIFIER=""
APP_EXECUTABLE_CDHASH=""
SIGNED_PROVENANCE_SHA256=""

usage() {
  cat <<'USAGE'
Usage: scripts/release-live-device-qa.sh [--check|--assert-complete|--self-test|--write-template PATH]

Prepare or assert the live-device release QA gate for Assemblywright.

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

The owner must also record non-empty live QA evidence notes:
  JARVIS_QA_OWNER_NAME
  JARVIS_QA_DEVICE_LABEL
  JARVIS_QA_PROFILE_LABEL
  JARVIS_QA_VOICE_CHECK_STARTED_AT
  JARVIS_QA_VOICE_CHECK_COMPLETED_AT
  JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE
  JARVIS_QA_FINDER_LAUNCH_EVIDENCE_NOTE
  JARVIS_QA_MICROPHONE_EVIDENCE_NOTE
  JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE
  JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE
  JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE
  JARVIS_QA_NOTIFICATION_EVIDENCE_NOTE
  JARVIS_QA_NOTIFICATION_OBSERVED_AT
  JARVIS_QA_NOTIFICATION_KIND
  JARVIS_QA_NOTIFICATION_TITLE
  JARVIS_QA_NOTIFICATION_BODY
  JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER
  JARVIS_QA_RESTART_EVIDENCE_NOTE
  JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE
  JARVIS_QA_VOICE_TEST_PHRASE
  JARVIS_QA_OBSERVED_TRANSCRIPT
  JARVIS_QA_EXPECTED_COMMAND_TEXT
  JARVIS_QA_OBSERVED_COMMAND_TEXT
  JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID
  JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL

Owner-recorded evidence notes must contain non-placeholder release evidence, not
values such as `TODO`, `TBD`, `pending`, `n/a`, `fixture`, or `self-test
fixture`. Other owner metadata fields must contain non-whitespace text.
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
  JARVIS_QA_INSTALLED_APP_PATH     Defaults to /Applications/Assemblywright.app
  JARVIS_QA_REPORT_PATH            Defaults to target/release-live-device-qa-report.json
  JARVIS_QA_SIGNED_PROVENANCE_REPORT
                                    Defaults to target/distribution/Assemblywright-<version>-signed-provenance.json
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

file_sha256() {
  local path="$1"
  shasum -a 256 "$path" | awk '{print $1}'
}

json_string_value() {
  local path="$1"
  local field="$2"
  python3 - "$path" "$field" <<'PY'
import json
import sys

path, field = sys.argv[1:3]
with open(path, encoding="utf-8") as handle:
    value = json.load(handle)
for part in field.split("."):
    if not isinstance(value, dict) or part not in value:
        raise SystemExit(f"missing required signed provenance field: {field}")
    value = value[part]
if not isinstance(value, str) or not value.strip():
    raise SystemExit(f"signed provenance field must be a non-empty string: {field}")
print(value, end="")
PY
}

codesign_metadata_value() {
  local output="$1"
  local key="$2"
  local value
  value="$(printf '%s\n' "$output" | awk -F= -v key="$key" '$1 == key { print substr($0, length(key) + 2); exit }')"
  [[ -n "$value" ]] || fail "installed app executable codesign evidence is missing $key"
  printf '%s' "$value"
}

require_hex_identity_value() {
  local label="$1"
  local value="$2"
  local minimum_length="$3"
  python3 - "$label" "$value" "$minimum_length" <<'PY'
import re
import sys

label, value, minimum_length = sys.argv[1:4]
if not re.fullmatch(rf"[0-9A-Fa-f]{{{int(minimum_length)},64}}", value):
    raise SystemExit(f"{label} must be a hexadecimal value between {minimum_length} and 64 characters")
PY
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

require_notification_kind_env() {
  local name="$1"
  local value="${!name:-}"
  require_non_empty_env "$name"
  case "$value" in
    due_now|failed|blocked_by_emergency_pause)
      ;;
    *)
      fail "$name must be one of due_now, failed, or blocked_by_emergency_pause"
      ;;
  esac
}

require_env_equals() {
  local name="$1"
  local expected="$2"
  local value="${!name:-}"
  require_non_empty_env "$name"
  [[ "$value" == "$expected" ]] || fail "$name must be $expected"
}

require_owner_evidence_note_env() {
  local name="$1"
  local value="${!name:-}"
  require_non_empty_env "$name"
  require_command python3
  python3 - "$name" "$value" <<'PY'
import sys

name, value = sys.argv[1:3]
normalized = " ".join(value.strip().lower().split())
exact_placeholders = {"n/a", "na"}
embedded_placeholders = (
    "self-test",
    "placeholder",
    "example",
    "fixture",
    "todo",
    "tbd",
    "replace-me",
    "changeme",
    "pending",
)
if normalized in exact_placeholders or any(marker in normalized for marker in embedded_placeholders):
    raise SystemExit(
        f"{name} must contain owner-recorded external evidence, not placeholder or fixture text"
    )
PY
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

require_plist_value_equals() {
  local label="$1"
  local value="$2"
  local expected="$3"
  require_plist_value "$label" "$value"
  [[ "$value" == "$expected" ]] ||
    fail "installed app Info.plist $label mismatch: expected '$expected', got '$value'"
}

validate_installed_app_bundle_metadata() {
  local info_plist="$APP_PATH/Contents/Info.plist"
  local core_path="$APP_PATH/Contents/Resources/bin/jarvis-cli"
  plutil -lint "$info_plist" >/dev/null
  APP_BUNDLE_ID="$(plist_value "$info_plist" CFBundleIdentifier)"
  APP_SHORT_VERSION="$(plist_value "$info_plist" CFBundleShortVersionString)"
  APP_BUILD_VERSION="$(plist_value "$info_plist" CFBundleVersion)"
  APP_MICROPHONE_USAGE="$(plist_value "$info_plist" NSMicrophoneUsageDescription)"
  APP_SPEECH_USAGE="$(plist_value "$info_plist" NSSpeechRecognitionUsageDescription)"
  APP_BUNDLED_CORE_PATH="$core_path"

  require_plist_value "CFBundleIdentifier" "$APP_BUNDLE_ID"
  require_plist_value "CFBundleShortVersionString" "$APP_SHORT_VERSION"
  require_plist_value "CFBundleVersion" "$APP_BUILD_VERSION"
  require_plist_value_equals "NSMicrophoneUsageDescription" "$APP_MICROPHONE_USAGE" "$EXPECTED_MICROPHONE_USAGE_DESCRIPTION"
  require_plist_value_equals "NSSpeechRecognitionUsageDescription" "$APP_SPEECH_USAGE" "$EXPECTED_SPEECH_RECOGNITION_USAGE_DESCRIPTION"
  [[ -x "$core_path" ]] || fail "installed app bundled core is missing or not executable: $core_path"

  [[ "$APP_BUNDLE_ID" == "$EXPECTED_BUNDLE_ID" ]] ||
    fail "installed app bundle id mismatch: expected $EXPECTED_BUNDLE_ID, got $APP_BUNDLE_ID"
  [[ "$APP_SHORT_VERSION" == "$EXPECTED_VERSION" ]] ||
    fail "installed app short version mismatch: expected $EXPECTED_VERSION, got $APP_SHORT_VERSION"
  [[ "$APP_BUILD_VERSION" == "$EXPECTED_VERSION" ]] ||
    fail "installed app build version mismatch: expected $EXPECTED_VERSION, got $APP_BUILD_VERSION"
  APP_BUNDLED_CORE_VERSION="$("$core_path" --version 2>&1)"
  [[ "$APP_BUNDLED_CORE_VERSION" == *"assemblywright $EXPECTED_VERSION"* ]] ||
    fail "installed app bundled core version mismatch: expected assemblywright $EXPECTED_VERSION, got $APP_BUNDLED_CORE_VERSION"
  require_command shasum
  APP_BUNDLED_CORE_SHA256="$(shasum -a 256 "$core_path" | awk '{print $1}')"
}

validate_installed_app_executable_binding() {
  local codesign_output
  local provenance_app_path
  local provenance_executable_path
  local provenance_executable_sha256
  local provenance_identifier
  local provenance_team_identifier
  local provenance_cdhash

  require_command python3
  require_command shasum
  require_command codesign
  require_command xcrun
  require_command spctl
  [[ -f "$SIGNED_PROVENANCE_PATH" ]] ||
    fail "signed-distribution provenance report is missing: $SIGNED_PROVENANCE_PATH"

  APP_EXECUTABLE_PATH="$APP_PATH/Contents/MacOS/JarvisMacApp"
  [[ -x "$APP_EXECUTABLE_PATH" ]] ||
    fail "installed app executable is missing or not executable: $APP_EXECUTABLE_PATH"
  APP_EXECUTABLE_SHA256="$(file_sha256 "$APP_EXECUTABLE_PATH")"
  SIGNED_PROVENANCE_SHA256="$(file_sha256 "$SIGNED_PROVENANCE_PATH")"

  [[ "$(json_string_value "$SIGNED_PROVENANCE_PATH" evidence_type)" == "signed_distribution_provenance" ]] ||
    fail "signed provenance evidence_type must be signed_distribution_provenance"
  [[ "$(json_string_value "$SIGNED_PROVENANCE_PATH" version)" == "$EXPECTED_VERSION" ]] ||
    fail "signed provenance version does not match $EXPECTED_VERSION"
  [[ "$(json_string_value "$SIGNED_PROVENANCE_PATH" bundle_identifier)" == "$EXPECTED_BUNDLE_ID" ]] ||
    fail "signed provenance bundle_identifier does not match $EXPECTED_BUNDLE_ID"

  provenance_app_path="$(json_string_value "$SIGNED_PROVENANCE_PATH" artifacts.app_path)"
  provenance_executable_path="$(json_string_value "$SIGNED_PROVENANCE_PATH" artifacts.app_executable_path)"
  [[ "$provenance_executable_path" == "$provenance_app_path/Contents/MacOS/JarvisMacApp" ]] ||
    fail "signed provenance artifacts.app_executable_path is not inside artifacts.app_path"
  provenance_executable_sha256="$(json_string_value "$SIGNED_PROVENANCE_PATH" artifacts.app_executable_sha256)"
  [[ "$APP_EXECUTABLE_SHA256" == "$provenance_executable_sha256" ]] ||
    fail "installed app executable SHA-256 does not match signed provenance"

  codesign --verify --deep --strict --verbose=2 "$APP_PATH" >/dev/null
  codesign --verify --strict --verbose=2 "$APP_EXECUTABLE_PATH" >/dev/null
  codesign_output="$(codesign -dv --verbose=4 "$APP_EXECUTABLE_PATH" 2>&1)"
  [[ "$codesign_output" == *"Authority=Developer ID Application: "* ]] ||
    fail "installed app executable is not signed by a Developer ID Application identity"
  APP_EXECUTABLE_IDENTIFIER="$(codesign_metadata_value "$codesign_output" Identifier)"
  APP_EXECUTABLE_TEAM_IDENTIFIER="$(codesign_metadata_value "$codesign_output" TeamIdentifier)"
  APP_EXECUTABLE_CDHASH="$(codesign_metadata_value "$codesign_output" CDHash)"
  [[ "$APP_EXECUTABLE_IDENTIFIER" == "$EXPECTED_BUNDLE_ID" ]] ||
    fail "installed app executable identifier mismatch: expected $EXPECTED_BUNDLE_ID, got $APP_EXECUTABLE_IDENTIFIER"
  [[ "$APP_EXECUTABLE_TEAM_IDENTIFIER" =~ ^[A-Z0-9]{10}$ ]] ||
    fail "installed app executable TeamIdentifier must be a 10-character Apple team identifier"
  require_hex_identity_value "installed app executable CDHash" "$APP_EXECUTABLE_CDHASH" 40

  provenance_identifier="$(json_string_value "$SIGNED_PROVENANCE_PATH" signing.app_executable_identifier)"
  provenance_team_identifier="$(json_string_value "$SIGNED_PROVENANCE_PATH" signing.app_executable_team_identifier)"
  provenance_cdhash="$(json_string_value "$SIGNED_PROVENANCE_PATH" signing.app_executable_cdhash)"
  [[ "$APP_EXECUTABLE_IDENTIFIER" == "$provenance_identifier" ]] ||
    fail "installed app executable identifier does not match signed provenance"
  [[ "$APP_EXECUTABLE_TEAM_IDENTIFIER" == "$provenance_team_identifier" ]] ||
    fail "installed app executable TeamIdentifier does not match signed provenance"
  [[ "$APP_EXECUTABLE_CDHASH" == "$provenance_cdhash" ]] ||
    fail "installed app executable CDHash does not match signed provenance"

  xcrun stapler validate "$APP_PATH" >/dev/null
  spctl --assess --type execute --verbose "$APP_PATH" >/dev/null
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
  local escaped_bundled_core_path
  local escaped_bundled_core_version
  local escaped_bundled_core_sha256
  local escaped_app_executable_path
  local escaped_app_executable_sha256
  local escaped_app_executable_identifier
  local escaped_app_executable_team_identifier
  local escaped_app_executable_cdhash
  local escaped_signed_provenance_path
  local escaped_signed_provenance_sha256
  local escaped_owner_name
  local escaped_device_label
  local escaped_profile_label
  local escaped_started_at
  local escaped_completed_at
  local escaped_microphone_note
  local escaped_speech_note
  local escaped_transcript_note
  local escaped_audio_note
  local escaped_clean_profile_note
  local escaped_finder_launch_note
  local escaped_notification_note
  local escaped_notification_observed_at
  local escaped_notification_kind
  local escaped_notification_title
  local escaped_notification_body
  local escaped_notification_thread_identifier
  local escaped_restart_note
  local escaped_manual_release_qa_note
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
  escaped_bundled_core_path="$(json_escape "$APP_BUNDLED_CORE_PATH")"
  escaped_bundled_core_version="$(json_escape "$APP_BUNDLED_CORE_VERSION")"
  escaped_bundled_core_sha256="$(json_escape "$APP_BUNDLED_CORE_SHA256")"
  escaped_app_executable_path="$(json_escape "$APP_EXECUTABLE_PATH")"
  escaped_app_executable_sha256="$(json_escape "$APP_EXECUTABLE_SHA256")"
  escaped_app_executable_identifier="$(json_escape "$APP_EXECUTABLE_IDENTIFIER")"
  escaped_app_executable_team_identifier="$(json_escape "$APP_EXECUTABLE_TEAM_IDENTIFIER")"
  escaped_app_executable_cdhash="$(json_escape "$APP_EXECUTABLE_CDHASH")"
  escaped_signed_provenance_path="$(json_escape "$SIGNED_PROVENANCE_PATH")"
  escaped_signed_provenance_sha256="$(json_escape "$SIGNED_PROVENANCE_SHA256")"
  escaped_owner_name="$(json_escape "$JARVIS_QA_OWNER_NAME")"
  escaped_device_label="$(json_escape "$JARVIS_QA_DEVICE_LABEL")"
  escaped_profile_label="$(json_escape "$JARVIS_QA_PROFILE_LABEL")"
  escaped_started_at="$(json_escape "$JARVIS_QA_VOICE_CHECK_STARTED_AT")"
  escaped_completed_at="$(json_escape "$JARVIS_QA_VOICE_CHECK_COMPLETED_AT")"
  escaped_microphone_note="$(json_escape "$JARVIS_QA_MICROPHONE_EVIDENCE_NOTE")"
  escaped_speech_note="$(json_escape "$JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE")"
  escaped_transcript_note="$(json_escape "$JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE")"
  escaped_audio_note="$(json_escape "$JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE")"
  escaped_clean_profile_note="$(json_escape "$JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE")"
  escaped_finder_launch_note="$(json_escape "$JARVIS_QA_FINDER_LAUNCH_EVIDENCE_NOTE")"
  escaped_notification_note="$(json_escape "$JARVIS_QA_NOTIFICATION_EVIDENCE_NOTE")"
  escaped_notification_observed_at="$(json_escape "$JARVIS_QA_NOTIFICATION_OBSERVED_AT")"
  escaped_notification_kind="$(json_escape "$JARVIS_QA_NOTIFICATION_KIND")"
  escaped_notification_title="$(json_escape "$JARVIS_QA_NOTIFICATION_TITLE")"
  escaped_notification_body="$(json_escape "$JARVIS_QA_NOTIFICATION_BODY")"
  escaped_notification_thread_identifier="$(json_escape "$JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER")"
  escaped_restart_note="$(json_escape "$JARVIS_QA_RESTART_EVIDENCE_NOTE")"
  escaped_manual_release_qa_note="$(json_escape "$JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE")"
  escaped_voice_test_phrase="$(json_escape "$JARVIS_QA_VOICE_TEST_PHRASE")"
  escaped_observed_transcript="$(json_escape "$JARVIS_QA_OBSERVED_TRANSCRIPT")"
  escaped_expected_command_text="$(json_escape "$JARVIS_QA_EXPECTED_COMMAND_TEXT")"
  escaped_observed_command_text="$(json_escape "$JARVIS_QA_OBSERVED_COMMAND_TEXT")"
  escaped_command_result_evidence_id="$(json_escape "$JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID")"
  escaped_audio_output_device_label="$(json_escape "$JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL")"
  escaped_boundary="$(json_escape "Owner-recorded clean-profile install, Finder launch, microphone/Speech permission prompts, spoken transcript handoff into the command path, live audio output, notification, restart, and manual release QA flags bound at report generation to the exact installed app executable bytes and Developer ID code identity recorded by signed provenance. This is point-in-time candidate binding, not installation provenance, continuous filesystem integrity, App Store review, marketplace trust, malware analysis, or OS-level sandbox/egress enforcement.")"

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
  "app_executable": {
    "executable_path": "$escaped_app_executable_path",
    "sha256": "$escaped_app_executable_sha256",
    "code_identifier": "$escaped_app_executable_identifier",
    "team_identifier": "$escaped_app_executable_team_identifier",
    "cdhash": "$escaped_app_executable_cdhash"
  },
  "signed_provenance": {
    "report_path": "$escaped_signed_provenance_path",
    "sha256": "$escaped_signed_provenance_sha256"
  },
  "bundled_core": {
    "executable_path": "$escaped_bundled_core_path",
    "version": "$escaped_bundled_core_version",
    "sha256": "$escaped_bundled_core_sha256"
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
  "owner_recorded_non_voice_evidence": {
    "clean_profile_evidence_note": "$escaped_clean_profile_note",
    "finder_launch_evidence_note": "$escaped_finder_launch_note",
    "notification_evidence_note": "$escaped_notification_note",
    "notification_observed_at": "$escaped_notification_observed_at",
    "restart_evidence_note": "$escaped_restart_note",
    "manual_release_qa_evidence_note": "$escaped_manual_release_qa_note"
  },
  "notification_observation": {
    "kind": "$escaped_notification_kind",
    "title": "$escaped_notification_title",
    "body": "$escaped_notification_body",
    "thread_identifier": "$escaped_notification_thread_identifier",
    "observed_at": "$escaped_notification_observed_at"
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
  cat >"$template_path" <<EOF
# Assemblywright live-device QA evidence template.
# Edit this file on the validated release machine after the signed, notarized
# app has been installed into a clean macOS profile and launched through Finder
# or LaunchServices.
#
# For the operator evidence session, launch Assemblywright with the exact opt-in
# JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF=true. Then set JARVIS_RELEASE_CORE_ENDPOINT
# to the running release core endpoint and JARVIS_IPC_TOKEN_FILE to the app-owned
# handoff file, source this template, and capture the command evidence ID from
# that same authenticated endpoint:
#   cargo run -p jarvis-cli -- command "status check" --endpoint "\${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}" --json
# Use the returned task ID as JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:<uuid>",
# or use an audit ID from task-associated command/audit evidence as "audit:<uuid>".
#
# After filling every field below, run:
#   set -a
#   source ./target/release-live-device-qa.env
#   set +a
#   ./scripts/release-live-device-qa.sh --assert-complete
#   JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint "\${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}"
#   JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint "\${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}"
#
# Keep all validation flags false until that check has actually been observed
# on the signed, notarized app installed in a clean macOS profile.
# Do not set JARVIS_QA_SELF_TEST_FIXTURE in release evidence; it is reserved for
# this script's internal fake-fixture self-test.

JARVIS_QA_INSTALLED_APP_PATH="/Applications/Assemblywright.app"
JARVIS_QA_REPORT_PATH="target/release-live-device-qa-report.json"
JARVIS_QA_SIGNED_PROVENANCE_REPORT="target/distribution/Assemblywright-$EXPECTED_VERSION-signed-provenance.json"
JARVIS_RELEASE_CORE_ENDPOINT="" # release core endpoint used for command evidence and external readiness checks
JARVIS_IPC_TOKEN_FILE="\$HOME/Library/Application Support/Jarvis/ipc-session-auth.json" # path only; never copy the bearer value into this template
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
JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE=""
JARVIS_QA_FINDER_LAUNCH_EVIDENCE_NOTE=""
JARVIS_QA_MICROPHONE_EVIDENCE_NOTE=""
JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE=""
JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE=""
JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE=""
JARVIS_QA_NOTIFICATION_EVIDENCE_NOTE=""
JARVIS_QA_NOTIFICATION_OBSERVED_AT=""
JARVIS_QA_NOTIFICATION_KIND=""
JARVIS_QA_NOTIFICATION_TITLE=""
JARVIS_QA_NOTIFICATION_BODY=""
JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER="jarvis.scheduler"
JARVIS_QA_RESTART_EVIDENCE_NOTE=""
JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE=""
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
require_file_contains "distribution packaging script" "$INFO_TEMPLATE_HINT" "$EXPECTED_MICROPHONE_USAGE_DESCRIPTION"
require_file_contains "distribution packaging script" "$INFO_TEMPLATE_HINT" "$EXPECTED_SPEECH_RECOGNITION_USAGE_DESCRIPTION"
require_file_contains "release checklist" "$ROOT_DIR/docs/release-checklist.md" "live microphone/Speech"
require_file_contains "release checklist" "$ROOT_DIR/docs/release-checklist.md" "live audio-output"

if [[ "$WRITE_TEMPLATE" == true ]]; then
  write_env_template "$WRITE_TEMPLATE_PATH"
  printf 'Assemblywright live-device QA env template written: %s\n' "$WRITE_TEMPLATE_PATH"
  printf 'Proof boundary: template generation only; no live device validation was performed.\n'
  exit 0
fi

if [[ "$SELF_TEST" == true ]]; then
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-live-qa-self-test.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT
  export JARVIS_QA_INTERNAL_SELF_TEST=true
  fixture_app="$tmp_dir/Assemblywright.app"
  fixture_report="$tmp_dir/release-live-device-qa-report.json"
  fixture_template="$tmp_dir/release-live-device-qa.env"
  fixture_signed_provenance="$tmp_dir/Assemblywright-$EXPECTED_VERSION-signed-provenance.json"
  stub_dir="$tmp_dir/bin"
  mkdir -p "$fixture_app/Contents/MacOS" "$fixture_app/Contents/Resources/bin" "$stub_dir"
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
  <string>$EXPECTED_MICROPHONE_USAGE_DESCRIPTION</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>$EXPECTED_SPEECH_RECOGNITION_USAGE_DESCRIPTION</string>
</dict>
</plist>
PLIST
  touch "$fixture_app/Contents/MacOS/JarvisMacApp"
  printf '#!/usr/bin/env sh\nprintf "assemblywright %s\\n"\n' "$EXPECTED_VERSION" >"$fixture_app/Contents/Resources/bin/jarvis-cli"
  chmod 755 "$fixture_app/Contents/MacOS/JarvisMacApp" "$fixture_app/Contents/Resources/bin/jarvis-cli"

  cat >"$stub_dir/codesign" <<'SH'
#!/usr/bin/env bash
if [[ " $* " == *" --verify "* ]]; then
  exit 0
fi
identifier="${JARVIS_QA_STUB_APP_IDENTIFIER:-com.nobiletechnology.jarvis.selftest}"
team_identifier="${JARVIS_QA_STUB_TEAM_IDENTIFIER:-9VZ742YKV4}"
cdhash="${JARVIS_QA_STUB_CDHASH:-0123456789abcdef0123456789abcdef01234567}"
printf 'Executable=/fixture/JarvisMacApp\nIdentifier=%s\nAuthority=Developer ID Application: Assemblywright QA Fixture\nTeamIdentifier=%s\nCDHash=%s\n' \
  "$identifier" "$team_identifier" "$cdhash"
SH
  cat >"$stub_dir/xcrun" <<'SH'
#!/usr/bin/env bash
if [[ "${1:-}" == "stapler" && "${2:-}" == "validate" ]]; then
  printf 'The validate action worked!\n'
  exit 0
fi
exit 1
SH
  cat >"$stub_dir/spctl" <<'SH'
#!/usr/bin/env bash
printf '%s: accepted\n' "${*: -1}"
SH
  chmod 755 "$stub_dir/codesign" "$stub_dir/xcrun" "$stub_dir/spctl"
  export PATH="$stub_dir:$PATH"

  fixture_app_executable_sha256="$(file_sha256 "$fixture_app/Contents/MacOS/JarvisMacApp")"
  cat >"$fixture_signed_provenance" <<JSON
{
  "schema_version": 1,
  "evidence_type": "signed_distribution_provenance",
  "version": "$EXPECTED_VERSION",
  "bundle_identifier": "com.nobiletechnology.jarvis.selftest",
  "artifacts": {
    "app_path": "$fixture_app",
    "app_executable_path": "$fixture_app/Contents/MacOS/JarvisMacApp",
    "app_executable_sha256": "$fixture_app_executable_sha256"
  },
  "signing": {
    "app_executable_identifier": "com.nobiletechnology.jarvis.selftest",
    "app_executable_team_identifier": "9VZ742YKV4",
    "app_executable_cdhash": "0123456789abcdef0123456789abcdef01234567"
  }
}
JSON
  export JARVIS_QA_SIGNED_PROVENANCE_REPORT="$fixture_signed_provenance"

  "$0" --write-template "$fixture_template" >/dev/null
  require_file_contains "live QA env template" "$fixture_template" "JARVIS_QA_EXPECTED_VERSION=\"$EXPECTED_VERSION\""
  require_file_contains "live QA env template" "$fixture_template" "JARVIS_QA_SIGNED_PROVENANCE_REPORT=\"target/distribution/Assemblywright-$EXPECTED_VERSION-signed-provenance.json\""
  if grep -F 'JARVIS_QA_EXPECTED_VERSION="$EXPECTED_VERSION"' "$fixture_template" >/dev/null 2>&1; then
    fail "live QA self-test expected env template to materialize the expected version"
  fi
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_CLEAN_PROFILE_VALIDATED=false'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=false'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE=""'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_NOTIFICATION_OBSERVED_AT=""'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_NOTIFICATION_KIND=""'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER="jarvis.scheduler"'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE=""'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_EXPECTED_COMMAND_TEXT=""'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_OBSERVED_COMMAND_TEXT=""'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_RELEASE_CORE_ENDPOINT=""'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_IPC_TOKEN_FILE="$HOME/Library/Application Support/Jarvis/ipc-session-auth.json"'
  require_file_contains "live QA env template" "$fixture_template" './scripts/release-live-device-qa.sh --assert-complete'
  require_file_contains "live QA env template" "$fixture_template" 'cargo run -p jarvis-cli -- command "status check" --endpoint "${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}" --json'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:<uuid>"'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint "${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}"'
  require_file_contains "live QA env template" "$fixture_template" 'JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint "${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}"'
  require_file_contains "live QA env template" "$fixture_template" 'Do not set JARVIS_QA_SELF_TEST_FIXTURE in release evidence'
  if grep -F 'JARVIS_QA_CLEAN_PROFILE_VALIDATED=true' "$fixture_template" >/dev/null 2>&1; then
    fail "live QA self-test expected env template validation flags to default false"
  fi

  export JARVIS_QA_NOTIFICATION_KIND="due_now"
  export JARVIS_QA_NOTIFICATION_TITLE="Scheduler job ready: self-test release reminder"
  export JARVIS_QA_NOTIFICATION_BODY="A scheduled Assemblywright job is due now."
  export JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER="jarvis.scheduler"

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
    JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE="Clean profile install observed in the controlled release QA lane." \
    JARVIS_QA_FINDER_LAUNCH_EVIDENCE_NOTE="Finder launch observed in the controlled release QA lane." \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the controlled release QA lane." \
    JARVIS_QA_NOTIFICATION_EVIDENCE_NOTE="Visible scheduler notification observed in the controlled release QA lane." \
    JARVIS_QA_NOTIFICATION_OBSERVED_AT="2026-05-22T16:04:00Z" \
    JARVIS_QA_NOTIFICATION_KIND="due_now" \
    JARVIS_QA_NOTIFICATION_TITLE="Scheduler job ready: self-test release reminder" \
    JARVIS_QA_NOTIFICATION_BODY="A scheduled Assemblywright job is due now." \
    JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER="jarvis.scheduler" \
    JARVIS_QA_RESTART_EVIDENCE_NOTE="Restart recovery observed in the controlled release QA lane." \
    JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE="Manual release QA surfaces observed in the controlled release QA lane." \
    JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright status check." \
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
  require_file_contains "live QA self-test report" "$fixture_report" "$EXPECTED_MICROPHONE_USAGE_DESCRIPTION"
  require_file_contains "live QA self-test report" "$fixture_report" "$EXPECTED_SPEECH_RECOGNITION_USAGE_DESCRIPTION"
  require_file_contains "live QA self-test report" "$fixture_report" '"bundled_core"'
  require_file_contains "live QA self-test report" "$fixture_report" '"app_executable"'
  require_file_contains "live QA self-test report" "$fixture_report" '"code_identifier": "com.nobiletechnology.jarvis.selftest"'
  require_file_contains "live QA self-test report" "$fixture_report" '"team_identifier": "9VZ742YKV4"'
  require_file_contains "live QA self-test report" "$fixture_report" '"cdhash": "0123456789abcdef0123456789abcdef01234567"'
  require_file_contains "live QA self-test report" "$fixture_report" '"signed_provenance"'
  require_file_contains "live QA self-test report" "$fixture_report" "\"report_path\": \"$fixture_signed_provenance\""
  require_file_contains "live QA self-test report" "$fixture_report" '"executable_path"'
  require_file_contains "live QA self-test report" "$fixture_report" "\"version\": \"assemblywright $EXPECTED_VERSION\""
  require_file_contains "live QA self-test report" "$fixture_report" '"sha256"'
  require_file_contains "live QA self-test report" "$fixture_report" '"transcript_handoff": true'
  require_file_contains "live QA self-test report" "$fixture_report" '"same_command_path": true'
  require_file_contains "live QA self-test report" "$fixture_report" '"owner_recorded_live_voice_evidence"'
  require_file_contains "live QA self-test report" "$fixture_report" '"owner_recorded_non_voice_evidence"'
  require_file_contains "live QA self-test report" "$fixture_report" '"clean_profile_evidence_note": "Clean profile install observed in the controlled release QA lane."'
  require_file_contains "live QA self-test report" "$fixture_report" '"notification_observed_at": "2026-05-22T16:04:00Z"'
  require_file_contains "live QA self-test report" "$fixture_report" '"notification_observation"'
  require_file_contains "live QA self-test report" "$fixture_report" '"kind": "due_now"'
  require_file_contains "live QA self-test report" "$fixture_report" '"title": "Scheduler job ready: self-test release reminder"'
  require_file_contains "live QA self-test report" "$fixture_report" '"body": "A scheduled Assemblywright job is due now."'
  require_file_contains "live QA self-test report" "$fixture_report" '"thread_identifier": "jarvis.scheduler"'
  require_file_contains "live QA self-test report" "$fixture_report" '"owner_name": "Assemblywright QA Self-Test"'
  require_file_contains "live QA self-test report" "$fixture_report" '"voice_command_observation"'
  require_file_contains "live QA self-test report" "$fixture_report" '"expected_command_text": "status check"'
  require_file_contains "live QA self-test report" "$fixture_report" '"observed_command_text": "status check"'
  require_file_contains "live QA self-test report" "$fixture_report" '"audio_output_evidence_note": "Observed speech output playback in the controlled release QA lane."'
  require_file_contains "live QA self-test report" "$fixture_report" '"proof_boundary"'

  run_fixture_assertion() {
    local report_path="$1"
    shift
    env \
      JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
      JARVIS_QA_REPORT_PATH="$report_path" \
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
      JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
      JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
      JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
      JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
      JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
      JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE="Clean profile install observed in the controlled release QA lane." \
      JARVIS_QA_FINDER_LAUNCH_EVIDENCE_NOTE="Finder launch observed in the controlled release QA lane." \
      JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
      JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
      JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
      JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the controlled release QA lane." \
      JARVIS_QA_NOTIFICATION_EVIDENCE_NOTE="Visible scheduler notification observed in the controlled release QA lane." \
      JARVIS_QA_NOTIFICATION_OBSERVED_AT="2026-05-22T16:04:00Z" \
      JARVIS_QA_NOTIFICATION_KIND="due_now" \
      JARVIS_QA_NOTIFICATION_TITLE="Scheduler job ready: self-test release reminder" \
      JARVIS_QA_NOTIFICATION_BODY="A scheduled Assemblywright job is due now." \
      JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER="jarvis.scheduler" \
      JARVIS_QA_RESTART_EVIDENCE_NOTE="Restart recovery observed in the controlled release QA lane." \
      JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE="Manual release QA surfaces observed in the controlled release QA lane." \
      JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
      JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright status check." \
      JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
      JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
      JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
      JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
      JARVIS_QA_SELF_TEST_FIXTURE=true \
      "$@" \
      "$0" --assert-complete
  }

  set_fixture_plist_value() {
    local key="$1"
    local value="$2"
    python3 - "$fixture_app/Contents/Info.plist" "$key" "$value" <<'PY'
import plistlib
import sys

path, key, value = sys.argv[1:4]
with open(path, "rb") as handle:
    data = plistlib.load(handle)
data[key] = value
with open(path, "wb") as handle:
    plistlib.dump(data, handle)
PY
  }

  cp "$fixture_app/Contents/MacOS/JarvisMacApp" "$tmp_dir/JarvisMacApp.original"
  printf 'mutated after signed provenance\n' >>"$fixture_app/Contents/MacOS/JarvisMacApp"
  if run_fixture_assertion "$tmp_dir/mutated-app-executable.json" >/dev/null 2>"$tmp_dir/mutated-app-executable.err"; then
    fail "live QA self-test expected mutated app executable to fail"
  fi
  require_file_contains "live QA self-test mutated app executable error" \
    "$tmp_dir/mutated-app-executable.err" "installed app executable SHA-256 does not match signed provenance"
  mv "$tmp_dir/JarvisMacApp.original" "$fixture_app/Contents/MacOS/JarvisMacApp"
  chmod 755 "$fixture_app/Contents/MacOS/JarvisMacApp"

  if run_fixture_assertion "$tmp_dir/mismatched-app-identity.json" \
    JARVIS_QA_STUB_APP_IDENTIFIER="com.example.WrongJarvis" >/dev/null 2>"$tmp_dir/mismatched-app-identity.err"; then
    fail "live QA self-test expected mismatched app executable identity to fail"
  fi
  require_file_contains "live QA self-test mismatched app executable identity error" \
    "$tmp_dir/mismatched-app-identity.err" "installed app executable identifier mismatch"

  python3 - "$fixture_signed_provenance" "$tmp_dir/mismatched-team-signed-provenance.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["signing"]["app_executable_team_identifier"] = "AAAAAAAAAA"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if run_fixture_assertion "$tmp_dir/mismatched-provenance-team.json" \
    JARVIS_QA_SIGNED_PROVENANCE_REPORT="$tmp_dir/mismatched-team-signed-provenance.json" >/dev/null 2>"$tmp_dir/mismatched-provenance-team.err"; then
    fail "live QA self-test expected mismatched signed provenance team identifier to fail"
  fi
  require_file_contains "live QA self-test mismatched signed provenance team error" \
    "$tmp_dir/mismatched-provenance-team.err" "TeamIdentifier does not match signed provenance"

  set_fixture_plist_value NSMicrophoneUsageDescription "Assemblywright microphone fixture"
  if run_fixture_assertion "$tmp_dir/mismatched-microphone-usage.json" >/dev/null 2>"$tmp_dir/mismatched-microphone-usage.err"; then
    fail "live QA self-test expected mismatched microphone usage description to fail"
  fi
  require_file_contains "live QA self-test mismatched microphone usage error" \
    "$tmp_dir/mismatched-microphone-usage.err" "NSMicrophoneUsageDescription mismatch"
  set_fixture_plist_value NSMicrophoneUsageDescription "$EXPECTED_MICROPHONE_USAGE_DESCRIPTION"

  set_fixture_plist_value NSSpeechRecognitionUsageDescription "Assemblywright speech fixture"
  if run_fixture_assertion "$tmp_dir/mismatched-speech-usage.json" >/dev/null 2>"$tmp_dir/mismatched-speech-usage.err"; then
    fail "live QA self-test expected mismatched Speech usage description to fail"
  fi
  require_file_contains "live QA self-test mismatched Speech usage error" \
    "$tmp_dir/mismatched-speech-usage.err" "NSSpeechRecognitionUsageDescription mismatch"
  set_fixture_plist_value NSSpeechRecognitionUsageDescription "$EXPECTED_SPEECH_RECOGNITION_USAGE_DESCRIPTION"

  if run_fixture_assertion "$tmp_dir/placeholder-live-voice-evidence.json" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="TODO before release" >/dev/null 2>"$tmp_dir/placeholder-live-voice-evidence.err"; then
    fail "live QA self-test expected placeholder live voice evidence note to fail"
  fi
  require_file_contains "live QA self-test placeholder live voice error" \
    "$tmp_dir/placeholder-live-voice-evidence.err" "owner-recorded external evidence"

  if run_fixture_assertion "$tmp_dir/placeholder-non-voice-evidence.json" \
    JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE="self-test fixture" >/dev/null 2>"$tmp_dir/placeholder-non-voice-evidence.err"; then
    fail "live QA self-test expected placeholder non-voice evidence note to fail"
  fi
  require_file_contains "live QA self-test placeholder non-voice error" \
    "$tmp_dir/placeholder-non-voice-evidence.err" "owner-recorded external evidence"

  if run_fixture_assertion "$tmp_dir/embedded-fixture-evidence.json" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the fake fixture." >/dev/null 2>"$tmp_dir/embedded-fixture-evidence.err"; then
    fail "live QA self-test expected embedded fixture evidence note to fail"
  fi
  require_file_contains "live QA self-test embedded fixture evidence error" \
    "$tmp_dir/embedded-fixture-evidence.err" "owner-recorded external evidence"

  if run_fixture_assertion "$tmp_dir/invalid-notification-kind.json" \
    JARVIS_QA_NOTIFICATION_KIND="reminder" >/dev/null 2>"$tmp_dir/invalid-notification-kind.err"; then
    fail "live QA self-test expected invalid notification kind to fail"
  fi
  require_file_contains "live QA self-test invalid notification kind error" \
    "$tmp_dir/invalid-notification-kind.err" "JARVIS_QA_NOTIFICATION_KIND must be one of"

  if run_fixture_assertion "$tmp_dir/blank-notification-title.json" \
    JARVIS_QA_NOTIFICATION_TITLE="   " >/dev/null 2>"$tmp_dir/blank-notification-title.err"; then
    fail "live QA self-test expected blank notification title to fail"
  fi
  require_file_contains "live QA self-test blank notification title error" \
    "$tmp_dir/blank-notification-title.err" "JARVIS_QA_NOTIFICATION_TITLE"

  if run_fixture_assertion "$tmp_dir/blank-notification-body.json" \
    JARVIS_QA_NOTIFICATION_BODY="   " >/dev/null 2>"$tmp_dir/blank-notification-body.err"; then
    fail "live QA self-test expected blank notification body to fail"
  fi
  require_file_contains "live QA self-test blank notification body error" \
    "$tmp_dir/blank-notification-body.err" "JARVIS_QA_NOTIFICATION_BODY"

  if run_fixture_assertion "$tmp_dir/wrong-notification-thread.json" \
    JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER="jarvis.other" >/dev/null 2>"$tmp_dir/wrong-notification-thread.err"; then
    fail "live QA self-test expected wrong notification thread identifier to fail"
  fi
  require_file_contains "live QA self-test wrong notification thread error" \
    "$tmp_dir/wrong-notification-thread.err" "JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER must be jarvis.scheduler"

  if run_fixture_assertion "$tmp_dir/malformed-notification-timestamp.json" \
    JARVIS_QA_NOTIFICATION_OBSERVED_AT="2026-05-22T16:04:00-04:00" >/dev/null 2>"$tmp_dir/malformed-notification-timestamp.err"; then
    fail "live QA self-test expected malformed notification timestamp to fail"
  fi
  require_file_contains "live QA self-test malformed notification timestamp error" \
    "$tmp_dir/malformed-notification-timestamp.err" "JARVIS_QA_NOTIFICATION_OBSERVED_AT must be a UTC RFC3339 timestamp"

  if run_fixture_assertion "$tmp_dir/early-notification-timestamp.json" \
    JARVIS_QA_NOTIFICATION_OBSERVED_AT="2026-05-22T15:59:00Z" >/dev/null 2>"$tmp_dir/early-notification-timestamp.err"; then
    fail "live QA self-test expected notification timestamp before voice start to fail"
  fi
  require_file_contains "live QA self-test early notification timestamp error" \
    "$tmp_dir/early-notification-timestamp.err" "JARVIS_QA_NOTIFICATION_OBSERVED_AT must be greater than or equal to JARVIS_QA_VOICE_CHECK_STARTED_AT"

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
    JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE="Clean profile install observed in the controlled release QA lane." \
    JARVIS_QA_FINDER_LAUNCH_EVIDENCE_NOTE="Finder launch observed in the controlled release QA lane." \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the controlled release QA lane." \
    JARVIS_QA_NOTIFICATION_EVIDENCE_NOTE="Visible scheduler notification observed in the controlled release QA lane." \
    JARVIS_QA_NOTIFICATION_OBSERVED_AT="2026-05-22T16:04:00Z" \
    JARVIS_QA_RESTART_EVIDENCE_NOTE="Restart recovery observed in the controlled release QA lane." \
    JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE="Manual release QA surfaces observed in the controlled release QA lane." \
    JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected operator-authored self-test fixture reports to fail"
  fi

  if JARVIS_QA_INSTALLED_APP_PATH="$fixture_app" \
    JARVIS_QA_REPORT_PATH="$tmp_dir/blank-clean-profile-evidence.json" \
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
    JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE="   " \
    JARVIS_QA_FINDER_LAUNCH_EVIDENCE_NOTE="Finder launch observed in the controlled release QA lane." \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the controlled release QA lane." \
    JARVIS_QA_NOTIFICATION_EVIDENCE_NOTE="Visible scheduler notification observed in the controlled release QA lane." \
    JARVIS_QA_NOTIFICATION_OBSERVED_AT="2026-05-22T16:04:00Z" \
    JARVIS_QA_RESTART_EVIDENCE_NOTE="Restart recovery observed in the controlled release QA lane." \
    JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE="Manual release QA surfaces observed in the controlled release QA lane." \
    JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected blank non-voice owner evidence to fail"
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
    JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="   " \
    JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright status check." \
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
    JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the controlled release QA lane." \
    JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright status check." \
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
    JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
    JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright status check." \
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
    JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the controlled release QA lane." \
    JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright status check." \
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
    JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the controlled release QA lane." \
    JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright stats check." \
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
    JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the controlled release QA lane." \
    JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright status check." \
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
    JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the controlled release QA lane." \
    JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright status check." \
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
    JARVIS_QA_OWNER_NAME="Assemblywright QA Self-Test" \
    JARVIS_QA_DEVICE_LABEL="self-test Mac fixture" \
    JARVIS_QA_PROFILE_LABEL="self-test clean profile" \
    JARVIS_QA_VOICE_CHECK_STARTED_AT="2026-05-22T16:05:00Z" \
    JARVIS_QA_VOICE_CHECK_COMPLETED_AT="2026-05-22T16:00:00Z" \
    JARVIS_QA_MICROPHONE_EVIDENCE_NOTE="Observed microphone permission prompt in the controlled release QA lane." \
    JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE="Observed Speech permission prompt in the controlled release QA lane." \
    JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE="Observed transcript handoff reach the command path in the controlled release QA lane." \
    JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE="Observed speech output playback in the controlled release QA lane." \
    JARVIS_QA_VOICE_TEST_PHRASE="Assemblywright status check." \
    JARVIS_QA_OBSERVED_TRANSCRIPT="Assemblywright status check." \
    JARVIS_QA_EXPECTED_COMMAND_TEXT="status check" \
    JARVIS_QA_OBSERVED_COMMAND_TEXT="status check" \
    JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID="task:00000000-0000-4000-8000-000000000001" \
    JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL="self-test audio output" \
    JARVIS_QA_SELF_TEST_FIXTURE=true \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "live QA self-test expected timestamp order validation to fail"
  fi

  check_output="$("$0" --check)"
  case "$check_output" in
    *"./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env"* )
      ;;
    *)
      fail "live QA self-test expected --check output to include the template command"
      ;;
  esac
  case "$check_output" in
    *"set -a && source target/release-live-device-qa.env && set +a"* )
      ;;
    *)
      fail "live QA self-test expected --check output to include the source command"
      ;;
  esac
  case "$check_output" in
    *"./scripts/release-live-device-qa.sh --assert-complete"* )
      ;;
    *)
      fail "live QA self-test expected --check output to include the assertion command"
      ;;
  esac

  printf 'Assemblywright live-device QA self-test: ok\n'
  printf 'Proof boundary: fake app fixture validates assertion/report mechanics only; no live device validation was performed.\n'
  exit 0
fi

if [[ "$CHECK_ONLY" == true ]]; then
  cat <<'CHECKLIST'
Assemblywright live-device QA preflight: ok

Manual release checks still required before production-ready language:
- Install the signed, notarized package into /Applications on a clean Mac profile.
- Launch Assemblywright through Finder or LaunchServices, not only from Terminal.
- Confirm the app supervises the bundled core and command, audit, memory, scheduler,
  plugin, pause/resume, diagnostics, restart, and release-readiness surfaces work.
- Start voice capture and verify microphone and Speech permission prompts.
- Speak a command and verify transcript handoff reaches the same command path.
- Play speech output and verify live audio output on the device.
- Verify scheduler notification permission and at least one visible notification,
  then record its kind, title, body, thread identifier, and observed timestamp.
- After validating on the release machine, generate and source the fillable
  environment file, then assert it:
  ./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env
  set -a && source target/release-live-device-qa.env && set +a
  ./scripts/release-live-device-qa.sh --assert-complete
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
validate_installed_app_executable_binding

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
require_owner_evidence_note_env JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE
require_owner_evidence_note_env JARVIS_QA_FINDER_LAUNCH_EVIDENCE_NOTE
require_owner_evidence_note_env JARVIS_QA_MICROPHONE_EVIDENCE_NOTE
require_owner_evidence_note_env JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE
require_owner_evidence_note_env JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE
require_owner_evidence_note_env JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE
require_owner_evidence_note_env JARVIS_QA_NOTIFICATION_EVIDENCE_NOTE
require_utc_timestamp_env JARVIS_QA_NOTIFICATION_OBSERVED_AT
require_timestamp_order JARVIS_QA_VOICE_CHECK_STARTED_AT JARVIS_QA_NOTIFICATION_OBSERVED_AT
require_not_future_timestamp_env JARVIS_QA_NOTIFICATION_OBSERVED_AT
require_notification_kind_env JARVIS_QA_NOTIFICATION_KIND
require_non_empty_env JARVIS_QA_NOTIFICATION_TITLE
require_non_empty_env JARVIS_QA_NOTIFICATION_BODY
require_env_equals JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER "jarvis.scheduler"
require_owner_evidence_note_env JARVIS_QA_RESTART_EVIDENCE_NOTE
require_owner_evidence_note_env JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE
require_non_empty_env JARVIS_QA_VOICE_TEST_PHRASE
require_non_empty_env JARVIS_QA_OBSERVED_TRANSCRIPT
require_observed_transcript_matches_phrase
require_trimmed_env_match JARVIS_QA_EXPECTED_COMMAND_TEXT JARVIS_QA_OBSERVED_COMMAND_TEXT
require_non_empty_env JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID
require_command_result_evidence_id_env JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID
require_non_empty_env JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL
write_report

cat <<EOF
Assemblywright live-device QA assertion: complete
Installed app: $APP_PATH
Bundle: $APP_BUNDLE_ID $APP_SHORT_VERSION ($APP_BUILD_VERSION)
App executable SHA-256: $APP_EXECUTABLE_SHA256
Signed provenance: $SIGNED_PROVENANCE_PATH
Report: $REPORT_PATH
Proof boundary: owner-recorded clean-profile install, Finder launch,
microphone/Speech permission prompts, spoken transcript handoff into the command
path, live audio output, structured scheduler notification observation, restart,
and manual release QA flags bound at report generation to the signed candidate's
app executable bytes and code identity only; this is not installation provenance,
continuous filesystem integrity, App Store review, marketplace trust, malware
analysis, or OS-level sandbox/egress enforcement.
EOF
