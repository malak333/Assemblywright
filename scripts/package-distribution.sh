#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
export CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-$ROOT_DIR/target/clang-module-cache}"
mkdir -p "$CLANG_MODULE_CACHE_PATH"

VERSION="${ASSEMBLYWRIGHT_PACKAGE_VERSION_OVERRIDE:-$("$ROOT_DIR/scripts/release-version.sh")}"
BUNDLE_ID="com.nobiletechnology.assemblywright"
CORE_CODE_ID="${BUNDLE_ID}.core"
APP_NAME="Assemblywright"
# Name of the executable inside the bundle. This is a signed-identity and
# release-evidence contract: signed provenance and live-device QA reports bind
# `Assemblywright.app/Contents/MacOS/AssemblywrightMacApp`. The SwiftPM product that
# produces it is named separately below and may be renamed freely.
APP_EXECUTABLE_NAME="AssemblywrightMacApp"
SWIFT_APP_PRODUCT="AssemblywrightMacApp"
CORE_EXECUTABLE_NAME="assemblywright-cli"
ENTITLEMENTS="$ROOT_DIR/packaging/Assemblywright.entitlements"
CORE_ENTITLEMENTS="$ROOT_DIR/packaging/AssemblywrightCore.entitlements"
BRAND_GENERATED_DIR="$ROOT_DIR/assets/brand/generated"
APP_ICON_FILE="Assemblywright.icns"
MENU_BAR_TEMPLATE_FILES=(
  "menubar-template.png"
  "menubar-template@2x.png"
  "menubar-template@3x.png"
)
DIST_DIR="${ASSEMBLYWRIGHT_DISTRIBUTION_DIR:-$ROOT_DIR/target/distribution}"
APP_PATH="$DIST_DIR/$APP_NAME.app"
ZIP_PATH="$DIST_DIR/$APP_NAME-$VERSION.zip"
PKG_PATH="$DIST_DIR/$APP_NAME-$VERSION.pkg"
PROVENANCE_PATH="${ASSEMBLYWRIGHT_SIGNED_PROVENANCE_PATH:-$DIST_DIR/$APP_NAME-$VERSION-signed-provenance.json}"
CHECK_ONLY=false
UNSIGNED_STRUCTURE_CHECK=false
UNSIGNED_LAUNCH_CHECK=false
CHECK_GUIDANCE_SELF_TEST=false
ENTITLEMENTS_POLICY_SELF_TEST=false
VERSION_CONSISTENCY_SELF_TEST=false
PROVENANCE_SELF_TEST=false
RUNNING_APP_GUARD_SELF_TEST=false
RUNNING_APP_GUARD_E2E=false

usage() {
  cat <<'USAGE'
Usage: scripts/package-distribution.sh [--check] [--unsigned-structure-check] [--unsigned-launch-check] [--check-guidance-self-test] [--entitlements-policy-self-test] [--version-consistency-self-test] [--provenance-self-test] [--running-app-guard-self-test] [--running-app-guard-e2e]

Build a distribution-shaped Assemblywright.app bundle, sign it with Developer ID, zip it,
submit it for notarization, staple the ticket, then build, sign, notarize, and
staple a Developer ID Installer package for /Applications installation.

Required for full distribution packaging:
  ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION  Developer ID Application signing identity
  ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER     Developer ID Installer signing identity

Required for notarization, choose one:
  ASSEMBLYWRIGHT_NOTARYTOOL_PROFILE        Stored notarytool keychain profile
  or
  ASSEMBLYWRIGHT_NOTARYTOOL_APPLE_ID
  ASSEMBLYWRIGHT_NOTARYTOOL_TEAM_ID
  ASSEMBLYWRIGHT_NOTARYTOOL_PASSWORD       App-specific password

Optional:
  ASSEMBLYWRIGHT_DISTRIBUTION_DIR          Defaults to target/distribution
  ASSEMBLYWRIGHT_SIGNED_PROVENANCE_PATH    Defaults to target/distribution/Assemblywright-<version>-signed-provenance.json

--check validates local tool/template preconditions without signing or notarizing.
--unsigned-structure-check builds and inspects an unsigned app/pkg layout without
Developer ID credentials, notarization, stapling, Finder launch, or live device
validation.
--unsigned-launch-check also launches the release-built app executable with an
isolated HOME and exercises the supervised core over loopback IPC. It still does
not prove Developer ID signing, notarization, stapling, Finder launch, or live
device validation.
--check-guidance-self-test verifies the credential-free package preflight still
prints the required downstream signed-distribution, QA, evidence-bundle, and
doctor handoff commands.
--entitlements-policy-self-test verifies the app entitlement template keeps
--version-consistency-self-test verifies package/crate version drift is rejected
without signing, notarizing, or building distribution artifacts.
--provenance-self-test verifies signed-provenance schema and semantic Apple
tool-output guards with stubbed local commands. It does not sign, notarize,
staple, launch, install, or manually validate artifacts.
--running-app-guard-self-test verifies that packaging refuses to replace the
exact distribution bundle while its app or bundled core executable is running.
It uses synthetic process records and does not launch or stop Assemblywright.
--running-app-guard-e2e launches harmless temporary app/core executable copies,
proves the real process inspection refuses replacement, stops only those test
processes, and proves the same temporary bundle is then accepted.
USAGE
}

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

running_distribution_processes_from_lsof() {
  local app_executable_path="$1"
  local core_executable_path="$2"
  local current_pid=""
  local field
  local executable_path

  while IFS= read -r field; do
    case "$field" in
      p*)
        current_pid="${field#p}"
        ;;
      n*)
        executable_path="${field#n}"
        if [[ -n "$current_pid" ]] &&
          [[ "$executable_path" == "$app_executable_path" || "$executable_path" == "$core_executable_path" ]]; then
          printf '%s\t%s\n' "$current_pid" "$executable_path"
        fi
        ;;
    esac
  done
}

reject_running_distribution_processes() {
  local matches="$1"
  local pids
  [[ -z "$matches" ]] && return 0

  pids="$(printf '%s\n' "$matches" | awk -F '\t' '!seen[$1]++ { printf "%s%s", separator, $1; separator="," }')"
  fail "refusing to replace $APP_PATH while Assemblywright is running from that bundle (PID(s): $pids); quit Assemblywright with Command-Q, then retry, or use a different ASSEMBLYWRIGHT_DISTRIBUTION_DIR"
}

run_running_app_guard_self_test() {
  local fixture_root="/tmp/Assemblywright guard fixture"
  local fixture_app="$fixture_root/Assemblywright.app/Contents/MacOS/$APP_EXECUTABLE_NAME"
  local fixture_core="$fixture_root/Assemblywright.app/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  local records
  local matches
  local output

  records="$(cat <<EOF
p101
ftxt
n$fixture_app
p102
ftxt
n$fixture_core
p103
ftxt
n$fixture_root/Assemblywright.app.backup/Contents/MacOS/$APP_EXECUTABLE_NAME
p104
ftxt
n/bin/zsh
EOF
)"
  matches="$(printf '%s\n' "$records" | running_distribution_processes_from_lsof "$fixture_app" "$fixture_core")"
  require_output_contains "running app guard app match" "$matches" $'101\t'"$fixture_app"
  require_output_contains "running app guard core match" "$matches" $'102\t'"$fixture_core"
  if [[ "$matches" == *$'103\t'* || "$matches" == *$'104\t'* ]]; then
    fail "running app guard self-test accepted a near-match executable path"
  fi

  output=""
  if output="$(APP_PATH="$fixture_root/Assemblywright.app" reject_running_distribution_processes "$matches" 2>&1)"; then
    fail "running app guard self-test expected an active distribution process to block replacement"
  fi
  require_output_contains "running app guard rejection" "$output" "refusing to replace $fixture_root/Assemblywright.app"
  require_output_contains "running app guard rejection" "$output" "PID(s): 101,102"
  require_output_contains "running app guard rejection" "$output" "quit Assemblywright with Command-Q"

  reject_running_distribution_processes ""
  printf '\nAssemblywright running app guard self-test: ok\n'
  printf 'Proof boundary: synthetic active-executable detection and refusal messaging only; no app was built, replaced, launched, stopped, signed, notarized, stapled, installed, or manually validated.\n'
}

if [[ -n "${ASSEMBLYWRIGHT_BUNDLE_ID:-}" && "$ASSEMBLYWRIGHT_BUNDLE_ID" != "$BUNDLE_ID" ]]; then
  fail "ASSEMBLYWRIGHT_BUNDLE_ID overrides are unsupported; Assemblywright code identity requires the fixed $BUNDLE_ID app identifier"
fi

require_output_contains() {
  local label="$1"
  local output="$2"
  local expected="$3"
  if [[ "$output" != *"$expected"* ]]; then
    printf 'error: %s did not include %q\n' "$label" "$expected" >&2
    printf '%s\n%s\n%s\n' "--- $label output ---" "$output" "--- end $label output ---" >&2
    exit 1
  fi
}

file_sha256() {
  shasum -a 256 "$1" | awk '{print $1}'
}

