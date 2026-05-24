#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="${JARVIS_EVIDENCE_VERSION:-$("$ROOT_DIR/scripts/release-version.sh")}"
DIST_DIR="${JARVIS_EVIDENCE_DIST_DIR:-$ROOT_DIR/target/distribution}"
APP_PATH="${JARVIS_EVIDENCE_APP_PATH:-$DIST_DIR/Jarvis.app}"
ZIP_PATH="${JARVIS_EVIDENCE_ZIP_PATH:-$DIST_DIR/Jarvis-$VERSION.zip}"
PKG_PATH="${JARVIS_EVIDENCE_PKG_PATH:-$DIST_DIR/Jarvis-$VERSION.pkg}"
SIGNED_PROVENANCE_REPORT="${JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT:-$DIST_DIR/Jarvis-$VERSION-signed-provenance.json}"
LIVE_QA_REPORT="${JARVIS_EVIDENCE_LIVE_QA_REPORT:-${JARVIS_QA_REPORT_PATH:-$ROOT_DIR/target/release-live-device-qa-report.json}}"
PLUGIN_QA_REPORT="${JARVIS_EVIDENCE_PLUGIN_QA_REPORT:-${JARVIS_PLUGIN_QA_REPORT_PATH:-$ROOT_DIR/target/release-plugin-trust-qa-report.json}}"
BUNDLE_PATH="${JARVIS_EVIDENCE_OUTPUT_PATH:-$ROOT_DIR/target/release-evidence-bundle.json}"
EXPECTED_BUNDLE_ID="${JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID:-com.nobiletechnology.jarvis}"
EXPECTED_VERSION="${JARVIS_EVIDENCE_EXPECTED_VERSION:-$VERSION}"

CHECK_ONLY=false
ASSERT_COMPLETE=false
SELF_TEST=false
MISSING_ITEMS=()
SATISFIED_ITEMS=()

