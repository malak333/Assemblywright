#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="${JARVIS_EVIDENCE_VERSION:-0.1.4}"
DIST_DIR="${JARVIS_EVIDENCE_DIST_DIR:-$ROOT_DIR/target/distribution}"
APP_PATH="${JARVIS_EVIDENCE_APP_PATH:-$DIST_DIR/Jarvis.app}"
ZIP_PATH="${JARVIS_EVIDENCE_ZIP_PATH:-$DIST_DIR/Jarvis-$VERSION.zip}"
PKG_PATH="${JARVIS_EVIDENCE_PKG_PATH:-$DIST_DIR/Jarvis-$VERSION.pkg}"
LIVE_QA_REPORT="${JARVIS_EVIDENCE_LIVE_QA_REPORT:-${JARVIS_QA_REPORT_PATH:-$ROOT_DIR/target/release-live-device-qa-report.json}}"
PLUGIN_QA_REPORT="${JARVIS_EVIDENCE_PLUGIN_QA_REPORT:-${JARVIS_PLUGIN_QA_REPORT_PATH:-$ROOT_DIR/target/release-plugin-trust-qa-report.json}}"
OUTPUT_PATH="${JARVIS_EVIDENCE_OUTPUT_PATH:-$ROOT_DIR/target/release-evidence-bundle.json}"
VALIDATE_LOCAL_SIGNATURES="${JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES:-true}"
EXPECTED_BUNDLE_ID="${JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID:-com.nobiletechnology.jarvis}"
EXPECTED_VERSION="${JARVIS_EVIDENCE_EXPECTED_VERSION:-$VERSION}"
CHECK_ONLY=false
BUNDLE=false
SELF_TEST=false
WRITE_TEMPLATE=false
WRITE_TEMPLATE_PATH=""

usage() {
  cat <<'USAGE'
Usage: scripts/release-evidence-bundle.sh [--check|--bundle|--self-test|--write-template PATH]

Collect and validate the release evidence bundle required before any
production-ready claim for Jarvis.

--check validates repo-owned evidence-bundle prerequisites and prints the
external artifacts/reports required for a production release decision.

--bundle validates the expected signed distribution artifacts, live-device QA
report, plugin-trust QA report, and explicit owner evidence flags, then writes
a JSON bundle manifest.

--self-test creates fake artifacts/reports in a temporary directory and
exercises the bundle manifest mechanics without claiming production readiness.

--write-template PATH writes a sourceable shell env template containing every
JARVIS_EVIDENCE_* input required by --bundle. Edit the template only after the
external release checks are complete, then source it and rerun --bundle.

Required before --bundle:
  JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true
  JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true
  JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true
  JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true
  JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true
  JARVIS_EVIDENCE_REPORTS_ARCHIVED=true

Optional:
  JARVIS_EVIDENCE_VERSION             Defaults to 0.1.4
  JARVIS_EVIDENCE_DIST_DIR            Defaults to target/distribution
  JARVIS_EVIDENCE_APP_PATH            Defaults to target/distribution/Jarvis.app
  JARVIS_EVIDENCE_ZIP_PATH            Defaults to target/distribution/Jarvis-<version>.zip
  JARVIS_EVIDENCE_PKG_PATH            Defaults to target/distribution/Jarvis-<version>.pkg
  JARVIS_EVIDENCE_LIVE_QA_REPORT      Defaults to JARVIS_QA_REPORT_PATH or target/release-live-device-qa-report.json
  JARVIS_EVIDENCE_PLUGIN_QA_REPORT    Defaults to JARVIS_PLUGIN_QA_REPORT_PATH or target/release-plugin-trust-qa-report.json
  JARVIS_EVIDENCE_OUTPUT_PATH         Defaults to target/release-evidence-bundle.json
  JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES
                                      Defaults to true. Set to false only for
                                      fake self-test fixtures.
  JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID  Defaults to com.nobiletechnology.jarvis
  JARVIS_EVIDENCE_EXPECTED_VERSION    Defaults to JARVIS_EVIDENCE_VERSION

This script validates evidence capture only. It does not sign, notarize,
install, launch Finder, run live microphone/audio checks, run malware scans, or
enforce an OS sandbox.
USAGE
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
}

require_true() {
  local name="$1"
  local value="${!name:-}"
  [[ "$value" == "true" ]] || fail "$name must be set to true after external validation"
}

