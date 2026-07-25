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
    [[ -f "$PACKAGE_PATH/Sources/JarvisMacCore/DeveloperEventRelay.swift" ]] \
      || fail "missing Mac event relay"
    [[ -f "$PACKAGE_PATH/JarvisMacBridge.xcodeproj/project.pbxproj" ]] \
      || fail "missing provisioned Mac bridge Xcode project"
    [[ -f "$ROOT_DIR/crates/jarvis-agent/src/main.rs" ]] \
      || fail "missing supervised Rust agent"
    [[ -f "$ROOT_DIR/packaging/JarvisMacBridge.entitlements" ]] \
      || fail "missing Mac bridge Keychain entitlement"
    bash -n "$ROOT_DIR/scripts/build-mac-bridge-signed.sh"
    swift build --package-path "$PACKAGE_PATH" --product "$PRODUCT"
    cargo build --manifest-path "$ROOT_DIR/Cargo.toml" -p jarvis-agent --locked
    printf 'Assemblywright Mac-Windows bridge live E2E harness: ready\n'
    exit 0
    ;;
  --run)
    ;;
  --run-relay)
    ;;
  --run-fixture)
    ;;
  --run-mlx)
    ;;
  --run-outage)
    ;;
  *)
    fail "usage: $0 [--check|--run|--run-relay|--run-fixture|--run-mlx|--run-outage]"
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

identity_profile_arguments=()
standard_status_json="$("$BRIDGE_BIN" status)"
[[ "$(json_value "$standard_status_json" status)" == "enrolled" ]] \
  || fail "standard Mac bridge identity is not installed in Keychain"
standard_device_id="$(json_value "$standard_status_json" device_id)"
standard_device_name="$(json_value "$standard_status_json" device_name)"
standard_master_endpoint="$(json_value "$standard_status_json" master_endpoint)"
standard_registry_revision="$(json_value "$standard_status_json" registry_revision)"
standard_certificate_not_after_ms="$(
  json_value "$standard_status_json" certificate_not_after_ms
)"
if [[ "$MODE" == "--run-fixture" ]]; then
  identity_profile_arguments=(--identity-profile fixture)
  status_json="$("$BRIDGE_BIN" status "${identity_profile_arguments[@]}")"
else
  status_json="$standard_status_json"
fi
[[ "$(json_value "$status_json" status)" == "enrolled" ]] \
  || fail "Mac bridge identity is not installed in Keychain"
device_id="$(json_value "$status_json" device_id)"
if [[ "$MODE" == "--run-fixture" ]]; then
  [[ "$device_id" != "$standard_device_id" ]] \
    || fail "fixture identity must be separately enrolled from the standard profile"
fi
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

connect_json="$(
  "$BRIDGE_BIN" connect \
    ${identity_profile_arguments[@]+"${identity_profile_arguments[@]}"}
)"
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

monitor_json="$(
  "$BRIDGE_BIN" monitor \
    ${identity_profile_arguments[@]+"${identity_profile_arguments[@]}"} \
    --samples 2 --interval-ms 100
)"
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

reconnect_json="$(
  "$BRIDGE_BIN" monitor \
    ${identity_profile_arguments[@]+"${identity_profile_arguments[@]}"} \
    --samples 2 --interval-ms 100 --reconnect-between-samples
)"
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