usage() {
  cat <<'USAGE'
Usage: scripts/release-evidence-doctor.sh [--check|--assert-complete|--self-test]

Inspect the standard Jarvis release evidence paths and report which production
evidence gates are present or missing.

--check prints a non-failing status summary. Missing external/manual evidence is
expected before a production release candidate is fully validated.

--assert-complete fails unless all expected artifact paths, QA reports, and
final evidence bundle flags are present and valid. Path presence does not prove
Developer ID signing, notarization, or stapling by itself.

--self-test creates fake artifacts and reports in a temporary directory and
exercises the status logic without claiming production readiness.

Optional paths match scripts/release-evidence-bundle.sh:
  JARVIS_EVIDENCE_VERSION
  JARVIS_EVIDENCE_DIST_DIR
  JARVIS_EVIDENCE_APP_PATH
  JARVIS_EVIDENCE_ZIP_PATH
  JARVIS_EVIDENCE_PKG_PATH
  JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT
  JARVIS_EVIDENCE_LIVE_QA_REPORT      Defaults to JARVIS_QA_REPORT_PATH or target/release-live-device-qa-report.json
  JARVIS_EVIDENCE_PLUGIN_QA_REPORT    Defaults to JARVIS_PLUGIN_QA_REPORT_PATH or target/release-plugin-trust-qa-report.json
  JARVIS_EVIDENCE_OUTPUT_PATH
  JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID
  JARVIS_EVIDENCE_EXPECTED_VERSION

Proof boundary: this script inspects evidence files only. It does not sign,
notarize, install, launch Finder, validate live microphone/Speech/audio, run a
malware scanner, operate a marketplace, or enforce an OS sandbox.
USAGE
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

record_satisfied() {
  SATISFIED_ITEMS+=("$1")
}

record_missing() {
  MISSING_ITEMS+=("$1")
}

print_next_steps() {
  cat <<'STEPS'
Recommended next evidence commands:
  signing: JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh
  live-device template: ./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env
  live-device assertion: set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete
  plugin-trust template: ./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env
  plugin-trust assertion: set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete
  final bundle template: ./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env
  final bundle: set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle
STEPS
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
}

json_bool_true() {
  local path="$1"
  local dotted_key="$2"
  python3 - "$path" "$dotted_key" <<'PY'
import json
import sys

path, dotted_key = sys.argv[1:3]
try:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
except Exception:
    raise SystemExit(1)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(1)
    cursor = cursor[segment]

raise SystemExit(0 if cursor is True else 1)
PY
}

json_bool_false() {
  local path="$1"
  local dotted_key="$2"
  python3 - "$path" "$dotted_key" <<'PY'
import json
import sys

path, dotted_key = sys.argv[1:3]
try:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
except Exception:
    raise SystemExit(1)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(1)
    cursor = cursor[segment]

raise SystemExit(0 if cursor is False else 1)
PY
}

json_number_equals() {
  local path="$1"
  local dotted_key="$2"
  local expected="$3"
  python3 - "$path" "$dotted_key" "$expected" <<'PY'
import json
import sys

path, dotted_key, expected = sys.argv[1:4]
try:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
except Exception:
    raise SystemExit(1)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(1)
    cursor = cursor[segment]

raise SystemExit(0 if cursor == int(expected) else 1)
PY
}

json_string_equals() {
  local path="$1"
  local dotted_key="$2"
  local expected="$3"
  python3 - "$path" "$dotted_key" "$expected" <<'PY'
import json
import sys

path, dotted_key, expected = sys.argv[1:4]
try:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
except Exception:
    raise SystemExit(1)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(1)
    cursor = cursor[segment]

raise SystemExit(0 if cursor == expected else 1)
PY
}

json_string_fields_equal() {
  local path="$1"
  local expected_key="$2"
  local actual_key="$3"
  python3 - "$path" "$expected_key" "$actual_key" <<'PY'
import json
import sys

path, expected_key, actual_key = sys.argv[1:4]
try:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
except Exception:
    raise SystemExit(1)

def get(dotted_key):
    cursor = data
    for segment in dotted_key.split("."):
        if not isinstance(cursor, dict) or segment not in cursor:
            raise SystemExit(1)
        cursor = cursor[segment]
    if not isinstance(cursor, str) or not cursor.strip():
        raise SystemExit(1)
    return cursor.strip()

raise SystemExit(0 if get(expected_key) == get(actual_key) else 1)
PY
}

json_nonempty_string() {
  local path="$1"
  local dotted_key="$2"
  python3 - "$path" "$dotted_key" <<'PY'
import json
import sys

path, dotted_key = sys.argv[1:3]
try:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
except Exception:
    raise SystemExit(1)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(1)
    cursor = cursor[segment]

raise SystemExit(0 if isinstance(cursor, str) and bool(cursor.strip()) else 1)
PY
}

json_sha256_string() {
  local path="$1"
  local dotted_key="$2"
  python3 - "$path" "$dotted_key" <<'PY'
import json
import re
import sys

path, dotted_key = sys.argv[1:3]
try:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
except Exception:
    raise SystemExit(1)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(1)
    cursor = cursor[segment]

raise SystemExit(0 if isinstance(cursor, str) and re.fullmatch(r"[0-9a-f]{64}", cursor) else 1)
PY
}

json_utc_timestamp() {
  local path="$1"
  local dotted_key="$2"
  python3 - "$path" "$dotted_key" <<'PY'
from datetime import datetime
import json
import sys

path, dotted_key = sys.argv[1:3]
try:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
except Exception:
    raise SystemExit(1)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(1)
    cursor = cursor[segment]

if not isinstance(cursor, str) or not cursor.endswith("Z"):
    raise SystemExit(1)
try:
    datetime.fromisoformat(cursor.replace("Z", "+00:00"))
except ValueError:
    raise SystemExit(1)
raise SystemExit(0)
PY
}

json_timestamp_order() {
  local path="$1"
  local start_key="$2"
  local completed_key="$3"
  python3 - "$path" "$start_key" "$completed_key" <<'PY'
from datetime import datetime
import json
import sys

path, start_key, completed_key = sys.argv[1:4]
try:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
except Exception:
    raise SystemExit(1)

def get_timestamp(dotted_key):
    cursor = data
    for segment in dotted_key.split("."):
        if not isinstance(cursor, dict) or segment not in cursor:
            raise SystemExit(1)
        cursor = cursor[segment]
    if not isinstance(cursor, str) or not cursor.endswith("Z"):
        raise SystemExit(1)
    try:
        return datetime.fromisoformat(cursor.replace("Z", "+00:00"))
    except ValueError:
        raise SystemExit(1)

raise SystemExit(0 if get_timestamp(completed_key) >= get_timestamp(start_key) else 1)
PY
}

valid_json_file() {
  local path="$1"
  [[ -f "$path" ]] && python3 -m json.tool "$path" >/dev/null 2>&1
}

check_path() {
  local label="$1"
  local path="$2"
  local kind="$3"

  case "$kind" in
    file)
      [[ -f "$path" ]] && record_satisfied "$label: $path" || record_missing "$label missing: $path"
      ;;
    dir)
      [[ -d "$path" ]] && record_satisfied "$label: $path" || record_missing "$label missing: $path"
      ;;
    executable)
      [[ -x "$path" ]] && record_satisfied "$label: $path" || record_missing "$label missing or not executable: $path"
      ;;
    *)
      fail "unknown path kind: $kind"
      ;;
  esac
}

