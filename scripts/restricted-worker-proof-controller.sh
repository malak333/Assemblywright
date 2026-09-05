#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
OUTPUT_RELATIVE="target/restricted-worker-live-proof"
RECEIPT_NAME="restricted-worker-live-proof.json"
DIGEST_NAME="restricted-worker-live-proof.sha256"
SCHEMA="assemblywright.restricted-worker-live-proof.v1"
CATEGORY="restricted_worker_live"
ORIGIN="restricted_worker_proof_controller"
PROOF_IDENTITY="assemblywright.restricted-worker-live.v1"
MAC_HARNESS_PATH="scripts/mac-windows-bridge-live-e2e.sh"
WINDOWS_CONTROL_PATH="scripts/windows-local-coding-live-control.ps1"
INTERNAL_STDIN_MARKER_NAME="ASSEMBLYWRIGHT_RESTRICTED_WORKER_INTERNAL_STDIN_V1"
INTERNAL_ROOT_MARKER_NAME="ASSEMBLYWRIGHT_RESTRICTED_WORKER_INTERNAL_ROOT"
INTERNAL_RECEIPT_FD_NAME="ASSEMBLYWRIGHT_RESTRICTED_WORKER_RECEIPT_FD"
PROOF_BOUNDARY="One owner-supervised signed Swift relay and real Rust agent completed the exact protocol-v5 snapshot-bound restricted-worker attempt against the schema-v19 Windows master, including Windows artifact validation, isolated candidate verification, retained-pair observation, cancellation, abandonment, and cleanup; this same-owner live proof is not a host sandbox or OS-wide egress claim and is not activation admission, review-provider, GitHub-publication, restart-recovery, Mac/Windows-control-streaming, notarization, clean-profile, or production-readiness proof."
unset ASSEMBLYWRIGHT_RESTRICTED_WORKER_INTERNAL_STDIN_V1
unset ASSEMBLYWRIGHT_RESTRICTED_WORKER_INTERNAL_ROOT
unset ASSEMBLYWRIGHT_RESTRICTED_WORKER_RECEIPT_FD
receipt_terminal_state=""
receipt_read_error=""

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
Usage: scripts/restricted-worker-proof-controller.sh [--check | --run | --self-test]

  --check      Validate the fixed controller prerequisites without running live work.
  --run        Run only the exact committed restricted-worker Mac/Windows live harness,
               then atomically write the fixed receipt after complete cleanup.
  --self-test  Exercise success and fail-closed behavior in disposable repositories.

The live harness prints fixed Windows-local actions and reads their sanitized JSON
receipts from this command's stdin. The controller accepts no repository, remote,
worker, executable, work packet, or alternate harness argument. It never admits
activation evidence and never activates the Feature Conveyor.
USAGE
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is unavailable: $1"
}

git_safe() {
  local root="$1"
  shift
  env -i \
    PATH="$PATH" \
    LC_ALL=C \
    GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_SYSTEM=/dev/null \
    GIT_CONFIG_NOSYSTEM=1 \
    GIT_ATTR_NOSYSTEM=1 \
    GIT_TERMINAL_PROMPT=0 \
    GIT_OPTIONAL_LOCKS=0 \
    git --no-replace-objects \
      -c core.fsmonitor=false \
      -c core.hooksPath=/dev/null \
      -c core.attributesFile=/dev/null \
      -c protocol.file.allow=never \
      -C "$root" "$@"
}

sha256_file() {
  shasum -a 256 "$1" | awk '{print $1}'
}

sha256_stdin() {
  shasum -a 256 | awk '{print $1}'
}

validate_hash() {
  [[ "$1" =~ ^[0-9a-f]{40}$ || "$1" =~ ^[0-9a-f]{64}$ ]]
}

validate_sha256() {
  [[ "$1" =~ ^[0-9a-f]{64}$ ]]
}

validate_uuid() {
  [[ "$1" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ ]]
}

restore_receipt_terminal() {
  local saved_state="${receipt_terminal_state:-}"
  [[ -n "$saved_state" ]] || return 0
  stty "$saved_state" <&0 || return 1
  receipt_terminal_state=""
}

read_live_receipt() {
  local destination="$1"
  local ready_marker="${2:-}"
  local LC_ALL=C
  local line="" character="" read_status=0 oversized=0 deadline remaining
  receipt_terminal_state=""
  receipt_read_error=""
  if [[ -t 0 ]]; then
    if ! receipt_terminal_state="$(stty -g <&0)"; then
      receipt_read_error="terminal_state_unavailable"
      return 1
    fi
    if ! stty -echo -icanon min 1 time 0 <&0; then
      receipt_read_error="terminal_mode_unavailable"
      restore_receipt_terminal || true
      return 1
    fi
  fi
  [[ -z "$ready_marker" ]] || printf '%s\n' "$ready_marker"
  deadline=$((SECONDS + 600))
  while :; do
    remaining=$((deadline - SECONDS))
    if [[ "$remaining" -le 0 ]] || ! IFS= read -r -t "$remaining" -n 1 character; then
      receipt_read_error="incomplete"
      read_status=1
      break
    fi
    [[ -n "$character" ]] || break
    if [[ "${#line}" -lt 8192 ]]; then
      line+="$character"
    else
      oversized=1
    fi
  done
  if [[ "$oversized" -eq 1 ]]; then
    receipt_read_error="oversized"
    read_status=1
  fi
  if ! restore_receipt_terminal; then
    receipt_read_error="terminal_restore_failed"
    return 1
  fi
  printf -v "$destination" '%s' "$line"
  return "$read_status"
}

validate_owner_directory() {
  local directory="$1"
  local owner mode kind
  [[ -d "$directory" && ! -L "$directory" ]] \
    || fail "proof output directory is not an ordinary directory"
  owner="$(stat -f '%u' "$directory")" \
    || fail "could not inspect proof output directory owner"
  mode="$(stat -f '%Lp' "$directory")" \
    || fail "could not inspect proof output directory mode"
  kind="$(stat -f '%HT' "$directory")" \
    || fail "could not inspect proof output directory type"
  [[ "$owner" == "$(id -u)" ]] || fail "proof output directory is not owner-matched"
  [[ "$mode" == "700" ]] || fail "proof output directory must have mode 0700"
  [[ "$kind" == "Directory" ]] || fail "proof output directory has the wrong type"
}

validate_target_directory() {
  local root="$1"
  local directory="$root/target"
  local owner mode kind canonical
  [[ -d "$directory" && ! -L "$directory" ]] \
    || fail "repository target is not an ordinary directory"
  owner="$(stat -f '%u' "$directory")" \
    || fail "could not inspect repository target owner"
  mode="$(stat -f '%Lp' "$directory")" \
    || fail "could not inspect repository target mode"
  kind="$(stat -f '%HT' "$directory")" \
    || fail "could not inspect repository target type"
  canonical="$(cd "$directory" && pwd -P)" \
    || fail "repository target could not be resolved"
  [[ "$owner" == "$(id -u)" ]] || fail "repository target is not owner-matched"
  [[ "$mode" =~ ^[0-7][0145][0145]$ ]] \
    || fail "repository target must not be group/world writable"
  [[ "$kind" == "Directory" ]] || fail "repository target has the wrong type"
  [[ "$canonical" == "$directory" ]] || fail "repository target identity is ambiguous"
}

directory_identity() {
  stat -f '%d:%i' "$1"
}

