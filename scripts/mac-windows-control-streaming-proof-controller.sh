#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "${BASH_SOURCE[0]%/*}/.." && pwd -P)"
SYSTEM_PATH="/usr/bin:/bin:/usr/sbin:/sbin"
LIVE_PATH="/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/Users/michaelnobile/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin"
PATH="$SYSTEM_PATH"
export PATH

OUTPUT_RELATIVE="target/mac-windows-control-streaming-live-proof"
RECEIPT_NAME="mac-windows-control-streaming-live-proof.json"
DIGEST_NAME="mac-windows-control-streaming-live-proof.sha256"
SCHEMA="assemblywright.mac-windows-control-streaming-live-proof.v1"
CATEGORY="mac_windows_control_event_streaming_live"
ORIGIN="mac_windows_control_event_streaming_proof_controller"
PROOF_IDENTITY="assemblywright.mac-windows-control-event-streaming-live.v1"
CONTROLLER_PATH="scripts/mac-windows-control-streaming-proof-controller.sh"
HARNESS_PATH="scripts/mac-windows-bridge-live-e2e.sh"
HELPER_RELATIVE="apps/mac/.build/assemblywright-mac-bridge-signed/Build/Products/Debug/assemblywright-mac-bridge.app/Contents/MacOS/assemblywright-mac-bridge"
AGENT_RELATIVE="target/debug/assemblywright-agent"
FIXED_HELPER_IDENTIFIER="com.nobiletechnology.assemblywright.developer-bridge.cli"
FIXED_HELPER_TEAM="H686S3N4V9"
MAX_TRANSCRIPT_BYTES=1048576
MAX_TRANSCRIPT_LINES=4096
MAX_LINE_BYTES=8192
PROOF_BOUNDARY="Exact clean published main used the committed native Mac/Windows bridge harness through fixed Bash stdin in --run-relay mode. One independently signed Swift helper and the exact signed Rust agent completed exporter-bound mTLS owner-control projection plus durable same-stream advancing event-cursor recovery after a fresh helper and agent restart. The private coordination transcript was hashed and deleted. This is proof production only: it does not admit evidence, approve or activate orchestration, grant protocol/schema/runtime authority, prove current-source linkage of the built binaries, Developer ID distribution, notarization, clean-profile installation, unattended operation, or production readiness."

FIXED_GIT=/usr/bin/git
FIXED_BASH=/bin/bash
FIXED_CODESIGN=/usr/bin/codesign
FIXED_SWIFT=/usr/bin/swift
FIXED_SQLITE=/usr/bin/sqlite3
FIXED_NC=/usr/bin/nc
FIXED_TAILSCALE=/usr/local/bin/tailscale
FIXED_CARGO=/Users/michaelnobile/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/cargo
FIXED_HOME=/Users/michaelnobile
FIXED_USER=michaelnobile
FIXED_GIT_SHA256=179301dcb41ea78accc3fa0048a7e6f6710d891945a751a34addd622020c1818
FIXED_BASH_SHA256=fde343ee184953c1fa1185abddeaa8be61c6acbebae4eb54db5d6b55b09a5755
FIXED_CODESIGN_SHA256=214d455584d19abc0d74d02b9cbc7d3da6bdcb0596c235e6156dd9ed2f4e1ba7
FIXED_SWIFT_SHA256=179301dcb41ea78accc3fa0048a7e6f6710d891945a751a34addd622020c1818
FIXED_SQLITE_SHA256=96f2e9df7e30cd3ad344c9c1bd06c1e3e0878ac723a181c0b07b7a3941ab3c00
FIXED_NC_SHA256=427423db6d5d5e9f720c5e110a2c9b3cba39ea089dafed4ab936d04dd218bdac
FIXED_TAILSCALE_SHA256=26b0e5a65e1b723e38f187619964b443e44118c2e02217e9b11165ce17afa65e
FIXED_CARGO_SHA256=c512bff73c86143b557463f021d0c3d5b0490d97d65040ba59ea2b3427784758

fixture_mode=0
live_pid=""

unset ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_STDIN_V1

fail() { printf 'error: %s\n' "$1" >&2; exit 1; }

usage() {
  cat <<'USAGE'
Usage: scripts/mac-windows-control-streaming-proof-controller.sh [--check | --run | --self-test]

  --check      Validate fixed prerequisites without running the live boundary.
  --run        Execute only the committed harness in fixed --run-relay mode and publish a receipt.
  --self-test  Exercise success and fail-closed behavior in disposable Git/process fixtures.

The controller accepts no repository, executable, endpoint, stream, helper, agent,
tool, or harness argument. It never admits evidence and never activates.
USAGE
}

