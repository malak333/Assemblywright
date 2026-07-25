#!/usr/bin/env bash
set -euo pipefail

# Cross-language protocol version contract.
#
# PROTOCOL_VERSION is declared independently in four places: the Rust protocol
# crate, the Swift bridge, and both Windows-local live control planes. Nothing
# derives one from another, and each language's tests only ever compare against
# its own declaration, so a partial bump passes every suite and then fails only
# against a live peer.
#
# Both halves of that failure have already shipped. Moving 1 -> 2 missed the
# Swift constant, which surfaced as a live-device handshake rejection after mTLS
# had already authenticated. The same bump also missed both PowerShell control
# planes, which have no test suite at all: they kept POSTing version 1 and the
# master answered "unsupported protocol version: expected 2, received 1" on
# every enqueue, so the fixture and MLX live lanes could not run.
#
# This gate compares the four declarations directly, and requires the PowerShell
# scripts to route every request and assertion through one named variable so a
# hardcoded literal cannot drift away from the value this check reads.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---check}"

RUST_DECLARATION="crates/assemblywright-protocol/src/lib.rs"
SWIFT_DECLARATION="apps/mac/Sources/AssemblywrightMacCore/DeveloperBridge.swift"
POWERSHELL_DECLARATIONS=(
  "scripts/windows-fixture-live-control.ps1"
  "scripts/windows-mlx-live-control.ps1"
)
PROTOCOL_PROSE_FILES=(
  "README.md"
  "docs/architecture-map.md"
  "crates/assemblywright-core/src/release.rs"
)

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
Usage: scripts/release-protocol-version-contract-smoke.sh [--check | --self-test]

  --check      Fail unless every language declares the same protocol version.
  --self-test  Prove the comparator detects each way the contract can break.
USAGE
}

# Each reader prints the declared version, or nothing when the declaration is
# absent. An absent declaration is a failure, never a silently skipped file.
read_rust_version() {
  sed -n 's/^pub const PROTOCOL_VERSION: u16 = \([0-9]\{1,\}\);$/\1/p' "$1" | head -n 1
}

read_swift_version() {
  sed -n 's/^ *public static let protocolVersion: UInt16 = \([0-9]\{1,\}\)$/\1/p' "$1" \
    | head -n 1
}

read_powershell_version() {
  sed -n 's/^\$protocolVersion = \([0-9]\{1,\}\)$/\1/p' "$1" | head -n 1
}

# True when a PowerShell control plane still spells a version as a literal at a
# request or assertion site instead of referencing the single declaration.
has_powershell_literal() {
  grep -Eq 'protocol_version (=|-ne) [0-9]' "$1"
}

has_protocol_version_prose_literal() {
  grep -Eq 'protocol version [0-9]+' "$1"
}

# Compare one Rust file, one Swift file, and zero or more PowerShell files.
# Prints every violation it finds and returns nonzero if there was any, so the
# self-test can drive it over fixtures without exiting the whole script.
compare_declarations() {
  local rust_path="$1"
  local swift_path="$2"
  shift 2
  local powershell_paths=()
  if [[ "$#" -gt 0 ]]; then
    powershell_paths=("$@")
  fi

  local violations=0
  local rust_version swift_version powershell_version powershell_path

  rust_version="$(read_rust_version "$rust_path")"
  if [[ -z "$rust_version" ]]; then
    printf 'error: could not read PROTOCOL_VERSION from %s\n' "$rust_path" >&2
    return 1
  fi

  swift_version="$(read_swift_version "$swift_path")"
  if [[ -z "$swift_version" ]]; then
    printf 'error: could not read protocolVersion from %s\n' "$swift_path" >&2
    violations=$((violations + 1))
  elif [[ "$swift_version" != "$rust_version" ]]; then
    printf 'error: protocol version mismatch: %s declares %s, %s declares %s\n' \
      "$rust_path" "$rust_version" "$swift_path" "$swift_version" >&2
    violations=$((violations + 1))
  fi

  for powershell_path in ${powershell_paths[@]+"${powershell_paths[@]}"}; do
    powershell_version="$(read_powershell_version "$powershell_path")"
    if [[ -z "$powershell_version" ]]; then
      printf 'error: could not read $protocolVersion from %s\n' "$powershell_path" >&2
      violations=$((violations + 1))
      continue
    fi
    if [[ "$powershell_version" != "$rust_version" ]]; then
      printf 'error: protocol version mismatch: %s declares %s, %s declares %s\n' \
        "$rust_path" "$rust_version" "$powershell_path" "$powershell_version" >&2
      violations=$((violations + 1))
    fi
    if has_powershell_literal "$powershell_path"; then
      printf 'error: %s hardcodes a protocol version literal; use $protocolVersion\n' \
        "$powershell_path" >&2
      violations=$((violations + 1))
    fi
  done

  [[ "$violations" -eq 0 ]]
}

check_repository() {
  local path
  for path in "$RUST_DECLARATION" "$SWIFT_DECLARATION" \
    ${POWERSHELL_DECLARATIONS[@]+"${POWERSHELL_DECLARATIONS[@]}"} \
    ${PROTOCOL_PROSE_FILES[@]+"${PROTOCOL_PROSE_FILES[@]}"}; do
    [[ -f "$ROOT_DIR/$path" ]] || fail "missing required file: $path"
  done

  ( cd "$ROOT_DIR" && compare_declarations \
    "$RUST_DECLARATION" "$SWIFT_DECLARATION" \
    ${POWERSHELL_DECLARATIONS[@]+"${POWERSHELL_DECLARATIONS[@]}"} ) \
    || fail "the declared protocol version is not the same in every language"

  local declared
  declared="$(read_rust_version "$ROOT_DIR/$RUST_DECLARATION")"
  for path in ${PROTOCOL_PROSE_FILES[@]+"${PROTOCOL_PROSE_FILES[@]}"}; do
    if has_protocol_version_prose_literal "$ROOT_DIR/$path"; then
      fail "$path hardcodes a protocol version in user-visible prose"
    fi
  done
  printf 'Assemblywright protocol version contract: ok (version %s in %s declarations)\n' \
    "$declared" "$((2 + ${#POWERSHELL_DECLARATIONS[@]}))"
}