validate_removable_output() {
  local path="$1"
  local owner mode links kind
  [[ ! -e "$path" && ! -L "$path" ]] && return 0
  [[ -f "$path" && ! -L "$path" ]] || fail "refusing unsafe existing proof output"
  owner="$(stat -f '%u' "$path")" || fail "could not inspect existing proof owner"
  mode="$(stat -f '%Lp' "$path")" || fail "could not inspect existing proof mode"
  links="$(stat -f '%l' "$path")" || fail "could not inspect existing proof link count"
  kind="$(stat -f '%HT' "$path")" || fail "could not inspect existing proof type"
  [[ "$owner" == "$(id -u)" ]] || fail "existing proof output is not owner-matched"
  [[ "$mode" == "600" ]] || fail "existing proof output must have mode 0600"
  [[ "$links" == "1" ]] || fail "existing proof output must have one link"
  [[ "$kind" == "Regular File" ]] || fail "existing proof output has the wrong type"
}

prepare_output_directory() {
  local root="$1"
  local target_directory="$root/target"
  local output_directory="$root/$OUTPUT_RELATIVE"
  umask 077
  if [[ -e "$target_directory" || -L "$target_directory" ]]; then
    validate_target_directory "$root"
  else
    mkdir -m 700 "$target_directory" || fail "could not create repository target directory"
    validate_target_directory "$root"
  fi
  if [[ ! -e "$output_directory" ]]; then
    mkdir -m 700 "$output_directory" || fail "could not create proof output directory"
  fi
  validate_owner_directory "$output_directory"
  cd "$output_directory" || fail "could not hold proof output directory"
  [[ "$(pwd -P)" == "$output_directory" ]] \
    || fail "proof output directory identity is ambiguous"
  prepared_target_identity="$(directory_identity "$target_directory")" \
    || fail "could not capture repository target identity"
  prepared_output_identity="$(directory_identity .)" \
    || fail "could not capture proof output identity"
  [[ "$(directory_identity "$output_directory")" == "$prepared_output_identity" ]] \
    || fail "proof output path changed during preparation"
  output_prepared=1
  validate_removable_output "$RECEIPT_NAME"
  validate_removable_output "$DIGEST_NAME"
  rm -f -- "$RECEIPT_NAME" "$DIGEST_NAME" \
    || fail "could not invalidate the prior fixed proof pair"
}

revalidate_output_identity() {
  local root="$1"
  local target_directory="$root/target"
  local output_directory="$root/$OUTPUT_RELATIVE"
  validate_target_directory "$root"
  validate_owner_directory "$output_directory"
  [[ "$(directory_identity "$target_directory")" == "$prepared_target_identity" ]] \
    || fail "repository target identity changed while live proof ran"
  [[ "$(directory_identity "$output_directory")" == "$prepared_output_identity" ]] \
    || fail "proof output path identity changed while live proof ran"
  [[ "$(directory_identity .)" == "$prepared_output_identity" ]] \
    || fail "held proof output identity changed while live proof ran"
  [[ "$(pwd -P)" == "$output_directory" ]] \
    || fail "held proof output path changed while live proof ran"
}

capture_repository_state() {
  local root="$1"
  local prefix="$2"
  local top branch head origin tree status mac_digest windows_digest
  top="$(git_safe "$root" rev-parse --show-toplevel)" \
    || fail "repository root could not be resolved"
  [[ "$top" == "$root" ]] || fail "controller must run from its own repository root"
  branch="$(git_safe "$root" symbolic-ref -q HEAD)" \
    || fail "detached HEAD is not eligible for restricted-worker proof"
  [[ "$branch" == "refs/heads/main" ]] || fail "restricted-worker proof requires exact main"
  head="$(git_safe "$root" rev-parse --verify 'HEAD^{commit}')" \
    || fail "HEAD commit could not be resolved"
  origin="$(git_safe "$root" rev-parse --verify 'refs/remotes/origin/main^{commit}')" \
    || fail "refs/remotes/origin/main is unavailable"
  [[ "$head" == "$origin" ]] || fail "HEAD does not equal refs/remotes/origin/main"
  tree="$(git_safe "$root" rev-parse --verify 'HEAD^{tree}')" \
    || fail "HEAD tree could not be resolved"
  if ! git_safe "$root" ls-files -v -- \
    | awk 'substr($0, 1, 2) != "H " { invalid = 1 } END { exit invalid }'; then
    fail "repository index contains hidden tracked-state flags"
  fi
  status="$(git_safe "$root" status --porcelain=v1 --untracked-files=all)" \
    || fail "working-tree status could not be resolved"
  [[ -z "$status" ]] || fail "restricted-worker proof requires a clean working tree"
  mac_digest="$(git_safe "$root" show "$head:$MAC_HARNESS_PATH" | sha256_stdin)" \
    || fail "committed Mac live harness could not be hashed"
  windows_digest="$(git_safe "$root" show "$head:$WINDOWS_CONTROL_PATH" | sha256_stdin)" \
    || fail "committed Windows live control could not be hashed"
  validate_hash "$head" || fail "HEAD has an unsupported object-id shape"
  validate_hash "$tree" || fail "HEAD tree has an unsupported object-id shape"
  validate_sha256 "$mac_digest" || fail "Mac live harness digest has the wrong shape"
  validate_sha256 "$windows_digest" || fail "Windows live control digest has the wrong shape"
  eval "${prefix}_branch=\$branch"
  eval "${prefix}_head=\$head"
  eval "${prefix}_origin=\$origin"
  eval "${prefix}_tree=\$tree"
  eval "${prefix}_status=\$status"
  eval "${prefix}_mac_digest=\$mac_digest"
  eval "${prefix}_windows_digest=\$windows_digest"
}

clear_live_environment() {
  local name
  while IFS='=' read -r name _value; do
    case "$name" in
      GIT_* | BASH_ENV | ENV | ASSEMBLYWRIGHT_MAC_BRIDGE_BIN | ASSEMBLYWRIGHT_MAC_AGENT_BIN | ASSEMBLYWRIGHT_TAILSCALE_BIN | ASSEMBLYWRIGHT_RESTRICTED_WORKER_INTERNAL_STDIN_V1 | ASSEMBLYWRIGHT_RESTRICTED_WORKER_INTERNAL_ROOT | ASSEMBLYWRIGHT_RESTRICTED_WORKER_RECEIPT_FD | ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_STDIN_V1 | ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_ROOT)
        unset "$name"
        ;;
    esac
  done < <(env)
}

terminate_live_group() {
  local count=0
  [[ "$live_pid" =~ ^[1-9][0-9]*$ ]] || return 0
  kill -TERM -- "-$live_pid" >/dev/null 2>&1 || true
  while kill -0 -- "-$live_pid" >/dev/null 2>&1; do
    count=$((count + 1))
    if [[ "$count" -ge 100 ]]; then
      kill -KILL -- "-$live_pid" >/dev/null 2>&1 || true
      break
    fi
    sleep 0.1
  done
  wait "$live_pid" >/dev/null 2>&1 || true
  live_pid=""
}

wait_for_live_group_drain() {
  local count=0
  [[ "$live_pid" =~ ^[1-9][0-9]*$ ]] || return 0
  while kill -0 -- "-$live_pid" >/dev/null 2>&1; do
    count=$((count + 1))
    [[ "$count" -lt 100 ]] || return 1
    sleep 0.1
  done
}

