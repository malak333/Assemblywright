#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="${ASSEMBLYWRIGHT_EVIDENCE_VERSION:-$("$ROOT_DIR/scripts/release-version.sh")}"
DIST_DIR="${ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR:-$ROOT_DIR/target/distribution}"
APP_PATH="${ASSEMBLYWRIGHT_EVIDENCE_APP_PATH:-$DIST_DIR/Assemblywright.app}"
ZIP_PATH="${ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH:-$DIST_DIR/Assemblywright-$VERSION.zip}"
PKG_PATH="${ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH:-$DIST_DIR/Assemblywright-$VERSION.pkg}"
SIGNED_PROVENANCE_REPORT="${ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT:-$DIST_DIR/Assemblywright-$VERSION-signed-provenance.json}"
LIVE_QA_REPORT="${ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT:-${ASSEMBLYWRIGHT_QA_REPORT_PATH:-$ROOT_DIR/target/release-live-device-qa-report.json}}"
OUTPUT_PATH="${ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH:-$ROOT_DIR/target/release-evidence-bundle.json}"
OVERWRITE_OUTPUT="${ASSEMBLYWRIGHT_EVIDENCE_OVERWRITE_OUTPUT:-false}"
VALIDATE_LOCAL_SIGNATURES="${ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES:-true}"
EXPECTED_BUNDLE_ID="${ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_BUNDLE_ID:-com.nobiletechnology.assemblywright}"
EXPECTED_VERSION="${ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_VERSION:-$VERSION}"
EXPECTED_INSTALLED_APP_PATH="${ASSEMBLYWRIGHT_QA_INSTALLED_APP_PATH:-/Applications/Assemblywright.app}"
CHECK_ONLY=false
BUNDLE=false
SELF_TEST=false
WRITE_TEMPLATE=false
WRITE_TEMPLATE_PATH=""

usage() {
  cat <<'USAGE'
Usage: scripts/release-evidence-bundle.sh [--check|--bundle|--self-test|--write-template PATH]

Collect and validate the release evidence bundle required before any
production-ready claim for Assemblywright.

--check validates repo-owned evidence-bundle prerequisites and prints the
external artifacts/reports required for a production release decision.

--bundle validates the expected signed distribution artifacts, live-device QA
report and explicit owner evidence flags, then writes
a JSON bundle manifest.

--self-test creates fake artifacts/reports in a temporary directory and
exercises the bundle manifest mechanics without claiming production readiness.

--write-template PATH writes a sourceable shell env template containing every
ASSEMBLYWRIGHT_EVIDENCE_* input required by --bundle. Edit the template only after the
external release checks are complete, then source it and rerun --bundle.

Required before --bundle:
  ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true
  ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true
  ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true
  ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true
  ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true
  ASSEMBLYWRIGHT_EVIDENCE_OWNER_NAME
  ASSEMBLYWRIGHT_EVIDENCE_COMPLETED_AT
  ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_NOTE
  ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_NOTE
  ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_NOTE
  ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_NOTE
  ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_NOTE
  ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_URI

Optional:
  ASSEMBLYWRIGHT_EVIDENCE_VERSION             Defaults to the Rust package release version
  ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR            Defaults to target/distribution
  ASSEMBLYWRIGHT_EVIDENCE_APP_PATH            Defaults to target/distribution/Assemblywright.app
  ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH            Defaults to target/distribution/Assemblywright-<version>.zip
  ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH            Defaults to target/distribution/Assemblywright-<version>.pkg
  ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT
                                      Defaults to target/distribution/Assemblywright-<version>-signed-provenance.json
  ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT      Defaults to ASSEMBLYWRIGHT_QA_REPORT_PATH or target/release-live-device-qa-report.json
  ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH         Defaults to target/release-evidence-bundle.json
  ASSEMBLYWRIGHT_EVIDENCE_OVERWRITE_OUTPUT    Defaults to false. Set to true only when
                                      intentionally replacing an existing
                                      bundle after preserving the old artifact.
  ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES
                                      Defaults to true. Set to false only for
                                      fake self-test fixtures.
  ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_BUNDLE_ID  Defaults to com.nobiletechnology.assemblywright
  ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_VERSION    Defaults to ASSEMBLYWRIGHT_EVIDENCE_VERSION
  ASSEMBLYWRIGHT_QA_INSTALLED_APP_PATH        Defaults to /Applications/Assemblywright.app and must match the live QA report

This script validates evidence capture only. It does not sign, notarize,
install, launch Finder, or
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

require_non_empty_env() {
  local name="$1"
  local value="${!name:-}"
  [[ -n "${value//[[:space:]]/}" ]] || fail "$name must be set to a non-empty owner-recorded evidence value"
}

require_meaningful_evidence_env() {
  local name="$1"
  local value="${!name:-}"
  require_non_empty_env "$name"
  require_command python3
  python3 - "$name" "$value" <<'PY'
import re
import sys

name, value = sys.argv[1:3]
normalized = value.strip().lower()
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
    raise SystemExit(f"{name} must contain owner-recorded external evidence, not placeholder or fixture text")
PY
}

require_reports_archive_uri_env() {
  local name="$1"
  local value="${!name:-}"
  require_non_empty_env "$name"
  require_command python3
  python3 - "$name" "$value" <<'PY'
import re
import sys
from urllib.parse import urlparse

name, value = sys.argv[1:3]
trimmed = value.strip()
parsed = urlparse(trimmed)
if not parsed.scheme:
    raise SystemExit(f"{name} must be a URI with a scheme such as file:, https:, s3:, or gs:")
if parsed.scheme == "file" and not (parsed.netloc or parsed.path):
    raise SystemExit(f"{name} file URI must include an archive location")
if parsed.scheme != "file" and not (parsed.netloc or parsed.path):
    raise SystemExit(f"{name} URI must include an archive location")

placeholder = re.compile(r"(self-test|placeholder|example|fixture|todo|tbd|replace-me|changeme|/tmp/|/temp/)", re.IGNORECASE)
if placeholder.search(trimmed):
    raise SystemExit(f"{name} must point to a durable release evidence archive, not a placeholder or self-test location")
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
    raise SystemExit(f"{name} must be a UTC RFC3339 timestamp like 2026-05-22T17:00:00Z") from exc
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
    raise SystemExit(f"{name} must not be future-dated")
PY
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

require_app_bundle_metadata() {
  local info_plist="$APP_PATH/Contents/Info.plist"
  require_file "app bundle Info.plist" "$info_plist"
  python3 - "$info_plist" "$EXPECTED_BUNDLE_ID" "$EXPECTED_VERSION" <<'PY'
import plistlib
import sys

path, expected_bundle_id, expected_version = sys.argv[1:4]
with open(path, "rb") as handle:
    data = plistlib.load(handle)

for key, expected in {
    "CFBundleIdentifier": expected_bundle_id,
    "CFBundleShortVersionString": expected_version,
    "CFBundleVersion": expected_version,
}.items():
    actual = data.get(key)
    if actual != expected:
        raise SystemExit(f"app bundle Info.plist {key} mismatch: expected {expected}, got {actual!r}")
PY
}

require_bundled_core_version() {
  local core_path="$APP_PATH/Contents/Resources/bin/assemblywright-cli"
  local marker_path="$core_path.version"
  local output
  [[ -x "$core_path" ]] || fail "missing bundled core executable: $core_path"
  [[ -f "$marker_path" ]] || fail "missing bundled core version marker: $marker_path; rerun ./scripts/package-distribution.sh --unsigned-launch-check for local evidence, or the signed package-distribution.sh lane before final release evidence"
  if [[ "$(tr -d '\r\n' <"$marker_path")" != "assemblywright $EXPECTED_VERSION" ]]; then
    fail "bundled core version marker mismatch: expected assemblywright $EXPECTED_VERSION from $marker_path; rerun ./scripts/package-distribution.sh --unsigned-launch-check for local evidence, or the signed package-distribution.sh lane before final release evidence"
  fi
  output="$("$core_path" --version)"
  if [[ "$output" != *"assemblywright $EXPECTED_VERSION"* ]]; then
    printf 'error: bundled core --version did not include %q\n' "assemblywright $EXPECTED_VERSION" >&2
    printf '%s\n%s\n%s\n' "--- bundled core version output ---" "$output" "--- end bundled core version output ---" >&2
    exit 1
  fi
}

require_artifact_validation_mode() {
  case "$VALIDATE_LOCAL_SIGNATURES" in
    true|false)
      ;;
    *)
      fail "ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES must be true or false"
      ;;
  esac
}

