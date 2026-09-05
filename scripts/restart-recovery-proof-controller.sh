#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "${BASH_SOURCE[0]%/*}/.." && pwd -P)"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
export PATH
OUTPUT_RELATIVE="target/restart-recovery-live-proof"
RECEIPT_NAME="restart-recovery-live-proof.json"
DIGEST_NAME="restart-recovery-live-proof.sha256"
SCHEMA="assemblywright.restart-recovery-live-proof.v2"
CATEGORY="restart_recovery_live"
ORIGIN="restart_recovery_proof_controller"
PROOF_IDENTITY="assemblywright.restart-recovery-live.v2"
HARNESS_PATH="scripts/restart-recovery-live-e2e.sh"
WINDOWS_PATH="scripts/windows-restart-recovery-live-control.ps1"
CONTROLLER_PATH="scripts/restart-recovery-proof-controller.sh"
FIXED_CARGO_PATH="/Users/michaelnobile/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/cargo"
FIXED_CARGO_SHA256="c512bff73c86143b557463f021d0c3d5b0490d97d65040ba59ea2b3427784758"
FIXED_RUSTC_PATH="/Users/michaelnobile/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/rustc"
FIXED_RUSTC_SHA256="b829b733131d4e1673eeebd1f34d06ae1e9ff4977b051313cf42e2a9e79ecf1c"
FIXED_CARGO_HOME="/Users/michaelnobile/.cargo"
FIXED_LIVE_PATH="/usr/bin:/bin:/usr/sbin:/sbin"
FIXED_GIT_PATH="/usr/bin/git"
FIXED_GIT_SHA256="179301dcb41ea78accc3fa0048a7e6f6710d891945a751a34addd622020c1818"
PROOF_BOUNDARY="Exact clean published main used the committed controller, harness, and Windows-control definitions to prove real Rust-agent retained-workspace functional recovery plus idle schema-v19 authoritative Windows-master stopped-state recovery and exact restoration. It separately binds the original/restored service digest and the transient pinned-toolchain exact-source rebuild digest, plus pinned Mac Git and Cargo, Windows Cargo, rustc, MSVC, frozen-database, migration, and continuity digests, and proves distinct healthy PIDs. Repository native focused tests separately cover master startup quarantine. This is not reproducible-build, installed-image source provenance, active-effect crash recovery, SCM retry-policy, signed-helper, control-streaming, admission, activation, signing, notarization, or production-readiness proof."
unset ASSEMBLYWRIGHT_RESTART_RECOVERY_INTERNAL_STDIN_V2
unset ASSEMBLYWRIGHT_RESTART_RECOVERY_RECEIPT_FD
unset ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD
unset ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH
unset ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256
unset ASSEMBLYWRIGHT_RESTART_RECOVERY_RUSTC_PATH
unset ASSEMBLYWRIGHT_RESTART_RECOVERY_RUSTC_SHA256
unset ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_HOME
receipt_terminal_state=""
receipt_read_error=""
fixture_mode=0

fail() { printf 'error: %s\n' "$1" >&2; exit 1; }

usage() {
  cat <<'USAGE'
Usage: scripts/restart-recovery-proof-controller.sh [--check | --run | --self-test]

  --check      Validate fixed prerequisites without restarting a process or service.
  --run        Run only the exact committed harness and write the fixed receipt.
  --self-test  Exercise success and fail-closed behavior in disposable Git fixtures.

The controller accepts no repository, remote, service, data directory, executable,
test, receipt, or harness argument. It never admits evidence or activates.
USAGE
}

require_command() { command -v "$1" >/dev/null 2>&1 || fail "required command is unavailable: $1"; }

git_safe() {
  local root="$1" protocol_policy=never; shift
  [[ "$fixture_mode" -eq 0 ]] || protocol_policy=always
  /usr/bin/env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin LC_ALL=C GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null \
    GIT_CONFIG_NOSYSTEM=1 GIT_ATTR_NOSYSTEM=1 GIT_TERMINAL_PROMPT=0 GIT_OPTIONAL_LOCKS=0 \
    "$FIXED_GIT_PATH" --no-replace-objects -c core.fsmonitor=false -c core.hooksPath=/dev/null \
      -c core.attributesFile=/dev/null -c protocol.file.allow="$protocol_policy" -C "$root" "$@"
}

sha256_file() { shasum -a 256 "$1" | awk '{print $1}'; }
sha256_stdin() { shasum -a 256 | awk '{print $1}'; }
valid_sha() { [[ "$1" =~ ^[0-9a-f]{64}$ ]]; }
valid_commit() { [[ "$1" =~ ^[0-9a-f]{40}$ ]]; }
directory_identity() { stat -f '%d:%i' "$1"; }