require_file() {
  local label="$1"
  local path="$2"
  [[ -f "$path" ]] || fail "missing $label: $path"
}

require_file_contains() {
  local label="$1"
  local path="$2"
  local expected="$3"
  require_file "$label" "$path"
  if ! grep -F "$expected" "$path" >/dev/null 2>&1; then
    fail "$label does not include required text: $expected"
  fi
}

require_dir() {
  local label="$1"
  local path="$2"
  [[ -d "$path" ]] || fail "missing $label: $path"
}

require_artifact_validation_mode() {
  case "$VALIDATE_LOCAL_SIGNATURES" in
    true|false)
      ;;
    *)
      fail "JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES must be true or false"
      ;;
  esac
}

require_production_signature_validation() {
  if [[ "$VALIDATE_LOCAL_SIGNATURES" != true && "${JARVIS_EVIDENCE_SELF_TEST_MODE:-}" != true ]]; then
    fail "JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false is only allowed during --self-test"
  fi
}

require_json_contains() {
  local label="$1"
  local path="$2"
  local expected="$3"
  require_file "$label" "$path"
  python3 -m json.tool "$path" >/dev/null
  if ! grep -F "$expected" "$path" >/dev/null 2>&1; then
    fail "$label does not include required evidence text: $expected"
  fi
}

require_json_bool_true() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$label" <<'PY'
import json
import sys

path, dotted_key, label = sys.argv[1:4]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence flag: {dotted_key}")
    cursor = cursor[segment]

if cursor is not True:
    raise SystemExit(f"{label} required evidence flag is not true: {dotted_key}")
PY
}

require_json_bool_false() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$label" <<'PY'
import json
import sys

path, dotted_key, label = sys.argv[1:4]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence flag: {dotted_key}")
    cursor = cursor[segment]

if cursor is not False:
    raise SystemExit(f"{label} required evidence flag is not false: {dotted_key}")
PY
}

require_json_number_equals() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  local expected="$4"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$expected" "$label" <<'PY'
import json
import sys

path, dotted_key, expected, label = sys.argv[1:5]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
    cursor = cursor[segment]

try:
    expected_number = int(expected)
except ValueError as exc:
    raise SystemExit(f"invalid expected number: {expected}") from exc

if cursor != expected_number:
    raise SystemExit(
        f"{label} evidence field {dotted_key} mismatch: expected {expected_number!r}, got {cursor!r}"
    )
PY
}

require_json_string_equals() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  local expected="$4"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$expected" "$label" <<'PY'
import json
import sys

path, dotted_key, expected, label = sys.argv[1:5]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
    cursor = cursor[segment]

if cursor != expected:
    raise SystemExit(
        f"{label} evidence field {dotted_key} mismatch: expected {expected!r}, got {cursor!r}"
    )
PY
}

require_json_string_fields_equal() {
  local label="$1"
  local path="$2"
  local expected_key="$3"
  local actual_key="$4"
  require_file "$label" "$path"
  python3 - "$path" "$expected_key" "$actual_key" "$label" <<'PY'
import json
import sys

path, expected_key, actual_key, label = sys.argv[1:5]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

def get(dotted_key):
    cursor = data
    for segment in dotted_key.split("."):
        if not isinstance(cursor, dict) or segment not in cursor:
            raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
        cursor = cursor[segment]
    if not isinstance(cursor, str) or not cursor.strip():
        raise SystemExit(f"{label} evidence field must be a non-empty string: {dotted_key}")
    return cursor.strip()

expected = get(expected_key)
actual = get(actual_key)
if expected != actual:
    raise SystemExit(
        f"{label} evidence field {actual_key} mismatch: expected {expected_key} value {expected!r}, got {actual!r}"
    )
PY
}

require_json_nonempty_string() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$label" <<'PY'
import json
import sys

path, dotted_key, label = sys.argv[1:4]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
    cursor = cursor[segment]

if not isinstance(cursor, str) or not cursor.strip():
    raise SystemExit(f"{label} required evidence field is blank: {dotted_key}")
PY
}

require_json_utc_timestamp() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$label" <<'PY'
from datetime import datetime
import json
import sys

path, dotted_key, label = sys.argv[1:4]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
    cursor = cursor[segment]

if not isinstance(cursor, str) or not cursor.endswith("Z"):
    raise SystemExit(f"{label} required evidence field must be a UTC timestamp ending in Z: {dotted_key}")