require_gatekeeper_accepted() {
  local label="$1"
  local output="$2"
  python3 - "$label" "$output" <<'PY'
import re
import sys

label, output = sys.argv[1:3]
lines = [line.strip() for line in output.splitlines() if line.strip()]
if not any(line == "accepted" or re.search(r":\s*accepted$", line) for line in lines):
    raise SystemExit(f"{label} must include an exact Gatekeeper accepted result")
PY
}

validate_app_zip_payload() {
  local zip_path="$1"
  python3 - "$zip_path" "$APP_ICON_FILE" <<'PY'
import plistlib
import sys
import zipfile

zip_path, expected_icon = sys.argv[1:3]
required_entries = (
    "Assemblywright.app/Contents/MacOS/AssemblywrightMacApp",
    "Assemblywright.app/Contents/Resources/bin/assemblywright-cli",
    "Assemblywright.app/Contents/Info.plist",
    f"Assemblywright.app/Contents/Resources/{expected_icon}",
)

with zipfile.ZipFile(zip_path) as archive:
    names = archive.namelist()

missing = [entry for entry in required_entries if entry not in names]
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
    raise SystemExit(
        f"zip payload must contain exactly one top-level Assemblywright.app root, got: {', '.join(sorted(app_roots))}"
    )

with zipfile.ZipFile(zip_path) as archive:
    info_plist = plistlib.loads(archive.read("Assemblywright.app/Contents/Info.plist"))


actual_icon = info_plist.get("CFBundleIconFile")
if actual_icon != expected_icon:
    raise SystemExit(
        "zip payload Info.plist CFBundleIconFile mismatch: "
        f"expected {expected_icon!r}, got {actual_icon!r}"
    )
PY
}

json_escape() {
  python3 -c 'import json, sys; print(json.dumps(sys.stdin.read()))'
}

extract_notary_submission_id() {
  python3 -c '
import re
import sys
text = sys.stdin.read()
for pattern in (r"\bid:\s*([0-9a-fA-F-]{20,})", r"\bsubmission id:\s*([0-9a-fA-F-]{20,})"):
    match = re.search(pattern, text, re.IGNORECASE)
    if match:
        print(match.group(1))
        raise SystemExit(0)
print("")
'
}

extract_notary_accepted_status() {
  python3 -c '
import re
import sys
text = sys.stdin.read()
match = re.search(r"(?im)^\s*status\s*:\s*(\S+)\s*$", text)
if match and match.group(1) == "Accepted":
    print("Accepted")
'
}

require_uuid() {
  local label="$1"
  local value="$2"
  python3 - "$label" "$value" <<'PY'
import sys
import uuid

label, value = sys.argv[1:3]
try:
    parsed = uuid.UUID(value.strip())
except Exception:
    raise SystemExit(f"{label} must be a UUID")
if parsed.int == 0:
    raise SystemExit(f"{label} must not be a nil UUID")
PY
}

capture_command() {
  local label="$1"
  local output_path="$2"
  shift 2
  local output

  printf '\n==> %s\n' "$*"
  if ! output="$("$@" 2>&1)"; then
    printf '%s\n' "$output" >&2
    printf '%s\n' "$output" >"$output_path"
    fail "$label failed; output captured at $output_path"
  fi
  printf '%s\n' "$output"
  printf '%s\n' "$output" >"$output_path"
}

assert_package_version_consistency() {
  local canonical_version
  canonical_version="$("$ROOT_DIR/scripts/release-version.sh")"

  if [[ "$VERSION" != "$canonical_version" ]]; then
    fail "package version mismatch: package-distribution.sh VERSION=$VERSION, canonical Rust release version=$canonical_version"
  fi
}

assert_bundled_core_version() {
  local core_path="$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  local marker_path="$core_path.version"
  local output
  [[ -x "$core_path" ]] || fail "bundled core executable missing: $core_path"
  [[ -f "$marker_path" ]] || fail "bundled core version marker missing: $marker_path; rerun this packaging command so the app bundle is rebuilt from the current assemblywright-cli"
  require_output_contains "bundled core version marker" "$(tr -d '\r\n' <"$marker_path")" "assemblywright $VERSION"
  output="$("$core_path" --version)"
  require_output_contains "bundled core version" "$output" "assemblywright $VERSION"
}


assert_code_identifier() {
  local label="$1"
  local path="$2"
  local expected_identifier="$3"
  local output
  output="$(codesign -dv --verbose=4 "$path" 2>&1)"
  require_output_contains "$label code identifier" "$output" "Identifier=$expected_identifier"
}

assert_app_core_code_identifiers() {
  local label="$1"
  assert_code_identifier "$label app" "$APP_PATH" "$BUNDLE_ID"
  assert_code_identifier "$label app executable" "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME" "$BUNDLE_ID"
  assert_code_identifier "$label bundled core" "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" "$CORE_CODE_ID"
}

codesign_metadata_value() {
  local label="$1"
  local output="$2"
  local key="$3"
  local value
  value="$(printf '%s\n' "$output" | awk -F= -v key="$key" '$1 == key { print substr($0, length(key) + 2); exit }')"
  [[ -n "$value" ]] || fail "$label codesign evidence is missing $key"
  printf '%s' "$value"
}

require_hex_identity_value() {
  local label="$1"
  local value="$2"
  local minimum_length="$3"
  require_command python3
  python3 - "$label" "$value" "$minimum_length" <<'PY'
import re
import sys

label, value, minimum_length = sys.argv[1:4]
if not re.fullmatch(rf"[0-9A-Fa-f]{{{int(minimum_length)},64}}", value):
    raise SystemExit(f"{label} must be a hexadecimal value between {minimum_length} and 64 characters")
PY
}

