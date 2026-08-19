#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
OUTPUT_RELATIVE="target/github-publication-live-proof"
RECEIPT_NAME="github-publication-live-proof.json"
DIGEST_NAME="github-publication-live-proof.sha256"
SCHEMA="assemblywright.github-publication-live-proof.v1"
CATEGORY="github_publication_live"
ORIGIN="github_publication_proof_controller"
PROOF_IDENTITY="assemblywright.github-publication-live.v1"
HARNESS_PATH="scripts/github-publication-live-e2e.sh"
WINDOWS_PATH="scripts/windows-github-publication-live-control.ps1"
CONTROLLER_PATH="scripts/github-publication-proof-controller.sh"
PROOF_BOUNDARY="Exact clean published main used the committed controller, harness, and Windows-control definitions to create one bounded metadata-only proof-marker pull request, require the two fixed protected checks, merge it, and reconcile origin/main to the reported protected merge commit; the local source checkout remained unchanged. This is fixed GitHub-publication integration proof, not activation-evidence admission, general branch-protection proof, queue lifecycle, restricted-worker, review-provider, restart-recovery, control-streaming, signing, notarization, or production-readiness proof."
unset ASSEMBLYWRIGHT_GITHUB_PUBLICATION_INTERNAL_STDIN_V1
unset ASSEMBLYWRIGHT_GITHUB_PUBLICATION_RECEIPT_FD
unset ASSEMBLYWRIGHT_GITHUB_PUBLICATION_EXPECTED_HEAD
receipt_terminal_state=""
receipt_read_error=""
fixture_mode=0

fail() { printf 'error: %s\n' "$1" >&2; exit 1; }

usage() {
  cat <<'USAGE'
Usage: scripts/github-publication-proof-controller.sh [--check | --run | --self-test]

  --check      Validate fixed prerequisites without a remote mutation.
  --run        Run only the exact committed live harness and write the fixed receipt.
  --self-test  Exercise success and fail-closed behavior in disposable Git fixtures.

The controller accepts no repository, remote, provider, executable, credential,
branch, check, or harness argument. It never admits evidence or activates.
USAGE
}

require_command() { command -v "$1" >/dev/null 2>&1 || fail "required command is unavailable: $1"; }

git_safe() {
  local root="$1" protocol_policy=never; shift
  [[ "$fixture_mode" -eq 0 ]] || protocol_policy=always
  env -i PATH="$PATH" LC_ALL=C GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null \
    GIT_CONFIG_NOSYSTEM=1 GIT_ATTR_NOSYSTEM=1 GIT_TERMINAL_PROMPT=0 GIT_OPTIONAL_LOCKS=0 \
    git --no-replace-objects -c core.fsmonitor=false -c core.hooksPath=/dev/null \
      -c core.attributesFile=/dev/null -c protocol.file.allow="$protocol_policy" -C "$root" "$@"
}