try:
    datetime.fromisoformat(cursor.replace("Z", "+00:00"))
except ValueError as exc:
    raise SystemExit(f"{label} required evidence field must be a UTC RFC3339 timestamp: {dotted_key}") from exc
PY
}

require_json_timestamp_order() {
  local label="$1"
  local path="$2"
  local start_key="$3"
  local completed_key="$4"
  require_file "$label" "$path"
  python3 - "$path" "$start_key" "$completed_key" "$label" <<'PY'
from datetime import datetime
import json
import sys

path, start_key, completed_key, label = sys.argv[1:5]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

def get_timestamp(dotted_key):
    cursor = data
    for segment in dotted_key.split("."):
        if not isinstance(cursor, dict) or segment not in cursor:
            raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
        cursor = cursor[segment]
    if not isinstance(cursor, str) or not cursor.endswith("Z"):
        raise SystemExit(f"{label} required evidence field must be a UTC timestamp ending in Z: {dotted_key}")
    try:
        return datetime.fromisoformat(cursor.replace("Z", "+00:00"))
    except ValueError as exc:
        raise SystemExit(f"{label} required evidence field must be a UTC RFC3339 timestamp: {dotted_key}") from exc

started = get_timestamp(start_key)
completed = get_timestamp(completed_key)
if completed < started:
    raise SystemExit(f"{label} {completed_key} must be greater than or equal to {start_key}")
PY
}

file_sha256() {
  local path="$1"
  shasum -a 256 "$path" | awk '{print $1}'
}

validate_zip_payload() {
  python3 - "$ZIP_PATH" <<'PY'
import sys
import zipfile

zip_path = sys.argv[1]
required_suffixes = (
    "Jarvis.app/Contents/MacOS/JarvisMacApp",
    "Jarvis.app/Contents/Resources/bin/jarvis-cli",
    "Jarvis.app/Contents/Info.plist",
)

with zipfile.ZipFile(zip_path) as archive:
    names = archive.namelist()

missing = [
    suffix
    for suffix in required_suffixes
    if not any(name.endswith(suffix) for name in names)
]
if missing:
    raise SystemExit(f"zip payload missing required app entries: {', '.join(missing)}")
PY
}

validate_local_distribution_evidence() {
  if [[ "$VALIDATE_LOCAL_SIGNATURES" != true ]]; then
    printf 'warning: local signature/stapling validation skipped by JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false\n' >&2
    return
  fi

  require_command codesign
  require_command pkgutil
  require_command xcrun
  require_command python3
  codesign --verify --deep --strict --verbose=2 "$APP_PATH" >/dev/null
  xcrun stapler validate "$APP_PATH" >/dev/null
  pkgutil --check-signature "$PKG_PATH" >/dev/null
  xcrun stapler validate "$PKG_PATH" >/dev/null
  validate_zip_payload
}

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

write_bundle() {
  local generated_at
  local escaped_app
  local escaped_zip
  local escaped_pkg
  local escaped_live
  local escaped_plugin
  local zip_sha
  local pkg_sha
  local live_sha
  local plugin_sha
  local escaped_boundary
  local local_signature_validation
  require_command shasum
  require_command python3
  generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  escaped_app="$(json_escape "$APP_PATH")"
  escaped_zip="$(json_escape "$ZIP_PATH")"
  escaped_pkg="$(json_escape "$PKG_PATH")"
  escaped_live="$(json_escape "$LIVE_QA_REPORT")"
  escaped_plugin="$(json_escape "$PLUGIN_QA_REPORT")"
  zip_sha="$(file_sha256 "$ZIP_PATH")"
  pkg_sha="$(file_sha256 "$PKG_PATH")"
  live_sha="$(file_sha256 "$LIVE_QA_REPORT")"
  plugin_sha="$(file_sha256 "$PLUGIN_QA_REPORT")"
  escaped_boundary="$(json_escape "Evidence bundle manifest only; records artifact paths, local signature/stapling validation status, owner-recorded signing/notarization validation flags, and QA reports.")"
  local_signature_validation="$VALIDATE_LOCAL_SIGNATURES"

  mkdir -p "$(dirname "$OUTPUT_PATH")"
  cat >"$OUTPUT_PATH" <<EOF
{
  "generated_at": "$generated_at",
  "version": "$VERSION",
  "artifacts": {
    "app_path": "$escaped_app",
    "zip_path": "$escaped_zip",
    "pkg_path": "$escaped_pkg",
    "zip_sha256": "$zip_sha",
    "pkg_sha256": "$pkg_sha"
  },
  "reports": {
    "live_device_qa_report": "$escaped_live",
    "plugin_trust_qa_report": "$escaped_plugin",
    "live_device_qa_sha256": "$live_sha",
    "plugin_trust_qa_sha256": "$plugin_sha"
  },
  "validation_flags": {
    "signed_distribution": true,
    "notarization": true,
    "clean_profile": true,
    "live_device_qa": true,
    "plugin_trust_qa": true,
    "reports_archived": true,
    "local_signature_validation": $local_signature_validation
  },
  "proof_boundary": "$escaped_boundary"
}
EOF
  python3 -m json.tool "$OUTPUT_PATH" >/dev/null
}