write_signed_distribution_provenance() {
  local generated_at
  local zip_sha
  local pkg_sha
  local bundled_core_path
  local bundled_core_sha
  local bundled_core_version
  local app_executable_path
  local app_executable_sha
  local app_codesign
  local core_codesign
  local app_executable_codesign
  local app_executable_identifier
  local app_executable_team_identifier
  local app_executable_cdhash
  local pkg_signature
  local app_staple
  local pkg_staple
  local app_gatekeeper
  local pkg_gatekeeper
  local zip_submission_id
  local pkg_submission_id
  local zip_notary_status
  local pkg_notary_status
  local zip_notary_log_sha
  local pkg_notary_log_sha
  local proof_boundary

  require_command python3
  require_command shasum
  require_command spctl

  validate_app_zip_payload "$ZIP_PATH"

  generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  zip_sha="$(file_sha256 "$ZIP_PATH")"
  pkg_sha="$(file_sha256 "$PKG_PATH")"
  bundled_core_path="$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  bundled_core_sha="$(file_sha256 "$bundled_core_path")"
  bundled_core_version="$("$bundled_core_path" --version)"
  require_output_contains "bundled core version" "$bundled_core_version" "assemblywright $VERSION"
  app_executable_path="$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
  app_executable_sha="$(file_sha256 "$app_executable_path")"
  app_codesign="$(codesign -dv --verbose=4 "$APP_PATH" 2>&1)"
  core_codesign="$(codesign -dv --verbose=4 "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" 2>&1)"
  app_executable_codesign="$(codesign -dv --verbose=4 "$app_executable_path" 2>&1)"
  app_executable_identifier="$(codesign_metadata_value "app executable" "$app_executable_codesign" "Identifier")"
  app_executable_team_identifier="$(codesign_metadata_value "app executable" "$app_executable_codesign" "TeamIdentifier")"
  app_executable_cdhash="$(codesign_metadata_value "app executable" "$app_executable_codesign" "CDHash")"
  [[ "$app_executable_identifier" == "$BUNDLE_ID" ]] ||
    fail "app executable codesign identifier mismatch: expected $BUNDLE_ID, got $app_executable_identifier"
  [[ "$app_executable_team_identifier" =~ ^[A-Z0-9]{10}$ ]] ||
    fail "app executable codesign TeamIdentifier must be a 10-character Apple team identifier"
  require_hex_identity_value "app executable codesign CDHash" "$app_executable_cdhash" 40
  pkg_signature="$(pkgutil --check-signature "$PKG_PATH" 2>&1)"
  app_staple="$(xcrun stapler validate "$APP_PATH" 2>&1)"
  pkg_staple="$(xcrun stapler validate "$PKG_PATH" 2>&1)"
  app_gatekeeper="$(spctl --assess --type execute --verbose "$APP_PATH" 2>&1)"
  pkg_gatekeeper="$(spctl --assess --type install --verbose "$PKG_PATH" 2>&1)"
  zip_submission_id="$(extract_notary_submission_id <"$ZIP_NOTARY_LOG")"
  pkg_submission_id="$(extract_notary_submission_id <"$PKG_NOTARY_LOG")"
  zip_notary_status="$(extract_notary_accepted_status <"$ZIP_NOTARY_LOG")"
  pkg_notary_status="$(extract_notary_accepted_status <"$PKG_NOTARY_LOG")"
  zip_notary_log_sha="$(file_sha256 "$ZIP_NOTARY_LOG")"
  pkg_notary_log_sha="$(file_sha256 "$PKG_NOTARY_LOG")"
  require_output_contains "Developer ID Application identity" "$ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION" "Developer ID Application: "
  require_output_contains "Developer ID Installer identity" "$ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER" "Developer ID Installer: "
  require_output_contains "app bundle codesign evidence" "$app_codesign" "Authority=Developer ID Application: "
  require_output_contains "app bundle configured codesign identity" "$app_codesign" "Authority=$ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION"
  require_output_contains "app bundle stable code identifier" "$app_codesign" "Identifier=$BUNDLE_ID"
  require_output_contains "bundled core codesign evidence" "$core_codesign" "Authority=Developer ID Application: "
  require_output_contains "bundled core configured codesign identity" "$core_codesign" "Authority=$ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION"
  require_output_contains "bundled core stable code identifier" "$core_codesign" "Identifier=$CORE_CODE_ID"
  require_output_contains "app executable codesign evidence" "$app_executable_codesign" "Authority=Developer ID Application: "
  require_output_contains "app executable configured codesign identity" "$app_executable_codesign" "Authority=$ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION"
  require_output_contains "app executable stable code identifier" "$app_executable_codesign" "Identifier=$BUNDLE_ID"
  require_output_contains "installer package signature evidence" "$pkg_signature" "Developer ID Installer: "
  require_output_contains "installer package configured signature identity" "$pkg_signature" "$ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER"
  require_uuid "app zip notary submission id" "$zip_submission_id"
  require_uuid "installer package notary submission id" "$pkg_submission_id"
  require_output_contains "app zip notary status" "$zip_notary_status" "Accepted"
  require_output_contains "installer package notary status" "$pkg_notary_status" "Accepted"
  require_output_contains "app bundle stapler validation" "$app_staple" "The validate action worked!"
  require_output_contains "installer package stapler validation" "$pkg_staple" "The validate action worked!"
  require_gatekeeper_accepted "app bundle Gatekeeper assessment" "$app_gatekeeper"
  require_gatekeeper_accepted "installer package Gatekeeper assessment" "$pkg_gatekeeper"
  proof_boundary="Signed distribution provenance report generated by package-distribution.sh after Developer ID signing, notarization, stapling, Gatekeeper assessment, and artifact digest capture. It records local command evidence for the referenced artifacts; it does not prove clean-profile installation, Finder launch, live-device QA, plugin-trust QA, App Store review, malware analysis, or OS sandbox enforcement."

  mkdir -p "$(dirname "$PROVENANCE_PATH")"
  PROVENANCE_GENERATED_AT="$generated_at" \
    PROVENANCE_VERSION="$VERSION" \
    PROVENANCE_BUNDLE_ID="$BUNDLE_ID" \
    PROVENANCE_APP_PATH="$APP_PATH" \
    PROVENANCE_ZIP_PATH="$ZIP_PATH" \
    PROVENANCE_PKG_PATH="$PKG_PATH" \
    PROVENANCE_ZIP_SHA="$zip_sha" \
    PROVENANCE_PKG_SHA="$pkg_sha" \
    PROVENANCE_BUNDLED_CORE_PATH="$bundled_core_path" \
    PROVENANCE_BUNDLED_CORE_SHA="$bundled_core_sha" \
    PROVENANCE_BUNDLED_CORE_VERSION="$bundled_core_version" \
    PROVENANCE_APP_EXECUTABLE_PATH="$app_executable_path" \
    PROVENANCE_APP_EXECUTABLE_SHA="$app_executable_sha" \
    PROVENANCE_DEVELOPER_ID_APPLICATION="$ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION" \
    PROVENANCE_DEVELOPER_ID_INSTALLER="$ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER" \
    PROVENANCE_APP_CODESIGN="$app_codesign" \
    PROVENANCE_APP_EXECUTABLE_CODESIGN="$app_executable_codesign" \
    PROVENANCE_APP_EXECUTABLE_IDENTIFIER="$app_executable_identifier" \
    PROVENANCE_APP_EXECUTABLE_TEAM_IDENTIFIER="$app_executable_team_identifier" \
    PROVENANCE_APP_EXECUTABLE_CDHASH="$app_executable_cdhash" \
    PROVENANCE_CORE_CODESIGN="$core_codesign" \
    PROVENANCE_PKG_SIGNATURE="$pkg_signature" \
    PROVENANCE_ZIP_SUBMISSION_ID="$zip_submission_id" \
    PROVENANCE_PKG_SUBMISSION_ID="$pkg_submission_id" \
    PROVENANCE_ZIP_NOTARY_STATUS="$zip_notary_status" \
    PROVENANCE_PKG_NOTARY_STATUS="$pkg_notary_status" \
    PROVENANCE_ZIP_NOTARY_LOG="$ZIP_NOTARY_LOG" \
    PROVENANCE_PKG_NOTARY_LOG="$PKG_NOTARY_LOG" \
    PROVENANCE_ZIP_NOTARY_LOG_SHA="$zip_notary_log_sha" \
    PROVENANCE_PKG_NOTARY_LOG_SHA="$pkg_notary_log_sha" \
    PROVENANCE_APP_STAPLE="$app_staple" \
    PROVENANCE_PKG_STAPLE="$pkg_staple" \
    PROVENANCE_APP_GATEKEEPER="$app_gatekeeper" \
    PROVENANCE_PKG_GATEKEEPER="$pkg_gatekeeper" \
    PROVENANCE_PROOF_BOUNDARY="$proof_boundary" \
    python3 - "$PROVENANCE_PATH" <<'PY'
import json
import os
import sys

path = sys.argv[1]
report = {
    "schema_version": 1,
    "evidence_type": "signed_distribution_provenance",
    "generated_at": os.environ["PROVENANCE_GENERATED_AT"],
    "version": os.environ["PROVENANCE_VERSION"],
    "bundle_identifier": os.environ["PROVENANCE_BUNDLE_ID"],
    "artifacts": {
        "app_path": os.environ["PROVENANCE_APP_PATH"],
        "zip_path": os.environ["PROVENANCE_ZIP_PATH"],
        "pkg_path": os.environ["PROVENANCE_PKG_PATH"],
        "zip_sha256": os.environ["PROVENANCE_ZIP_SHA"],
        "pkg_sha256": os.environ["PROVENANCE_PKG_SHA"],
        "app_executable_path": os.environ["PROVENANCE_APP_EXECUTABLE_PATH"],
        "app_executable_sha256": os.environ["PROVENANCE_APP_EXECUTABLE_SHA"],
        "bundled_core_path": os.environ["PROVENANCE_BUNDLED_CORE_PATH"],
        "bundled_core_sha256": os.environ["PROVENANCE_BUNDLED_CORE_SHA"],
        "bundled_core_version": os.environ["PROVENANCE_BUNDLED_CORE_VERSION"],
    },
    "signing": {
        "developer_id_application_identity": os.environ["PROVENANCE_DEVELOPER_ID_APPLICATION"],
        "developer_id_installer_identity": os.environ["PROVENANCE_DEVELOPER_ID_INSTALLER"],
        "app_bundle_codesign": os.environ["PROVENANCE_APP_CODESIGN"],
        "app_executable_codesign": os.environ["PROVENANCE_APP_EXECUTABLE_CODESIGN"],
        "app_executable_identifier": os.environ["PROVENANCE_APP_EXECUTABLE_IDENTIFIER"],
        "app_executable_team_identifier": os.environ["PROVENANCE_APP_EXECUTABLE_TEAM_IDENTIFIER"],
        "app_executable_cdhash": os.environ["PROVENANCE_APP_EXECUTABLE_CDHASH"],
        "bundled_core_codesign": os.environ["PROVENANCE_CORE_CODESIGN"],
        "installer_pkg_signature": os.environ["PROVENANCE_PKG_SIGNATURE"],
    },
    "notarization": {
        "app_zip_submission_id": os.environ["PROVENANCE_ZIP_SUBMISSION_ID"],
        "installer_pkg_submission_id": os.environ["PROVENANCE_PKG_SUBMISSION_ID"],
        "app_zip_status": os.environ["PROVENANCE_ZIP_NOTARY_STATUS"],
        "installer_pkg_status": os.environ["PROVENANCE_PKG_NOTARY_STATUS"],
        "app_zip_notary_log": os.environ["PROVENANCE_ZIP_NOTARY_LOG"],
        "installer_pkg_notary_log": os.environ["PROVENANCE_PKG_NOTARY_LOG"],
        "app_zip_notary_log_sha256": os.environ["PROVENANCE_ZIP_NOTARY_LOG_SHA"],
        "installer_pkg_notary_log_sha256": os.environ["PROVENANCE_PKG_NOTARY_LOG_SHA"],
    },
    "stapling": {
        "app_bundle_validation": os.environ["PROVENANCE_APP_STAPLE"],
        "installer_pkg_validation": os.environ["PROVENANCE_PKG_STAPLE"],
    },
    "gatekeeper": {
        "app_bundle_assessment": os.environ["PROVENANCE_APP_GATEKEEPER"],
        "installer_pkg_assessment": os.environ["PROVENANCE_PKG_GATEKEEPER"],
    },
    "validation_flags": {
        "developer_id_application_signed": True,
        "developer_id_installer_signed": True,
        "app_zip_notarized": True,
        "installer_pkg_notarized": True,
        "app_stapled": True,
        "installer_pkg_stapled": True,
        "gatekeeper_assessed": True,
        "artifact_digests_recorded": True,
        "app_executable_identity_recorded": True,
    },
    "proof_boundary": os.environ["PROVENANCE_PROOF_BOUNDARY"],
}
os.makedirs(os.path.dirname(path), exist_ok=True)
with open(path, "w", encoding="utf-8") as handle:
    json.dump(report, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY
  python3 -m json.tool "$PROVENANCE_PATH" >/dev/null
}

select_port() {
  if [[ -n "${ASSEMBLYWRIGHT_DISTRIBUTION_LAUNCH_CHECK_PORT:-}" ]]; then
    printf '%s\n' "$ASSEMBLYWRIGHT_DISTRIBUTION_LAUNCH_CHECK_PORT"
    return
  fi

  for port in 18817 18818 18819 18820 18821; do
    if ! nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
      printf '%s\n' "$port"
      return
    fi
  done

  fail "no distribution launch-check port is available"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      CHECK_ONLY=true
      shift
      ;;
    --unsigned-structure-check)
      UNSIGNED_STRUCTURE_CHECK=true
      shift
      ;;
    --unsigned-launch-check)
      UNSIGNED_LAUNCH_CHECK=true
      shift
      ;;
    --check-guidance-self-test)
      CHECK_GUIDANCE_SELF_TEST=true
      shift
      ;;
    --entitlements-policy-self-test)
      ENTITLEMENTS_POLICY_SELF_TEST=true
      shift
      ;;
    --version-consistency-self-test)
      VERSION_CONSISTENCY_SELF_TEST=true
      shift
      ;;
    --provenance-self-test)
      PROVENANCE_SELF_TEST=true
      shift
      ;;
    --running-app-guard-self-test)
      RUNNING_APP_GUARD_SELF_TEST=true
      shift
      ;;
    --running-app-guard-e2e)
      RUNNING_APP_GUARD_E2E=true
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

