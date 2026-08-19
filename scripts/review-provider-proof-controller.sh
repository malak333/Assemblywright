#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
OUTPUT_RELATIVE="target/review-provider-live-proof"
RECEIPT_NAME="review-provider-live-proof.json"
DIGEST_NAME="review-provider-live-proof.sha256"
SCHEMA="assemblywright.review-provider-live-proof.v1"
CATEGORY="review_provider_live"
ORIGIN="review_provider_proof_controller"
HARNESS_PATH="scripts/review-provider-live-e2e.sh"
WINDOWS_PATH="scripts/windows-review-provider-live-control.ps1"
CONTROLLER_PATH="scripts/review-provider-proof-controller.sh"
ADAPTER_PATH="crates/assemblywright-master/src/review_provider_adapter.rs"
OUTPUT_SCHEMA_PATH="crates/assemblywright-master/resources/review-output-schema.json"
PROVIDER_ID="openai.codex"
MODEL_ID="gpt-5.6-sol"
PROOF_BOUNDARY="Exact clean main at origin/main used the committed controller, harness, Windows control, adapter, and output-schema definitions to run one fixed approval and one fixed rejection through the selected pinned Codex adapter under the Windows master Job Object; this is semantic sanity proof, not activation evidence admission, general review competence, queue or gateway lifecycle, publication, restart recovery, control streaming, signing, notarization, or production-readiness proof."
unset ASSEMBLYWRIGHT_REVIEW_PROVIDER_INTERNAL_STDIN_V1
unset ASSEMBLYWRIGHT_REVIEW_PROVIDER_RECEIPT_FD
unset ASSEMBLYWRIGHT_REVIEW_PROVIDER_EXPECTED_HEAD

fail() { printf 'error: %s\n' "$1" >&2; exit 1; }

usage() {
  cat <<'USAGE'
Usage: scripts/review-provider-proof-controller.sh [--check | --run | --self-test]

  --check      Validate fixed prerequisites without a provider call.
  --run        Run only the exact committed live harness and write the fixed receipt.
  --self-test  Exercise success and fail-closed controller behavior in fixtures.

The controller accepts no repository, provider, model, executable, schema, or
harness argument. It never admits activation evidence or mutates conveyor state.
USAGE
}

require_command() { command -v "$1" >/dev/null 2>&1 || fail "required command is unavailable: $1"; }

git_safe() {
  local root="$1"; shift
  env -i PATH="$PATH" LC_ALL=C GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null \
    GIT_CONFIG_NOSYSTEM=1 GIT_ATTR_NOSYSTEM=1 GIT_TERMINAL_PROMPT=0 GIT_OPTIONAL_LOCKS=0 \
    git --no-replace-objects -c core.fsmonitor=false -c core.hooksPath=/dev/null \
      -c core.attributesFile=/dev/null -c protocol.file.allow=never -C "$root" "$@"
}

sha256_file() { shasum -a 256 "$1" | awk '{print $1}'; }
sha256_stdin() { shasum -a 256 | awk '{print $1}'; }
valid_sha() { [[ "$1" =~ ^[0-9a-f]{64}$ ]]; }
valid_commit() { [[ "$1" =~ ^[0-9a-f]{40}$ ]]; }
directory_identity() { stat -f '%d:%i' "$1"; }

validate_owner_directory() {
  local path="$1"
  [[ -d "$path" && ! -L "$path" ]] || fail "proof output is not an ordinary directory"
  [[ "$(stat -f '%u' "$path")" == "$(id -u)" ]] || fail "proof output owner is not exact"
  [[ "$(stat -f '%Lp' "$path")" == "700" ]] || fail "proof output directory must have mode 0700"
  [[ "$(stat -f '%HT' "$path")" == "Directory" ]] || fail "proof output type is invalid"
}

validate_target_directory() {
  local root="$1" path="$1/target" canonical mode
  [[ -d "$path" && ! -L "$path" ]] || fail "repository target is not an ordinary directory"
  [[ "$(stat -f '%u' "$path")" == "$(id -u)" ]] || fail "repository target owner is not exact"
  mode="$(stat -f '%Lp' "$path")"
  [[ "$mode" =~ ^[0-7][0145][0145]$ ]] || fail "repository target is group/world writable"
  [[ "$(stat -f '%HT' "$path")" == "Directory" ]] || fail "repository target type is invalid"
  canonical="$(cd "$path" && pwd -P)"
  [[ "$canonical" == "$path" ]] || fail "repository target identity is ambiguous"
}