require_production_signature_validation() {
  if [[ "$VALIDATE_LOCAL_SIGNATURES" != true && "${ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE:-}" != true ]]; then
    fail "ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false is only allowed during --self-test"
  fi
}

require_output_write_mode() {
  case "$OVERWRITE_OUTPUT" in
    true|false)
      ;;
    *)
      fail "ASSEMBLYWRIGHT_EVIDENCE_OVERWRITE_OUTPUT must be true or false"
      ;;
  esac
}

require_output_path_available() {
  if [[ -e "$OUTPUT_PATH" && "$OVERWRITE_OUTPUT" != true ]]; then
    fail "release evidence bundle output already exists: $OUTPUT_PATH; preserve the existing artifact or set ASSEMBLYWRIGHT_EVIDENCE_OVERWRITE_OUTPUT=true for an intentional replacement"
  fi
}

require_distinct_evidence_paths() {
  require_command python3
  python3 - \
    "$OUTPUT_PATH" \
    "$ZIP_PATH" \
    "$PKG_PATH" \
    "$SIGNED_PROVENANCE_REPORT" \
    "$LIVE_QA_REPORT" <<'PY'
import os
import sys

output_path = os.path.abspath(os.path.expanduser(sys.argv[1]))
inputs = [
    ("app zip artifact", sys.argv[2]),
    ("installer package artifact", sys.argv[3]),
    ("signed-distribution provenance report", sys.argv[4]),
    ("live-device QA report", sys.argv[5]),
]
for label, path in inputs:
    normalized = os.path.abspath(os.path.expanduser(path))
    if output_path == normalized:
        raise SystemExit(
            "release evidence bundle output path must not overwrite "
            f"{label}: {path}"
        )
PY
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

require_json_string_one_of() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  shift 3
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$label" "$@" <<'PY'
import json
import sys

path, dotted_key, label, *allowed = sys.argv[1:]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
    cursor = cursor[segment]

if cursor not in allowed:
    expected = ", ".join(allowed)
    raise SystemExit(
        f"{label} evidence field {dotted_key} must be one of {expected}; got {cursor!r}"
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

require_json_command_result_evidence_id() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$label" <<'PY'
import json
import re
import sys

path, dotted_key, label = sys.argv[1:4]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
    cursor = cursor[segment]

pattern = re.compile(
    r"^(task|audit):[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
)
if not isinstance(cursor, str) or not pattern.fullmatch(cursor.strip()):
    raise SystemExit(f"{label} evidence field {dotted_key} must be task:<uuid> or audit:<uuid>")
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

require_json_meaningful_evidence_string() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  require_json_nonempty_string "$label" "$path" "$dotted_key"
  python3 - "$path" "$dotted_key" "$label" <<'PY'
import json
import sys

path, dotted_key, label = sys.argv[1:4]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    cursor = cursor[segment]

normalized = cursor.strip().lower()
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
    raise SystemExit(f"{label} evidence field {dotted_key} must contain owner-recorded external evidence, not placeholder or fixture text")
PY
}

require_json_reports_archive_uri() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$label" <<'PY'
import json
import re
import sys
from urllib.parse import urlparse

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

trimmed = cursor.strip()
parsed = urlparse(trimmed)
if not parsed.scheme:
    raise SystemExit(f"{label} evidence field {dotted_key} must be a URI with a scheme")
if parsed.scheme == "file" and not (parsed.netloc or parsed.path):
    raise SystemExit(f"{label} evidence field {dotted_key} file URI must include an archive location")
if parsed.scheme != "file" and not (parsed.netloc or parsed.path):
    raise SystemExit(f"{label} evidence field {dotted_key} URI must include an archive location")

placeholder = re.compile(r"(self-test|placeholder|example|fixture|todo|tbd|replace-me|changeme|/tmp/|/temp/)", re.IGNORECASE)
if placeholder.search(trimmed):
    raise SystemExit(f"{label} evidence field {dotted_key} must point to a durable release evidence archive, not a placeholder or self-test location")
PY
}

require_json_string_prefix() {
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

if not isinstance(cursor, str) or not cursor.startswith(expected):
    raise SystemExit(f"{label} evidence field {dotted_key} must start with {expected!r}")
PY
}

require_json_string_contains() {
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

if not isinstance(cursor, str) or expected not in cursor:
    raise SystemExit(f"{label} evidence field {dotted_key} must include {expected!r}")
PY
}

require_json_gatekeeper_accepted() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$label" <<'PY'
import json
import re
import sys

path, dotted_key, label = sys.argv[1:4]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
    cursor = cursor[segment]

if not isinstance(cursor, str):
    raise SystemExit(f"{label} evidence field {dotted_key} must be a string")

lines = [line.strip() for line in cursor.splitlines() if line.strip()]
if not any(line == "accepted" or re.search(r":\s*accepted$", line) for line in lines):
    raise SystemExit(f"{label} evidence field {dotted_key} must include an exact Gatekeeper accepted result")
PY
}

require_json_uuid() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$label" <<'PY'
import json
import sys
import uuid

path, dotted_key, label = sys.argv[1:4]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
    cursor = cursor[segment]

try:
    parsed = uuid.UUID(cursor.strip()) if isinstance(cursor, str) else uuid.UUID("")
except Exception:
    raise SystemExit(f"{label} evidence field {dotted_key} must be a UUID")

if parsed.int == 0:
    raise SystemExit(f"{label} evidence field {dotted_key} must not be a nil UUID")
PY
}

require_json_sha256() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$label" <<'PY'
import json
import string
import sys

path, dotted_key, label = sys.argv[1:4]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
    cursor = cursor[segment]

hexdigits = set(string.hexdigits)
if not isinstance(cursor, str) or len(cursor) != 64 or any(char not in hexdigits for char in cursor):
    raise SystemExit(f"{label} required evidence field must be a SHA-256 hex digest: {dotted_key}")
PY
}