check_json_flag() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"

  if json_bool_true "$path" "$dotted_key"; then
    record_satisfied "$label: $dotted_key=true"
  else
    record_missing "$label missing true flag: $dotted_key in $path"
  fi
}

check_json_false_flag() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"

  if json_bool_false "$path" "$dotted_key"; then
    record_satisfied "$label: $dotted_key=false"
  else
    record_missing "$label missing false flag: $dotted_key in $path"
  fi
}

check_json_number() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  local expected="$4"

  if json_number_equals "$path" "$dotted_key" "$expected"; then
    record_satisfied "$label: $dotted_key=$expected"
  else
    record_missing "$label mismatch or missing: $dotted_key expected $expected in $path"
  fi
}

check_json_string() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  local expected="$4"

  if json_string_equals "$path" "$dotted_key" "$expected"; then
    record_satisfied "$label: $dotted_key=$expected"
  else
    record_missing "$label mismatch or missing: $dotted_key expected $expected in $path"
  fi
}

check_json_string_fields_equal() {
  local label="$1"
  local path="$2"
  local expected_key="$3"
  local actual_key="$4"

  if json_string_fields_equal "$path" "$expected_key" "$actual_key"; then
    record_satisfied "$label: $actual_key matches $expected_key"
  else
    record_missing "$label field mismatch: $actual_key must match $expected_key in $path"
  fi
}

check_json_nonempty_string() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"

  if json_nonempty_string "$path" "$dotted_key"; then
    record_satisfied "$label: $dotted_key present"
  else
    record_missing "$label missing non-empty field: $dotted_key in $path"
  fi
}

check_json_sha256() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"

  if json_sha256_string "$path" "$dotted_key"; then
    record_satisfied "$label: $dotted_key is SHA-256"
  else
    record_missing "$label invalid SHA-256 field: $dotted_key in $path"
  fi
}

check_json_utc_timestamp() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"

  if json_utc_timestamp "$path" "$dotted_key"; then
    record_satisfied "$label: $dotted_key is UTC"
  else
    record_missing "$label invalid UTC timestamp: $dotted_key in $path"
  fi
}

check_json_timestamp_order() {
  local label="$1"
  local path="$2"
  local start_key="$3"
  local completed_key="$4"

  if json_timestamp_order "$path" "$start_key" "$completed_key"; then
    record_satisfied "$label: $completed_key >= $start_key"
  else
    record_missing "$label timestamp order invalid: $completed_key must be greater than or equal to $start_key in $path"
  fi
}

