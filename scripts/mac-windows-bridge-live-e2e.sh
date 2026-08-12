#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---run}"
PACKAGE_PATH="$ROOT_DIR/apps/mac"
PRODUCT="assemblywright-mac-bridge"
DEFAULT_SIGNED_APP="$PACKAGE_PATH/.build/assemblywright-mac-bridge-signed/Build/Products/Debug/assemblywright-mac-bridge.app"
DEFAULT_SIGNED_BIN="$DEFAULT_SIGNED_APP/Contents/MacOS/assemblywright-mac-bridge"

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

assert_feature_conveyor_sample() {
  local sample="$1"
  local label="$2"
  local schema queue_revision guidance_state next_owner_action
  schema="$(json_value "$sample" feature_conveyor.schema_version)"
  queue_revision="$(json_value "$sample" feature_conveyor.queue_revision)"
  guidance_state="$(json_value "$sample" feature_conveyor.owner_guidance.state)"
  next_owner_action="$(json_value "$sample" feature_conveyor.owner_guidance.next_owner_action)"
  [[ "$schema" == "8" ]] \
    || fail "$label Feature Conveyor schema was not v8"
  [[ "$queue_revision" =~ ^[0-9]+$ ]] \
    || fail "$label Feature Conveyor queue revision was invalid"
  [[ "$guidance_state" =~ ^(idle|ready|blocked|in_progress)$ ]] \
    || fail "$label Feature Conveyor guidance state was invalid"
  [[ "$next_owner_action" =~ ^(prepare_approved_feature|await_owner_control_surface|resolve_head_dependency|wait|reconcile_active_feature|resume_emergency_pause)$ ]] \
    || fail "$label Feature Conveyor guidance action was invalid"
}

case "$MODE" in
  --check)
    [[ -f "$PACKAGE_PATH/Package.swift" ]] || fail "missing Mac Swift package"
    [[ -f "$PACKAGE_PATH/Sources/AssemblywrightMacBridgeCLI/AssemblywrightMacBridgeCLI.swift" ]] \
      || fail "missing Mac bridge CLI"
    [[ -f "$PACKAGE_PATH/Sources/AssemblywrightMacCore/DeveloperEventRelay.swift" ]] \
      || fail "missing Mac event relay"
    [[ -f "$PACKAGE_PATH/AssemblywrightMacBridge.xcodeproj/project.pbxproj" ]] \
      || fail "missing provisioned Mac bridge Xcode project"
    [[ -f "$ROOT_DIR/crates/assemblywright-agent/src/main.rs" ]] \
      || fail "missing supervised Rust agent"
    [[ -f "$ROOT_DIR/packaging/AssemblywrightMacBridge.entitlements" ]] \
      || fail "missing Mac bridge Keychain entitlement"
    [[ -f "$ROOT_DIR/scripts/windows-local-coding-live-control.ps1" ]] \
      || fail "missing Windows local-coding live controller"
    bash -n "$ROOT_DIR/scripts/build-mac-bridge-signed.sh"
    bash -n "$ROOT_DIR/scripts/windows-local-coding-live-control-self-check.sh"
    "$ROOT_DIR/scripts/windows-local-coding-live-control-self-check.sh"
    swift build --package-path "$PACKAGE_PATH" --product "$PRODUCT"
    cargo build --manifest-path "$ROOT_DIR/Cargo.toml" -p assemblywright-agent --locked
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
  --run-local-coding)
    ;;
  --run-outage)
    ;;
  *)
    fail "usage: $0 [--check|--run|--run-relay|--run-fixture|--run-mlx|--run-local-coding|--run-outage]"
    ;;
esac

BRIDGE_BIN="${ASSEMBLYWRIGHT_MAC_BRIDGE_BIN:-$DEFAULT_SIGNED_BIN}"
[[ -x "$BRIDGE_BIN" ]] || fail \
  "signed Mac bridge is required; run ./scripts/build-mac-bridge-signed.sh or set ASSEMBLYWRIGHT_MAC_BRIDGE_BIN"
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