validate_output_file() {
  local path="$1"
  [[ ! -e "$path" && ! -L "$path" ]] && return 0
  [[ -f "$path" && ! -L "$path" ]] || fail "refusing unsafe existing proof output"
  [[ "$(stat -f '%u' "$path")" == "$(id -u)" ]] || fail "existing proof owner is not exact"
  [[ "$(stat -f '%Lp' "$path")" == "600" ]] || fail "existing proof mode is not 0600"
  [[ "$(stat -f '%l' "$path")" == "1" ]] || fail "existing proof has multiple links"
  [[ "$(stat -f '%HT' "$path")" == "Regular File" ]] || fail "existing proof type is invalid"
}

prepare_output() {
  local root="$1" target="$1/target" output="$1/$OUTPUT_RELATIVE"
  umask 077
  if [[ -e "$target" || -L "$target" ]]; then validate_target_directory "$root"; else mkdir -m 700 "$target"; fi
  [[ -e "$output" || -L "$output" ]] || mkdir -m 700 "$output"
  validate_owner_directory "$output"
  cd "$output"
  [[ "$(pwd -P)" == "$output" ]] || fail "proof output identity is ambiguous"
  target_identity="$(directory_identity "$target")"
  output_identity="$(directory_identity .)"
  output_prepared=1
  validate_output_file "$RECEIPT_NAME"
  validate_output_file "$DIGEST_NAME"
  rm -f -- "$RECEIPT_NAME" "$DIGEST_NAME"
}

revalidate_output() {
  local root="$1" output="$1/$OUTPUT_RELATIVE"
  validate_target_directory "$root"
  validate_owner_directory "$output"
  [[ "$(directory_identity "$root/target")" == "$target_identity" ]] || fail "target identity changed"
  [[ "$(directory_identity "$output")" == "$output_identity" ]] || fail "proof output identity changed"
  [[ "$(directory_identity .)" == "$output_identity" && "$(pwd -P)" == "$output" ]] || fail "held output changed"
}

capture_repository_state() {
  local root="$1" prefix="$2" top branch head origin tree status path digest variable
  top="$(git_safe "$root" rev-parse --show-toplevel)"
  [[ "$top" == "$root" ]] || fail "controller must run at its repository root"
  branch="$(git_safe "$root" symbolic-ref -q HEAD)" || fail "detached HEAD is not eligible"
  [[ "$branch" == refs/heads/main ]] || fail "review-provider proof requires exact main"
  head="$(git_safe "$root" rev-parse --verify 'HEAD^{commit}')"
  origin="$(git_safe "$root" rev-parse --verify 'refs/remotes/origin/main^{commit}')"
  [[ "$head" == "$origin" ]] || fail "HEAD does not equal origin/main"
  tree="$(git_safe "$root" rev-parse --verify 'HEAD^{tree}')"
  git_safe "$root" ls-files -v -- | awk 'substr($0,1,2)!="H "{bad=1} END{exit bad}' \
    || fail "repository index contains hidden tracked-state flags"
  status="$(git_safe "$root" status --porcelain=v1 --untracked-files=all)"
  [[ -z "$status" ]] || fail "review-provider proof requires a clean working tree"
  valid_commit "$head" || fail "HEAD shape is invalid"
  for path in "$CONTROLLER_PATH" "$HARNESS_PATH" "$WINDOWS_PATH" "$ADAPTER_PATH" "$OUTPUT_SCHEMA_PATH"; do
    digest="$(git_safe "$root" show "$head:$path" | sha256_stdin)" || fail "committed proof definition is unavailable: $path"
    valid_sha "$digest" || fail "committed proof definition digest is invalid"
    case "$path" in
      "$CONTROLLER_PATH") variable=controller_digest ;;
      "$HARNESS_PATH") variable=harness_digest ;;
      "$WINDOWS_PATH") variable=windows_digest ;;
      "$ADAPTER_PATH") variable=adapter_digest ;;
      *) variable=schema_digest ;;
    esac
    eval "${prefix}_${variable}=\$digest"
  done
  eval "${prefix}_branch=\$branch"; eval "${prefix}_head=\$head"; eval "${prefix}_origin=\$origin"
  eval "${prefix}_tree=\$tree"; eval "${prefix}_status=\$status"
}

