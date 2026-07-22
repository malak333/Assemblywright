#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---run}"
PACKAGE_PATH="$ROOT_DIR/apps/mac"
PRODUCT="jarvis-mac-bridge"
DEFAULT_SIGNED_APP="$PACKAGE_PATH/.build/jarvis-mac-bridge-signed/Build/Products/Debug/jarvis-mac-bridge.app"
DEFAULT_SIGNED_BIN="$DEFAULT_SIGNED_APP/Contents/MacOS/jarvis-mac-bridge"

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

json_value() {
  local document="$1"
  local key="$2"
  printf '%s' "$document" | /usr/bin/plutil -extract "$key" raw -o - - 2>/dev/null \
    || fail "bridge receipt omitted or invalidated $key"
}

case "$MODE" in
  --check)
    [[ -f "$PACKAGE_PATH/Package.swift" ]] || fail "missing Mac Swift package"
    [[ -f "$PACKAGE_PATH/Sources/JarvisMacBridgeCLI/JarvisMacBridgeCLI.swift" ]] \
      || fail "missing Mac bridge CLI"
    [[ -f "$PACKAGE_PATH/JarvisMacBridge.xcodeproj/project.pbxproj" ]] \
      || fail "missing provisioned Mac bridge Xcode project"
    [[ -f "$ROOT_DIR/packaging/JarvisMacBridge.entitlements" ]] \
      || fail "missing Mac bridge Keychain entitlement"
    bash -n "$ROOT_DIR/scripts/build-mac-bridge-signed.sh"
    swift build --package-path "$PACKAGE_PATH" --product "$PRODUCT"
    printf 'Jarvis Mac-Windows bridge live E2E harness: ready\n'
    exit 0
    ;;
  --run)
    ;;
  *)
    fail "usage: $0 [--check|--run]"
    ;;
esac

BRIDGE_BIN="${JARVIS_MAC_BRIDGE_BIN:-$DEFAULT_SIGNED_BIN}"
[[ -x "$BRIDGE_BIN" ]] || fail \
  "signed Mac bridge is required; run ./scripts/build-mac-bridge-signed.sh or set JARVIS_MAC_BRIDGE_BIN"
codesign --verify --strict "$BRIDGE_BIN" >/dev/null 2>&1 \
  || fail "Mac bridge signature is invalid"
bridge_codesign="$(codesign -dv --verbose=4 "$BRIDGE_BIN" 2>&1)"
bridge_entitlements="$(codesign -d --entitlements :- "$BRIDGE_BIN" 2>/dev/null)"
[[ "$bridge_codesign" == *"Authority=Apple Development: "* \
  || "$bridge_codesign" == *"Authority=Developer ID Application: "* ]] \
  || fail "Mac bridge is not signed with an Apple application identity"
[[ "$bridge_codesign" != *"TeamIdentifier=not set"* ]] \
  || fail "Mac bridge signature has no Apple team identifier"
[[ "$bridge_entitlements" == *"<key>com.apple.application-identifier</key>"* ]] \
  || fail "Mac bridge signature omitted its application identifier entitlement"
[[ "$bridge_entitlements" == *"<key>keychain-access-groups</key>"* ]] \
  || fail "Mac bridge signature omitted its Keychain access group"

TAILSCALE_BIN="${JARVIS_TAILSCALE_BIN:-$(command -v tailscale || true)}"
[[ -n "$TAILSCALE_BIN" && -x "$TAILSCALE_BIN" ]] \
  || fail "Tailscale CLI is required; set JARVIS_TAILSCALE_BIN to its executable"
command -v nc >/dev/null 2>&1 || fail "nc is required for the live TCP preflight"

status_json="$("$BRIDGE_BIN" status)"
[[ "$(json_value "$status_json" status)" == "enrolled" ]] \
  || fail "Mac bridge identity is not installed in Keychain"
endpoint="$(json_value "$status_json" master_endpoint)"

if [[ "$endpoint" == \[*\]:* ]]; then
  host="${endpoint%%]*}"
  host="${host#[}"
  port="${endpoint##*:}"
else
  host="${endpoint%:*}"
  port="${endpoint##*:}"
fi
[[ -n "$host" && "$port" =~ ^[0-9]+$ && "$port" -gt 0 && "$port" -le 65535 ]] \
  || fail "installed bridge endpoint is invalid"

"$TAILSCALE_BIN" ping --c 3 "$host" >/dev/null
nc -z -w 3 "$host" "$port" >/dev/null 2>&1 \
  || fail "Windows master mTLS endpoint is unreachable"

connect_json="$("$BRIDGE_BIN" connect)"
[[ "$(json_value "$connect_json" status)" == "authenticated" ]] \
  || fail "bridge did not complete authenticated application handshake"
[[ "$(json_value "$connect_json" master_mode)" == "developer_remote_master" ]] \
  || fail "authenticated peer did not report the remote master mode"
[[ "$(json_value "$connect_json" master_endpoint)" == "$endpoint" ]] \
  || fail "authenticated receipt endpoint drifted from the Keychain profile"
connection_epoch="$(json_value "$connect_json" connection_epoch)"
[[ "$connection_epoch" =~ ^[0-9]+$ && "$connection_epoch" -gt 0 ]] \
  || fail "master returned an invalid connection epoch"

for forbidden in grant_secret certificate_pem ca_certificate_pem maintenance_reason; do
  [[ "$connect_json" != *"$forbidden"* ]] \
    || fail "live receipt exposed forbidden field: $forbidden"
done

printf 'jarvis_mac_windows_bridge_live_e2e_ok endpoint=%s connection_epoch=%s\n' \
  "$endpoint" "$connection_epoch"