write_env_template() {
  local template_path="$1"
  mkdir -p "$(dirname "$template_path")"
  cat >"$template_path" <<EOF
# Jarvis final release evidence bundle template.
# Edit this file only after the signed/notarized distribution artifacts,
# live-device QA report, plugin-trust QA report, and archive locations have
# been validated for the release candidate. Keep every validation flag false
# until the matching external check has actually completed.
#
# Usage:
#   set -a
#   source "$template_path"
#   set +a
#   ./scripts/release-evidence-bundle.sh --bundle

JARVIS_EVIDENCE_VERSION="$VERSION"
JARVIS_EVIDENCE_DIST_DIR="$DIST_DIR"
JARVIS_EVIDENCE_APP_PATH="$APP_PATH"
JARVIS_EVIDENCE_ZIP_PATH="$ZIP_PATH"
JARVIS_EVIDENCE_PKG_PATH="$PKG_PATH"
JARVIS_EVIDENCE_LIVE_QA_REPORT="$LIVE_QA_REPORT"
JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$PLUGIN_QA_REPORT"
JARVIS_EVIDENCE_OUTPUT_PATH="$OUTPUT_PATH"
JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=true
JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID="$EXPECTED_BUNDLE_ID"
JARVIS_EVIDENCE_EXPECTED_VERSION="$EXPECTED_VERSION"

JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=false
JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=false
JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=false
JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=false
JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=false
JARVIS_EVIDENCE_REPORTS_ARCHIVED=false
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      CHECK_ONLY=true
      shift
      ;;
    --bundle)
      BUNDLE=true
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

if { [[ "$CHECK_ONLY" == true ]] && { [[ "$BUNDLE" == true ]] || [[ "$SELF_TEST" == true ]] || [[ "$WRITE_TEMPLATE" == true ]]; }; } ||
  { [[ "$BUNDLE" == true ]] && { [[ "$SELF_TEST" == true ]] || [[ "$WRITE_TEMPLATE" == true ]]; }; } ||
  { [[ "$SELF_TEST" == true ]] && [[ "$WRITE_TEMPLATE" == true ]]; }; then
  fail "--check, --bundle, --self-test, and --write-template are mutually exclusive"
fi

if [[ "$CHECK_ONLY" != true && "$BUNDLE" != true && "$SELF_TEST" != true && "$WRITE_TEMPLATE" != true ]]; then
  usage
  exit 0
fi

require_command grep
require_command python3
require_artifact_validation_mode

if [[ "$WRITE_TEMPLATE" == true ]]; then
  write_env_template "$WRITE_TEMPLATE_PATH"
  printf 'Jarvis release evidence bundle env template written: %s\n' "$WRITE_TEMPLATE_PATH"
  printf 'Proof boundary: template generation only; no release evidence was validated or created.\n'
  exit 0
fi

