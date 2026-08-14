#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_RELATIVE="target/repository-gate-proof"
RECEIPT_NAME="repository-gate-proof.json"
DIGEST_NAME="repository-gate-proof.sha256"
SCHEMA="assemblywright.repository-gate-proof.v1"
CATEGORY="repository_gate_proof"
ORIGIN="repository_gate_proof_controller"
GATE_IDENTITY="assemblywright.release-local.v1"
PROOF_BOUNDARY="Exact clean main at origin/main ran the exact committed local-gate bytes with pre/post same-UID mutation edge checks, not host isolation; this is not activation admission, signing, notarization, live-device, restricted-worker, review-provider, GitHub-publication, restart-recovery, Mac/Windows-control, or production-readiness proof."
INTERNAL_STDIN_MARKER_NAME="ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_STDIN_V1"
INTERNAL_ROOT_MARKER_NAME="ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_ROOT"
unset ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_STDIN_V1
unset ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_ROOT

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
Usage: scripts/repository-gate-proof-controller.sh [--check | --run | --self-test]

  --check      Validate the fixed controller prerequisites without running the gate.
  --run        Run only ./scripts/release-local.sh from an exact clean main equal
               to refs/remotes/origin/main, then atomically write the fixed receipt.
  --self-test  Exercise success and fail-closed controller behavior in disposable repos.

The controller never admits activation evidence, posts to a service, activates the
Feature Conveyor, accepts a repository path, or accepts an alternate command.
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
    || fail "repository target identity changed while the gate ran"
  [[ "$(directory_identity "$output_directory")" == "$prepared_output_identity" ]] \
    || fail "proof output path identity changed while the gate ran"
  [[ "$(directory_identity .)" == "$prepared_output_identity" ]] \
    || fail "held proof output identity changed while the gate ran"
  [[ "$(pwd -P)" == "$output_directory" ]] \
    || fail "held proof output path changed while the gate ran"
}

capture_repository_state() {
  local root="$1"
  local prefix="$2"
  local top branch head origin tree status definition_digest
  top="$(git_safe "$root" rev-parse --show-toplevel)" \
    || fail "repository root could not be resolved"
  [[ "$top" == "$root" ]] || fail "controller must run from its own repository root"
  branch="$(git_safe "$root" symbolic-ref -q HEAD)" \
    || fail "detached HEAD is not eligible for repository-gate proof"
  [[ "$branch" == "refs/heads/main" ]] || fail "repository-gate proof requires exact main"
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
  [[ -z "$status" ]] || fail "repository-gate proof requires a clean working tree"
  definition_digest="$(git_safe "$root" show "$head:scripts/release-local.sh" | sha256_stdin)" \
    || fail "committed release-local definition could not be hashed"
  validate_hash "$head" || fail "HEAD has an unsupported object-id shape"
  validate_hash "$tree" || fail "HEAD tree has an unsupported object-id shape"
  validate_sha256 "$definition_digest" \
    || fail "release-local definition digest has the wrong shape"
  eval "${prefix}_branch=\$branch"
  eval "${prefix}_head=\$head"
  eval "${prefix}_origin=\$origin"
  eval "${prefix}_tree=\$tree"
  eval "${prefix}_status=\$status"
  eval "${prefix}_definition_digest=\$definition_digest"
}