relay_directory=""
relay_data_directory=""
relay_agent_bin=""
app_lifecycle_environment=()
cleanup_relay() {
  if [[ -n "$relay_directory" ]]; then
    rm -rf -- "$relay_directory"
  fi
}
if [[ "$MODE" == "--run-relay" || "$MODE" == "--run-fixture" || "$MODE" == "--run-mlx" ]]; then
  command -v sqlite3 >/dev/null 2>&1 \
    || fail "sqlite3 is required for the durable relay proof"
  if [[ -n "${JARVIS_MAC_AGENT_BIN:-}" ]]; then
    relay_agent_bin="$JARVIS_MAC_AGENT_BIN"
  else
    cargo build --manifest-path "$ROOT_DIR/Cargo.toml" -p jarvis-agent --locked
    relay_agent_bin="$ROOT_DIR/target/debug/jarvis-agent"
  fi
  [[ -x "$relay_agent_bin" ]] \
    || fail "jarvis-agent executable is unavailable"
  codesign --verify --strict "$relay_agent_bin" >/dev/null 2>&1 \
    || fail "jarvis-agent signature is invalid"
  relay_directory="$(mktemp -d -t jarvis-mac-agent-relay)"
  chmod 700 "$relay_directory"
  relay_data_directory="$relay_directory/data"
  app_lifecycle_environment=(
    "JARVIS_MAC_DEVELOPER_AGENT_EXECUTABLE=$relay_agent_bin"
    "JARVIS_MAC_DEVELOPER_AGENT_DATA_DIR=$relay_data_directory"
  )
  if [[ "$MODE" == "--run-fixture" ]]; then
    app_lifecycle_environment+=(
      "JARVIS_MAC_DEVELOPER_FIXTURE_JOBS_ENABLED=true"
    )
  elif [[ "$MODE" == "--run-mlx" ]]; then
    [[ -n "${JARVIS_MAC_DEVELOPER_MLX_EXECUTABLE:-}" \
      && -x "$JARVIS_MAC_DEVELOPER_MLX_EXECUTABLE" ]] \
      || fail "set JARVIS_MAC_DEVELOPER_MLX_EXECUTABLE to the exact executable"
    [[ -n "${JARVIS_MAC_DEVELOPER_MLX_MODEL_DIR:-}" \
      && -d "$JARVIS_MAC_DEVELOPER_MLX_MODEL_DIR" ]] \
      || fail "set JARVIS_MAC_DEVELOPER_MLX_MODEL_DIR to the offline model directory"
    [[ -n "${JARVIS_MAC_DEVELOPER_MLX_MODEL_ID:-}" ]] \
      || fail "set JARVIS_MAC_DEVELOPER_MLX_MODEL_ID to the enrolled model identifier"
    app_lifecycle_environment+=(
      "JARVIS_MAC_DEVELOPER_MLX_JOBS_ENABLED=true"
      "JARVIS_MAC_DEVELOPER_MLX_EXECUTABLE=$JARVIS_MAC_DEVELOPER_MLX_EXECUTABLE"
      "JARVIS_MAC_DEVELOPER_MLX_MODEL_DIR=$JARVIS_MAC_DEVELOPER_MLX_MODEL_DIR"
      "JARVIS_MAC_DEVELOPER_MLX_MODEL_ID=$JARVIS_MAC_DEVELOPER_MLX_MODEL_ID"
    )
  fi
  trap cleanup_relay EXIT
fi

if [[ "$MODE" != "--run-fixture" && "$MODE" != "--run-mlx" ]] \
  && ! app_lifecycle_output="$(
  env \
    JARVIS_MAC_DEVELOPER_BRIDGE_LIVE_E2E=true \
    JARVIS_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
    JARVIS_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
    "${app_lifecycle_environment[@]}" \
    swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
      --filter liveSignedHelperAppLifecycleReachesWindowsMaster 2>&1
)"; then
  printf '%s\n' "$app_lifecycle_output" >&2
  fail "production app bridge lifecycle did not reach the Windows master"
fi
if [[ "$MODE" != "--run-fixture" && "$MODE" != "--run-mlx" ]]; then
  [[ "$app_lifecycle_output" == *"jarvis_mac_app_bridge_live_e2e_ok"* ]] \
    || fail "production app bridge lifecycle omitted its live E2E marker"
fi

if [[ "$MODE" == "--run-relay" ]]; then
  relay_database="$relay_data_directory/agent.sqlite3"
  [[ -f "$relay_database" ]] \
    || fail "production app relay did not create the durable agent cursor database"
  first_cursor="$(
    sqlite3 "$relay_database" \
      'SELECT COALESCE(stream_id, ""), sequence FROM agent_event_cursor WHERE singleton = 1;'
  )"
  first_stream="${first_cursor%%|*}"
  first_sequence="${first_cursor##*|}"
  [[ "$first_stream" =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ \
    && "$first_sequence" =~ ^[0-9]+$ \
    && "$first_sequence" -gt 0 ]] \
    || fail "production app relay did not persist a concrete event cursor"

  if ! relay_resume_output="$(
    env \
      JARVIS_MAC_DEVELOPER_BRIDGE_LIVE_E2E=true \
      JARVIS_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
      JARVIS_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
      "${app_lifecycle_environment[@]}" \
      swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
        --filter liveSignedHelperAppLifecycleReachesWindowsMaster 2>&1
  )"; then
    printf '%s\n' "$relay_resume_output" >&2
    fail "production app relay did not resume through a fresh helper and agent"
  fi
  [[ "$relay_resume_output" == *"jarvis_mac_app_bridge_live_e2e_ok"* ]] \
    || fail "production app relay resume omitted its live E2E marker"

  resumed_cursor="$(
    sqlite3 "$relay_database" \
      'SELECT COALESCE(stream_id, ""), sequence FROM agent_event_cursor WHERE singleton = 1;'
  )"
  resumed_stream="${resumed_cursor%%|*}"
  resumed_sequence="${resumed_cursor##*|}"
  [[ "$resumed_stream" == "$first_stream" \
    && "$resumed_sequence" =~ ^[0-9]+$ \
    && "$resumed_sequence" -gt "$first_sequence" ]] \
    || fail "production app relay did not durably resume the same advancing stream"
  printf 'jarvis_mac_windows_event_relay_live_e2e_ok endpoint=%s stream_id=%s sequence_before=%s sequence_after=%s app_supervision=verified agent_restart=verified\n' \
    "$endpoint" "$first_stream" "$first_sequence" "$resumed_sequence"
