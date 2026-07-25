#!/usr/bin/env bash
set -euo pipefail

# Shell portability contract.
#
# Every script here runs under `set -u` on the macOS system bash, which is 3.2.
# There, expanding an empty array as "${name[@]}" aborts the script with
# `name[@]: unbound variable` instead of expanding to nothing. Bash 4.4+ and zsh
# do not, so the defect is invisible on a modern shell and fires only on the
# release Mac, only in whichever code path leaves the array empty.
#
# The portable idiom is ${name[@]+"${name[@]}"}: the outer expansion is
# unquoted so it disappears entirely when the array is unset, and the inner one
# stays quoted so populated elements keep their word boundaries. A length guard
# around the expansion is equally correct.
#
# This check fails on any new unguarded expansion. Entries in ALLOWED_UNGUARDED
# are expansions reviewed and proven safe; each records why. Add to it only with
# a justification of the same kind, never to silence a finding.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---check}"

# Reviewed expansions that cannot be reached with an empty array.
ALLOWED_UNGUARDED=(
  # Expanded only inside the --run-fixture branch that populates it.
  "scripts/mac-windows-bridge-live-e2e.sh:identity_profile_arguments"
  # Initialized from a non-empty literal list of template filenames.
  "scripts/package-distribution.sh:MENU_BAR_TEMPLATE_FILES"
  # A hard credential check fails the run before either notarization call.
  "scripts/package-distribution.sh:notary_args"
  # Both loops sit inside an explicit ${#...[@]} -gt 0 length guard.
  "scripts/release-evidence-doctor.sh:SATISFIED_ITEMS"
  "scripts/release-evidence-doctor.sh:MISSING_ITEMS"
  # Initialized from a non-empty literal manifest of expected gate commands.
  "scripts/release-ci-workflow-smoke.sh:expected_local_gate_commands"
  # This contract script has to contain the forbidden spelling to describe and
  # to test it: `name` appears in the comment that documents the rule, `empty`
  # in the bash -c string the self-test executes to prove the 3.2 behavior, and
  # `extra_arguments` in the deliberate-violation fixtures. None is an
  # expansion this script ever performs. They are named individually rather
  # than exempting the file, so a genuine unguarded expansion added here still
  # fails. Its one live array, ALLOWED_UNGUARDED, is expanded guarded.
  "scripts/release-shell-portability-smoke.sh:name"
  "scripts/release-shell-portability-smoke.sh:empty"
  "scripts/release-shell-portability-smoke.sh:extra_arguments"
)

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
Usage: scripts/release-shell-portability-smoke.sh [--check | --self-test]

  --check      Fail on any unguarded empty-capable array expansion.
  --self-test  Prove the bash 3.2 behavior and that the scanner detects it.
USAGE
}

# Report every unguarded array expansion in one file as "line:array".
# The guarded idiom embeds the unguarded spelling, so strip complete guarded
# constructs before looking for what remains.
scan_file() {
  local path="$1"
  sed -E 's/\$\{[A-Za-z_][A-Za-z0-9_]*\[@\]\+"\$\{[A-Za-z_][A-Za-z0-9_]*\[@\]\}"\}//g' "$path" \
    | grep -noE '"\$\{[A-Za-z_][A-Za-z0-9_]*\[@\]\}"' \
    | sed -E 's/^([0-9]+):"\$\{([A-Za-z_][A-Za-z0-9_]*)\[@\]\}"$/\1:\2/' \
    || true
}

# True when the script opts into nounset, the only case this contract governs.
uses_nounset() {
  grep -qE '^[[:space:]]*set[[:space:]]+-[a-z]*u' "$1"
}

allowed() {
  local candidate="$1"
  local entry
  for entry in ${ALLOWED_UNGUARDED[@]+"${ALLOWED_UNGUARDED[@]}"}; do
    [[ "$entry" == "$candidate" ]] && return 0
  done
  return 1
}