# Prove the comparator rather than asserting it. Every fixture below is a way
# the contract has actually broken or could break next.
self_test() {
  # Not local: the EXIT trap runs after this function's scope is gone, and a
  # trap that dereferences a dead local aborts under set -u.
  fixture_dir="$(mktemp -d -t assemblywright-protocol-version-contract)"
  trap 'rm -rf -- "$fixture_dir"' EXIT

  write_rust_fixture() {
    printf 'pub const PROTOCOL_VERSION: u16 = %s;\n' "$1" >"$fixture_dir/lib.rs"
  }
  write_swift_fixture() {
    printf '    public static let protocolVersion: UInt16 = %s\n' "$1" \
      >"$fixture_dir/Bridge.swift"
  }
  write_powershell_fixture() {
    local filename="$1"
    local version="$2"
    local site="$3"
    {
      printf '$protocolVersion = %s\n' "$version"
      printf '            protocol_version = %s\n' "$site"
      printf '            $batch.protocol_version -ne %s -or\n' "$site"
    } >"$fixture_dir/$filename"
  }

  # A fully aligned set is the only shape that passes.
  write_rust_fixture 2
  write_swift_fixture 2
  write_powershell_fixture "aligned.ps1" 2 '$protocolVersion'
  compare_declarations "$fixture_dir/lib.rs" "$fixture_dir/Bridge.swift" \
    "$fixture_dir/aligned.ps1" >/dev/null 2>&1 \
    || fail "self-test: the comparator rejected an aligned declaration set"

  # The f6339c9 defect: Rust moved and Swift did not.
  write_swift_fixture 1
  if compare_declarations "$fixture_dir/lib.rs" "$fixture_dir/Bridge.swift" \
    "$fixture_dir/aligned.ps1" >/dev/null 2>&1; then
    fail "self-test: the comparator accepted a stale Swift declaration"
  fi
  write_swift_fixture 2

  # The defect this gate was added for: both control planes kept version 1.
  write_powershell_fixture "stale.ps1" 1 '$protocolVersion'
  if compare_declarations "$fixture_dir/lib.rs" "$fixture_dir/Bridge.swift" \
    "$fixture_dir/stale.ps1" >/dev/null 2>&1; then
    fail "self-test: the comparator accepted a stale PowerShell declaration"
  fi

  # A literal at a request site drifts independently of the declaration, so the
  # aligned declaration above must not be enough to pass on its own.
  write_powershell_fixture "literal.ps1" 2 '1'
  if compare_declarations "$fixture_dir/lib.rs" "$fixture_dir/Bridge.swift" \
    "$fixture_dir/literal.ps1" >/dev/null 2>&1; then
    fail "self-test: the comparator accepted a hardcoded PowerShell literal"
  fi

  # A declaration that is missing entirely must fail, never be skipped.
  printf 'nothing to see here\n' >"$fixture_dir/absent.ps1"
  if compare_declarations "$fixture_dir/lib.rs" "$fixture_dir/Bridge.swift" \
    "$fixture_dir/absent.ps1" >/dev/null 2>&1; then
    fail "self-test: the comparator accepted a missing PowerShell declaration"
  fi
  printf 'nothing to see here\n' >"$fixture_dir/absent.swift"
  if compare_declarations "$fixture_dir/lib.rs" "$fixture_dir/absent.swift" \
    "$fixture_dir/aligned.ps1" >/dev/null 2>&1; then
    fail "self-test: the comparator accepted a missing Swift declaration"
  fi

  # One misaligned file among several aligned ones must still fail.
  write_powershell_fixture "second-stale.ps1" 1 '$protocolVersion'
  if compare_declarations "$fixture_dir/lib.rs" "$fixture_dir/Bridge.swift" \
    "$fixture_dir/aligned.ps1" "$fixture_dir/second-stale.ps1" >/dev/null 2>&1; then
    fail "self-test: the comparator accepted one stale file beside an aligned one"
  fi

  printf 'The current protocol version owns this seam.\n' >"$fixture_dir/prose.txt"
  if has_protocol_version_prose_literal "$fixture_dir/prose.txt"; then
    fail "self-test: the prose scanner rejected version-independent text"
  fi
  printf 'The protocol version 1 owns this seam.\n' >"$fixture_dir/prose.txt"
  if ! has_protocol_version_prose_literal "$fixture_dir/prose.txt"; then
    fail "self-test: the prose scanner accepted a hardcoded protocol version"
  fi

  printf 'Assemblywright protocol version contract self-test: ok\n'
  printf 'Proof boundary: declaration comparison and user-visible prose scanning only; no live peer, handshake, signing, notarization, or live-device evidence was produced.\n'
}

case "$MODE" in
  --check)
    check_repository
    ;;
  --self-test)
    self_test
    ;;
  -h | --help)
    usage
    ;;
  *)
    usage >&2
    fail "unknown mode: $MODE"
    ;;
esac