sha256_file() { shasum -a 256 "$1" | awk '{print $1}'; }
sha256_stdin() { shasum -a 256 | awk '{print $1}'; }
valid_sha() { [[ "$1" =~ ^[0-9a-f]{64}$ ]]; }
valid_commit() { [[ "$1" =~ ^[0-9a-f]{40}$ ]]; }
directory_identity() { stat -f '%d:%i' "$1"; }

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
  local root="$1" prefix="$2" require_published="$3" top branch head origin tree status remote_url path digest variable
  top="$(git_safe "$root" rev-parse --show-toplevel)"; [[ "$top" == "$root" ]] || fail "controller must run at its repository root"
  branch="$(git_safe "$root" symbolic-ref -q HEAD)" || fail "detached HEAD is not eligible"
  [[ "$branch" == refs/heads/main ]] || fail "GitHub-publication proof requires exact main"
  head="$(git_safe "$root" rev-parse --verify 'HEAD^{commit}')"
  origin="$(git_safe "$root" rev-parse --verify 'refs/remotes/origin/main^{commit}')"
  remote_url="$(git_safe "$root" remote get-url origin)"
  if [[ "$fixture_mode" -eq 0 ]]; then
    [[ "$remote_url" == "https://github.com/malak333/Assemblywright" || \
       "$remote_url" == "https://github.com/malak333/Assemblywright.git" ]] \
      || fail "origin is not the fixed Assemblywright GitHub repository"
  fi
  [[ "$require_published" != yes || "$head" == "$origin" ]] || fail "HEAD does not equal origin/main"
  tree="$(git_safe "$root" rev-parse --verify 'HEAD^{tree}')"
  git_safe "$root" ls-files -v -- | awk 'substr($0,1,2)!="H "{bad=1} END{exit bad}' \
    || fail "repository index contains hidden tracked-state flags"
  status="$(git_safe "$root" status --porcelain=v1 --untracked-files=all)"
  [[ -z "$status" ]] || fail "GitHub-publication proof requires a clean working tree"
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
    case "$name" in GIT_*|BASH_ENV|ENV|GH_*|GITHUB_*|ASSEMBLYWRIGHT_GITHUB_PUBLICATION_*) unset "$name";; esac
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
      export ASSEMBLYWRIGHT_GITHUB_PUBLICATION_INTERNAL_STDIN_V1="$SCHEMA"
      export ASSEMBLYWRIGHT_GITHUB_PUBLICATION_RECEIPT_FD=3
      export ASSEMBLYWRIGHT_GITHUB_PUBLICATION_EXPECTED_HEAD="$head"
      exec bash /dev/fd/9 --run 3<&3
    ) 3<&3 | tee "$transcript"
  ) 3<"$fifo" &
  live_pid="$!"; set +m; exec 4>"$fifo"; receipt_writer_open=1
  while ! grep -q '^assemblywright_github_publication_windows_run_required ' "$transcript" 2>/dev/null; do
    kill -0 "$live_pid" >/dev/null 2>&1 || fail "live harness exited before requesting the Windows action"
    count=$((count + 1)); [[ "$count" -lt 600 ]] || fail "timed out waiting for the Windows action marker"; sleep 0.1
  done
  [[ "$(grep -c '^assemblywright_github_publication_windows_run_required ' "$transcript")" == 1 ]] \
    || fail "live harness emitted a duplicate action marker"
  if ! read_live_receipt receipt; then
    fail "sanitized Windows receipt ${receipt_read_error:-could_not_be_read}"
  fi
  [[ -n "$receipt" && "${#receipt}" -le 8192 ]] || fail "sanitized Windows receipt was empty or oversized"
  printf '%s\n' "$receipt" >&4; exec 4>&-; receipt_writer_open=0
  set +e; wait "$live_pid"; status="$?"; set -e
  [[ "$status" -eq 0 ]] || { terminate_live_group; fail "committed GitHub-publication live harness failed"; }
  if kill -0 -- "-$live_pid" >/dev/null 2>&1; then terminate_live_group; fail "committed live harness left a descendant"; fi
  live_pid=""
}

