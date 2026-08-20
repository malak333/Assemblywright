#!/bin/bash
set -euo pipefail
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
export PATH

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

FIXED_CARGO_PATH="/Users/michaelnobile/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/cargo"
FIXED_CARGO_SHA256="c512bff73c86143b557463f021d0c3d5b0490d97d65040ba59ea2b3427784758"
FIXED_RUSTC_PATH="/Users/michaelnobile/.rustup/toolchains/1.95.0-aarch64-apple-darwin/bin/rustc"
FIXED_RUSTC_SHA256="b829b733131d4e1673eeebd1f34d06ae1e9ff4977b051313cf42e2a9e79ecf1c"
FIXED_CARGO_HOME="/Users/michaelnobile/.cargo"

validate_system_tool() {
  local path="$1" mode="$2"
  [[ -f "$path" && ! -L "$path" ]] || fail "a fixed system proof tool is unavailable"
  [[ "$(/usr/bin/stat -f '%u:%Lp:%HT' "$path")" == "0:${mode}:Regular File" ]] \
    || fail "a fixed system proof tool identity is invalid"
  [[ "$(cd "${path%/*}" && pwd -P)/${path##*/}" == "$path" ]] \
    || fail "a fixed system proof tool canonical identity is invalid"
}

validate_system_tool /bin/bash 555
validate_system_tool /usr/bin/python3 755
validate_system_tool /usr/bin/grep 755

clear_build_environment() {
  local name
  while IFS='=' read -r name _; do
    case "$name" in
      CARGO_*|RUST*|CC|CXX|AR|AS|CPP|CFLAGS|CXXFLAGS|CPPFLAGS|LDFLAGS|SDKROOT|DEVELOPER_DIR|MACOSX_DEPLOYMENT_TARGET|MAKEFLAGS) unset "$name";;
    esac
  done < <(/usr/bin/env)
}

validate_no_cargo_configs() {
  local current parent config
  current="$(pwd -P)"
  while :; do
    for config in "$current/.cargo/config" "$current/.cargo/config.toml"; do
      [[ ! -e "$config" && ! -L "$config" ]] || fail "Cargo configuration is forbidden in the proof checkout or its ancestors"
    done
    [[ "$current" == / ]] && break
    parent="${current%/*}"; [[ -n "$parent" ]] || parent=/
    current="$parent"
  done
  for config in "$FIXED_CARGO_HOME/config" "$FIXED_CARGO_HOME/config.toml"; do
    [[ ! -e "$config" && ! -L "$config" ]] || fail "fixed Cargo home contains a build-affecting configuration"
  done
}

if [[ "${1:---check}" == "--check" ]]; then
  [[ "$#" -le 1 ]] || fail "the restart-recovery harness accepts no extra arguments"
  printf 'Assemblywright restart-recovery live E2E harness check: ok\n'
  printf 'Proof boundary: static harness shape only; no process or service restart ran.\n'
  exit 0
fi

[[ "$#" -eq 1 && "$1" == "--run" ]] \
  || fail "the restart-recovery harness accepts only --check or internal --run"
[[ "${ASSEMBLYWRIGHT_RESTART_RECOVERY_INTERNAL_STDIN_V1:-}" == \
  "assemblywright.restart-recovery-live-proof.v1" ]] \
  || fail "live execution requires the fixed proof controller"
[[ "${ASSEMBLYWRIGHT_RESTART_RECOVERY_RECEIPT_FD:-}" == "3" ]] \
  || fail "live execution requires the isolated receipt descriptor"
[[ "${ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD:-}" =~ ^[0-9a-f]{40}$ ]] \
  || fail "the expected source commit is unavailable"
[[ "${ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_PATH:-}" == "$FIXED_CARGO_PATH" ]] \
  || fail "the fixed Cargo path binding is unavailable"
[[ "${ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256:-}" == "$FIXED_CARGO_SHA256" ]] \
  || fail "the fixed Cargo digest binding is unavailable"
[[ "${ASSEMBLYWRIGHT_RESTART_RECOVERY_RUSTC_PATH:-}" == "$FIXED_RUSTC_PATH" ]] \
  || fail "the fixed rustc path binding is unavailable"
[[ "${ASSEMBLYWRIGHT_RESTART_RECOVERY_RUSTC_SHA256:-}" == "$FIXED_RUSTC_SHA256" ]] \
  || fail "the fixed rustc digest binding is unavailable"
[[ "${ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_HOME:-}" == "$FIXED_CARGO_HOME" ]] \
  || fail "the fixed Cargo home binding is unavailable"
validate_no_cargo_configs
clear_build_environment
export CARGO_HOME="$FIXED_CARGO_HOME"
export RUSTC="$FIXED_RUSTC_PATH"
export RUSTC_WRAPPER=""
export RUSTC_WORKSPACE_WRAPPER=""
[[ -f "$FIXED_CARGO_PATH" && ! -L "$FIXED_CARGO_PATH" ]] \
  || fail "the fixed Cargo executable is not an ordinary file"
[[ "$(/usr/bin/stat -f '%u:%Lp:%l:%HT' "$FIXED_CARGO_PATH")" == \
  "$(/usr/bin/id -u):755:1:Regular File" ]] \
  || fail "the fixed Cargo executable identity is invalid"
cargo_sha256="$(/usr/bin/shasum -a 256 "$FIXED_CARGO_PATH" | /usr/bin/awk '{print $1}')"
[[ "$cargo_sha256" == "$FIXED_CARGO_SHA256" ]] \
  || fail "the fixed Cargo executable digest drifted"
[[ -f "$FIXED_RUSTC_PATH" && ! -L "$FIXED_RUSTC_PATH" ]] \
  || fail "the fixed rustc executable is not an ordinary file"
[[ "$(/usr/bin/stat -f '%u:%Lp:%l:%HT' "$FIXED_RUSTC_PATH")" == \
  "$(/usr/bin/id -u):755:1:Regular File" ]] \
  || fail "the fixed rustc executable identity is invalid"
rustc_sha256="$(/usr/bin/shasum -a 256 "$FIXED_RUSTC_PATH" | /usr/bin/awk '{print $1}')"
[[ "$rustc_sha256" == "$FIXED_RUSTC_SHA256" ]] \
  || fail "the fixed rustc executable digest drifted"
[[ -d "$FIXED_CARGO_HOME" && ! -L "$FIXED_CARGO_HOME" ]] \
  || fail "the fixed Cargo home is not an ordinary directory"
[[ "$(/usr/bin/stat -f '%u:%HT' "$FIXED_CARGO_HOME")" == "$(/usr/bin/id -u):Directory" ]] \
  || fail "the fixed Cargo home owner or type is invalid"
[[ "$(cd "$FIXED_CARGO_HOME" && pwd -P)" == "$FIXED_CARGO_HOME" ]] \
  || fail "the fixed Cargo home canonical identity is invalid"
[[ "$((8#$(/usr/bin/stat -f '%Lp' "$FIXED_CARGO_HOME") & 8#022))" -eq 0 ]] \
  || fail "the fixed Cargo home is group or world writable"

test_name="authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup"
if [[ "${ASSEMBLYWRIGHT_RESTART_RECOVERY_VALIDATION_SELF_TEST_V1:-}" == \
  "assemblywright.restart-recovery-live-proof.v1" ]]; then
  native_output="test ${test_name} ... ok"
else
  [[ -z "${ASSEMBLYWRIGHT_RESTART_RECOVERY_VALIDATION_SELF_TEST_V1:-}" ]] \
    || fail "the internal validation self-test marker was invalid"
  native_output="$("$FIXED_CARGO_PATH" test -p assemblywright-agent --test local_relay_e2e \
    "$test_name" -- --exact --nocapture 2>&1)" \
    || { printf '%s\n' "$native_output" >&2; fail "the exact real-agent retained-workspace restart E2E failed"; }
fi
printf '%s\n' "$native_output"
[[ "$(printf '%s\n' "$native_output" | /usr/bin/grep -Ec \
  "^test ${test_name} \.\.\. ok$")" == 1 ]] \
  || fail "the exact real-agent retained-workspace restart E2E result was absent or duplicated"

printf 'assemblywright_restart_recovery_windows_run_required action=Run confirm=true expected_source_head=%s receipt_fd=3\n' \
  "$ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD"
receipt=""
IFS= read -r -u 3 receipt || fail "the sanitized Windows receipt was incomplete"
[[ -n "$receipt" && "${#receipt}" -le 8192 ]] \
  || fail "the sanitized Windows receipt was empty or oversized"

printf '%s\n' "$receipt" | /usr/bin/python3 -c '
import json, os, re, sys, time

def pairs(items):
    result = {}
    for key, value in items:
        if key in result:
            raise ValueError("duplicate key")
        result[key] = value
    return result

raw = sys.stdin.buffer.read(8193)
if not raw or len(raw) > 8192 or b"\n" in raw.rstrip(b"\n"):
    raise SystemExit(1)
try:
    value = json.loads(raw, object_pairs_hook=pairs)
except Exception:
    raise SystemExit(1)
expected = {
    "schema_version", "status", "source_head", "protocol_version",
    "master_schema_version", "service_executable_sha256",
    "windows_cargo_executable_sha256", "windows_rustc_executable_sha256",
    "windows_msvc_environment_sha256", "frozen_database_sha256", "pre_process_id",
    "post_process_id", "queue_revision", "emergency_pause_revision",
    "owner_control_designation_revision", "activation_status",
    "activation_evidence_sha256", "migration_backup_count",
    "migration_backups_sha256", "continuity_sha256", "observed_at_ms",
}
sha = re.compile(r"^[0-9a-f]{64}$")
commit = re.compile(r"^[0-9a-f]{40}$")
now = int(time.time() * 1000)
if not isinstance(value, dict) or set(value) != expected:
    raise SystemExit(1)
if (type(value["schema_version"]) is not int or value["schema_version"] != 1 or
    value["status"] != "restart_recovery_windows_live_passed" or
    value["source_head"] != os.environ["ASSEMBLYWRIGHT_RESTART_RECOVERY_EXPECTED_HEAD"] or
    not commit.fullmatch(value["source_head"]) or
    type(value["protocol_version"]) is not int or value["protocol_version"] != 5 or
    type(value["master_schema_version"]) is not int or value["master_schema_version"] != 19 or
    type(value["pre_process_id"]) is not int or value["pre_process_id"] < 1 or
    type(value["post_process_id"]) is not int or value["post_process_id"] < 1 or
    value["pre_process_id"] == value["post_process_id"] or
    type(value["queue_revision"]) is not int or value["queue_revision"] < 0 or
    type(value["emergency_pause_revision"]) is not int or value["emergency_pause_revision"] < 0 or
    type(value["owner_control_designation_revision"]) is not int or value["owner_control_designation_revision"] < 0 or
    value["activation_status"] not in ("inactive", "active") or
    type(value["migration_backup_count"]) is not int or not 0 <= value["migration_backup_count"] <= 32 or
    type(value["observed_at_ms"]) is not int or value["observed_at_ms"] < now - 3600000 or
    value["observed_at_ms"] > now + 30000):
    raise SystemExit(1)
if (value["windows_cargo_executable_sha256"] != "dc19c8e6d66802d120bf0696b1924b748bd90f3ca16f21391e54a290ff12b7c5" or
    value["windows_rustc_executable_sha256"] != "e3ebbd547ea7b73c034d588ba569602b379f3b05ad1a3b5f8dcfab9d4478d74a" or
    value["windows_msvc_environment_sha256"] != "6b516d8fcf543c14b2d861e1f45661e0029230fe0dc48e86ce78522801822209"):
    raise SystemExit(1)
for key in ("service_executable_sha256", "windows_cargo_executable_sha256",
            "windows_rustc_executable_sha256", "windows_msvc_environment_sha256",
            "frozen_database_sha256", "activation_evidence_sha256",
            "migration_backups_sha256", "continuity_sha256"):
    if not isinstance(value[key], str) or not sha.fullmatch(value[key]):
        raise SystemExit(1)
print("assemblywright_restart_recovery_live_e2e_ok", end="")
print(" cargo_executable_sha256={}".format(os.environ["ASSEMBLYWRIGHT_RESTART_RECOVERY_CARGO_SHA256"]), end="")
for key in ("source_head", "protocol_version", "master_schema_version",
            "service_executable_sha256", "windows_cargo_executable_sha256",
            "windows_rustc_executable_sha256", "windows_msvc_environment_sha256",
            "frozen_database_sha256", "pre_process_id", "post_process_id",
            "queue_revision", "emergency_pause_revision",
            "owner_control_designation_revision", "activation_status",
            "activation_evidence_sha256", "migration_backup_count",
            "migration_backups_sha256", "continuity_sha256", "observed_at_ms"):
    print(f" {key}={value[key]}", end="")
print()
' || fail "the sanitized Windows receipt failed strict validation"