write_receipt_atomically() {
  local head="$1"
  local tree="$2"
  local definition_digest="$3"
  local observed_at_ms="$4"
  local receipt_temp digest_temp receipt_sha256 receipt_bytes
  receipt_temp="$(mktemp .repository-gate-proof.json.XXXXXX)" \
    || fail "could not allocate receipt temporary file"
  digest_temp="$(mktemp .repository-gate-proof.sha256.XXXXXX)" \
    || {
      rm -f -- "$receipt_temp" || true
      fail "could not allocate digest temporary file"
    }
  chmod 600 "$receipt_temp" "$digest_temp" || {
    rm -f -- "$receipt_temp" "$digest_temp" || true
    fail "could not restrict proof temporary files"
  }
  printf '{"schema":"%s","category":"%s","origin":"%s","head_commit":"%s","tree_id":"%s","release_local_definition_sha256":"%s","gate_identity":"%s","observed_at_ms":%s,"status":"passed","proof_boundary":"%s"}\n' \
    "$SCHEMA" "$CATEGORY" "$ORIGIN" "$head" "$tree" "$definition_digest" \
    "$GATE_IDENTITY" "$observed_at_ms" "$PROOF_BOUNDARY" >"$receipt_temp" \
    || {
      rm -f -- "$receipt_temp" "$digest_temp" || true
      fail "could not write proof receipt"
    }
  receipt_bytes="$(wc -c <"$receipt_temp" | tr -d '[:space:]')" \
    || {
      rm -f -- "$receipt_temp" "$digest_temp" || true
      fail "could not measure proof receipt"
    }
  [[ "$receipt_bytes" -le 2048 ]] || {
    rm -f -- "$receipt_temp" "$digest_temp" || true
    fail "receipt exceeded the fixed 2048-byte bound"
  }
  receipt_sha256="$(sha256_file "$receipt_temp")" \
    || {
      rm -f -- "$receipt_temp" "$digest_temp" || true
      fail "receipt digest could not be computed"
    }
  validate_sha256 "$receipt_sha256" || {
    rm -f -- "$receipt_temp" "$digest_temp" || true
    fail "receipt digest has the wrong shape"
  }
  printf '%s\n' "$receipt_sha256" >"$digest_temp" || {
    rm -f -- "$receipt_temp" "$digest_temp" || true
    fail "could not write receipt digest"
  }
  mv -f -- "$digest_temp" "$DIGEST_NAME" || {
    rm -f -- "$receipt_temp" "$digest_temp" "$DIGEST_NAME" || true
    fail "could not publish receipt digest"
  }
  mv -f -- "$receipt_temp" "$RECEIPT_NAME" || {
    rm -f -- "$receipt_temp" "$RECEIPT_NAME" "$DIGEST_NAME" || true
    fail "could not publish receipt commit marker"
  }
}

clear_gate_git_environment() {
  local name
  while IFS='=' read -r name _value; do
    case "$name" in
      GIT_* | BASH_ENV | ENV | ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_STDIN_V1 | ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_ROOT)
        unset "$name"
        ;;
    esac
  done < <(env)
}

terminate_gate_group() {
  local count=0
  [[ "$gate_pid" =~ ^[1-9][0-9]*$ ]] || return 0
  kill -TERM -- "-$gate_pid" >/dev/null 2>&1 || true
  while kill -0 -- "-$gate_pid" >/dev/null 2>&1; do
    count=$((count + 1))
    if [[ "$count" -ge 50 ]]; then
      kill -KILL -- "-$gate_pid" >/dev/null 2>&1 || true
      break
    fi
    sleep 0.1
  done
  wait "$gate_pid" >/dev/null 2>&1 || true
  gate_pid=""
}

wait_for_gate_group_drain() {
  local count=0
  [[ "$gate_pid" =~ ^[1-9][0-9]*$ ]] || return 0
  while kill -0 -- "-$gate_pid" >/dev/null 2>&1; do
    count=$((count + 1))
    [[ "$count" -lt 50 ]] || return 1
    sleep 0.1
  done
}

run_committed_gate() {
  local root="$1"
  local head="$2"
  local gate_status
  local group_remained=0
  set -m
  (
    set +m
    git_safe "$root" show "$head:scripts/release-local.sh" |
      (
        cd "$root" || exit 1
        clear_gate_git_environment
        export ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_STDIN_V1="$SCHEMA"
        export ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_ROOT="$root"
        exec bash -s
      )
  ) &
  gate_pid="$!"
  set +m
  [[ "$gate_pid" =~ ^[1-9][0-9]*$ ]] || fail "canonical gate process could not be identified"
  set +e
  wait "$gate_pid"
  gate_status="$?"
  set -e
  if [[ "$gate_status" -ne 0 ]]; then
    terminate_gate_group
    fail "committed canonical release-local gate failed"
  fi
  if ! wait_for_gate_group_drain; then
    group_remained=1
    terminate_gate_group
  else
    gate_pid=""
  fi
  [[ "$group_remained" -eq 0 ]] || fail "committed canonical gate left a live descendant"
}