validate_transcript() {
  local transcript="$1" source_head="$2" marker success line
  marker="$(grep -c '^assemblywright_github_publication_windows_run_required ' "$transcript" || true)"
  success="$(grep -c '^assemblywright_github_publication_live_e2e_ok ' "$transcript" || true)"
  [[ "$marker" == 1 && "$success" == 1 ]] || fail "live transcript marker set was not exact"
  [[ "$(grep -Ec '^assemblywright_github_publication_.*(required|ok) ' "$transcript" || true)" == 2 ]] \
    || fail "live transcript contained an unexpected proof marker"
  line="$(grep '^assemblywright_github_publication_live_e2e_ok ' "$transcript")"
  [[ "${#line}" -le 3072 ]] || fail "live success record was oversized"
  TRANSCRIPT_LINE="$line" EXPECTED_HEAD="$source_head" python3 - <<'PY' || fail "live success bindings were invalid"
import os, re, sys
parts=os.environ["TRANSCRIPT_LINE"].split(" ")
names=["source_head","publication_commit","resulting_main_commit","protocol_version","master_schema_version","repository","base_branch","pull_request_number","pull_request_url_sha256","branch_name_sha256","required_checks_sha256","post_merge_checks_sha256","master_executable_sha256","git_version","git_executable_sha256","gh_version","gh_executable_sha256","observed_at_ms"]
if len(parts)!=len(names)+1 or parts[0]!="assemblywright_github_publication_live_e2e_ok": sys.exit(1)
values={}
for name,part in zip(names,parts[1:]):
    if not part.startswith(name+"=") or name in values: sys.exit(1)
    values[name]=part[len(name)+1:]
if values["source_head"]!=os.environ["EXPECTED_HEAD"]: sys.exit(1)
if values["source_head"] in (values["publication_commit"],values["resulting_main_commit"]) or values["publication_commit"]==values["resulting_main_commit"]: sys.exit(1)
if values["protocol_version"]!="5" or values["master_schema_version"]!="19" or values["repository"]!="malak333/Assemblywright" or values["base_branch"]!="main": sys.exit(1)
if values["git_version"]!="2.55.0.windows.2" or values["gh_version"]!="2.96.0": sys.exit(1)
if not re.fullmatch(r"[1-9][0-9]*",values["pull_request_number"]): sys.exit(1)
if not re.fullmatch(r"[0-9a-f]{40}",values["publication_commit"]) or not re.fullmatch(r"[0-9a-f]{40}",values["resulting_main_commit"]): sys.exit(1)
for key in ("pull_request_url_sha256","branch_name_sha256","required_checks_sha256","post_merge_checks_sha256","master_executable_sha256","git_executable_sha256","gh_executable_sha256"):
    if not re.fullmatch(r"[0-9a-f]{64}",values[key]): sys.exit(1)
if not re.fullmatch(r"[0-9]{13}",values["observed_at_ms"]): sys.exit(1)
print(values["resulting_main_commit"])
PY
  published_commit="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["resulting_main_commit"])')"
  master_executable_digest="$(TRANSCRIPT_LINE="$line" python3 -c 'import os; print(dict(x.split("=",1) for x in os.environ["TRANSCRIPT_LINE"].split()[1:])["master_executable_sha256"])')"
}

validate_windows_rollback_contract() {
  python3 - "$ROOT_DIR/$WINDOWS_PATH" <<'PY' || fail "Windows provisioning rollback contract was incomplete"
import pathlib, sys
text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
required = (
    "function Restore-PreviousPublicationDeployment",
    "$swapAttempted = $false",
    "$swapAttempted = $true",
    "Restore-PreviousPublicationDeployment -Master $master",
    "GitHub-publication provisioning rollback failed.",
    "the previous deployment and service state were restored.",
    "Start-Service -Name $serviceName -ErrorAction Stop",
    "[void](Invoke-MasterHealth $Master)",
    "github-publication-master.previous",
    "Copy-Item -LiteralPath $MasterBackup -Destination $Master -Force -ErrorAction Stop",
    "$restoredMasterSha -cne $OriginalMasterSha256",
    "master_executable_sha256 = $masterExecutableSha256",
    "$proof.master_executable_sha256 -cne [string]$assets.MasterSha256",
    "function Invoke-WithGitHubPublicationControlLock",
    "$controlMutexName = \"Global\\Assemblywright.GitHubPublication.Control.v1\"",
    "$security.SetAccessRuleProtection($true, $false)",
    "Another GitHub-publication control operation is active.",
    "A prior GitHub-publication staging artifact requires owner reconciliation.",
    "$stagingOwned = $false",
    "if ($stagingOwned -and (Test-Path -LiteralPath $staging))",
    "Get-SourceHead $false",
    "refs/heads/main:refs/remotes/origin/main",
    "github-publication-proof --confirm --expected-source-head $head",
)
if any(token not in text for token in required):
    raise SystemExit(1)
if "finally { Start-Service -Name $serviceName" in text:
    raise SystemExit(1)
if "if (Test-Path -LiteralPath $staging) { Remove-Item" in text:
    raise SystemExit(1)
helper = text.index("function Restore-PreviousPublicationDeployment")
provision = text.index("function Invoke-Provision")
original_master = text.index("$originalMasterSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $master)", provision)
first_mutation = text.index("Copy-Item -LiteralPath $master -Destination $masterBackup", provision)
restore_call = text.index("Restore-PreviousPublicationDeployment -Master $master", provision)
rollback_failure = text.index("GitHub-publication provisioning rollback failed.", restore_call)
run = text.index("function Invoke-Run")
refresh = text.index("refs/heads/main:refs/remotes/origin/main", run)
effect = text.index("github-publication-proof --confirm", run)
lock_wrapper = text.rindex("Invoke-WithGitHubPublicationControlLock {")
if not helper < provision < original_master < first_mutation < restore_call < rollback_failure:
    raise SystemExit(1)
if not run < refresh < effect < lock_wrapper:
    raise SystemExit(1)
PY
}