check_release_evidence() {
  check_path "app bundle path" "$APP_PATH" dir
  check_path "app executable" "$APP_PATH/Contents/MacOS/JarvisMacApp" executable
  check_path "bundled core executable" "$APP_PATH/Contents/Resources/bin/jarvis-cli" executable
  check_path "app zip path" "$ZIP_PATH" file
  check_path "installer package path" "$PKG_PATH" file

  if valid_json_file "$SIGNED_PROVENANCE_REPORT"; then
    record_satisfied "signed-distribution provenance report JSON: $SIGNED_PROVENANCE_REPORT"
    check_json_number "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "schema_version" "1"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "evidence_type" "signed_distribution_provenance"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "version" "$EXPECTED_VERSION"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "bundle_identifier" "$EXPECTED_BUNDLE_ID"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.app_path" "$APP_PATH"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.zip_path" "$ZIP_PATH"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.pkg_path" "$PKG_PATH"
    for field in artifacts.zip_sha256 artifacts.pkg_sha256; do
      check_json_sha256 "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "$field"
    done
    for flag in developer_id_application_signed developer_id_installer_signed app_zip_notarized installer_pkg_notarized app_stapled installer_pkg_stapled gatekeeper_assessed artifact_digests_recorded; do
      check_json_flag "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "validation_flags.$flag"
    done
    for field in signing.developer_id_application_identity signing.developer_id_installer_identity signing.app_bundle_codesign signing.app_executable_codesign signing.bundled_core_codesign signing.installer_pkg_signature notarization.app_zip_submission_id notarization.installer_pkg_submission_id notarization.app_zip_notary_log notarization.installer_pkg_notary_log stapling.app_bundle_validation stapling.installer_pkg_validation gatekeeper.app_bundle_assessment gatekeeper.installer_pkg_assessment; do
      check_json_nonempty_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "$field"
    done
    check_json_utc_timestamp "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "generated_at"
  else
    record_missing "signed-distribution provenance report missing or invalid JSON: $SIGNED_PROVENANCE_REPORT"
  fi

  if valid_json_file "$LIVE_QA_REPORT"; then
    record_satisfied "live-device QA report JSON: $LIVE_QA_REPORT"
    for flag in clean_profile finder_launch microphone speech_permission audio_output notification restart manual_release_qa; do
      check_json_flag "live-device QA report" "$LIVE_QA_REPORT" "validation_flags.$flag"
    done
    check_json_flag "live-device QA report" "$LIVE_QA_REPORT" "validation_flags.transcript_handoff"
    for flag in microphone_permission_prompt speech_permission_prompt spoken_transcript_handoff same_command_path speech_output_playback; do
      check_json_flag "live-device QA report" "$LIVE_QA_REPORT" "voice_loop.$flag"
    done
    check_json_number "live-device QA report" "$LIVE_QA_REPORT" "schema_version" "1"
    check_json_string "live-device QA report" "$LIVE_QA_REPORT" "evidence_type" "owner_recorded_live_device_qa"
    check_json_false_flag "live-device QA report" "$LIVE_QA_REPORT" "self_test_fixture"
    for field in owner_name device_label profile_label voice_check_started_at voice_check_completed_at microphone_evidence_note speech_permission_evidence_note transcript_handoff_evidence_note audio_output_evidence_note; do
      check_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.$field"
    done
    check_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_started_at"
    check_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_completed_at"
    check_json_timestamp_order "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_started_at" "owner_recorded_live_voice_evidence.voice_check_completed_at"
    check_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "generated_at"
    check_json_timestamp_order "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_completed_at" "generated_at"
    for field in test_phrase observed_transcript expected_command_text observed_command_text command_result_evidence_id audio_output_device_label; do
      check_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "voice_command_observation.$field"
    done
    check_json_string_fields_equal "live-device QA report" "$LIVE_QA_REPORT" "voice_command_observation.expected_command_text" "voice_command_observation.observed_command_text"
    check_json_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.bundle_identifier" "$EXPECTED_BUNDLE_ID"
    check_json_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.short_version" "$EXPECTED_VERSION"
    check_json_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.build_version" "$EXPECTED_VERSION"
    check_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.microphone_usage_description"
    check_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.speech_recognition_usage_description"
  else
    record_missing "live-device QA report missing or invalid JSON: $LIVE_QA_REPORT"
  fi

  if valid_json_file "$PLUGIN_QA_REPORT"; then
    record_satisfied "plugin-trust QA report JSON: $PLUGIN_QA_REPORT"
    for flag in marketplace_review malware_scan os_sandbox egress_enforcement signed_publisher_policy manual_trust_review; do
      check_json_flag "plugin-trust QA report" "$PLUGIN_QA_REPORT" "validation_flags.$flag"
    done
    for field in owner_name review_started_at review_completed_at marketplace_evidence_note malware_scan_evidence_note os_sandbox_evidence_note egress_evidence_note egress_policy_label egress_deny_fixture_evidence_note egress_allow_fixture_evidence_note signed_publisher_evidence_note manual_review_evidence_note; do
      check_json_nonempty_string "plugin-trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.$field"
    done
    check_json_utc_timestamp "plugin-trust QA report" "$PLUGIN_QA_REPORT" "generated_at"
    check_json_utc_timestamp "plugin-trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.review_started_at"
    check_json_utc_timestamp "plugin-trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.review_completed_at"
    check_json_utc_timestamp "plugin-trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.egress_validation_completed_at"
    check_json_timestamp_order "plugin-trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.review_started_at" "owner_recorded_plugin_trust_evidence.review_completed_at"
    check_json_timestamp_order "plugin-trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.review_started_at" "owner_recorded_plugin_trust_evidence.egress_validation_completed_at"
    check_json_timestamp_order "plugin-trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.egress_validation_completed_at" "owner_recorded_plugin_trust_evidence.review_completed_at"
    check_json_timestamp_order "plugin-trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.review_completed_at" "generated_at"
  else
    record_missing "plugin-trust QA report missing or invalid JSON: $PLUGIN_QA_REPORT"
  fi

  if valid_json_file "$BUNDLE_PATH"; then
    record_satisfied "release evidence bundle JSON: $BUNDLE_PATH"
    for flag in signed_distribution notarization clean_profile live_device_qa plugin_trust_qa reports_archived; do
      check_json_flag "release evidence bundle" "$BUNDLE_PATH" "validation_flags.$flag"
    done
    check_json_flag "release evidence bundle" "$BUNDLE_PATH" "validation_flags.local_signature_validation"
    check_json_utc_timestamp "release evidence bundle" "$BUNDLE_PATH" "generated_at"
    check_json_string "release evidence bundle" "$BUNDLE_PATH" "version" "$VERSION"
    for field in artifacts.app_path artifacts.zip_path artifacts.pkg_path reports.signed_distribution_provenance_report reports.live_device_qa_report reports.plugin_trust_qa_report; do
      check_json_nonempty_string "release evidence bundle" "$BUNDLE_PATH" "$field"
    done
    for field in artifacts.zip_sha256 artifacts.pkg_sha256 reports.signed_distribution_provenance_sha256 reports.live_device_qa_sha256 reports.plugin_trust_qa_sha256; do
      check_json_sha256 "release evidence bundle" "$BUNDLE_PATH" "$field"
    done
  else
    record_missing "release evidence bundle missing or invalid JSON: $BUNDLE_PATH"
  fi
}