require_json_app_code_identity() {
  local label="$1"
  local path="$2"
  local identifier_field="$3"
  local team_field="$4"
  local cdhash_field="$5"
  local expected_identifier="$6"
  require_command python3
  python3 - "$label" "$path" "$identifier_field" "$team_field" "$cdhash_field" "$expected_identifier" <<'PY'
import json
import re
import sys

label, path, identifier_field, team_field, cdhash_field, expected_identifier = sys.argv[1:7]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

def value_at(field):
    value = data
    for part in field.split("."):
        if not isinstance(value, dict) or part not in value:
            raise SystemExit(f"{label} is missing required field: {field}")
        value = value[part]
    if not isinstance(value, str) or not value:
        raise SystemExit(f"{label} field must be a non-empty string: {field}")
    return value

identifier = value_at(identifier_field)
team = value_at(team_field)
cdhash = value_at(cdhash_field)
if identifier != expected_identifier:
    raise SystemExit(f"{label} {identifier_field} must equal {expected_identifier}")
if not re.fullmatch(r"[A-Z0-9]{10}", team):
    raise SystemExit(f"{label} {team_field} must be a 10-character Apple team identifier")
if not re.fullmatch(r"[0-9A-Fa-f]{40,64}", cdhash):
    raise SystemExit(f"{label} {cdhash_field} must be a 40-64 character hexadecimal CDHash")
PY
}

require_json_sha256_matches_file() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  local artifact_label="$4"
  local artifact_path="$5"
  local expected_sha
  require_file "$label" "$path"
  require_file "$artifact_label" "$artifact_path"
  expected_sha="$(file_sha256 "$artifact_path")"
  python3 - "$path" "$dotted_key" "$expected_sha" "$label" "$artifact_label" <<'PY'
import json
import sys

path, dotted_key, expected_sha, label, artifact_label = sys.argv[1:6]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
    cursor = cursor[segment]

if cursor != expected_sha:
    raise SystemExit(f"{label} {dotted_key} does not match current {artifact_label}")
PY
}

require_json_sha256_matches_json_path() {
  local label="$1"
  local path="$2"
  local digest_key="$3"
  local artifact_label="$4"
  local path_key="$5"
  require_file "$label" "$path"
  python3 - "$path" "$digest_key" "$path_key" "$label" "$artifact_label" <<'PY'
import hashlib
import json
import sys

path, digest_key, path_key, label, artifact_label = sys.argv[1:6]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

def get(dotted_key):
    cursor = data
    for segment in dotted_key.split("."):
        if not isinstance(cursor, dict) or segment not in cursor:
            raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
        cursor = cursor[segment]
    if not isinstance(cursor, str) or not cursor.strip():
        raise SystemExit(f"{label} required evidence field must be non-empty: {dotted_key}")
    return cursor.strip()

artifact_path = get(path_key)
expected_sha = get(digest_key)
try:
    with open(artifact_path, "rb") as handle:
        actual_sha = hashlib.sha256(handle.read()).hexdigest()
except OSError as error:
    raise SystemExit(f"{label} {path_key} points to unreadable {artifact_label}: {artifact_path}: {error}")

if actual_sha != expected_sha:
    raise SystemExit(f"{label} {digest_key} does not match current {artifact_label}: {artifact_path}")
PY
}

require_json_utc_timestamp() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  require_file "$label" "$path"
  python3 - "$path" "$dotted_key" "$label" <<'PY'
from datetime import datetime, timezone
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
    timestamp = datetime.fromisoformat(cursor.replace("Z", "+00:00"))
except ValueError as exc:
    raise SystemExit(f"{label} required evidence field must be a UTC RFC3339 timestamp: {dotted_key}") from exc
if timestamp > datetime.now(timezone.utc):
    raise SystemExit(f"{label} required evidence field must not be future-dated: {dotted_key}")
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
required_entries = (
    "Assemblywright.app/Contents/MacOS/AssemblywrightMacApp",
    "Assemblywright.app/Contents/Resources/bin/assemblywright-cli",
    "Assemblywright.app/Contents/Info.plist",
)

with zipfile.ZipFile(zip_path) as archive:
    names = archive.namelist()

missing = [
    entry
    for entry in required_entries
    if entry not in names
]
if missing:
    raise SystemExit(f"zip payload missing required app entries: {', '.join(missing)}")

nested = [
    name
    for name in names
    if "/Assemblywright.app/" in name and not name.startswith("Assemblywright.app/")
]
if nested:
    raise SystemExit(f"zip payload contains nested Assemblywright.app entries: {', '.join(nested[:3])}")

app_roots = {
    name.split("Assemblywright.app/", 1)[0] + "Assemblywright.app/"
    for name in names
    if "Assemblywright.app/" in name
}
if app_roots != {"Assemblywright.app/"}:
    raise SystemExit(f"zip payload must contain exactly one top-level Assemblywright.app root, got: {', '.join(sorted(app_roots))}")
PY
}

require_json_fields_equal() {
  local label="$1"
  local left_path="$2"
  local left_key="$3"
  local right_path="$4"
  local right_key="$5"
  require_file "$label left report" "$left_path"
  require_file "$label right report" "$right_path"
  python3 - "$left_path" "$left_key" "$right_path" "$right_key" "$label" <<'PY'
import json
import sys

left_path, left_key, right_path, right_key, label = sys.argv[1:6]

def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)

def get(data, dotted_key):
    cursor = data
    for segment in dotted_key.split("."):
        if not isinstance(cursor, dict) or segment not in cursor:
            raise SystemExit(f"{label} is missing required evidence field: {dotted_key}")
        cursor = cursor[segment]
    return cursor

left = get(load(left_path), left_key)
right = get(load(right_path), right_key)
if left != right:
    raise SystemExit(
        f"{label} mismatch: {left_key} from {left_path} must match {right_key} from {right_path}"
    )
PY
}

require_json_timestamp_between_reports() {
  local label="$1"
  local lower_path="$2"
  local lower_key="$3"
  local value_path="$4"
  local value_key="$5"
  local upper_path="$6"
  local upper_key="$7"
  require_file "$label lower report" "$lower_path"
  require_file "$label value report" "$value_path"
  require_file "$label upper report" "$upper_path"
  python3 - "$lower_path" "$lower_key" "$value_path" "$value_key" "$upper_path" "$upper_key" "$label" <<'PY'
from datetime import datetime
import json
import sys

lower_path, lower_key, value_path, value_key, upper_path, upper_key, label = sys.argv[1:8]

def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)

def get(data, dotted_key):
    cursor = data
    for segment in dotted_key.split("."):
        if not isinstance(cursor, dict) or segment not in cursor:
            raise SystemExit(f"{label} is missing required evidence timestamp: {dotted_key}")
        cursor = cursor[segment]
    if not isinstance(cursor, str) or not cursor.endswith("Z"):
        raise SystemExit(f"{label} required evidence timestamp must end in Z: {dotted_key}")
    try:
        return datetime.fromisoformat(cursor.replace("Z", "+00:00"))
    except ValueError as exc:
        raise SystemExit(f"{label} required evidence timestamp must be RFC3339 UTC: {dotted_key}") from exc

lower = get(load(lower_path), lower_key)
value = get(load(value_path), value_key)
upper = get(load(upper_path), upper_key)
if lower > value:
    raise SystemExit(f"{label} timestamp order invalid: {value_key} must be >= {lower_key}")
if value > upper:
    raise SystemExit(f"{label} timestamp order invalid: {value_key} must be <= {upper_key}")
PY
}

validate_local_distribution_evidence() {
  if [[ "$VALIDATE_LOCAL_SIGNATURES" != true ]]; then
    printf 'warning: local signature/stapling validation skipped by ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false\n' >&2
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
}

