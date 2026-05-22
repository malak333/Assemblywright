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

check_release_evidence() {
  check_path "app bundle path" "$APP_PATH" dir
  check_path "app executable" "$APP_PATH/Contents/MacOS/JarvisMacApp" executable
  check_path "bundled core executable" "$APP_PATH/Contents/Resources/bin/jarvis-cli" executable
  check_path "app zip path" "$ZIP_PATH" file
  check_path "installer package path" "$PKG_PATH" file

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
    for field in test_phrase observed_transcript observed_command_text command_result_evidence_id audio_output_device_label; do
      check_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "voice_command_observation.$field"
    done
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
    for field in owner_name review_started_at review_completed_at marketplace_evidence_note malware_scan_evidence_note os_sandbox_evidence_note egress_evidence_note signed_publisher_evidence_note manual_review_evidence_note; do
      check_json_nonempty_string "plugin-trust QA report" "$PLUGIN_QA_REPORT" "owner_recorded_plugin_trust_evidence.$field"
    done
  else
    record_missing "plugin-trust QA report missing or invalid JSON: $PLUGIN_QA_REPORT"
  fi

  if valid_json_file "$BUNDLE_PATH"; then
    record_satisfied "release evidence bundle JSON: $BUNDLE_PATH"
    for flag in signed_distribution notarization clean_profile live_device_qa plugin_trust_qa reports_archived; do
      check_json_flag "release evidence bundle" "$BUNDLE_PATH" "validation_flags.$flag"
    done
    check_json_string "release evidence bundle" "$BUNDLE_PATH" "version" "$VERSION"
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

  cat >"$live_path" <<'JSON'
{
  "schema_version": 1,
  "evidence_type": "owner_recorded_live_device_qa",
  "self_test_fixture": false,
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
  cat >"$plugin_path" <<'JSON'
{
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
  cat >"$bundle_path" <<'JSON'
{
  "version": "0.1.4",
  "validation_flags": {
    "signed_distribution": true,
    "notarization": true,
    "clean_profile": true,
    "live_device_qa": true,
    "plugin_trust_qa": true,
    "reports_archived": true,
    "local_signature_validation": false
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
  mkdir -p "$tmp_dir/dist"
  write_fixture_app "$tmp_dir/dist/Jarvis.app"
  touch "$tmp_dir/dist/Jarvis-0.1.4.zip" "$tmp_dir/dist/Jarvis-0.1.4.pkg"
  write_fixture_reports "$tmp_dir/live.json" "$tmp_dir/plugin.json" "$tmp_dir/bundle.json"

  JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null

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
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected blank live voice observation to fail"
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
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected blank plugin trust observation to fail"
  fi

  rm "$tmp_dir/plugin.json"
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
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