check_repository() {
  local violations=0
  local scanned=0
  local relative_path absolute_path finding line_number name

  while IFS= read -r relative_path; do
    absolute_path="$ROOT_DIR/$relative_path"
    [[ -f "$absolute_path" ]] || continue
    uses_nounset "$absolute_path" || continue
    scanned=$((scanned + 1))
    while IFS= read -r finding; do
      [[ -n "$finding" ]] || continue
      line_number="${finding%%:*}"
      name="${finding##*:}"
      if allowed "$relative_path:$name"; then
        continue
      fi
      printf 'error: %s:%s expands %s unguarded; use ${%s[@]+"${%s[@]}"}\n' \
        "$relative_path" "$line_number" "$name" "$name" "$name" >&2
      violations=$((violations + 1))
    done < <(scan_file "$absolute_path")
  done < <(cd "$ROOT_DIR" && git ls-files '*.sh')

  [[ "$scanned" -gt 0 ]] || fail "no nounset shell scripts were scanned"
  if [[ "$violations" -gt 0 ]]; then
    fail "$violations unguarded array expansion(s) would abort under macOS bash 3.2"
  fi

  printf 'Assemblywright shell portability smoke: ok (%s scripts scanned)\n' "$scanned"
}

# Pin the runtime behavior this contract exists to prevent, so the rule is
# proven rather than asserted.
self_test_runtime_behavior() {
  if bash -c 'set -euo pipefail; empty=(); printf "%s" "${empty[@]}"' >/dev/null 2>&1; then
    fail "self-test: expected an unguarded empty expansion to abort under set -u"
  fi
  bash -c 'set -euo pipefail; empty=(); printf "%s" ${empty[@]+"${empty[@]}"}' >/dev/null \
    || fail "self-test: the guarded idiom failed on an empty array"
  local populated
  populated="$(
    bash -c 'set -euo pipefail; items=(first second); printf "%s\n" ${items[@]+"${items[@]}"}'
  )"
  [[ "$populated" == "first"$'\n'"second" ]] \
    || fail "self-test: the guarded idiom did not preserve populated elements"
}

# Prove the scanner itself detects a violation and accepts the fix.
self_test_scanner() {
  local fixture_dir
  fixture_dir="$(mktemp -d -t assemblywright-shell-portability)"
  chmod 700 "$fixture_dir"
  # shellcheck disable=SC2064
  trap "rm -rf -- '$fixture_dir'" RETURN

  cat >"$fixture_dir/violation.sh" <<'FIXTURE'
#!/usr/bin/env bash
set -euo pipefail
extra_arguments=()
env "${extra_arguments[@]}" true
FIXTURE

  cat >"$fixture_dir/guarded.sh" <<'FIXTURE'
#!/usr/bin/env bash
set -euo pipefail
extra_arguments=()
env ${extra_arguments[@]+"${extra_arguments[@]}"} true
FIXTURE

  cat >"$fixture_dir/opted-out.sh" <<'FIXTURE'
#!/usr/bin/env bash
set -eo pipefail
extra_arguments=()
env "${extra_arguments[@]}" true
FIXTURE

  local detected
  detected="$(scan_file "$fixture_dir/violation.sh")"
  [[ "$detected" == "4:extra_arguments" ]] \
    || fail "self-test: scanner missed the unguarded expansion, saw '$detected'"

  detected="$(scan_file "$fixture_dir/guarded.sh")"
  [[ -z "$detected" ]] \
    || fail "self-test: scanner rejected the guarded idiom, saw '$detected'"

  uses_nounset "$fixture_dir/violation.sh" \
    || fail "self-test: nounset detection missed an opted-in script"
  if uses_nounset "$fixture_dir/opted-out.sh"; then
    fail "self-test: nounset detection claimed an opted-out script"
  fi

  allowed "scripts/package-distribution.sh:notary_args" \
    || fail "self-test: allowlist rejected a reviewed entry"
  if allowed "scripts/package-distribution.sh:not_reviewed"; then
    fail "self-test: allowlist accepted an unreviewed entry"
  fi

  # The fixture violation must genuinely abort, not merely look wrong.
  if bash "$fixture_dir/violation.sh" >/dev/null 2>&1; then
    fail "self-test: the fixture violation did not abort"
  fi
  bash "$fixture_dir/guarded.sh" >/dev/null 2>&1 \
    || fail "self-test: the guarded fixture did not run"
}

case "$MODE" in
  --check)
    check_repository
    ;;
  --self-test)
    self_test_runtime_behavior
    self_test_scanner
    printf 'Assemblywright shell portability self-test: ok\n'
    printf 'Proof boundary: shell expansion behavior and scanner mechanics only; no build, signing, notarization, or live-device evidence was produced.\n'
    ;;
  --help | -h)
    usage
    ;;
  *)
    usage >&2
    fail "unknown mode: $MODE"
    ;;
esac