write_bundle() {
  local generated_at
  local zip_sha
  local pkg_sha
  local live_sha
  local signed_provenance_sha
  local local_signature_validation
  require_command shasum
  require_command python3
  generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  require_distinct_evidence_paths
  zip_sha="$(file_sha256 "$ZIP_PATH")"
  pkg_sha="$(file_sha256 "$PKG_PATH")"
  live_sha="$(file_sha256 "$LIVE_QA_REPORT")"
  signed_provenance_sha="$(file_sha256 "$SIGNED_PROVENANCE_REPORT")"
  local_signature_validation="$VALIDATE_LOCAL_SIGNATURES"

  mkdir -p "$(dirname "$OUTPUT_PATH")"
  require_output_path_available
  python3 - \
    "$OUTPUT_PATH" \
    "$generated_at" \
    "$VERSION" \
    "$APP_PATH" \
    "$ZIP_PATH" \
    "$PKG_PATH" \
    "$SIGNED_PROVENANCE_REPORT" \
    "$LIVE_QA_REPORT" \
    "$zip_sha" \
    "$pkg_sha" \
    "$signed_provenance_sha" \
    "$live_sha" \
    "$local_signature_validation" \
    "$ASSEMBLYWRIGHT_EVIDENCE_OWNER_NAME" \
    "$ASSEMBLYWRIGHT_EVIDENCE_COMPLETED_AT" \
    "$ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_NOTE" \
    "$ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_NOTE" \
    "$ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_NOTE" \
    "$ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_NOTE" \
    "$ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_NOTE" \
    "$ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_URI" <<'PY'
import json
import sys

(
    output_path,
    generated_at,
    version,
    app_path,
    zip_path,
    pkg_path,
    signed_provenance_report,
    live_qa_report,
    zip_sha,
    pkg_sha,
    signed_provenance_sha,
    live_sha,
    local_signature_validation,
    owner_name,
    completed_at,
    signed_distribution_note,
    notarization_note,
    clean_profile_note,
    live_device_qa_note,
    reports_archive_note,
    reports_archive_uri,
) = sys.argv[1:22]

data = {
    "schema_version": 1,
    "evidence_type": "release_evidence_bundle",
    "generated_at": generated_at,
    "version": version,
    "artifacts": {
        "app_path": app_path,
        "zip_path": zip_path,
        "pkg_path": pkg_path,
        "zip_sha256": zip_sha,
        "pkg_sha256": pkg_sha,
    },
    "reports": {
        "signed_distribution_provenance_report": signed_provenance_report,
        "live_device_qa_report": live_qa_report,
        "signed_distribution_provenance_sha256": signed_provenance_sha,
        "live_device_qa_sha256": live_sha,
    },
    "validation_flags": {
        "signed_distribution": True,
        "notarization": True,
        "clean_profile": True,
        "live_device_qa": True,
        "reports_archived": True,
        "local_signature_validation": local_signature_validation == "true",
    },
    "owner_recorded_release_evidence": {
        "owner_name": owner_name,
        "completed_at": completed_at,
        "signed_distribution_note": signed_distribution_note,
        "notarization_note": notarization_note,
        "clean_profile_note": clean_profile_note,
        "live_device_qa_note": live_device_qa_note,
        "reports_archive_note": reports_archive_note,
        "reports_archive_uri": reports_archive_uri,
    },
    "proof_boundary": "Evidence bundle manifest only; records artifact paths, signed-distribution provenance report path, local signature/stapling validation status, owner-recorded signing/notarization validation flags, and QA reports.",
}

with open(output_path, "w", encoding="utf-8") as handle:
    json.dump(data, handle, indent=2)
    handle.write("\n")
PY
  python3 -m json.tool "$OUTPUT_PATH" >/dev/null
}