if [[ "$SELF_TEST" == true ]]; then
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-release-evidence-self-test.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT
  mkdir -p "$tmp_dir/dist/Jarvis.app/Contents/MacOS" "$tmp_dir/dist/Jarvis.app/Contents/Resources/bin"
  touch "$tmp_dir/dist/Jarvis.app/Contents/MacOS/JarvisMacApp"
  touch "$tmp_dir/dist/Jarvis.app/Contents/Resources/bin/jarvis-cli"
  touch "$tmp_dir/dist/Jarvis-0.1.4.zip"
  touch "$tmp_dir/dist/Jarvis-0.1.4.pkg"
  cat >"$tmp_dir/live.json" <<'JSON'
{
  "schema_version": 1,
  "evidence_type": "owner_recorded_live_device_qa",
  "self_test_fixture": false,
  "generated_at": "2026-05-22T16:06:00Z",
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
    "owner_name": "Jarvis QA Self-Test",
    "device_label": "self-test Mac fixture",
    "profile_label": "self-test clean profile",
    "voice_check_started_at": "2026-05-22T16:00:00Z",
    "voice_check_completed_at": "2026-05-22T16:05:00Z",
    "microphone_evidence_note": "Observed microphone permission prompt in the fake fixture.",
    "speech_permission_evidence_note": "Observed Speech permission prompt in the fake fixture.",
    "transcript_handoff_evidence_note": "Observed transcript handoff reach the command path in the fake fixture.",
    "audio_output_evidence_note": "Observed speech output playback in the fake fixture."
  },
  "voice_command_observation": {
    "test_phrase": "Jarvis status check.",
    "observed_transcript": "Jarvis status check.",
    "expected_command_text": "status check",
    "observed_command_text": "status check",
    "command_result_evidence_id": "self-test-task-id",
    "audio_output_device_label": "self-test audio output"
  },
  "app_bundle": {
    "bundle_identifier": "com.nobiletechnology.jarvis",
    "short_version": "0.1.4",
    "build_version": "0.1.4",
    "microphone_usage_description": "self-test fixture",
    "speech_recognition_usage_description": "self-test fixture"
  },
  "proof_boundary": "self-test fixture"
}
JSON
  cat >"$tmp_dir/plugin.json" <<'JSON'
{
  "generated_at": "2026-05-22T16:30:00Z",
  "validation_flags": {
    "marketplace_review": true,
    "malware_scan": true,
    "os_sandbox": true,
    "egress_enforcement": true,
    "signed_publisher_policy": true,
    "manual_trust_review": true
  },
  "owner_recorded_plugin_trust_evidence": {
    "owner_name": "Jarvis Plugin QA Self-Test",
    "review_started_at": "2026-05-22T16:10:00Z",
    "review_completed_at": "2026-05-22T16:20:00Z",
    "marketplace_evidence_note": "Marketplace review fixture was observed.",
    "malware_scan_evidence_note": "Malware scan fixture was observed.",
    "os_sandbox_evidence_note": "OS sandbox fixture was observed.",
    "egress_evidence_note": "Egress fixture was observed.",
    "signed_publisher_evidence_note": "Signed publisher policy fixture was observed.",
    "manual_review_evidence_note": "Manual trust review fixture was observed."
  },
  "proof_boundary": "self-test fixture"
}
JSON

  "$0" --write-template "$tmp_dir/release-evidence-bundle.env" >/dev/null
  require_file "release evidence template" "$tmp_dir/release-evidence-bundle.env"
  for field in \
    JARVIS_EVIDENCE_VERSION \
    JARVIS_EVIDENCE_APP_PATH \
    JARVIS_EVIDENCE_ZIP_PATH \
    JARVIS_EVIDENCE_PKG_PATH \
    JARVIS_EVIDENCE_LIVE_QA_REPORT \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT \
    JARVIS_EVIDENCE_OUTPUT_PATH \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES \
    JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID \
    JARVIS_EVIDENCE_EXPECTED_VERSION; do
    require_file_contains "release evidence template" "$tmp_dir/release-evidence-bundle.env" "$field="
  done
  for flag in \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED; do
    require_file_contains "release evidence template" "$tmp_dir/release-evidence-bundle.env" "$flag=false"
    if grep -F "$flag=true" "$tmp_dir/release-evidence-bundle.env" >/dev/null 2>&1; then
      fail "release evidence template must not default $flag to true"
    fi
  done

  JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    JARVIS_EVIDENCE_SELF_TEST_MODE=true \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"reports_archived": true'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"local_signature_validation": false'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"zip_sha256"'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"live_device_qa_sha256"'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"plugin_trust_qa_report"'

  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/forbidden-bundle.json" \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected production bundle to reject disabled local signature validation"
  fi

  cat >"$tmp_dir/incomplete-live.json" <<'JSON'
{
  "validation_flags": {
    "manual_release_qa": true
  },
  "proof_boundary": "incomplete self-test fixture"
}
JSON
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/incomplete-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/incomplete-live-bundle.json" \
    JARVIS_EVIDENCE_SELF_TEST_MODE=true \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected incomplete live-device report to be rejected"
  fi

  python3 - "$tmp_dir/live.json" "$tmp_dir/missing-observation-live.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["owner_recorded_live_voice_evidence"]["audio_output_evidence_note"] = ""
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/missing-observation-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/missing-observation-bundle.json" \
    JARVIS_EVIDENCE_SELF_TEST_MODE=true \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected blank live voice observation to be rejected"
  fi

  python3 - "$tmp_dir/live.json" "$tmp_dir/mismatched-command-live.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["voice_command_observation"]["observed_command_text"] = "different command"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/mismatched-command-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/mismatched-command-bundle.json" \
    JARVIS_EVIDENCE_SELF_TEST_MODE=true \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected mismatched live command observation to be rejected"
  fi

  python3 - "$tmp_dir/live.json" "$tmp_dir/pregenerated-live.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["generated_at"] = "2026-05-22T16:04:00Z"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/pregenerated-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/pregenerated-live-bundle.json" \
    JARVIS_EVIDENCE_SELF_TEST_MODE=true \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected live report generated before completion to be rejected"
  fi

  cat >"$tmp_dir/incomplete-plugin.json" <<'JSON'
{
  "validation_flags": {
    "manual_trust_review": true
  },
  "proof_boundary": "incomplete self-test fixture"
}
JSON
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/incomplete-plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/incomplete-plugin-bundle.json" \
    JARVIS_EVIDENCE_SELF_TEST_MODE=true \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected incomplete plugin-trust report to be rejected"
  fi

  python3 - "$tmp_dir/plugin.json" "$tmp_dir/missing-observation-plugin.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["owner_recorded_plugin_trust_evidence"]["egress_evidence_note"] = ""
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/missing-observation-plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/missing-plugin-observation-bundle.json" \
    JARVIS_EVIDENCE_SELF_TEST_MODE=true \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected blank plugin trust observation to be rejected"
  fi

  python3 - "$tmp_dir/plugin.json" "$tmp_dir/non-utc-plugin.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["owner_recorded_plugin_trust_evidence"]["review_started_at"] = "2026-05-22T16:10:00-04:00"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/non-utc-plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/non-utc-plugin-bundle.json" \
    JARVIS_EVIDENCE_SELF_TEST_MODE=true \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected non-UTC plugin trust timestamp to be rejected"
  fi

  python3 - "$tmp_dir/plugin.json" "$tmp_dir/reversed-plugin.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["owner_recorded_plugin_trust_evidence"]["review_started_at"] = "2026-05-22T16:20:00Z"