print_status() {
  local status="incomplete"
  if [[ "${#MISSING_ITEMS[@]}" -eq 0 ]]; then
    status="complete"
  fi

  printf 'Jarvis release evidence inventory: %s\n' "$status"
  printf 'Satisfied evidence items: %s\n' "${#SATISFIED_ITEMS[@]}"
  if [[ "${#SATISFIED_ITEMS[@]}" -gt 0 ]]; then
    for item in "${SATISFIED_ITEMS[@]}"; do
      printf '  ok: %s\n' "$item"
    done
  fi
  printf 'Missing evidence items: %s\n' "${#MISSING_ITEMS[@]}"
  if [[ "${#MISSING_ITEMS[@]}" -gt 0 ]]; then
    for item in "${MISSING_ITEMS[@]}"; do
      printf '  missing: %s\n' "$item"
    done
    print_next_steps
  fi
  printf 'Proof boundary: file/report path inventory only; present artifact paths do not prove Developer ID signing, notarization, stapling, installation, Finder launch, live device QA, marketplace review, malware scan, or OS sandbox enforcement.\n'
}

write_fixture_app() {
  local app_path="$1"
  mkdir -p "$app_path/Contents/MacOS" "$app_path/Contents/Resources/bin"
  touch "$app_path/Contents/MacOS/JarvisMacApp" "$app_path/Contents/Resources/bin/jarvis-cli"
  chmod 755 "$app_path/Contents/MacOS/JarvisMacApp" "$app_path/Contents/Resources/bin/jarvis-cli"
}