write_env_template() {
  local template_path="$1"
  mkdir -p "$(dirname "$template_path")"
  cat >"$template_path" <<EOF
# Assemblywright final release evidence bundle template.
# Edit this file only after the signed/notarized distribution artifacts,
# live-device QA report and archive locations have
# been validated for the release candidate. Keep every validation flag false
# until the matching external check has actually completed.
#
# Usage:
#   set -a
#   source "$template_path"
#   set +a
#   ./scripts/release-evidence-bundle.sh --bundle

ASSEMBLYWRIGHT_EVIDENCE_VERSION="$VERSION"
ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$DIST_DIR"
ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$APP_PATH"
ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="$ZIP_PATH"
ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="$PKG_PATH"
ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$SIGNED_PROVENANCE_REPORT"
ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$LIVE_QA_REPORT"
ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$OUTPUT_PATH"
ASSEMBLYWRIGHT_EVIDENCE_OVERWRITE_OUTPUT=false
ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=true
ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_BUNDLE_ID="$EXPECTED_BUNDLE_ID"
ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_VERSION="$EXPECTED_VERSION"

ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=false
ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=false
ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=false
ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=false
ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=false

ASSEMBLYWRIGHT_EVIDENCE_OWNER_NAME=""
ASSEMBLYWRIGHT_EVIDENCE_COMPLETED_AT=""
ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_NOTE=""
ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_NOTE=""
ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_NOTE=""
ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_NOTE=""
ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_NOTE=""
ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_URI=""
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
  printf 'Assemblywright release evidence bundle env template written: %s\n' "$WRITE_TEMPLATE_PATH"
  printf 'Proof boundary: template generation only; no release evidence was validated or created.\n'
  exit 0
fi

if [[ "$SELF_TEST" == true ]]; then
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/assemblywright-release-evidence-self-test.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT
  self_test_zip="$tmp_dir/dist/Assemblywright-$VERSION.zip"
  self_test_pkg="$tmp_dir/dist/Assemblywright-$VERSION.pkg"
  mkdir -p "$tmp_dir/dist/Assemblywright.app/Contents/MacOS" "$tmp_dir/dist/Assemblywright.app/Contents/Resources/bin"
  cat >"$tmp_dir/dist/Assemblywright.app/Contents/Info.plist" <<XML
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.nobiletechnology.assemblywright</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
</dict>
</plist>
XML
  touch "$tmp_dir/dist/Assemblywright.app/Contents/MacOS/AssemblywrightMacApp"
  cat >"$tmp_dir/dist/Assemblywright.app/Contents/Resources/bin/assemblywright-cli" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  printf 'assemblywright $VERSION\n'
  exit 0
fi
printf 'self-test assemblywright-cli fixture\n'
EOF
  printf 'assemblywright %s\n' "$VERSION" >"$tmp_dir/dist/Assemblywright.app/Contents/Resources/bin/assemblywright-cli.version"
  chmod 755 "$tmp_dir/dist/Assemblywright.app/Contents/MacOS/AssemblywrightMacApp" "$tmp_dir/dist/Assemblywright.app/Contents/Resources/bin/assemblywright-cli"
  (cd "$tmp_dir/dist" && zip -qr "$self_test_zip" Assemblywright.app)
  touch "$self_test_pkg"
  self_test_zip_sha="$(file_sha256 "$self_test_zip")"
  self_test_pkg_sha="$(file_sha256 "$self_test_pkg")"
  self_test_app_executable_sha="$(file_sha256 "$tmp_dir/dist/Assemblywright.app/Contents/MacOS/AssemblywrightMacApp")"
  self_test_core_sha="$(file_sha256 "$tmp_dir/dist/Assemblywright.app/Contents/Resources/bin/assemblywright-cli")"
  cat >"$tmp_dir/app-zip-notarytool.log" <<'LOG'
id: 00000000-0000-4000-8000-000000000001
status: Accepted
LOG
  cat >"$tmp_dir/installer-pkg-notarytool.log" <<'LOG'
id: 00000000-0000-4000-8000-000000000002
status: Accepted
LOG
  self_test_app_notary_log_sha="$(file_sha256 "$tmp_dir/app-zip-notarytool.log")"
  self_test_pkg_notary_log_sha="$(file_sha256 "$tmp_dir/installer-pkg-notarytool.log")"
  cat >"$tmp_dir/live.json" <<JSON
{
  "schema_version": 1,
  "evidence_type": "owner_recorded_live_device_qa",
  "self_test_fixture": false,
  "generated_at": "2026-05-22T16:06:00Z",
  "installed_app_path": "/Applications/Assemblywright.app",
  "validation_flags": {
    "clean_profile": true,
    "finder_launch": true,
    "restart": true,
    "manual_release_qa": true
  },
  "owner_recorded_device_evidence": {
    "owner_name": "Assemblywright Release Self-Test",
    "device_label": "Self-test device",
    "profile_label": "Self-test profile",
    "device_check_started_at": "2026-05-22T16:00:00Z",
    "device_check_completed_at": "2026-05-22T16:05:00Z",
    "clean_profile_evidence_note": "Observed clean-profile install in the controlled release lane.",
    "finder_launch_evidence_note": "Observed Finder launch in the controlled release lane.",
    "restart_evidence_note": "Observed restart recovery in the controlled release lane.",
    "manual_release_qa_evidence_note": "Observed manual release QA surfaces in the controlled release lane."
  },
  "app_bundle": {
    "bundle_identifier": "com.nobiletechnology.assemblywright",
    "short_version": "$VERSION",
    "build_version": "$VERSION"
  },
  "app_executable": {
    "executable_path": "/Applications/Assemblywright.app/Contents/MacOS/AssemblywrightMacApp",
    "sha256": "$self_test_app_executable_sha",
    "code_identifier": "com.nobiletechnology.assemblywright",
    "team_identifier": "9VZ742YKV4",
    "cdhash": "0123456789abcdef0123456789abcdef01234567"
  },
  "bundled_core": {
    "executable_path": "/Applications/Assemblywright.app/Contents/Resources/bin/assemblywright-cli",
    "version": "assemblywright $VERSION",
    "sha256": "$self_test_core_sha"
  },
  "proof_boundary": "self-test fixture"
}
JSON
  cat >"$tmp_dir/signed-provenance.json" <<JSON
{
  "schema_version": 1,
  "evidence_type": "signed_distribution_provenance",
  "generated_at": "2026-05-22T16:40:00Z",
  "version": "$VERSION",
  "bundle_identifier": "com.nobiletechnology.assemblywright",
  "artifacts": {
    "app_path": "$tmp_dir/dist/Assemblywright.app",
    "zip_path": "$self_test_zip",
    "pkg_path": "$self_test_pkg",
    "zip_sha256": "$self_test_zip_sha",
    "pkg_sha256": "$self_test_pkg_sha",
    "app_executable_path": "$tmp_dir/dist/Assemblywright.app/Contents/MacOS/AssemblywrightMacApp",
    "app_executable_sha256": "$self_test_app_executable_sha",
    "bundled_core_path": "$tmp_dir/dist/Assemblywright.app/Contents/Resources/bin/assemblywright-cli",
    "bundled_core_sha256": "$self_test_core_sha",
    "bundled_core_version": "assemblywright $VERSION"
  },
  "signing": {
    "developer_id_application_identity": "Developer ID Application: Assemblywright QA Fixture",
    "developer_id_installer_identity": "Developer ID Installer: Assemblywright QA Fixture",
    "app_bundle_codesign": "Authority=Developer ID Application: Assemblywright QA Fixture",
    "app_executable_codesign": "Authority=Developer ID Application: Assemblywright QA Fixture",
    "app_executable_identifier": "com.nobiletechnology.assemblywright",
    "app_executable_team_identifier": "9VZ742YKV4",
    "app_executable_cdhash": "0123456789abcdef0123456789abcdef01234567",
    "bundled_core_codesign": "Authority=Developer ID Application: Assemblywright QA Fixture",
    "installer_pkg_signature": "Developer ID Installer: Assemblywright QA Fixture"
  },
  "notarization": {
    "app_zip_submission_id": "00000000-0000-4000-8000-000000000001",
    "installer_pkg_submission_id": "00000000-0000-4000-8000-000000000002",
    "app_zip_status": "Accepted",
    "installer_pkg_status": "Accepted",
    "app_zip_notary_log": "$tmp_dir/app-zip-notarytool.log",
    "installer_pkg_notary_log": "$tmp_dir/installer-pkg-notarytool.log",
    "app_zip_notary_log_sha256": "$self_test_app_notary_log_sha",
    "installer_pkg_notary_log_sha256": "$self_test_pkg_notary_log_sha"
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
    "artifact_digests_recorded": true,
    "app_executable_identity_recorded": true
  },
  "proof_boundary": "self-test fixture"
}
JSON
  self_test_signed_provenance_sha="$(file_sha256 "$tmp_dir/signed-provenance.json")"
  python3 - "$tmp_dir/live.json" "$tmp_dir/signed-provenance.json" "$self_test_signed_provenance_sha" <<'PY'
import json
import sys

live_path, provenance_path, provenance_sha = sys.argv[1:4]
with open(live_path, encoding="utf-8") as handle:
    data = json.load(handle)
data["signed_provenance"] = {
    "report_path": provenance_path,
    "sha256": provenance_sha,
}
with open(live_path, "w", encoding="utf-8") as handle:
    json.dump(data, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY

  "$0" --write-template "$tmp_dir/release-evidence-bundle.env" >/dev/null
  require_file "release evidence template" "$tmp_dir/release-evidence-bundle.env"
  for field in \
    ASSEMBLYWRIGHT_EVIDENCE_VERSION \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH \
    ASSEMBLYWRIGHT_EVIDENCE_OVERWRITE_OUTPUT \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES \
    ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_BUNDLE_ID \
    ASSEMBLYWRIGHT_EVIDENCE_EXPECTED_VERSION \
    ASSEMBLYWRIGHT_EVIDENCE_OWNER_NAME \
    ASSEMBLYWRIGHT_EVIDENCE_COMPLETED_AT \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_NOTE \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_NOTE \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_NOTE \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_NOTE \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_NOTE \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_URI; do
    require_file_contains "release evidence template" "$tmp_dir/release-evidence-bundle.env" "$field="
  done
  for flag in \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED; do
    require_file_contains "release evidence template" "$tmp_dir/release-evidence-bundle.env" "$flag=false"
    if grep -F "$flag=true" "$tmp_dir/release-evidence-bundle.env" >/dev/null 2>&1; then
      fail "release evidence template must not default $flag to true"
    fi
  done
  export ASSEMBLYWRIGHT_EVIDENCE_OWNER_NAME="Assemblywright Release Self-Test"
  export ASSEMBLYWRIGHT_EVIDENCE_COMPLETED_AT="2026-05-22T16:45:00Z"
  export ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_NOTE="Signed distribution provenance reviewed in the controlled release lane."
  export ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_NOTE="Notarization evidence reviewed in the controlled release lane."
  export ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_NOTE="Clean profile evidence reviewed in the controlled release lane."
  export ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_NOTE="Live-device QA evidence reviewed in the controlled release lane."
  export ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_NOTE=$'Release evidence reports archived in the controlled release lane.\nArchive reviewer noted "release archive index" and preserved backslash \\ marker.'
  export ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_URI="file:///Users/assemblywright/releases/evidence-archive"

  ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"reports_archived": true'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"schema_version": 1'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"evidence_type": "release_evidence_bundle"'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"local_signature_validation": false'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"zip_sha256"'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"signed_distribution_provenance_report"'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"signed_distribution_provenance_sha256"'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"live_device_qa_sha256"'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"owner_recorded_release_evidence"'
  require_json_contains "release evidence self-test bundle" "$tmp_dir/bundle.json" '"owner_name": "Assemblywright Release Self-Test"'
  python3 - "$tmp_dir/bundle.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    data = json.load(handle)
note = data["owner_recorded_release_evidence"]["reports_archive_note"]
if "\n" not in note or '"release archive index"' not in note or "\\ marker" not in note:
    raise SystemExit("release evidence self-test expected structured JSON writer to preserve multiline reports archive note")
PY

  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/temp-archive-uri-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_URI="file:///tmp/assemblywright/release-evidence" \
    "$0" --bundle >/dev/null 2>"$tmp_dir/temp-archive-uri.err"; then
    fail "release evidence self-test expected temporary reports archive URI to be rejected"
  fi
  require_file_contains "temporary reports archive URI error" "$tmp_dir/temp-archive-uri.err" "durable release evidence archive"

  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/bare-archive-uri-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_URI="release-evidence/archive" \
    "$0" --bundle >/dev/null 2>"$tmp_dir/bare-archive-uri.err"; then
    fail "release evidence self-test expected bare reports archive location to be rejected"
  fi
  require_file_contains "bare reports archive URI error" "$tmp_dir/bare-archive-uri.err" "must be a URI with a scheme"

  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OVERWRITE_OUTPUT=true \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>"$tmp_dir/output-collision.err"; then
    fail "release evidence self-test expected bundle output collision to be rejected"
  fi
  require_file_contains "bundle output collision error" "$tmp_dir/output-collision.err" "must not overwrite live-device QA report"

  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>"$tmp_dir/existing-output.err"; then
    fail "release evidence self-test expected existing bundle output to be rejected"
  fi
  require_file_contains "existing bundle output error" "$tmp_dir/existing-output.err" "release evidence bundle output already exists"

  ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OVERWRITE_OUTPUT=true \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null
  require_json_contains "release evidence self-test overwritten bundle" "$tmp_dir/bundle.json" '"evidence_type": "release_evidence_bundle"'

  python3 - "$tmp_dir/bundle.json" "$tmp_dir/placeholder-archive-bundle.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["owner_recorded_release_evidence"]["reports_archive_uri"] = "file://self-test/release-evidence"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if require_json_reports_archive_uri "release evidence self-test placeholder bundle" "$tmp_dir/placeholder-archive-bundle.json" "owner_recorded_release_evidence.reports_archive_uri" >/dev/null 2>"$tmp_dir/placeholder-archive.err"; then
    fail "release evidence self-test expected placeholder archive URI to be rejected"
  fi
  require_file_contains "placeholder archive URI error" "$tmp_dir/placeholder-archive.err" "durable release evidence archive"

  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/placeholder-note-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_NOTE="pending external archive" \
    "$0" --bundle >/dev/null 2>"$tmp_dir/placeholder-note.err"; then
    fail "release evidence self-test expected placeholder owner evidence note to be rejected"
  fi
  require_file_contains "placeholder owner evidence note error" "$tmp_dir/placeholder-note.err" "owner-recorded external evidence"

  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/embedded-fixture-note-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_NOTE="Signed distribution fixture was archived." \
    "$0" --bundle >/dev/null 2>"$tmp_dir/embedded-fixture-note.err"; then
    fail "release evidence self-test expected embedded fixture owner evidence note to be rejected"
  fi
  require_file_contains "embedded fixture owner evidence note error" "$tmp_dir/embedded-fixture-note.err" "owner-recorded external evidence"

  nested_zip="$tmp_dir/dist/nested-Assemblywright-$VERSION.zip"
  mkdir -p "$tmp_dir/nested/payload"
  cp -R "$tmp_dir/dist/Assemblywright.app" "$tmp_dir/nested/payload/Assemblywright.app"
  (cd "$tmp_dir/nested" && zip -qr "$nested_zip" payload/Assemblywright.app)
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="$nested_zip" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="$self_test_pkg" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/nested-zip-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected nested app zip payload to be rejected"
  fi

  python3 - "$tmp_dir/signed-provenance.json" "$tmp_dir/negated-gatekeeper-signed-provenance.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["gatekeeper"]["app_bundle_assessment"] = "not accepted"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/negated-gatekeeper-signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/negated-gatekeeper-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected negated Gatekeeper acceptance to be rejected"
  fi

  python3 - "$tmp_dir/signed-provenance.json" "$tmp_dir/pending-notary-signed-provenance.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["notarization"]["installer_pkg_status"] = "In Progress"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/pending-notary-signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/pending-notary-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected pending notary status to be rejected"
  fi

  printf 'assemblywright 0.0.0\n' >"$tmp_dir/dist/Assemblywright.app/Contents/Resources/bin/assemblywright-cli.version"
  stale_marker_output=""
  stale_marker_output="$(ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/stale-marker-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle 2>&1 >/dev/null || true)"
  if [[ "$stale_marker_output" != *"./scripts/package-distribution.sh --unsigned-launch-check"* ]]; then
    fail "release evidence self-test expected stale bundled core guidance to include package-distribution.sh --unsigned-launch-check"
  fi
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/stale-marker-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected stale bundled core version marker to be rejected"
  fi
  printf 'assemblywright %s\n' "$VERSION" >"$tmp_dir/dist/Assemblywright.app/Contents/Resources/bin/assemblywright-cli.version"

  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/forbidden-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
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
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/incomplete-live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/incomplete-live-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected incomplete live-device report to be rejected"
  fi

  python3 - "$tmp_dir/live.json" "$tmp_dir/mismatched-installed-app-live.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["installed_app_path"] = "/tmp/Assemblywright.app"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/mismatched-installed-app-live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/mismatched-installed-app-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected mismatched installed app path to be rejected"
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
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/pregenerated-live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/pregenerated-live-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected live report generated before completion to be rejected"
  fi

  python3 - "$tmp_dir/live.json" "$tmp_dir/mismatched-core-digest-live.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["bundled_core"]["sha256"] = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/mismatched-core-digest-live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/mismatched-core-digest-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected mismatched live bundled-core digest to be rejected"
  fi

  python3 - "$tmp_dir/live.json" "$tmp_dir/post-completion-live.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["generated_at"] = "2026-05-22T16:50:00Z"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/post-completion-live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/post-completion-live-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected child report generated after owner completion to be rejected"
  fi

  python3 - "$tmp_dir/signed-provenance.json" "$tmp_dir/stale-digest-signed-provenance.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["artifacts"]["zip_sha256"] = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/stale-digest-signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/stale-digest-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
	    fail "release evidence self-test expected stale signed provenance digest to be rejected"
	  fi

  python3 - "$tmp_dir/signed-provenance.json" "$tmp_dir/stale-notary-log-signed-provenance.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["notarization"]["app_zip_notary_log_sha256"] = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/stale-notary-log-signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/stale-notary-log-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected stale signed provenance notary log digest to be rejected"
  fi

  python3 - "$tmp_dir/signed-provenance.json" "$tmp_dir/bad-apple-tool-signed-provenance.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["signing"]["app_bundle_codesign"] = "Authority=Apple Development: Assemblywright QA Fixture"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if ASSEMBLYWRIGHT_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    ASSEMBLYWRIGHT_EVIDENCE_APP_PATH="$tmp_dir/dist/Assemblywright.app" \
    ASSEMBLYWRIGHT_EVIDENCE_ZIP_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_PKG_PATH="" \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/bad-apple-tool-signed-provenance.json" \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    ASSEMBLYWRIGHT_EVIDENCE_OUTPUT_PATH="$tmp_dir/bad-apple-tool-bundle.json" \
    ASSEMBLYWRIGHT_EVIDENCE_SELF_TEST_MODE=true \
    ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=false \
    ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true \
    ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED=true \
    "$0" --bundle >/dev/null 2>&1; then
    fail "release evidence self-test expected non-Developer-ID signed provenance evidence to be rejected"
  fi

  check_output="$("$0" --check)"
  case "$check_output" in
    *"--write-template target/release-evidence-bundle.env"* )
      ;;
    *)
      fail "release evidence --check output must point operators to the fillable env template"
      ;;
  esac
  case "$check_output" in
    *"sourceable final-bundle environment file"* )
      ;;
    *)
      fail "release evidence --check output must describe the sourceable final-bundle template"
      ;;
  esac
  case "$check_output" in
    *"set -a && source target/release-evidence-bundle.env && set +a"* )
      ;;
    *)
      fail "release evidence --check output must include the source command"
      ;;
  esac
  case "$check_output" in
    *"./scripts/release-evidence-bundle.sh --bundle"* )
      ;;
    *)
      fail "release evidence --check output must include the bundle command"
      ;;
  esac
  case "$check_output" in
    *"./scripts/release-evidence-doctor.sh --assert-complete"* )
      ;;
    *)
      fail "release evidence --check output must include the final doctor assertion command"
      ;;
  esac

  printf 'Assemblywright release evidence bundle self-test: ok\n'
  printf 'Proof boundary: fake artifacts and reports validate bundle mechanics only; no production evidence was created.\n'
  exit 0