relay_live_receipts() {
  local transcript="$1"
  local marker marker_count wait_count receipt
  local -a markers
  markers=(
    assemblywright_mac_windows_local_coding_prepare_required
    assemblywright_mac_windows_local_coding_dispatch_required
    assemblywright_mac_windows_artifact_integration_required
    assemblywright_mac_windows_local_coding_cancel_required
    assemblywright_mac_windows_local_coding_abandon_required
    assemblywright_mac_windows_local_coding_cleanup_required
  )
  for marker in ${markers[@]+"${markers[@]}"}; do
    wait_count=0
    while :; do
      marker_count="$(grep -c "^$marker " "$transcript" || true)"
      [[ "$marker_count" == "0" || "$marker_count" == "1" ]] \
        || fail "live harness emitted a duplicate Windows-local action marker"
      [[ "$marker_count" == "0" ]] || break
      kill -0 "$live_pid" >/dev/null 2>&1 \
        || fail "live harness exited before requesting every Windows-local action"
      wait_count=$((wait_count + 1))
      [[ "$wait_count" -lt 6000 ]] \
        || fail "timed out waiting for the next Windows-local action marker"
      sleep 0.1
    done
    receipt=""
    if ! read_live_receipt receipt; then
      case "$receipt_read_error" in
        oversized)
          fail "sanitized Windows-local receipt exceeded the 8192-byte input bound"
          ;;
        incomplete)
          fail "timed out waiting for one complete sanitized Windows-local receipt"
          ;;
        *)
          fail "could not safely read and restore one sanitized Windows-local receipt"
          ;;
      esac
    fi
    [[ -n "$receipt" && "${#receipt}" -le 8192 ]] \
      || fail "sanitized Windows-local receipt was empty or oversized"
    printf '%s\n' "$receipt" >&4 \
      || fail "could not relay the sanitized Windows-local receipt"
  done
}

run_committed_live_harness() {
  local root="$1"
  local head="$2"
  local transcript="$3"
  local receipt_fifo="$4"
  local live_status group_remained=0
  set -m
  (
    set +m
    git_safe "$root" show "$head:$MAC_HARNESS_PATH" |
      (
        cd "$root" || exit 1
        clear_live_environment
        export ASSEMBLYWRIGHT_RESTRICTED_WORKER_INTERNAL_STDIN_V1="$SCHEMA"
        export ASSEMBLYWRIGHT_RESTRICTED_WORKER_INTERNAL_ROOT="$root"
        export ASSEMBLYWRIGHT_RESTRICTED_WORKER_RECEIPT_FD=3
        exec bash -s -- --run-local-coding 3<&3
      ) 3<&3 | tee "$transcript"
  ) 3<"$receipt_fifo" &
  live_pid="$!"
  set +m
  [[ "$live_pid" =~ ^[1-9][0-9]*$ ]] \
    || fail "restricted-worker live process could not be identified"
  exec 4>"$receipt_fifo"
  receipt_writer_open=1
  relay_live_receipts "$transcript"
  exec 4>&-
  receipt_writer_open=0
  set +e
  wait "$live_pid"
  live_status="$?"
  set -e
  if [[ "$live_status" -ne 0 ]]; then
    terminate_live_group
    fail "committed restricted-worker live harness failed"
  fi
  if ! wait_for_live_group_drain; then
    group_remained=1
    terminate_live_group
  else
    live_pid=""
  fi
  [[ "$group_remained" -eq 0 ]] \
    || fail "committed restricted-worker live harness left a live descendant"
}

validate_live_transcript() {
  local transcript="$1"
  local expected_head="$2"
  local line line_count success_line_number marker marker_count marker_entry marker_line_number
  local previous_marker_line=0 action_marker_count
  local -a fields expected_flags action_markers
  line_count="$(grep -c '^assemblywright_mac_windows_local_coding_live_e2e_ok ' "$transcript" || true)"
  [[ "$line_count" == "1" ]] || fail "live transcript did not contain one exact success record"
  line="$(grep '^assemblywright_mac_windows_local_coding_live_e2e_ok ' "$transcript")"
  success_line_number="$(grep -n '^assemblywright_mac_windows_local_coding_live_e2e_ok ' "$transcript" | cut -d: -f1)"
  [[ "$success_line_number" =~ ^[1-9][0-9]*$ ]] \
    || fail "live success record position was malformed"
  action_markers=(
    assemblywright_mac_windows_local_coding_prepare_required
    assemblywright_mac_windows_local_coding_dispatch_required
    assemblywright_mac_windows_artifact_integration_required
    assemblywright_mac_windows_local_coding_cancel_required
    assemblywright_mac_windows_local_coding_abandon_required
    assemblywright_mac_windows_local_coding_cleanup_required
  )
  action_marker_count="$(grep -Ec '^assemblywright_mac_windows_.*_required ' "$transcript" || true)"
  [[ "$action_marker_count" == "${#action_markers[@]}" ]] \
    || fail "live transcript contained an unexpected Windows-local action marker set"
  for marker in ${action_markers[@]+"${action_markers[@]}"}; do
    marker_count="$(grep -c "^$marker " "$transcript" || true)"
    [[ "$marker_count" == "1" ]] \
      || fail "live transcript did not contain one exact $marker record"
    marker_entry="$(grep -n "^$marker " "$transcript")"
    marker_line_number="${marker_entry%%:*}"
    [[ "$marker_line_number" =~ ^[1-9][0-9]*$ \
      && "$marker_line_number" -gt "$previous_marker_line" \
      && "$marker_line_number" -lt "$success_line_number" ]] \
      || fail "live Windows-local action markers were not unique and strictly ordered"
    previous_marker_line="$marker_line_number"
  done
  [[ "${#line}" -le 4096 ]] || fail "live success record exceeded its fixed bound"
  IFS=' ' read -r -a fields <<<"$line"
  [[ "${#fields[@]}" -eq 37 ]] || fail "live success record had an unexpected field count"
  [[ "${fields[0]}" == "assemblywright_mac_windows_local_coding_live_e2e_ok" ]] \
    || fail "live success record marker drifted"
  [[ "${fields[1]}" == endpoint=*:* \
    && "${#fields[1]}" -le 128 \
    && "${fields[1]}" != *"="*"="* ]] \
    || fail "live success endpoint binding was malformed"
  [[ "${fields[2]}" == "head_commit=$expected_head" ]] \
    || fail "Windows proof checkout did not bind the controller HEAD"
  [[ "${fields[3]}" == "protocol_version=5" \
    && "${fields[4]}" == "master_schema_version=19" \
    && "${fields[5]}" == "feature_conveyor_schema_version=9" ]] \
    || fail "live protocol or schema binding drifted"
  [[ "${fields[6]}" =~ ^feature_id=[0-9a-fA-F-]{36}$ ]] \
    && validate_uuid "${fields[6]#feature_id=}" \
    || fail "live feature ID label or value was malformed"
  [[ "${fields[7]}" =~ ^task_id=[0-9a-fA-F-]{36}$ ]] \
    && validate_uuid "${fields[7]#task_id=}" \
    || fail "live task ID label or value was malformed"
  [[ "${fields[8]}" =~ ^step_id=[0-9a-fA-F-]{36}$ ]] \
    && validate_uuid "${fields[8]#step_id=}" \
    || fail "live step ID label or value was malformed"
  [[ "${fields[9]}" =~ ^queued_sequence=[1-9][0-9]*$ \
    && "${fields[10]}" =~ ^leased_sequence=[1-9][0-9]*$ \
    && "${fields[11]}" =~ ^succeeded_sequence=[1-9][0-9]*$ ]] \
    || fail "live event sequence binding was malformed"
  [[ "${fields[9]#queued_sequence=}" -lt "${fields[10]#leased_sequence=}" \
    && "${fields[10]#leased_sequence=}" -lt "${fields[11]#succeeded_sequence=}" ]] \
    || fail "live event sequence was not strictly ordered"
  [[ "${fields[12]}" =~ ^snapshot_sha256=[0-9a-f]{64}$ ]] \
    && validate_sha256 "${fields[12]#snapshot_sha256=}" \
    || fail "live snapshot digest label or value was malformed"
  [[ "${fields[13]}" =~ ^work_packet_sha256=[0-9a-f]{64}$ ]] \
    && validate_sha256 "${fields[13]#work_packet_sha256=}" \
    || fail "live packet digest label or value was malformed"
  [[ "${fields[14]}" =~ ^integration_id=[0-9a-fA-F-]{36}$ ]] \
    && validate_uuid "${fields[14]#integration_id=}" \
    || fail "live integration ID label or value was malformed"
  [[ "${fields[15]}" =~ ^candidate_commit=[0-9a-f]{40}$ \
    && "${fields[16]}" =~ ^candidate_tree=[0-9a-f]{40}$ ]] \
    || fail "live candidate identity was malformed"
  [[ "${fields[17]}" =~ ^artifact_set_sha256=[0-9a-f]{64}$ ]] \
    && validate_sha256 "${fields[17]#artifact_set_sha256=}" \
    || fail "live artifact-set digest label or value was malformed"
  expected_flags=(
    separate_identity signed_swift_relay real_rust_agent
    mac_retained_attempt_pair_shape harness_owned_pair_cleanup artifact_integration
    detached_candidate candidate_remote_absent candidate_fsck_clean exact_integration_retry
    source_checkout_clean owner_cancel owner_abandon queue_empty feature_lease_empty
    distributed_active_state_empty windows_transfer_staging_empty grants_revoked
    disposable_checkout_removed
  )
  local index=18 flag
  for flag in ${expected_flags[@]+"${expected_flags[@]}"}; do
    [[ "${fields[$index]}" == "$flag=verified" ]] \
      || fail "live success record omitted exact $flag proof"
    index=$((index + 1))
  done
}

