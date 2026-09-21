#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"
PACKAGE_PATH="$ROOT_DIR/apps/mac"
UI_TEST_ONE='AssemblywrightMacAppTests.DeveloperProjectChatTests/shiftReturnInsertsNewlineAtCursorAndReplacesSelection()'
UI_TEST_TWO='AssemblywrightMacAppTests.DeveloperProjectChatTests/approvalViewPresentsExactDetailsAndDecisions()'
# Exact copies of the release-local expressions. The CI workflow smoke binds
# these constants to the literal command manifest so either copy drifting fails.
UI_FILTER_REGEX='^AssemblywrightMacAppTests\.DeveloperProjectChatTests/(shiftReturnInsertsNewlineAtCursorAndReplacesSelection|approvalViewPresentsExactDetailsAndDecisions)\(\)(/.*)?$'
BRIDGE_FILTER_REGEX='^AssemblywrightMacCoreTests\.DeveloperBridgeTests/'

scratch="$(mktemp -d "${TMPDIR:-/tmp}/assemblywright-swift-partition.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT HUP INT TERM

verify_partition() {
  local discovered="$1"
  local ui_filter_regex="$UI_FILTER_REGEX"
  local bridge_filter_regex="$BRIDGE_FILTER_REGEX"
  [[ "$#" -lt 2 ]] || ui_filter_regex="$2"
  [[ "$#" -lt 3 ]] || bridge_filter_regex="$3"
  local duplicates="$scratch/duplicate-tests"
  local all_count=0
  local ui_count=0
  local bridge_count=0
  local remaining_count=0
  local ui_one_count=0
  local ui_two_count=0
  local membership_failures=0
  local test_id

  while IFS= read -r test_id; do
    [[ -n "$test_id" ]] || continue
    local ui_match=0
    local bridge_match=0
    local remaining_match=0
    [[ "$test_id" =~ $ui_filter_regex ]] && ui_match=1
    [[ "$test_id" =~ $bridge_filter_regex ]] && bridge_match=1
    [[ "$ui_match" -eq 0 && "$bridge_match" -eq 0 ]] && remaining_match=1
    if [[ $((ui_match + bridge_match + remaining_match)) -ne 1 ]]; then
      membership_failures=$((membership_failures + 1))
    fi
    [[ "$ui_match" -eq 0 ]] || ui_count=$((ui_count + 1))
    [[ "$test_id" != "$UI_TEST_ONE" ]] || ui_one_count=$((ui_one_count + 1))
    [[ "$test_id" != "$UI_TEST_TWO" ]] || ui_two_count=$((ui_two_count + 1))
    [[ "$bridge_match" -eq 0 ]] || bridge_count=$((bridge_count + 1))
    [[ "$remaining_match" -eq 0 ]] || remaining_count=$((remaining_count + 1))
    all_count=$((all_count + 1))
  done <"$discovered"

  LC_ALL=C sort "$discovered" | uniq -d >"$duplicates"
  [[ ! -s "$duplicates" ]] || {
    printf 'error: Swift test discovery contains duplicate specifiers\n' >&2
    return 1
  }
  [[ "$membership_failures" -eq 0 ]] || {
    printf 'error: a discovered Swift test matches overlapping release partitions\n' >&2
    return 1
  }
  [[ "$ui_one_count" -eq 1 && "$ui_two_count" -eq 1 && "$ui_count" -eq 2 ]] || {
    printf 'error: Swift AppKit partition must contain its two exact tests once each\n' >&2
    return 1
  }
  [[ "$bridge_count" -gt 0 ]] || {
    printf 'error: Swift DeveloperBridge partition is empty\n' >&2
    return 1
  }
  [[ "$remaining_count" -gt 0 ]] || {
    printf 'error: remaining Swift test partition is empty\n' >&2
    return 1
  }
  [[ $((ui_count + bridge_count + remaining_count)) -eq "$all_count" ]] || {
    printf 'error: Swift test partitions do not cover the discovered set\n' >&2
    return 1
  }

  printf '%s %s %s %s\n' "$all_count" "$ui_count" "$bridge_count" "$remaining_count"
}