fi

if [[ "$CHECK_ONLY" == true ]]; then
  require_file "package-distribution script" "$ROOT_DIR/scripts/package-distribution.sh"
  require_file "live-device QA script" "$ROOT_DIR/scripts/release-live-device-qa.sh"
  require_file "release checklist" "$ROOT_DIR/docs/release-checklist.md"
  printf 'Assemblywright release evidence bundle preflight: ok\n\n'
  cat <<'CHECKLIST'
Required before --bundle:
- App zip artifact path exists, with Developer ID signing/notarization validated separately.
- /Applications installer package path exists, with Developer ID signing/notarization validated separately.
- Signed-distribution provenance report generated by package-distribution.sh exists,
  binds version/bundle ID/artifact digests plus bundled core path/version/digest,
  and records signing/notary/staple/Gatekeeper evidence.
- Clean-profile install, Finder launch, restart, and manual release QA report
  exists.
- Marketplace review, malware scan, signed publisher policy, OS sandbox, and
  host-level egress evidence report exists.
- Owner sets every ASSEMBLYWRIGHT_EVIDENCE_* validation flag to true and fills the
  owner name, completion timestamp, evidence notes, and reports archive URI.
- After the matching external checks complete, generate and source the
  sourceable final-bundle environment file, then bundle it:
  ./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env
  set -a && source target/release-evidence-bundle.env && set +a
  ./scripts/release-evidence-bundle.sh --bundle
