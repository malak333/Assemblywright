#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
export CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-$ROOT_DIR/target/clang-module-cache}"
mkdir -p "$CLANG_MODULE_CACHE_PATH"

VERSION="${JARVIS_PACKAGE_VERSION_OVERRIDE:-$("$ROOT_DIR/scripts/release-version.sh")}"
BUNDLE_ID="${JARVIS_BUNDLE_ID:-com.nobiletechnology.jarvis}"
APP_NAME="Jarvis"
APP_EXECUTABLE_NAME="JarvisMacApp"
CORE_EXECUTABLE_NAME="jarvis-cli"
ENTITLEMENTS="$ROOT_DIR/packaging/Jarvis.entitlements"
DIST_DIR="${JARVIS_DISTRIBUTION_DIR:-$ROOT_DIR/target/distribution}"
APP_PATH="$DIST_DIR/$APP_NAME.app"
ZIP_PATH="$DIST_DIR/$APP_NAME-$VERSION.zip"
PKG_PATH="$DIST_DIR/$APP_NAME-$VERSION.pkg"
PROVENANCE_PATH="${JARVIS_SIGNED_PROVENANCE_PATH:-$DIST_DIR/$APP_NAME-$VERSION-signed-provenance.json}"
CHECK_ONLY=false
UNSIGNED_STRUCTURE_CHECK=false
UNSIGNED_LAUNCH_CHECK=false
VERSION_CONSISTENCY_SELF_TEST=false

usage() {
  cat <<'USAGE'
Usage: scripts/package-distribution.sh [--check] [--unsigned-structure-check] [--unsigned-launch-check] [--version-consistency-self-test]

Build a distribution-shaped Jarvis.app bundle, sign it with Developer ID, zip it,
submit it for notarization, staple the ticket, then build, sign, notarize, and
staple a Developer ID Installer package for /Applications installation.

Required for full distribution packaging:
  JARVIS_DEVELOPER_ID_APPLICATION  Developer ID Application signing identity
  JARVIS_DEVELOPER_ID_INSTALLER     Developer ID Installer signing identity

Required for notarization, choose one:
  JARVIS_NOTARYTOOL_PROFILE        Stored notarytool keychain profile
  or
  JARVIS_NOTARYTOOL_APPLE_ID
  JARVIS_NOTARYTOOL_TEAM_ID
  JARVIS_NOTARYTOOL_PASSWORD       App-specific password

Optional:
  JARVIS_BUNDLE_ID                 Defaults to com.nobiletechnology.jarvis
  JARVIS_DISTRIBUTION_DIR          Defaults to target/distribution
  JARVIS_SIGNED_PROVENANCE_PATH    Defaults to target/distribution/Jarvis-<version>-signed-provenance.json

--check validates local tool/template preconditions without signing or notarizing.
--unsigned-structure-check builds and inspects an unsigned app/pkg layout without
Developer ID credentials, notarization, stapling, Finder launch, or live device
validation.
--unsigned-launch-check also launches the release-built app executable with an
isolated HOME and exercises the supervised core over loopback IPC. It still does
not prove Developer ID signing, notarization, stapling, Finder launch, or live
device validation.
--version-consistency-self-test verifies package/crate version drift is rejected
without signing, notarizing, or building distribution artifacts.
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
  [[ -f "$marker_path" ]] || fail "bundled core version marker missing: $marker_path; rerun this packaging command so the app bundle is rebuilt from the current jarvis-cli"
  require_output_contains "bundled core version marker" "$(tr -d '\r\n' <"$marker_path")" "jarvis $VERSION"
  output="$("$core_path" --version)"
  require_output_contains "bundled core version" "$output" "jarvis $VERSION"
}

assert_app_audio_input_entitlement() {
  local label="$1"
  local output
  output="$(codesign -d --entitlements :- "$APP_PATH" 2>/dev/null)"
  if [[ "$output" != *"com.apple.security.device.audio-input"* ]]; then
    printf 'error: %s entitlements do not include microphone access\n' "$label" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
}