terminate_live_group() {
  local count=0
  [[ "${live_pid:-}" =~ ^[1-9][0-9]*$ ]] || return 0
  kill -TERM -- "-$live_pid" >/dev/null 2>&1 || true
  while kill -0 -- "-$live_pid" >/dev/null 2>&1; do
    count=$((count + 1)); [[ "$count" -lt 100 ]] || { kill -KILL -- "-$live_pid" >/dev/null 2>&1 || true; break; }
    sleep 0.1
  done
  wait "$live_pid" >/dev/null 2>&1 || true
  live_pid=""
}

clear_live_environment() {
  local name
  while IFS='=' read -r name _; do
    case "$name" in GIT_*|BASH_ENV|ENV|ASSEMBLYWRIGHT_REVIEW_PROVIDER_*) unset "$name" ;; esac
  done < <(env)
}

run_committed_harness() {
  local root="$1" head="$2" transcript="$3" fifo="$4" count=0 receipt="" status
  set -m
  (
    set +m
    git_safe "$root" show "$head:$HARNESS_PATH" | (
      exec 9<&0
      exec 0</dev/null
      cd "$root"
      clear_live_environment
      export ASSEMBLYWRIGHT_REVIEW_PROVIDER_INTERNAL_STDIN_V1="$SCHEMA"
      export ASSEMBLYWRIGHT_REVIEW_PROVIDER_RECEIPT_FD=3
      export ASSEMBLYWRIGHT_REVIEW_PROVIDER_EXPECTED_HEAD="$head"
      exec bash /dev/fd/9 --run 3<&3
    ) 3<&3 | tee "$transcript"
  ) 3<"$fifo" &
  live_pid="$!"
  set +m
  exec 4>"$fifo"; receipt_writer_open=1
  while ! grep -q '^assemblywright_review_provider_windows_run_required ' "$transcript" 2>/dev/null; do
    kill -0 "$live_pid" >/dev/null 2>&1 || fail "live harness exited before requesting the Windows action"
    count=$((count + 1)); [[ "$count" -lt 600 ]] || fail "timed out waiting for the Windows action marker"
    sleep 0.1
  done
  [[ "$(grep -c '^assemblywright_review_provider_windows_run_required ' "$transcript")" == 1 ]] \
    || fail "live harness emitted a duplicate action marker"
  IFS= read -r -t 1200 receipt || fail "timed out waiting for the sanitized Windows receipt"
  [[ -n "$receipt" && "${#receipt}" -le 8192 ]] || fail "sanitized Windows receipt was empty or oversized"
  printf '%s\n' "$receipt" >&4
  exec 4>&-; receipt_writer_open=0
  set +e; wait "$live_pid"; status="$?"; set -e
  [[ "$status" -eq 0 ]] || { terminate_live_group; fail "committed review-provider live harness failed"; }
  if kill -0 -- "-$live_pid" >/dev/null 2>&1; then terminate_live_group; fail "committed live harness left a descendant"; fi
  live_pid=""
}