write_receipt() {
  local source="$1" tree="$2" published="$3" master="$4" controller="$5" harness="$6" windows="$7" transcript="$8" observed="$9"
  local receipt_tmp digest_tmp digest bytes
  receipt_tmp="$(mktemp .github-publication-live-proof.json.XXXXXX)"; digest_tmp="$(mktemp .github-publication-live-proof.sha256.XXXXXX)"
  chmod 600 "$receipt_tmp" "$digest_tmp"
  printf '{"schema":"%s","category":"%s","origin":"%s","source_head_commit":"%s","source_tree_id":"%s","published_main_commit":"%s","master_executable_sha256":"%s","controller_definition_sha256":"%s","harness_definition_sha256":"%s","windows_control_definition_sha256":"%s","proof_transcript_sha256":"%s","proof_identity":"%s","observed_at_ms":%s,"status":"passed","proof_boundary":"%s"}\n' \
    "$SCHEMA" "$CATEGORY" "$ORIGIN" "$source" "$tree" "$published" "$master" "$controller" "$harness" "$windows" "$transcript" "$PROOF_IDENTITY" "$observed" "$PROOF_BOUNDARY" >"$receipt_tmp"
  bytes="$(wc -c <"$receipt_tmp" | tr -d '[:space:]')"; [[ "$bytes" -le 4096 ]] || fail "proof receipt was oversized"
  digest="$(sha256_file "$receipt_tmp")"; valid_sha "$digest" || fail "receipt digest was invalid"
  printf '%s\n' "$digest" >"$digest_tmp"
  mv -f -- "$digest_tmp" "$DIGEST_NAME"; mv -f -- "$receipt_tmp" "$RECEIPT_NAME"
}