selected_mode_count=0
for selected_mode in "$CHECK_ONLY" "$UNSIGNED_STRUCTURE_CHECK" "$UNSIGNED_LAUNCH_CHECK" "$CHECK_GUIDANCE_SELF_TEST" "$ENTITLEMENTS_POLICY_SELF_TEST" "$VERSION_CONSISTENCY_SELF_TEST" "$PROVENANCE_SELF_TEST" "$RUNNING_APP_GUARD_SELF_TEST" "$RUNNING_APP_GUARD_E2E"; do
  if [[ "$selected_mode" == true ]]; then
    selected_mode_count=$((selected_mode_count + 1))
  fi
done
if [[ "$selected_mode_count" -gt 1 ]]; then
  fail "--check, --unsigned-structure-check, --unsigned-launch-check, --check-guidance-self-test, --entitlements-policy-self-test, --version-consistency-self-test, --provenance-self-test, --running-app-guard-self-test, and --running-app-guard-e2e are mutually exclusive"
fi

if [[ "$RUNNING_APP_GUARD_SELF_TEST" == true ]]; then
  run_running_app_guard_self_test
  exit 0
fi

if [[ "$CHECK_GUIDANCE_SELF_TEST" == true ]]; then
  CHECK_OUTPUT=""
  if ! CHECK_OUTPUT="$("$0" --check 2>&1)"; then
    printf '%s\n' "$CHECK_OUTPUT" >&2
    fail "package check guidance self-test expected --check to pass"
  fi
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "Assemblywright distribution packaging preflight: ok"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "Next release evidence commands:"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "cargo run -p assemblywright-cli -- release signed-distribution-runbook"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' ASSEMBLYWRIGHT_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "ASSEMBLYWRIGHT_NOTARYTOOL_APPLE_ID='apple-id@example.com' ASSEMBLYWRIGHT_NOTARYTOOL_TEAM_ID='TEAMID1234' ASSEMBLYWRIGHT_NOTARYTOOL_PASSWORD='app-specific-password'"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "Set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' in target/release-live-device-qa.env"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "cargo run -p assemblywright-cli -- command \"status check\" --endpoint \"\${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}\" --json"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "ASSEMBLYWRIGHT_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid>'"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "set -a && source target/release-live-device-qa.env && set +a"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "./scripts/release-live-device-qa.sh --assert-complete"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release evidence-status --endpoint \"\${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}\""
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release readiness --endpoint \"\${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}\""
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "set -a && source target/release-plugin-trust-qa.env && set +a"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "./scripts/release-plugin-trust-qa.sh --assert-complete"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "set -a && source target/release-evidence-bundle.env && set +a"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "./scripts/release-evidence-bundle.sh --bundle"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "./scripts/release-evidence-doctor.sh --check"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "./scripts/release-evidence-doctor.sh --assert-complete"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "Proof boundary: packaging prerequisite check only"
  require_output_contains "package check guidance self-test" "$CHECK_OUTPUT" "no app was signed"
  OVERRIDE_OUTPUT=""
  if OVERRIDE_OUTPUT="$(ASSEMBLYWRIGHT_BUNDLE_ID=com.example.assemblywright "$0" --check 2>&1)"; then
    printf '%s\n' "$OVERRIDE_OUTPUT" >&2
    fail "package check guidance self-test expected a non-production bundle identifier to fail"
  fi
  require_output_contains "package bundle identifier self-test" "$OVERRIDE_OUTPUT" "ASSEMBLYWRIGHT_BUNDLE_ID overrides are unsupported"
  printf '\nAssemblywright package check guidance self-test: ok\n'
  printf 'Proof boundary: package --check guidance only; no app was built, signed, notarized, stapled, installed, launched, or manually validated.\n'
  exit 0
fi

if [[ "$ENTITLEMENTS_POLICY_SELF_TEST" == true ]]; then
  run plutil -lint "$ENTITLEMENTS"
  run plutil -lint "$CORE_ENTITLEMENTS"
  python3 - "$ENTITLEMENTS" "$CORE_ENTITLEMENTS" <<'PY'
import plistlib
import sys

app_entitlements_path, core_entitlements_path = sys.argv[1:3]
with open(app_entitlements_path, "rb") as handle:
    app_entitlements = plistlib.load(handle)
with open(core_entitlements_path, "rb") as handle:
    core_entitlements = plistlib.load(handle)

microphone_key = "com.apple.security.device.audio-input"
if microphone_key in app_entitlements or microphone_key in core_entitlements:
    raise SystemExit("entitlement templates must not request microphone access")

for key in (
    "com.apple.security.cs.allow-jit",
    "com.apple.security.cs.allow-unsigned-executable-memory",
    "com.apple.security.cs.disable-library-validation",
):
    if app_entitlements.get(key) is not False:
        raise SystemExit(f"app entitlement {key} must be false")
    if core_entitlements.get(key) is not False:
        raise SystemExit(f"core entitlement {key} must be false")
PY
  printf '\nAssemblywright package entitlements policy self-test: ok\n'
  printf 'Proof boundary: entitlement template policy only; no app was built, signed, notarized, stapled, installed, launched, or manually validated.\n'
  exit 0
fi

if [[ "$VERSION_CONSISTENCY_SELF_TEST" == true ]]; then
  SELF_TEST_OUTPUT=""
  if SELF_TEST_OUTPUT="$(ASSEMBLYWRIGHT_PACKAGE_VERSION_OVERRIDE=9.9.9 "$0" --check 2>&1)"; then
    printf '%s\n' "$SELF_TEST_OUTPUT" >&2
    fail "version consistency self-test expected mismatched package version to fail"
  fi
  require_output_contains "version consistency self-test" "$SELF_TEST_OUTPUT" "package version mismatch"
  printf '\nAssemblywright package version consistency self-test: ok\n'
  printf 'Proof boundary: mismatch guard only; no app was built, signed, notarized, stapled, installed, launched, or manually validated.\n'
  exit 0
fi

assert_package_version_consistency

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
}

canonical_path() {
  python3 - "$1" <<'PY'
import os
import sys

print(os.path.realpath(os.path.abspath(sys.argv[1])))
PY
}

assert_distribution_bundle_not_running() {
  local canonical_app_path
  local app_executable_path
  local core_executable_path
  local candidate_pids=""
  local name
  local name_pids
  local probe_status
  local pid
  local process_records=""
  local pid_records
  local matches

  require_command lsof
  require_command pgrep
  require_command python3

  canonical_app_path="$(canonical_path "$APP_PATH")"
  app_executable_path="$canonical_app_path/Contents/MacOS/$APP_EXECUTABLE_NAME"
  core_executable_path="$canonical_app_path/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"

  for name in "$APP_EXECUTABLE_NAME" "$CORE_EXECUTABLE_NAME"; do
    set +e
    name_pids="$(pgrep -x "$name" 2>/dev/null)"
    probe_status=$?
    set -e
    if [[ "$probe_status" -gt 1 ]]; then
      fail "could not inspect running $name processes before replacing $APP_PATH"
    fi
    if [[ -n "$name_pids" ]]; then
      candidate_pids+="${candidate_pids:+$'\n'}$name_pids"
    fi
  done

  candidate_pids="$(printf '%s\n' "$candidate_pids" | awk 'NF && !seen[$1]++ { print $1 }')"
  while IFS= read -r pid; do
    [[ -n "$pid" ]] || continue
    set +e
    pid_records="$(lsof -a -nP -p "$pid" -d txt -Fpn 2>/dev/null)"
    probe_status=$?
    set -e
    if [[ "$probe_status" -ne 0 ]]; then
      if kill -0 "$pid" 2>/dev/null; then
        fail "could not inspect candidate Assemblywright process $pid before replacing $APP_PATH"
      fi
      continue
    fi
    process_records+="${process_records:+$'\n'}$pid_records"
  done <<<"$candidate_pids"

  matches="$(printf '%s\n' "$process_records" |
    running_distribution_processes_from_lsof "$app_executable_path" "$core_executable_path")"
  reject_running_distribution_processes "$matches"
}