validate_transcript() {
  local transcript="$1" head="$2" marker success line
  marker="$(grep -c '^assemblywright_review_provider_windows_run_required ' "$transcript" || true)"
  success="$(grep -c '^assemblywright_review_provider_live_e2e_ok ' "$transcript" || true)"
  [[ "$marker" == 1 && "$success" == 1 ]] || fail "live transcript marker set was not exact"
  [[ "$(grep -Ec '^assemblywright_review_provider_.*(required|ok) ' "$transcript" || true)" == 2 ]] \
    || fail "live transcript contained an unexpected proof marker"
  line="$(grep '^assemblywright_review_provider_live_e2e_ok ' "$transcript")"
  [[ "${#line}" -le 2048 ]] || fail "live success record was oversized"
  TRANSCRIPT_LINE="$line" EXPECTED_HEAD="$head" python3 - <<'PY' || fail "live success record bindings were invalid"
import os, re, sys
parts = os.environ["TRANSCRIPT_LINE"].split(" ")
names = ["source_head", "protocol_version", "master_schema_version", "provider_id", "model_id",
         "service_executable_sha256", "review_provider_executable_sha256", "codex_executable_sha256",
         "output_schema_sha256", "approval_packet_sha256", "approval_output_sha256",
         "rejection_packet_sha256", "rejection_output_sha256", "observed_at_ms"]
if len(parts) != 15 or parts[0] != "assemblywright_review_provider_live_e2e_ok": sys.exit(1)
values = {}
for expected, part in zip(names, parts[1:]):
    if not part.startswith(expected + "=") or expected in values: sys.exit(1)
    values[expected] = part[len(expected)+1:]
if values["source_head"] != os.environ["EXPECTED_HEAD"]: sys.exit(1)
if values["protocol_version"] != "5" or values["master_schema_version"] != "19": sys.exit(1)
if values["provider_id"] != "openai.codex" or values["model_id"] != "gpt-5.6-sol": sys.exit(1)
if any(not re.fullmatch(r"[0-9a-f]{64}", values[key]) for key in names[5:13]): sys.exit(1)
if not re.fullmatch(r"[0-9]{13}", values["observed_at_ms"]): sys.exit(1)
PY
}

write_receipt() {
  local head="$1" tree="$2" controller="$3" harness="$4" windows="$5" adapter="$6" schema="$7" transcript="$8" observed="$9"
  local receipt_tmp digest_tmp digest bytes
  receipt_tmp="$(mktemp .review-provider-live-proof.json.XXXXXX)"
  digest_tmp="$(mktemp .review-provider-live-proof.sha256.XXXXXX)"
  chmod 600 "$receipt_tmp" "$digest_tmp"
  printf '{"schema":"%s","category":"%s","origin":"%s","head_commit":"%s","tree_id":"%s","controller_definition_sha256":"%s","harness_definition_sha256":"%s","windows_control_definition_sha256":"%s","adapter_definition_sha256":"%s","output_schema_definition_sha256":"%s","transcript_sha256":"%s","provider_id":"%s","model_id":"%s","observed_at_ms":%s,"status":"passed","proof_boundary":"%s"}\n' \
    "$SCHEMA" "$CATEGORY" "$ORIGIN" "$head" "$tree" "$controller" "$harness" "$windows" "$adapter" "$schema" "$transcript" "$PROVIDER_ID" "$MODEL_ID" "$observed" "$PROOF_BOUNDARY" >"$receipt_tmp"
  bytes="$(wc -c <"$receipt_tmp" | tr -d '[:space:]')"; [[ "$bytes" -le 4096 ]] || fail "proof receipt was oversized"
  digest="$(sha256_file "$receipt_tmp")"; valid_sha "$digest" || fail "receipt digest was invalid"
  printf '%s\n' "$digest" >"$digest_tmp"
  mv -f -- "$digest_tmp" "$DIGEST_NAME"
  mv -f -- "$receipt_tmp" "$RECEIPT_NAME"
}