data["owner_recorded_plugin_trust_evidence"]["review_completed_at"] = "2026-05-22T16:10:00Z"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/reversed-plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/reversed-plugin-bundle.json" \
    JARVIS_EVIDENCE_SELF_TEST_MODE=true \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected reversed plugin trust timestamps to be rejected"
  fi

  python3 - "$tmp_dir/plugin.json" "$tmp_dir/pregenerated-plugin.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["generated_at"] = "2026-05-22T16:15:00Z"
data["owner_recorded_plugin_trust_evidence"]["review_completed_at"] = "2026-05-22T16:20:00Z"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/pregenerated-plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/pregenerated-plugin-bundle.json" \
    JARVIS_EVIDENCE_SELF_TEST_MODE=true \
    JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true \
    JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true \
    JARVIS_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected plugin report generated before completion to be rejected"
  fi

  printf 'Jarvis release evidence bundle self-test: ok\n'
  printf 'Proof boundary: fake artifacts and reports validate bundle mechanics only; no production evidence was created.\n'
  exit 0
fi

if [[ "$CHECK_ONLY" == true ]]; then
  require_file "package-distribution script" "$ROOT_DIR/scripts/package-distribution.sh"
  require_file "live-device QA script" "$ROOT_DIR/scripts/release-live-device-qa.sh"
  require_file "plugin trust QA script" "$ROOT_DIR/scripts/release-plugin-trust-qa.sh"
  require_file "release checklist" "$ROOT_DIR/docs/release-checklist.md"
  printf 'Jarvis release evidence bundle preflight: ok\n\n'
  cat <<'CHECKLIST'