write_signed_distribution_provenance() {
  local generated_at
  local zip_sha
  local pkg_sha
  local bundled_core_version
  local app_codesign
  local core_codesign
  local app_executable_codesign
  local pkg_signature
  local app_staple
  local pkg_staple
  local app_gatekeeper
  local pkg_gatekeeper
  local zip_submission_id
  local pkg_submission_id
  local proof_boundary

  require_command python3
  require_command shasum
  require_command spctl

  generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  zip_sha="$(file_sha256 "$ZIP_PATH")"
  pkg_sha="$(file_sha256 "$PKG_PATH")"
  bundled_core_version="$("$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" --version)"
  require_output_contains "bundled core version" "$bundled_core_version" "jarvis $VERSION"
  app_codesign="$(codesign -dv "$APP_PATH" 2>&1)"
  core_codesign="$(codesign -dv "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" 2>&1)"
  app_executable_codesign="$(codesign -dv "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME" 2>&1)"
  pkg_signature="$(pkgutil --check-signature "$PKG_PATH" 2>&1)"
  app_staple="$(xcrun stapler validate "$APP_PATH" 2>&1)"
  pkg_staple="$(xcrun stapler validate "$PKG_PATH" 2>&1)"
  app_gatekeeper="$(spctl --assess --type execute --verbose "$APP_PATH" 2>&1)"
  pkg_gatekeeper="$(spctl --assess --type install --verbose "$PKG_PATH" 2>&1)"
  zip_submission_id="$(extract_notary_submission_id <"$ZIP_NOTARY_LOG")"
  pkg_submission_id="$(extract_notary_submission_id <"$PKG_NOTARY_LOG")"
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
    PROVENANCE_BUNDLED_CORE_VERSION="$bundled_core_version" \
    PROVENANCE_DEVELOPER_ID_APPLICATION="$JARVIS_DEVELOPER_ID_APPLICATION" \
    PROVENANCE_DEVELOPER_ID_INSTALLER="$JARVIS_DEVELOPER_ID_INSTALLER" \
    PROVENANCE_APP_CODESIGN="$app_codesign" \
    PROVENANCE_APP_EXECUTABLE_CODESIGN="$app_executable_codesign" \
    PROVENANCE_CORE_CODESIGN="$core_codesign" \
    PROVENANCE_PKG_SIGNATURE="$pkg_signature" \
    PROVENANCE_ZIP_SUBMISSION_ID="$zip_submission_id" \
    PROVENANCE_PKG_SUBMISSION_ID="$pkg_submission_id" \
    PROVENANCE_ZIP_NOTARY_LOG="$ZIP_NOTARY_LOG" \
    PROVENANCE_PKG_NOTARY_LOG="$PKG_NOTARY_LOG" \
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
        "bundled_core_version": os.environ["PROVENANCE_BUNDLED_CORE_VERSION"],
    },
    "signing": {
        "developer_id_application_identity": os.environ["PROVENANCE_DEVELOPER_ID_APPLICATION"],
        "developer_id_installer_identity": os.environ["PROVENANCE_DEVELOPER_ID_INSTALLER"],
        "app_bundle_codesign": os.environ["PROVENANCE_APP_CODESIGN"],
        "app_executable_codesign": os.environ["PROVENANCE_APP_EXECUTABLE_CODESIGN"],
        "bundled_core_codesign": os.environ["PROVENANCE_CORE_CODESIGN"],
        "installer_pkg_signature": os.environ["PROVENANCE_PKG_SIGNATURE"],
    },
    "notarization": {
        "app_zip_submission_id": os.environ["PROVENANCE_ZIP_SUBMISSION_ID"],
        "installer_pkg_submission_id": os.environ["PROVENANCE_PKG_SUBMISSION_ID"],
        "app_zip_notary_log": os.environ["PROVENANCE_ZIP_NOTARY_LOG"],
        "installer_pkg_notary_log": os.environ["PROVENANCE_PKG_NOTARY_LOG"],
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
  if [[ -n "${JARVIS_DISTRIBUTION_LAUNCH_CHECK_PORT:-}" ]]; then
    printf '%s\n' "$JARVIS_DISTRIBUTION_LAUNCH_CHECK_PORT"
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
    --version-consistency-self-test)
      VERSION_CONSISTENCY_SELF_TEST=true
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