validate_fixed_cargo() {
  [[ "$FIXED_CARGO_PATH" == /Users/michaelnobile/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/cargo ]] \
    || fail "fixed Cargo path is invalid"
  [[ -f "$FIXED_CARGO_PATH" && ! -L "$FIXED_CARGO_PATH" ]] || fail "fixed Cargo is not an ordinary file"
  [[ "$(/usr/bin/stat -f '%u:%Lp:%l:%HT' "$FIXED_CARGO_PATH")" == \
     "$(/usr/bin/id -u):755:1:Regular File" ]] || fail "fixed Cargo owner, mode, link, or type is invalid"
  [[ "$(cd "$(dirname "$FIXED_CARGO_PATH")" && pwd -P)/$(basename "$FIXED_CARGO_PATH")" == "$FIXED_CARGO_PATH" ]] \
    || fail "fixed Cargo canonical identity is invalid"
  [[ "$(/usr/bin/shasum -a 256 "$FIXED_CARGO_PATH" | /usr/bin/awk '{print $1}')" == "$FIXED_CARGO_SHA256" ]] \
    || fail "fixed Cargo digest drifted"
  [[ -f "$FIXED_RUSTC_PATH" && ! -L "$FIXED_RUSTC_PATH" ]] || fail "fixed rustc is not an ordinary file"
  [[ "$(/usr/bin/stat -f '%u:%Lp:%l:%HT' "$FIXED_RUSTC_PATH")" == \
     "$(/usr/bin/id -u):755:1:Regular File" ]] || fail "fixed rustc owner, mode, link, or type is invalid"
  [[ "$(cd "${FIXED_RUSTC_PATH%/*}" && pwd -P)/${FIXED_RUSTC_PATH##*/}" == "$FIXED_RUSTC_PATH" ]] \
    || fail "fixed rustc canonical identity is invalid"
  [[ "$(/usr/bin/shasum -a 256 "$FIXED_RUSTC_PATH" | /usr/bin/awk '{print $1}')" == "$FIXED_RUSTC_SHA256" ]] \
    || fail "fixed rustc digest drifted"
  [[ -d "$FIXED_CARGO_HOME" && ! -L "$FIXED_CARGO_HOME" ]] || fail "fixed Cargo home is not an ordinary directory"
  [[ "$(/usr/bin/stat -f '%u:%HT' "$FIXED_CARGO_HOME")" == "$(/usr/bin/id -u):Directory" ]] \
    || fail "fixed Cargo home owner or type is invalid"
  [[ "$(cd "$FIXED_CARGO_HOME" && pwd -P)" == "$FIXED_CARGO_HOME" ]] || fail "fixed Cargo home canonical identity is invalid"
  [[ "$((8#$(/usr/bin/stat -f '%Lp' "$FIXED_CARGO_HOME") & 8#022))" -eq 0 ]] \
    || fail "fixed Cargo home is group or world writable"
  validate_no_cargo_configs "$ROOT_DIR"
  for config in "$FIXED_CARGO_HOME/config" "$FIXED_CARGO_HOME/config.toml"; do
    [[ ! -e "$config" && ! -L "$config" ]] || fail "fixed Cargo home contains a build-affecting configuration"
  done
}

validate_no_cargo_configs() {
  local current="$1" parent config
  current="$(cd "$current" && pwd -P)"
  while :; do
    for config in "$current/.cargo/config" "$current/.cargo/config.toml"; do
      [[ ! -e "$config" && ! -L "$config" ]] || fail "Cargo configuration is forbidden in the proof checkout or its ancestors"
    done
    [[ "$current" == / ]] && break
    parent="${current%/*}"; [[ -n "$parent" ]] || parent=/
    current="$parent"
  done
}

fixed_cargo_available() {
  [[ -f "$FIXED_CARGO_PATH" && ! -L "$FIXED_CARGO_PATH" ]] || return 1
  [[ "$(/usr/bin/stat -f '%u:%Lp:%l:%HT' "$FIXED_CARGO_PATH" 2>/dev/null)" == \
     "$(/usr/bin/id -u):755:1:Regular File" ]] || return 1
  [[ "$(/usr/bin/shasum -a 256 "$FIXED_CARGO_PATH" | /usr/bin/awk '{print $1}')" == "$FIXED_CARGO_SHA256" ]] || return 1
  [[ -f "$FIXED_RUSTC_PATH" && ! -L "$FIXED_RUSTC_PATH" ]] || return 1
  [[ "$(/usr/bin/stat -f '%u:%Lp:%l:%HT' "$FIXED_RUSTC_PATH" 2>/dev/null)" == \
     "$(/usr/bin/id -u):755:1:Regular File" ]] || return 1
  [[ "$(/usr/bin/shasum -a 256 "$FIXED_RUSTC_PATH" | /usr/bin/awk '{print $1}')" == "$FIXED_RUSTC_SHA256" ]]
}

validate_fixed_git_live() {
  validate_system_git_portable
  [[ "$(/usr/bin/shasum -a 256 "$FIXED_GIT_PATH" | /usr/bin/awk '{print $1}')" == "$FIXED_GIT_SHA256" ]] \
    || fail "fixed Git digest drifted"
}

validate_system_git_portable() {
  [[ "$FIXED_GIT_PATH" == /usr/bin/git && -f "$FIXED_GIT_PATH" && ! -L "$FIXED_GIT_PATH" ]] \
    || fail "fixed Git path or type is invalid"
  [[ "$(/usr/bin/stat -f '%u:%Lp:%HT' "$FIXED_GIT_PATH")" == "0:755:Regular File" ]] \
    || fail "fixed Git owner or mode is invalid"
  [[ "$(cd "${FIXED_GIT_PATH%/*}" && pwd -P)/${FIXED_GIT_PATH##*/}" == "$FIXED_GIT_PATH" ]] \
    || fail "fixed Git canonical identity is invalid"
}

restore_receipt_terminal() {
  local saved="${receipt_terminal_state:-}"
  [[ -n "$saved" ]] || return 0
  stty "$saved" <&0 || return 1
  receipt_terminal_state=""
}

read_live_receipt() {
  local destination="$1" ready_marker="${2:-}" LC_ALL=C line="" character="" status=0 oversized=0 deadline remaining
  receipt_terminal_state=""; receipt_read_error=""
  if [[ -t 0 ]]; then
    receipt_terminal_state="$(stty -g <&0)" || { receipt_read_error=terminal_state_unavailable; return 1; }
    stty -echo -icanon min 1 time 0 <&0 \
      || { receipt_read_error=terminal_mode_unavailable; restore_receipt_terminal || true; return 1; }
  fi
  [[ -z "$ready_marker" ]] || printf '%s\n' "$ready_marker"
  deadline=$((SECONDS + 1200))
  while :; do
    remaining=$((deadline - SECONDS))
    if [[ "$remaining" -le 0 ]] || ! IFS= read -r -t "$remaining" -n 1 character; then
      receipt_read_error=incomplete; status=1; break
    fi
    [[ -n "$character" ]] || break
    if [[ "${#line}" -lt 8192 ]]; then line+="$character"; else oversized=1; fi
  done
  if [[ "$oversized" -eq 1 ]]; then receipt_read_error=oversized; status=1; fi
  restore_receipt_terminal || { receipt_read_error=terminal_restore_failed; return 1; }
  printf -v "$destination" '%s' "$line"
  return "$status"
}

validate_owner_directory() {
  local path="$1"
  [[ -d "$path" && ! -L "$path" ]] || fail "proof output is not an ordinary directory"
  [[ "$(stat -f '%u' "$path")" == "$(id -u)" ]] || fail "proof output owner is not exact"
  [[ "$(stat -f '%Lp' "$path")" == 700 ]] || fail "proof output directory must have mode 0700"
  [[ "$(stat -f '%HT' "$path")" == Directory ]] || fail "proof output type is invalid"
}

validate_target_directory() {
  local root="$1" path="$1/target" canonical mode
  [[ -d "$path" && ! -L "$path" ]] || fail "repository target is not an ordinary directory"
  [[ "$(stat -f '%u' "$path")" == "$(id -u)" ]] || fail "repository target owner is not exact"
  mode="$(stat -f '%Lp' "$path")"
  [[ "$mode" =~ ^[0-7][0145][0145]$ ]] || fail "repository target is group/world writable"
  [[ "$(stat -f '%HT' "$path")" == Directory ]] || fail "repository target type is invalid"
  canonical="$(cd "$path" && pwd -P)"; [[ "$canonical" == "$path" ]] || fail "repository target identity is ambiguous"
}

validate_output_file() {
  local path="$1"
  [[ ! -e "$path" && ! -L "$path" ]] && return 0
  [[ -f "$path" && ! -L "$path" ]] || fail "refusing unsafe existing proof output"
  [[ "$(stat -f '%u' "$path")" == "$(id -u)" ]] || fail "existing proof owner is not exact"
  [[ "$(stat -f '%Lp' "$path")" == 600 ]] || fail "existing proof mode is not 0600"
  [[ "$(stat -f '%l' "$path")" == 1 ]] || fail "existing proof has multiple links"
  [[ "$(stat -f '%HT' "$path")" == "Regular File" ]] || fail "existing proof type is invalid"
}

prepare_output() {
  local root="$1" target="$1/target" output="$1/$OUTPUT_RELATIVE"
  umask 077
  if [[ -e "$target" || -L "$target" ]]; then validate_target_directory "$root"; else mkdir -m 700 "$target"; fi
  [[ -e "$output" || -L "$output" ]] || mkdir -m 700 "$output"
  validate_owner_directory "$output"; cd "$output"
  [[ "$(pwd -P)" == "$output" ]] || fail "proof output identity is ambiguous"
  target_identity="$(directory_identity "$target")"; output_identity="$(directory_identity .)"; output_prepared=1
  validate_output_file "$RECEIPT_NAME"; validate_output_file "$DIGEST_NAME"
  rm -f -- "$RECEIPT_NAME" "$DIGEST_NAME"
}

revalidate_output() {
  local root="$1" output="$1/$OUTPUT_RELATIVE"
  validate_target_directory "$root"; validate_owner_directory "$output"
  [[ "$(directory_identity "$root/target")" == "$target_identity" ]] || fail "target identity changed"
  [[ "$(directory_identity "$output")" == "$output_identity" ]] || fail "proof output identity changed"
  [[ "$(directory_identity .)" == "$output_identity" && "$(pwd -P)" == "$output" ]] || fail "held output changed"
}

capture_source_state() {
  local root="$1" prefix="$2" top branch head origin tree status remote_url path digest variable
  top="$(git_safe "$root" rev-parse --show-toplevel)"; [[ "$top" == "$root" ]] || fail "controller must run at its repository root"
  branch="$(git_safe "$root" symbolic-ref -q HEAD)" || fail "detached HEAD is not eligible"
  [[ "$branch" == refs/heads/main ]] || fail "restart-recovery proof requires exact main"
  head="$(git_safe "$root" rev-parse --verify 'HEAD^{commit}')"
  origin="$(git_safe "$root" rev-parse --verify 'refs/remotes/origin/main^{commit}')"
  remote_url="$(git_safe "$root" remote get-url origin)"
  if [[ "$fixture_mode" -eq 0 ]]; then
    [[ "$remote_url" == "https://github.com/malak333/Assemblywright" || \
       "$remote_url" == "https://github.com/malak333/Assemblywright.git" ]] \
      || fail "origin is not the fixed Assemblywright GitHub repository"
  fi
  [[ "$head" == "$origin" ]] || fail "HEAD does not equal origin/main"
  tree="$(git_safe "$root" rev-parse --verify 'HEAD^{tree}')"
  git_safe "$root" ls-files -v -- | awk 'substr($0,1,2)!="H "{bad=1} END{exit bad}' \
    || fail "repository index contains hidden tracked-state flags"
  status="$(git_safe "$root" status --porcelain=v1 --untracked-files=all)"
  [[ -z "$status" ]] || fail "restart-recovery proof requires a clean working tree"
  valid_commit "$head" && valid_commit "$origin" || fail "commit shape is invalid"
  for path in "$CONTROLLER_PATH" "$HARNESS_PATH" "$WINDOWS_PATH"; do
    digest="$(git_safe "$root" show "$head:$path" | sha256_stdin)" || fail "committed proof definition is unavailable: $path"
    valid_sha "$digest" || fail "committed proof definition digest is invalid"
    case "$path" in "$CONTROLLER_PATH") variable=controller_digest;; "$HARNESS_PATH") variable=harness_digest;; *) variable=windows_digest;; esac
    eval "${prefix}_${variable}=\$digest"
  done
  eval "${prefix}_branch=\$branch"; eval "${prefix}_head=\$head"; eval "${prefix}_origin=\$origin"
  eval "${prefix}_tree=\$tree"; eval "${prefix}_status=\$status"
}

clear_live_environment() {
  local name
  while IFS='=' read -r name _; do
    case "$name" in PATH|GIT_*|BASH_ENV|ENV|CARGO_*|RUST*|DYLD_*|LD_*|CC|CXX|AR|AS|CPP|CFLAGS|CXXFLAGS|CPPFLAGS|LDFLAGS|SDKROOT|DEVELOPER_DIR|MACOSX_DEPLOYMENT_TARGET|MAKEFLAGS|ASSEMBLYWRIGHT_RESTART_RECOVERY_*) unset "$name";; esac
  done < <(env)
}