run_controller() (
  local root="$(cd "$1" && pwd -P)" transcript fifo before_branch before_head before_origin before_tree before_status
  local before_controller_digest before_harness_digest before_windows_digest before_adapter_digest before_schema_digest
  local after_branch after_head after_origin after_tree after_status after_controller_digest after_harness_digest
  local after_windows_digest after_adapter_digest after_schema_digest transcript_digest observed published=0
  local output_prepared=0 target_identity="" output_identity="" live_pid="" receipt_writer_open=0
  cleanup() {
    [[ "$receipt_writer_open" -eq 0 ]] || exec 4>&- || true
    terminate_live_group
    rm -f -- "${fifo:-}" "${transcript:-}" 2>/dev/null || true
    if [[ "$output_prepared" -eq 1 && "$published" -ne 1 ]]; then
      rm -f -- "$RECEIPT_NAME" "$DIGEST_NAME" .review-provider-live-proof.* 2>/dev/null || true
    fi
  }
  trap cleanup EXIT HUP INT TERM
  prepare_output "$root"
  capture_repository_state "$root" before
  transcript="$(mktemp .review-provider-transcript.XXXXXX)"; fifo="$(mktemp -u .review-provider-receipt.XXXXXX)"
  mkfifo -m 600 "$fifo"; chmod 600 "$transcript"
  run_committed_harness "$root" "$before_head" "$transcript" "$fifo"
  validate_transcript "$transcript" "$before_head"
  capture_repository_state "$root" after
  for name in branch head origin tree status controller_digest harness_digest windows_digest adapter_digest schema_digest; do
    eval '[[ "$before_'"$name"'" == "$after_'"$name"'" ]]' || fail "repository or proof definition changed during live proof"
  done
  revalidate_output "$root"
  transcript_digest="$(sha256_file "$transcript")"; valid_sha "$transcript_digest" || fail "transcript digest was invalid"
  observed="$(date -u '+%s')000"; [[ "$observed" =~ ^[0-9]{13}$ ]] || fail "observed time was invalid"
  write_receipt "$before_head" "$before_tree" "$before_controller_digest" "$before_harness_digest" \
    "$before_windows_digest" "$before_adapter_digest" "$before_schema_digest" "$transcript_digest" "$observed"
  revalidate_output "$root"; validate_output_file "$RECEIPT_NAME"; validate_output_file "$DIGEST_NAME"
  [[ "$(tr -d '[:space:]' <"$DIGEST_NAME")" == "$(sha256_file "$RECEIPT_NAME")" ]] || fail "published proof pair did not match"
  published=1; rm -f -- "$fifo" "$transcript"; trap - EXIT HUP INT TERM
  printf 'Assemblywright review-provider live proof controller: passed\nReceipt: %s/%s\nReceipt SHA-256: %s/%s\nProof boundary: %s\n' \
    "$OUTPUT_RELATIVE" "$RECEIPT_NAME" "$OUTPUT_RELATIVE" "$DIGEST_NAME" "$PROOF_BOUNDARY"
)

check_controller() {
  local command_name
  for command_name in git shasum awk mktemp mkfifo date id stat chmod mkdir mv rm wc tr grep tee env sleep python3; do require_command "$command_name"; done
  for path in "$CONTROLLER_PATH" "$HARNESS_PATH" "$WINDOWS_PATH" "$ADAPTER_PATH" "$OUTPUT_SCHEMA_PATH"; do
    [[ -f "$ROOT_DIR/$path" && ! -L "$ROOT_DIR/$path" ]] || fail "fixed proof definition is unavailable: $path"
  done
  git_safe "$ROOT_DIR" check-ignore -q "$OUTPUT_RELATIVE/$RECEIPT_NAME" || fail "fixed proof output is not ignored"
  grep -Fq 'review-provider-proof-controller.sh --check' "$ROOT_DIR/scripts/release-local.sh" || fail "release-local omits the controller check"
  grep -Fq 'review-provider-proof-controller.sh --self-test' "$ROOT_DIR/scripts/release-local.sh" || fail "release-local omits the controller self-test"
  grep -Fq 'ASSEMBLYWRIGHT_REVIEW_PROVIDER_INTERNAL_STDIN_V1' "$ROOT_DIR/$HARNESS_PATH" || fail "harness omits committed-byte execution"
  grep -Fq '[System.Collections.IDictionary]' "$ROOT_DIR/$WINDOWS_PATH" || fail "Windows receipt validation is not dictionary-safe"
  printf 'Assemblywright review-provider proof controller check: ok\nProof boundary: static prerequisites only; no provider call ran and no receipt was created.\n'
}

initialize_fixture() {
  local fixture="$1" body="$2" path
  mkdir -p "$fixture/scripts" "$fixture/crates/assemblywright-master/src" "$fixture/crates/assemblywright-master/resources"
  git_safe "$fixture" init -q; git_safe "$fixture" checkout -q -b main
  git_safe "$fixture" config user.name 'Assemblywright Review Provider Self Test'
  git_safe "$fixture" config user.email 'review-provider-self-test@invalid.example'
  printf 'target/\n' >"$fixture/.gitignore"
  printf '%s\n' "$body" >"$fixture/$HARNESS_PATH"
  printf 'fixture controller\n' >"$fixture/$CONTROLLER_PATH"
  printf '[System.Collections.IDictionary]\n' >"$fixture/$WINDOWS_PATH"
  printf 'fixture adapter\n' >"$fixture/$ADAPTER_PATH"
  printf '{}\n' >"$fixture/$OUTPUT_SCHEMA_PATH"
  git_safe "$fixture" add .; git_safe "$fixture" commit -q -m fixture
  git_safe "$fixture" update-ref refs/remotes/origin/main HEAD
}