run_running_app_guard_e2e() (
  local tmp_dir
  local fixture_bundle
  local fixture_app
  local fixture_core
  local app_pid=""
  local core_pid=""
  local output=""
  local detected=false
  local attempt

  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/assemblywright-running-app-guard-e2e.XXXXXX")"
  fixture_bundle="$tmp_dir/Assemblywright.app"
  fixture_app="$fixture_bundle/Contents/MacOS/$APP_EXECUTABLE_NAME"
  fixture_core="$fixture_bundle/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"

  cleanup_running_app_guard_e2e() {
    if [[ -n "$app_pid" ]]; then
      kill "$app_pid" 2>/dev/null || true
      wait "$app_pid" 2>/dev/null || true
    fi
    if [[ -n "$core_pid" ]]; then
      kill "$core_pid" 2>/dev/null || true
      wait "$core_pid" 2>/dev/null || true
    fi
    rm -rf "$tmp_dir"
  }
  trap cleanup_running_app_guard_e2e EXIT

  require_command clang
  mkdir -p "$(dirname "$fixture_app")" "$(dirname "$fixture_core")"
  clang -O0 "$ROOT_DIR/scripts/fixtures/running-app-guard-process.c" -o "$fixture_app"
  clang -O0 "$ROOT_DIR/scripts/fixtures/running-app-guard-process.c" -o "$fixture_core"
  "$fixture_app" &
  app_pid=$!
  "$fixture_core" &
  core_pid=$!

  for attempt in $(seq 1 100); do
    output=""
    if ! output="$(APP_PATH="$fixture_bundle" assert_distribution_bundle_not_running 2>&1)"; then
      if [[ "$output" == *"refusing to replace $fixture_bundle"* ]]; then
        detected=true
        break
      fi
      printf '%s\n' "$output" >&2
      fail "running app guard E2E encountered an unexpected inspection failure"
    fi
    sleep 0.02
  done

  [[ "$detected" == true ]] || fail "running app guard E2E did not detect the temporary live bundle"
  require_output_contains "running app guard E2E app PID" "$output" "$app_pid"
  require_output_contains "running app guard E2E core PID" "$output" "$core_pid"
  require_output_contains "running app guard E2E guidance" "$output" "quit Assemblywright with Command-Q"

  kill "$app_pid" "$core_pid"
  wait "$app_pid" 2>/dev/null || true
  wait "$core_pid" 2>/dev/null || true
  app_pid=""
  core_pid=""

  APP_PATH="$fixture_bundle" assert_distribution_bundle_not_running

  printf '\nAssemblywright running app guard E2E: ok\n'
  printf 'Proof boundary: real process-name and text-vnode inspection against temporary app/core stand-ins only; no Assemblywright process or distribution artifact was launched, stopped, replaced, signed, notarized, stapled, installed, or manually validated.\n'
)

if [[ "$RUNNING_APP_GUARD_E2E" == true ]]; then
  run_running_app_guard_e2e
  exit 0
fi

run_provenance_self_test() {
  local tmp_dir
  local stub_dir
  local real_path
  local output
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/assemblywright-provenance-self-test.XXXXXX")"
  stub_dir="$tmp_dir/bin"
  real_path="$PATH"
  local DIST_DIR="$tmp_dir/dist"
  local APP_PATH="$DIST_DIR/$APP_NAME.app"
  local ZIP_PATH="$DIST_DIR/$APP_NAME-$VERSION.zip"
  local PKG_PATH="$DIST_DIR/$APP_NAME-$VERSION.pkg"
  local PROVENANCE_PATH="$DIST_DIR/$APP_NAME-$VERSION-signed-provenance.json"
  local ZIP_NOTARY_LOG="$DIST_DIR/notary-logs/$APP_NAME-$VERSION-app-zip-notarytool.log"
  local PKG_NOTARY_LOG="$DIST_DIR/notary-logs/$APP_NAME-$VERSION-installer-pkg-notarytool.log"
  mkdir -p "$stub_dir" "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Resources/bin" "$(dirname "$ZIP_NOTARY_LOG")"

  cleanup_provenance_self_test() {
    PATH="$real_path"
    rm -rf "$tmp_dir"
  }
  trap cleanup_provenance_self_test EXIT

  cat >"$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" <<'SH'
#!/usr/bin/env bash
printf 'assemblywright %s\n' "${ASSEMBLYWRIGHT_PACKAGE_STUB_VERSION:?}"
SH
  chmod 755 "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
  chmod 755 "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
  copy_brand_resources "$APP_PATH"
  cat >"$APP_PATH/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIconFile</key>
  <string>$APP_ICON_FILE</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_ID</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
</dict>
</plist>
PLIST
  python3 - "$APP_PATH" "$ZIP_PATH" <<'PY'
import pathlib
import sys
import zipfile

app_path = pathlib.Path(sys.argv[1])
zip_path = pathlib.Path(sys.argv[2])
with zipfile.ZipFile(zip_path, "w") as archive:
    for path in sorted(app_path.rglob("*")):
        if path.is_file():
            archive.write(path, pathlib.Path("Assemblywright.app") / path.relative_to(app_path))
PY
  printf 'pkg artifact\n' >"$PKG_PATH"
  cat >"$ZIP_NOTARY_LOG" <<'LOG'
id: 00000000-0000-4000-8000-000000000011
status: Accepted
LOG
  cat >"$PKG_NOTARY_LOG" <<'LOG'
id: 00000000-0000-4000-8000-000000000012
status: Accepted
LOG

  cat >"$stub_dir/codesign" <<'SH'
#!/usr/bin/env bash
identifier="${ASSEMBLYWRIGHT_PACKAGE_STUB_BUNDLE_ID:?}"
if [[ "${*: -1}" == *"/assemblywright-cli" ]]; then
  identifier="${identifier}.core"
fi
if [[ "${*: -1}" == *"/AssemblywrightMacApp" ]]; then
  identifier="${ASSEMBLYWRIGHT_PACKAGE_STUB_APP_EXECUTABLE_IDENTIFIER:-$identifier}"
fi
team_identifier="${ASSEMBLYWRIGHT_PACKAGE_STUB_TEAM_IDENTIFIER:-9VZ742YKV4}"
cdhash="${ASSEMBLYWRIGHT_PACKAGE_STUB_CDHASH:-0123456789abcdef0123456789abcdef01234567}"
printf 'Executable=/fixture\nIdentifier=%s\nAuthority=Developer ID Application: Assemblywright QA Fixture\nTeamIdentifier=%s\nCDHash=%s\n' \
  "$identifier" "$team_identifier" "$cdhash"
SH
  cat >"$stub_dir/pkgutil" <<'SH'
#!/usr/bin/env bash
printf 'Status: signed by a developer certificate issued by Apple for distribution\nDeveloper ID Installer: Assemblywright QA Fixture\n'
SH
  cat >"$stub_dir/xcrun" <<'SH'
#!/usr/bin/env bash
if [[ "${1:-}" == "stapler" && "${2:-}" == "validate" ]]; then
  printf 'The validate action worked!\n'
  exit 0
fi
printf 'unexpected xcrun invocation: %s\n' "$*" >&2
exit 1
SH
  cat >"$stub_dir/spctl" <<'SH'
#!/usr/bin/env bash
if [[ "${ASSEMBLYWRIGHT_PACKAGE_STUB_GATEKEEPER_MODE:-accepted}" == "negated" ]]; then
  printf '%s: rejected: not accepted\n' "${@: -1}"
  exit 0
fi
printf '%s: accepted\n' "${@: -1}"
SH
  chmod 755 "$stub_dir/codesign" "$stub_dir/pkgutil" "$stub_dir/xcrun" "$stub_dir/spctl"

  PATH="$stub_dir:$PATH" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_BUNDLE_ID="$BUNDLE_ID" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_VERSION="$VERSION" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION="Developer ID Application: Assemblywright QA Fixture" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER="Developer ID Installer: Assemblywright QA Fixture" \
    write_signed_distribution_provenance

  python3 - "$PROVENANCE_PATH" "$VERSION" "$APP_PATH" "$ZIP_PATH" "$PKG_PATH" <<'PY'
import hashlib
import json
import sys
import uuid

path, version, app_path, zip_path, pkg_path = sys.argv[1:6]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)