terminate_live_group() {
  local count=0
  [[ "${live_pid:-}" =~ ^[1-9][0-9]*$ ]] || return 0
  kill -TERM -- "-$live_pid" >/dev/null 2>&1 || true
  while kill -0 -- "-$live_pid" >/dev/null 2>&1; do
    count=$((count + 1)); [[ "$count" -lt 100 ]] || { kill -KILL -- "-$live_pid" >/dev/null 2>&1 || true; break; }
    sleep 0.1
  done
  wait "$live_pid" >/dev/null 2>&1 || true; live_pid=""
}

run_committed_harness() {
  local root="$1" head="$2" transcript="$3" fifo="$4" count=0 receipt="" status
  set -m
  (
    set +m
    git_safe "$root" show "$head:$HARNESS_PATH" | (
      exec 9<&0; exec 0</dev/null; cd "$root"; clear_live_environment
      export ASSEMBLYWRIGHT_RESTART_RECOVERY_INTERNAL_STDIN_V2="$SCHEMA"
      export ASSEMBLYWRIGHT_RESTART_RECOVERY_RECEIPT_FD=3
      export ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD="$head"
      export ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH="$FIXED_CARGO_PATH"
      export ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256="$FIXED_CARGO_SHA256"
      export ASSEMBLYWRIGHT_RESTART_RECOVERY_RUSTC_PATH="$FIXED_RUSTC_PATH"
      export ASSEMBLYWRIGHT_RESTART_RECOVERY_RUSTC_SHA256="$FIXED_RUSTC_SHA256"
      export ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_HOME="$FIXED_CARGO_HOME"
      export PATH="$FIXED_LIVE_PATH"
      exec /bin/bash /dev/fd/9 --run 3<&3
    ) 3<&3 | tee "$transcript"
  ) 3<"$fifo" &
  live_pid="$!"; set +m; exec 4>"$fifo"; receipt_writer_open=1
  while ! grep -q '^assemblywright_restart_recovery_windows_run_required ' "$transcript" 2>/dev/null; do
    kill -0 "$live_pid" >/dev/null 2>&1 || fail "live harness exited before requesting the Windows action"
    count=$((count + 1)); [[ "$count" -lt 12000 ]] || fail "timed out waiting for the Windows action marker"; sleep 0.1
  done
  [[ "$(grep -c '^assemblywright_restart_recovery_windows_run_required ' "$transcript")" == 1 ]] \
    || fail "live harness emitted a duplicate action marker"
  if ! read_live_receipt receipt; then fail "sanitized Windows receipt ${receipt_read_error:-could_not_be_read}"; fi
  [[ -n "$receipt" && "${#receipt}" -le 8192 ]] || fail "sanitized Windows receipt was empty or oversized"
  printf '%s\n' "$receipt" >&4; exec 4>&-; receipt_writer_open=0
  set +e; wait "$live_pid"; status="$?"; set -e
  [[ "$status" -eq 0 ]] || { terminate_live_group; fail "committed restart-recovery live harness failed"; }
  if kill -0 -- "-$live_pid" >/dev/null 2>&1; then terminate_live_group; fail "committed live harness left a descendant"; fi
  live_pid=""
}

validate_transcript() {
  local transcript="$1" source_head="$2" marker success line
  marker="$(grep -c '^assemblywright_restart_recovery_windows_run_required ' "$transcript" || true)"
  success="$(grep -c '^assemblywright_restart_recovery_live_e2e_ok ' "$transcript" || true)"
  [[ "$marker" == 1 && "$success" == 1 ]] || fail "live transcript marker set was not exact"
  [[ "$(grep -Ec '^assemblywright_restart_recovery_.*(required|ok) ' "$transcript" || true)" == 2 ]] \
    || fail "live transcript contained an unexpected proof marker"
  line="$(grep '^assemblywright_restart_recovery_live_e2e_ok ' "$transcript")"
  [[ "${#line}" -le 3072 ]] || fail "live success record was oversized"
  TRANSCRIPT_LINE="$line" EXPECTED_HEAD="$source_head" python3 - <<'PY' || fail "live success bindings were invalid"
import os, re, sys
parts=os.environ["TRANSCRIPT_LINE"].split(" ")
names=["cargo_executable_sha256","source_head","protocol_version","master_schema_version","service_executable_sha256","rebuilt_service_executable_sha256","windows_cargo_executable_sha256","windows_rustc_executable_sha256","windows_msvc_environment_sha256","frozen_database_sha256","pre_process_id","post_process_id","queue_revision","emergency_pause_revision","owner_control_designation_revision","activation_status","activation_evidence_sha256","migration_backup_count","migration_backups_sha256","continuity_sha256","observed_at_ms"]
if len(parts)!=len(names)+1 or parts[0]!="assemblywright_restart_recovery_live_e2e_ok": sys.exit(1)
values={}
for name,part in zip(names,parts[1:]):
    if not part.startswith(name+"=") or name in values: sys.exit(1)
    values[name]=part[len(name)+1:]
if values["source_head"]!=os.environ["EXPECTED_HEAD"] or values["protocol_version"]!="5" or values["master_schema_version"]!="19": sys.exit(1)
if values["cargo_executable_sha256"]!="c512bff73c86143b557463f021d0c3d5b0490d97d65040ba59ea2b3427784758": sys.exit(1)
if values["windows_cargo_executable_sha256"]!="dc19c8e6d66802d120bf0696b1924b748bd90f3ca16f21391e54a290ff12b7c5": sys.exit(1)
if values["windows_rustc_executable_sha256"]!="e3ebbd547ea7b73c034d588ba569602b379f3b05ad1a3b5f8dcfab9d4478d74a": sys.exit(1)
if values["windows_msvc_environment_sha256"]!="6b516d8fcf543c14b2d861e1f45661e0029230fe0dc48e86ce78522801822209": sys.exit(1)
if values["activation_status"] not in ("inactive","active"): sys.exit(1)
if not re.fullmatch(r"[0-9a-f]{40}",values["source_head"]): sys.exit(1)
for key in ("service_executable_sha256","rebuilt_service_executable_sha256","windows_cargo_executable_sha256","windows_rustc_executable_sha256","windows_msvc_environment_sha256","frozen_database_sha256","activation_evidence_sha256","migration_backups_sha256","continuity_sha256"):
    if not re.fullmatch(r"[0-9a-f]{64}",values[key]): sys.exit(1)
for key in ("pre_process_id","post_process_id"):
    if not re.fullmatch(r"[1-9][0-9]*",values[key]): sys.exit(1)
if values["pre_process_id"]==values["post_process_id"]: sys.exit(1)
for key in ("queue_revision","emergency_pause_revision","owner_control_designation_revision","migration_backup_count"):
    if not re.fullmatch(r"[0-9]+",values[key]): sys.exit(1)
if int(values["migration_backup_count"])>32 or not re.fullmatch(r"[0-9]{13}",values["observed_at_ms"]): sys.exit(1)
PY
  service_executable_digest="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["service_executable_sha256"])')"
  rebuilt_service_executable_digest="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["rebuilt_service_executable_sha256"])')"
  windows_cargo_executable_digest="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["windows_cargo_executable_sha256"])')"
  windows_rustc_executable_digest="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["windows_rustc_executable_sha256"])')"
  windows_msvc_environment_digest="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["windows_msvc_environment_sha256"])')"
  frozen_database_digest="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["frozen_database_sha256"])')"
  cargo_executable_digest="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["cargo_executable_sha256"])')"
  continuity_digest="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["continuity_sha256"])')"
  activation_evidence_digest="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["activation_evidence_sha256"])')"
  migration_backups_digest="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["migration_backups_sha256"])')"
}