if [[ "$CHECK_ONLY" == true && "$UNSIGNED_STRUCTURE_CHECK" == true ]] ||
  [[ "$CHECK_ONLY" == true && "$UNSIGNED_LAUNCH_CHECK" == true ]] ||
  [[ "$CHECK_ONLY" == true && "$VERSION_CONSISTENCY_SELF_TEST" == true ]] ||
  [[ "$UNSIGNED_STRUCTURE_CHECK" == true && "$UNSIGNED_LAUNCH_CHECK" == true ]] ||
  [[ "$UNSIGNED_STRUCTURE_CHECK" == true && "$VERSION_CONSISTENCY_SELF_TEST" == true ]] ||
  [[ "$UNSIGNED_LAUNCH_CHECK" == true && "$VERSION_CONSISTENCY_SELF_TEST" == true ]]; then
  fail "--check, --unsigned-structure-check, --unsigned-launch-check, and --version-consistency-self-test are mutually exclusive"
fi

if [[ "$VERSION_CONSISTENCY_SELF_TEST" == true ]]; then
  SELF_TEST_OUTPUT=""
  if SELF_TEST_OUTPUT="$(JARVIS_PACKAGE_VERSION_OVERRIDE=9.9.9 "$0" --check 2>&1)"; then
    printf '%s\n' "$SELF_TEST_OUTPUT" >&2
    fail "version consistency self-test expected mismatched package version to fail"
  fi
  require_output_contains "version consistency self-test" "$SELF_TEST_OUTPUT" "package version mismatch"
  printf '\nJarvis package version consistency self-test: ok\n'
  printf 'Proof boundary: mismatch guard only; no app was built, signed, notarized, stapled, installed, launched, or manually validated.\n'
  exit 0
fi

assert_package_version_consistency

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
}

build_app_bundle() {
  rm -rf "$DIST_DIR"
  mkdir -p "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Resources/bin"

  run cargo build --release -p jarvis-cli
  run swift build --disable-sandbox -c release --package-path apps/mac

  SWIFT_BIN_DIR="$(swift build --disable-sandbox -c release --package-path apps/mac --show-bin-path)"
  SWIFT_EXECUTABLE="$SWIFT_BIN_DIR/$APP_EXECUTABLE_NAME"
  CORE_EXECUTABLE="$ROOT_DIR/target/release/jarvis"

  [[ -x "$SWIFT_EXECUTABLE" ]] || fail "Swift release executable missing: $SWIFT_EXECUTABLE"
  [[ -x "$CORE_EXECUTABLE" ]] || fail "Rust release executable missing: $CORE_EXECUTABLE"

  cp "$SWIFT_EXECUTABLE" "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
  cp "$CORE_EXECUTABLE" "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  printf 'jarvis %s\n' "$VERSION" >"$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME.version"
  chmod 755 "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME" "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  assert_bundled_core_version

  cat >"$APP_PATH/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>$APP_EXECUTABLE_NAME</string>
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
  <key>NSMicrophoneUsageDescription</key>
  <string>Jarvis uses microphone input only when you explicitly start local voice capture.</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>Jarvis uses speech recognition only to turn your spoken command into a local assistant request.</string>
</dict>
</plist>
PLIST

  run plutil -lint "$APP_PATH/Contents/Info.plist"

  INFO_PLIST_CONTENTS="$(cat "$APP_PATH/Contents/Info.plist")"
  require_output_contains "Info.plist" "$INFO_PLIST_CONTENTS" "<string>$APP_EXECUTABLE_NAME</string>"
  require_output_contains "Info.plist" "$INFO_PLIST_CONTENTS" "<string>$BUNDLE_ID</string>"
  require_output_contains "Info.plist" "$INFO_PLIST_CONTENTS" "<string>APPL</string>"
  require_output_contains "Info.plist" "$INFO_PLIST_CONTENTS" "NSMicrophoneUsageDescription"
  require_output_contains "Info.plist" "$INFO_PLIST_CONTENTS" "NSSpeechRecognitionUsageDescription"
}

notary_args=()
if [[ -n "${JARVIS_NOTARYTOOL_PROFILE:-}" ]]; then
  notary_args=(--keychain-profile "$JARVIS_NOTARYTOOL_PROFILE")