expect_rejection() {
  local discovered="$1"
  local label="$2"
  local ui_filter_regex="$UI_FILTER_REGEX"
  local bridge_filter_regex="$BRIDGE_FILTER_REGEX"
  [[ "$#" -lt 3 ]] || ui_filter_regex="$3"
  [[ "$#" -lt 4 ]] || bridge_filter_regex="$4"
  if verify_partition \
      "$discovered" "$ui_filter_regex" "$bridge_filter_regex" >/dev/null 2>&1; then
    printf 'error: Swift partition self-test unexpectedly accepted %s\n' "$label" >&2
    exit 1
  fi
}

if [[ "${1-}" == "--self-test" ]]; then
  valid="$scratch/valid"
  missing_ui="$scratch/missing-ui"
  duplicate="$scratch/duplicate"
  empty_bridge="$scratch/empty-bridge"
  collision="$scratch/collision"
  empty_remaining="$scratch/empty-remaining"
  printf '%s\n%s\n%s\n%s\n' \
    "$UI_TEST_ONE" "$UI_TEST_TWO" \
    'AssemblywrightMacCoreTests.DeveloperBridgeTests/sample()' \
    'AssemblywrightMacCoreTests.OtherTests/sample()' >"$valid"
  printf '%s\n%s\n%s\n' \
    "$UI_TEST_ONE" \
    'AssemblywrightMacCoreTests.DeveloperBridgeTests/sample()' \
    'AssemblywrightMacCoreTests.OtherTests/sample()' >"$missing_ui"
  printf '%s\n%s\n%s\n%s\n%s\n' \
    "$UI_TEST_ONE" "$UI_TEST_TWO" "$UI_TEST_ONE" \
    'AssemblywrightMacCoreTests.DeveloperBridgeTests/sample()' \
    'AssemblywrightMacCoreTests.OtherTests/sample()' >"$duplicate"
  printf '%s\n%s\n%s\n' \
    "$UI_TEST_ONE" "$UI_TEST_TWO" \
    'AssemblywrightMacCoreTests.OtherTests/sample()' >"$empty_bridge"
  printf '%s\n%s\n%s\n%s\n' \
    "$UI_TEST_ONE" "$UI_TEST_TWO" \
    'AssemblywrightMacCoreTests.DeveloperBridgeTests/collision()' \
    'AssemblywrightMacCoreTests.OtherTests/sample()' >"$collision"
  printf '%s\n%s\n%s\n' \
    "$UI_TEST_ONE" "$UI_TEST_TWO" \
    'AssemblywrightMacCoreTests.DeveloperBridgeTests/sample()' >"$empty_remaining"
  verify_partition "$valid" >/dev/null
  expect_rejection "$missing_ui" "missing UI test"
  expect_rejection "$duplicate" "duplicate discovery"
  expect_rejection "$empty_bridge" "empty bridge partition"
  expect_rejection \
    "$collision" "overlapping regex predicates" \
    "$UI_FILTER_REGEX|^AssemblywrightMacCoreTests\.DeveloperBridgeTests/collision\(\)$" \
    "$BRIDGE_FILTER_REGEX"
  expect_rejection "$empty_remaining" "empty remaining partition"
  printf 'Assemblywright Swift test partition self-test: ok\n'
  exit 0
fi

[[ "$#" -eq 0 ]] || {
  printf 'usage: %s [--self-test]\n' "$0" >&2
  exit 2
}
command -v swift >/dev/null 2>&1 || {
  printf 'error: swift is required for the Swift test partition smoke\n' >&2
  exit 1
}
discovered="$scratch/discovered-tests"
swift test --package-path "$PACKAGE_PATH" list >"$discovered"
read -r all_count ui_count bridge_count remaining_count < <(verify_partition "$discovered")
printf 'Assemblywright Swift test partition smoke: ok (%s = %s UI + %s bridge + %s remaining)\n' \
  "$all_count" "$ui_count" "$bridge_count" "$remaining_count"
