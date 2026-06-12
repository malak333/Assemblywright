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
EXPECTED_INSTALLED_APP_PATH="${JARVIS_QA_INSTALLED_APP_PATH:-/Applications/Jarvis.app}"
EVIDENCE_STATUS_ENDPOINT="${JARVIS_EVIDENCE_STATUS_ENDPOINT:-}"

CHECK_ONLY=false
ASSERT_COMPLETE=false
SELF_TEST=false
MISSING_ITEMS=()
SATISFIED_ITEMS=()
SIGNED_PROVENANCE_REPORT_VALID=false
LIVE_QA_REPORT_VALID=false
PLUGIN_QA_REPORT_VALID=false

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
  JARVIS_QA_INSTALLED_APP_PATH        Defaults to /Applications/Jarvis.app and must match the live QA report
  JARVIS_EVIDENCE_STATUS_ENDPOINT     Optional jarvis serve endpoint for repository-backed --assert-complete parity

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
  package preflight: ./scripts/package-distribution.sh --check
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

json_command_result_evidence_id() {
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

pattern = re.compile(
    r"^(task|audit):[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
)
raise SystemExit(0 if isinstance(cursor, str) and pattern.fullmatch(cursor.strip()) else 1)
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

json_string_contains() {
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

raise SystemExit(0 if isinstance(cursor, str) and expected in cursor else 1)
PY
}

json_gatekeeper_accepted() {
  local path="$1"
  local dotted_key="$2"
  python3 - "$path" "$dotted_key" <<'PY'
import json
import re
import sys

path, dotted_key = sys.argv[1:3]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

cursor = data
for segment in dotted_key.split("."):
    if not isinstance(cursor, dict) or segment not in cursor:
        raise SystemExit(1)
    cursor = cursor[segment]

if not isinstance(cursor, str):
    raise SystemExit(1)

lines = [line.strip() for line in cursor.splitlines() if line.strip()]
if not any(line == "accepted" or re.search(r":\s*accepted$", line) for line in lines):
    raise SystemExit(1)
PY
}

json_string_prefix() {
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

raise SystemExit(0 if isinstance(cursor, str) and cursor.startswith(expected) else 1)
PY
}

json_uuid_string() {
  local path="$1"
  local dotted_key="$2"
  python3 - "$path" "$dotted_key" <<'PY'
import json
import sys
import uuid

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

try:
    parsed = uuid.UUID(cursor.strip()) if isinstance(cursor, str) else uuid.UUID("")
except Exception:
    raise SystemExit(1)

raise SystemExit(0 if parsed.int != 0 else 1)
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

file_sha256() {
  local path="$1"
  shasum -a 256 "$path" | awk '{print $1}'
}

json_sha256_matches_file() {
  local path="$1"
  local dotted_key="$2"
  local artifact_path="$3"
  local expected_sha
  [[ -f "$artifact_path" ]] || return 1
  expected_sha="$(file_sha256 "$artifact_path")"
  python3 - "$path" "$dotted_key" "$expected_sha" <<'PY'
import json
import sys

path, dotted_key, expected_sha = sys.argv[1:4]
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

raise SystemExit(0 if cursor == expected_sha else 1)
PY
}

json_utc_timestamp() {
  local path="$1"
  local dotted_key="$2"
  python3 - "$path" "$dotted_key" <<'PY'
from datetime import datetime, timezone
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
    timestamp = datetime.fromisoformat(cursor.replace("Z", "+00:00"))
except ValueError:
    raise SystemExit(1)
if timestamp > datetime.now(timezone.utc):
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

zip_payload_valid() {
  local zip_path="$1"
  python3 - "$zip_path" <<'PY'
import sys
import zipfile

zip_path = sys.argv[1]
required_entries = (
    "Jarvis.app/Contents/MacOS/JarvisMacApp",
    "Jarvis.app/Contents/Resources/bin/jarvis-cli",
    "Jarvis.app/Contents/Info.plist",
)

try:
    with zipfile.ZipFile(zip_path) as archive:
        names = archive.namelist()
except Exception:
    raise SystemExit(1)

if any(entry not in names for entry in required_entries):
    raise SystemExit(1)
if any("/Jarvis.app/" in name and not name.startswith("Jarvis.app/") for name in names):
    raise SystemExit(1)

app_roots = {
    name.split("Jarvis.app/", 1)[0] + "Jarvis.app/"
    for name in names
    if "Jarvis.app/" in name
}
raise SystemExit(0 if app_roots == {"Jarvis.app/"} else 1)
PY
}

json_fields_equal_across_files() {
  local left_path="$1"
  local left_key="$2"
  local right_path="$3"
  local right_key="$4"
  python3 - "$left_path" "$left_key" "$right_path" "$right_key" <<'PY'
import json
import sys

left_path, left_key, right_path, right_key = sys.argv[1:5]

def load(path):
    try:
        with open(path, encoding="utf-8") as handle:
            return json.load(handle)
    except Exception:
        raise SystemExit(1)

def get(data, dotted_key):
    cursor = data
    for segment in dotted_key.split("."):
        if not isinstance(cursor, dict) or segment not in cursor:
            raise SystemExit(1)
        cursor = cursor[segment]
    return cursor

raise SystemExit(0 if get(load(left_path), left_key) == get(load(right_path), right_key) else 1)
PY
}

json_timestamp_between_reports() {
  local lower_path="$1"
  local lower_key="$2"
  local value_path="$3"
  local value_key="$4"
  local upper_path="$5"
  local upper_key="$6"
  python3 - "$lower_path" "$lower_key" "$value_path" "$value_key" "$upper_path" "$upper_key" <<'PY'
from datetime import datetime
import json
import sys

lower_path, lower_key, value_path, value_key, upper_path, upper_key = sys.argv[1:7]

def load(path):
    try:
        with open(path, encoding="utf-8") as handle:
            return json.load(handle)
    except Exception:
        raise SystemExit(1)

def get(data, dotted_key):
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

lower = get(load(lower_path), lower_key)
value = get(load(value_path), value_key)
upper = get(load(upper_path), upper_key)
raise SystemExit(0 if lower <= value <= upper else 1)
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

check_zip_payload() {
  local label="$1"
  local path="$2"

  if [[ ! -f "$path" ]]; then
    return
  fi

  if zip_payload_valid "$path"; then
    record_satisfied "$label payload has exactly one top-level Jarvis.app"
  else
    record_missing "$label payload invalid: expected exactly one top-level Jarvis.app with Info.plist, app executable, and bundled core"
  fi
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

check_json_fields_equal_across_files() {
  local label="$1"
  local left_path="$2"
  local left_key="$3"
  local right_path="$4"
  local right_key="$5"

  if json_fields_equal_across_files "$left_path" "$left_key" "$right_path" "$right_key"; then
    record_satisfied "$label: $left_key matches $right_key"
  else
    record_missing "$label mismatch: $left_key in $left_path must match $right_key in $right_path"
  fi
}

check_json_command_result_evidence_id() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"

  if json_command_result_evidence_id "$path" "$dotted_key"; then
    record_satisfied "$label: $dotted_key is a task/audit UUID reference"
  else
    record_missing "$label invalid evidence reference: $dotted_key must be task:<uuid> or audit:<uuid> in $path"
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

check_json_string_prefix() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  local expected="$4"

  if json_string_prefix "$path" "$dotted_key" "$expected"; then
    record_satisfied "$label: $dotted_key starts with $expected"
  else
    record_missing "$label semantic mismatch: $dotted_key must start with $expected in $path"
  fi
}

check_json_string_contains() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  local expected="$4"

  if json_string_contains "$path" "$dotted_key" "$expected"; then
    record_satisfied "$label: $dotted_key includes $expected"
  else
    record_missing "$label semantic mismatch: $dotted_key must include $expected in $path"
  fi
}

check_json_gatekeeper_accepted() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"

  if json_gatekeeper_accepted "$path" "$dotted_key"; then
    record_satisfied "$label: $dotted_key has exact Gatekeeper accepted evidence"
  else
    record_missing "$label semantic mismatch: $dotted_key must include an exact Gatekeeper accepted result in $path"
  fi
}

check_json_uuid() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"

  if json_uuid_string "$path" "$dotted_key"; then
    record_satisfied "$label: $dotted_key is a non-nil UUID"
  else
    record_missing "$label invalid UUID field: $dotted_key in $path"
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

check_json_sha256_matches_file() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"
  local artifact_label="$4"
  local artifact_path="$5"

  if json_sha256_matches_file "$path" "$dotted_key" "$artifact_path"; then
    record_satisfied "$label: $dotted_key matches current $artifact_label"
  else
    record_missing "$label digest mismatch: $dotted_key must match current $artifact_label in $path"
  fi
}

check_json_utc_timestamp() {
  local label="$1"
  local path="$2"
  local dotted_key="$3"

  if json_utc_timestamp "$path" "$dotted_key"; then
    record_satisfied "$label: $dotted_key is UTC and not future-dated"
  else
    record_missing "$label invalid or future UTC timestamp: $dotted_key in $path"
  fi
}

plist_string_value() {
  local path="$1"
  local key="$2"
  python3 - "$path" "$key" <<'PY'
import plistlib
import sys

path, key = sys.argv[1:3]
try:
    with open(path, "rb") as handle:
        data = plistlib.load(handle)
except Exception:
    raise SystemExit(1)

value = data.get(key)
if not isinstance(value, str) or not value:
    raise SystemExit(1)
print(value)
PY
}

check_app_bundle_metadata() {
  if [[ ! -d "$APP_PATH" ]]; then
    return 0
  fi
  local info_plist="$APP_PATH/Contents/Info.plist"
  if [[ ! -f "$info_plist" ]]; then
    record_missing "app bundle Info.plist missing: $info_plist"
    return
  fi

  local bundle_id short_version build_version
  bundle_id="$(plist_string_value "$info_plist" CFBundleIdentifier || true)"
  short_version="$(plist_string_value "$info_plist" CFBundleShortVersionString || true)"
  build_version="$(plist_string_value "$info_plist" CFBundleVersion || true)"

  if [[ "$bundle_id" == "$EXPECTED_BUNDLE_ID" ]]; then
    record_satisfied "app bundle Info.plist CFBundleIdentifier matches expected bundle id"
  else
    record_missing "app bundle Info.plist CFBundleIdentifier mismatch: expected $EXPECTED_BUNDLE_ID, got ${bundle_id:-<missing>}"
  fi
  if [[ "$short_version" == "$EXPECTED_VERSION" ]]; then
    record_satisfied "app bundle Info.plist CFBundleShortVersionString matches expected version"
  else
    record_missing "app bundle Info.plist CFBundleShortVersionString mismatch: expected $EXPECTED_VERSION, got ${short_version:-<missing>}"
  fi
  if [[ "$build_version" == "$EXPECTED_VERSION" ]]; then
    record_satisfied "app bundle Info.plist CFBundleVersion matches expected version"
  else
    record_missing "app bundle Info.plist CFBundleVersion mismatch: expected $EXPECTED_VERSION, got ${build_version:-<missing>}"
  fi
}

check_bundled_core_version() {
  local core_path="$APP_PATH/Contents/Resources/bin/jarvis-cli"
  local marker_path="$core_path.version"
  local remediation="rerun ./scripts/package-distribution.sh --unsigned-launch-check for local evidence, or the signed package-distribution.sh lane before final release evidence"
  if [[ ! -x "$core_path" ]]; then
    return 0
  fi

  if [[ -f "$marker_path" ]] && [[ "$(tr -d '\r\n' <"$marker_path")" == "jarvis $EXPECTED_VERSION" ]]; then
    record_satisfied "bundled core version marker matches expected version"
  else
    record_missing "bundled core version marker mismatch: expected jarvis $EXPECTED_VERSION from $marker_path; $remediation"
  fi

  if [[ "$ASSERT_COMPLETE" != true ]]; then
    return 0
  fi

  local output
  if output="$("$core_path" --version 2>&1)" && [[ "$output" == *"jarvis $EXPECTED_VERSION"* ]]; then
    record_satisfied "bundled core --version matches expected version"
  else
    record_missing "bundled core --version mismatch: expected jarvis $EXPECTED_VERSION from $core_path; $remediation"
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

check_json_timestamp_between_reports() {
  local label="$1"
  local lower_path="$2"
  local lower_key="$3"
  local value_path="$4"
  local value_key="$5"
  local upper_path="$6"
  local upper_key="$7"

  if json_timestamp_between_reports "$lower_path" "$lower_key" "$value_path" "$value_key" "$upper_path" "$upper_key"; then
    record_satisfied "$label: $lower_key <= $value_key <= $upper_key"
  else
    record_missing "$label timestamp order invalid: $value_key must be after $lower_key and before $upper_key"
  fi
}

check_release_evidence() {
  check_path "app bundle path" "$APP_PATH" dir
  check_app_bundle_metadata
  check_path "app executable" "$APP_PATH/Contents/MacOS/JarvisMacApp" executable
  check_path "bundled core executable" "$APP_PATH/Contents/Resources/bin/jarvis-cli" executable
  check_bundled_core_version
  check_path "app zip path" "$ZIP_PATH" file
  check_zip_payload "app zip" "$ZIP_PATH"
  check_path "installer package path" "$PKG_PATH" file

  if valid_json_file "$SIGNED_PROVENANCE_REPORT"; then
    record_satisfied "signed-distribution provenance report JSON: $SIGNED_PROVENANCE_REPORT"
    local missing_before_signed="${#MISSING_ITEMS[@]}"
    check_json_number "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "schema_version" "1"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "evidence_type" "signed_distribution_provenance"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "version" "$EXPECTED_VERSION"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "bundle_identifier" "$EXPECTED_BUNDLE_ID"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.app_path" "$APP_PATH"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.zip_path" "$ZIP_PATH"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.pkg_path" "$PKG_PATH"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.bundled_core_path" "$APP_PATH/Contents/Resources/bin/jarvis-cli"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.bundled_core_version" "jarvis $EXPECTED_VERSION"
    for field in artifacts.zip_sha256 artifacts.pkg_sha256 artifacts.bundled_core_sha256; do
      check_json_sha256 "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "$field"
    done
    check_json_sha256_matches_file "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.zip_sha256" "app zip artifact" "$ZIP_PATH"
    check_json_sha256_matches_file "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.pkg_sha256" "installer package artifact" "$PKG_PATH"
    check_json_sha256_matches_file "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "artifacts.bundled_core_sha256" "bundled core executable" "$APP_PATH/Contents/Resources/bin/jarvis-cli"
    for flag in developer_id_application_signed developer_id_installer_signed app_zip_notarized installer_pkg_notarized app_stapled installer_pkg_stapled gatekeeper_assessed artifact_digests_recorded; do
      check_json_flag "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "validation_flags.$flag"
    done
    for field in notarization.app_zip_notary_log notarization.installer_pkg_notary_log; do
      check_json_nonempty_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "$field"
    done
    check_json_string_prefix "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "signing.developer_id_application_identity" "Developer ID Application: "
    check_json_string_prefix "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "signing.developer_id_installer_identity" "Developer ID Installer: "
    for field in signing.app_bundle_codesign signing.app_executable_codesign signing.bundled_core_codesign; do
      check_json_string_contains "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "$field" "Authority=Developer ID Application: "
    done
    check_json_string_contains "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "signing.installer_pkg_signature" "Developer ID Installer: "
    check_json_uuid "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "notarization.app_zip_submission_id"
    check_json_uuid "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "notarization.installer_pkg_submission_id"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "notarization.app_zip_status" "Accepted"
    check_json_string "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "notarization.installer_pkg_status" "Accepted"
    check_json_string_contains "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "stapling.app_bundle_validation" "The validate action worked!"
    check_json_string_contains "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "stapling.installer_pkg_validation" "The validate action worked!"
    check_json_gatekeeper_accepted "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "gatekeeper.app_bundle_assessment"
    check_json_gatekeeper_accepted "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "gatekeeper.installer_pkg_assessment"
    check_json_utc_timestamp "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT" "generated_at"
    if [[ "${#MISSING_ITEMS[@]}" -eq "$missing_before_signed" ]]; then
      SIGNED_PROVENANCE_REPORT_VALID=true
    fi
  else
    record_missing "signed-distribution provenance report missing or invalid JSON: $SIGNED_PROVENANCE_REPORT"
  fi

  if valid_json_file "$LIVE_QA_REPORT"; then
    record_satisfied "live-device QA report JSON: $LIVE_QA_REPORT"
    local missing_before_live="${#MISSING_ITEMS[@]}"
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
    check_json_string "live-device QA report" "$LIVE_QA_REPORT" "installed_app_path" "$EXPECTED_INSTALLED_APP_PATH"
    for field in owner_name device_label profile_label voice_check_started_at voice_check_completed_at microphone_evidence_note speech_permission_evidence_note transcript_handoff_evidence_note audio_output_evidence_note; do
      check_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.$field"
    done
    for field in clean_profile_evidence_note finder_launch_evidence_note notification_evidence_note notification_observed_at restart_evidence_note manual_release_qa_evidence_note; do
      check_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_non_voice_evidence.$field"
    done
    check_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_started_at"
    check_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_completed_at"
    check_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_non_voice_evidence.notification_observed_at"
    check_json_timestamp_order "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_started_at" "owner_recorded_live_voice_evidence.voice_check_completed_at"
    check_json_timestamp_order "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_started_at" "owner_recorded_non_voice_evidence.notification_observed_at"
    check_json_utc_timestamp "live-device QA report" "$LIVE_QA_REPORT" "generated_at"
    check_json_timestamp_order "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_live_voice_evidence.voice_check_completed_at" "generated_at"
    check_json_timestamp_order "live-device QA report" "$LIVE_QA_REPORT" "owner_recorded_non_voice_evidence.notification_observed_at" "generated_at"
    for field in test_phrase observed_transcript expected_command_text observed_command_text command_result_evidence_id audio_output_device_label; do
      check_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "voice_command_observation.$field"
    done
    check_json_string_fields_equal "live-device QA report" "$LIVE_QA_REPORT" "voice_command_observation.test_phrase" "voice_command_observation.observed_transcript"
    check_json_string_fields_equal "live-device QA report" "$LIVE_QA_REPORT" "voice_command_observation.expected_command_text" "voice_command_observation.observed_command_text"
    check_json_command_result_evidence_id "live-device QA report" "$LIVE_QA_REPORT" "voice_command_observation.command_result_evidence_id"
    check_json_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.bundle_identifier" "$EXPECTED_BUNDLE_ID"
    check_json_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.short_version" "$EXPECTED_VERSION"
    check_json_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.build_version" "$EXPECTED_VERSION"
    check_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.microphone_usage_description"
    check_json_nonempty_string "live-device QA report" "$LIVE_QA_REPORT" "app_bundle.speech_recognition_usage_description"
    check_json_string "live-device QA report" "$LIVE_QA_REPORT" "bundled_core.executable_path" "$EXPECTED_INSTALLED_APP_PATH/Contents/Resources/bin/jarvis-cli"
    check_json_string "live-device QA report" "$LIVE_QA_REPORT" "bundled_core.version" "jarvis $EXPECTED_VERSION"
    check_json_sha256 "live-device QA report" "$LIVE_QA_REPORT" "bundled_core.sha256"
    if valid_json_file "$SIGNED_PROVENANCE_REPORT"; then
      check_json_fields_equal_across_files "live-device bundled-core digest" "$LIVE_QA_REPORT" "bundled_core.sha256" "$SIGNED_PROVENANCE_REPORT" "artifacts.bundled_core_sha256"
    fi
    if [[ "${#MISSING_ITEMS[@]}" -eq "$missing_before_live" ]]; then
      LIVE_QA_REPORT_VALID=true
    fi
  else
    record_missing "live-device QA report missing or invalid JSON: $LIVE_QA_REPORT"
  fi

  if valid_json_file "$PLUGIN_QA_REPORT"; then
    record_satisfied "plugin-trust QA report JSON: $PLUGIN_QA_REPORT"
    local missing_before_plugin="${#MISSING_ITEMS[@]}"
    check_json_number "plugin-trust QA report" "$PLUGIN_QA_REPORT" "schema_version" "1"
    check_json_string "plugin-trust QA report" "$PLUGIN_QA_REPORT" "evidence_type" "owner_recorded_plugin_trust_qa"
    check_json_false_flag "plugin-trust QA report" "$PLUGIN_QA_REPORT" "self_test_fixture"
    check_json_string "plugin-trust QA report" "$PLUGIN_QA_REPORT" "review_source" "owner-asserted-manual-review"
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
    if [[ "${#MISSING_ITEMS[@]}" -eq "$missing_before_plugin" ]]; then
      PLUGIN_QA_REPORT_VALID=true
    fi
  else
    record_missing "plugin-trust QA report missing or invalid JSON: $PLUGIN_QA_REPORT"
  fi

  if valid_json_file "$BUNDLE_PATH"; then
    record_satisfied "release evidence bundle JSON: $BUNDLE_PATH"
    check_json_number "release evidence bundle" "$BUNDLE_PATH" "schema_version" "1"
    check_json_string "release evidence bundle" "$BUNDLE_PATH" "evidence_type" "release_evidence_bundle"
    for flag in signed_distribution notarization clean_profile live_device_qa plugin_trust_qa reports_archived; do
      check_json_flag "release evidence bundle" "$BUNDLE_PATH" "validation_flags.$flag"
    done
    check_json_flag "release evidence bundle" "$BUNDLE_PATH" "validation_flags.local_signature_validation"
    check_json_utc_timestamp "release evidence bundle" "$BUNDLE_PATH" "generated_at"
    check_json_string "release evidence bundle" "$BUNDLE_PATH" "version" "$VERSION"
    check_json_string "release evidence bundle" "$BUNDLE_PATH" "artifacts.app_path" "$APP_PATH"
    check_json_string "release evidence bundle" "$BUNDLE_PATH" "artifacts.zip_path" "$ZIP_PATH"
    check_json_string "release evidence bundle" "$BUNDLE_PATH" "artifacts.pkg_path" "$PKG_PATH"
    check_json_string "release evidence bundle" "$BUNDLE_PATH" "reports.signed_distribution_provenance_report" "$SIGNED_PROVENANCE_REPORT"
    check_json_string "release evidence bundle" "$BUNDLE_PATH" "reports.live_device_qa_report" "$LIVE_QA_REPORT"
    check_json_string "release evidence bundle" "$BUNDLE_PATH" "reports.plugin_trust_qa_report" "$PLUGIN_QA_REPORT"
    for field in artifacts.app_path artifacts.zip_path artifacts.pkg_path reports.signed_distribution_provenance_report reports.live_device_qa_report reports.plugin_trust_qa_report; do
      check_json_nonempty_string "release evidence bundle" "$BUNDLE_PATH" "$field"
    done
    for field in owner_name completed_at signed_distribution_note notarization_note clean_profile_note live_device_qa_note plugin_trust_qa_note reports_archive_note reports_archive_uri; do
      check_json_nonempty_string "release evidence bundle" "$BUNDLE_PATH" "owner_recorded_release_evidence.$field"
    done
    check_json_utc_timestamp "release evidence bundle" "$BUNDLE_PATH" "owner_recorded_release_evidence.completed_at"
    check_json_timestamp_order "release evidence bundle" "$BUNDLE_PATH" "owner_recorded_release_evidence.completed_at" "generated_at"
    if valid_json_file "$SIGNED_PROVENANCE_REPORT"; then
      check_json_timestamp_between_reports "release evidence bundle signed provenance" "$SIGNED_PROVENANCE_REPORT" "generated_at" "$BUNDLE_PATH" "owner_recorded_release_evidence.completed_at" "$BUNDLE_PATH" "generated_at"
    fi
    if valid_json_file "$LIVE_QA_REPORT"; then
      check_json_timestamp_between_reports "release evidence bundle live-device QA" "$LIVE_QA_REPORT" "generated_at" "$BUNDLE_PATH" "owner_recorded_release_evidence.completed_at" "$BUNDLE_PATH" "generated_at"
    fi
    if valid_json_file "$PLUGIN_QA_REPORT"; then
      check_json_timestamp_between_reports "release evidence bundle plugin-trust QA" "$PLUGIN_QA_REPORT" "generated_at" "$BUNDLE_PATH" "owner_recorded_release_evidence.completed_at" "$BUNDLE_PATH" "generated_at"
    fi
    for field in artifacts.zip_sha256 artifacts.pkg_sha256 reports.signed_distribution_provenance_sha256 reports.live_device_qa_sha256 reports.plugin_trust_qa_sha256; do
      check_json_sha256 "release evidence bundle" "$BUNDLE_PATH" "$field"
    done
    check_json_sha256_matches_file "release evidence bundle" "$BUNDLE_PATH" "artifacts.zip_sha256" "app zip artifact" "$ZIP_PATH"
    check_json_sha256_matches_file "release evidence bundle" "$BUNDLE_PATH" "artifacts.pkg_sha256" "installer package artifact" "$PKG_PATH"
    check_json_sha256_matches_file "release evidence bundle" "$BUNDLE_PATH" "reports.signed_distribution_provenance_sha256" "signed-distribution provenance report" "$SIGNED_PROVENANCE_REPORT"
    check_json_sha256_matches_file "release evidence bundle" "$BUNDLE_PATH" "reports.live_device_qa_sha256" "live-device QA report" "$LIVE_QA_REPORT"
    check_json_sha256_matches_file "release evidence bundle" "$BUNDLE_PATH" "reports.plugin_trust_qa_sha256" "plugin-trust QA report" "$PLUGIN_QA_REPORT"
    if [[ "$SIGNED_PROVENANCE_REPORT_VALID" != true ]]; then
      record_missing "release evidence bundle references invalid signed-distribution provenance report: $SIGNED_PROVENANCE_REPORT"
    fi
    if [[ "$LIVE_QA_REPORT_VALID" != true ]]; then
      record_missing "release evidence bundle references invalid live-device QA report: $LIVE_QA_REPORT"
    fi
    if [[ "$PLUGIN_QA_REPORT_VALID" != true ]]; then
      record_missing "release evidence bundle references invalid plugin-trust QA report: $PLUGIN_QA_REPORT"
    fi
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
  printf 'Proof boundary: file/report inventory plus semantic report validation only; present artifact paths do not prove Developer ID signing, notarization, stapling, installation, Finder launch, live device QA, marketplace review, malware scan, OS sandbox, or host-level egress enforcement.\n'
}

assert_cli_evidence_status_complete() {
  if [[ "${JARVIS_EVIDENCE_DOCTOR_SELF_TEST_SHAPE_ONLY:-}" == "true" ]]; then
    return
  fi

  local endpoint_args=()
  if [[ -n "$EVIDENCE_STATUS_ENDPOINT" ]]; then
    endpoint_args=(--endpoint "$EVIDENCE_STATUS_ENDPOINT")
  fi

  local status_output
  if ! status_output="$(JARVIS_EVIDENCE_VERSION="$VERSION" \
    JARVIS_EVIDENCE_DIST_DIR="$DIST_DIR" \
    JARVIS_EVIDENCE_APP_PATH="$APP_PATH" \
    JARVIS_EVIDENCE_ZIP_PATH="$ZIP_PATH" \
    JARVIS_EVIDENCE_PKG_PATH="$PKG_PATH" \
    JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT="$SIGNED_PROVENANCE_REPORT" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$LIVE_QA_REPORT" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$PLUGIN_QA_REPORT" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$BUNDLE_PATH" \
    JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID="$EXPECTED_BUNDLE_ID" \
    JARVIS_EVIDENCE_EXPECTED_VERSION="$EXPECTED_VERSION" \
    JARVIS_QA_INSTALLED_APP_PATH="$EXPECTED_INSTALLED_APP_PATH" \
    cargo run -q -p jarvis-cli -- release evidence-status --json "${endpoint_args[@]}" 2>&1)"; then
    fail "release evidence doctor --assert-complete requires jarvis release evidence-status --json to pass; output: $status_output"
  fi

  STATUS_JSON="$status_output" python3 - <<'PY'
import json
import os
import sys

try:
    status = json.loads(os.environ["STATUS_JSON"])
except Exception as exc:
    print(f"release evidence-status returned non-JSON output: {exc}", file=sys.stderr)
    raise SystemExit(1)

complete = status.get("complete")
missing_count = status.get("missing_count")
invalid_count = status.get("invalid_count")
items = status.get("items")
if complete is True and missing_count == 0 and invalid_count == 0:
    if isinstance(items, list) and all(item.get("status") == "present" for item in items if isinstance(item, dict)):
        raise SystemExit(0)

problems = []
if complete is not True:
    problems.append(f"complete={complete!r}")
if missing_count != 0:
    problems.append(f"missing_count={missing_count!r}")
if invalid_count != 0:
    problems.append(f"invalid_count={invalid_count!r}")
if isinstance(items, list):
    bad_items = [
        f"{item.get('key', '<unknown>')}={item.get('status', '<missing status>')}"
        for item in items
        if isinstance(item, dict) and item.get("status") != "present"
    ]
    if bad_items:
        problems.append("items: " + ", ".join(bad_items))
else:
    problems.append("items=<not a list>")

print(
    "release evidence doctor --assert-complete requires jarvis release evidence-status --json "
    "to report complete=true with zero missing/invalid evidence and all items present; "
    + "; ".join(problems),
    file=sys.stderr,
)
raise SystemExit(1)
PY
}

write_fixture_app() {
  local app_path="$1"
  mkdir -p "$app_path/Contents/MacOS" "$app_path/Contents/Resources/bin"
  cat >"$app_path/Contents/Info.plist" <<XML
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.nobiletechnology.jarvis</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
</dict>
</plist>
XML
  touch "$app_path/Contents/MacOS/JarvisMacApp"
  cat >"$app_path/Contents/Resources/bin/jarvis-cli" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  printf 'jarvis $VERSION\n'
  exit 0
fi
printf 'self-test jarvis-cli fixture\n'
EOF
  printf 'jarvis %s\n' "$VERSION" >"$app_path/Contents/Resources/bin/jarvis-cli.version"
  chmod 755 "$app_path/Contents/MacOS/JarvisMacApp" "$app_path/Contents/Resources/bin/jarvis-cli"
}

write_fixture_reports() {
  local live_path="$1"
  local plugin_path="$2"
  local live_core_sha
  live_core_sha="$(file_sha256 "$(dirname "$live_path")/dist/Jarvis.app/Contents/Resources/bin/jarvis-cli")"

  cat >"$live_path" <<JSON
{
  "schema_version": 1,
  "evidence_type": "owner_recorded_live_device_qa",
  "self_test_fixture": false,
  "generated_at": "2026-05-22T16:06:00Z",
  "installed_app_path": "/Applications/Jarvis.app",
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
  "owner_recorded_non_voice_evidence": {
    "clean_profile_evidence_note": "Clean profile install observed in the fake fixture.",
    "finder_launch_evidence_note": "Finder launch observed in the fake fixture.",
    "notification_evidence_note": "Visible scheduler notification observed in the fake fixture.",
    "notification_observed_at": "2026-05-22T16:04:00Z",
    "restart_evidence_note": "Restart recovery observed in the fake fixture.",
    "manual_release_qa_evidence_note": "Manual release QA surfaces observed in the fake fixture."
  },
  "voice_command_observation": {
    "test_phrase": "Jarvis status check.",
    "observed_transcript": "Jarvis status check.",
    "expected_command_text": "status check",
    "observed_command_text": "status check",
    "command_result_evidence_id": "task:00000000-0000-4000-8000-000000000001",
    "audio_output_device_label": "self-test audio output"
  },
  "app_bundle": {
    "bundle_identifier": "com.nobiletechnology.jarvis",
    "short_version": "$VERSION",
    "build_version": "$VERSION",
    "microphone_usage_description": "self-test fixture",
    "speech_recognition_usage_description": "self-test fixture"
  },
  "bundled_core": {
    "executable_path": "/Applications/Jarvis.app/Contents/Resources/bin/jarvis-cli",
    "version": "jarvis $VERSION",
    "sha256": "$live_core_sha"
  },
  "proof_boundary": "self-test fixture"
}
JSON
  cat >"$plugin_path" <<'JSON'
{
  "schema_version": 1,
  "evidence_type": "owner_recorded_plugin_trust_qa",
  "self_test_fixture": false,
  "generated_at": "2026-05-22T16:30:00Z",
  "review_source": "owner-asserted-manual-review",
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
}

write_fixture_bundle() {
  local bundle_path="$1"
  local app_path="$2"
  local zip_path="$3"
  local pkg_path="$4"
  local signed_path="$5"
  local live_path="$6"
  local plugin_path="$7"
  local zip_sha
  local pkg_sha
  local signed_sha
  local live_sha
  local plugin_sha
  zip_sha="$(file_sha256 "$zip_path")"
  pkg_sha="$(file_sha256 "$pkg_path")"
  signed_sha="$(file_sha256 "$signed_path")"
  live_sha="$(file_sha256 "$live_path")"
  plugin_sha="$(file_sha256 "$plugin_path")"

  cat >"$bundle_path" <<JSON
{
  "schema_version": 1,
  "evidence_type": "release_evidence_bundle",
  "generated_at": "2026-05-22T17:00:00Z",
  "version": "$VERSION",
  "artifacts": {
    "app_path": "$app_path",
    "zip_path": "$zip_path",
    "pkg_path": "$pkg_path",
    "zip_sha256": "$zip_sha",
    "pkg_sha256": "$pkg_sha"
  },
  "reports": {
    "signed_distribution_provenance_report": "$signed_path",
    "live_device_qa_report": "$live_path",
    "plugin_trust_qa_report": "$plugin_path",
    "signed_distribution_provenance_sha256": "$signed_sha",
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
    "local_signature_validation": true
  },
  "owner_recorded_release_evidence": {
    "owner_name": "Jarvis Release Self-Test",
    "completed_at": "2026-05-22T16:45:00Z",
    "signed_distribution_note": "Signed distribution provenance fixture reviewed.",
    "notarization_note": "Notarization fixture reviewed.",
    "clean_profile_note": "Clean profile fixture reviewed.",
    "live_device_qa_note": "Live-device QA fixture reviewed.",
    "plugin_trust_qa_note": "Plugin-trust QA fixture reviewed.",
    "reports_archive_note": "Release evidence reports archived in the fixture.",
    "reports_archive_uri": "file://self-test/release-evidence"
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
  export JARVIS_EVIDENCE_DOCTOR_SELF_TEST_SHAPE_ONLY=true
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-release-evidence-doctor.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT
  self_test_zip="$tmp_dir/dist/Jarvis-$VERSION.zip"
  self_test_pkg="$tmp_dir/dist/Jarvis-$VERSION.pkg"
  mkdir -p "$tmp_dir/dist"
  write_fixture_app "$tmp_dir/dist/Jarvis.app"
  (cd "$tmp_dir/dist" && zip -qr "$self_test_zip" Jarvis.app)
  touch "$self_test_pkg"
  self_test_zip_sha="$(file_sha256 "$self_test_zip")"
  self_test_pkg_sha="$(file_sha256 "$self_test_pkg")"
  self_test_core_sha="$(file_sha256 "$tmp_dir/dist/Jarvis.app/Contents/Resources/bin/jarvis-cli")"
  write_fixture_reports "$tmp_dir/live.json" "$tmp_dir/plugin.json"
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
    "zip_sha256": "$self_test_zip_sha",
    "pkg_sha256": "$self_test_pkg_sha",
    "bundled_core_path": "$tmp_dir/dist/Jarvis.app/Contents/Resources/bin/jarvis-cli",
    "bundled_core_sha256": "$self_test_core_sha",
    "bundled_core_version": "jarvis $VERSION"
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
    "app_zip_status": "Accepted",
    "installer_pkg_status": "Accepted",
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
  write_fixture_bundle \
    "$tmp_dir/bundle.json" \
    "$tmp_dir/dist/Jarvis.app" \
    "$self_test_zip" \
    "$self_test_pkg" \
    "$tmp_dir/dist/Jarvis-$VERSION-signed-provenance.json" \
    "$tmp_dir/live.json" \
    "$tmp_dir/plugin.json"

  JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null

  printf 'jarvis 0.0.0\n' >"$tmp_dir/dist/Jarvis.app/Contents/Resources/bin/jarvis-cli.version"
  stale_marker_output=""
  stale_marker_output="$(JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --check)"
  if [[ "$stale_marker_output" != *"./scripts/package-distribution.sh --unsigned-launch-check"* ]]; then
    fail "release evidence doctor self-test expected stale bundled core guidance to include package-distribution.sh --unsigned-launch-check"
  fi
  if [[ "$stale_marker_output" != *"package preflight: ./scripts/package-distribution.sh --check"* ]]; then
    fail "release evidence doctor self-test expected next-step guidance to include package-distribution.sh --check"
  fi
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected stale bundled core version marker to fail"
  fi
  printf 'jarvis %s\n' "$VERSION" >"$tmp_dir/dist/Jarvis.app/Contents/Resources/bin/jarvis-cli.version"

  nested_zip="$tmp_dir/dist/nested-Jarvis-$VERSION.zip"
  mkdir -p "$tmp_dir/nested/payload"
  cp -R "$tmp_dir/dist/Jarvis.app" "$tmp_dir/nested/payload/Jarvis.app"
  (cd "$tmp_dir/nested" && zip -qr "$nested_zip" payload/Jarvis.app)
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="$nested_zip" \
    JARVIS_EVIDENCE_PKG_PATH="$self_test_pkg" \
    JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/dist/Jarvis-$VERSION-signed-provenance.json" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected nested app zip payload to fail"
  fi

  python3 - "$tmp_dir/dist/Jarvis-$VERSION-signed-provenance.json" "$tmp_dir/dist/stale-digest-signed-provenance.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["artifacts"]["zip_sha256"] = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/dist/stale-digest-signed-provenance.json" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected stale signed provenance digest to fail"
  fi

  python3 - "$tmp_dir/dist/Jarvis-$VERSION-signed-provenance.json" "$tmp_dir/dist/bad-apple-tool-signed-provenance.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["signing"]["app_bundle_codesign"] = "Authority=Apple Development: Jarvis QA Fixture"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/dist/bad-apple-tool-signed-provenance.json" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected non-Developer-ID signed provenance evidence to fail"
  fi

  python3 - "$tmp_dir/dist/Jarvis-$VERSION-signed-provenance.json" "$tmp_dir/dist/negated-gatekeeper-signed-provenance.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["gatekeeper"]["app_bundle_assessment"] = "not accepted"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/dist/negated-gatekeeper-signed-provenance.json" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected negated Gatekeeper acceptance to fail"
  fi

  python3 - "$tmp_dir/dist/Jarvis-$VERSION-signed-provenance.json" "$tmp_dir/dist/rejected-notary-signed-provenance.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["notarization"]["app_zip_status"] = "Rejected"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT="$tmp_dir/dist/rejected-notary-signed-provenance.json" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected rejected notary status to fail"
  fi

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

  python3 - "$tmp_dir/live.json" "$tmp_dir/mismatched-transcript-live.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["voice_command_observation"]["observed_transcript"] = "Jarvis stats check."
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/mismatched-transcript-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected mismatched live transcript observation to fail"
  fi

  python3 - "$tmp_dir/live.json" "$tmp_dir/mismatched-installed-app-live.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["installed_app_path"] = "/tmp/Jarvis.app"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/mismatched-installed-app-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected mismatched installed app path to fail"
  fi

  python3 - "$tmp_dir/live.json" "$tmp_dir/malformed-command-result-evidence-live.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["voice_command_observation"]["command_result_evidence_id"] = "looked good"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/malformed-command-result-evidence-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected malformed command result evidence id to fail"
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
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/mismatched-core-digest-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected mismatched live bundled-core digest to fail"
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

  python3 - "$tmp_dir/plugin.json" "$tmp_dir/future-plugin.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["generated_at"] = "2999-01-01T00:00:00Z"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/future-plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected future-dated plugin trust report to fail"
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

  python3 - "$tmp_dir/bundle.json" "$tmp_dir/stale-digest-bundle.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["reports"]["live_device_qa_sha256"] = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/stale-digest-bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected stale final bundle report digest to fail"
  fi

  python3 - "$tmp_dir/bundle.json" "$tmp_dir/post-child-completion-bundle.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["owner_recorded_release_evidence"]["completed_at"] = "2026-05-22T16:00:00Z"
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/post-child-completion-bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected final bundle completed before child reports to fail"
  fi

  python3 - "$tmp_dir/live.json" "$tmp_dir/invalid-bundle-child-live.json" <<'PY'
import json
import sys

source, target = sys.argv[1:3]
with open(source, encoding="utf-8") as handle:
    data = json.load(handle)
data["validation_flags"]["notification"] = False
with open(target, "w", encoding="utf-8") as handle:
    json.dump(data, handle)
PY
  write_fixture_bundle \
    "$tmp_dir/invalid-child-bundle.json" \
    "$tmp_dir/dist/Jarvis.app" \
    "$self_test_zip" \
    "$self_test_pkg" \
    "$tmp_dir/dist/Jarvis-$VERSION-signed-provenance.json" \
    "$tmp_dir/invalid-bundle-child-live.json" \
    "$tmp_dir/plugin.json"
  invalid_child_output="$(JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/invalid-bundle-child-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/invalid-child-bundle.json" \
    "$0" --check)"
  if [[ "$invalid_child_output" != *"release evidence bundle references invalid live-device QA report"* ]]; then
    fail "release evidence doctor self-test expected final bundle to reject invalid live-device child report"
  fi
  if JARVIS_EVIDENCE_DIST_DIR="$tmp_dir/dist" \
    JARVIS_EVIDENCE_APP_PATH="$tmp_dir/dist/Jarvis.app" \
    JARVIS_EVIDENCE_ZIP_PATH="" \
    JARVIS_EVIDENCE_PKG_PATH="" \
    JARVIS_EVIDENCE_LIVE_QA_REPORT="$tmp_dir/invalid-bundle-child-live.json" \
    JARVIS_EVIDENCE_PLUGIN_QA_REPORT="$tmp_dir/plugin.json" \
    JARVIS_EVIDENCE_OUTPUT_PATH="$tmp_dir/invalid-child-bundle.json" \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "release evidence doctor self-test expected invalid final bundle child report to fail"
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

if [[ "$ASSERT_COMPLETE" == true ]]; then
  assert_cli_evidence_status_complete
fi