validate_windows_contract() {
  python3 - "$ROOT_DIR/$WINDOWS_PATH" <<'PY' || fail "Windows restart-recovery contract was incomplete"
import pathlib, sys
text=pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
required=(
  '$sourceRepository = "C:\\Users\\mike\\Codex\\Assemblywright"',
  '$dataDir = "C:\\Users\\mike\\AppData\\Local\\Assemblywright\\master"',
  '$serviceName = "AssemblywrightMaster"', '$serviceOwner = "MIKE-PC\\mike"',
  '$remoteEndpoint = "100.64.23.14:7792"', '$argumentTail -cne $expectedTail',
  '$gitSource = "C:\\Program Files\\Git\\cmd\\git.exe"',
  '$gitExecutableSha256 = "22fead8244ef3a7225fb800099a4e43eca8bcec0466774917669599c2f19a05a"',
  '$sqliteLibrary = "C:\\Windows\\System32\\winsqlite3.dll"',
  '$cargoExecutable = "C:\\Users\\mike\\.rustup\\toolchains\\1.95.0-x86_64-pc-windows-msvc\\bin\\cargo.exe"',
  '$cargoExecutableSha256 = "dc19c8e6d66802d120bf0696b1924b748bd90f3ca16f21391e54a290ff12b7c5"',
  '$rustcExecutableSha256 = "e3ebbd547ea7b73c034d588ba569602b379f3b05ad1a3b5f8dcfab9d4478d74a"',
  '$msvcEnvironmentScript = "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\\VC\\Auxiliary\\Build\\vcvars64.bat"',
  '$msvcEnvironmentSha256 = "6b516d8fcf543c14b2d861e1f45661e0029230fe0dc48e86ce78522801822209"',
  'cargo 1.95.0 (f2d3ce0bd 2026-03-21)', '--locked --offline --release',
  'Push-Location -LiteralPath $sourceRepository', '--manifest-path $manifestPath',
  '"C:\\Users\\mike\\Codex\\.cargo\\config.toml"', '"C:\\.cargo\\config.toml"',
  'Invoke-WithRestartRecoveryControlLock', 'Run requires -ConfirmAction.',
  'The Windows source HEAD did not match the controller-reported expected HEAD.',
  'Stop-ExactService', 'Invoke-ExactOfflineMasterBuild', 'Assert-OwnerSystemFileIdentity',
  '$postFrozenDatabaseSha -cne $frozenDatabaseSha',
  'Copy-Item -LiteralPath $originalBackup -Destination $service.Executable -Force',
  'PRAGMA integrity_check', 'feature_activation_evidence', 'feature_orchestration_activation',
  'Assert-SameSnapshot $preDatabase $postDatabase',
  '$health.emergency_paused -ne $false', '$health.state.active_attempts -ne 0',
  '$status.owner_guidance.state -cne "idle"',
  '$final.Health.process_id -eq [UInt64]$preHealth.process_id',
  'windows_cargo_executable_sha256 = $build.CargoSha256',
  'windows_rustc_executable_sha256 = $build.RustcSha256',
  'windows_msvc_environment_sha256 = $build.MsvcEnvironmentSha256',
  'rebuilt_service_executable_sha256 = $build.ExecutableSha256',
  'frozen_database_sha256 = $frozenDatabaseSha',
  'migration_backups_sha256 = $postDatabase.BackupsSha256',
)
if any(token not in text for token in required): raise SystemExit(1)
run=text.index('function Invoke-Run')
build=text.index('function Invoke-ExactOfflineMasterBuild')
push=text.index('Push-Location -LiteralPath $sourceRepository',build)
manifest=text.index('--manifest-path $manifestPath',push)
pop=text.index('Pop-Location',manifest)
confirm=text.index('Run requires -ConfirmAction.',run)
snapshot=text.index('$preDatabase = Get-DatabaseSnapshot',run)
effect=text.index('$build = Invoke-ExactOfflineMasterBuild',snapshot)
post=text.index('$postDatabase = Get-DatabaseSnapshot',effect)
compare=text.index('Assert-SameSnapshot $preDatabase $postDatabase',post)
restore=text.index('Copy-Item -LiteralPath $originalBackup -Destination $service.Executable -Force',compare)
final=text.index('$final = Start-ExactServiceHealthy',restore)
if not build < push < manifest < pop < run < confirm < snapshot < effect < post < compare < restore < final: raise SystemExit(1)
PY
}

write_receipt() {
  local source="$1" tree="$2" controller="$3" harness="$4" windows="$5" transcript="$6" observed="$7"
  local receipt_tmp digest_tmp digest bytes
  receipt_tmp="$(mktemp .restart-recovery-live-proof.json.XXXXXX)"; digest_tmp="$(mktemp .restart-recovery-live-proof.sha256.XXXXXX)"
  chmod 600 "$receipt_tmp" "$digest_tmp"
  printf '{"schema":"%s","category":"%s","origin":"%s","source_head_commit":"%s","source_tree_id":"%s","cargo_executable_sha256":"%s","windows_cargo_executable_sha256":"%s","windows_rustc_executable_sha256":"%s","windows_msvc_environment_sha256":"%s","service_executable_sha256":"%s","rebuilt_service_executable_sha256":"%s","frozen_database_sha256":"%s","continuity_sha256":"%s","activation_evidence_sha256":"%s","migration_backups_sha256":"%s","controller_definition_sha256":"%s","harness_definition_sha256":"%s","windows_control_definition_sha256":"%s","proof_transcript_sha256":"%s","proof_identity":"%s","observed_at_ms":%s,"status":"passed","proof_boundary":"%s"}\n' \
    "$SCHEMA" "$CATEGORY" "$ORIGIN" "$source" "$tree" "$cargo_executable_digest" "$windows_cargo_executable_digest" "$windows_rustc_executable_digest" "$windows_msvc_environment_digest" "$service_executable_digest" "$rebuilt_service_executable_digest" "$frozen_database_digest" "$continuity_digest" "$activation_evidence_digest" "$migration_backups_digest" "$controller" "$harness" "$windows" "$transcript" "$PROOF_IDENTITY" "$observed" "$PROOF_BOUNDARY" >"$receipt_tmp"
  bytes="$(wc -c <"$receipt_tmp" | tr -d '[:space:]')"; [[ "$bytes" -le 4096 ]] || fail "proof receipt was oversized"
  digest="$(sha256_file "$receipt_tmp")"; valid_sha "$digest" || fail "receipt digest was invalid"
  printf '%s\n' "$digest" >"$digest_tmp"
  mv -f -- "$digest_tmp" "$DIGEST_NAME"; mv -f -- "$receipt_tmp" "$RECEIPT_NAME"
}