run_controller() (
  local root="$(cd "$1" && pwd -P)" transcript fifo transcript_digest observed published=0 published_commit="" master_executable_digest=""
  local output_prepared=0 target_identity="" output_identity="" live_pid="" receipt_writer_open=0
  local before_branch before_head before_origin before_tree before_status before_controller_digest before_harness_digest before_windows_digest
  local after_branch after_head after_origin after_tree after_status after_controller_digest after_harness_digest after_windows_digest
  cleanup() {
    [[ "$receipt_writer_open" -eq 0 ]] || exec 4>&- || true
    restore_receipt_terminal || true; terminate_live_group
    rm -f -- "${fifo:-}" "${transcript:-}" 2>/dev/null || true
    if [[ "$output_prepared" -eq 1 && "$published" -ne 1 ]]; then rm -f -- "$RECEIPT_NAME" "$DIGEST_NAME" .github-publication-live-proof.* 2>/dev/null || true; fi
  }
  trap cleanup EXIT
  trap 'exit 129' HUP
  trap 'exit 130' INT
  trap 'exit 143' TERM
  prepare_output "$root"
  capture_source_state "$root" refresh no
  git_safe "$root" fetch --no-tags origin '+refs/heads/main:refs/remotes/origin/main' >/dev/null \
    || fail "could not refresh the fixed origin/main before publication"
  capture_source_state "$root" before yes
  transcript="$(mktemp .github-publication-transcript.XXXXXX)"; fifo="$(mktemp -u .github-publication-receipt.XXXXXX)"
  mkfifo -m 600 "$fifo"; chmod 600 "$transcript"
  run_committed_harness "$root" "$before_head" "$transcript" "$fifo"
  validate_transcript "$transcript" "$before_head"
  valid_commit "$published_commit" || fail "reported protected merge commit was malformed"
  capture_source_state "$root" after no
  for name in branch head tree status controller_digest harness_digest windows_digest; do
    eval '[[ "$before_'"$name"'" == "$after_'"$name"'" ]]' || fail "local source checkout or proof definition changed during live proof"
  done
  git_safe "$root" fetch --no-tags origin '+refs/heads/main:refs/remotes/origin/main' >/dev/null \
    || fail "could not fetch the fixed origin/main after publication"
  after_origin="$(git_safe "$root" rev-parse --verify 'refs/remotes/origin/main^{commit}')"
  [[ "$after_origin" == "$published_commit" ]] || fail "origin/main did not equal the reported protected merge commit"
  [[ "$(git_safe "$root" rev-parse --verify 'HEAD^{commit}')" == "$before_head" ]] \
    || fail "local source HEAD changed during origin reconciliation"
  [[ -z "$(git_safe "$root" status --porcelain=v1 --untracked-files=all)" ]] || fail "local source checkout changed during origin reconciliation"
  revalidate_output "$root"
  transcript_digest="$(sha256_file "$transcript")"; valid_sha "$transcript_digest" || fail "transcript digest was invalid"
  observed="$(date -u '+%s')000"; [[ "$observed" =~ ^[0-9]{13}$ ]] || fail "observed time was invalid"
  valid_sha "$master_executable_digest" || fail "reported master executable digest was malformed"
  write_receipt "$before_head" "$before_tree" "$published_commit" "$master_executable_digest" "$before_controller_digest" "$before_harness_digest" "$before_windows_digest" "$transcript_digest" "$observed"
  revalidate_output "$root"; validate_output_file "$RECEIPT_NAME"; validate_output_file "$DIGEST_NAME"
  [[ "$(tr -d '[:space:]' <"$DIGEST_NAME")" == "$(sha256_file "$RECEIPT_NAME")" ]] || fail "published proof pair did not match"
  published=1; rm -f -- "$fifo" "$transcript"; trap - EXIT HUP INT TERM
  printf 'Assemblywright GitHub-publication live proof controller: passed\nReceipt: %s/%s\nReceipt SHA-256: %s/%s\nProof boundary: %s\n' \
    "$OUTPUT_RELATIVE" "$RECEIPT_NAME" "$OUTPUT_RELATIVE" "$DIGEST_NAME" "$PROOF_BOUNDARY"
)

check_controller() {
  local command_name
  for command_name in git shasum awk mktemp mkfifo date id stat chmod mkdir mv rm wc tr grep tee env sleep python3 stty expect; do require_command "$command_name"; done
  for path in "$CONTROLLER_PATH" "$HARNESS_PATH" "$WINDOWS_PATH"; do [[ -f "$ROOT_DIR/$path" && ! -L "$ROOT_DIR/$path" ]] || fail "fixed proof definition is unavailable: $path"; done
  git_safe "$ROOT_DIR" check-ignore -q "$OUTPUT_RELATIVE/$RECEIPT_NAME" || fail "fixed proof output is not ignored"
  grep -Fq 'github-publication-proof-controller.sh --check' "$ROOT_DIR/scripts/release-local.sh" || fail "release-local omits the controller check"
  grep -Fq 'github-publication-proof-controller.sh --self-test' "$ROOT_DIR/scripts/release-local.sh" || fail "release-local omits the controller self-test"
  grep -Fq 'ASSEMBLYWRIGHT_GITHUB_PUBLICATION_INTERNAL_STDIN_V1' "$ROOT_DIR/$HARNESS_PATH" || fail "harness omits committed-byte execution"
  grep -Fq '[System.Collections.IDictionary]' "$ROOT_DIR/$WINDOWS_PATH" || fail "Windows receipt validation is not dictionary-safe"
  validate_windows_rollback_contract
  printf 'Assemblywright GitHub-publication proof controller check: ok\nProof boundary: static prerequisites only; no remote mutation ran and no receipt was created.\n'
}