TAILSCALE_BIN="${ASSEMBLYWRIGHT_TAILSCALE_BIN:-$(command -v tailscale || true)}"
[[ -n "$TAILSCALE_BIN" && -x "$TAILSCALE_BIN" ]] \
  || fail "Tailscale CLI is required; set ASSEMBLYWRIGHT_TAILSCALE_BIN to its executable"
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
assert_feature_conveyor_sample "$monitor_first" "first bridge monitor sample"
assert_feature_conveyor_sample "$monitor_second" "second bridge monitor sample"
for forbidden in grant_secret certificate_pem ca_certificate_pem maintenance_reason boundary service_identity repository_id provider_id model_id owner_token; do
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
assert_feature_conveyor_sample "$reconnect_first" "first bridge reconnect sample"
assert_feature_conveyor_sample "$reconnect_second" "second bridge reconnect sample"
for forbidden in grant_secret certificate_pem ca_certificate_pem maintenance_reason boundary service_identity repository_id provider_id model_id owner_token; do
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
if [[ "$MODE" == "--run-relay" || "$MODE" == "--run-fixture" \
  || "$MODE" == "--run-mlx" || "$MODE" == "--run-local-coding" ]]; then
  command -v sqlite3 >/dev/null 2>&1 \
    || fail "sqlite3 is required for the durable relay proof"
  if [[ -n "${ASSEMBLYWRIGHT_MAC_AGENT_BIN:-}" ]]; then
    relay_agent_bin="$ASSEMBLYWRIGHT_MAC_AGENT_BIN"
  else
    cargo build --manifest-path "$ROOT_DIR/Cargo.toml" -p assemblywright-agent --locked
    relay_agent_bin="$ROOT_DIR/target/debug/assemblywright-agent"
  fi
  [[ -x "$relay_agent_bin" ]] \
    || fail "assemblywright-agent executable is unavailable"
  codesign --verify --strict "$relay_agent_bin" >/dev/null 2>&1 \
    || fail "assemblywright-agent signature is invalid"
  relay_directory="$(mktemp -d -t assemblywright-mac-agent-relay)"
  chmod 700 "$relay_directory"
  relay_data_directory="$relay_directory/data"
  app_lifecycle_environment=(
    "ASSEMBLYWRIGHT_MAC_DEVELOPER_AGENT_EXECUTABLE=$relay_agent_bin"
    "ASSEMBLYWRIGHT_MAC_DEVELOPER_AGENT_DATA_DIR=$relay_data_directory"
  )
  if [[ "$MODE" == "--run-fixture" ]]; then
    app_lifecycle_environment+=(
      "ASSEMBLYWRIGHT_MAC_DEVELOPER_FIXTURE_JOBS_ENABLED=true"
    )
  elif [[ "$MODE" == "--run-mlx" ]]; then
    [[ -n "${ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_EXECUTABLE:-}" \
      && -x "$ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_EXECUTABLE" ]] \
      || fail "set ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_EXECUTABLE to the exact executable"
    [[ -n "${ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_DIR:-}" \
      && -d "$ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_DIR" ]] \
      || fail "set ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_DIR to the offline model directory"
    [[ -n "${ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_ID:-}" ]] \
      || fail "set ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_ID to the enrolled model identifier"
    app_lifecycle_environment+=(
      "ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_JOBS_ENABLED=true"
      "ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_EXECUTABLE=$ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_EXECUTABLE"
      "ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_DIR=$ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_DIR"
      "ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_ID=$ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_ID"
    )
  elif [[ "$MODE" == "--run-local-coding" ]]; then
    app_lifecycle_environment+=(
      "ASSEMBLYWRIGHT_MAC_DEVELOPER_LOCAL_CODING_SNAPSHOTS_ENABLED=true"
    )
  fi
  trap cleanup_relay EXIT
fi

if [[ "$MODE" != "--run-fixture" && "$MODE" != "--run-mlx" \
  && "$MODE" != "--run-local-coding" ]] \
  && ! app_lifecycle_output="$(
  env \
    ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_LIVE_E2E=true \
    ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
    ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
    ${app_lifecycle_environment[@]+"${app_lifecycle_environment[@]}"} \
    swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
      --filter liveSignedHelperAppLifecycleReachesWindowsMaster 2>&1
)"; then
  printf '%s\n' "$app_lifecycle_output" >&2
  fail "production app bridge lifecycle did not reach the Windows master"
fi
if [[ "$MODE" != "--run-fixture" && "$MODE" != "--run-mlx" \
  && "$MODE" != "--run-local-coding" ]]; then
  [[ "$app_lifecycle_output" == *"assemblywright_mac_app_bridge_live_e2e_ok"* ]] \
    || fail "production app bridge lifecycle omitted its live E2E marker"
  [[ "$app_lifecycle_output" == *"feature_conveyor_schema=8"* ]] \
    || fail "production app bridge lifecycle omitted schema-v8 Feature Conveyor proof"
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
      ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_LIVE_E2E=true \
      ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
      ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
      ${app_lifecycle_environment[@]+"${app_lifecycle_environment[@]}"} \
      swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
        --filter liveSignedHelperAppLifecycleReachesWindowsMaster 2>&1
  )"; then
    printf '%s\n' "$relay_resume_output" >&2
    fail "production app relay did not resume through a fresh helper and agent"
  fi
  [[ "$relay_resume_output" == *"assemblywright_mac_app_bridge_live_e2e_ok"* ]] \
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
  printf 'assemblywright_mac_windows_event_relay_live_e2e_ok endpoint=%s stream_id=%s sequence_before=%s sequence_after=%s app_supervision=verified agent_restart=verified\n' \
    "$endpoint" "$first_stream" "$first_sequence" "$resumed_sequence"
fi