- Run the final evidence doctor assertion after bundle generation:
  ./scripts/release-evidence-doctor.sh --assert-complete
- The bundle command can locally verify app signing, app stapling, installer
  signature, installer stapling, and the app zip payload.

Proof boundary: preflight and runbook only; no production evidence was created.
CHECKLIST
  exit 0
fi

require_dir "app bundle path" "$APP_PATH"
require_app_bundle_metadata
require_bundled_core_version
require_file "app zip path" "$ZIP_PATH"
require_file "installer package path" "$PKG_PATH"
validate_zip_payload
require_file "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT"
require_json_number_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "schema_version" "1"
require_json_string_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "evidence_type" "signed_distribution_provenance"
require_json_string_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "version" "$EXPECTED_VERSION"
require_json_string_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "bundle_identifier" "$EXPECTED_BUNDLE_ID"
require_json_string_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.app_path" "$APP_PATH"
require_json_string_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.zip_path" "$ZIP_PATH"
require_json_string_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.pkg_path" "$PKG_PATH"
require_json_string_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.app_executable_path" "$APP_PATH/Contents/MacOS/AssemblywrightMacApp"
require_json_string_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.bundled_core_path" "$APP_PATH/Contents/Resources/bin/assemblywright-cli"
require_json_string_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.bundled_core_version" "assemblywright $EXPECTED_VERSION"
for field in artifacts.zip_sha256 artifacts.pkg_sha256 artifacts.app_executable_sha256 artifacts.bundled_core_sha256; do
  require_json_sha256 "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "$field"
done
require_json_sha256_matches_file "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.zip_sha256" "app zip artifact" "$ZIP_PATH"
require_json_sha256_matches_file "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.pkg_sha256" "installer package artifact" "$PKG_PATH"
require_json_sha256_matches_file "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.app_executable_sha256" "app executable" "$APP_PATH/Contents/MacOS/AssemblywrightMacApp"
require_json_sha256_matches_file "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.bundled_core_sha256" "bundled core executable" "$APP_PATH/Contents/Resources/bin/assemblywright-cli"
for flag in developer_id_application_signed developer_id_installer_signed app_zip_notarized installer_pkg_notarized app_stapled installer_pkg_stapled gatekeeper_assessed artifact_digests_recorded app_executable_identity_recorded; do
  require_json_bool_true "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "validation_flags.$flag"
done
for field in notarization.app_zip_notary_log notarization.installer_pkg_notary_log; do
  require_json_nonempty_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "$field"
done
for field in notarization.app_zip_notary_log_sha256 notarization.installer_pkg_notary_log_sha256; do
  require_json_sha256 "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "$field"
done
require_json_sha256_matches_json_path "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "notarization.app_zip_notary_log_sha256" "app zip notary log" "notarization.app_zip_notary_log"
require_json_sha256_matches_json_path "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "notarization.installer_pkg_notary_log_sha256" "installer package notary log" "notarization.installer_pkg_notary_log"
require_json_string_prefix "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "signing.developer_id_application_identity" "Developer ID Application: "
require_json_string_prefix "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "signing.developer_id_installer_identity" "Developer ID Installer: "
for field in signing.app_bundle_codesign signing.app_executable_codesign signing.bundled_core_codesign; do
  require_json_string_contains "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "$field" "Authority=Developer ID Application: "