initialize_fixture() {
  local fixture="$1" body="$2" bare="$3"
  mkdir -p "$fixture/scripts"; git init -q --bare "$bare"
  git_safe "$fixture" init -q; git_safe "$fixture" checkout -q -b main
  git_safe "$fixture" config user.name 'Assemblywright GitHub Publication Self Test'; git_safe "$fixture" config user.email 'github-publication-self-test@invalid.example'
  printf 'target/\n' >"$fixture/.gitignore"; printf '%s\n' "$body" >"$fixture/$HARNESS_PATH"
  printf 'fixture controller\n' >"$fixture/$CONTROLLER_PATH"; printf '[System.Collections.IDictionary]\n' >"$fixture/$WINDOWS_PATH"
  git_safe "$fixture" add .; git_safe "$fixture" commit -q -m fixture
  git_safe "$fixture" remote add origin "$bare"; git -c protocol.file.allow=always -C "$fixture" push -q -u origin main
}

fixture_harness() {
  cat <<'FIXTURE'
#!/usr/bin/env bash
set -euo pipefail
printf 'assemblywright_github_publication_windows_run_required action=Run confirm=true receipt_fd=3\n'
IFS= read -r -u 3 receipt
tmp="$(mktemp -d)"; remote="$(git config --get remote.origin.url)"
git -c protocol.file.allow=always clone -q "$remote" "$tmp/repo"
git -C "$tmp/repo" config user.name Fixture; git -C "$tmp/repo" config user.email fixture@invalid.example
git -C "$tmp/repo" checkout -q -b proof
git -C "$tmp/repo" commit -q --allow-empty -m proof; publication="$(git -C "$tmp/repo" rev-parse HEAD)"
git -C "$tmp/repo" checkout -q main
git -C "$tmp/repo" merge -q --no-ff proof -m merge; resulting="$(git -C "$tmp/repo" rev-parse HEAD)"
git -C "$tmp/repo" push -q origin HEAD:main; rm -rf "$tmp"
printf 'assemblywright_github_publication_live_e2e_ok source_head=%s publication_commit=%s resulting_main_commit=%s protocol_version=5 master_schema_version=19 repository=malak333/Assemblywright base_branch=main pull_request_number=1 pull_request_url_sha256=%064d branch_name_sha256=%064d required_checks_sha256=%064d post_merge_checks_sha256=%064d master_executable_sha256=%064d git_version=2.55.0.windows.2 git_executable_sha256=%064d gh_version=2.96.0 gh_executable_sha256=%064d observed_at_ms=%s\n' "$ASSEMBLYWRIGHT_GITHUB_PUBLICATION_EXPECTED_HEAD" "$publication" "$resulting" 1 2 3 4 5 6 7 "$(date -u +%s)000"
FIXTURE
}

assert_no_proof() {
  local fixture="$1"; [[ ! -e "$fixture/$OUTPUT_RELATIVE/$RECEIPT_NAME" && ! -e "$fixture/$OUTPUT_RELATIVE/$DIGEST_NAME" ]] || fail "rejected fixture retained proof"
}