if [[ "$MODE" == "--run-local-coding" ]]; then
  local_coding_profile=(--identity-profile local-coding)
  local_coding_status="$(
    "$BRIDGE_BIN" status \
      ${local_coding_profile[@]+"${local_coding_profile[@]}"}
  )"
  [[ "$(json_value "$local_coding_status" status)" == "enrolled" ]] \
    || fail "the separate local-coding identity is not enrolled"
  local_coding_device_id="$(json_value "$local_coding_status" device_id)"
  local_coding_registry_revision="$(json_value "$local_coding_status" registry_revision)"
  local_coding_endpoint="$(json_value "$local_coding_status" master_endpoint)"
  [[ "$local_coding_device_id" =~ ^[0-9a-fA-F-]{36}$ \
    && "$local_coding_device_id" != "$standard_device_id" \
    && "$local_coding_registry_revision" =~ ^[0-9]+$ \
    && "$local_coding_registry_revision" -gt 0 \
    && "$local_coding_endpoint" == "$standard_master_endpoint" ]] \
    || fail "the local-coding identity was not isolated and endpoint-bound"
  owner_designation_revision="${ASSEMBLYWRIGHT_FEATURE_CONVEYOR_OWNER_CONTROL_DESIGNATION_REVISION:-}"
  [[ "$owner_designation_revision" =~ ^[0-9]+$ && "$owner_designation_revision" -gt 0 ]] \
    || fail "set ASSEMBLYWRIGHT_FEATURE_CONVEYOR_OWNER_CONTROL_DESIGNATION_REVISION to the exact current revision"

  local_coding_coordination_directory="$relay_directory/local-coding-coordination"
  mkdir "$local_coding_coordination_directory"
  chmod 700 "$local_coding_coordination_directory"
  local_coding_output="$relay_directory/local-coding-live.log"
  : >"$local_coding_output"
  chmod 600 "$local_coding_output"
  local_coding_pid=""

  cleanup_local_coding() {
    if [[ -n "$local_coding_pid" ]] && kill -0 "$local_coding_pid" >/dev/null 2>&1; then
      kill "$local_coding_pid" >/dev/null 2>&1 || true
      local deadline=$((SECONDS + 5))
      while kill -0 "$local_coding_pid" >/dev/null 2>&1 \
        && (( SECONDS < deadline )); do
        sleep 0.1
      done
      if kill -0 "$local_coding_pid" >/dev/null 2>&1; then
        kill -KILL "$local_coding_pid" >/dev/null 2>&1 || true
      fi
      wait "$local_coding_pid" >/dev/null 2>&1 || true
    fi
    cleanup_relay
  }
  trap cleanup_local_coding EXIT

  capture_local_coding_receipt() {
    local filename="$1"
    local label="$2"
    local receipt=""
    if ! IFS= read -r -t 600 receipt; then
      fail "timed out waiting for the sanitized $label receipt on stdin"
    fi
    [[ -n "$receipt" && "${#receipt}" -le 8192 ]] \
      || fail "the sanitized $label receipt was empty or oversized"
    (
      umask 077
      printf '%s' "$receipt" >"$local_coding_coordination_directory/$filename.tmp"
    )
    chmod 600 "$local_coding_coordination_directory/$filename.tmp"
    mv "$local_coding_coordination_directory/$filename.tmp" \
      "$local_coding_coordination_directory/$filename"
  }

  printf '%s\n' \
    "assemblywright_mac_windows_local_coding_prepare_required action=Prepare script=scripts/windows-local-coding-live-control.ps1 owner_control_designation_revision=$owner_designation_revision receipt_stdin=required"
  capture_local_coding_receipt "prepare-control.json" "local-coding repository preparation"
  prepare_receipt="$(<"$local_coding_coordination_directory/prepare-control.json")"
  [[ "$(json_value "$prepare_receipt" status)" == "local_coding_repository_prepared" \
    && "$(json_value "$prepare_receipt" owner_control_designation_revision)" == "$owner_designation_revision" ]] \
    || fail "the repository preparation receipt drifted from owner control"
  local_coding_repository_id="$(json_value "$prepare_receipt" repository_id)"
  local_coding_feature_id="$(json_value "$prepare_receipt" feature_id)"
  local_coding_head_commit="$(json_value "$prepare_receipt" head_commit)"
  local_coding_prepare_queue_revision="$(json_value "$prepare_receipt" queue_revision)"
  local_coding_pause_revision="$(json_value "$prepare_receipt" emergency_pause_revision)"
  local_coding_approved_request_sha="$(json_value "$prepare_receipt" approved_request_sha256)"
  local_coding_approved_request_base64="$(json_value "$prepare_receipt" approved_request_base64)"
  [[ "$local_coding_repository_id" =~ ^[0-9a-fA-F-]{36}$ \
    && "$local_coding_feature_id" =~ ^[0-9a-fA-F-]{36}$ \
    && "$local_coding_head_commit" =~ ^[0-9a-f]{40}$ \
    && "$local_coding_prepare_queue_revision" =~ ^[0-9]+$ \
    && "$local_coding_pause_revision" =~ ^[0-9]+$ \
    && "$local_coding_approved_request_sha" =~ ^[0-9a-f]{64}$ \
    && "${#local_coding_approved_request_base64}" -le 6144 ]] \
    || fail "the repository preparation receipt contained invalid bindings"
  local_coding_approved_request="$(printf '%s' "$local_coding_approved_request_base64" | /usr/bin/base64 -D)" \
    || fail "the approved local-coding request was not canonical base64"
  computed_approved_request_sha="$(printf '%s' "$local_coding_approved_request" | shasum -a 256 | awk '{print $1}')"
  [[ "$computed_approved_request_sha" == "$local_coding_approved_request_sha" ]] \
    || fail "the approved local-coding request digest drifted"
  enqueue_receipt="$(printf '%s' "$local_coding_approved_request" \
    | "$BRIDGE_BIN" feature-conveyor approve-and-enqueue --confirm)"
  enqueue_receipt_feature_id="$(
    json_value "$enqueue_receipt" feature_id | tr '[:upper:]' '[:lower:]'
  )"
  [[ "$(json_value "$enqueue_receipt" status)" == "queued" \
    && "$enqueue_receipt_feature_id" == "$local_coding_feature_id" \
    && "$(json_value "$enqueue_receipt" specification_revision)" == "1" \
    && "$(json_value "$enqueue_receipt" lifecycle_revision)" == "1" \
    && "$(json_value "$enqueue_receipt" queue_revision)" -eq $((local_coding_prepare_queue_revision + 1)) \
    && "$(json_value "$enqueue_receipt" owner_control_designation_revision)" == "$owner_designation_revision" \
    && "$(json_value "$enqueue_receipt" emergency_pause_revision)" == "$local_coding_pause_revision" ]] \
    || fail "the signed-helper enqueue receipt drifted"
  local_coding_enqueue_queue_revision="$(json_value "$enqueue_receipt" queue_revision)"

  local_coding_startup="$(printf \
    '{"agent_data_dir":"%s","agent_executable_path":"%s","fixture_jobs_enabled":false,"local_coding_snapshots_enabled":true,"mlx_executable_path":null,"mlx_jobs_enabled":false,"mlx_model_dir":null,"mlx_model_id":null,"version":4}' \
    "$relay_data_directory" "$relay_agent_bin")"
  printf '%s' "$local_coding_startup" \
    | "$BRIDGE_BIN" relay \
      ${local_coding_profile[@]+"${local_coding_profile[@]}"} \
      --samples 10000 --interval-ms 100 \
      >"$local_coding_output" 2>&1 &
  local_coding_pid="$!"
  local_coding_relay_deadline=$((SECONDS + 180))
  while ! rg -q '"phase":"authenticated"' "$local_coding_output"; do
    kill -0 "$local_coding_pid" >/dev/null 2>&1 \
      || { cat "$local_coding_output" >&2; fail "the production local-coding relay exited"; }
    (( SECONDS < local_coding_relay_deadline )) \
      || { cat "$local_coding_output" >&2; fail "timed out waiting for local-coding relay"; }
    sleep 0.25
  done
  ! rg -q '"feature_conveyor"' "$local_coding_output" \
    || fail "the InferenceWorker relay received the MacBridge-only Conveyor projection"

  printf '%s\n' \
    "assemblywright_mac_windows_local_coding_dispatch_required action=ClaimAndDispatch script=scripts/windows-local-coding-live-control.ps1 repository_id=$local_coding_repository_id feature_id=$local_coding_feature_id head_commit=$local_coding_head_commit local_coding_device_id=$local_coding_device_id local_coding_registry_revision=$local_coding_registry_revision expected_lifecycle_revision=1 expected_queue_revision=$local_coding_enqueue_queue_revision expected_emergency_pause_revision=$local_coding_pause_revision receipt_stdin=required"
  capture_local_coding_receipt "dispatch-control.json" "local-coding dispatch"
  dispatch_receipt="$(<"$local_coding_coordination_directory/dispatch-control.json")"
  [[ "$(json_value "$dispatch_receipt" status)" == "local_coding_dispatch_succeeded" \
    && "$(json_value "$dispatch_receipt" repository_id)" == "$local_coding_repository_id" \
    && "$(json_value "$dispatch_receipt" feature_id)" == "$local_coding_feature_id" \
    && "$(json_value "$dispatch_receipt" device_id)" == "$local_coding_device_id" \
    && "$(json_value "$dispatch_receipt" transfer_staging_empty)" == "true" \
    && "$(json_value "$dispatch_receipt" proof_checkout_clean)" == "true" ]] \
    || fail "the terminal local-coding dispatch receipt drifted"
  local_coding_lifecycle_revision="$(json_value "$dispatch_receipt" lifecycle_revision)"
  local_coding_claim_queue_revision="$(json_value "$dispatch_receipt" queue_revision)"
  local_coding_task_id="$(json_value "$dispatch_receipt" task_id)"
  local_coding_step_id="$(json_value "$dispatch_receipt" step_id)"
  local_coding_queued_sequence="$(json_value "$dispatch_receipt" queued_sequence)"
  local_coding_leased_sequence="$(json_value "$dispatch_receipt" leased_sequence)"
  local_coding_succeeded_sequence="$(json_value "$dispatch_receipt" succeeded_sequence)"
  local_coding_snapshot_sha="$(json_value "$dispatch_receipt" snapshot_sha256)"
  local_coding_packet_sha="$(json_value "$dispatch_receipt" work_packet_sha256)"
  [[ "$local_coding_lifecycle_revision" =~ ^[0-9]+$ \
    && "$local_coding_claim_queue_revision" -eq $((local_coding_enqueue_queue_revision + 1)) \
    && "$local_coding_task_id" =~ ^[0-9a-fA-F-]{36}$ \
    && "$local_coding_step_id" =~ ^[0-9a-fA-F-]{36}$ \
    && "$local_coding_queued_sequence" -lt "$local_coding_leased_sequence" \
    && "$local_coding_leased_sequence" -lt "$local_coding_succeeded_sequence" \
    && "$local_coding_snapshot_sha" =~ ^[0-9a-f]{64}$ \
    && "$local_coding_packet_sha" =~ ^[0-9a-f]{64}$ ]] \
    || fail "the terminal local-coding receipt omitted exact revision or digest evidence"

  printf '%s\n' \
    "assemblywright_mac_windows_artifact_integration_required action=Integrate script=scripts/windows-local-coding-live-control.ps1 repository_id=$local_coding_repository_id feature_id=$local_coding_feature_id head_commit=$local_coding_head_commit receipt_stdin=required"
  capture_local_coding_receipt "integration-control.json" "artifact integration"
  integration_receipt="$(<"$local_coding_coordination_directory/integration-control.json")"
  [[ "$(json_value "$integration_receipt" status)" == "artifact_integration_candidate_frozen" \
    && "$(json_value "$integration_receipt" repository_id)" == "$local_coding_repository_id" \
    && "$(json_value "$integration_receipt" feature_id)" == "$local_coding_feature_id" \
    && "$(json_value "$integration_receipt" base_commit)" == "$local_coding_head_commit" \
    && "$(json_value "$integration_receipt" candidate_detached)" == "true" \
    && "$(json_value "$integration_receipt" candidate_remote_absent)" == "true" \
    && "$(json_value "$integration_receipt" candidate_worktree_clean)" == "true" \
    && "$(json_value "$integration_receipt" candidate_fsck_clean)" == "true" \
    && "$(json_value "$integration_receipt" proof_checkout_clean)" == "true" \
    && "$(json_value "$integration_receipt" exact_retry_idempotent)" == "true" ]] \
    || fail "the artifact integration receipt drifted"
  local_coding_lifecycle_revision="$(json_value "$integration_receipt" lifecycle_revision)"
  local_coding_integration_id="$(json_value "$integration_receipt" integration_id)"
  local_coding_candidate_commit="$(json_value "$integration_receipt" candidate_commit)"
  local_coding_candidate_tree="$(json_value "$integration_receipt" candidate_tree)"
  local_coding_artifact_set_sha="$(json_value "$integration_receipt" artifact_set_sha256)"
  [[ "$local_coding_lifecycle_revision" =~ ^[0-9]+$ \
    && "$local_coding_integration_id" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ \
    && "$local_coding_candidate_commit" =~ ^[0-9a-f]{40}$ \
    && "$local_coding_candidate_tree" =~ ^[0-9a-f]{40}$ \
    && "$local_coding_artifact_set_sha" =~ ^[0-9a-f]{64}$ ]] \
    || fail "the artifact integration receipt omitted exact candidate evidence"

  kill "$local_coding_pid" >/dev/null 2>&1 || true
  wait "$local_coding_pid" >/dev/null 2>&1 || true
  local_coding_pid=""
  local_coding_snapshot_root="$relay_data_directory/local-coding-snapshots"
  [[ -d "$local_coding_snapshot_root" && ! -L "$local_coding_snapshot_root" \
    && "$(stat -f '%Lp' "$local_coding_snapshot_root")" == "700" ]] \
    || fail "the Mac local-coding retention root was absent, linked, or not private"
  shopt -s nullglob
  local_coding_retained_entries=("$local_coding_snapshot_root"/*)
  shopt -u nullglob
  [[ "${#local_coding_retained_entries[@]}" -eq 2 ]] \
    || fail "the Mac did not retain exactly one sealed workspace and recovery record"
  local_coding_sealed_workspace=""
  local_coding_retention_record=""
  for local_coding_retained_entry in \
    ${local_coding_retained_entries[@]+"${local_coding_retained_entries[@]}"}; do
    case "$(basename "$local_coding_retained_entry")" in
      *.sealed)
        [[ -z "$local_coding_sealed_workspace" \
          && -d "$local_coding_retained_entry" \
          && ! -L "$local_coding_retained_entry" \
          && "$(stat -f '%Lp' "$local_coding_retained_entry")" == "700" ]] \
          || fail "the retained Mac workspace was ambiguous, linked, or not private"
        local_coding_sealed_workspace="$local_coding_retained_entry"
        ;;
      *.retention.json)
        [[ -z "$local_coding_retention_record" \
          && -f "$local_coding_retained_entry" \
          && ! -L "$local_coding_retained_entry" \
          && "$(stat -f '%Lp' "$local_coding_retained_entry")" == "600" ]] \
          || fail "the retained Mac recovery record was ambiguous, linked, or not private"
        local_coding_retention_record="$local_coding_retained_entry"
        ;;
      *)
        fail "the Mac retained an unexpected local-coding entry"
        ;;
    esac
  done
  [[ -n "$local_coding_sealed_workspace" && -n "$local_coding_retention_record" ]] \
    || fail "the Mac retained an incomplete local-coding attempt pair"
  local_coding_retained_attempt="$(basename "$local_coding_sealed_workspace" .sealed)"
  [[ "$local_coding_retained_attempt" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ \
    && "$(basename "$local_coding_retention_record")" == "$local_coding_retained_attempt.retention.json" ]] \
    || fail "the retained Mac workspace and recovery record were not one exact attempt pair"

  # This disposable harness owns the temporary relay root. Product cancellation
  # and restart recovery are proved by the native relay E2E; this live lane first
  # proves retention, then removes only the validated harness-owned pair.
  rm -rf -- "$local_coding_sealed_workspace"
  rm -f -- "$local_coding_retention_record"
  [[ -z "$(find "$local_coding_snapshot_root" -mindepth 1 -maxdepth 1 -print -quit)" ]] \
    || fail "the harness did not remove its exact retained Mac attempt pair"
  local_coding_mac_cleanup_sha="$(printf \
    'assemblywright.local-coding-live.mac-retained-cleanup.v2\\0%s\\0%s\\0%s\\0%s' \
    "$local_coding_feature_id" "$local_coding_task_id" "$local_coding_step_id" \
    "$local_coding_retained_attempt" \
    | shasum -a 256 | awk '{print $1}')"

  printf '%s\n' \
    "assemblywright_mac_windows_local_coding_cancel_required action=Cancel script=scripts/windows-local-coding-live-control.ps1 feature_id=$local_coding_feature_id expected_lifecycle_revision=$local_coding_lifecycle_revision expected_queue_revision=$local_coding_claim_queue_revision expected_emergency_pause_revision=$local_coding_pause_revision receipt_stdin=required"
  capture_local_coding_receipt "cancel-control.json" "active-feature cancellation"
  cancel_receipt="$(<"$local_coding_coordination_directory/cancel-control.json")"
  [[ "$(json_value "$cancel_receipt" status)" == "local_coding_feature_cancelled" \
    && "$(json_value "$cancel_receipt" feature_id)" == "$local_coding_feature_id" \
    && "$(json_value "$cancel_receipt" queue_revision)" == "$local_coding_claim_queue_revision" \
    && "$(json_value "$cancel_receipt" lease_retained)" == "true" \
    && "$(json_value "$cancel_receipt" advancement_authorized)" == "false" ]] \
    || fail "cancellation did not retain the lease and deny advancement"
  local_coding_cancel_lifecycle="$(json_value "$cancel_receipt" lifecycle_revision)"
  [[ "$local_coding_cancel_lifecycle" -eq $((local_coding_lifecycle_revision + 1)) ]] \
    || fail "cancellation lifecycle revision was not contiguous"

  printf '%s\n' \
    "assemblywright_mac_windows_local_coding_abandon_required action=Abandon script=scripts/windows-local-coding-live-control.ps1 repository_id=$local_coding_repository_id feature_id=$local_coding_feature_id head_commit=$local_coding_head_commit task_id=$local_coding_task_id step_id=$local_coding_step_id succeeded_sequence=$local_coding_succeeded_sequence mac_cleanup_sha256=$local_coding_mac_cleanup_sha expected_lifecycle_revision=$local_coding_cancel_lifecycle expected_queue_revision=$local_coding_claim_queue_revision expected_emergency_pause_revision=$local_coding_pause_revision receipt_stdin=required"
  capture_local_coding_receipt "abandon-control.json" "safe abandonment"
  abandon_receipt="$(<"$local_coding_coordination_directory/abandon-control.json")"
  [[ "$(json_value "$abandon_receipt" status)" == "local_coding_feature_abandoned" \
    && "$(json_value "$abandon_receipt" feature_id)" == "$local_coding_feature_id" \
    && "$(json_value "$abandon_receipt" lease_released)" == "true" \
    && "$(json_value "$abandon_receipt" queue_empty)" == "true" \
    && "$(json_value "$abandon_receipt" transfer_staging_empty)" == "true" \
    && "$(json_value "$abandon_receipt" safe_reconciliation_sha256)" =~ ^[0-9a-f]{64}$ ]] \
    || fail "safe abandonment did not release the exact lease into an empty queue"

  printf '%s\n' \
    "assemblywright_mac_windows_local_coding_cleanup_required action=Cleanup script=scripts/windows-local-coding-live-control.ps1 repository_id=$local_coding_repository_id feature_id=$local_coding_feature_id head_commit=$local_coding_head_commit receipt_stdin=required"
  capture_local_coding_receipt "cleanup-control.json" "disposable proof cleanup"
  cleanup_receipt="$(<"$local_coding_coordination_directory/cleanup-control.json")"
  [[ "$(json_value "$cleanup_receipt" status)" == "local_coding_live_cleanup_complete" \
    && "$(json_value "$cleanup_receipt" repository_id)" == "$local_coding_repository_id" \
    && "$(json_value "$cleanup_receipt" feature_id)" == "$local_coding_feature_id" \
    && "$(json_value "$cleanup_receipt" absent_grant_count)" == "0" \
    && "$(json_value "$cleanup_receipt" revoked_grant_count)" == "3" \
    && "$(json_value "$cleanup_receipt" grant_cleanup_status)" == "absent_or_revoked" \
    && "$(json_value "$cleanup_receipt" proof_checkout_removed)" == "true" ]] \
    || fail "the disposable Windows cleanup receipt drifted"

  local_coding_status_after="$(
    "$BRIDGE_BIN" status \
      ${local_coding_profile[@]+"${local_coding_profile[@]}"}
  )"
  [[ "$(json_value "$local_coding_status_after" status)" == "enrolled" \
    && "$(json_value "$local_coding_status_after" device_id)" == "$local_coding_device_id" \
    && "$(json_value "$local_coding_status_after" registry_revision)" == "$local_coding_registry_revision" ]] \
    || fail "the live proof changed the separate local-coding identity"
  printf 'assemblywright_mac_windows_local_coding_live_e2e_ok endpoint=%s feature_id=%s task_id=%s step_id=%s queued_sequence=%s leased_sequence=%s succeeded_sequence=%s snapshot_sha256=%s work_packet_sha256=%s integration_id=%s candidate_commit=%s candidate_tree=%s artifact_set_sha256=%s separate_identity=verified signed_swift_relay=verified real_rust_agent=verified mac_retained_attempt_pair_shape=verified harness_owned_pair_cleanup=verified artifact_integration=verified detached_candidate=verified candidate_remote_absent=verified candidate_fsck_clean=verified exact_integration_retry=verified source_checkout_clean=verified owner_cancel=verified owner_abandon=verified queue_empty=verified feature_lease_empty=verified distributed_active_state_empty=verified windows_transfer_staging_empty=verified grants_revoked=verified disposable_checkout_removed=verified\n' \
    "$local_coding_endpoint" "$local_coding_feature_id" "$local_coding_task_id" \
    "$local_coding_step_id" "$local_coding_queued_sequence" "$local_coding_leased_sequence" \
    "$local_coding_succeeded_sequence" "$local_coding_snapshot_sha" "$local_coding_packet_sha" \
    "$local_coding_integration_id" "$local_coding_candidate_commit" \
    "$local_coding_candidate_tree" "$local_coding_artifact_set_sha"
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
    ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_LIVE_E2E=true \
    ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_COORDINATION_DIR="$mlx_coordination_directory" \
    ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
    ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
    ${app_lifecycle_environment[@]+"${app_lifecycle_environment[@]}"} \
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
    "assemblywright_mac_windows_mlx_success_enqueue_required action=EnqueueSuccess script=scripts/windows-mlx-live-control.ps1 expected_device_id=$standard_device_id receipt_stdin=required"
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
    "assemblywright_mac_windows_mlx_cancellation_enqueue_required action=EnqueueCancellationAndPause script=scripts/windows-mlx-live-control.ps1 expected_device_id=$standard_device_id receipts_stdin=leased_then_cancelled"
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
    "assemblywright_mac_windows_mlx_pause_receipt_required action=EnqueueCancellationAndPause expected_device_id=$standard_device_id connection_epoch=$cancellation_receipt_epoch receipt_stdin=second_receipt"
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
    'assemblywright_mac_windows_mlx_resume_required action=Resume script=scripts/windows-mlx-live-control.ps1 receipt_stdin=required'
  capture_mlx_control_receipt "resume-control.json" "MLX resume"
  wait_for_mlx_marker "mlx-complete" 300 "deliberate MLX admission resume"
  if ! wait "$mlx_pid"; then
    cat "$mlx_output" >&2
    fail "production app MLX lifecycle failed"
  fi
  mlx_pid=""
  [[ "$(cat "$mlx_output")" == *"assemblywright_mac_app_mlx_live_e2e_ok"* ]] \
    || fail "production app MLX lifecycle omitted its live E2E marker"

  if ! mlx_restart_output="$(
    env \
      ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_LIVE_E2E=true \
      ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
      ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
      ${app_lifecycle_environment[@]+"${app_lifecycle_environment[@]}"} \
      swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
        --filter liveSignedHelperAppLifecycleReachesWindowsMaster 2>&1
  )"; then
    printf '%s\n' "$mlx_restart_output" >&2
    fail "MLX cursor did not survive a fresh app/helper/agent chain"
  fi
  [[ "$mlx_restart_output" == *"assemblywright_mac_app_bridge_live_e2e_ok"* ]] \
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
  printf 'assemblywright_mac_windows_mlx_live_e2e_ok endpoint=%s sequence_before=%s sequence_success=%s sequence_cancelled=%s sequence_restarted=%s local_inference=verified exact_event_binding=verified cancellation=verified late_output_suppression=verified agent_restart=verified\n' \
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
    ASSEMBLYWRIGHT_MAC_DEVELOPER_FIXTURE_LIVE_E2E=true \
    ASSEMBLYWRIGHT_MAC_DEVELOPER_FIXTURE_COORDINATION_DIR="$fixture_coordination_directory" \
    ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
    ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
    ${app_lifecycle_environment[@]+"${app_lifecycle_environment[@]}"} \
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
    'assemblywright_mac_windows_fixture_success_enqueue_required action=EnqueueSuccess script=scripts/windows-fixture-live-control.ps1 receipt_stdin=required'
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
    'assemblywright_mac_windows_fixture_cancellation_enqueue_required action=EnqueueCancellation script=scripts/windows-fixture-live-control.ps1 receipt_stdin=required'
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
    'assemblywright_mac_windows_fixture_pause_required action=Pause script=scripts/windows-fixture-live-control.ps1 args=prior_cancellation_receipt receipt_stdin=required'
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
    'assemblywright_mac_windows_fixture_resume_required action=Resume script=scripts/windows-fixture-live-control.ps1 receipt_stdin=required'
  capture_fixture_control_receipt "resume-control.json" "fixture resume"
  wait_for_fixture_marker "fixture-complete" 240 "deliberate fixture admission resume"
  if ! wait "$fixture_pid"; then
    fail "production app fixture lifecycle failed"
  fi
  fixture_pid=""
  [[ "$(cat "$fixture_output")" == *"assemblywright_mac_app_fixture_live_e2e_ok"* ]] \
    || fail "production app fixture lifecycle omitted its live E2E marker"

  if ! fixture_restart_output="$(
    env \
      ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_LIVE_E2E=true \
      ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
      ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
      ${app_lifecycle_environment[@]+"${app_lifecycle_environment[@]}"} \
      swift test --disable-sandbox --package-path "$PACKAGE_PATH" \
        --filter liveSignedHelperAppLifecycleReachesWindowsMaster 2>&1
  )"; then
    printf '%s\n' "$fixture_restart_output" >&2
    fail "fixture cursor did not survive a fresh app/helper/agent chain"
  fi
  [[ "$fixture_restart_output" == *"assemblywright_mac_app_bridge_live_e2e_ok"* ]] \
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
  printf 'assemblywright_mac_windows_fixture_live_e2e_ok endpoint=%s sequence_before=%s sequence_success=%s sequence_cancelled=%s sequence_restarted=%s fixture_profile=verified exact_event_binding=verified standard_profile_preserved=verified standard_profile_reauthenticated=verified cancellation=verified late_output_suppression=verified agent_restart=verified\n' \
    "$endpoint" "$fixture_sequence_before" "$fixture_sequence_success" \
    "$fixture_sequence_cancelled" "$fixture_sequence_restarted"
fi

if [[ "$MODE" == "--run-outage" ]]; then
  outage_directory="$(mktemp -d -t assemblywright-mac-bridge-outage)"
  chmod 700 "$outage_directory"
  outage_output="$(mktemp -t assemblywright-mac-bridge-outage-log)"
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

  ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_OUTAGE_LIVE_E2E=true \
  ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_OUTAGE_COORDINATION_DIR="$outage_directory" \
  ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_EXECUTABLE="$BRIDGE_BIN" \
  ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER="$bridge_team" \
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
  printf 'assemblywright_mac_windows_outage_stop_required service=AssemblywrightMaster connection_epoch=%s\n' \
    "$outage_epoch_before"

  wait_for_outage_marker "master-offline" 180 "Master Offline after the induced outage"
  outage_error="$(tr -d '\r\n' <"$outage_directory/master-offline")"
  [[ "$outage_error" == "bridge_unavailable" \
    || "$outage_error" == "connection_failed" \
    || "$outage_error" == "invalid_health" ]] \
    || fail "outage lifecycle emitted a non-redacted offline error"
  printf 'assemblywright_mac_windows_outage_start_required service=AssemblywrightMaster offline_error=%s\n' \
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
  [[ "$(cat "$outage_output")" == *"assemblywright_mac_app_bridge_outage_recovery_live_e2e_ok"* ]] \
    || fail "production app outage lifecycle omitted its live E2E marker"
  printf 'assemblywright_mac_windows_outage_recovery_live_e2e_ok endpoint=%s connection_epoch_before=%s connection_epoch_after=%s offline_error=%s app_supervision=verified\n' \
    "$endpoint" "$outage_epoch_before" "$outage_epoch_after" "$outage_error"
fi

printf 'assemblywright_mac_windows_bridge_live_e2e_ok endpoint=%s connection_epoch=%s monitor_epoch=%s monitor_samples=2 reconnect_epoch_before=%s reconnect_epoch_after=%s app_supervision=verified team=%s\n' \
  "$endpoint" "$connection_epoch" "$monitor_first_epoch" "$reconnect_first_epoch" "$reconnect_second_epoch" "$bridge_team"
