#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT="$ROOT_DIR/apps/mac/AssemblywrightMacBridge.xcodeproj"
DERIVED_DATA="${ASSEMBLYWRIGHT_MAC_BRIDGE_DERIVED_DATA:-$ROOT_DIR/apps/mac/.build/assemblywright-mac-bridge-signed}"
CONFIGURATION="${ASSEMBLYWRIGHT_MAC_BRIDGE_CONFIGURATION:-Debug}"
BUNDLE_ID="com.nobiletechnology.assemblywright.developer-bridge.cli"

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

resolve_development_team() {
  if [[ -n "${ASSEMBLYWRIGHT_APPLE_DEVELOPMENT_TEAM:-}" ]]; then
    printf '%s\n' "$ASSEMBLYWRIGHT_APPLE_DEVELOPMENT_TEAM"
    return
  fi

  local identity_output identity_count certificate team
  identity_output="$(security find-identity -v -p codesigning 2>/dev/null \
    | sed -n 's/^[[:space:]]*[0-9][0-9]*) [0-9A-F]* "\(Apple Development:.*\)"$/\1/p')"
  identity_count="$(printf '%s\n' "$identity_output" | sed '/^$/d' | wc -l | tr -d ' ')"
  [[ "$identity_count" == "1" ]] \
    || fail "set ASSEMBLYWRIGHT_APPLE_DEVELOPMENT_TEAM when exactly one Apple Development identity is not available"
  certificate="$(printf '%s\n' "$identity_output")"
  team="$(security find-certificate -a -p -c "$certificate" 2>/dev/null \
    | openssl x509 -noout -subject -nameopt RFC2253 2>/dev/null \
    | sed -n 's/.*OU=\([A-Z0-9][A-Z0-9]*\).*/\1/p' | head -n 1)"
  [[ "$team" =~ ^[A-Z0-9]{10}$ ]] \
    || fail "could not derive a 10-character Apple development team identifier"
  printf '%s\n' "$team"
}

[[ "$(uname -s)" == "Darwin" ]] || fail "signed Mac bridge builds require macOS"
command -v xcodebuild >/dev/null 2>&1 || fail "Xcode is required"
command -v security >/dev/null 2>&1 || fail "macOS security tooling is required"
command -v openssl >/dev/null 2>&1 || fail "openssl is required to inspect the signing certificate"
[[ -d "$PROJECT" ]] || fail "missing Mac bridge Xcode project"

team="$(resolve_development_team)"
xcodebuild \
  -project "$PROJECT" \
  -scheme AssemblywrightMacBridge \
  -configuration "$CONFIGURATION" \
  -destination 'platform=macOS' \
  -derivedDataPath "$DERIVED_DATA" \
  -allowProvisioningUpdates \
  DEVELOPMENT_TEAM="$team" \
  CODE_SIGN_STYLE=Automatic \
  build

app="$DERIVED_DATA/Build/Products/$CONFIGURATION/assemblywright-mac-bridge.app"
binary="$app/Contents/MacOS/assemblywright-mac-bridge"
[[ -x "$binary" ]] || fail "signed bridge executable was not produced"
[[ -f "$app/Contents/embedded.provisionprofile" ]] \
  || fail "signed bridge omitted its embedded provisioning profile"
codesign --verify --deep --strict "$app"

entitlements="$(codesign -d --entitlements :- "$app" 2>/dev/null)"
[[ "$entitlements" == *"<key>com.apple.application-identifier</key>"* ]] \
  || fail "signed bridge omitted its application identifier entitlement"
[[ "$entitlements" == *"<key>keychain-access-groups</key>"* ]] \
  || fail "signed bridge omitted its Keychain access group"
[[ "$entitlements" == *"$team.$BUNDLE_ID"* ]] \
  || fail "signed bridge Keychain application identity does not match the selected team"

printf 'assemblywright_mac_bridge_signed_build_ok app=%s binary=%s team=%s\n' \
  "$app" "$binary" "$team"