write_receipt_atomically() {
  local head="$1"
  local tree="$2"
  local mac_digest="$3"
  local windows_digest="$4"
  local transcript_digest="$5"
  local observed_at_ms="$6"
  local receipt_temp digest_temp receipt_sha256 receipt_bytes
  receipt_temp="$(mktemp .restricted-worker-live-proof.json.XXXXXX)" \
    || fail "could not allocate receipt temporary file"
  digest_temp="$(mktemp .restricted-worker-live-proof.sha256.XXXXXX)" \
    || { rm -f -- "$receipt_temp" || true; fail "could not allocate digest temporary file"; }
  chmod 600 "$receipt_temp" "$digest_temp" \
    || { rm -f -- "$receipt_temp" "$digest_temp" || true; fail "could not restrict proof temporary files"; }
  printf '{"schema":"%s","category":"%s","origin":"%s","head_commit":"%s","tree_id":"%s","mac_live_harness_definition_sha256":"%s","windows_live_control_definition_sha256":"%s","proof_transcript_sha256":"%s","proof_identity":"%s","observed_at_ms":%s,"status":"passed","proof_boundary":"%s"}\n' \
    "$SCHEMA" "$CATEGORY" "$ORIGIN" "$head" "$tree" "$mac_digest" \
    "$windows_digest" "$transcript_digest" "$PROOF_IDENTITY" "$observed_at_ms" \
    "$PROOF_BOUNDARY" >"$receipt_temp" \
    || { rm -f -- "$receipt_temp" "$digest_temp" || true; fail "could not write proof receipt"; }
  receipt_bytes="$(wc -c <"$receipt_temp" | tr -d '[:space:]')" \
    || { rm -f -- "$receipt_temp" "$digest_temp" || true; fail "could not measure proof receipt"; }
  [[ "$receipt_bytes" -le 3072 ]] \
    || { rm -f -- "$receipt_temp" "$digest_temp" || true; fail "receipt exceeded the fixed 3072-byte bound"; }
  receipt_sha256="$(sha256_file "$receipt_temp")" \
    || { rm -f -- "$receipt_temp" "$digest_temp" || true; fail "receipt digest could not be computed"; }
  validate_sha256 "$receipt_sha256" \
    || { rm -f -- "$receipt_temp" "$digest_temp" || true; fail "receipt digest has the wrong shape"; }
  printf '%s\n' "$receipt_sha256" >"$digest_temp" \
    || { rm -f -- "$receipt_temp" "$digest_temp" || true; fail "could not write receipt digest"; }
  mv -f -- "$digest_temp" "$DIGEST_NAME" \
    || { rm -f -- "$receipt_temp" "$digest_temp" "$DIGEST_NAME" || true; fail "could not publish receipt digest"; }
  mv -f -- "$receipt_temp" "$RECEIPT_NAME" \
    || { rm -f -- "$receipt_temp" "$RECEIPT_NAME" "$DIGEST_NAME" || true; fail "could not publish receipt commit marker"; }
}

run_controller() (
  local root
  root="$(cd "$1" && pwd -P)"
  local before_branch before_head before_origin before_tree before_status before_mac_digest before_windows_digest
  local after_branch after_head after_origin after_tree after_status after_mac_digest after_windows_digest
  local observed_at_ms transcript transcript_digest published_digest actual_digest receipt_fifo
  local output_prepared=0 published=0
  local prepared_target_identity="" prepared_output_identity="" live_pid=""
  local receipt_writer_open=0

  cleanup_controller() {
    restore_receipt_terminal || true
    if [[ "$receipt_writer_open" -eq 1 ]]; then
      exec 4>&-
      receipt_writer_open=0
    fi
    terminate_live_group
    if [[ "$output_prepared" -eq 1 ]]; then
      rm -f -- .restricted-worker-transcript.* .restricted-worker-receipts.* 2>/dev/null || true
      if [[ "$published" -ne 1 ]]; then
        rm -f -- "$RECEIPT_NAME" "$DIGEST_NAME" \
          .restricted-worker-live-proof.json.* \
          .restricted-worker-live-proof.sha256.* 2>/dev/null || true
      fi
    fi
  }
  handle_controller_signal() {
    local exit_status="$1"
    trap - HUP INT TERM
    restore_receipt_terminal || true
    terminate_live_group
    exit "$exit_status"
  }
  trap cleanup_controller EXIT
  trap 'handle_controller_signal 129' HUP
  trap 'handle_controller_signal 130' INT
  trap 'handle_controller_signal 143' TERM

  prepare_output_directory "$root"
  capture_repository_state "$root" before
  [[ "${ASSEMBLYWRIGHT_FEATURE_CONVEYOR_OWNER_CONTROL_DESIGNATION_REVISION:-}" =~ ^[1-9][0-9]*$ ]] \
    || fail "set ASSEMBLYWRIGHT_FEATURE_CONVEYOR_OWNER_CONTROL_DESIGNATION_REVISION to the exact current revision"
  transcript="$(mktemp .restricted-worker-transcript.XXXXXX)" \
    || fail "could not allocate the bounded live transcript"
  chmod 600 "$transcript" || fail "could not restrict the live transcript"
  receipt_fifo=".restricted-worker-receipts.$$.$RANDOM"
  [[ ! -e "$receipt_fifo" && ! -L "$receipt_fifo" ]] \
    || fail "could not allocate the fixed-shape receipt relay"
  mkfifo -m 600 "$receipt_fifo" || fail "could not create the private receipt relay"

  run_committed_live_harness "$root" "$before_head" "$transcript" "$receipt_fifo"
  rm -f -- "$receipt_fifo" || fail "could not remove the private receipt relay"
  receipt_fifo=""
  validate_live_transcript "$transcript" "$before_head"
  transcript_digest="$(sha256_file "$transcript")"
  validate_sha256 "$transcript_digest" || fail "live transcript digest had the wrong shape"

  capture_repository_state "$root" after
  [[ "$after_branch" == "$before_branch" ]] || fail "branch changed while live proof ran"
  [[ "$after_head" == "$before_head" ]] || fail "HEAD changed while live proof ran"
  [[ "$after_origin" == "$before_origin" ]] || fail "origin/main changed while live proof ran"
  [[ "$after_tree" == "$before_tree" ]] || fail "HEAD tree changed while live proof ran"
  [[ "$after_status" == "$before_status" ]] || fail "working-tree status changed while live proof ran"
  [[ "$after_mac_digest" == "$before_mac_digest" \
    && "$after_windows_digest" == "$before_windows_digest" ]] \
    || fail "committed live-controller definitions changed while proof ran"
  revalidate_output_identity "$root"

  observed_at_ms="$(date -u '+%s')000"
  [[ "$observed_at_ms" =~ ^[0-9]{13}$ ]] || fail "observed time has the wrong shape"
  write_receipt_atomically "$before_head" "$before_tree" "$before_mac_digest" \
    "$before_windows_digest" "$transcript_digest" "$observed_at_ms"
  rm -f -- "$transcript" || fail "could not remove the private live transcript"
  revalidate_output_identity "$root"
  validate_removable_output "$RECEIPT_NAME"
  validate_removable_output "$DIGEST_NAME"
  published_digest="$(tr -d '[:space:]' <"$DIGEST_NAME")" \
    || fail "published receipt digest could not be read"
  actual_digest="$(sha256_file "$RECEIPT_NAME")" \
    || fail "published receipt could not be hashed"
  validate_sha256 "$published_digest" || fail "published receipt digest has the wrong shape"
  [[ "$published_digest" == "$actual_digest" ]] \
    || fail "published receipt and digest do not match"
  published=1
  trap - EXIT HUP INT TERM
  printf 'Assemblywright restricted-worker live proof controller: passed\n'
  printf 'Receipt: %s\n' "$OUTPUT_RELATIVE/$RECEIPT_NAME"
  printf 'Receipt SHA-256: %s\n' "$OUTPUT_RELATIVE/$DIGEST_NAME"
  printf 'Proof boundary: %s\n' "$PROOF_BOUNDARY"
)