self_test_controller() {
  local scratch success dirty wrong hidden hostile stale stale_publish receipt digest bare pty_reader pty_expect pty_output long_receipt oversized_receipt
  local cancellation cancellation_ready cancellation_survived controller_pid controller_status wait_count
  fixture_mode=1
  scratch="$(mktemp -d -t assemblywright-github-publication-proof)"; chmod 700 "$scratch"; trap 'rm -rf -- "$scratch"' RETURN
  validate_windows_rollback_contract
  pty_reader="$scratch/pty-reader.sh"
  {
    printf '%s\n' '#!/usr/bin/env bash' 'set -euo pipefail' 'receipt_terminal_state=""' 'receipt_read_error=""'
    declare -f restore_receipt_terminal read_live_receipt
    cat <<'PTY_READER'
before="$(stty -g <&0)"; receipt=""
if [[ "${PTY_MODE:?}" == success ]]; then
  read_live_receipt receipt assemblywright_github_publication_pty_ready
  [[ "$(stty -g <&0)" == "$before" ]]
  printf 'assemblywright_github_publication_pty_ok bytes=%s terminal_restored=verified\n' "${#receipt}"
else
  if read_live_receipt receipt assemblywright_github_publication_pty_ready; then exit 41; fi
  [[ "$receipt_read_error" == oversized && "$(stty -g <&0)" == "$before" ]]
  printf 'assemblywright_github_publication_pty_oversized_rejected bytes=%s terminal_restored=verified\n' "${#receipt}"
fi
PTY_READER
  } >"$pty_reader"
  chmod 700 "$pty_reader"
  pty_expect="$scratch/pty.expect"
  cat <<'EXPECT' >"$pty_expect"
set timeout 10
spawn -noecho env PTY_MODE=$env(PTY_MODE) $env(PTY_READER)
expect -re {assemblywright_github_publication_pty_ready}
send -- "$env(PTY_RECEIPT)\r"
expect {
  -re $env(PTY_EXPECT) {}
  timeout { exit 2 }
  eof { exit 3 }
}
expect eof
wait
EXPECT
  long_receipt="$(python3 -c 'print("x" * 2182, end="")')"
  pty_output="$(PTY_MODE=success PTY_READER="$pty_reader" PTY_RECEIPT="$long_receipt" \
    PTY_EXPECT='assemblywright_github_publication_pty_ok bytes=2182 terminal_restored=verified' \
    expect "$pty_expect")"
  [[ "$pty_output" == *'assemblywright_github_publication_pty_ok bytes=2182 terminal_restored=verified'* ]] \
    || fail "long PTY receipt was not accepted and restored"
  oversized_receipt="$(python3 -c 'print("y" * 9000, end="")')"
  pty_output="$(PTY_MODE=oversized PTY_READER="$pty_reader" PTY_RECEIPT="$oversized_receipt" \
    PTY_EXPECT='assemblywright_github_publication_pty_oversized_rejected bytes=8192 terminal_restored=verified' \
    expect "$pty_expect")"
  [[ "$pty_output" == *'assemblywright_github_publication_pty_oversized_rejected bytes=8192 terminal_restored=verified'* ]] \
    || fail "oversized PTY receipt was not rejected and restored"
  success="$scratch/success"; bare="$scratch/success.git"; initialize_fixture "$success" "$(fixture_harness)" "$bare"
  printf '{}\n' | run_controller "$success" >/dev/null
  receipt="$success/$OUTPUT_RELATIVE/$RECEIPT_NAME"; digest="$success/$OUTPUT_RELATIVE/$DIGEST_NAME"
  [[ -f "$receipt" && -f "$digest" && "$(stat -f '%Lp' "$receipt")" == 600 ]] || fail "success fixture did not publish private proof"
  [[ "$(sha256_file "$receipt")" == "$(tr -d '[:space:]' <"$digest")" ]] || fail "success proof pair did not match"
  grep -Eq '"published_main_commit":"[0-9a-f]{40}"' "$receipt" || fail "success proof omitted published commit"
  if grep -Eq '(/Users/|/private/|/tmp/|https?://|github\.com|ghp_|github_pat_)' "$receipt"; then fail "self-test receipt leaked a path, remote, or credential"; fi
  dirty="$scratch/dirty"; initialize_fixture "$dirty" "$(fixture_harness)" "$scratch/dirty.git"; printf dirty >>"$dirty/$WINDOWS_PATH"
  if printf '{}\n' | run_controller "$dirty" >/dev/null 2>&1; then fail "dirty fixture was accepted"; fi; assert_no_proof "$dirty"
  wrong="$scratch/wrong"; initialize_fixture "$wrong" "$(fixture_harness)" "$scratch/wrong.git"; git_safe "$wrong" checkout -q -b other
  if printf '{}\n' | run_controller "$wrong" >/dev/null 2>&1; then fail "wrong branch fixture was accepted"; fi; assert_no_proof "$wrong"
  hidden="$scratch/hidden"; initialize_fixture "$hidden" "$(fixture_harness)" "$scratch/hidden.git"; git_safe "$hidden" update-index --skip-worktree "$HARNESS_PATH"
  if printf '{}\n' | run_controller "$hidden" >/dev/null 2>&1; then fail "hidden index fixture was accepted"; fi; assert_no_proof "$hidden"
  stale="$scratch/stale"; initialize_fixture "$stale" "$(fixture_harness)" "$scratch/stale.git"
  stale_publish="$scratch/stale-publish"
  git -c protocol.file.allow=always clone -q "$scratch/stale.git" "$stale_publish"
  git -C "$stale_publish" config user.name Fixture; git -C "$stale_publish" config user.email fixture@invalid.example
  git -C "$stale_publish" commit -q --allow-empty -m remote-advanced
  git -C "$stale_publish" push -q origin HEAD:main
  if printf '{}\n' | run_controller "$stale" >/dev/null 2>&1; then fail "stale local main fixture was accepted"; fi; assert_no_proof "$stale"
  hostile="$scratch/hostile"; initialize_fixture "$hostile" "$(fixture_harness)" "$scratch/hostile.git"; mkdir -m 700 "$hostile/target"; ln -s "$scratch" "$hostile/target/github-publication-live-proof"
  if printf '{}\n' | run_controller "$hostile" >/dev/null 2>&1; then fail "hostile output fixture was accepted"; fi; assert_no_proof "$hostile"
  cancellation="$scratch/cancellation"; cancellation_ready="$scratch/cancellation-ready"; cancellation_survived="$scratch/cancellation-survived"
  initialize_fixture "$cancellation" \
    "printf '%s\\n' 'assemblywright_github_publication_windows_run_required action=Run confirm=true receipt_fd=3'"$'\n'\
"IFS= read -r -u 3 receipt"$'\n'\
"printf ready >'$cancellation_ready'"$'\n'\
"(sleep 10; printf survived >'$cancellation_survived') & wait" "$scratch/cancellation.git"
  run_controller "$cancellation" < <(printf '{}\n') >/dev/null 2>&1 &
  controller_pid="$!"; wait_count=0
  while [[ ! -e "$cancellation_ready" ]]; do
    wait_count=$((wait_count + 1)); [[ "$wait_count" -lt 50 ]] || { kill -TERM "$controller_pid" >/dev/null 2>&1 || true; fail "cancellation fixture did not start"; }
    sleep 0.1
  done
  kill -TERM "$controller_pid"; set +e; wait "$controller_pid" >/dev/null 2>&1; controller_status="$?"; set -e
  [[ "$controller_status" -eq 143 ]] || fail "cancelled controller returned the wrong status"
  sleep 3
  [[ ! -e "$cancellation_survived" ]] || fail "cancelled controller left a live descendant"
  assert_no_proof "$cancellation"
  printf 'Assemblywright GitHub-publication proof controller self-test: ok\n'
  printf 'Proof boundary: disposable Git/process/PTY fixtures prove bounded noncanonical receipt input and restoration, process-group cancellation/reaping, remote advancement with an unchanged local checkout, origin reconciliation, definition binding, clean-main/index enforcement, private digest-first output, redaction, and negative-path denial only.\n'
}

MODE="${1:---check}"
[[ "$#" -le 1 ]] || { usage >&2; fail "the controller accepts no extra arguments"; }
case "$MODE" in
  --check) check_controller;;
  --run) run_controller "$ROOT_DIR";;
  --self-test) self_test_controller;;
  --help|-h) usage;;
  *) usage >&2; fail "unknown mode: $MODE";;
esac
