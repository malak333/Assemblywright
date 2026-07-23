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
  --run-outage)
    ;;
  *)
    fail "usage: $0 [--check|--run|--run-outage]"
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
bridge_team="$(printf '%s\n' "$bridge_codesign" | sed -n 's/^TeamIdentifier=//p' | head -1)"
[[ "$bridge_team" =~ ^[A-Z0-9]{10}$ ]] \
  || fail "Mac bridge signature has an invalid Apple team identifier"
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

monitor_json="$("$BRIDGE_BIN" monitor --samples 2 --interval-ms 100)"
monitor_first="$(printf '%s\n' "$monitor_json" | sed -n '1p')"
monitor_second="$(printf '%s\n' "$monitor_json" | sed -n '2p')"
monitor_third="$(printf '%s\n' "$monitor_json" | sed -n '3p')"
[[ -n "$monitor_first" && -n "$monitor_second" && -z "$monitor_third" ]] \
  || fail "bridge monitor did not emit exactly two bounded samples"
[[ "$(json_value "$monitor_first" phase)" == "authenticated" ]] \
  || fail "first bridge monitor sample was not authenticated"
[[ "$(json_value "$monitor_second" phase)" == "authenticated" ]] \
  || fail "second bridge monitor sample was not authenticated"
monitor_first_epoch="$(json_value "$monitor_first" connection_epoch)"
monitor_second_epoch="$(json_value "$monitor_second" connection_epoch)"
[[ "$monitor_first_epoch" =~ ^[0-9]+$ && "$monitor_first_epoch" -gt 0 ]] \
  || fail "first bridge monitor epoch was invalid"
[[ "$monitor_first_epoch" == "$monitor_second_epoch" ]] \
  || fail "bridge monitor did not reuse one authenticated connection"
for forbidden in grant_secret certificate_pem ca_certificate_pem maintenance_reason boundary service_identity; do
  [[ "$monitor_json" != *"$forbidden"* ]] \
    || fail "live monitor exposed forbidden field: $forbidden"
done

reconnect_json="$("$BRIDGE_BIN" monitor --samples 2 --interval-ms 100 --reconnect-between-samples)"
reconnect_first="$(printf '%s\n' "$reconnect_json" | sed -n '1p')"
reconnect_second="$(printf '%s\n' "$reconnect_json" | sed -n '2p')"
reconnect_third="$(printf '%s\n' "$reconnect_json" | sed -n '3p')"
[[ -n "$reconnect_first" && -n "$reconnect_second" && -z "$reconnect_third" ]] \
  || fail "bridge reconnect diagnostic did not emit exactly two bounded samples"
[[ "$(json_value "$reconnect_first" phase)" == "authenticated" \
  && "$(json_value "$reconnect_second" phase)" == "authenticated" ]] \
  || fail "bridge reconnect diagnostic did not authenticate both sessions"
reconnect_first_epoch="$(json_value "$reconnect_first" connection_epoch)"
reconnect_second_epoch="$(json_value "$reconnect_second" connection_epoch)"
[[ "$reconnect_first_epoch" =~ ^[0-9]+$ && "$reconnect_second_epoch" =~ ^[0-9]+$ \
  && "$reconnect_second_epoch" -gt "$reconnect_first_epoch" ]] \
  || fail "bridge reconnect diagnostic did not advance the connection epoch"
for forbidden in grant_secret certificate_pem ca_certificate_pem maintenance_reason boundary service_identity; do
  [[ "$reconnect_json" != *"$forbidden"* ]] \
    || fail "live reconnect diagnostic exposed forbidden field: $forbidden"
done

if ! app_lifecycle_output="$(
  JARVIS_MAC_DEVELOPER_BRIDGE_LIVE_E2E=true \
  JARVIS_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
  JARVIS_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
    swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
      --filter liveSignedHelperAppLifecycleReachesWindowsMaster 2>&1
)"; then
  printf '%s\n' "$app_lifecycle_output" >&2
  fail "production app bridge lifecycle did not reach the Windows master"
fi
[[ "$app_lifecycle_output" == *"jarvis_mac_app_bridge_live_e2e_ok"* ]] \
  || fail "production app bridge lifecycle omitted its live E2E marker"