check_controller() {
  local command_name
  for command_name in git shasum awk mktemp mkfifo date id stat chmod mkdir mv rm wc tr grep find ln tee env sleep sed cut stty expect; do
    require_command "$command_name"
  done
  [[ -f "$ROOT_DIR/$MAC_HARNESS_PATH" ]] || fail "Mac live harness is unavailable"
  [[ -f "$ROOT_DIR/$WINDOWS_CONTROL_PATH" ]] || fail "Windows live control is unavailable"
  [[ -f "$ROOT_DIR/.gitignore" ]] || fail "repository .gitignore is unavailable"
  git_safe "$ROOT_DIR" check-ignore -q "$OUTPUT_RELATIVE/$RECEIPT_NAME" \
    || fail "fixed proof output is not ignored"
  grep -Fq 'run ./scripts/restricted-worker-proof-controller.sh --check' \
    "$ROOT_DIR/scripts/release-local.sh" \
    || fail "release-local does not run the restricted-worker controller check"
  grep -Fq 'run ./scripts/restricted-worker-proof-controller.sh --self-test' \
    "$ROOT_DIR/scripts/release-local.sh" \
    || fail "release-local does not run the restricted-worker controller self-test"
  if grep -Fq 'restricted-worker-proof-controller.sh --run' "$ROOT_DIR/scripts/release-local.sh"; then
    fail "release-local must not invoke live restricted-worker proof"
  fi
  grep -Fq 'ASSEMBLYWRIGHT_RESTRICTED_WORKER_INTERNAL_STDIN_V1' \
    "$ROOT_DIR/$MAC_HARNESS_PATH" \
    || fail "Mac harness does not recognize committed-stdin execution"
  grep -Fq 'RECEIPT_INPUT_FD=3' "$ROOT_DIR/$MAC_HARNESS_PATH" \
    || fail "Mac harness does not isolate committed script bytes from owner receipts"
  grep -Fq '$masterSchemaVersion = 19' "$ROOT_DIR/$WINDOWS_CONTROL_PATH" \
    || fail "Windows live control does not require schema v19"
  grep -Fq '$featureConveyorProjectionSchemaVersion = 9' "$ROOT_DIR/$WINDOWS_CONTROL_PATH" \
    || fail "Windows live control does not require projection schema v9"
  printf 'Assemblywright restricted-worker live proof controller check: ok\n'
  printf 'Proof boundary: static controller prerequisites only; no live worker ran and no receipt was created.\n'
}

write_fixture_harness() {
  local fixture="$1"
  local body="$2"
  mkdir -p "$fixture/scripts"
  printf '#!/usr/bin/env bash\nset -euo pipefail\n%s\n' "$body" >"$fixture/$MAC_HARNESS_PATH"
  chmod 700 "$fixture/$MAC_HARNESS_PATH"
  printf '%s\n' '\$masterSchemaVersion = 19' '\$featureConveyorProjectionSchemaVersion = 9' \
    >"$fixture/$WINDOWS_CONTROL_PATH"
}

fixture_coordination_body() {
  cat <<'BODY'
for marker in \
  assemblywright_mac_windows_local_coding_prepare_required \
  assemblywright_mac_windows_local_coding_dispatch_required \
  assemblywright_mac_windows_artifact_integration_required \
  assemblywright_mac_windows_local_coding_cancel_required \
  assemblywright_mac_windows_local_coding_abandon_required \
  assemblywright_mac_windows_local_coding_cleanup_required; do
  printf '%s action=fixed receipt_stdin=required\n' "$marker"
  IFS= read -r -u 3 receipt
  [[ -n "$receipt" ]]
done
BODY
}

fixture_terminal_body() {
  cat <<'BODY'
head="$(git rev-parse HEAD)"
printf 'assemblywright_mac_windows_local_coding_live_e2e_ok endpoint=100.64.23.14:7792 head_commit=%s protocol_version=5 master_schema_version=19 feature_conveyor_schema_version=9 feature_id=11111111-1111-4111-8111-111111111111 task_id=22222222-2222-4222-8222-222222222222 step_id=33333333-3333-4333-8333-333333333333 queued_sequence=10 leased_sequence=11 succeeded_sequence=12 snapshot_sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa work_packet_sha256=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb integration_id=44444444-4444-4444-8444-444444444444 candidate_commit=cccccccccccccccccccccccccccccccccccccccc candidate_tree=dddddddddddddddddddddddddddddddddddddddd artifact_set_sha256=eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee separate_identity=verified signed_swift_relay=verified real_rust_agent=verified mac_retained_attempt_pair_shape=verified harness_owned_pair_cleanup=verified artifact_integration=verified detached_candidate=verified candidate_remote_absent=verified candidate_fsck_clean=verified exact_integration_retry=verified source_checkout_clean=verified owner_cancel=verified owner_abandon=verified queue_empty=verified feature_lease_empty=verified distributed_active_state_empty=verified windows_transfer_staging_empty=verified grants_revoked=verified disposable_checkout_removed=verified\n' "$head"
BODY
}

fixture_receipts() {
  printf '{}\n{}\n{}\n{}\n{}\n{}\n'
}

run_fixture_controller() {
  local fixture="$1"
  fixture_receipts | \
    ASSEMBLYWRIGHT_FEATURE_CONVEYOR_OWNER_CONTROL_DESIGNATION_REVISION=1 \
      run_controller "$fixture"
}