run_controller() (
  PATH="/usr/bin:/bin:/usr/sbin:/sbin"; export PATH
  local root="$(cd "$1" && pwd -P)" transcript fifo transcript_digest observed published=0
  local cargo_executable_digest="" windows_cargo_executable_digest="" windows_rustc_executable_digest="" windows_msvc_environment_digest="" service_executable_digest="" rebuilt_service_executable_digest="" frozen_database_digest="" continuity_digest="" activation_evidence_digest="" migration_backups_digest=""
  local output_prepared=0 target_identity="" output_identity="" live_pid="" receipt_writer_open=0
  local before_branch before_head before_origin before_tree before_status before_controller_digest before_harness_digest before_windows_digest
  local after_branch after_head after_origin after_tree after_status after_controller_digest after_harness_digest after_windows_digest
  cleanup() {
    [[ "$receipt_writer_open" -eq 0 ]] || exec 4>&- || true
    restore_receipt_terminal || true; terminate_live_group
    rm -f -- "${fifo:-}" "${transcript:-}" 2>/dev/null || true
    if [[ "$output_prepared" -eq 1 && "$published" -ne 1 ]]; then rm -f -- "$RECEIPT_NAME" "$DIGEST_NAME" .restart-recovery-live-proof.* 2>/dev/null || true; fi
  }
  trap cleanup EXIT
  trap 'exit 129' HUP; trap 'exit 130' INT; trap 'exit 143' TERM
  prepare_output "$root"
  validate_no_cargo_configs "$root"
  [[ "$fixture_mode" -eq 1 ]] || validate_fixed_cargo
  capture_source_state "$root" before
  transcript="$(mktemp .restart-recovery-transcript.XXXXXX)"; fifo="$(mktemp -u .restart-recovery-receipt.XXXXXX)"
  mkfifo -m 600 "$fifo"; chmod 600 "$transcript"
  run_committed_harness "$root" "$before_head" "$transcript" "$fifo"
  validate_transcript "$transcript" "$before_head"
  capture_source_state "$root" after
  for name in branch head origin tree status controller_digest harness_digest windows_digest; do
    eval '[[ "$before_'"$name"'" == "$after_'"$name"'" ]]' || fail "source checkout or proof definition changed during live proof"
  done
  revalidate_output "$root"
  transcript_digest="$(sha256_file "$transcript")"; valid_sha "$transcript_digest" || fail "transcript digest was invalid"
  observed="$(date -u '+%s')000"; [[ "$observed" =~ ^[0-9]{13}$ ]] || fail "observed time was invalid"
  write_receipt "$before_head" "$before_tree" "$before_controller_digest" "$before_harness_digest" "$before_windows_digest" "$transcript_digest" "$observed"
  revalidate_output "$root"; validate_output_file "$RECEIPT_NAME"; validate_output_file "$DIGEST_NAME"
  [[ "$(tr -d '[:space:]' <"$DIGEST_NAME")" == "$(sha256_file "$RECEIPT_NAME")" ]] || fail "published proof pair did not match"
  published=1; rm -f -- "$fifo" "$transcript"; trap - EXIT HUP INT TERM
  printf 'Assemblywright restart-recovery live proof controller: passed\nReceipt: %s/%s\nReceipt SHA-256: %s/%s\nProof boundary: %s\n' \
    "$OUTPUT_RELATIVE" "$RECEIPT_NAME" "$OUTPUT_RELATIVE" "$DIGEST_NAME" "$PROOF_BOUNDARY"
)

check_controller() {
  local command_name
  for command_name in shasum awk mktemp mkfifo date id stat chmod mkdir rmdir mv rm wc tr grep tee env sleep python3 stty expect; do require_command "$command_name"; done
  for path in "$CONTROLLER_PATH" "$HARNESS_PATH" "$WINDOWS_PATH"; do [[ -f "$ROOT_DIR/$path" && ! -L "$ROOT_DIR/$path" ]] || fail "fixed proof definition is unavailable: $path"; done
  validate_system_git_portable
  git_safe "$ROOT_DIR" check-ignore -q "$OUTPUT_RELATIVE/$RECEIPT_NAME" || fail "fixed proof output is not ignored"
  grep -Fq 'restart-recovery-proof-controller.sh --check' "$ROOT_DIR/scripts/release-local.sh" || fail "release-local omits the controller check"
  grep -Fq 'restart-recovery-proof-controller.sh --self-test' "$ROOT_DIR/scripts/release-local.sh" || fail "release-local omits the controller self-test"
  grep -Fq 'authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup' "$ROOT_DIR/$HARNESS_PATH" || fail "harness omits the exact real-agent restart E2E"
  grep -Fq 'validate_no_cargo_configs' "$ROOT_DIR/$HARNESS_PATH" || fail "harness omits Cargo configuration containment"
  grep -Fq 'export RUSTC_WRAPPER=""' "$ROOT_DIR/$HARNESS_PATH" || fail "harness omits explicit rustc-wrapper clearing"
  grep -Fq '[System.Collections.IDictionary]' "$ROOT_DIR/$WINDOWS_PATH" || fail "Windows JSON validation is not dictionary-safe"
  validate_windows_contract
  "$ROOT_DIR/$HARNESS_PATH" --check >/dev/null
  printf 'Assemblywright restart-recovery proof controller check: ok\nProof boundary: static prerequisites only; no process or service restart ran and no receipt was created.\n'
}

initialize_fixture() {
  local fixture="$1" body="$2" bare="$3"
  mkdir -p "$fixture/scripts"; "$FIXED_GIT_PATH" init -q --bare "$bare"
  git_safe "$fixture" init -q; git_safe "$fixture" checkout -q -b main
  git_safe "$fixture" config user.name 'Assemblywright Restart Recovery Self Test'; git_safe "$fixture" config user.email 'restart-recovery-self-test@invalid.example'
  printf 'target/\n' >"$fixture/.gitignore"; printf '%s\n' "$body" >"$fixture/$HARNESS_PATH"
  printf 'fixture controller\n' >"$fixture/$CONTROLLER_PATH"; printf '[System.Collections.IDictionary]\n' >"$fixture/$WINDOWS_PATH"
  git_safe "$fixture" add .; git_safe "$fixture" commit -q -m fixture
  git_safe "$fixture" remote add origin "$bare"; git_safe "$fixture" push -q -u origin main
  "$FIXED_GIT_PATH" --git-dir="$bare" symbolic-ref HEAD refs/heads/main
}