run_controller() (
  local root
  root="$(cd "$1" && pwd -P)"
  local before_branch before_head before_origin before_tree before_status
  local before_definition_digest after_branch after_head after_origin after_tree
  local after_status after_definition_digest observed_at_ms published=0
  local output_prepared=0
  local prepared_target_identity="" prepared_output_identity="" gate_pid=""
  local published_digest actual_digest

  cleanup_controller() {
    terminate_gate_group
    if [[ "$output_prepared" -eq 1 && "$published" -ne 1 ]]; then
      rm -f -- "$RECEIPT_NAME" "$DIGEST_NAME" \
        .repository-gate-proof.json.* \
        .repository-gate-proof.sha256.* 2>/dev/null || true
    fi
  }
  handle_controller_signal() {
    local exit_status="$1"
    trap - HUP INT TERM
    terminate_gate_group
    exit "$exit_status"
  }
  trap cleanup_controller EXIT
  trap 'handle_controller_signal 129' HUP
  trap 'handle_controller_signal 130' INT
  trap 'handle_controller_signal 143' TERM

  prepare_output_directory "$root"
  capture_repository_state "$root" before

  run_committed_gate "$root" "$before_head"

  capture_repository_state "$root" after
  [[ "$after_branch" == "$before_branch" ]] || fail "branch changed while the gate ran"
  [[ "$after_head" == "$before_head" ]] || fail "HEAD changed while the gate ran"
  [[ "$after_origin" == "$before_origin" ]] || fail "origin/main changed while the gate ran"
  [[ "$after_tree" == "$before_tree" ]] || fail "HEAD tree changed while the gate ran"
  [[ "$after_status" == "$before_status" ]] || fail "working-tree status changed while the gate ran"
  [[ "$after_definition_digest" == "$before_definition_digest" ]] \
    || fail "committed release-local definition changed while the gate ran"
  revalidate_output_identity "$root"

  observed_at_ms="$(date -u '+%s')000"
  [[ "$observed_at_ms" =~ ^[0-9]{13}$ ]] || fail "observed time has the wrong shape"
  write_receipt_atomically \
    "$before_head" "$before_tree" "$before_definition_digest" "$observed_at_ms"
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
  printf 'Assemblywright repository-gate proof controller: passed\n'
  printf 'Receipt: %s\n' "$OUTPUT_RELATIVE/$RECEIPT_NAME"
  printf 'Receipt SHA-256: %s\n' "$OUTPUT_RELATIVE/$DIGEST_NAME"
  printf 'Proof boundary: %s\n' "$PROOF_BOUNDARY"
)

check_controller() {
  local command_name
  for command_name in git shasum awk mktemp date id stat chmod mkdir mv rm wc tr grep find ln cat env sleep sed; do
    require_command "$command_name"
  done
  [[ -x "$ROOT_DIR/scripts/release-local.sh" ]] \
    || fail "canonical ./scripts/release-local.sh is unavailable or not executable"
  [[ -f "$ROOT_DIR/.gitignore" ]] || fail "repository .gitignore is unavailable"
  git_safe "$ROOT_DIR" check-ignore -q "$OUTPUT_RELATIVE/$RECEIPT_NAME" \
    || fail "fixed proof output is not ignored"
  grep -Fq 'run ./scripts/repository-gate-proof-controller.sh --check' \
    "$ROOT_DIR/scripts/release-local.sh" \
    || fail "release-local does not run the repository-gate controller check"
  grep -Fq 'run ./scripts/repository-gate-proof-controller.sh --self-test' \
    "$ROOT_DIR/scripts/release-local.sh" \
    || fail "release-local does not run the repository-gate controller self-test"
  grep -Fq 'ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_STDIN_V1' \
    "$ROOT_DIR/scripts/release-local.sh" \
    || fail "release-local does not recognize the committed-stdin marker"
  grep -Fq '${BASH_SOURCE[0]-}' "$ROOT_DIR/scripts/release-local.sh" \
    || fail "release-local internal stdin detection is not Bash 3.2 nounset-safe"
  if grep -Fq 'repository-gate-proof-controller.sh --run' "$ROOT_DIR/scripts/release-local.sh"; then
    fail "release-local must not recursively run repository-gate proof"
  fi
  printf 'Assemblywright repository-gate proof controller check: ok\n'
  printf 'Proof boundary: static controller prerequisites only; the repository gate was not run and no receipt was created.\n'
}