done
require_json_app_code_identity "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" \
  "signing.app_executable_identifier" "signing.app_executable_team_identifier" \
  "signing.app_executable_cdhash" "$EXPECTED_BUNDLE_ID"
require_json_string_contains "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "signing.installer_pkg_signature" "Developer ID Installer: "
require_json_uuid "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "notarization.app_zip_submission_id"
require_json_uuid "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "notarization.installer_pkg_submission_id"
require_json_string_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "notarization.app_zip_status" "Accepted"
require_json_string_equals "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "notarization.installer_pkg_status" "Accepted"
require_json_string_contains "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "stapling.app_bundle_validation" "The validate action worked!"
require_json_string_contains "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "stapling.installer_pkg_validation" "The validate action worked!"
require_json_gatekeeper_accepted "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "gatekeeper.app_bundle_assessment"
require_json_gatekeeper_accepted "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "gatekeeper.installer_pkg_assessment"
require_json_utc_timestamp "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "generated_at"
require_output_write_mode
require_production_signature_validation
validate_local_distribution_evidence
for flag in clean_profile finder_launch restart manual_release_qa; do
  require_json_bool_true "live-device QA report" "$LIVE_QA_REPORT" "validation_flags.$flag"
done
for field in owner_name device_label profile_label device_check_started_at device_check_completed_at; do
  require_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_device_evidence.$field"
done
for field in clean_profile_evidence_note finder_launch_evidence_note restart_evidence_note manual_release_qa_evidence_note; do
  require_json_meaningful_evidence_string "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_device_evidence.$field"
done
require_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_device_evidence.device_check_started_at"
require_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_device_evidence.device_check_completed_at"
require_json_timestamp_order "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_device_evidence.device_check_started_at" "owner_recorded_device_evidence.device_check_completed_at"
require_json_timestamp_order "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_device_evidence.device_check_completed_at" "generated_at"
require_json_string_equals "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.bundle_identifier" "$EXPECTED_BUNDLE_ID"
require_json_string_equals "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.short_version" "$EXPECTED_VERSION"
require_json_string_equals "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.build_version" "$EXPECTED_VERSION"
require_json_string_equals "live-device QA report" "$LIVE_QA_REPORT" "app_executable.executable_path" "$EXPECTED_INSTALLED_APP_PATH/Contents/MacOS/AssemblywrightMacApp"
require_json_sha256 "live-device QA report" "$LIVE_QA_REPORT" "app_executable.sha256"
require_json_app_code_identity "live-device QA report" "$LIVE_QA_REPORT" \
  "app_executable.code_identifier" "app_executable.team_identifier" \
  "app_executable.cdhash" "$EXPECTED_BUNDLE_ID"
require_json_string_equals "live-device QA report" "$LIVE_QA_REPORT" "signed_provenance.report_path" "$SIGNED_PROVENANCE_REPORT"
require_json_sha256 "live-device QA report" "$LIVE_QA_REPORT" "signed_provenance.sha256"
require_json_sha256_matches_file "live-device QA report" "$LIVE_QA_REPORT" "signed_provenance.sha256" "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT"
require_json_string_equals "live-device QA report" "$LIVE_QA_REPORT" "bundled_core.executable_path" "$EXPECTED_INSTALLED_APP_PATH/Contents/Resources/bin/assemblywright-cli"
require_json_string_equals "live-device QA report" "$LIVE_QA_REPORT" "bundled_core.version" "assemblywright $EXPECTED_VERSION"
require_json_sha256 "live-device QA report" "$LIVE_QA_REPORT" "bundled_core.sha256"
require_json_fields_equal "live-device bundled-core digest" "$LIVE_QA_REPORT" "bundled_core.sha256" "$SIGNED_PROVENANCE_REPORT" "artifacts.bundled_core_sha256"
require_json_fields_equal "live-device app-executable digest" "$LIVE_QA_REPORT" "app_executable.sha256" "$SIGNED_PROVENANCE_REPORT" "artifacts.app_executable_sha256"
require_json_fields_equal "live-device app-executable identifier" "$LIVE_QA_REPORT" "app_executable.code_identifier" "$SIGNED_PROVENANCE_REPORT" "signing.app_executable_identifier"
require_json_fields_equal "live-device app-executable team identifier" "$LIVE_QA_REPORT" "app_executable.team_identifier" "$SIGNED_PROVENANCE_REPORT" "signing.app_executable_team_identifier"
require_json_fields_equal "live-device app-executable CDHash" "$LIVE_QA_REPORT" "app_executable.cdhash" "$SIGNED_PROVENANCE_REPORT" "signing.app_executable_cdhash"
require_true ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED
require_true ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_VALIDATED
require_true ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_VALIDATED
require_true ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_VALIDATED
require_true ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVED
require_non_empty_env ASSEMBLYWRIGHT_EVIDENCE_OWNER_NAME
require_not_future_timestamp_env ASSEMBLYWRIGHT_EVIDENCE_COMPLETED_AT
require_meaningful_evidence_env ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_NOTE
require_meaningful_evidence_env ASSEMBLYWRIGHT_EVIDENCE_NOTARIZATION_NOTE
require_meaningful_evidence_env ASSEMBLYWRIGHT_EVIDENCE_CLEAN_PROFILE_NOTE
require_meaningful_evidence_env ASSEMBLYWRIGHT_EVIDENCE_LIVE_DEVICE_QA_NOTE
require_meaningful_evidence_env ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_NOTE
require_reports_archive_uri_env ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_URI
write_bundle
require_json_number_equals "release evidence bundle" "$OUTPUT_PATH" "schema_version" "1"
require_json_string_equals "release evidence bundle" "$OUTPUT_PATH" "evidence_type" "release_evidence_bundle"
for field in owner_name completed_at reports_archive_uri; do
  require_json_nonempty_string "release evidence bundle" "$OUTPUT_PATH" "owner_recorded_release_evidence.$field"
done
for field in signed_distribution_note notarization_note clean_profile_note live_device_qa_note reports_archive_note; do
  require_json_meaningful_evidence_string "release evidence bundle" "$OUTPUT_PATH" "owner_recorded_release_evidence.$field"
done
require_json_reports_archive_uri "release evidence bundle" "$OUTPUT_PATH" "owner_recorded_release_evidence.reports_archive_uri"
require_json_utc_timestamp "release evidence bundle" "$OUTPUT_PATH" "owner_recorded_release_evidence.completed_at"
require_json_timestamp_order "release evidence bundle" "$OUTPUT_PATH" "owner_recorded_release_evidence.completed_at" "generated_at"
require_json_timestamp_between_reports "release evidence bundle signed provenance" "$SIGNED_PROVENANCE_REPORT" "generated_at" "$OUTPUT_PATH" "owner_recorded_release_evidence.completed_at" "$OUTPUT_PATH" "generated_at"
require_json_timestamp_between_reports "release evidence bundle live-device QA" "$LIVE_QA_REPORT" "generated_at" "$OUTPUT_PATH" "owner_recorded_release_evidence.completed_at" "$OUTPUT_PATH" "generated_at"

cat <<EOF
Assemblywright release evidence bundle: complete
Bundle: $OUTPUT_PATH
Proof boundary: evidence manifest only; production readiness still depends on
the external evidence referenced by the artifact paths, QA reports, and owner
validation flags.
EOF