initialize_fixture() {
  local fixture="$1"
  local body="$2"
  mkdir -p "$fixture"
  git_safe "$fixture" init -q
  git_safe "$fixture" checkout -q -b main
  git_safe "$fixture" config user.name 'Assemblywright Restricted Worker Self Test'
  git_safe "$fixture" config user.email 'restricted-worker-self-test@invalid.example'
  printf 'target/\n' >"$fixture/.gitignore"
  printf 'fixture\n' >"$fixture/README.md"
  write_fixture_harness "$fixture" "$body"
  git_safe "$fixture" add .
  git_safe "$fixture" commit -q -m fixture
  git_safe "$fixture" update-ref refs/remotes/origin/main HEAD
}

assert_no_proof_output() {
  local fixture="$1"
  local output_directory="$fixture/$OUTPUT_RELATIVE"
  [[ ! -e "$output_directory/$RECEIPT_NAME" ]] \
    || fail "self-test left a receipt after rejected execution"
  [[ ! -e "$output_directory/$DIGEST_NAME" ]] \
    || fail "self-test left a digest after rejected execution"
  if [[ -d "$output_directory" ]] && \
    find "$output_directory" -type f -name '.restricted-worker-*' -print -quit | grep -q .; then
    fail "self-test left a temporary proof file after rejected execution"
  fi
}

expect_controller_failure() {
  local fixture="$1"
  if run_fixture_controller "$fixture" >/dev/null 2>&1; then
    fail "self-test expected controller rejection"
  fi
  assert_no_proof_output "$fixture"
}

self_test_controller() {
  local scratch success transcript_variant dirty hidden_index hidden_skip_worktree wrong_branch failure malformed missing_label wrong_label duplicate late_duplicate_marker
  local origin_drift status_drift environment_hardening
  local persistent cancellation cancellation_ready cancellation_sentinel controller_pid controller_status
  local hostile_target hostile_external directory_swap swapped_output writable_target
  local receipt digest expected_digest actual_digest receipt_bytes file_count
  local success_transcript_digest variant_transcript_digest
  local success_head success_tree success_mac_digest success_windows_digest
  local mv_wrapper_dir digest_move_failure receipt_move_failure
  local pty_reader pty_expect long_receipt long_receipt_digest oversized_receipt
  local coordination_body terminal_body success_body missing_label_body wrong_label_body
  scratch="$(mktemp -d -t assemblywright-restricted-worker-proof)"
  chmod 700 "$scratch"
  # shellcheck disable=SC2064
  trap "rm -rf -- '$scratch'" RETURN
  coordination_body="$(fixture_coordination_body)"
  terminal_body="$(fixture_terminal_body)"
  success_body="$coordination_body"$'\n'"$terminal_body"

  "$ROOT_DIR/scripts/restricted-worker-proof-controller.sh" >/dev/null \
    || fail "self-test default mode did not perform the static check"
  if "$ROOT_DIR/scripts/restricted-worker-proof-controller.sh" --unknown-mode >/dev/null 2>&1; then
    fail "self-test accepted an unknown controller mode"
  fi
  if "$ROOT_DIR/scripts/restricted-worker-proof-controller.sh" --check extra >/dev/null 2>&1; then
    fail "self-test accepted an extra controller argument"
  fi

  pty_reader="$scratch/pty-receipt-reader.sh"
  {
    printf '%s\n' '#!/usr/bin/env bash' 'set -euo pipefail' \
      'receipt_terminal_state=""' 'receipt_read_error=""'
    declare -f restore_receipt_terminal read_live_receipt
    cat <<'PTY_READER'
before_state="$(stty -g <&0)"
receipt=""
case "${PTY_MODE:?}" in
  success)
    read_live_receipt receipt assemblywright_pty_receipt_reader_ready
    after_state="$(stty -g <&0)"
    [[ "$after_state" == "$before_state" ]]
    receipt_digest="$(printf '%s' "$receipt" | shasum -a 256 | awk '{print $1}')"
    printf 'assemblywright_pty_receipt_reader_ok bytes=%s sha256=%s terminal_restored=verified\n' \
      "${#receipt}" "$receipt_digest"
    ;;
  oversized)
    if read_live_receipt receipt assemblywright_pty_receipt_reader_ready; then
      exit 41
    fi
    after_state="$(stty -g <&0)"
    [[ "$after_state" == "$before_state" && "$receipt_read_error" == "oversized" ]]
    printf 'assemblywright_pty_receipt_oversized_rejected bytes=%s error=%s terminal_restored=verified\n' \
      "${#receipt}" "$receipt_read_error"
    ;;
  signal)
    handle_term() {
      restore_receipt_terminal
      after_state="$(stty -g <&0)"
      [[ "$after_state" == "$before_state" ]]
      printf 'assemblywright_pty_receipt_signal_restored signal=TERM terminal_restored=verified\n'
      exit 143
    }
    trap handle_term TERM
    read_live_receipt receipt assemblywright_pty_receipt_reader_ready
    exit 42
    ;;
  *) exit 43 ;;
esac
PTY_READER
  } >"$pty_reader"
  chmod 700 "$pty_reader"
  pty_expect="$scratch/pty-receipt-expect.tcl"
  cat <<'EXPECT' >"$pty_expect"