fixture_harness() {
  cat <<'FIXTURE'
#!/usr/bin/env bash
set -euo pipefail
printf 'assemblywright_review_provider_windows_run_required action=Run confirm=true receipt_fd=3\n'
IFS= read -r -u 3 receipt
printf 'assemblywright_review_provider_live_e2e_ok source_head=%s protocol_version=5 master_schema_version=19 provider_id=openai.codex model_id=gpt-5.6-sol service_executable_sha256=%064d review_provider_executable_sha256=%064d codex_executable_sha256=%064d output_schema_sha256=%064d approval_packet_sha256=%064d approval_output_sha256=%064d rejection_packet_sha256=%064d rejection_output_sha256=%064d observed_at_ms=%s\n' "$ASSEMBLYWRIGHT_REVIEW_PROVIDER_EXPECTED_HEAD" 1 2 3 4 5 6 7 8 "$(date -u +%s)000"
FIXTURE
}

self_test_controller() {
  local scratch success dirty wrong hidden hostile receipt digest
  scratch="$(mktemp -d -t assemblywright-review-provider-proof)"; chmod 700 "$scratch"
  trap 'rm -rf -- "$scratch"' RETURN
  success="$scratch/success"; initialize_fixture "$success" "$(fixture_harness)"
  printf '{}\n' | run_controller "$success" >/dev/null
  receipt="$success/$OUTPUT_RELATIVE/$RECEIPT_NAME"; digest="$success/$OUTPUT_RELATIVE/$DIGEST_NAME"
  [[ -f "$receipt" && -f "$digest" && "$(stat -f '%Lp' "$receipt")" == 600 ]] || fail "success fixture did not publish private proof"
  [[ "$(sha256_file "$receipt")" == "$(tr -d '[:space:]' <"$digest")" ]] || fail "success fixture proof pair did not match"
  dirty="$scratch/dirty"; initialize_fixture "$dirty" "$(fixture_harness)"; printf 'dirty\n' >>"$dirty/$ADAPTER_PATH"
  if printf '{}\n' | run_controller "$dirty" >/dev/null 2>&1; then fail "dirty fixture was accepted"; fi
  wrong="$scratch/wrong"; initialize_fixture "$wrong" "$(fixture_harness)"; git_safe "$wrong" checkout -q -b other
  if printf '{}\n' | run_controller "$wrong" >/dev/null 2>&1; then fail "wrong branch fixture was accepted"; fi
  hidden="$scratch/hidden"; initialize_fixture "$hidden" "$(fixture_harness)"; git_safe "$hidden" update-index --skip-worktree "$HARNESS_PATH"
  if printf '{}\n' | run_controller "$hidden" >/dev/null 2>&1; then fail "hidden index fixture was accepted"; fi
  hostile="$scratch/hostile"; initialize_fixture "$hostile" "$(fixture_harness)"; mkdir -m 700 "$hostile/target"; ln -s "$scratch" "$hostile/target/review-provider-live-proof"
  if printf '{}\n' | run_controller "$hostile" >/dev/null 2>&1; then fail "hostile output fixture was accepted"; fi
  printf 'Assemblywright review-provider proof controller self-test: ok\n'
  printf 'Proof boundary: disposable Git/process fixtures prove fixed success, committed definition binding, clean-main/index enforcement, private digest-first output, and hostile output denial only.\n'
}

MODE="${1:---check}"
[[ "$#" -le 1 ]] || { usage >&2; fail "the controller accepts no extra arguments"; }
case "$MODE" in
  --check) check_controller ;;
  --run) run_controller "$ROOT_DIR" ;;
  --self-test) self_test_controller ;;
  --help|-h) usage ;;
  *) usage >&2; fail "unknown mode: $MODE" ;;
esac