write_fixture_reports() {
  local live_path="$1"
  local plugin_path="$2"
  local bundle_path="$3"

  cat >"$live_path" <<JSON
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
    "short_version": "$VERSION",
    "build_version": "$VERSION",
    "microphone_usage_description": "self-test fixture",
    "speech_recognition_usage_description": "self-test fixture"
  },
  "proof_boundary": "self-test fixture"
}
JSON
  cat >"$plugin_path" <<'JSON'
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
    "egress_policy_label": "Self-test host egress policy fixture",
    "egress_validation_completed_at": "2026-05-22T16:18:00Z",
    "egress_deny_fixture_evidence_note": "Deny fixture blocked undeclared outbound traffic.",
    "egress_allow_fixture_evidence_note": "Allow fixture reached the declared host only.",
    "signed_publisher_evidence_note": "Signed publisher policy fixture was observed.",
    "manual_review_evidence_note": "Manual trust review fixture was observed."
  },
  "proof_boundary": "self-test fixture"
}
JSON
  cat >"$bundle_path" <<JSON
{
  "generated_at": "2026-05-22T16:30:00Z",
  "version": "$VERSION",
  "artifacts": {
    "app_path": "target/distribution/Jarvis.app",
    "zip_path": "target/distribution/Jarvis-$VERSION.zip",
    "pkg_path": "target/distribution/Jarvis-$VERSION.pkg",
    "zip_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "pkg_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  },
  "reports": {
    "signed_distribution_provenance_report": "target/distribution/Jarvis-$VERSION-signed-provenance.json",
    "live_device_qa_report": "target/release-live-device-qa-report.json",
    "plugin_trust_qa_report": "target/release-plugin-trust-qa-report.json",
    "signed_distribution_provenance_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "live_device_qa_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "plugin_trust_qa_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  },
  "validation_flags": {
    "signed_distribution": true,
    "notarization": true,
    "clean_profile": true,
    "live_device_qa": true,
    "plugin_trust_qa": true,
    "reports_archived": true,
    "local_signature_validation": true
  },
  "proof_boundary": "self-test fixture"
}
JSON
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

require_command python3

if [[ "$SELF_TEST" == true ]]; then
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-release-evidence-doctor.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT
  self_test_zip="$tmp_dir/dist/Jarvis-$VERSION.zip"
  self_test_pkg="$tmp_dir/dist/Jarvis-$VERSION.pkg"
  mkdir -p "$tmp_dir/dist"
  write_fixture_app "$tmp_dir/dist/Jarvis.app"
  touch "$self_test_zip" "$self_test_pkg"
  write_fixture_reports "$tmp_dir/live.json" "$tmp_dir/plugin.json" "$tmp_dir/bundle.json"
  cat >"$tmp_dir/dist/Jarvis-$VERSION-signed-provenance.json" <<JSON
{
  "schema_version": 1,
  "evidence_type": "signed_distribution_provenance",
  "generated_at": "2026-05-22T16:40:00Z",
  "version": "$VERSION",
  "bundle_identifier": "com.nobiletechnology.jarvis",
  "artifacts": {
    "app_path": "$tmp_dir/dist/Jarvis.app",
    "zip_path": "$self_test_zip",
    "pkg_path": "$self_test_pkg",
    "zip_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "pkg_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  },
  "signing": {
    "developer_id_application_identity": "Developer ID Application: Jarvis QA Fixture",
    "developer_id_installer_identity": "Developer ID Installer: Jarvis QA Fixture",
    "app_bundle_codesign": "Authority=Developer ID Application: Jarvis QA Fixture",
    "app_executable_codesign": "Authority=Developer ID Application: Jarvis QA Fixture",
    "bundled_core_codesign": "Authority=Developer ID Application: Jarvis QA Fixture",
    "installer_pkg_signature": "Developer ID Installer: Jarvis QA Fixture"
  },
  "notarization": {
    "app_zip_submission_id": "00000000-0000-4000-8000-000000000001",
    "installer_pkg_submission_id": "00000000-0000-4000-8000-000000000002",
    "app_zip_notary_log": "$tmp_dir/app-zip-notarytool.log",
    "installer_pkg_notary_log": "$tmp_dir/installer-pkg-notarytool.log"
  },
  "stapling": {
    "app_bundle_validation": "The validate action worked!",
    "installer_pkg_validation": "The validate action worked!"
  },
  "gatekeeper": {
    "app_bundle_assessment": "accepted",
    "installer_pkg_assessment": "accepted"
  },
  "validation_flags": {
    "developer_id_application_signed": true,
    "developer_id_installer_signed": true,
    "app_zip_notarized": true,
    "installer_pkg_notarized": true,
    "app_stapled": true,
    "installer_pkg_stapled": true,
    "gatekeeper_assessed": true,
    "artifact_digests_recorded": true
  },
  "proof_boundary": "self-test fixture"
}
JSON

  JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null

  check_output="$(JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/missing-dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/missing-dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/missing-dist/Jarvis-$VERSION.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/missing-dist/Jarvis-$VERSION.pkg" \
    JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/missing-dist/Jarvis-$VERSION-signed-provenance.json" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/missing-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/missing-plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/missing-bundle.json" \
    "$0" --check)"
  for expected in \
    "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env" \
    "source target/release-live-device-qa.env" \
    "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env" \
    "source target/release-plugin-trust-qa.env" \
    "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env" \
    "source target/release-evidence-bundle.env" \
    "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...'"; do
    if [[ "$check_output" != *"$expected"* ]]; then
      fail "release evidence doctor self-test expected --check output to include: $expected"
    fi
  done

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
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/missing-observation-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected blank live voice observation to fail"
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
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/mismatched-command-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected mismatched live command observation to fail"
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
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/pregenerated-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected live report generated before completion to fail"
  fi

  python3 - "$tmp_dir/plugin.json" "$tmp_dir/missing-observation-plugin.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["owner_recorded_plugin_trust_evidence"]["egress_deny_fixture_evidence_note"] = ""
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/missing-observation-plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected blank plugin trust observation to fail"
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
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/non-utc-plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected non-UTC plugin trust timestamp to fail"
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
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/reversed-plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected reversed plugin trust timestamps to fail"
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
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/pregenerated-plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected plugin report generated before completion to fail"
  fi

  python3 - "$tmp_dir/bundle.json" "$tmp_dir/minimal-bundle.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data.pop("artifacts", None)
data.pop("reports", None)
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/minimal-bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected minimal final bundle to fail"
  fi

  python3 - "$tmp_dir/bundle.json" "$tmp_dir/bad-digest-bundle.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["artifacts"]["zip_sha256"] = "not-a-sha"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bad-digest-bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected malformed final bundle digest to fail"
  fi

  python3 - "$tmp_dir/bundle.json" "$tmp_dir/disabled-local-signature-bundle.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["validation_flags"]["local_signature_validation"] = False
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/disabled-local-signature-bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected disabled local signature validation to fail"
  fi

  rm "$tmp_dir/plugin.json"
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected missing plugin report to fail"
  fi

  printf 'Jarvis release evidence doctor self-test: ok\n'
  printf 'Proof boundary: fake artifacts and reports validate doctor status mechanics only; no production evidence was created.\n'
  exit 0
fi

check_release_evidence
print_status

if [[ "$ASSERT_COMPLETE" == true && "${#MISSING_ITEMS[@]}" -gt 0 ]]; then
  exit 1
fi