write_fixture_gate() {
  local fixture="$1"
  local body="$2"
  mkdir -p "$fixture/scripts"
  printf '#!/usr/bin/env bash\nset -euo pipefail\n%s\n' "$body" >"$fixture/scripts/release-local.sh"
  chmod 700 "$fixture/scripts/release-local.sh"
}

initialize_fixture() {
  local fixture="$1"
  local body="$2"
  mkdir -p "$fixture"
  git_safe "$fixture" init -q
  git_safe "$fixture" checkout -q -b main
  git_safe "$fixture" config user.name 'Assemblywright Controller Self Test'
  git_safe "$fixture" config user.email 'controller-self-test@invalid.example'
  printf 'target/\n' >"$fixture/.gitignore"
  printf 'fixture\n' >"$fixture/README.md"
  write_fixture_gate "$fixture" "$body"
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
    find "$output_directory" -type f -name '.repository-gate-proof.*' -print -quit \
      | grep -q .; then
    fail "self-test left a temporary proof file after rejected execution"
  fi
}

expect_controller_failure() {
  local fixture="$1"
  if run_controller "$fixture" >/dev/null 2>&1; then
    fail "self-test expected controller rejection"
  fi
  assert_no_proof_output "$fixture"
}

self_test_controller() {
  local scratch success dirty wrong_branch gate_failure origin_drift status_drift cancellation
  local hostile_target hostile_external directory_swap writable_target environment_hardening
  local hidden_index hidden_skip_worktree digest_move_failure receipt_move_failure mv_wrapper_dir
  local natural_drain persistent_descendant failed_gate_descendant
  local failed_gate_sentinel
  local cancellation_sentinel cancellation_ready swapped_output controller_pid controller_status
  local cancellation_wait_count
  local receipt digest expected_digest actual_digest receipt_bytes file_count
  local success_head success_tree success_gate_digest
  local internal_probe_root internal_probe_output
  scratch="$(mktemp -d -t assemblywright-repository-gate-proof)"
  chmod 700 "$scratch"
  # shellcheck disable=SC2064
  trap "rm -rf -- '$scratch'" RETURN

  internal_probe_root="$scratch/internal-release-local-probe"
  mkdir -m 700 "$internal_probe_root"
  internal_probe_root="$(cd "$internal_probe_root" && pwd -P)"
  internal_probe_output="$({
    sed '/^run() {/,$d' "$ROOT_DIR/scripts/release-local.sh"
    printf '%s\n' 'printf "%s" "$ROOT_DIR"'
  } | ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_STDIN_V1="$SCHEMA" \
      ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_ROOT="$internal_probe_root" \
      CLANG_MODULE_CACHE_PATH="$internal_probe_root/target/clang-module-cache" \
      BASH_ENV=/dev/null ENV=/dev/null bash -s)"
  [[ "$internal_probe_output" == "$internal_probe_root" ]] \
    || fail "self-test release-local internal stdin/root marker drifted"

  "$ROOT_DIR/scripts/repository-gate-proof-controller.sh" >/dev/null \
    || fail "self-test default mode did not perform the static check"
  if "$ROOT_DIR/scripts/repository-gate-proof-controller.sh" --unknown-mode \
      >/dev/null 2>&1; then
    fail "self-test accepted an unknown controller mode"
  fi
  if "$ROOT_DIR/scripts/repository-gate-proof-controller.sh" --check extra \
      >/dev/null 2>&1; then
    fail "self-test accepted an extra controller argument"
  fi

  success="$scratch/success"
  initialize_fixture "$success" '[[ -z "${BASH_SOURCE[0]-}" ]]'
  run_controller "$success" >/dev/null
  receipt="$success/$OUTPUT_RELATIVE/$RECEIPT_NAME"
  digest="$success/$OUTPUT_RELATIVE/$DIGEST_NAME"
  [[ -f "$receipt" && ! -L "$receipt" ]] || fail "self-test success receipt is absent"
  [[ -f "$digest" && ! -L "$digest" ]] || fail "self-test success digest is absent"
  [[ "$(stat -f '%Lp' "$receipt")" == "600" ]] || fail "self-test receipt mode drifted"
  [[ "$(stat -f '%Lp' "$digest")" == "600" ]] || fail "self-test digest mode drifted"
  expected_digest="$(tr -d '[:space:]' <"$digest")"
  actual_digest="$(sha256_file "$receipt")"
  [[ "$expected_digest" == "$actual_digest" ]] || fail "self-test receipt digest mismatch"
  receipt_bytes="$(wc -c <"$receipt" | tr -d '[:space:]')"
  [[ "$receipt_bytes" -le 2048 ]] || fail "self-test receipt is not bounded"
  file_count="$(find "$success/$OUTPUT_RELATIVE" -type f | wc -l | tr -d '[:space:]')"
  [[ "$file_count" == "2" ]] || fail "self-test success output is not fixed-shape"
  grep -Fq '"schema":"assemblywright.repository-gate-proof.v1"' "$receipt" \
    || fail "self-test receipt omitted schema"
  grep -Fq '"category":"repository_gate_proof"' "$receipt" \
    || fail "self-test receipt omitted category"
  grep -Fq '"origin":"repository_gate_proof_controller"' "$receipt" \
    || fail "self-test receipt omitted origin"
  grep -Fq '"gate_identity":"assemblywright.release-local.v1"' "$receipt" \
    || fail "self-test receipt omitted fixed gate identity"
  grep -Fq '"status":"passed"' "$receipt" || fail "self-test receipt omitted pass status"
  success_head="$(git_safe "$success" rev-parse 'HEAD^{commit}')"
  success_tree="$(git_safe "$success" rev-parse 'HEAD^{tree}')"
  success_gate_digest="$(sha256_file "$success/scripts/release-local.sh")"
  grep -Fq "\"head_commit\":\"$success_head\"" "$receipt" \
    || fail "self-test receipt did not bind exact HEAD"
  grep -Fq "\"tree_id\":\"$success_tree\"" "$receipt" \
    || fail "self-test receipt did not bind exact tree"
  grep -Fq "\"release_local_definition_sha256\":\"$success_gate_digest\"" "$receipt" \
    || fail "self-test receipt did not bind the committed gate definition"
  grep -Eq '"observed_at_ms":[0-9]{13}' "$receipt" \
    || fail "self-test receipt omitted bounded observed time"
  if grep -Fq "$success" "$receipt" || grep -Eq '(/Users/|/private/|/tmp/|https?://|github\.com)' "$receipt"; then
    fail "self-test receipt leaked a path or remote"
  fi

  printf 'dirty rerun\n' >"$success/dirty-rerun.txt"
  expect_controller_failure "$success"

  hidden_index="$scratch/hidden-index"
  initialize_fixture "$hidden_index" ':'
  git_safe "$hidden_index" update-index --assume-unchanged scripts/release-local.sh
  printf '#!/usr/bin/env bash\nexit 97\n' >"$hidden_index/scripts/release-local.sh"
  chmod 700 "$hidden_index/scripts/release-local.sh"
  expect_controller_failure "$hidden_index"

  hidden_skip_worktree="$scratch/hidden-skip-worktree"
  initialize_fixture "$hidden_skip_worktree" ':'
  git_safe "$hidden_skip_worktree" update-index --skip-worktree scripts/release-local.sh
  printf '#!/usr/bin/env bash\nexit 98\n' >"$hidden_skip_worktree/scripts/release-local.sh"
  chmod 700 "$hidden_skip_worktree/scripts/release-local.sh"
  expect_controller_failure "$hidden_skip_worktree"

  dirty="$scratch/dirty"
  initialize_fixture "$dirty" ':'
  printf 'dirty\n' >"$dirty/untracked.txt"
  expect_controller_failure "$dirty"

  wrong_branch="$scratch/wrong-branch"
  initialize_fixture "$wrong_branch" ':'
  git_safe "$wrong_branch" checkout -q -b feature
  expect_controller_failure "$wrong_branch"

  gate_failure="$scratch/gate-failure"
  initialize_fixture "$gate_failure" 'exit 23'
  expect_controller_failure "$gate_failure"

  failed_gate_descendant="$scratch/failed-gate-descendant"
  failed_gate_sentinel="$scratch/failed-gate-descendant-survived"
  initialize_fixture "$failed_gate_descendant" \
    "(sleep 2; printf survived >'$failed_gate_sentinel') & exit 23"
  expect_controller_failure "$failed_gate_descendant"
  sleep 3
  [[ ! -e "$failed_gate_sentinel" ]] \
    || fail "self-test failed gate left a live descendant"

  origin_drift="$scratch/origin-drift"
  initialize_fixture "$origin_drift" \
    'git update-ref refs/remotes/origin/main 0000000000000000000000000000000000000000'
  expect_controller_failure "$origin_drift"

  status_drift="$scratch/status-drift"
  initialize_fixture "$status_drift" 'printf drift >post-gate-drift.txt'
  expect_controller_failure "$status_drift"

  natural_drain="$scratch/natural-drain"
  initialize_fixture "$natural_drain" '(sleep 1) &'
  run_controller "$natural_drain" >/dev/null

  persistent_descendant="$scratch/persistent-descendant"
  initialize_fixture "$persistent_descendant" \
    "(trap '' TERM; while :; do sleep 1; done) &"
  expect_controller_failure "$persistent_descendant"

  cancellation="$scratch/cancellation"
  cancellation_sentinel="$scratch/cancellation-descendant-survived"
  cancellation_ready="$scratch/cancellation-ready"
  initialize_fixture "$cancellation" \
    "(sleep 2; printf survived >'$cancellation_sentinel') & printf ready >'$cancellation_ready'; wait"
  set -m
  run_controller "$cancellation" >/dev/null 2>&1 &
  controller_pid="$!"
  set +m
  cancellation_wait_count=0
  while [[ ! -e "$cancellation_ready" ]]; do
    cancellation_wait_count=$((cancellation_wait_count + 1))
    if [[ "$cancellation_wait_count" -ge 50 ]]; then
      kill -TERM "$controller_pid" >/dev/null 2>&1 || true
      wait "$controller_pid" >/dev/null 2>&1 || true
      fail "self-test cancellation gate did not start"
    fi
    sleep 0.1
  done
  kill -TERM -- "-$controller_pid" || fail "self-test could not cancel the controller"
  set +e
  wait "$controller_pid" >/dev/null 2>&1
  controller_status="$?"
  set -e
  [[ "$controller_status" -eq 143 ]] \
    || fail "self-test controller cancellation returned the wrong status"
  sleep 3
  [[ ! -e "$cancellation_sentinel" ]] \
    || fail "self-test cancellation left a live gate descendant"
  assert_no_proof_output "$cancellation"

  hostile_target="$scratch/hostile-target"
  hostile_external="$scratch/hostile-external"
  initialize_fixture "$hostile_target" ':'
  mkdir -m 700 "$hostile_external"
  printf 'preserve\n' >"$hostile_external/sentinel"
  ln -s "$hostile_external" "$hostile_target/target"
  expect_controller_failure "$hostile_target"
  [[ "$(cat "$hostile_external/sentinel")" == "preserve" ]] \
    || fail "self-test hostile target changed external state"
  [[ ! -e "$hostile_external/repository-gate-proof" ]] \
    || fail "self-test hostile target escaped the repository"

  writable_target="$scratch/writable-target"
  initialize_fixture "$writable_target" ':'
  mkdir -m 777 "$writable_target/target"
  chmod 777 "$writable_target/target"
  expect_controller_failure "$writable_target"

  directory_swap="$scratch/directory-swap"
  initialize_fixture "$directory_swap" \
    'mv target target-swapped; mkdir -m 700 target; mkdir -m 700 target/repository-gate-proof'
  expect_controller_failure "$directory_swap"
  swapped_output="$directory_swap/target-swapped/repository-gate-proof"
  [[ ! -e "$swapped_output/$RECEIPT_NAME" && ! -e "$swapped_output/$DIGEST_NAME" ]] \
    || fail "self-test directory swap retained output through the held directory"

  environment_hardening="$scratch/environment-hardening"
  initialize_fixture "$environment_hardening" \
    '[[ "${ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_STDIN_V1:-}" == "assemblywright.repository-gate-proof.v1" ]]; [[ "${ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_ROOT:-}" == "$PWD" ]]; [[ -z "${GIT_DIR:-}${GIT_WORK_TREE:-}${GIT_INDEX_FILE:-}${GIT_OBJECT_DIRECTORY:-}${GIT_ALTERNATE_OBJECT_DIRECTORIES:-}${GIT_COMMON_DIR:-}${GIT_REPLACE_REF_BASE:-}${GIT_SHALLOW_FILE:-}${GIT_NAMESPACE:-}${GIT_CONFIG_COUNT:-}" ]]'
  GIT_DIR="$scratch/redirected.git" \
    GIT_WORK_TREE="$scratch/redirected-worktree" \
    GIT_INDEX_FILE="$scratch/redirected-index" \
    GIT_OBJECT_DIRECTORY="$scratch/redirected-objects" \
    GIT_REPLACE_REF_BASE="refs/evil/" \
    GIT_ALTERNATE_OBJECT_DIRECTORIES="$scratch/alternate-objects" \
    GIT_COMMON_DIR="$scratch/redirected-common" \
    GIT_SHALLOW_FILE="$scratch/redirected-shallow" \
    GIT_NAMESPACE="evil" \
    GIT_CONFIG_COUNT=1 \
    GIT_CONFIG_KEY_0=core.fsmonitor \
    GIT_CONFIG_VALUE_0="$scratch/evil-fsmonitor" \
    ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_STDIN_V1="caller-controlled" \
    ASSEMBLYWRIGHT_REPOSITORY_GATE_INTERNAL_ROOT="$scratch/caller-root" \
    run_controller "$environment_hardening" >/dev/null

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
  initialize_fixture "$digest_move_failure" ':'
  if PATH="$mv_wrapper_dir:$PATH" FAIL_MV_DEST="$DIGEST_NAME" \
    run_controller "$digest_move_failure" >/dev/null 2>&1; then
    fail "self-test expected digest publication failure"
  fi
  assert_no_proof_output "$digest_move_failure"

  receipt_move_failure="$scratch/receipt-move-failure"
  initialize_fixture "$receipt_move_failure" ':'
  if PATH="$mv_wrapper_dir:$PATH" FAIL_MV_DEST="$RECEIPT_NAME" \
    run_controller "$receipt_move_failure" >/dev/null 2>&1; then
    fail "self-test expected receipt publication failure"
  fi
  assert_no_proof_output "$receipt_move_failure"

  printf 'Assemblywright repository-gate proof controller self-test: ok\n'
  printf 'Proof boundary: disposable Git/process fixtures prove CLI shape, committed-byte execution structure, dirty/hidden-index/wrong-branch/origin/status drift, stale-receipt invalidation, immediate failed-gate descendant suppression, atomic-move failure, success-only natural process-group drain, persistent-descendant rejection, cancellation, hostile/swap/writable-target denial, environment hardening, redaction, digest, permissions, and no-output behavior only.\n'
}

MODE="${1:---check}"
[[ "$#" -le 1 ]] || {
  usage >&2
  fail "the controller accepts no repository, command, or extra arguments"
  exit 1
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
    exit 1
    ;;
esac