fixture_harness() {
  cat <<'FIXTURE'
#!/usr/bin/env bash
set -euo pipefail
[[ "$ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH" == "/Users/michaelnobile/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/cargo" ]]
[[ "$ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256" == "c512bff73c86143b557463f021d0c3d5b0490d97d65040ba59ea2b3427784758" ]]
[[ "$ASSEMBLYWRIGHT_RESTART_RECOVERY_RUSTC_PATH" == "/Users/michaelnobile/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/rustc" ]]
[[ "$ASSEMBLYWRIGHT_RESTART_RECOVERY_RUSTC_SHA256" == "b829b733131d4e1673eeebd1f34d06ae1e9ff4977b051313cf42e2a9e79ecf1c" ]]
[[ "$ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_HOME" == "/Users/michaelnobile/.cargo" ]]
[[ "$PATH" == "/usr/bin:/bin:/usr/sbin:/sbin" ]]
printf 'test authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup ... ok\n'
printf 'assemblywright_restart_recovery_windows_run_required action=Run confirm=true expected_source_head=%s receipt_fd=3\n' "$ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD"
IFS= read -r -u 3 receipt
printf 'assemblywright_restart_recovery_live_e2e_ok cargo_executable_sha256=%s source_head=%s protocol_version=5 master_schema_version=19 service_executable_sha256=%064d rebuilt_service_executable_sha256=%064d windows_cargo_executable_sha256=dc19c8e6d66802d120bf0696b1924b748bd90f3ca16f21391e54a290ff12b7c5 windows_rustc_executable_sha256=e3ebbd547ea7b73c034d588ba569602b379f3b05ad1a3b5f8dcfab9d4478d74a windows_msvc_environment_sha256=6b516d8fcf543c14b2d861e1f45661e0029230fe0dc48e86ce78522801822209 frozen_database_sha256=%064d pre_process_id=10 post_process_id=11 queue_revision=0 emergency_pause_revision=0 owner_control_designation_revision=0 activation_status=inactive activation_evidence_sha256=%064d migration_backup_count=1 migration_backups_sha256=%064d continuity_sha256=%064d observed_at_ms=%s\n' "$ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256" "$ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD" 1 6 2 3 4 5 "$(date -u +%s)000"
FIXTURE
}

assert_no_proof() {
  local fixture="$1"; [[ ! -e "$fixture/$OUTPUT_RELATIVE/$RECEIPT_NAME" && ! -e "$fixture/$OUTPUT_RELATIVE/$DIGEST_NAME" ]] || fail "rejected fixture retained proof"
}

self_test_controller() {
  local scratch success configured harness_config dirty wrong hidden hostile stale receipt digest bare cancellation cancellation_ready cancellation_survived controller_pid controller_status wait_count
  local fake_bin valid_windows_receipt legacy_windows_receipt missing_rebuilt_receipt harness_output oversized_windows_receipt
  local pty_reader pty_expect long_receipt long_receipt_digest oversized_receipt
  fixture_mode=1
  export ASSEMBLYWRIGHT_RESTART_RECOVERY_RUSTC_PATH="$FIXED_RUSTC_PATH"
  export ASSEMBLYWRIGHT_RESTART_RECOVERY_RUSTC_SHA256="$FIXED_RUSTC_SHA256"
  export ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_HOME="$FIXED_CARGO_HOME"
  scratch="$(mktemp -d -t assemblywright-restart-recovery-proof)"; chmod 700 "$scratch"; trap 'rm -rf -- "$scratch"' RETURN
  validate_windows_contract
  if "$ROOT_DIR/$CONTROLLER_PATH" >/dev/null 2>&1 || "$ROOT_DIR/$CONTROLLER_PATH" --unknown >/dev/null 2>&1 ||
     "$ROOT_DIR/$CONTROLLER_PATH" --check extra >/dev/null 2>&1 || "$ROOT_DIR/$HARNESS_PATH" --check extra >/dev/null 2>&1; then
    fail "strict controller or harness CLI accepted an unsupported shape"
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
    read_live_receipt receipt assemblywright_restart_recovery_pty_ready
    [[ "$(stty -g <&0)" == "$before_state" ]]
    receipt_digest="$(printf '%s' "$receipt" | shasum -a 256 | awk '{print $1}')"
    printf 'assemblywright_restart_recovery_pty_ok bytes=%s sha256=%s terminal_restored=verified\n' "${#receipt}" "$receipt_digest"
    ;;
  oversized)
    if read_live_receipt receipt assemblywright_restart_recovery_pty_ready; then exit 41; fi
    [[ "$(stty -g <&0)" == "$before_state" && "$receipt_read_error" == oversized ]]
    printf 'assemblywright_restart_recovery_pty_oversized_rejected bytes=%s error=%s terminal_restored=verified\n' "${#receipt}" "$receipt_read_error"
    ;;
  signal)
    handle_term() {
      restore_receipt_terminal
      [[ "$(stty -g <&0)" == "$before_state" ]]
      printf 'assemblywright_restart_recovery_pty_signal_restored signal=TERM terminal_restored=verified\n'
      exit 143
    }
    trap handle_term TERM
    read_live_receipt receipt assemblywright_restart_recovery_pty_ready
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
  -re {assemblywright_restart_recovery_pty_ready} {}
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
  [[ "${#long_receipt}" -eq 2182 ]] || fail "PTY success fixture had the wrong length"
  long_receipt_digest="$(printf '%s' "$long_receipt" | sha256_stdin)"
  env PTY_MODE=success PTY_READER="$pty_reader" PTY_RECEIPT="$long_receipt" \
    PTY_EXPECTED_MARKER="assemblywright_restart_recovery_pty_ok bytes=2182 sha256=$long_receipt_digest terminal_restored=verified" \
    expect "$pty_expect" >/dev/null || fail "PTY success/terminal restoration failed"
  oversized_receipt='{"receipt":"'
  while [[ "${#oversized_receipt}" -lt 8998 ]]; do oversized_receipt+="y"; done
  oversized_receipt+='"}'
  [[ "${#oversized_receipt}" -eq 9000 ]] || fail "PTY oversized fixture had the wrong length"
  env PTY_MODE=oversized PTY_READER="$pty_reader" PTY_RECEIPT="$oversized_receipt" \
    PTY_EXPECTED_MARKER='assemblywright_restart_recovery_pty_oversized_rejected bytes=8192 error=oversized terminal_restored=verified' \
    expect "$pty_expect" >/dev/null || fail "PTY oversized drain/rejection failed"
  env PTY_MODE=signal PTY_READER="$pty_reader" PTY_RECEIPT='' \
    PTY_EXPECTED_MARKER='assemblywright_restart_recovery_pty_signal_restored signal=TERM terminal_restored=verified' \
    expect "$pty_expect" >/dev/null || fail "PTY signal terminal restoration failed"
  fake_bin="$scratch/fake-bin"; mkdir -m 700 "$fake_bin"
  cat <<'FAKE_CARGO' >"$fake_bin/cargo"
#!/usr/bin/env bash
printf forged >"${ASSEMBLYWRIGHT_HOSTILE_CARGO_SENTINEL:?}"
printf 'test authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup ... ok\n'
FAKE_CARGO
  chmod 700 "$fake_bin/cargo"
  cat <<'FAKE_GIT' >"$fake_bin/git"
#!/bin/bash
printf forged >"${ASSEMBLYWRIGHT_HOSTILE_GIT_SENTINEL:?}"
exit 0
FAKE_GIT
  chmod 700 "$fake_bin/git"
  for hostile_name in bash python3 grep; do
    cat <<'FAKE_SYSTEM_TOOL' >"$fake_bin/$hostile_name"
