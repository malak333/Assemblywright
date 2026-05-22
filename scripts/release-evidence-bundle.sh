#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="${JARVIS_EVIDENCE_VERSION:-0.1.4}"
DIST_DIR="${JARVIS_EVIDENCE_DIST_DIR:-$ROOT_DIR/target/distribution}"
APP_PATH="${JARVIS_EVIDENCE_APP_PATH:-$DIST_DIR/Jarvis.app}"
ZIP_PATH="${JARVIS_EVIDENCE_ZIP_PATH:-$DIST_DIR/Jarvis-$VERSION.zip}"
PKG_PATH="${JARVIS_EVIDENCE_PKG_PATH:-$DIST_DIR/Jarvis-$VERSION.pkg}"
LIVE_QA_REPORT="${JARVIS_EVIDENCE_LIVE_QA_REPORT:-$ROOT_DIR/target/release-live-device-qa-report.json}"
PLUGIN_QA_REPORT="${JARVIS_EVIDENCE_PLUGIN_QA_REPORT:-$ROOT_DIR/target/release-plugin-trust-qa-report.json}"
OUTPUT_PATH="${JARVIS_EVIDENCE_OUTPUT_PATH:-$ROOT_DIR/target/release-evidence-bundle.json}"
VALIDATE_LOCAL_SIGNATURES="${JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES:-true}"
CHECK_ONLY=false
BUNDLE=false
SELF_TEST=false

usage() {
  cat <<'USAGE'
Usage: scripts/release-evidence-bundle.sh [--check|--bundle|--self-test]

Collect and validate the release evidence bundle required before any
production-ready claim for Jarvis.

--check validates repo-owned evidence-bundle prerequisites and prints the
external artifacts/reports required for a production release decision.

--bundle validates the expected signed distribution artifacts, live-device QA
report, plugin-trust QA report, and explicit owner evidence flags, then writes
a JSON bundle manifest.

--self-test creates fake artifacts/reports in a temporary directory and
exercises the bundle manifest mechanics without claiming production readiness.

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
  JARVIS_EVIDENCE_LIVE_QA_REPORT      Defaults to target/release-live-device-qa-report.json
  JARVIS_EVIDENCE_PLUGIN_QA_REPORT    Defaults to target/release-plugin-trust-qa-report.json
  JARVIS_EVIDENCE_OUTPUT_PATH         Defaults to target/release-evidence-bundle.json
  JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES
                                      Defaults to true. Set to false only for
                                      fake self-test fixtures.

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
  local escaped_boundary
  local local_signature_validation
  require_command python3
  generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  escaped_app="$(json_escape "$APP_PATH")"
  escaped_zip="$(json_escape "$ZIP_PATH")"
  escaped_pkg="$(json_escape "$PKG_PATH")"
  escaped_live="$(json_escape "$LIVE_QA_REPORT")"
  escaped_plugin="$(json_escape "$PLUGIN_QA_REPORT")"
  escaped_boundary="$(json_escape "Evidence bundle manifest only; relies on owner-recorded external validation flags and referenced signed/notarized artifacts plus QA reports.")"
  local_signature_validation="$VALIDATE_LOCAL_SIGNATURES"

  mkdir -p "$(dirname "$OUTPUT_PATH")"
  cat >"$OUTPUT_PATH" <<EOF
{
  "generated_at": "$generated_at",
  "version": "$VERSION",
  "artifacts": {
    "app_path": "$escaped_app",
    "zip_path": "$escaped_zip",
    "pkg_path": "$escaped_pkg"
  },
  "reports": {
    "live_device_qa_report": "$escaped_live",
    "plugin_trust_qa_report": "$escaped_plugin"
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
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

if { [[ "$CHECK_ONLY" == true ]] && { [[ "$BUNDLE" == true ]] || [[ "$SELF_TEST" == true ]]; }; } ||
  { [[ "$BUNDLE" == true ]] && [[ "$SELF_TEST" == true ]]; }; then
  fail "--check, --bundle, and --self-test are mutually exclusive"
fi

if [[ "$CHECK_ONLY" != true && "$BUNDLE" != true && "$SELF_TEST" != true ]]; then
  usage
  exit 0
fi

require_command grep
require_command python3
require_artifact_validation_mode

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
  "proof_boundary": "self-test fixture"
}
JSON
  cat >"$tmp_dir/plugin.json" <<'JSON'
{
  "validation_flags": {
    "marketplace_review": true,
    "malware_scan": true,
    "os_sandbox": true,
    "egress_enforcement": true,
    "signed_publisher_policy": true,
    "manual_trust_review": true
  },
  "proof_boundary": "self-test fixture"
}
JSON

  JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$tmp_dir/dist/Jarvis-0.1.4.zip" \
    JARVIS_EVIDENCE_PKG_PATH="$tmp_dir/dist/Jarvis-0.1.4.pkg" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
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
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"plugin_trust_qa_report"'
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
- Full Developer ID signed and notarized app zip exists.
- Full Developer ID signed and notarized /Applications installer package exists.
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

require_dir "signed app bundle" "$APP_PATH"
require_file "signed app zip" "$ZIP_PATH"
require_file "signed installer package" "$PKG_PATH"
validate_local_distribution_evidence
require_json_contains "live-device QA report" "$LIVE_QA_REPORT" '"manual_release_qa": true'
require_json_contains "plugin trust QA report" "$PLUGIN_QA_REPORT" '"manual_trust_review": true'
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