Required before --bundle:
- App zip artifact path exists, with Developer ID signing/notarization validated separately.
- /Applications installer package path exists, with Developer ID signing/notarization validated separately.
- Clean-profile install, Finder launch, live microphone/Speech, audio output,
  notification, restart, and manual release QA report exists.
- Marketplace review, malware scan, signed publisher policy, OS sandbox, and
  host-level egress evidence report exists.
- Owner sets every JARVIS_EVIDENCE_* validation flag to true.
- The bundle command can locally verify app signing, app stapling, installer
  signature, installer stapling, and the app zip payload.

Proof boundary: preflight and runbook only; no production evidence was created.
CHECKLIST
  exit 0
fi

require_dir "app bundle path" "$APP_PATH"
require_file "app zip path" "$ZIP_PATH"
require_file "installer package path" "$PKG_PATH"
require_production_signature_validation
validate_local_distribution_evidence
for flag in clean_profile finder_launch microphone speech_permission transcript_handoff audio_output notification restart manual_release_qa; do
  require_json_bool_true "live-device QA report" "$LIVE_QA_REPORT" "validation_flags.$flag"
done
for flag in microphone_permission_prompt speech_permission_prompt spoken_transcript_handoff same_command_path speech_output_playback; do
  require_json_bool_true "live-device QA report" "$LIVE_QA_REPORT" "voice_loop.$flag"
done
require_json_number_equals "live-device QA report" "$LIVE_QA_REPORT" "schema_version" "1"
require_json_string_equals "live-device QA report" "$LIVE_QA_REPORT" "evidence_type" "owner_recorded_live_device_qa"
require_json_bool_false "live-device QA report" "$LIVE_QA_REPORT" "self_test_fixture"
for field in owner_name device_label profile_label voice_check_started_at voice_check_completed_at microphone_evidence_note speech_permission_evidence_note transcript_handoff_evidence_note audio_output_evidence_note; do
  require_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.$field"
done
require_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_started_at"
require_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_completed_at"
require_json_timestamp_order "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_started_at" "owner_recorded_live_voice_evidence.voice_check_completed_at"
require_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "generated_at"
require_json_timestamp_order "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_completed_at" "generated_at"
for field in test_phrase observed_transcript expected_command_text observed_command_text command_result_evidence_id audio_output_device_label; do
  require_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "voice_command_observation.$field"
done
require_json_string_fields_equal "live-device QA report" "$LIVE_QA_REPORT" "voice_command_observation.expected_command_text" "voice_command_observation.observed_command_text"
require_json_string_equals "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.bundle_identifier" "$EXPECTED_BUNDLE_ID"
require_json_string_equals "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.short_version" "$EXPECTED_VERSION"
require_json_string_equals "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.build_version" "$EXPECTED_VERSION"
require_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.microphone_usage_description"
require_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.speech_recognition_usage_description"
for flag in marketplace_review malware_scan os_sandbox egress_enforcement signed_publisher_policy manual_trust_review; do
  require_json_bool_true "plugin trust QA report" "$PLUGIN_QA_REPORT" "validation_flags.$flag"
done
for field in owner_name review_started_at review_completed_at marketplace_evidence_note malware_scan_evidence_note os_sandbox_evidence_note egress_evidence_note signed_publisher_evidence_note manual_review_evidence_note; do
  require_json_nonempty_string "plugin trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.$field"
done
require_json_utc_timestamp "plugin trust QA report" "$PLUGIN_QA_REPORT" "generated_at"
require_json_utc_timestamp "plugin trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.review_started_at"
require_json_utc_timestamp "plugin trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.review_completed_at"
require_json_timestamp_order "plugin trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.review_started_at" "owner_recorded_plugin_trust_evidence.review_completed_at"
require_json_timestamp_order "plugin trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.review_completed_at" "generated_at"
require_true JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED
require_true JARVIS_EVIDENCE_NOTARIZATION_VALIDATED
require_true JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED
require_true JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED
require_true JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED
require_true JARVIS_EVIDENCE_REPORTS_ARCHIVED
write_bundle

cat <<EOF
Jarvis release evidence bundle: complete
Bundle: $OUTPUT_PATH
Proof boundary: evidence manifest only; production readiness still depends on
the external evidence referenced by the artifact paths, QA reports, and owner
validation flags.
EOF