#!/bin/bash
printf forged >"${ASSEMBLYWRIGHT_HOSTILE_SYSTEM_TOOL_SENTINEL:?}"
exit 99
FAKE_SYSTEM_TOOL
    chmod 700 "$fake_bin/$hostile_name"
  done
  PATH="$fake_bin:$PATH" ASSEMBLYWRIGHT_HOSTILE_SYSTEM_TOOL_SENTINEL="$scratch/hostile-harness-system-tool-ran" \
    /bin/bash "$ROOT_DIR/$HARNESS_PATH" --check >/dev/null \
    || fail "strict harness check failed under a hostile caller PATH"
  [[ ! -e "$scratch/hostile-harness-system-tool-ran" ]] || fail "hostile caller PATH replaced a fixed harness system tool"
  valid_windows_receipt='{"schema_version":2,"status":"restart_recovery_windows_live_passed","source_head":"0000000000000000000000000000000000000000","protocol_version":5,"master_schema_version":19,"service_executable_sha256":"1111111111111111111111111111111111111111111111111111111111111111","rebuilt_service_executable_sha256":"5555555555555555555555555555555555555555555555555555555555555555","windows_cargo_executable_sha256":"dc19c8e6d66802d120bf0696b1924b748bd90f3ca16f21391e54a290ff12b7c5","windows_rustc_executable_sha256":"e3ebbd547ea7b73c034d588ba569602b379f3b05ad1a3b5f8dcfab9d4478d74a","windows_msvc_environment_sha256":"6b516d8fcf543c14b2d861e1f45661e0029230fe0dc48e86ce78522801822209","frozen_database_sha256":"6666666666666666666666666666666666666666666666666666666666666666","pre_process_id":10,"post_process_id":11,"queue_revision":0,"emergency_pause_revision":0,"owner_control_designation_revision":0,"activation_status":"inactive","activation_evidence_sha256":"2222222222222222222222222222222222222222222222222222222222222222","migration_backup_count":0,"migration_backups_sha256":"3333333333333333333333333333333333333333333333333333333333333333","continuity_sha256":"4444444444444444444444444444444444444444444444444444444444444444","observed_at_ms":'"$(date -u +%s)"'000}'
  harness_config="$scratch/harness-config"; mkdir -p "$harness_config/.cargo"; printf '[build]\nrustc-wrapper="forged"\n' >"$harness_config/.cargo/config.toml"
  if (cd "$harness_config" && ASSEMBLYWRIGHT_RESTART_RECOVERY_INTERNAL_STDIN_V2="$SCHEMA" ASSEMBLYWRIGHT_RESTART_RECOVERY_VALIDATION_SELF_TEST_V2="$SCHEMA" \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_RECEIPT_FD=3 ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD=0000000000000000000000000000000000000000 \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH="$FIXED_CARGO_PATH" ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256="$FIXED_CARGO_SHA256" \
    /bin/bash "$ROOT_DIR/$HARNESS_PATH" --run 3<<<"$valid_windows_receipt" >/dev/null 2>&1); then
    fail "strict harness accepted an ancestor Cargo configuration"
  fi
  if ASSEMBLYWRIGHT_RESTART_RECOVERY_INTERNAL_STDIN_V2="$SCHEMA" ASSEMBLYWRIGHT_RESTART_RECOVERY_VALIDATION_SELF_TEST_V2="$SCHEMA" \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_RECEIPT_FD=3 ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD=0000000000000000000000000000000000000000 \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH=/Users/runner/forbidden-owner-cargo ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256="$FIXED_CARGO_SHA256" \
    /bin/bash "$ROOT_DIR/$HARNESS_PATH" --run 3<<<"$valid_windows_receipt" >/dev/null 2>&1; then
    fail "strict harness accepted an unavailable non-owner Cargo path"
  fi
  if fixed_cargo_available; then
  harness_output="$(PATH="$fake_bin:$PATH" ASSEMBLYWRIGHT_HOSTILE_CARGO_SENTINEL="$scratch/hostile-cargo-ran" \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_INTERNAL_STDIN_V2="$SCHEMA" ASSEMBLYWRIGHT_RESTART_RECOVERY_VALIDATION_SELF_TEST_V2="$SCHEMA" \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_RECEIPT_FD=3 ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD=0000000000000000000000000000000000000000 \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH="$FIXED_CARGO_PATH" ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256="$FIXED_CARGO_SHA256" \
    /bin/bash "$ROOT_DIR/$HARNESS_PATH" --run 3<<<"$valid_windows_receipt")" \
    || fail "strict harness rejected its exact valid receipt fixture"
  [[ "$harness_output" == *'assemblywright_restart_recovery_live_e2e_ok '* ]] || fail "strict harness omitted success"
  [[ ! -e "$scratch/hostile-cargo-ran" ]] || fail "hostile caller PATH forged the native Cargo result"
  legacy_windows_receipt="${valid_windows_receipt/\"schema_version\":2/\"schema_version\":1}"
  if ASSEMBLYWRIGHT_RESTART_RECOVERY_INTERNAL_STDIN_V2="$SCHEMA" ASSEMBLYWRIGHT_RESTART_RECOVERY_VALIDATION_SELF_TEST_V2="$SCHEMA" \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_RECEIPT_FD=3 ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD=0000000000000000000000000000000000000000 \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH="$FIXED_CARGO_PATH" ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256="$FIXED_CARGO_SHA256" \
    /bin/bash "$ROOT_DIR/$HARNESS_PATH" --run 3<<<"$legacy_windows_receipt" >/dev/null 2>&1; then
    fail "strict harness accepted a schema-v1 Windows receipt"
  fi
  missing_rebuilt_receipt="${valid_windows_receipt/,\"rebuilt_service_executable_sha256\":\"5555555555555555555555555555555555555555555555555555555555555555\"/}"
  if ASSEMBLYWRIGHT_RESTART_RECOVERY_INTERNAL_STDIN_V2="$SCHEMA" ASSEMBLYWRIGHT_RESTART_RECOVERY_VALIDATION_SELF_TEST_V2="$SCHEMA" \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_RECEIPT_FD=3 ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD=0000000000000000000000000000000000000000 \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH="$FIXED_CARGO_PATH" ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256="$FIXED_CARGO_SHA256" \
    /bin/bash "$ROOT_DIR/$HARNESS_PATH" --run 3<<<"$missing_rebuilt_receipt" >/dev/null 2>&1; then
    fail "strict harness accepted Windows evidence without the transient rebuild digest"
  fi
  if PATH="$fake_bin:$PATH" ASSEMBLYWRIGHT_RESTART_RECOVERY_INTERNAL_STDIN_V2="$SCHEMA" ASSEMBLYWRIGHT_RESTART_RECOVERY_VALIDATION_SELF_TEST_V2="$SCHEMA" \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_RECEIPT_FD=3 ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD=0000000000000000000000000000000000000000 \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH="$FIXED_CARGO_PATH" ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256="$FIXED_CARGO_SHA256" \
    /bin/bash "$ROOT_DIR/$HARNESS_PATH" --run 3<<<"${valid_windows_receipt%\}},\"source_head\":\"/private/leak\"}" >/dev/null 2>&1; then
    fail "strict harness accepted duplicate/path-bearing Windows evidence"
  fi
  oversized_windows_receipt="$(/usr/bin/python3 -c 'print("x" * 9000, end="")')"
  if PATH="$fake_bin:$PATH" ASSEMBLYWRIGHT_RESTART_RECOVERY_INTERNAL_STDIN_V2="$SCHEMA" ASSEMBLYWRIGHT_RESTART_RECOVERY_VALIDATION_SELF_TEST_V2="$SCHEMA" \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_RECEIPT_FD=3 ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD=0000000000000000000000000000000000000000 \
    ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH="$FIXED_CARGO_PATH" ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256="$FIXED_CARGO_SHA256" \
    /bin/bash "$ROOT_DIR/$HARNESS_PATH" --run 3<<<"$oversized_windows_receipt" >/dev/null 2>&1; then
    fail "strict harness accepted oversized Windows evidence"
  fi
  else
    if PATH="$fake_bin:$PATH" ASSEMBLYWRIGHT_HOSTILE_CARGO_SENTINEL="$scratch/hostile-cargo-ran" \
      ASSEMBLYWRIGHT_RESTART_RECOVERY_INTERNAL_STDIN_V2="$SCHEMA" ASSEMBLYWRIGHT_RESTART_RECOVERY_VALIDATION_SELF_TEST_V2="$SCHEMA" \
      ASSEMBLYWRIGHT_RESTART_RECOVERY_RECEIPT_FD=3 ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD=0000000000000000000000000000000000000000 \
      ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH="$FIXED_CARGO_PATH" ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256="$FIXED_CARGO_SHA256" \
      /bin/bash "$ROOT_DIR/$HARNESS_PATH" --run 3<<<"$valid_windows_receipt" >/dev/null 2>&1; then
      fail "strict harness accepted an unavailable owner-only Cargo identity"
    fi
    [[ ! -e "$scratch/hostile-cargo-ran" ]] || fail "hostile caller PATH substituted for unavailable owner-only Cargo"
  fi
  success="$scratch/success"; bare="$scratch/success.git"; initialize_fixture "$success" "$(fixture_harness)" "$bare"
  PATH="$fake_bin:$PATH" ASSEMBLYWRIGHT_HOSTILE_CARGO_SENTINEL="$scratch/hostile-controller-cargo-ran" ASSEMBLYWRIGHT_HOSTILE_GIT_SENTINEL="$scratch/hostile-controller-git-ran" ASSEMBLYWRIGHT_HOSTILE_SYSTEM_TOOL_SENTINEL="$scratch/hostile-controller-system-tool-ran" \
    run_controller "$success" < <(printf '{}\n') >/dev/null
  [[ ! -e "$scratch/hostile-controller-cargo-ran" ]] || fail "hostile caller PATH forged the controller Cargo result"
  [[ ! -e "$scratch/hostile-controller-git-ran" ]] || fail "hostile caller PATH forged committed Git evidence"
  [[ ! -e "$scratch/hostile-controller-system-tool-ran" ]] || fail "hostile caller PATH replaced committed harness execution tools"
  receipt="$success/$OUTPUT_RELATIVE/$RECEIPT_NAME"; digest="$success/$OUTPUT_RELATIVE/$DIGEST_NAME"
  [[ -f "$receipt" && -f "$digest" && "$(stat -f '%Lp' "$receipt")" == 600 && "$(stat -f '%Lp' "$digest")" == 600 ]] || fail "success fixture did not publish private proof"
  [[ "$(sha256_file "$receipt")" == "$(tr -d '[:space:]' <"$digest")" ]] || fail "success proof pair did not match"
  grep -Fq '"category":"restart_recovery_live","origin":"restart_recovery_proof_controller"' "$receipt" || fail "success proof omitted activation binding"
  if grep -Eq '(/Users/|/private/|/tmp/|[A-Za-z]:\\|https?://|github\.com|token|credential)' "$receipt"; then fail "self-test receipt leaked a path, remote, or credential"; fi
  [[ -z "$(find "$success/$OUTPUT_RELATIVE" -maxdepth 1 -name '*transcript*' -print)" ]] || fail "private transcript was retained"
  configured="$scratch/configured"; initialize_fixture "$configured" "$(fixture_harness)" "$scratch/configured.git"
  mkdir -p "$scratch/.cargo"; printf '[build]\nrustc-wrapper="forged"\n' >"$scratch/.cargo/config.toml"
  if printf '{}\n' | run_controller "$configured" >/dev/null 2>&1; then fail "ancestor Cargo configuration was accepted"; fi
  assert_no_proof "$configured"; rm -f -- "$scratch/.cargo/config.toml"; rmdir "$scratch/.cargo"
  dirty="$scratch/dirty"; initialize_fixture "$dirty" "$(fixture_harness)" "$scratch/dirty.git"; printf dirty >>"$dirty/$WINDOWS_PATH"
  mkdir -p "$dirty/$OUTPUT_RELATIVE"; chmod 700 "$dirty/target" "$dirty/$OUTPUT_RELATIVE"; printf old >"$dirty/$OUTPUT_RELATIVE/$RECEIPT_NAME"; printf old >"$dirty/$OUTPUT_RELATIVE/$DIGEST_NAME"; chmod 600 "$dirty/$OUTPUT_RELATIVE/"*
  if printf '{}\n' | run_controller "$dirty" >/dev/null 2>&1; then fail "dirty fixture was accepted"; fi; assert_no_proof "$dirty"
  wrong="$scratch/wrong"; initialize_fixture "$wrong" "$(fixture_harness)" "$scratch/wrong.git"; git_safe "$wrong" checkout -q -b other
  if printf '{}\n' | run_controller "$wrong" >/dev/null 2>&1; then fail "wrong branch fixture was accepted"; fi; assert_no_proof "$wrong"
  hidden="$scratch/hidden"; initialize_fixture "$hidden" "$(fixture_harness)" "$scratch/hidden.git"; git_safe "$hidden" update-index --skip-worktree "$HARNESS_PATH"
  if printf '{}\n' | run_controller "$hidden" >/dev/null 2>&1; then fail "hidden index fixture was accepted"; fi; assert_no_proof "$hidden"
  hostile="$scratch/hostile"; initialize_fixture "$hostile" "$(fixture_harness)" "$scratch/hostile.git"; mkdir -m 700 "$hostile/target"; ln -s "$scratch" "$hostile/target/restart-recovery-live-proof"
  if printf '{}\n' | run_controller "$hostile" >/dev/null 2>&1; then fail "hostile output fixture was accepted"; fi; assert_no_proof "$hostile"
  stale="$scratch/stale"; initialize_fixture "$stale" "$(fixture_harness)" "$scratch/stale.git"; "$FIXED_GIT_PATH" -c protocol.file.allow=always clone -q "$scratch/stale.git" "$scratch/stale-publish"
  git_safe "$scratch/stale-publish" config user.name Fixture; git_safe "$scratch/stale-publish" config user.email fixture@invalid.example; git_safe "$scratch/stale-publish" commit -q --allow-empty -m remote-advanced; git_safe "$scratch/stale-publish" push -q origin HEAD:main
  git_safe "$stale" fetch -q origin '+refs/heads/main:refs/remotes/origin/main'
  if printf '{}\n' | run_controller "$stale" >/dev/null 2>&1; then fail "stale main fixture was accepted"; fi; assert_no_proof "$stale"
  cancellation="$scratch/cancellation"; cancellation_ready="$scratch/cancellation-ready"; cancellation_survived="$scratch/cancellation-survived"
  initialize_fixture "$cancellation" \
    "printf '%s\\n' 'test authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup ... ok'"$'\n'\