assert data["schema_version"] == 1
assert data["evidence_type"] == "signed_distribution_provenance"
assert data["version"] == version
assert data["artifacts"]["app_path"] == app_path
assert data["artifacts"]["zip_path"] == zip_path
assert data["artifacts"]["pkg_path"] == pkg_path
assert data["artifacts"]["app_executable_path"] == f"{app_path}/Contents/MacOS/AssemblywrightMacApp"
with open(data["artifacts"]["app_executable_path"], "rb") as handle:
    assert data["artifacts"]["app_executable_sha256"] == hashlib.sha256(handle.read()).hexdigest()
assert data["artifacts"]["bundled_core_version"] == f"assemblywright {version}"
assert data["signing"]["developer_id_application_identity"].startswith("Developer ID Application: ")
assert data["signing"]["developer_id_installer_identity"].startswith("Developer ID Installer: ")
for key in ("app_bundle_codesign", "app_executable_codesign", "bundled_core_codesign"):
    assert "Authority=Developer ID Application: " in data["signing"][key]
assert data["signing"]["app_executable_identifier"] == "com.nobiletechnology.assemblywright"
assert data["signing"]["app_executable_team_identifier"] == "9VZ742YKV4"
assert data["signing"]["app_executable_cdhash"] == "0123456789abcdef0123456789abcdef01234567"
assert "Developer ID Installer: " in data["signing"]["installer_pkg_signature"]
uuid.UUID(data["notarization"]["app_zip_submission_id"])
uuid.UUID(data["notarization"]["installer_pkg_submission_id"])
assert data["notarization"]["app_zip_status"] == "Accepted"
assert data["notarization"]["installer_pkg_status"] == "Accepted"
for path_key, digest_key in (
    ("app_zip_notary_log", "app_zip_notary_log_sha256"),
    ("installer_pkg_notary_log", "installer_pkg_notary_log_sha256"),
):
    with open(data["notarization"][path_key], "rb") as handle:
        assert data["notarization"][digest_key] == hashlib.sha256(handle.read()).hexdigest()
assert "The validate action worked!" in data["stapling"]["app_bundle_validation"]
assert "The validate action worked!" in data["stapling"]["installer_pkg_validation"]
assert data["gatekeeper"]["app_bundle_assessment"].strip().endswith(": accepted")
assert data["gatekeeper"]["installer_pkg_assessment"].strip().endswith(": accepted")
for key, value in data["validation_flags"].items():
    assert value is True, key
PY

  set +e
  output="$(PATH="$stub_dir:$PATH" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_BUNDLE_ID="$BUNDLE_ID" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_VERSION="$VERSION" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_APP_EXECUTABLE_IDENTIFIER="com.example.WrongAssemblywright" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION="Developer ID Application: Assemblywright QA Fixture" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER="Developer ID Installer: Assemblywright QA Fixture" \
    write_signed_distribution_provenance 2>&1)"
  set -e
  require_output_contains "signed provenance app executable identifier self-test" "$output" "app executable codesign identifier mismatch"

  set +e
  output="$(PATH="$stub_dir:$PATH" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_BUNDLE_ID="$BUNDLE_ID" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_VERSION="$VERSION" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_TEAM_IDENTIFIER="missing" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION="Developer ID Application: Assemblywright QA Fixture" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER="Developer ID Installer: Assemblywright QA Fixture" \
    write_signed_distribution_provenance 2>&1)"
  set -e
  require_output_contains "signed provenance app executable team self-test" "$output" "TeamIdentifier must be a 10-character Apple team identifier"

  output="$(PATH="$stub_dir:$PATH" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_BUNDLE_ID="$BUNDLE_ID" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_VERSION="$VERSION" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_GATEKEEPER_MODE=negated \
    ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION="Developer ID Application: Assemblywright QA Fixture" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER="Developer ID Installer: Assemblywright QA Fixture" \
    write_signed_distribution_provenance 2>&1 || true)"
  require_output_contains "signed provenance negated Gatekeeper self-test" "$output" "Gatekeeper accepted result"

  python3 - "$ZIP_PATH" <<'PY'
import pathlib
import sys
import zipfile

zip_path = pathlib.Path(sys.argv[1])
with zipfile.ZipFile(zip_path, "w") as archive:
    archive.writestr("Assemblywright.app/Contents/MacOS/AssemblywrightMacApp", "")
    archive.writestr("Assemblywright.app/Contents/Resources/bin/assemblywright-cli", "")
    archive.writestr("Assemblywright.app/Contents/Resources/Assemblywright.icns", "")
    archive.writestr("Assemblywright.app/Contents/Info.plist", "")
    archive.writestr("payload/Assemblywright.app/Contents/MacOS/AssemblywrightMacApp", "")
    archive.writestr("payload/Assemblywright.app/Contents/Resources/bin/assemblywright-cli", "")
    archive.writestr("payload/Assemblywright.app/Contents/Resources/Assemblywright.icns", "")
    archive.writestr("payload/Assemblywright.app/Contents/Info.plist", "")
PY
  output="$(PATH="$stub_dir:$PATH" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_BUNDLE_ID="$BUNDLE_ID" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_VERSION="$VERSION" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION="Developer ID Application: Assemblywright QA Fixture" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER="Developer ID Installer: Assemblywright QA Fixture" \
    write_signed_distribution_provenance 2>&1 || true)"
  require_output_contains "signed provenance nested app zip self-test" "$output" "zip payload contains nested Assemblywright.app entries"

  python3 - "$APP_PATH" "$ZIP_PATH" <<'PY'
import pathlib
import sys
import zipfile

app_path = pathlib.Path(sys.argv[1])
zip_path = pathlib.Path(sys.argv[2])
with zipfile.ZipFile(zip_path, "w") as archive:
    for path in sorted(app_path.rglob("*")):
        if path.is_file():
            archive.write(path, pathlib.Path("Assemblywright.app") / path.relative_to(app_path))
PY

  python3 - "$APP_PATH" "$ZIP_PATH" <<'PY'
import pathlib
import sys
import zipfile

app_path = pathlib.Path(sys.argv[1])
zip_path = pathlib.Path(sys.argv[2])
with zipfile.ZipFile(zip_path, "w") as archive:
    for path in sorted(app_path.rglob("*")):
        if path.is_file():
            archive.write(path, pathlib.Path("Assemblywright.app") / path.relative_to(app_path))
PY

  cat >"$ZIP_NOTARY_LOG" <<'LOG'
id: not-a-submission-id
status: Accepted
LOG
  output=""
  output="$(PATH="$stub_dir:$PATH" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_BUNDLE_ID="$BUNDLE_ID" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_VERSION="$VERSION" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION="Developer ID Application: Assemblywright QA Fixture" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER="Developer ID Installer: Assemblywright QA Fixture" \
    write_signed_distribution_provenance 2>&1 || true)"
  require_output_contains "signed provenance bad-notary self-test" "$output" "app zip notary submission id must be a UUID"

  cat >"$ZIP_NOTARY_LOG" <<'LOG'
id: 00000000-0000-4000-8000-000000000011
status: Rejected
LOG
  set +e
  output="$(PATH="$stub_dir:$PATH" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_BUNDLE_ID="$BUNDLE_ID" \
    ASSEMBLYWRIGHT_PACKAGE_STUB_VERSION="$VERSION" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION="Developer ID Application: Assemblywright QA Fixture" \
    ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER="Developer ID Installer: Assemblywright QA Fixture" \
    write_signed_distribution_provenance 2>&1)"
  set -e
  require_output_contains "signed provenance rejected-notary self-test" "$output" "app zip notary status"

  trap - EXIT
  PATH="$real_path"
  rm -rf "$tmp_dir"

  printf '\nAssemblywright signed provenance self-test: ok\n'
  printf 'Proof boundary: stubbed signed-provenance writer and Apple-tool output guards only; no app was signed, notarized, stapled, installed, launched, or manually validated.\n'
}

copy_brand_resources() {
  local app_path="$1"
  local resources="$app_path/Contents/Resources"
  local source

  source="$BRAND_GENERATED_DIR/$APP_ICON_FILE"
  [[ -f "$source" ]] || fail "brand app icon missing: $source (run scripts/generate-brand-assets.sh)"
  cp "$source" "$resources/$APP_ICON_FILE"

  local template
  for template in "${MENU_BAR_TEMPLATE_FILES[@]}"; do
    source="$BRAND_GENERATED_DIR/$template"
    [[ -f "$source" ]] || fail "brand menu bar template missing: $source (run scripts/generate-brand-assets.sh)"
    cp "$source" "$resources/$template"
  done
}

assert_brand_resources() {
  local label="$1"
  local app_path="$2"
  local resources="$app_path/Contents/Resources"

  [[ -f "$resources/$APP_ICON_FILE" ]] || fail "$label bundle is missing Contents/Resources/$APP_ICON_FILE"

  local template
  for template in "${MENU_BAR_TEMPLATE_FILES[@]}"; do
    [[ -f "$resources/$template" ]] || fail "$label bundle is missing Contents/Resources/$template"
  done
}