elif [[ -n "${JARVIS_NOTARYTOOL_APPLE_ID:-}" ]] &&
  [[ -n "${JARVIS_NOTARYTOOL_TEAM_ID:-}" ]] &&
  [[ -n "${JARVIS_NOTARYTOOL_PASSWORD:-}" ]]; then
  notary_args=(
    --apple-id "$JARVIS_NOTARYTOOL_APPLE_ID"
    --team-id "$JARVIS_NOTARYTOOL_TEAM_ID"
    --password "$JARVIS_NOTARYTOOL_PASSWORD"
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
run plutil -lint "$ENTITLEMENTS"

if [[ "$CHECK_ONLY" == true ]]; then
  if [[ -z "${JARVIS_DEVELOPER_ID_APPLICATION:-}" ]]; then
    printf 'warning: JARVIS_DEVELOPER_ID_APPLICATION is not set; full signing will fail until configured.\n' >&2
  fi
  if [[ -z "${JARVIS_DEVELOPER_ID_INSTALLER:-}" ]]; then
    printf 'warning: JARVIS_DEVELOPER_ID_INSTALLER is not set; full installer signing will fail until configured.\n' >&2
  fi
  if [[ ${#notary_args[@]} -eq 0 ]]; then
    printf 'warning: notarization credentials are not set; full notarization will fail until configured.\n' >&2
  fi
  printf '\nJarvis distribution packaging preflight: ok\n'
  printf 'Proof boundary: template/tool check only; no app was signed, notarized, stapled, or manually launched.\n'
  exit 0
fi

run_unsigned_structure_check() {
  build_app_bundle

  SIGNING_STATUS="not attempted"
  if command -v codesign >/dev/null 2>&1; then
    run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
    run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
    run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_PATH"
    run codesign --verify --deep --strict "$APP_PATH"
    assert_app_audio_input_entitlement "unsigned structure app"
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

  PAYLOAD_OUTPUT="$(pkgutil --payload-files "$PKG_PATH")"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Jarvis.app/Contents/MacOS/$APP_EXECUTABLE_NAME"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Jarvis.app/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Jarvis.app/Contents/Info.plist"

  printf '\nJarvis unsigned distribution structure check: ok\n'
  printf 'App: %s\n' "$APP_PATH"
  printf 'Pkg: %s\n' "$PKG_PATH"
  printf 'Signing: %s\n' "$SIGNING_STATUS"
  printf 'Proof boundary: release app and unsigned installer payload structure only; no Developer ID signing, notarization, stapling, /Applications install, Finder launch, live microphone/Speech validation, spoken transcript handoff, live audio-output validation, App Store validation, or manual QA.\n'
}

run_unsigned_launch_check() {
  build_app_bundle

  SIGNING_STATUS="not attempted"
  if command -v codesign >/dev/null 2>&1; then
    run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
    run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
    run codesign --force --sign - --entitlements "$ENTITLEMENTS" "$APP_PATH"
    run codesign --verify --deep --strict "$APP_PATH"
    assert_app_audio_input_entitlement "unsigned launch app"
    SIGNING_STATUS="ad-hoc signed with codesign -"
  fi

  PKG_PATH="$DIST_DIR/$APP_NAME-$VERSION-unsigned-launch.pkg"
  rm -f "$PKG_PATH"
  run pkgbuild \
    --component "$APP_PATH" \
    --install-location /Applications \
    --identifier "$BUNDLE_ID.unsigned-launch.pkg" \
    --version "$VERSION" \
    "$PKG_PATH"

  PAYLOAD_OUTPUT="$(pkgutil --payload-files "$PKG_PATH")"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Jarvis.app/Contents/MacOS/$APP_EXECUTABLE_NAME"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Jarvis.app/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
  require_output_contains "unsigned package payload" "$PAYLOAD_OUTPUT" "Jarvis.app/Contents/Info.plist"

  LAUNCH_TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-distribution-launch.XXXXXX")"
  APP_PID=""
  PORT="$(select_port)"
  ENDPOINT="http://127.0.0.1:$PORT"
  CLEAN_HOME="$LAUNCH_TMP_DIR/home"
  APP_DB="$CLEAN_HOME/Library/Application Support/Jarvis/jarvis.sqlite"
  APP_LOG="$LAUNCH_TMP_DIR/JarvisMacApp.log"
  mkdir -p "$CLEAN_HOME"

  cleanup_launch() {
    if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
      kill "$APP_PID" 2>/dev/null || true
      wait "$APP_PID" 2>/dev/null || true
    fi

    if command -v lsof >/dev/null 2>&1; then
      while IFS= read -r pid; do
        if [[ -n "$pid" ]]; then
          kill "$pid" 2>/dev/null || true
        fi
      done < <(lsof -ti "tcp:$PORT" 2>/dev/null || true)
    fi

    rm -rf "$LAUNCH_TMP_DIR"
  }
  trap cleanup_launch EXIT

  printf '\n==> Launching release app %s with HOME=%s and endpoint %s\n' "$APP_PATH" "$CLEAN_HOME" "$ENDPOINT"
  env \
    HOME="$CLEAN_HOME" \
    JARVIS_MAC_CORE_BIND_ADDRESS="127.0.0.1:$PORT" \
    JARVIS_MAC_CORE_ENDPOINT="$ENDPOINT" \
    JARVIS_MAC_CORE_DATABASE="$APP_DB" \
    "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME" >"$APP_LOG" 2>&1 &
  APP_PID="$!"

  HEALTH_OUTPUT=""
  for _ in {1..60}; do
    if ! kill -0 "$APP_PID" 2>/dev/null; then
      printf 'error: release app exited before core became healthy; app log follows\n' >&2
      cat "$APP_LOG" >&2 || true
      exit 1
    fi

    if HEALTH_OUTPUT="$("$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" health --endpoint "$ENDPOINT" 2>/dev/null)"; then
      require_output_contains "release app health" "$HEALTH_OUTPUT" "jarvis-core: ok"
      require_output_contains "release app health" "$HEALTH_OUTPUT" "runtime: routed-fake-local-model+first-party-plugins"
      break
    fi
    sleep 0.25
  done

  if [[ -z "$HEALTH_OUTPUT" ]]; then
    printf 'error: release app did not supervise a healthy core; app log follows\n' >&2
    cat "$APP_LOG" >&2 || true
    exit 1
  fi

  COMMAND_OUTPUT="$("$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" command --json "plugin echo unsigned distribution launch smoke" --endpoint "$ENDPOINT")"
  require_output_contains "release app command" "$COMMAND_OUTPUT" '"accepted":true'
  require_output_contains "release app command" "$COMMAND_OUTPUT" '"status":"completed"'
  require_output_contains "release app command" "$COMMAND_OUTPUT" '"event_type":"plugin_completed"'

  AUDIT_OUTPUT="$("$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" tasks audit --json --endpoint "$ENDPOINT")"
  require_output_contains "release app audit" "$AUDIT_OUTPUT" '"event_type":"plugin_completed"'
  require_output_contains "release app audit" "$AUDIT_OUTPUT" '"event_type":"task_completed"'

  DIAGNOSTICS_OUTPUT="$("$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" diagnostics export --endpoint "$ENDPOINT")"
  require_output_contains "release app diagnostics" "$DIAGNOSTICS_OUTPUT" '"repository_backed":true'
  require_output_contains "release app diagnostics" "$DIAGNOSTICS_OUTPUT" '"task_count":1'
  require_output_contains "release app diagnostics" "$DIAGNOSTICS_OUTPUT" '"redaction":"diagnostics export omits command bodies'

  PAUSE_OUTPUT="$("$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" pause --endpoint "$ENDPOINT" --reason "unsigned distribution launch smoke")"
  require_output_contains "release app pause" "$PAUSE_OUTPUT" '"paused":true'

  BLOCKED_OUTPUT="$("$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" command --json "plugin echo blocked by unsigned distribution launch smoke" --endpoint "$ENDPOINT" --dry-run)"
  require_output_contains "release app blocked command" "$BLOCKED_OUTPUT" '"accepted":false'
  require_output_contains "release app blocked command" "$BLOCKED_OUTPUT" '"status":"blocked"'

  RESUME_OUTPUT="$("$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME" resume --endpoint "$ENDPOINT")"
  require_output_contains "release app resume" "$RESUME_OUTPUT" '"paused":false'

  if [[ ! -s "$APP_DB" ]]; then
    printf 'error: clean HOME database was not created at %s\n' "$APP_DB" >&2
    exit 1
  fi

  printf '\nJarvis unsigned distribution launch check: ok\n'
  printf 'App: %s\n' "$APP_PATH"
  printf 'Pkg: %s\n' "$PKG_PATH"
  printf 'Signing: %s\n' "$SIGNING_STATUS"
  printf 'Clean HOME database: %s\n' "$APP_DB"
  printf 'Proof boundary: release-built app executable, bundled core, unsigned installer payload structure, isolated HOME launch, command/audit/diagnostics/pause smoke, and optional ad-hoc signing only; no Developer ID signing, notarization, stapling, /Applications install, Finder/LaunchServices validation, live microphone/Speech validation, spoken transcript handoff, live audio-output validation, App Store validation, or manual QA.\n'
}

if [[ "$UNSIGNED_STRUCTURE_CHECK" == true ]]; then
  run_unsigned_structure_check
  exit 0
fi

if [[ "$UNSIGNED_LAUNCH_CHECK" == true ]]; then
  run_unsigned_launch_check
  exit 0
fi

[[ -n "${JARVIS_DEVELOPER_ID_APPLICATION:-}" ]] ||
  fail "JARVIS_DEVELOPER_ID_APPLICATION must name a Developer ID Application identity"
[[ -n "${JARVIS_DEVELOPER_ID_INSTALLER:-}" ]] ||
  fail "JARVIS_DEVELOPER_ID_INSTALLER must name a Developer ID Installer identity"
[[ ${#notary_args[@]} -gt 0 ]] ||
  fail "notarization credentials are required; set JARVIS_NOTARYTOOL_PROFILE or Apple ID/team/password vars"

build_app_bundle

NOTARY_LOG_DIR="$DIST_DIR/notary-logs"
ZIP_NOTARY_LOG="$NOTARY_LOG_DIR/$APP_NAME-$VERSION-app-zip-notarytool.log"
PKG_NOTARY_LOG="$NOTARY_LOG_DIR/$APP_NAME-$VERSION-installer-pkg-notarytool.log"
mkdir -p "$NOTARY_LOG_DIR"

run codesign --force --timestamp --options runtime \
  --entitlements "$ENTITLEMENTS" \
  --sign "$JARVIS_DEVELOPER_ID_APPLICATION" \
  "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
run codesign --force --timestamp --options runtime \
  --entitlements "$ENTITLEMENTS" \
  --sign "$JARVIS_DEVELOPER_ID_APPLICATION" \
  "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
run codesign --force --timestamp --options runtime \
  --entitlements "$ENTITLEMENTS" \
  --sign "$JARVIS_DEVELOPER_ID_APPLICATION" \
  "$APP_PATH"
run codesign --verify --deep --strict --verbose=2 "$APP_PATH"
assert_app_audio_input_entitlement "signed app"

rm -f "$ZIP_PATH"
run ditto -c -k --keepParent "$APP_PATH" "$ZIP_PATH"
capture_command "app zip notarization" "$ZIP_NOTARY_LOG" xcrun notarytool submit "$ZIP_PATH" "${notary_args[@]}" --wait
run xcrun stapler staple "$APP_PATH"
run xcrun stapler validate "$APP_PATH"

rm -f "$PKG_PATH"
run pkgbuild \
  --component "$APP_PATH" \
  --install-location /Applications \
  --identifier "$BUNDLE_ID.pkg" \
  --version "$VERSION" \
  --sign "$JARVIS_DEVELOPER_ID_INSTALLER" \
  "$PKG_PATH"
run pkgutil --check-signature "$PKG_PATH"
capture_command "installer package notarization" "$PKG_NOTARY_LOG" xcrun notarytool submit "$PKG_PATH" "${notary_args[@]}" --wait
run xcrun stapler staple "$PKG_PATH"
run xcrun stapler validate "$PKG_PATH"
write_signed_distribution_provenance

printf '\nJarvis distribution package: ok\n'
printf 'App: %s\n' "$APP_PATH"
printf 'Zip: %s\n' "$ZIP_PATH"
printf 'Pkg: %s\n' "$PKG_PATH"
printf 'Signed provenance: %s\n' "$PROVENANCE_PATH"
printf 'Proof boundary: signed, notarized app zip and signed, notarized installer package only; clean-profile install, Finder launch, live microphone/Speech validation, spoken transcript handoff, live audio-output validation, and App Store validation remain manual release checks.\n'