fi

if [[ "$MODE" == "--run-mlx" ]]; then
  mlx_coordination_directory="$relay_directory/mlx-coordination"
  mkdir "$mlx_coordination_directory"
  chmod 700 "$mlx_coordination_directory"
  mlx_output="$relay_directory/mlx-live.log"
  : >"$mlx_output"
  chmod 600 "$mlx_output"
  mlx_pid=""

  cleanup_mlx() {
    if [[ -n "$mlx_pid" ]] && kill -0 "$mlx_pid" >/dev/null 2>&1; then
      : >"$mlx_coordination_directory/cancel"
      kill "$mlx_pid" >/dev/null 2>&1 || true
      local deadline=$((SECONDS + 5))
      while kill -0 "$mlx_pid" >/dev/null 2>&1 \
        && (( SECONDS < deadline )); do
        sleep 0.1
      done
      if kill -0 "$mlx_pid" >/dev/null 2>&1; then
        kill -KILL "$mlx_pid" >/dev/null 2>&1 || true
      fi
      wait "$mlx_pid" >/dev/null 2>&1 || true
    fi
    cleanup_relay
  }
  trap cleanup_mlx EXIT

  mlx_cursor() {
    local database="$relay_data_directory/agent.sqlite3"
    [[ -f "$database" ]] || return 1
    sqlite3 "$database" \
      'SELECT COALESCE(stream_id, ""), sequence FROM agent_event_cursor WHERE singleton = 1;'
  }
  wait_for_mlx_marker() {
    local marker="$1"
    local timeout_seconds="$2"
    local label="$3"
    local deadline=$((SECONDS + timeout_seconds))
    while [[ ! -s "$mlx_coordination_directory/$marker" ]]; do
      if ! kill -0 "$mlx_pid" >/dev/null 2>&1; then
        wait "$mlx_pid" >/dev/null 2>&1 || true
        cat "$mlx_output" >&2
        fail "production app MLX lifecycle exited before $label"
      fi
      (( SECONDS < deadline )) || {
        cat "$mlx_output" >&2
        fail "timed out waiting for $label"
      }
      sleep 0.25
    done
  }
  wait_for_mlx_sequence() {
    local minimum="$1"
    local timeout_seconds="$2"
    local label="$3"
    local deadline=$((SECONDS + timeout_seconds))
    local observed=""
    while true; do
      observed="$(mlx_cursor 2>/dev/null || true)"
      local sequence="${observed##*|}"
      if [[ "$sequence" =~ ^[0-9]+$ && "$sequence" -ge "$minimum" ]]; then
        printf '%s' "$observed"
        return
      fi
      if ! kill -0 "$mlx_pid" >/dev/null 2>&1; then
        wait "$mlx_pid" >/dev/null 2>&1 || true
        cat "$mlx_output" >&2
        fail "production app MLX lifecycle exited before $label"
      fi
      (( SECONDS < deadline )) || {
        cat "$mlx_output" >&2
        fail "timed out waiting for $label"
      }
      sleep 0.25
    done
  }
  capture_mlx_control_receipt() {
    local filename="$1"
    local label="$2"
    local receipt=""
    if ! IFS= read -r -t 300 receipt; then
      fail "timed out waiting for the sanitized $label receipt on stdin"
    fi
    [[ -n "$receipt" && "${#receipt}" -le 4096 ]] \
      || fail "the sanitized $label receipt was empty or oversized"
    (
      umask 077
      printf '%s' "$receipt" >"$mlx_coordination_directory/$filename.tmp"
    )
    chmod 600 "$mlx_coordination_directory/$filename.tmp"
    mv "$mlx_coordination_directory/$filename.tmp" \
      "$mlx_coordination_directory/$filename"
  }

  env \
    JARVIS_MAC_DEVELOPER_MLX_LIVE_E2E=true \
    JARVIS_MAC_DEVELOPER_MLX_COORDINATION_DIR="$mlx_coordination_directory" \
    JARVIS_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
    JARVIS_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
    "${app_lifecycle_environment[@]}" \
    swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
      --filter liveSignedHelperAppLifecycleRunsMLXJob \
      >"$mlx_output" 2>&1 </dev/null &
  mlx_pid="$!"

  wait_for_mlx_marker "mlx-ready" 180 "the exact MLX-capability connection"
  mlx_ready_epoch="$(
    tr -d '\r\n' <"$mlx_coordination_directory/mlx-ready"
  )"
  [[ "$mlx_ready_epoch" =~ ^[0-9]+$ && "$mlx_ready_epoch" -gt 0 ]] \
    || fail "MLX lifecycle emitted an invalid connection epoch"
  initial_cursor="$(mlx_cursor)" \
    || fail "MLX lifecycle did not create the durable agent cursor"
  mlx_stream="${initial_cursor%%|*}"
  mlx_sequence_before="${initial_cursor##*|}"
  [[ "$mlx_stream" =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ \
    && "$mlx_sequence_before" =~ ^[0-9]+$ ]] \
    || fail "MLX lifecycle emitted an invalid initial cursor"

  printf '%s\n' \
    "jarvis_mac_windows_mlx_success_enqueue_required action=EnqueueSuccess script=scripts/windows-mlx-live-control.ps1 expected_device_id=$standard_device_id receipt_stdin=required"
  capture_mlx_control_receipt "success-control.json" "MLX success"
  wait_for_mlx_marker "success-observed" 30 "strict MLX success receipt validation"
  success_receipt="$(<"$mlx_coordination_directory/success-control.json")"
  success_receipt_stream="$(json_value "$success_receipt" stream_id)"
  success_receipt_device="$(json_value "$success_receipt" device_id)"
  success_receipt_epoch="$(json_value "$success_receipt" connection_epoch)"
  success_receipt_sequence="$(json_value "$success_receipt" succeeded_sequence)"
  [[ "$success_receipt_stream" == "$mlx_stream" \
    && "$success_receipt_device" == "$standard_device_id" \
    && "$success_receipt_epoch" =~ ^[0-9]+$ \
    && "$success_receipt_epoch" -ge "$mlx_ready_epoch" \
    && "$success_receipt_sequence" =~ ^[0-9]+$ \
    && "$success_receipt_sequence" -gt "$mlx_sequence_before" ]] \
    || fail "strict success receipt did not bind the active MLX stream"
  success_cursor="$(
    wait_for_mlx_sequence "$success_receipt_sequence" 180 \
      "the exact MLX success terminal event"
  )"
  mlx_sequence_success="${success_cursor##*|}"

  printf '%s\n' \
    "jarvis_mac_windows_mlx_cancellation_enqueue_required action=EnqueueCancellationAndPause script=scripts/windows-mlx-live-control.ps1 expected_device_id=$standard_device_id receipts_stdin=leased_then_cancelled"
  capture_mlx_control_receipt "cancellation-control.json" "MLX cancellation lease"
  wait_for_mlx_marker \
    "cancellation-leased-observed" 30 "strict MLX cancellation lease receipt validation"
  cancellation_receipt="$(<"$mlx_coordination_directory/cancellation-control.json")"
  cancellation_receipt_stream="$(json_value "$cancellation_receipt" stream_id)"
  cancellation_receipt_device="$(json_value "$cancellation_receipt" device_id)"
  cancellation_receipt_epoch="$(json_value "$cancellation_receipt" connection_epoch)"
  cancellation_receipt_leased_sequence="$(
    json_value "$cancellation_receipt" leased_sequence
  )"
  [[ "$cancellation_receipt_stream" == "$mlx_stream" \
    && "$cancellation_receipt_device" == "$standard_device_id" \
    && "$cancellation_receipt_epoch" =~ ^[0-9]+$ \
    && "$cancellation_receipt_epoch" -ge "$success_receipt_epoch" \
    && "$cancellation_receipt_leased_sequence" =~ ^[0-9]+$ \
    && "$cancellation_receipt_leased_sequence" -gt "$mlx_sequence_success" ]] \
    || fail "strict cancellation receipt did not bind the active MLX stream"
  printf '%s\n' \
    "jarvis_mac_windows_mlx_pause_receipt_required action=EnqueueCancellationAndPause expected_device_id=$standard_device_id connection_epoch=$cancellation_receipt_epoch receipt_stdin=second_receipt"
  capture_mlx_control_receipt "pause-control.json" "MLX cancellation"
  wait_for_mlx_marker "cancellation-observed" 300 "the fail-closed paused MLX state"
  pause_receipt="$(<"$mlx_coordination_directory/pause-control.json")"
  pause_receipt_stream="$(json_value "$pause_receipt" stream_id)"
  pause_receipt_device="$(json_value "$pause_receipt" device_id)"
  pause_receipt_epoch="$(json_value "$pause_receipt" connection_epoch)"
  pause_receipt_cancelled_sequence="$(json_value "$pause_receipt" cancelled_sequence)"
  [[ "$pause_receipt_stream" == "$mlx_stream" \
    && "$pause_receipt_device" == "$standard_device_id" \
    && "$pause_receipt_epoch" == "$cancellation_receipt_epoch" \
    && "$pause_receipt_cancelled_sequence" =~ ^[0-9]+$ \
    && "$pause_receipt_cancelled_sequence" -gt "$cancellation_receipt_leased_sequence" ]] \
    || fail "strict cancellation receipt did not bind the active MLX stream"
  cancellation_cursor="$(
    wait_for_mlx_sequence "$pause_receipt_cancelled_sequence" 180 \
      "the exact MLX cancellation and late-output suppression"
  )"
  mlx_sequence_cancelled="${cancellation_cursor##*|}"

  printf '%s\n' \
    'jarvis_mac_windows_mlx_resume_required action=Resume script=scripts/windows-mlx-live-control.ps1 receipt_stdin=required'
  capture_mlx_control_receipt "resume-control.json" "MLX resume"
  wait_for_mlx_marker "mlx-complete" 300 "deliberate MLX admission resume"
  if ! wait "$mlx_pid"; then
    cat "$mlx_output" >&2
    fail "production app MLX lifecycle failed"
  fi
  mlx_pid=""
  [[ "$(cat "$mlx_output")" == *"jarvis_mac_app_mlx_live_e2e_ok"* ]] \
    || fail "production app MLX lifecycle omitted its live E2E marker"

  if ! mlx_restart_output="$(
    env \
      JARVIS_MAC_DEVELOPER_BRIDGE_LIVE_E2E=true \
      JARVIS_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
      JARVIS_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
      "${app_lifecycle_environment[@]}" \
      swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
        --filter liveSignedHelperAppLifecycleReachesWindowsMaster 2>&1
  )"; then
    printf '%s\n' "$mlx_restart_output" >&2
    fail "MLX cursor did not survive a fresh app/helper/agent chain"
  fi
  [[ "$mlx_restart_output" == *"jarvis_mac_app_bridge_live_e2e_ok"* ]] \
    || fail "MLX restart omitted its live E2E marker"
  restarted_cursor="$(mlx_cursor)" \
    || fail "MLX restart lost the durable agent cursor"
  restarted_stream="${restarted_cursor%%|*}"
  mlx_sequence_restarted="${restarted_cursor##*|}"
  [[ "$restarted_stream" == "$mlx_stream" \
    && "$mlx_sequence_restarted" =~ ^[0-9]+$ \
    && "$mlx_sequence_restarted" -gt "$mlx_sequence_cancelled" ]] \
    || fail "MLX restart did not resume the same advancing cursor"

  standard_status_after="$("$BRIDGE_BIN" status)"
  [[ "$(json_value "$standard_status_after" status)" == "enrolled" \
    && "$(json_value "$standard_status_after" device_id)" == "$standard_device_id" \
    && "$(json_value "$standard_status_after" device_name)" == "$standard_device_name" \
    && "$(json_value "$standard_status_after" master_endpoint)" == "$standard_master_endpoint" \
    && "$(json_value "$standard_status_after" registry_revision)" == "$standard_registry_revision" \
    && "$(json_value "$standard_status_after" certificate_not_after_ms)" \
      == "$standard_certificate_not_after_ms" ]] \
    || fail "MLX run changed the stable standard Mac bridge profile projection"
  printf 'jarvis_mac_windows_mlx_live_e2e_ok endpoint=%s sequence_before=%s sequence_success=%s sequence_cancelled=%s sequence_restarted=%s local_inference=verified exact_event_binding=verified cancellation=verified late_output_suppression=verified agent_restart=verified\n' \
    "$endpoint" "$mlx_sequence_before" "$mlx_sequence_success" \
    "$mlx_sequence_cancelled" "$mlx_sequence_restarted"