build_app_bundle() {
  assert_distribution_bundle_not_running
  rm -rf "$DIST_DIR"
  mkdir -p "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Resources/bin"

  run cargo build --release -p assemblywright-cli
  run swift build --disable-sandbox -c release --package-path apps/mac

  SWIFT_BIN_DIR="$(swift build --disable-sandbox -c release --package-path apps/mac --show-bin-path)"
  SWIFT_EXECUTABLE="$SWIFT_BIN_DIR/$SWIFT_APP_PRODUCT"
  CORE_EXECUTABLE="$ROOT_DIR/target/release/assemblywright"

  [[ -x "$SWIFT_EXECUTABLE" ]] || fail "Swift release executable missing: $SWIFT_EXECUTABLE"
  [[ -x "$CORE_EXECUTABLE" ]] || fail "Rust release executable missing: $CORE_EXECUTABLE"

  cp "$SWIFT_EXECUTABLE" "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
  cp "$CORE_EXECUTABLE" "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  printf 'assemblywright %s\n' "$VERSION" >"$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME.version"
  chmod 755 "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME" "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  assert_bundled_core_version

  copy_brand_resources "$APP_PATH"

  cat >"$APP_PATH/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>$APP_NAME</string>
  <key>CFBundleExecutable</key>
  <string>$APP_EXECUTABLE_NAME</string>
  <key>CFBundleIconFile</key>
  <string>$APP_ICON_FILE</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_ID</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>$APP_NAME</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

  run plutil -lint "$APP_PATH/Contents/Info.plist"

  INFO_PLIST_CONTENTS="$(cat "$APP_PATH/Contents/Info.plist")"
  require_output_contains "Info.plist" "$INFO_PLIST_CONTENTS" "<string>$APP_EXECUTABLE_NAME</string>"
  require_output_contains "Info.plist" "$INFO_PLIST_CONTENTS" "<string>$BUNDLE_ID</string>"
  require_output_contains "Info.plist" "$INFO_PLIST_CONTENTS" "<string>APPL</string>"
  require_output_contains "Info.plist" "$INFO_PLIST_CONTENTS" "CFBundleIconFile"
  require_output_contains "Info.plist" "$INFO_PLIST_CONTENTS" "<string>$APP_ICON_FILE</string>"
  assert_brand_resources "release app" "$APP_PATH"
}

validate_package_metadata() {
  local pkg_path="$1"
  local expected_identifier="$2"
  local label="${3:-package}"
  local tmp_dir
  local expanded_dir
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/assemblywright-pkg-metadata.XXXXXX")"
  expanded_dir="$tmp_dir/expanded"

  if ! pkgutil --expand "$pkg_path" "$expanded_dir" >/dev/null; then
    rm -rf "$tmp_dir"
    fail "failed to expand $label for metadata validation: $pkg_path"
  fi

  if ! python3 - "$expanded_dir/PackageInfo" "$expected_identifier" "$VERSION" <<'PY'
import sys
import xml.etree.ElementTree as ET

package_info, expected_identifier, expected_version = sys.argv[1:4]
root = ET.parse(package_info).getroot()

checks = {
    "identifier": expected_identifier,
    "version": expected_version,
    "install-location": "/Applications",
}
for key, expected in checks.items():
    actual = root.attrib.get(key)
    if actual != expected:
        raise SystemExit(
            f"package metadata {key} mismatch: expected {expected!r}, got {actual!r}"
        )
PY
  then
    rm -rf "$tmp_dir"
    fail "$label metadata validation failed: $pkg_path"
  fi

  rm -rf "$tmp_dir"
}

notary_args=()
if [[ -n "${ASSEMBLYWRIGHT_NOTARYTOOL_PROFILE:-}" ]]; then
  notary_args=(--keychain-profile "$ASSEMBLYWRIGHT_NOTARYTOOL_PROFILE")
elif [[ -n "${ASSEMBLYWRIGHT_NOTARYTOOL_APPLE_ID:-}" ]] &&
  [[ -n "${ASSEMBLYWRIGHT_NOTARYTOOL_TEAM_ID:-}" ]] &&
  [[ -n "${ASSEMBLYWRIGHT_NOTARYTOOL_PASSWORD:-}" ]]; then
  notary_args=(
    --apple-id "$ASSEMBLYWRIGHT_NOTARYTOOL_APPLE_ID"
    --team-id "$ASSEMBLYWRIGHT_NOTARYTOOL_TEAM_ID"
    --password "$ASSEMBLYWRIGHT_NOTARYTOOL_PASSWORD"
  )
fi

require_command cargo
require_command swift
require_command plutil
require_command pkgbuild
require_command pkgutil

if [[ "$UNSIGNED_STRUCTURE_CHECK" != true && "$UNSIGNED_LAUNCH_CHECK" != true ]]; then
  require_command codesign
  require_command xcrun
  require_command ditto
fi

[[ -f "$ENTITLEMENTS" ]] || fail "missing entitlements file: $ENTITLEMENTS"
[[ -f "$CORE_ENTITLEMENTS" ]] || fail "missing bundled core entitlements file: $CORE_ENTITLEMENTS"
run plutil -lint "$ENTITLEMENTS"
run plutil -lint "$CORE_ENTITLEMENTS"

