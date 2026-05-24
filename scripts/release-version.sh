#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CHECK_ONLY=false

usage() {
  cat <<'USAGE'
Usage: scripts/release-version.sh [--check]

Print the canonical Jarvis release version from Rust package metadata.

--check validates that jarvis-core, jarvis-cli, and the jarvis-cli dependency
constraint for jarvis-core all agree before printing a human-readable status.
USAGE
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

read_toml_package_version() {
  local path="$1"
  local label="$2"
  local version
  [[ -f "$path" ]] || fail "missing $label manifest: $path"
  version="$(sed -nE 's/^version = "([^"]+)".*/\1/p' "$path" | head -n 1)"
  [[ -n "$version" ]] || fail "missing package version in $label manifest: $path"
  printf '%s\n' "$version"
}

read_cli_core_dependency_version() {
  local path="$1"
  local version
  [[ -f "$path" ]] || fail "missing jarvis-cli manifest: $path"
  version="$(sed -nE 's/^jarvis-core = .*version = "([^"]+)".*/\1/p' "$path" | head -n 1)"
  [[ -n "$version" ]] || fail "missing jarvis-core dependency version in jarvis-cli manifest: $path"
  printf '%s\n' "$version"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      CHECK_ONLY=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

CORE_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/jarvis-core/Cargo.toml" "jarvis-core")"
CLI_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/jarvis-cli/Cargo.toml" "jarvis-cli")"
CLI_CORE_DEPENDENCY_VERSION="$(read_cli_core_dependency_version "$ROOT_DIR/crates/jarvis-cli/Cargo.toml")"

if [[ "$CORE_VERSION" != "$CLI_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$CLI_CORE_DEPENDENCY_VERSION" ]]; then
  fail "release version mismatch: jarvis-core=$CORE_VERSION, jarvis-cli=$CLI_VERSION, jarvis-cli jarvis-core dependency=$CLI_CORE_DEPENDENCY_VERSION"
fi

if [[ "$CHECK_ONLY" == true ]]; then
  printf 'Jarvis release version consistency: ok (%s)\n' "$CORE_VERSION"
  printf 'Proof boundary: Rust package metadata agreement only; no app was built, signed, notarized, stapled, installed, launched, or manually validated.\n'
else
  printf '%s\n' "$CORE_VERSION"
fi