if [[ "$MODE" == "--run-outage" ]]; then
  outage_directory="$(mktemp -d -t jarvis-mac-bridge-outage)"
  chmod 700 "$outage_directory"
  outage_output="$(mktemp -t jarvis-mac-bridge-outage-log)"
  chmod 600 "$outage_output"
  outage_pid=""
  outage_descendants() {
    local parent_pid="$1"
    local child_pid
    for child_pid in $(/usr/bin/pgrep -P "$parent_pid" 2>/dev/null || true); do
      outage_descendants "$child_pid"
      printf '%s\n' "$child_pid"
    done
  }
  cleanup_outage() {
    local child_pid
    if [[ -n "$outage_pid" ]] && kill -0 "$outage_pid" >/dev/null 2>&1; then
      : >"$outage_directory/cancel"
      local graceful_deadline=$((SECONDS + 5))
      while kill -0 "$outage_pid" >/dev/null 2>&1 \
        && (( SECONDS < graceful_deadline )); do
        sleep 0.1
      done
    fi
    if [[ -n "$outage_pid" ]] && kill -0 "$outage_pid" >/dev/null 2>&1; then
      local descendant_pids
      descendant_pids="$(outage_descendants "$outage_pid")"
      for child_pid in $descendant_pids; do
        kill "$child_pid" >/dev/null 2>&1 || true
      done
      kill "$outage_pid" >/dev/null 2>&1 || true
      sleep 0.5
      for child_pid in $descendant_pids; do
        kill -KILL "$child_pid" >/dev/null 2>&1 || true
      done
      kill -KILL "$outage_pid" >/dev/null 2>&1 || true
    fi
    if [[ -n "$outage_pid" ]]; then
      wait "$outage_pid" >/dev/null 2>&1 || true
    fi
    rm -f -- "$outage_output"
    rm -rf -- "$outage_directory"
  }
  trap cleanup_outage EXIT

  JARVIS_MAC_DEVELOPER_BRIDGE_OUTAGE_LIVE_E2E=true \
  JARVIS_MAC_DEVELOPER_BRIDGE_OUTAGE_COORDINATION_DIR="$outage_directory" \
  JARVIS_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
  JARVIS_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
    swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
      --filter liveSignedHelperAppLifecycleRecoversFromWindowsOutage \
      >"$outage_output" 2>&1 &
  outage_pid="$!"

  wait_for_outage_marker() {
    local marker="$1"
    local timeout_seconds="$2"
    local label="$3"
    local deadline=$((SECONDS + timeout_seconds))
    while [[ ! -s "$outage_directory/$marker" ]]; do
      if ! kill -0 "$outage_pid" >/dev/null 2>&1; then
        wait "$outage_pid" >/dev/null 2>&1 || true
        cat "$outage_output" >&2
        fail "production app outage lifecycle exited before $label"
      fi
      (( SECONDS < deadline )) || {
        cat "$outage_output" >&2
        fail "timed out waiting for $label"
      }
      sleep 0.25
    done
  }

  wait_for_outage_marker "connected-before" 120 "the initial authenticated state"
  outage_epoch_before="$(tr -d '\r\n' <"$outage_directory/connected-before")"
  [[ "$outage_epoch_before" =~ ^[0-9]+$ && "$outage_epoch_before" -gt 0 ]] \
    || fail "outage lifecycle emitted an invalid initial epoch"
  printf 'jarvis_mac_windows_outage_stop_required service=JarvisMaster connection_epoch=%s\n' \
    "$outage_epoch_before"

  wait_for_outage_marker "master-offline" 180 "Master Offline after the induced outage"
  outage_error="$(tr -d '\r\n' <"$outage_directory/master-offline")"
  [[ "$outage_error" == "bridge_unavailable" \
    || "$outage_error" == "connection_failed" \
    || "$outage_error" == "invalid_health" ]] \
    || fail "outage lifecycle emitted a non-redacted offline error"
  printf 'jarvis_mac_windows_outage_start_required service=JarvisMaster offline_error=%s\n' \
    "$outage_error"

  wait_for_outage_marker "connected-after" 240 "authenticated recovery"
  outage_epoch_after="$(tr -d '\r\n' <"$outage_directory/connected-after")"
  [[ "$outage_epoch_after" =~ ^[0-9]+$ \
    && "$outage_epoch_after" -gt "$outage_epoch_before" ]] \
    || fail "outage lifecycle did not advance the connection epoch"
  if ! wait "$outage_pid"; then
    cat "$outage_output" >&2
    fail "production app outage lifecycle failed"
  fi
  outage_pid=""
  [[ "$(cat "$outage_output")" == *"jarvis_mac_app_bridge_outage_recovery_live_e2e_ok"* ]] \
    || fail "production app outage lifecycle omitted its live E2E marker"
  printf 'jarvis_mac_windows_outage_recovery_live_e2e_ok endpoint=%s connection_epoch_before=%s connection_epoch_after=%s offline_error=%s app_supervision=verified\n' \
    "$endpoint" "$outage_epoch_before" "$outage_epoch_after" "$outage_error"
fi

printf 'jarvis_mac_windows_bridge_live_e2e_ok endpoint=%s connection_epoch=%s monitor_epoch=%s monitor_samples=2 reconnect_epoch_before=%s reconnect_epoch_after=%s app_supervision=verified team=%s\n' \
  "$endpoint" "$connection_epoch" "$monitor_first_epoch" "$reconnect_first_epoch" "$reconnect_second_epoch" "$bridge_team"