if [[ "$CHECK_ONLY" == true ]]; then
  if [[ -z "${ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION:-}" ]]; then
    printf 'warning: ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION is not set; full signing will fail until configured.\n' >&2
  fi
  if [[ -z "${ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER:-}" ]]; then
    printf 'warning: ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER is not set; full installer signing will fail until configured.\n' >&2
  fi
  if [[ ${#notary_args[@]} -eq 0 ]]; then
    printf 'warning: notarization credentials are not set; full notarization will fail until configured.\n' >&2
  fi
  printf '\nAssemblywright distribution packaging preflight: ok\n'
  cat <<'CHECKLIST'

Next release evidence commands:
  cargo run -p assemblywright-cli -- release signed-distribution-runbook
  ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' ASSEMBLYWRIGHT_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh
  # Alternative notarization auth:
  ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' ASSEMBLYWRIGHT_NOTARYTOOL_APPLE_ID='apple-id@example.com' ASSEMBLYWRIGHT_NOTARYTOOL_TEAM_ID='TEAMID1234' ASSEMBLYWRIGHT_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh
  ./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env
  Set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' in target/release-live-device-qa.env
  Launch Assemblywright with ASSEMBLYWRIGHT_MAC_ENABLE_IPC_CLI_HANDOFF=true for the operator evidence session, then confirm ASSEMBLYWRIGHT_IPC_TOKEN_FILE points to the app-owned ipc-session-auth.json path before IPC commands
  set -a && source target/release-live-device-qa.env && set +a
  cargo run -p assemblywright-cli -- command "status check" --endpoint "${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}" --json
  record the returned task ID as ASSEMBLYWRIGHT_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid>' or a task-associated audit ID as 'audit:<uuid>'
  set -a && source target/release-live-device-qa.env && set +a
  ./scripts/release-live-device-qa.sh --assert-complete
  ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release evidence-status --endpoint "${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}"
  ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release readiness --endpoint "${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}"
  ./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env
  set -a && source target/release-plugin-trust-qa.env && set +a
  ./scripts/release-plugin-trust-qa.sh --assert-complete
  ./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env
  set -a && source target/release-evidence-bundle.env && set +a
  ./scripts/release-evidence-bundle.sh --bundle
  ./scripts/release-evidence-doctor.sh --check
  ./scripts/release-evidence-doctor.sh --assert-complete

Proof boundary: packaging prerequisite check only; no app was signed,
notarized, stapled, installed, Finder-launched, live-device validated, or
manually approved.
CHECKLIST
  exit 0
fi

run_unsigned_structure_check() {
  build_app_bundle

  SIGNING_STATUS="not attempted"
  if command -v codesign >/dev/null 2>&1; then
    run codesign --force --sign - --identifier "$CORE_CODE_ID" --entitlements "$CORE_ENTITLEMENTS" "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
    run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
    run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_PATH"
    run codesign --verify --deep --strict "$APP_PATH"
    assert_app_core_code_identifiers "unsigned structure"
    assert_app_audio_input_entitlement "unsigned structure app"
    assert_bundled_core_no_audio_input_entitlement "unsigned structure bundled core"
    SIGNING_STATUS="ad-hoc signed with codesign -"
  fi

  PKG_PATH="$DIST_DIR/$APP_NAME-$VERSION-unsigned-structure.pkg"
  rm -f "$PKG_PATH"
  run pkgbuild \
    --component "$APP_PATH" \
    --install-location /Applications \
    --identifier "$BUNDLE_ID.unsigned-structure.pkg" \
    --version "$VERSION" \
    "$PKG_PATH"

  validate_package_metadata "$PKG_PATH" "$BUNDLE_ID.unsigned-structure.pkg" "unsigned structure package"
  PAYLOAD_OUTPUT="$(pkgutil --payload-files "$PKG_PATH")"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Assemblywright.app/Contents/MacOS/$APP_EXECUTABLE_NAME"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Assemblywright.app/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Assemblywright.app/Contents/Info.plist"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Assemblywright.app/Contents/Resources/$APP_ICON_FILE"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Assemblywright.app/Contents/Resources/menubar-template.png"

  printf '\nAssemblywright unsigned distribution structure check: ok\n'
  printf 'App: %s\n' "$APP_PATH"
  printf 'Pkg: %s\n' "$PKG_PATH"
  printf 'Signing: %s\n' "$SIGNING_STATUS"
  printf 'Proof boundary: release app and unsigned installer payload structure only; no Developer ID signing, notarization, stapling, /Applications install, Finder launch, App Store validation, or manual QA.\n'
}

run_unsigned_launch_check() {
  require_command codesign
  require_command curl
  require_command lsof
  require_command pgrep
  require_command sqlite3
  build_app_bundle

  run codesign --force --sign - --identifier "$CORE_CODE_ID" --entitlements "$CORE_ENTITLEMENTS" "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
  run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_PATH"
  run codesign --verify --deep --strict "$APP_PATH"
  assert_app_core_code_identifiers "unsigned launch"
  SIGNING_STATUS="ad-hoc signed with codesign -"

  PKG_PATH="$DIST_DIR/$APP_NAME-$VERSION-unsigned-launch.pkg"
  rm -f "$PKG_PATH"
  run pkgbuild \
    --component "$APP_PATH" \
    --install-location /Applications \
    --identifier "$BUNDLE_ID.unsigned-launch.pkg" \
    --version "$VERSION" \
    "$PKG_PATH"

  validate_package_metadata "$PKG_PATH" "$BUNDLE_ID.unsigned-launch.pkg" "unsigned launch package"
  PAYLOAD_OUTPUT="$(pkgutil --payload-files "$PKG_PATH")"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Assemblywright.app/Contents/MacOS/$APP_EXECUTABLE_NAME"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Assemblywright.app/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Assemblywright.app/Contents/Info.plist"

  # Keep the isolated HOME short enough for macOS sockaddr_un.sun_path (103 bytes).
  LAUNCH_TMP_DIR="$(mktemp -d "/tmp/aw-dl.XXXXXX")"
  APP_PID=""
  CLEAN_HOME="$LAUNCH_TMP_DIR/h"
  APP_LOG="$LAUNCH_TMP_DIR/AssemblywrightMacApp.log"
  mkdir -p "$CLEAN_HOME"

  stop_launch() {
    local child_pids=("")
    local orphaned_child=false
    if [[ -n "$APP_PID" ]]; then
      while IFS= read -r pid; do
        if [[ -n "$pid" ]]; then
          child_pids+=("$pid")
        fi
      done < <(pgrep -P "$APP_PID" 2>/dev/null || true)
    fi
    if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
      kill -KILL "$APP_PID" 2>/dev/null || true
      wait "$APP_PID" 2>/dev/null || true
    fi
    APP_PID=""

    for _ in {1..40}; do
      local child_alive=false
      for pid in "${child_pids[@]:-}"; do
        if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
          child_alive=true
          break
        fi
      done
      if [[ "$child_alive" == false ]]; then
        break
      fi
      sleep 0.1
    done
    for pid in "${child_pids[@]:-}"; do
      if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        orphaned_child=true
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
      fi
    done
    if [[ "$orphaned_child" == true ]]; then
      return 1
    fi
    return 0
  }

  cleanup_launch() {
    stop_launch || true
    rm -rf "$LAUNCH_TMP_DIR"
  }
  trap cleanup_launch EXIT

  printf '\n==> Launching release app in an isolated HOME with Developer Mode default-off\n'
  env -u ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_EXECUTABLE -u ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER \
    HOME="$CLEAN_HOME" \
    "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME" >"$APP_LOG" 2>&1 &
  APP_PID="$!"

  # The app must stay up on its own. With Developer Mode opt-in absent it must
  # not spawn a helper child, open any listener, or write credential state.
  for _ in {1..20}; do
    if ! kill -0 "$APP_PID" 2>/dev/null; then
      printf 'error: release app exited during the isolated-HOME launch check; app log follows\n' >&2
      cat "$APP_LOG" >&2 || true
      exit 1
    fi
    sleep 0.25
  done

  CHILD_PIDS="$(pgrep -P "$APP_PID" 2>/dev/null || true)"
  [[ -z "$CHILD_PIDS" ]] || fail "default app launch supervised a child process without Developer Mode opt-in"
  LISTENERS="$(lsof -a -p "$APP_PID" -iTCP -sTCP:LISTEN -P -n 2>/dev/null || true)"
  [[ -z "$LISTENERS" ]] || fail "default app launch exposed a TCP listener: $LISTENERS"

  stop_launch || fail "release app left an orphaned child after abrupt termination"

  printf '\nAssemblywright unsigned distribution launch check: ok\n'
  printf 'App: %s\n' "$APP_PATH"
  printf 'Pkg: %s\n' "$PKG_PATH"
  printf 'Signing: %s\n' "$SIGNING_STATUS"
  printf 'Clean HOME: %s\n' "$CLEAN_HOME"
  printf 'Proof boundary: release-built app executable, bundled CLI, stable ad-hoc app/CLI identifiers, unsigned installer payload structure, and an isolated-HOME launch that stays up with Developer Mode default-off, spawns no helper child, opens no TCP listener, and leaves no orphan after abrupt termination. Ad-hoc cdhash evidence does not prove Developer ID publisher identity, and this lane does not prove Developer ID signing, notarization, stapling, /Applications install, Finder/LaunchServices validation, device authentication, App Sandbox, live Developer Mode bridge connectivity, or manual QA.\n'
}

if [[ "$UNSIGNED_STRUCTURE_CHECK" == true ]]; then
  run_unsigned_structure_check
  exit 0
fi

if [[ "$UNSIGNED_LAUNCH_CHECK" == true ]]; then
  run_unsigned_launch_check
  exit 0
fi

if [[ "$PROVENANCE_SELF_TEST" == true ]]; then
  run_provenance_self_test
  exit 0
fi

[[ -n "${ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION:-}" ]] ||
  fail "ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION must name a Developer ID Application identity"
[[ -n "${ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER:-}" ]] ||
  fail "ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER must name a Developer ID Installer identity"
[[ ${#notary_args[@]} -gt 0 ]] ||
  fail "notarization credentials are required; set ASSEMBLYWRIGHT_NOTARYTOOL_PROFILE or Apple ID/team/password vars"

build_app_bundle

NOTARY_LOG_DIR="$DIST_DIR/notary-logs"
ZIP_NOTARY_LOG="$NOTARY_LOG_DIR/$APP_NAME-$VERSION-app-zip-notarytool.log"
PKG_NOTARY_LOG="$NOTARY_LOG_DIR/$APP_NAME-$VERSION-installer-pkg-notarytool.log"
mkdir -p "$NOTARY_LOG_DIR"

run codesign --force --timestamp --options runtime \
  --identifier "$CORE_CODE_ID" \
  --entitlements "$CORE_ENTITLEMENTS" \
  --sign "$ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION" \
  "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
run codesign --force --timestamp --options runtime \
  --entitlements "$ENTITLEMENTS" \
  --sign "$ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION" \
  "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
run codesign --force --timestamp --options runtime \
  --entitlements "$ENTITLEMENTS" \
  --sign "$ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION" \
  "$APP_PATH"
run codesign --verify --deep --strict --verbose=2 "$APP_PATH"
assert_app_core_code_identifiers "signed distribution"
assert_app_audio_input_entitlement "signed app"
assert_bundled_core_no_audio_input_entitlement "signed bundled core"

rm -f "$ZIP_PATH"
run ditto -c -k --keepParent "$APP_PATH" "$ZIP_PATH"
validate_app_zip_payload "$ZIP_PATH"
capture_command "app zip notarization" "$ZIP_NOTARY_LOG" xcrun notarytool submit "$ZIP_PATH" "${notary_args[@]}" --wait
run xcrun stapler staple "$APP_PATH"
run xcrun stapler validate "$APP_PATH"

rm -f "$PKG_PATH"
run pkgbuild \
  --component "$APP_PATH" \
  --install-location /Applications \
  --identifier "$BUNDLE_ID.pkg" \
  --version "$VERSION" \
  --sign "$ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER" \
  "$PKG_PATH"
validate_package_metadata "$PKG_PATH" "$BUNDLE_ID.pkg" "signed installer package"
run pkgutil --check-signature "$PKG_PATH"
capture_command "installer package notarization" "$PKG_NOTARY_LOG" xcrun notarytool submit "$PKG_PATH" "${notary_args[@]}" --wait
run xcrun stapler staple "$PKG_PATH"
run xcrun stapler validate "$PKG_PATH"
write_signed_distribution_provenance

printf '\nAssemblywright distribution package: ok\n'
printf 'App: %s\n' "$APP_PATH"
printf 'Zip: %s\n' "$ZIP_PATH"
printf 'Pkg: %s\n' "$PKG_PATH"
printf 'Signed provenance: %s\n' "$PROVENANCE_PATH"
printf 'Proof boundary: signed, notarized app zip and signed, notarized installer package only; clean-profile install, Finder launch, and App Store validation remain manual release checks.\n'