require_command() { command -v "$1" >/dev/null 2>&1 || fail "required command is unavailable: $1"; }
sha256_file() { /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'; }
sha256_stdin() { /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}'; }
valid_sha() { [[ "$1" =~ ^[0-9a-f]{64}$ ]]; }
valid_oid() { [[ "$1" =~ ^[0-9a-f]{40}$ || "$1" =~ ^[0-9a-f]{64}$ ]]; }
directory_identity() { /usr/bin/stat -f '%d:%i' "$1"; }

git_safe() {
  local root="$1" protocol_policy=never
  shift
  [[ "$fixture_mode" -eq 0 ]] || protocol_policy=always
  /usr/bin/env -i PATH="$SYSTEM_PATH" LC_ALL=C GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null \
    GIT_CONFIG_NOSYSTEM=1 GIT_ATTR_NOSYSTEM=1 GIT_TERMINAL_PROMPT=0 GIT_OPTIONAL_LOCKS=0 \
    "$FIXED_GIT" --no-replace-objects -c core.fsmonitor=false -c core.hooksPath=/dev/null \
      -c core.attributesFile=/dev/null -c protocol.file.allow="$protocol_policy" -C "$root" "$@"
}

validate_fixed_file() {
  local path="$1" digest="$2" owner="$3" mode="$4"
  [[ -f "$path" && ! -L "$path" ]] || fail "fixed tool is not an ordinary file: $path"
  [[ "$(/usr/bin/stat -f '%u:%Lp:%HT' "$path")" == "$owner:$mode:Regular File" ]] \
    || fail "fixed tool identity drifted: $path"
  [[ "$(cd "${path%/*}" && pwd -P)/${path##*/}" == "$path" ]] || fail "fixed tool path is ambiguous: $path"
  [[ "$(sha256_file "$path")" == "$digest" ]] || fail "fixed tool digest drifted: $path"
}

validate_live_tools() {
  [[ "$fixture_mode" -eq 1 ]] && return 0
  validate_fixed_file "$FIXED_GIT" "$FIXED_GIT_SHA256" 0 755
  validate_fixed_file "$FIXED_BASH" "$FIXED_BASH_SHA256" 0 555
  validate_fixed_file "$FIXED_CODESIGN" "$FIXED_CODESIGN_SHA256" 0 755
  validate_fixed_file "$FIXED_SWIFT" "$FIXED_SWIFT_SHA256" 0 755
  validate_fixed_file "$FIXED_SQLITE" "$FIXED_SQLITE_SHA256" 0 755
  validate_fixed_file "$FIXED_NC" "$FIXED_NC_SHA256" 0 555
  validate_fixed_file "$FIXED_TAILSCALE" "$FIXED_TAILSCALE_SHA256" 0 755
  validate_fixed_file "$FIXED_CARGO" "$FIXED_CARGO_SHA256" "$(/usr/bin/id -u)" 755
  [[ "$(/usr/bin/id -un)" == "$FIXED_USER" \
    && -d "$FIXED_HOME" && ! -L "$FIXED_HOME" \
    && "$(cd "$FIXED_HOME" && pwd -P)" == "$FIXED_HOME" \
    && "$(/usr/bin/stat -f '%u:%HT' "$FIXED_HOME")" == "$(/usr/bin/id -u):Directory" \
    && "$((8#$(/usr/bin/stat -f '%Lp' "$FIXED_HOME") & 8#022))" -eq 0 ]] \
    || fail "fixed owner home identity is invalid"
}

validate_target_directory() {
  local root="$1" path="$1/target" mode
  [[ -d "$path" && ! -L "$path" ]] || fail "repository target is not an ordinary directory"
  [[ "$(/usr/bin/stat -f '%u:%HT' "$path")" == "$(/usr/bin/id -u):Directory" ]] \
    || fail "repository target owner or type is invalid"
  mode="$(/usr/bin/stat -f '%Lp' "$path")"
  [[ "$((8#$mode & 8#022))" -eq 0 ]] || fail "repository target is group or world writable"
  [[ "$(cd "$path" && pwd -P)" == "$path" ]] || fail "repository target identity is ambiguous"
}

validate_owner_directory() {
  local path="$1"
  [[ -d "$path" && ! -L "$path" ]] || fail "proof output is not an ordinary directory"
  [[ "$(/usr/bin/stat -f '%u:%Lp:%HT' "$path")" == "$(/usr/bin/id -u):700:Directory" ]] \
    || fail "proof output owner, mode, or type is invalid"
}

validate_output_file() {
  local path="$1"
  [[ ! -e "$path" && ! -L "$path" ]] && return 0
  [[ -f "$path" && ! -L "$path" ]] || fail "refusing unsafe existing proof output"
  [[ "$(/usr/bin/stat -f '%u:%Lp:%l:%HT' "$path")" == "$(/usr/bin/id -u):600:1:Regular File" ]] \
    || fail "existing proof output identity is invalid"
}

prepare_output() {
  local root="$1" target="$1/target" output="$1/$OUTPUT_RELATIVE"
  umask 077
  if [[ -e "$target" || -L "$target" ]]; then validate_target_directory "$root"; else /bin/mkdir -m 700 "$target"; fi
  [[ -e "$output" || -L "$output" ]] || /bin/mkdir -m 700 "$output"
  validate_owner_directory "$output"
  cd "$output"
  [[ "$(pwd -P)" == "$output" ]] || fail "proof output identity is ambiguous"
  prepared_target_identity="$(directory_identity "$target")"
  prepared_output_identity="$(directory_identity .)"
  output_prepared=1
  validate_output_file "$RECEIPT_NAME"
  validate_output_file "$DIGEST_NAME"
  /bin/rm -f -- "$RECEIPT_NAME" "$DIGEST_NAME"
}

revalidate_output() {
  local root="$1" output="$1/$OUTPUT_RELATIVE"
  validate_target_directory "$root"
  validate_owner_directory "$output"
  [[ "$(directory_identity "$root/target")" == "$prepared_target_identity" ]] || fail "target identity changed"
  [[ "$(directory_identity "$output")" == "$prepared_output_identity" ]] || fail "proof output path changed"
  [[ "$(directory_identity .)" == "$prepared_output_identity" && "$(pwd -P)" == "$output" ]] \
    || fail "held proof output changed"
}

capture_source_state() {
  local root="$1" prefix="$2" top branch head origin tree status remote path digest variable
  top="$(git_safe "$root" rev-parse --show-toplevel)"; [[ "$top" == "$root" ]] || fail "controller must run at repository root"
  branch="$(git_safe "$root" symbolic-ref -q HEAD)" || fail "detached HEAD is ineligible"
  [[ "$branch" == refs/heads/main ]] || fail "control-stream proof requires exact main"
  head="$(git_safe "$root" rev-parse --verify 'HEAD^{commit}')"
  origin="$(git_safe "$root" rev-parse --verify 'refs/remotes/origin/main^{commit}')"
  [[ "$head" == "$origin" ]] || fail "HEAD does not equal refs/remotes/origin/main"
  if [[ "$fixture_mode" -eq 0 ]]; then
    remote="$(git_safe "$root" remote get-url origin)"
    [[ "$remote" == https://github.com/malak333/Assemblywright || "$remote" == https://github.com/malak333/Assemblywright.git ]] \
      || fail "origin is not the fixed Assemblywright repository"
  fi
  tree="$(git_safe "$root" rev-parse --verify 'HEAD^{tree}')"
  git_safe "$root" ls-files -v -- | /usr/bin/awk 'substr($0,1,2)!="H "{bad=1} END{exit bad}' \
    || fail "repository index contains hidden tracked-state flags"
  status="$(git_safe "$root" status --porcelain=v1 --untracked-files=all)"
  [[ -z "$status" ]] || fail "control-stream proof requires a clean working tree"
  valid_oid "$head" && valid_oid "$tree" || fail "Git object identity is malformed"
  for path in "$CONTROLLER_PATH" "$HARNESS_PATH"; do
    digest="$(git_safe "$root" show "$head:$path" | sha256_stdin)" || fail "committed proof definition is unavailable: $path"
    valid_sha "$digest" || fail "committed proof definition digest is malformed"
    case "$path" in "$CONTROLLER_PATH") variable=controller_digest;; *) variable=harness_digest;; esac
    eval "${prefix}_${variable}=\$digest"
  done
  eval "${prefix}_branch=\$branch"; eval "${prefix}_head=\$head"; eval "${prefix}_origin=\$origin"
  eval "${prefix}_tree=\$tree"; eval "${prefix}_status=\$status"
}

capture_binary_state() {
  local root="$1" prefix="$2" label relative path identity digest details cdhash team identifier mode
  for label in helper agent; do
    [[ "$label" == helper ]] && relative="$HELPER_RELATIVE" || relative="$AGENT_RELATIVE"
    path="$root/$relative"
    [[ -f "$path" && ! -L "$path" && -x "$path" ]] || fail "$label executable is not an ordinary executable file"
    [[ "$(/usr/bin/stat -f '%u:%l:%HT' "$path")" == "$(/usr/bin/id -u):1:Regular File" ]] \
      || fail "$label executable owner, link, or type is invalid"
    mode="$(/usr/bin/stat -f '%Lp' "$path")"
    [[ "$((8#$mode & 8#022))" -eq 0 ]] || fail "$label executable is group or world writable"
    [[ "$(cd "${path%/*}" && pwd -P)/${path##*/}" == "$path" ]] || fail "$label executable path is ambiguous"
    identity="$(/usr/bin/stat -f '%d:%i:%u:%Lp:%l' "$path")"
    digest="$(sha256_file "$path")"; valid_sha "$digest" || fail "$label digest is malformed"
    if [[ "$fixture_mode" -eq 0 ]]; then
      "$FIXED_CODESIGN" --verify --strict "$path" >/dev/null 2>&1 || fail "$label signature validation failed"
      details="$("$FIXED_CODESIGN" -dv --verbose=4 "$path" 2>&1)"
      cdhash="$(printf '%s\n' "$details" | /usr/bin/sed -n 's/^CDHash=//p' | /usr/bin/head -1)"
      team="$(printf '%s\n' "$details" | /usr/bin/sed -n 's/^TeamIdentifier=//p' | /usr/bin/head -1)"
      identifier="$(printf '%s\n' "$details" | /usr/bin/sed -n 's/^Identifier=//p' | /usr/bin/head -1)"
      [[ "$cdhash" =~ ^[0-9a-f]{40,64}$ && "$identifier" =~ ^[A-Za-z0-9._-]{1,128}$ ]] \
        || fail "$label signature identity is malformed"
      if [[ "$label" == helper ]]; then
        [[ "$team" == "$FIXED_HELPER_TEAM" && "$identifier" == "$FIXED_HELPER_IDENTIFIER" ]] \
          || fail "helper signature is not the fixed owner helper identity"
      else
        [[ "$team" == "not set" || "$team" =~ ^[A-Z0-9]{10}$ ]] || fail "agent signature team is malformed"
      fi
    else
      cdhash="$(printf '%040d' 0)"; team=selftest; identifier="assemblywright.$label.selftest"
    fi
    eval "${prefix}_${label}_identity=\$identity"; eval "${prefix}_${label}_digest=\$digest"
    eval "${prefix}_${label}_cdhash=\$cdhash"; eval "${prefix}_${label}_team=\$team"
    eval "${prefix}_${label}_identifier=\$identifier"
  done
}

clear_live_environment() {
  local name function_name
  while IFS='=' read -r name _; do
    case "$name" in
      PATH|GIT_*|BASH_ENV|ENV|CARGO_*|RUST*|DYLD_*|LD_*|SWIFT_*|SDKROOT|DEVELOPER_DIR|MACOSX_DEPLOYMENT_TARGET|MAKEFLAGS|\
      BASH_FUNC_*|ASSEMBLYWRIGHT_MAC_*|ASSEMBLYWRIGHT_CONTROL_STREAMING_*|ASSEMBLYWRIGHT_FEATURE_CONVEYOR_* )
        unset "$name" 2>/dev/null || true;;
    esac
  done < <(/usr/bin/env)
  while read -r _ _ function_name; do
    unset -f "$function_name"
  done < <(declare -F)
}

terminate_live_group() {
  local count=0
  [[ "${live_pid:-}" =~ ^[1-9][0-9]*$ ]] || return 0
  /bin/kill -TERM -- "-$live_pid" >/dev/null 2>&1 || true
  while /bin/kill -0 -- "-$live_pid" >/dev/null 2>&1; do
    count=$((count + 1))
    if [[ "$count" -ge 100 ]]; then /bin/kill -KILL -- "-$live_pid" >/dev/null 2>&1 || true; break; fi
    /bin/sleep 0.1
  done
  wait "$live_pid" >/dev/null 2>&1 || true
  live_pid=""
}

wait_for_live_group_drain() {
  local count=0
  while [[ "${live_pid:-}" =~ ^[1-9][0-9]*$ ]] && /bin/kill -0 -- "-$live_pid" >/dev/null 2>&1; do
    count=$((count + 1)); [[ "$count" -lt 100 ]] || return 1; /bin/sleep 0.1
  done
}

run_committed_harness() {
  local root="$1" head="$2" transcript="$3" status group_remained=0
  set -m
  (
    set +m
    git_safe "$root" show "$head:$HARNESS_PATH" | (
      cd "$root" || exit 1
      clear_live_environment
      exec /usr/bin/env -i HOME="$FIXED_HOME" USER="$FIXED_USER" LOGNAME="$FIXED_USER" \
        PATH="$LIVE_PATH" LC_ALL=C \
        ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_STDIN_V1="$PROOF_IDENTITY" \
        ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_ROOT="$root" \
        "$FIXED_BASH" -c 'exec -a bash /bin/bash -s -- "$@"' bash --run-relay
    ) 2>&1 | LC_ALL=C /usr/bin/awk -v max_bytes="$MAX_TRANSCRIPT_BYTES" -v max_lines="$MAX_TRANSCRIPT_LINES" -v max_line="$MAX_LINE_BYTES" '
      BEGIN { bytes=0; lines=0 }
      { bytes += length($0)+1; lines++; if (bytes > max_bytes || lines > max_lines || length($0) > max_line) exit 97; print; fflush() }
    ' | /usr/bin/tee "$transcript"
  ) &
  live_pid="$!"
  set +m
  [[ "$live_pid" =~ ^[1-9][0-9]*$ ]] || fail "live process could not be identified"
  set +e; wait "$live_pid"; status="$?"; set -e
  if [[ "$status" -ne 0 ]]; then terminate_live_group; fail "committed control-stream harness failed or exceeded output bounds"; fi
  if ! wait_for_live_group_drain; then group_remained=1; terminate_live_group; else live_pid=""; fi
  [[ "$group_remained" -eq 0 ]] || fail "committed control-stream harness left a live descendant"
}

validate_transcript() {
  local transcript="$1" line bridge_line relay_count bridge_count total terminal_pair terminal_relay terminal_bridge
  local -a fields bridge_fields
  relay_count="$(/usr/bin/grep -c '^assemblywright_mac_windows_event_relay_live_e2e_ok' "$transcript" || true)"
  bridge_count="$(/usr/bin/grep -c '^assemblywright_mac_windows_bridge_live_e2e_ok' "$transcript" || true)"
  [[ "$relay_count" == 1 && "$bridge_count" == 1 ]] \
    || fail "live transcript did not contain one exact relay/bridge terminal pair"
  line="$(/usr/bin/grep '^assemblywright_mac_windows_event_relay_live_e2e_ok' "$transcript")"
  bridge_line="$(/usr/bin/grep '^assemblywright_mac_windows_bridge_live_e2e_ok' "$transcript")"
  terminal_pair="$(/usr/bin/awk 'NF{previous=last; last=$0} END{print previous; print last}' "$transcript")"
  terminal_relay="$(printf '%s\n' "$terminal_pair" | /usr/bin/sed -n '1p')"
  terminal_bridge="$(printf '%s\n' "$terminal_pair" | /usr/bin/sed -n '2p')"
  [[ "$line" == "$terminal_relay" && "$bridge_line" == "$terminal_bridge" ]] \
    || fail "live relay/bridge terminal pair was reordered or followed by extra output"
  IFS=' ' read -r -a fields <<<"$line"
  [[ "${#fields[@]}" -eq 7 && "${fields[0]}" == assemblywright_mac_windows_event_relay_live_e2e_ok ]] \
    || fail "live terminal marker shape drifted"
  [[ "${fields[1]}" == endpoint=*:* && "${#fields[1]}" -le 128 && "${fields[1]}" != *"="*"="* ]] \
    || fail "live endpoint marker is malformed"
  [[ "${fields[2]}" =~ ^stream_id=[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$ ]] \
    || fail "live stream marker is malformed"
  [[ "${fields[3]}" =~ ^sequence_before=[1-9][0-9]{0,18}$ && "${fields[4]}" =~ ^sequence_after=[1-9][0-9]{0,18}$ ]] \
    || fail "live sequence markers are malformed"
  [[ "${fields[3]#sequence_before=}" -lt "${fields[4]#sequence_after=}" ]] || fail "live stream cursor did not advance"
  [[ "${fields[5]}" == app_supervision=verified && "${fields[6]}" == agent_restart=verified ]] \
    || fail "live supervision or restart proof is missing"
  IFS=' ' read -r -a bridge_fields <<<"$bridge_line"
  [[ "${#bridge_fields[@]}" -eq 9 && "${bridge_fields[0]}" == assemblywright_mac_windows_bridge_live_e2e_ok ]] \
    || fail "live bridge terminal marker shape drifted"
  [[ "${bridge_fields[1]}" == "${fields[1]}" \
    && "${bridge_fields[2]}" =~ ^connection_epoch=[1-9][0-9]{0,18}$ \
    && "${bridge_fields[3]}" =~ ^monitor_epoch=[1-9][0-9]{0,18}$ \
    && "${bridge_fields[4]}" == monitor_samples=2 \
    && "${bridge_fields[5]}" =~ ^reconnect_epoch_before=[1-9][0-9]{0,18}$ \
    && "${bridge_fields[6]}" =~ ^reconnect_epoch_after=[1-9][0-9]{0,18}$ \
    && "${bridge_fields[7]}" == app_supervision=verified \
    && "${bridge_fields[8]}" =~ ^team=[A-Z0-9]{10}$ ]] \
    || fail "live bridge terminal bindings are malformed"
  [[ "${bridge_fields[5]#reconnect_epoch_before=}" -lt "${bridge_fields[6]#reconnect_epoch_after=}" ]] \
    || fail "live bridge reconnect epoch did not advance"
  total="$(/usr/bin/wc -c <"$transcript" | /usr/bin/tr -d '[:space:]')"
  [[ "$total" -gt 0 && "$total" -le "$MAX_TRANSCRIPT_BYTES" ]] || fail "live transcript bound is invalid"
}

write_receipt() {
  local head="$1" tree="$2" controller_digest="$3" harness_digest="$4" helper_digest="$5" helper_cdhash="$6"
  local helper_team="$7" helper_identifier="$8" agent_digest="$9" agent_cdhash="${10}" agent_team="${11}"
  local agent_identifier="${12}" transcript_digest="${13}" observed="${14}"
  local receipt_tmp digest_tmp receipt_digest bytes
  receipt_tmp="$(/usr/bin/mktemp .control-stream-proof.json.XXXXXX)"
  digest_tmp="$(/usr/bin/mktemp .control-stream-proof.sha256.XXXXXX)" || { /bin/rm -f "$receipt_tmp"; fail "could not allocate digest temporary"; }
  /bin/chmod 600 "$receipt_tmp" "$digest_tmp"
  printf '{"schema":"%s","category":"%s","origin":"%s","head_commit":"%s","tree_id":"%s","controller_definition_sha256":"%s","mac_live_harness_definition_sha256":"%s","signed_helper_executable_sha256":"%s","signed_helper_cdhash":"%s","signed_helper_team":"%s","signed_helper_identifier":"%s","signed_agent_executable_sha256":"%s","signed_agent_cdhash":"%s","signed_agent_team":"%s","signed_agent_identifier":"%s","event_stream_transcript_sha256":"%s","proof_identity":"%s","observed_at_ms":%s,"status":"passed","proof_boundary":"%s"}\n' \
    "$SCHEMA" "$CATEGORY" "$ORIGIN" "$head" "$tree" "$controller_digest" "$harness_digest" \
    "$helper_digest" "$helper_cdhash" "$helper_team" "$helper_identifier" "$agent_digest" "$agent_cdhash" "$agent_team" "$agent_identifier" \
    "$transcript_digest" "$PROOF_IDENTITY" "$observed" "$PROOF_BOUNDARY" >"$receipt_tmp"
  bytes="$(/usr/bin/wc -c <"$receipt_tmp" | /usr/bin/tr -d '[:space:]')"; [[ "$bytes" -le 4096 ]] \
    || { /bin/rm -f "$receipt_tmp" "$digest_tmp"; fail "receipt exceeded fixed bound"; }
  receipt_digest="$(sha256_file "$receipt_tmp")"; valid_sha "$receipt_digest" || fail "receipt digest is malformed"
  printf '%s\n' "$receipt_digest" >"$digest_tmp"
  /bin/mv -f -- "$digest_tmp" "$DIGEST_NAME" || { /bin/rm -f "$receipt_tmp" "$digest_tmp" "$DIGEST_NAME"; fail "could not publish digest"; }
  /bin/mv -f -- "$receipt_tmp" "$RECEIPT_NAME" || { /bin/rm -f "$receipt_tmp" "$RECEIPT_NAME" "$DIGEST_NAME"; fail "could not publish receipt"; }
}

run_controller() (
  local root="$(cd "$1" && pwd -P)" output_prepared=0 published=0 prepared_target_identity="" prepared_output_identity=""
  local before_branch before_head before_origin before_tree before_status before_controller_digest before_harness_digest
  local after_branch after_head after_origin after_tree after_status after_controller_digest after_harness_digest
  local before_helper_identity before_helper_digest before_helper_cdhash before_helper_team before_helper_identifier
  local before_agent_identity before_agent_digest before_agent_cdhash before_agent_team before_agent_identifier
  local after_helper_identity after_helper_digest after_helper_cdhash after_helper_team after_helper_identifier
  local after_agent_identity after_agent_digest after_agent_cdhash after_agent_team after_agent_identifier
  local transcript transcript_digest observed published_digest actual_digest
  cleanup() {
    terminate_live_group
    if [[ "$output_prepared" -eq 1 ]]; then
      /bin/rm -f -- .control-stream-transcript.* .control-stream-proof.json.* .control-stream-proof.sha256.* 2>/dev/null || true
      [[ "$published" -eq 1 ]] || /bin/rm -f -- "$RECEIPT_NAME" "$DIGEST_NAME" 2>/dev/null || true
    fi
  }
  signal_exit() { local code="$1"; trap - HUP INT TERM; terminate_live_group; exit "$code"; }
  trap cleanup EXIT
  trap 'signal_exit 129' HUP; trap 'signal_exit 130' INT; trap 'signal_exit 143' TERM

  prepare_output "$root"
  validate_live_tools
  capture_source_state "$root" before
  capture_binary_state "$root" before
  transcript="$(/usr/bin/mktemp .control-stream-transcript.XXXXXX)"
  /bin/chmod 600 "$transcript"
  run_committed_harness "$root" "$before_head" "$transcript"
  validate_transcript "$transcript"
  transcript_digest="$(sha256_file "$transcript")"; valid_sha "$transcript_digest" || fail "transcript digest is malformed"

  capture_source_state "$root" after
  capture_binary_state "$root" after
  [[ "$after_branch:$after_head:$after_origin:$after_tree:$after_status" == "$before_branch:$before_head:$before_origin:$before_tree:$before_status" ]] \
    || fail "repository state drifted during live proof"
  [[ "$after_controller_digest:$after_harness_digest" == "$before_controller_digest:$before_harness_digest" ]] \
    || fail "committed proof definitions drifted during live proof"
  [[ "$after_helper_identity:$after_helper_digest:$after_helper_cdhash:$after_helper_team:$after_helper_identifier" == "$before_helper_identity:$before_helper_digest:$before_helper_cdhash:$before_helper_team:$before_helper_identifier" ]] \
    || fail "signed helper identity drifted during live proof"
  [[ "$after_agent_identity:$after_agent_digest:$after_agent_cdhash:$after_agent_team:$after_agent_identifier" == "$before_agent_identity:$before_agent_digest:$before_agent_cdhash:$before_agent_team:$before_agent_identifier" ]] \
    || fail "signed agent identity drifted during live proof"
  revalidate_output "$root"
  observed="$(/bin/date -u '+%s')000"; [[ "$observed" =~ ^[0-9]{13}$ ]] || fail "observed time is malformed"
  write_receipt "$before_head" "$before_tree" "$before_controller_digest" "$before_harness_digest" \
    "$before_helper_digest" "$before_helper_cdhash" "$before_helper_team" "$before_helper_identifier" \
    "$before_agent_digest" "$before_agent_cdhash" "$before_agent_team" "$before_agent_identifier" \
    "$transcript_digest" "$observed"
  /bin/rm -f -- "$transcript" || fail "could not delete private transcript"
  revalidate_output "$root"
  validate_output_file "$RECEIPT_NAME"; validate_output_file "$DIGEST_NAME"
  published_digest="$(/usr/bin/tr -d '[:space:]' <"$DIGEST_NAME")"; actual_digest="$(sha256_file "$RECEIPT_NAME")"
  [[ "$published_digest" == "$actual_digest" ]] && valid_sha "$published_digest" || fail "published raw digest does not match receipt"
  published=1
  trap - EXIT HUP INT TERM
  printf 'Assemblywright Mac/Windows control-streaming live proof controller: passed\n'
  printf 'Receipt: %s/%s\n' "$OUTPUT_RELATIVE" "$RECEIPT_NAME"
  printf 'Receipt SHA-256: %s/%s\n' "$OUTPUT_RELATIVE" "$DIGEST_NAME"
  printf 'Proof boundary: %s\n' "$PROOF_BOUNDARY"
)

check_controller() {
  local command_name
  for command_name in awk bash chmod codesign date env git grep head id mkdir mktemp mv rm sed shasum stat tee tr wc; do require_command "$command_name"; done
  [[ -f "$ROOT_DIR/$HARNESS_PATH" && -f "$ROOT_DIR/$CONTROLLER_PATH" ]] || fail "fixed controller definitions are unavailable"
  [[ "$FIXED_GIT" == /usr/bin/git && -f "$FIXED_GIT" && ! -L "$FIXED_GIT" ]] || fail "fixed Git contract is unavailable"
  [[ "$FIXED_BASH" == /bin/bash && -f "$FIXED_BASH" && ! -L "$FIXED_BASH" ]] || fail "fixed Bash contract is unavailable"
  git_safe "$ROOT_DIR" check-ignore -q "$OUTPUT_RELATIVE/$RECEIPT_NAME" || fail "proof output is not ignored"
  /usr/bin/grep -Fq 'run ./scripts/mac-windows-control-streaming-proof-controller.sh --check' "$ROOT_DIR/scripts/release-local.sh" \
    || fail "release-local omits controller check"
  /usr/bin/grep -Fq 'run ./scripts/mac-windows-control-streaming-proof-controller.sh --self-test' "$ROOT_DIR/scripts/release-local.sh" \
    || fail "release-local omits controller self-test"
  ! /usr/bin/grep -Fq 'mac-windows-control-streaming-proof-controller.sh --run' "$ROOT_DIR/scripts/release-local.sh" \
    || fail "release-local must never run live control-stream proof"
  /usr/bin/grep -Fq -- '--run-relay' "$ROOT_DIR/$HARNESS_PATH" || fail "committed harness omits fixed relay mode"
  /usr/bin/grep -Fq 'ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_STDIN_V1' "$ROOT_DIR/$HARNESS_PATH" \
    || fail "committed harness omits fixed control-stream stdin identity"
  /usr/bin/grep -Fq 'assemblywright_mac_windows_event_relay_live_e2e_ok' "$ROOT_DIR/$HARNESS_PATH" \
    || fail "committed harness omits terminal stream marker"
  printf 'Assemblywright Mac/Windows control-streaming proof controller check: ok\n'
  printf 'Proof boundary: static prerequisites only; no live relay ran and no receipt was created.\n'
}

write_fixture_files() {
  local fixture="$1" body="$2"
  /bin/mkdir -p "$fixture/scripts" "$fixture/${HELPER_RELATIVE%/*}" "$fixture/${AGENT_RELATIVE%/*}"
  printf '%s\n' '#!/bin/bash' 'set -euo pipefail' \
    'internal_marker="${ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_STDIN_V1:-}"' \
    'internal_root="${ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_ROOT:-}"' \
    'unset ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_STDIN_V1' \
    'unset ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_ROOT' \
    '[[ -z "${BASH_SOURCE[0]-}" && "$0" == bash && "$#" -eq 1 && "${1:-}" == --run-relay ]] || exit 91' \
    '[[ "$internal_marker" == assemblywright.mac-windows-control-event-streaming-live.v1 ]] || exit 92' \
    '[[ "$internal_root" == /* && -d "$internal_root" && ! -L "$internal_root" ]] || exit 93' \
    '[[ "$(cd "$internal_root" && pwd -P)" == "$internal_root" ]] || exit 94' \
    "$body" >"$fixture/$HARNESS_PATH"
  printf '#!/bin/bash\nexit 0\n' >"$fixture/$CONTROLLER_PATH"
  printf '#!/bin/bash\nexit 0\n' >"$fixture/$HELPER_RELATIVE"
  printf '#!/bin/bash\nexit 0\n' >"$fixture/$AGENT_RELATIVE"
  /bin/chmod 700 "$fixture/$HARNESS_PATH" "$fixture/$CONTROLLER_PATH" "$fixture/$HELPER_RELATIVE" "$fixture/$AGENT_RELATIVE"
}

fixture_terminal() {
  printf '%s\n' \
    "printf '%s\\n' 'assemblywright_mac_windows_event_relay_live_e2e_ok endpoint=100.64.23.14:7792 stream_id=11111111-1111-4111-8111-111111111111 sequence_before=13 sequence_after=15 app_supervision=verified agent_restart=verified'" \
    "printf '%s\\n' 'assemblywright_mac_windows_bridge_live_e2e_ok endpoint=100.64.23.14:7792 connection_epoch=21 monitor_epoch=22 monitor_samples=2 reconnect_epoch_before=23 reconnect_epoch_after=24 app_supervision=verified team=H686S3N4V9'"
}

initialize_fixture() {
  local fixture="$1" body="$2"
  /bin/mkdir -p "$fixture"; git_safe "$fixture" init -q; git_safe "$fixture" checkout -q -b main
  git_safe "$fixture" config user.name 'Assemblywright Control Stream Self Test'
  git_safe "$fixture" config user.email 'control-stream-self-test@invalid.example'
  printf 'target/\napps/mac/.build/\n' >"$fixture/.gitignore"; printf 'fixture\n' >"$fixture/README.md"
  write_fixture_files "$fixture" "$body"
  git_safe "$fixture" add .; git_safe "$fixture" commit -q -m fixture; git_safe "$fixture" update-ref refs/remotes/origin/main HEAD
}

assert_no_output() {
  local fixture="$1" output
  output="$fixture/$OUTPUT_RELATIVE"
  [[ ! -e "$output/$RECEIPT_NAME" && ! -e "$output/$DIGEST_NAME" ]] || fail "rejected fixture retained proof output"
}

expect_failure() { local fixture="$1"; if run_controller "$fixture" >/dev/null 2>&1; then fail "self-test expected rejection"; fi; assert_no_output "$fixture"; }

self_test_controller() {
  local scratch success receipt digest actual expected head tree controller_digest harness_digest transcript_digest variant variant_digest
  local dirty wrong stale hidden empty malformed mismatched_endpoint nonadvancing_sequence nonadvancing_reconnect
  local duplicate reordered oversize path_leak env_hardening source_drift binary_drift persistent
  local cancellation ready survived pid status count hostile external
  local -a transcript_leftovers
  scratch="$(/usr/bin/mktemp -d -t assemblywright-control-stream-proof)"; /bin/chmod 700 "$scratch"
  trap "/bin/rm -rf -- '$scratch'" RETURN
  "$ROOT_DIR/$CONTROLLER_PATH" >/dev/null || fail "default mode did not run check"
  if "$ROOT_DIR/$CONTROLLER_PATH" --unknown >/dev/null 2>&1; then fail "unknown mode was accepted"; fi
  if "$ROOT_DIR/$CONTROLLER_PATH" --check extra >/dev/null 2>&1; then fail "extra CLI argument was accepted"; fi
  fixture_mode=1

  success="$scratch/success"; initialize_fixture "$success" "$(fixture_terminal)"; run_controller "$success" >/dev/null
  receipt="$success/$OUTPUT_RELATIVE/$RECEIPT_NAME"; digest="$success/$OUTPUT_RELATIVE/$DIGEST_NAME"
  [[ "$(/usr/bin/stat -f '%Lp' "$receipt")" == 600 && "$(/usr/bin/stat -f '%Lp' "$digest")" == 600 ]] || fail "receipt modes drifted"
  expected="$(/usr/bin/tr -d '[:space:]' <"$digest")"; actual="$(sha256_file "$receipt")"; [[ "$expected" == "$actual" ]] || fail "raw receipt digest mismatch"
  head="$(git_safe "$success" rev-parse HEAD)"; tree="$(git_safe "$success" rev-parse 'HEAD^{tree}')"
  controller_digest="$(git_safe "$success" show "$head:$CONTROLLER_PATH" | sha256_stdin)"
  harness_digest="$(git_safe "$success" show "$head:$HARNESS_PATH" | sha256_stdin)"
  /usr/bin/grep -Fq "\"head_commit\":\"$head\"" "$receipt" || fail "receipt omitted exact HEAD"
  /usr/bin/grep -Fq "\"tree_id\":\"$tree\"" "$receipt" || fail "receipt omitted exact tree"
  /usr/bin/grep -Fq "\"controller_definition_sha256\":\"$controller_digest\"" "$receipt" || fail "receipt omitted controller digest"
  /usr/bin/grep -Fq "\"mac_live_harness_definition_sha256\":\"$harness_digest\"" "$receipt" || fail "receipt omitted harness digest"
  /usr/bin/grep -Fq "\"category\":\"$CATEGORY\"" "$receipt" || fail "receipt category drifted"
  /usr/bin/grep -Fq "\"origin\":\"$ORIGIN\"" "$receipt" || fail "receipt origin drifted"
  if /usr/bin/grep -Eq 'endpoint|stream_id|100\.64\.|/Users/|/private/|/tmp/|github\.com' "$receipt"; then fail "receipt leaked private/path data"; fi
  transcript_digest="$(/usr/bin/sed -E 's/.*"event_stream_transcript_sha256":"([0-9a-f]{64})".*/\1/' "$receipt")"; valid_sha "$transcript_digest" || fail "transcript digest missing"
  transcript_leftovers=( "$success/$OUTPUT_RELATIVE"/.control-stream-transcript.* )
  [[ ! -e "${transcript_leftovers[0]}" ]] || fail "private transcript was retained after publication"
  variant="$scratch/variant"; initialize_fixture "$variant" "printf 'private variation\\n'"$'\n'"$(fixture_terminal)"; run_controller "$variant" >/dev/null
  variant_digest="$(/usr/bin/sed -E 's/.*"event_stream_transcript_sha256":"([0-9a-f]{64})".*/\1/' "$variant/$OUTPUT_RELATIVE/$RECEIPT_NAME")"
  [[ "$variant_digest" != "$transcript_digest" ]] || fail "complete transcript was not digest-bound"

  printf dirty >"$success/dirty"; expect_failure "$success"
  dirty="$scratch/dirty"; initialize_fixture "$dirty" "$(fixture_terminal)"; printf dirty >"$dirty/untracked"; expect_failure "$dirty"
  wrong="$scratch/wrong"; initialize_fixture "$wrong" "$(fixture_terminal)"; git_safe "$wrong" checkout -q -b feature; expect_failure "$wrong"
  stale="$scratch/stale"; initialize_fixture "$stale" "$(fixture_terminal)"; git_safe "$stale" update-ref refs/remotes/origin/main HEAD~0; git_safe "$stale" commit --allow-empty -q -m stale; expect_failure "$stale"
  hidden="$scratch/hidden"; initialize_fixture "$hidden" "$(fixture_terminal)"; git_safe "$hidden" update-index --assume-unchanged "$HARNESS_PATH"; printf '#!/bin/bash\nexit 9\n' >"$hidden/$HARNESS_PATH"; expect_failure "$hidden"
  empty="$scratch/empty"; initialize_fixture "$empty" ':'; expect_failure "$empty"
  malformed="$scratch/malformed"; initialize_fixture "$malformed" "printf '%s\\n' 'assemblywright_mac_windows_event_relay_live_e2e_ok malformed=true'"; expect_failure "$malformed"
  mismatched_endpoint="$scratch/mismatched-endpoint"; initialize_fixture "$mismatched_endpoint" \
    "printf '%s\\n' 'assemblywright_mac_windows_event_relay_live_e2e_ok endpoint=100.64.23.14:7792 stream_id=11111111-1111-4111-8111-111111111111 sequence_before=13 sequence_after=15 app_supervision=verified agent_restart=verified'"$'\n'\
"printf '%s\\n' 'assemblywright_mac_windows_bridge_live_e2e_ok endpoint=100.64.23.15:7792 connection_epoch=21 monitor_epoch=22 monitor_samples=2 reconnect_epoch_before=23 reconnect_epoch_after=24 app_supervision=verified team=H686S3N4V9'"; expect_failure "$mismatched_endpoint"
  nonadvancing_sequence="$scratch/nonadvancing-sequence"; initialize_fixture "$nonadvancing_sequence" \
    "printf '%s\\n' 'assemblywright_mac_windows_event_relay_live_e2e_ok endpoint=100.64.23.14:7792 stream_id=11111111-1111-4111-8111-111111111111 sequence_before=15 sequence_after=15 app_supervision=verified agent_restart=verified'"$'\n'\
"printf '%s\\n' 'assemblywright_mac_windows_bridge_live_e2e_ok endpoint=100.64.23.14:7792 connection_epoch=21 monitor_epoch=22 monitor_samples=2 reconnect_epoch_before=23 reconnect_epoch_after=24 app_supervision=verified team=H686S3N4V9'"; expect_failure "$nonadvancing_sequence"
  nonadvancing_reconnect="$scratch/nonadvancing-reconnect"; initialize_fixture "$nonadvancing_reconnect" \
    "printf '%s\\n' 'assemblywright_mac_windows_event_relay_live_e2e_ok endpoint=100.64.23.14:7792 stream_id=11111111-1111-4111-8111-111111111111 sequence_before=13 sequence_after=15 app_supervision=verified agent_restart=verified'"$'\n'\
"printf '%s\\n' 'assemblywright_mac_windows_bridge_live_e2e_ok endpoint=100.64.23.14:7792 connection_epoch=21 monitor_epoch=22 monitor_samples=2 reconnect_epoch_before=24 reconnect_epoch_after=24 app_supervision=verified team=H686S3N4V9'"; expect_failure "$nonadvancing_reconnect"
  duplicate="$scratch/duplicate"; initialize_fixture "$duplicate" "$(fixture_terminal)"$'\n'"$(fixture_terminal)"; expect_failure "$duplicate"
  reordered="$scratch/reordered"; initialize_fixture "$reordered" "$(fixture_terminal)"$'\n'"printf 'late output\\n'"; expect_failure "$reordered"
  oversize="$scratch/oversize"; initialize_fixture "$oversize" "awk 'BEGIN { for(i=0;i<1100000;i++) printf \"x\"; print \"\" }'"; expect_failure "$oversize"
  path_leak="$scratch/path-leak"; initialize_fixture "$path_leak" "printf '/Users/owner/private endpoint detail\\n'"$'\n'"$(fixture_terminal)"; run_controller "$path_leak" >/dev/null
  ! /usr/bin/grep -Eq '/Users/|endpoint|stream_id' "$path_leak/$OUTPUT_RELATIVE/$RECEIPT_NAME" || fail "receipt retained hostile private output"
  env_hardening="$scratch/env"; initialize_fixture "$env_hardening" '[[ "$PATH" == "'"$LIVE_PATH"'" && "${HOME:-}" == "'"$FIXED_HOME"'" && "$(type -t swift)" == file && -z "${ASSEMBLYWRIGHT_MAC_BRIDGE_BIN:-}${ASSEMBLYWRIGHT_MAC_AGENT_BIN:-}${ASSEMBLYWRIGHT_TAILSCALE_BIN:-}${ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_DIR:-}${GIT_DIR:-}" ]]'$'\n'"$(fixture_terminal)"
  swift() { return 97; }
  export -f swift
  PATH="$scratch/evil:$PATH" GIT_DIR="$scratch/git" ASSEMBLYWRIGHT_MAC_BRIDGE_BIN=x ASSEMBLYWRIGHT_MAC_AGENT_BIN=y ASSEMBLYWRIGHT_TAILSCALE_BIN=z ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_DIR=q run_controller "$env_hardening" >/dev/null
  unset -f swift
  source_drift="$scratch/source-drift"; initialize_fixture "$source_drift" "$(fixture_terminal)"$'\n'"printf drift >>README.md"; expect_failure "$source_drift"
  binary_drift="$scratch/binary-drift"; initialize_fixture "$binary_drift" "$(fixture_terminal)"$'\n'"printf drift >>'$binary_drift/$AGENT_RELATIVE'"; expect_failure "$binary_drift"
  persistent="$scratch/persistent"; initialize_fixture "$persistent" "$(fixture_terminal)"$'\n'"(trap '' TERM; while :; do sleep 1; done) >/dev/null 2>&1 &"; expect_failure "$persistent"

  cancellation="$scratch/cancellation"; ready="$scratch/ready"; survived="$scratch/survived"
  initialize_fixture "$cancellation" "printf ready >'$ready'; (sleep 2; printf survived >'$survived') & wait"
  set -m; run_controller "$cancellation" >/dev/null 2>&1 & pid="$!"; set +m
  count=0; while [[ ! -e "$ready" ]]; do count=$((count+1)); [[ "$count" -lt 50 ]] || fail "cancellation fixture did not start"; /bin/sleep 0.1; done
  /bin/kill -TERM -- "-$pid"; set +e; wait "$pid" >/dev/null 2>&1; status="$?"; set -e
  [[ "$status" -eq 143 ]] || fail "cancellation returned wrong status"
  /bin/sleep 3; [[ ! -e "$survived" ]] || fail "cancellation left a descendant"; assert_no_output "$cancellation"

  hostile="$scratch/hostile"; external="$scratch/external"; initialize_fixture "$hostile" "$(fixture_terminal)"; /bin/mkdir -m 700 "$external"; printf preserve >"$external/sentinel"; /bin/mv "$hostile/target" "$hostile/target-real"; ln -s "$external" "$hostile/target"; expect_failure "$hostile"
  [[ "$(<"$external/sentinel")" == preserve ]] || fail "hostile output path changed external state"
  printf 'Assemblywright Mac/Windows control-streaming proof controller self-test: ok\n'
  printf 'Proof boundary: disposable fixtures cover strict CLI, success, raw digest, permissions, complete-transcript hashing/deletion, path/endpoint/stream redaction, stale invalidation, dirty/wrong/stale/hidden Git, empty/malformed/duplicate/reordered/oversized/hostile output, marker endpoint/cursor binding, variable and exported-function environment clearing, source/binary drift, surviving descendants, cancellation cleanup, and hostile output paths.\n'
}

MODE="${1:---check}"
[[ "$#" -le 1 ]] || { usage >&2; fail "controller accepts no extra arguments"; }
case "$MODE" in
  --check) check_controller;;
  --run) run_controller "$ROOT_DIR";;
  --self-test) self_test_controller;;
  --help|-h) usage;;
  *) usage >&2; fail "unknown mode: $MODE";;
esac