log_user 0
proc abort_fixture {} {
  set fixture_pid [exp_pid]
  catch {exec kill -TERM $fixture_pid}
  after 100
  catch {exec kill -KILL $fixture_pid}
  catch {close}
  catch {wait}
}
set timeout 240
spawn -noecho env PTY_MODE=$env(PTY_MODE) $env(PTY_READER)
expect {
  -re {assemblywright_pty_receipt_reader_ready} {}
  timeout { abort_fixture; exit 4 }
  eof { catch {wait}; exit 5 }
}
set timeout 10
if {$env(PTY_MODE) eq "signal"} {
  exec kill -TERM [exp_pid]
} else {
  send -- "$env(PTY_RECEIPT)\r"
}
expect {
  -re $env(PTY_EXPECTED_MARKER) {}
  timeout { abort_fixture; exit 2 }
  eof { catch {wait}; exit 3 }
}
expect {
  eof {}
  timeout { abort_fixture; exit 6 }
}
wait
exit 0
EXPECT
  chmod 600 "$pty_expect"
  long_receipt='{"receipt":"'
  while [[ "${#long_receipt}" -lt 2180 ]]; do long_receipt+="x"; done
  long_receipt+='"}'
  [[ "${#long_receipt}" -eq 2182 ]] \
    || fail "self-test PTY receipt fixture had the wrong length"
  long_receipt_digest="$(printf '%s' "$long_receipt" | sha256_stdin)"
  if ! env PTY_MODE=success PTY_READER="$pty_reader" PTY_RECEIPT="$long_receipt" \
      PTY_EXPECTED_MARKER="assemblywright_pty_receipt_reader_ok bytes=2182 sha256=$long_receipt_digest terminal_restored=verified" \
      expect "$pty_expect" >/dev/null; then
    fail "self-test PTY receipt reader failed"
  fi
  oversized_receipt='{"receipt":"'
  while [[ "${#oversized_receipt}" -lt 8998 ]]; do oversized_receipt+="y"; done
  oversized_receipt+='"}'
  [[ "${#oversized_receipt}" -eq 9000 ]] \
    || fail "self-test oversized PTY receipt fixture had the wrong length"
  if ! env PTY_MODE=oversized PTY_READER="$pty_reader" PTY_RECEIPT="$oversized_receipt" \
      PTY_EXPECTED_MARKER='assemblywright_pty_receipt_oversized_rejected bytes=8192 error=oversized terminal_restored=verified' \
      expect "$pty_expect" >/dev/null; then
    fail "self-test oversized PTY receipt rejection failed"
  fi
  if ! env PTY_MODE=signal PTY_READER="$pty_reader" PTY_RECEIPT='' \
      PTY_EXPECTED_MARKER='assemblywright_pty_receipt_signal_restored signal=TERM terminal_restored=verified' \
      expect "$pty_expect" >/dev/null; then
    fail "self-test signalled PTY receipt restoration failed"
  fi

  success="$scratch/success"
  initialize_fixture "$success" "$success_body"
  run_fixture_controller "$success" >/dev/null
  receipt="$success/$OUTPUT_RELATIVE/$RECEIPT_NAME"
  digest="$success/$OUTPUT_RELATIVE/$DIGEST_NAME"
  [[ -f "$receipt" && ! -L "$receipt" && -f "$digest" && ! -L "$digest" ]] \
    || fail "self-test success proof pair is absent"
  [[ "$(stat -f '%Lp' "$receipt")" == "600" \
    && "$(stat -f '%Lp' "$digest")" == "600" ]] \
    || fail "self-test proof pair permissions drifted"
  expected_digest="$(tr -d '[:space:]' <"$digest")"
  actual_digest="$(sha256_file "$receipt")"
  [[ "$expected_digest" == "$actual_digest" ]] || fail "self-test receipt digest mismatch"
  receipt_bytes="$(wc -c <"$receipt" | tr -d '[:space:]')"
  [[ "$receipt_bytes" -le 3072 ]] || fail "self-test receipt is not bounded"
  file_count="$(find "$success/$OUTPUT_RELATIVE" -type f | wc -l | tr -d '[:space:]')"
  [[ "$file_count" == "2" ]] || fail "self-test success output is not fixed-shape"
  success_head="$(git_safe "$success" rev-parse 'HEAD^{commit}')"
  success_tree="$(git_safe "$success" rev-parse 'HEAD^{tree}')"
  success_mac_digest="$(git_safe "$success" show "$success_head:$MAC_HARNESS_PATH" | sha256_stdin)"
  success_windows_digest="$(git_safe "$success" show "$success_head:$WINDOWS_CONTROL_PATH" | sha256_stdin)"
  grep -Fq '"schema":"assemblywright.restricted-worker-live-proof.v1"' "$receipt" \
    || fail "self-test receipt omitted schema"
  grep -Fq '"category":"restricted_worker_live"' "$receipt" \
    || fail "self-test receipt omitted category"
  grep -Fq '"origin":"restricted_worker_proof_controller"' "$receipt" \
    || fail "self-test receipt omitted origin"
  grep -Fq "\"head_commit\":\"$success_head\"" "$receipt" \
    || fail "self-test receipt did not bind exact HEAD"
  grep -Fq "\"tree_id\":\"$success_tree\"" "$receipt" \
    || fail "self-test receipt did not bind exact tree"
  grep -Fq "\"mac_live_harness_definition_sha256\":\"$success_mac_digest\"" "$receipt" \
    || fail "self-test receipt omitted Mac harness definition"
  grep -Fq "\"windows_live_control_definition_sha256\":\"$success_windows_digest\"" "$receipt" \
    || fail "self-test receipt omitted Windows control definition"
  grep -Eq '"proof_transcript_sha256":"[0-9a-f]{64}"' "$receipt" \
    || fail "self-test receipt omitted transcript digest"
  grep -Eq '"observed_at_ms":[0-9]{13}' "$receipt" \
    || fail "self-test receipt omitted observed time"
  if grep -Fq "$success" "$receipt" || grep -Eq '(/Users/|/private/|/tmp/|https?://|github\.com|100\.64\.)' "$receipt"; then
    fail "self-test receipt leaked a path or endpoint"
  fi

  success_transcript_digest="$(sed -E 's/.*"proof_transcript_sha256":"([0-9a-f]{64})".*/\1/' "$receipt")"
  transcript_variant="$scratch/transcript-variant"
  initialize_fixture "$transcript_variant" "printf '%s\\n' 'bounded pre-success transcript variation'"$'\n'"$success_body"
  run_fixture_controller "$transcript_variant" >/dev/null
  variant_transcript_digest="$(sed -E 's/.*"proof_transcript_sha256":"([0-9a-f]{64})".*/\1/' \
    "$transcript_variant/$OUTPUT_RELATIVE/$RECEIPT_NAME")"
  validate_sha256 "$success_transcript_digest" \
    || fail "self-test success transcript digest was malformed"
  validate_sha256 "$variant_transcript_digest" \
    || fail "self-test varied transcript digest was malformed"
  [[ "$success_transcript_digest" != "$variant_transcript_digest" ]] \
    || fail "self-test transcript digest did not bind the complete transcript"

  printf 'dirty rerun\n' >"$success/dirty-rerun.txt"
  expect_controller_failure "$success"

  dirty="$scratch/dirty"
  initialize_fixture "$dirty" "$success_body"
  printf 'dirty\n' >"$dirty/untracked.txt"
  expect_controller_failure "$dirty"

  hidden_index="$scratch/hidden-index"
  initialize_fixture "$hidden_index" "$success_body"
  git_safe "$hidden_index" update-index --assume-unchanged "$MAC_HARNESS_PATH"
  printf '#!/usr/bin/env bash\nexit 97\n' >"$hidden_index/$MAC_HARNESS_PATH"
  expect_controller_failure "$hidden_index"

  hidden_skip_worktree="$scratch/hidden-skip-worktree"
  initialize_fixture "$hidden_skip_worktree" "$success_body"
  git_safe "$hidden_skip_worktree" update-index --skip-worktree "$MAC_HARNESS_PATH"
  printf '#!/usr/bin/env bash\nexit 98\n' >"$hidden_skip_worktree/$MAC_HARNESS_PATH"
  expect_controller_failure "$hidden_skip_worktree"

  wrong_branch="$scratch/wrong-branch"
  initialize_fixture "$wrong_branch" "$success_body"
  git_safe "$wrong_branch" checkout -q -b feature
  expect_controller_failure "$wrong_branch"

  failure="$scratch/failure"
  initialize_fixture "$failure" 'exit 23'
  expect_controller_failure "$failure"

  malformed="$scratch/malformed"
  initialize_fixture "$malformed" "$coordination_body"$'\n'"printf '%s\\n' 'assemblywright_mac_windows_local_coding_live_e2e_ok malformed=true'"
  expect_controller_failure "$malformed"

  missing_label_body="$(printf '%s\n' "$terminal_body" | sed 's/ feature_id=/ /')"
  missing_label="$scratch/missing-label"
  initialize_fixture "$missing_label" "$coordination_body"$'\n'"$missing_label_body"
  expect_controller_failure "$missing_label"

  wrong_label_body="$(printf '%s\n' "$terminal_body" | sed 's/ snapshot_sha256=/ wrong_snapshot_sha256=/')"
  wrong_label="$scratch/wrong-label"
  initialize_fixture "$wrong_label" "$coordination_body"$'\n'"$wrong_label_body"
  expect_controller_failure "$wrong_label"

  duplicate="$scratch/duplicate"
  initialize_fixture "$duplicate" "$coordination_body"$'\n'"$terminal_body"$'\n'"$terminal_body"
  expect_controller_failure "$duplicate"

  late_duplicate_marker="$scratch/late-duplicate-marker"
  initialize_fixture "$late_duplicate_marker" \
    "$coordination_body"$'\n'"$terminal_body"$'\n'"printf '%s\\n' 'assemblywright_mac_windows_local_coding_prepare_required action=late receipt_stdin=required'"
  expect_controller_failure "$late_duplicate_marker"

  origin_drift="$scratch/origin-drift"
  initialize_fixture "$origin_drift" "$success_body"$'\n'"git update-ref -d refs/remotes/origin/main"
  expect_controller_failure "$origin_drift"

  status_drift="$scratch/status-drift"
  initialize_fixture "$status_drift" "$success_body"$'\n'"printf drift >post-live-drift.txt"
  expect_controller_failure "$status_drift"

  environment_hardening="$scratch/environment-hardening"
  initialize_fixture "$environment_hardening" \
    "$coordination_body"$'\n'"[[ -z \"\${GIT_DIR:-}\${GIT_WORK_TREE:-}\${GIT_INDEX_FILE:-}\${GIT_OBJECT_DIRECTORY:-}\${GIT_ALTERNATE_OBJECT_DIRECTORIES:-}\${GIT_REPLACE_REF_BASE:-}\${ASSEMBLYWRIGHT_MAC_BRIDGE_BIN:-}\${ASSEMBLYWRIGHT_MAC_AGENT_BIN:-}\${ASSEMBLYWRIGHT_TAILSCALE_BIN:-}\${ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_STDIN_V1:-}\${ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_ROOT:-}\" ]]"$'\n'"$terminal_body"
  GIT_DIR="$scratch/redirected.git" \
    GIT_WORK_TREE="$scratch/redirected-worktree" \
    GIT_INDEX_FILE="$scratch/redirected-index" \
    GIT_OBJECT_DIRECTORY="$scratch/redirected-objects" \
    GIT_ALTERNATE_OBJECT_DIRECTORIES="$scratch/alternate-objects" \
    GIT_REPLACE_REF_BASE="refs/evil/" \
    ASSEMBLYWRIGHT_MAC_BRIDGE_BIN="$scratch/fake-bridge" \
    ASSEMBLYWRIGHT_MAC_AGENT_BIN="$scratch/fake-agent" \
    ASSEMBLYWRIGHT_TAILSCALE_BIN="$scratch/fake-tailscale" \
    ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_STDIN_V1="caller-marker" \
    ASSEMBLYWRIGHT_CONTROL_STREAMING_INTERNAL_ROOT="$scratch/caller-root" \
    run_fixture_controller "$environment_hardening" >/dev/null

  persistent="$scratch/persistent"
  initialize_fixture "$persistent" "$success_body"$'\n'"(trap '' TERM; while :; do sleep 1; done) >/dev/null 2>&1 &"
  expect_controller_failure "$persistent"

  cancellation="$scratch/cancellation"
  cancellation_ready="$scratch/cancellation-ready"
  cancellation_sentinel="$scratch/cancellation-survived"
  initialize_fixture "$cancellation" \
    "printf ready >'$cancellation_ready'; (sleep 2; printf survived >'$cancellation_sentinel') & wait"
  set -m
  ASSEMBLYWRIGHT_FEATURE_CONVEYOR_OWNER_CONTROL_DESIGNATION_REVISION=1 \
    run_controller "$cancellation" </dev/null >/dev/null 2>&1 &
  controller_pid="$!"
  set +m
  local cancellation_wait_count=0
  while [[ ! -e "$cancellation_ready" ]]; do
    cancellation_wait_count=$((cancellation_wait_count + 1))
    if [[ "$cancellation_wait_count" -ge 50 ]]; then
      kill -TERM "$controller_pid" >/dev/null 2>&1 || true
      wait "$controller_pid" >/dev/null 2>&1 || true
      fail "self-test cancellation harness did not start"
    fi
    sleep 0.1
  done
  kill -TERM -- "-$controller_pid" || fail "self-test could not cancel the controller"
  set +e
  wait "$controller_pid" >/dev/null 2>&1
  controller_status="$?"
  set -e
  [[ "$controller_status" -eq 143 ]] || fail "self-test cancellation returned the wrong status"
  sleep 3
  [[ ! -e "$cancellation_sentinel" ]] || fail "self-test cancellation left a live descendant"
  assert_no_proof_output "$cancellation"

  hostile_target="$scratch/hostile-target"
  hostile_external="$scratch/hostile-external"
  initialize_fixture "$hostile_target" "$success_body"
  mkdir -m 700 "$hostile_external"
  printf 'preserve\n' >"$hostile_external/sentinel"
  ln -s "$hostile_external" "$hostile_target/target"
  expect_controller_failure "$hostile_target"
  [[ "$(<"$hostile_external/sentinel")" == "preserve" ]] \
    || fail "self-test hostile target changed external state"

  writable_target="$scratch/writable-target"
  initialize_fixture "$writable_target" "$success_body"
  mkdir -m 777 "$writable_target/target"
  chmod 777 "$writable_target/target"
  expect_controller_failure "$writable_target"

  directory_swap="$scratch/directory-swap"
  initialize_fixture "$directory_swap" "$success_body"$'\n'"mv target target-swapped; mkdir -m 700 target; mkdir -m 700 target/restricted-worker-live-proof"
  expect_controller_failure "$directory_swap"
  swapped_output="$directory_swap/target-swapped/restricted-worker-live-proof"
  [[ ! -e "$swapped_output/$RECEIPT_NAME" && ! -e "$swapped_output/$DIGEST_NAME" ]] \
    || fail "self-test directory swap retained proof output"

  mv_wrapper_dir="$scratch/mv-wrapper"
  mkdir -m 700 "$mv_wrapper_dir"
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'destination=""' \
    'for argument in "$@"; do destination="$argument"; done' \
    '[[ "$destination" != "${FAIL_MV_DEST:-}" ]] || exit 75' \
    'exec /bin/mv "$@"' >"$mv_wrapper_dir/mv"
  chmod 700 "$mv_wrapper_dir/mv"

  digest_move_failure="$scratch/digest-move-failure"
  initialize_fixture "$digest_move_failure" "$success_body"
  if PATH="$mv_wrapper_dir:$PATH" FAIL_MV_DEST="$DIGEST_NAME" \
      run_fixture_controller "$digest_move_failure" >/dev/null 2>&1; then
    fail "self-test expected digest publication failure"
  fi
  assert_no_proof_output "$digest_move_failure"

  receipt_move_failure="$scratch/receipt-move-failure"
  initialize_fixture "$receipt_move_failure" "$success_body"
  if PATH="$mv_wrapper_dir:$PATH" FAIL_MV_DEST="$RECEIPT_NAME" \
      run_fixture_controller "$receipt_move_failure" >/dev/null 2>&1; then
    fail "self-test expected receipt publication failure"
  fi
  assert_no_proof_output "$receipt_move_failure"

  printf 'Assemblywright restricted-worker live proof controller self-test: ok\n'
  printf 'Proof boundary: disposable Git/process/PTY fixtures prove fixed CLI, committed-byte/receipt-FD structure, bounded long receipt input beyond MAX_CANON, oversized-line drain/rejection, success/TERM terminal restoration, exact ordered action/success transcript validation, complete-transcript digest binding, dirty/hidden-index/wrong-branch/origin/status drift, environment isolation, stale-proof invalidation, malformed/duplicate output, descendant rejection, cancellation, hostile/swap/writable-target denial, atomic publication failure, permissions, redaction, and no-output behavior only.\n'
}

MODE="${1:---check}"
[[ "$#" -le 1 ]] || {
  usage >&2
  fail "the controller accepts no repository, remote, worker, executable, packet, harness, or extra arguments"
}

case "$MODE" in
  --check)
    check_controller
    ;;
  --run)
    run_controller "$ROOT_DIR"
    ;;
  --self-test)
    self_test_controller
    ;;
  --help | -h)
    usage
    ;;
  *)
    usage >&2
    fail "unknown mode: $MODE"
    ;;
esac
