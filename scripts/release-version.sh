#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CHECK_ONLY=false

usage() {
  cat <<'USAGE'
Usage: scripts/release-version.sh [--check]

Print the canonical Assemblywright release version from Rust package metadata.

--check validates that assemblywright-protocol, assemblywright-master, assemblywright-core, assemblywright-agent,
assemblywright-cli, and their local dependency constraints all agree before printing a
human-readable status.
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
  [[ -f "$path" ]] || fail "missing assemblywright-cli manifest: $path"
  version="$(sed -nE 's/^assemblywright-core = .*version = "([^"]+)".*/\1/p' "$path" | head -n 1)"
  [[ -n "$version" ]] || fail "missing assemblywright-core dependency version in assemblywright-cli manifest: $path"
  printf '%s\n' "$version"
}

read_core_protocol_dependency_version() {
  local path="$1"
  local version
  [[ -f "$path" ]] || fail "missing assemblywright-core manifest: $path"
  version="$(sed -nE 's/^assemblywright-protocol = .*version = "([^"]+)".*/\1/p' "$path" | head -n 1)"
  [[ -n "$version" ]] || fail "missing assemblywright-protocol dependency version in assemblywright-core manifest: $path"
  printf '%s\n' "$version"
}

read_master_protocol_dependency_version() {
  local path="$1"
  local version
  [[ -f "$path" ]] || fail "missing assemblywright-master manifest: $path"
  version="$(sed -nE 's/^assemblywright-protocol = .*version = "([^"]+)".*/\1/p' "$path" | head -n 1)"
  [[ -n "$version" ]] || fail "missing assemblywright-protocol dependency version in assemblywright-master manifest: $path"
  printf '%s\n' "$version"
}

read_agent_dependency_version() {
  local path="$1"
  local dependency="$2"
  local version
  [[ -f "$path" ]] || fail "missing assemblywright-agent manifest: $path"
  version="$(sed -nE "s/^${dependency} = .*version = \"([^\"]+)\".*/\\1/p" "$path" | head -n 1)"
  [[ -n "$version" ]] || fail "missing $dependency dependency version in assemblywright-agent manifest: $path"
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

PROTOCOL_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/assemblywright-protocol/Cargo.toml" "assemblywright-protocol")"
MASTER_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/assemblywright-master/Cargo.toml" "assemblywright-master")"
AGENT_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/assemblywright-agent/Cargo.toml" "assemblywright-agent")"
CORE_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/assemblywright-core/Cargo.toml" "assemblywright-core")"
CLI_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/assemblywright-cli/Cargo.toml" "assemblywright-cli")"
MASTER_PROTOCOL_DEPENDENCY_VERSION="$(read_master_protocol_dependency_version "$ROOT_DIR/crates/assemblywright-master/Cargo.toml")"
CORE_PROTOCOL_DEPENDENCY_VERSION="$(read_core_protocol_dependency_version "$ROOT_DIR/crates/assemblywright-core/Cargo.toml")"
CLI_CORE_DEPENDENCY_VERSION="$(read_cli_core_dependency_version "$ROOT_DIR/crates/assemblywright-cli/Cargo.toml")"
AGENT_CORE_DEPENDENCY_VERSION="$(read_agent_dependency_version "$ROOT_DIR/crates/assemblywright-agent/Cargo.toml" "assemblywright-core")"
AGENT_PROTOCOL_DEPENDENCY_VERSION="$(read_agent_dependency_version "$ROOT_DIR/crates/assemblywright-agent/Cargo.toml" "assemblywright-protocol")"

if [[ "$CORE_VERSION" != "$PROTOCOL_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$MASTER_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$AGENT_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$MASTER_PROTOCOL_DEPENDENCY_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$CLI_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$CORE_PROTOCOL_DEPENDENCY_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$CLI_CORE_DEPENDENCY_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$AGENT_CORE_DEPENDENCY_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$AGENT_PROTOCOL_DEPENDENCY_VERSION" ]]; then
  fail "release version mismatch: assemblywright-protocol=$PROTOCOL_VERSION, assemblywright-master=$MASTER_VERSION, assemblywright-agent=$AGENT_VERSION, assemblywright-core=$CORE_VERSION, assemblywright-cli=$CLI_VERSION, assemblywright-master assemblywright-protocol dependency=$MASTER_PROTOCOL_DEPENDENCY_VERSION, assemblywright-core assemblywright-protocol dependency=$CORE_PROTOCOL_DEPENDENCY_VERSION, assemblywright-agent assemblywright-core dependency=$AGENT_CORE_DEPENDENCY_VERSION, assemblywright-agent assemblywright-protocol dependency=$AGENT_PROTOCOL_DEPENDENCY_VERSION, assemblywright-cli assemblywright-core dependency=$CLI_CORE_DEPENDENCY_VERSION"
fi

if [[ "$CHECK_ONLY" == true ]]; then
  printf 'Assemblywright release version consistency: ok (%s)\n' "$CORE_VERSION"
  printf 'Proof boundary: Rust package metadata agreement only; no app was built, signed, notarized, stapled, installed, launched, or manually validated.\n'
else
  printf '%s\n' "$CORE_VERSION"
fi