fi

if [[ "$MODE" == "--run-fixture" ]]; then
  fixture_coordination_directory="$relay_directory/fixture-coordination"
  mkdir "$fixture_coordination_directory"
  chmod 700 "$fixture_coordination_directory"
  fixture_output="$relay_directory/fixture-live.log"
  : >"$fixture_output"
  chmod 600 "$fixture_output"
  fixture_pid=""

  cleanup_fixture() {
    if [[ -n "$fixture_pid" ]] && kill -0 "$fixture_pid" >/dev/null 2>&1; then
      : >"$fixture_coordination_directory/cancel"
      kill "$fixture_pid" >/dev/null 2>&1 || true
      local deadline=$((SECONDS + 5))
      while kill -0 "$fixture_pid" >/dev/null 2>&1 \
        && (( SECONDS < deadline )); do
        sleep 0.1
      done
      if kill -0 "$fixture_pid" >/dev/null 2>&1; then
        kill -KILL "$fixture_pid" >/dev/null 2>&1 || true
      fi
      wait "$fixture_pid" >/dev/null 2>&1 || true
    fi
    cleanup_relay
  }
  trap cleanup_fixture EXIT

  fixture_cursor() {
    local database="$relay_data_directory/agent.sqlite3"
    [[ -f "$database" ]] || return 1
    sqlite3 "$database" \
      'SELECT COALESCE(stream_id, ""), sequence FROM agent_event_cursor WHERE singleton = 1;'
  }
  wait_for_fixture_marker() {
    local marker="$1"
    local timeout_seconds="$2"
    local label="$3"
    local deadline=$((SECONDS + timeout_seconds))
    while [[ ! -s "$fixture_coordination_directory/$marker" ]]; do
      if ! kill -0 "$fixture_pid" >/dev/null 2>&1; then
        wait "$fixture_pid" >/dev/null 2>&1 || true
        fail "production app fixture lifecycle exited before $label"
      fi
      (( SECONDS < deadline )) || {
        fail "timed out waiting for $label"
      }
      sleep 0.25
    done
  }
  wait_for_fixture_sequence() {
    local minimum="$1"
    local timeout_seconds="$2"
    local label="$3"
    local deadline=$((SECONDS + timeout_seconds))
    local observed=""
    while true; do
      observed="$(fixture_cursor 2>/dev/null || true)"
      local sequence="${observed##*|}"
      if [[ "$sequence" =~ ^[0-9]+$ && "$sequence" -ge "$minimum" ]]; then
        printf '%s' "$observed"
        return
      fi
      if ! kill -0 "$fixture_pid" >/dev/null 2>&1; then
        wait "$fixture_pid" >/dev/null 2>&1 || true
        fail "production app fixture lifecycle exited before $label"
      fi
      (( SECONDS < deadline )) || {
        fail "timed out waiting for $label"
      }
      sleep 0.25
    done
  }
  capture_fixture_control_receipt() {
    local filename="$1"
    local label="$2"
    local receipt=""
    if ! IFS= read -r -t 300 receipt; then
      fail "timed out waiting for the sanitized $label receipt on stdin"
    fi
    [[ -n "$receipt" && "${#receipt}" -le 4096 ]] \
      || fail "the sanitized $label receipt was empty or oversized"
    (
      umask 077
      printf '%s' "$receipt" >"$fixture_coordination_directory/$filename.tmp"
    )
    chmod 600 "$fixture_coordination_directory/$filename.tmp"
    mv "$fixture_coordination_directory/$filename.tmp" \
      "$fixture_coordination_directory/$filename"
  }

  env \
    JARVIS_MAC_DEVELOPER_FIXTURE_LIVE_E2E=true \
    JARVIS_MAC_DEVELOPER_FIXTURE_COORDINATION_DIR="$fixture_coordination_directory" \
    JARVIS_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
    JARVIS_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
    "${app_lifecycle_environment[@]}" \
    swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
      --filter liveSignedHelperAppLifecycleRunsFixtureJob \
      >"$fixture_output" 2>&1 </dev/null &
  fixture_pid="$!"

  wait_for_fixture_marker "fixture-ready" 120 "the exact fixture-profile connection"
  fixture_ready_epoch="$(
    tr -d '\r\n' <"$fixture_coordination_directory/fixture-ready"
  )"
  [[ "$fixture_ready_epoch" =~ ^[0-9]+$ && "$fixture_ready_epoch" -gt 0 ]] \
    || fail "fixture lifecycle emitted an invalid connection epoch"
  initial_cursor="$(fixture_cursor)" \
    || fail "fixture lifecycle did not create the durable agent cursor"
  fixture_stream="${initial_cursor%%|*}"
  fixture_sequence_before="${initial_cursor##*|}"
  [[ "$fixture_stream" =~ ^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ \
    && "$fixture_sequence_before" =~ ^[0-9]+$ ]] \
    || fail "fixture lifecycle emitted an invalid initial cursor"

  printf '%s\n' \
    'jarvis_mac_windows_fixture_success_enqueue_required action=EnqueueSuccess script=scripts/windows-fixture-live-control.ps1 receipt_stdin=required'
  capture_fixture_control_receipt "success-control.json" "fixture success"
  wait_for_fixture_marker "success-observed" 30 "strict success receipt validation"
  success_receipt="$(<"$fixture_coordination_directory/success-control.json")"
  success_receipt_stream="$(json_value "$success_receipt" stream_id)"
  success_receipt_sequence="$(json_value "$success_receipt" succeeded_sequence)"
  [[ "$success_receipt_stream" == "$fixture_stream" \
    && "$success_receipt_sequence" =~ ^[0-9]+$ \
    && "$success_receipt_sequence" -gt "$fixture_sequence_before" ]] \
    || fail "strict success receipt did not bind the active fixture stream"
  success_cursor="$(
    wait_for_fixture_sequence "$success_receipt_sequence" 120 \
      "the exact synthetic success terminal event"
  )"
  success_stream="${success_cursor%%|*}"
  fixture_sequence_success="${success_cursor##*|}"
  [[ "$success_stream" == "$fixture_stream" ]] \
    || fail "fixture success replaced the durable event stream"

  printf '%s\n' \
    'jarvis_mac_windows_fixture_cancellation_enqueue_required action=EnqueueCancellation script=scripts/windows-fixture-live-control.ps1 receipt_stdin=required'
  capture_fixture_control_receipt "cancellation-control.json" "fixture cancellation lease"
  wait_for_fixture_marker \
    "cancellation-leased-observed" 30 "strict cancellation lease receipt validation"
  cancellation_receipt="$(<"$fixture_coordination_directory/cancellation-control.json")"
  cancellation_receipt_stream="$(json_value "$cancellation_receipt" stream_id)"
  cancellation_receipt_leased_sequence="$(
    json_value "$cancellation_receipt" leased_sequence
  )"
  [[ "$cancellation_receipt_stream" == "$fixture_stream" \
    && "$cancellation_receipt_leased_sequence" =~ ^[0-9]+$ \
    && "$cancellation_receipt_leased_sequence" -gt "$fixture_sequence_success" ]] \
    || fail "strict cancellation receipt did not bind the active fixture stream"
  leased_cursor="$(
    wait_for_fixture_sequence "$cancellation_receipt_leased_sequence" 120 \
      "the exact delayed cancellation fixture lease"
  )"
  leased_stream="${leased_cursor%%|*}"
  [[ "$leased_stream" == "$fixture_stream" ]] \
    || fail "fixture cancellation lease replaced the durable event stream"
  fixture_sequence_leased="$cancellation_receipt_leased_sequence"
  printf '%s\n' \
    'jarvis_mac_windows_fixture_pause_required action=Pause script=scripts/windows-fixture-live-control.ps1 args=prior_cancellation_receipt receipt_stdin=required'
  capture_fixture_control_receipt "pause-control.json" "fixture cancellation"
  wait_for_fixture_marker "cancellation-observed" 240 "the fail-closed paused state"
  pause_receipt="$(<"$fixture_coordination_directory/pause-control.json")"
  pause_receipt_stream="$(json_value "$pause_receipt" stream_id)"
  pause_receipt_cancelled_sequence="$(json_value "$pause_receipt" cancelled_sequence)"
  [[ "$pause_receipt_stream" == "$fixture_stream" \
    && "$pause_receipt_cancelled_sequence" =~ ^[0-9]+$ \
    && "$pause_receipt_cancelled_sequence" -gt "$fixture_sequence_leased" ]] \
    || fail "strict cancellation receipt did not bind the active fixture stream"
  cancellation_cursor="$(
    wait_for_fixture_sequence "$pause_receipt_cancelled_sequence" 120 \
      "the exact cancellation acknowledgement and late-output suppression"
  )"
  cancellation_stream="${cancellation_cursor%%|*}"
  fixture_sequence_cancelled="${cancellation_cursor##*|}"
  [[ "$cancellation_stream" == "$fixture_stream" ]] \
    || fail "fixture cancellation replaced the durable event stream"

  printf '%s\n' \
    'jarvis_mac_windows_fixture_resume_required action=Resume script=scripts/windows-fixture-live-control.ps1 receipt_stdin=required'
  capture_fixture_control_receipt "resume-control.json" "fixture resume"
  wait_for_fixture_marker "fixture-complete" 240 "deliberate fixture admission resume"
  if ! wait "$fixture_pid"; then
    fail "production app fixture lifecycle failed"
  fi
  fixture_pid=""
  [[ "$(cat "$fixture_output")" == *"jarvis_mac_app_fixture_live_e2e_ok"* ]] \
    || fail "production app fixture lifecycle omitted its live E2E marker"

  if ! fixture_restart_output="$(
    env \
      JARVIS_MAC_DEVELOPER_BRIDGE_LIVE_E2E=true \
      JARVIS_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
      JARVIS_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
      "${app_lifecycle_environment[@]}" \
      swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
        --filter liveSignedHelperAppLifecycleReachesWindowsMaster 2>&1
  )"; then
    printf '%s\n' "$fixture_restart_output" >&2
    fail "fixture cursor did not survive a fresh app/helper/agent chain"
  fi
  [[ "$fixture_restart_output" == *"jarvis_mac_app_bridge_live_e2e_ok"* ]] \
    || fail "fixture restart omitted its live E2E marker"
  restarted_cursor="$(fixture_cursor)" \
    || fail "fixture restart lost the durable agent cursor"
  restarted_stream="${restarted_cursor%%|*}"
  fixture_sequence_restarted="${restarted_cursor##*|}"
  [[ "$restarted_stream" == "$fixture_stream" \
    && "$fixture_sequence_restarted" =~ ^[0-9]+$ \
    && "$fixture_sequence_restarted" -gt "$fixture_sequence_cancelled" ]] \
    || fail "fixture restart did not resume the same advancing cursor"

  standard_status_after="$("$BRIDGE_BIN" status)"
  [[ "$(json_value "$standard_status_after" status)" == "enrolled" \
    && "$(json_value "$standard_status_after" device_id)" == "$standard_device_id" \
    && "$(json_value "$standard_status_after" device_name)" == "$standard_device_name" \
    && "$(json_value "$standard_status_after" master_endpoint)" == "$standard_master_endpoint" \
    && "$(json_value "$standard_status_after" registry_revision)" == "$standard_registry_revision" \
    && "$(json_value "$standard_status_after" certificate_not_after_ms)" \
      == "$standard_certificate_not_after_ms" ]] \
    || fail "fixture run changed the stable standard Mac bridge profile projection"
  standard_connect_after="$("$BRIDGE_BIN" connect)"
  [[ "$(json_value "$standard_connect_after" status)" == "authenticated" \
    && "$(json_value "$standard_connect_after" device_id)" == "$standard_device_id" \
    && "$(json_value "$standard_connect_after" master_endpoint)" \
      == "$standard_master_endpoint" ]] \
    || fail "standard Mac bridge profile did not freshly reauthenticate after the fixture run"
  printf 'jarvis_mac_windows_fixture_live_e2e_ok endpoint=%s sequence_before=%s sequence_success=%s sequence_cancelled=%s sequence_restarted=%s fixture_profile=verified exact_event_binding=verified standard_profile_preserved=verified standard_profile_reauthenticated=verified cancellation=verified late_output_suppression=verified agent_restart=verified\n' \
    "$endpoint" "$fixture_sequence_before" "$fixture_sequence_success" \
    "$fixture_sequence_cancelled" "$fixture_sequence_restarted"
fi

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
  printf 'jarvis_mac_windows_outage_stop_required service=AssemblywrightMaster connection_epoch=%s\n' \
    "$outage_epoch_before"

  wait_for_outage_marker "master-offline" 180 "Master Offline after the induced outage"
  outage_error="$(tr -d '\r\n' <"$outage_directory/master-offline")"
  [[ "$outage_error" == "bridge_unavailable" \
    || "$outage_error" == "connection_failed" \
    || "$outage_error" == "invalid_health" ]] \
    || fail "outage lifecycle emitted a non-redacted offline error"
  printf 'jarvis_mac_windows_outage_start_required service=AssemblywrightMaster offline_error=%s\n' \
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