"printf '%s\\n' 'assemblywright_restart_recovery_windows_run_required action=Run confirm=true expected_source_head=0000000000000000000000000000000000000000 receipt_fd=3'"$'\n'\
"IFS= read -r -u 3 receipt"$'\n'\
"printf ready >'$cancellation_ready'"$'\n'\
"(sleep 10; printf survived >'$cancellation_survived') & wait" "$scratch/cancellation.git"
  run_controller "$cancellation" < <(printf '{}\n') >/dev/null 2>&1 & controller_pid="$!"; wait_count=0
  while [[ ! -e "$cancellation_ready" ]]; do wait_count=$((wait_count + 1)); [[ "$wait_count" -lt 50 ]] || { kill -TERM "$controller_pid" >/dev/null 2>&1 || true; fail "cancellation fixture did not start"; }; sleep 0.1; done
  kill -TERM "$controller_pid"; set +e; wait "$controller_pid" >/dev/null 2>&1; controller_status="$?"; set -e
  [[ "$controller_status" -ne 0 ]]; sleep 1; [[ ! -e "$cancellation_survived" ]] || fail "cancellation left a live descendant"; assert_no_proof "$cancellation"
  printf 'Assemblywright restart-recovery proof controller self-test: ok\nCovered: portable owner-Cargo absence, Cargo-home/ancestor-config and wrapper rejection, strict CLI and valid/malformed/path-bearing/oversized Windows receipts, schema-v1 and missing-rebuild-digest rejection, fixed Cargo/rustc and hostile Git/Cargo/Bash/Python/grep PATH rejection, real-PTY success/oversize/signal restoration, raw digest pairing, private transcript removal, stale invalidation, dirty/wrong/stale/hidden source rejection, hostile output rejection, cancellation process-group cleanup, and path-free receipt boundaries.\n'
}

[[ "$#" -eq 1 ]] || { usage >&2; exit 2; }
case "$1" in
  --check) check_controller;;
  --run) fixture_mode=0; check_controller >/dev/null; validate_fixed_git_live; validate_fixed_cargo; run_controller "$ROOT_DIR";;
  --self-test) check_controller >/dev/null; self_test_controller;;
  -h|--help) usage;;
  *) usage >&2; exit 2;;
esac
