#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

VERSION="0.1.4"
BUNDLE_ID="${JARVIS_BUNDLE_ID:-com.nobiletechnology.jarvis}"
APP_NAME="Jarvis"
APP_EXECUTABLE_NAME="JarvisMacApp"
CORE_EXECUTABLE_NAME="jarvis-cli"
ENTITLEMENTS="$ROOT_DIR/packaging/Jarvis.entitlements"
DIST_DIR="${JARVIS_DISTRIBUTION_DIR:-$ROOT_DIR/target/distribution}"
APP_PATH="$DIST_DIR/$APP_NAME.app"
ZIP_PATH="$DIST_DIR/$APP_NAME-$VERSION.zip"
CHECK_ONLY=false

usage() {
  cat <<'USAGE'
Usage: scripts/package-distribution.sh [--check]

Build a distribution-shaped Jarvis.app bundle, sign it with Developer ID, zip it,
submit it for notarization, and staple the ticket.

Required for full distribution packaging:
  JARVIS_DEVELOPER_ID_APPLICATION  Developer ID Application signing identity

Required for notarization, choose one:
  JARVIS_NOTARYTOOL_PROFILE        Stored notarytool keychain profile
  or
  JARVIS_NOTARYTOOL_APPLE_ID
  JARVIS_NOTARYTOOL_TEAM_ID
  JARVIS_NOTARYTOOL_PASSWORD       App-specific password

Optional:
  JARVIS_BUNDLE_ID                 Defaults to com.nobiletechnology.jarvis
  JARVIS_DISTRIBUTION_DIR          Defaults to target/distribution

--check validates local tool/template preconditions without signing or notarizing.
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

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      CHECK_ONLY=true
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

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
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
require_command codesign
require_command xcrun
require_command ditto

[[ -f "$ENTITLEMENTS" ]] || fail "missing entitlements file: $ENTITLEMENTS"
run plutil -lint "$ENTITLEMENTS"

if [[ "$CHECK_ONLY" == true ]]; then
  if [[ -z "${JARVIS_DEVELOPER_ID_APPLICATION:-}" ]]; then
    printf 'warning: JARVIS_DEVELOPER_ID_APPLICATION is not set; full signing will fail until configured.\n' >&2
  fi
  if [[ ${#notary_args[@]} -eq 0 ]]; then
    printf 'warning: notarization credentials are not set; full notarization will fail until configured.\n' >&2
  fi
  printf '\nJarvis distribution packaging preflight: ok\n'
  printf 'Proof boundary: template/tool check only; no app was signed, notarized, stapled, or manually launched.\n'
  exit 0
fi

[[ -n "${JARVIS_DEVELOPER_ID_APPLICATION:-}" ]] ||
  fail "JARVIS_DEVELOPER_ID_APPLICATION must name a Developer ID Application identity"
[[ ${#notary_args[@]} -gt 0 ]] ||
  fail "notarization credentials are required; set JARVIS_NOTARYTOOL_PROFILE or Apple ID/team/password vars"

rm -rf "$DIST_DIR"
mkdir -p "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Resources/bin"

run cargo build --release -p jarvis-cli
run swift build -c release --package-path apps/mac

SWIFT_BIN_DIR="$(swift build -c release --package-path apps/mac --show-bin-path)"
SWIFT_EXECUTABLE="$SWIFT_BIN_DIR/$APP_EXECUTABLE_NAME"
CORE_EXECUTABLE="$ROOT_DIR/target/release/jarvis"

[[ -x "$SWIFT_EXECUTABLE" ]] || fail "Swift release executable missing: $SWIFT_EXECUTABLE"
[[ -x "$CORE_EXECUTABLE" ]] || fail "Rust release executable missing: $CORE_EXECUTABLE"

cp "$SWIFT_EXECUTABLE" "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME"
cp "$CORE_EXECUTABLE" "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"
chmod 755 "$APP_PATH/Contents/MacOS/$APP_EXECUTABLE_NAME" "$APP_PATH/Contents/Resources/bin/$CORE_EXECUTABLE_NAME"

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

rm -f "$ZIP_PATH"
run ditto -c -k --keepParent "$APP_PATH" "$ZIP_PATH"
run xcrun notarytool submit "$ZIP_PATH" "${notary_args[@]}" --wait
run xcrun stapler staple "$APP_PATH"
run xcrun stapler validate "$APP_PATH"

printf '\nJarvis distribution package: ok\n'
printf 'App: %s\n' "$APP_PATH"
printf 'Zip: %s\n' "$ZIP_PATH"
printf 'Proof boundary: signed and notarized package only; clean-profile Finder launch, live microphone/Speech validation, and installer/App Store validation remain manual release checks.\n'
