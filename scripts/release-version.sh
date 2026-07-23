#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CHECK_ONLY=false

usage() {
  cat <<'USAGE'
Usage: scripts/release-version.sh [--check]

Print the canonical Jarvis release version from Rust package metadata.

--check validates that jarvis-protocol, jarvis-master, jarvis-core, jarvis-agent,
jarvis-cli, and their local dependency constraints all agree before printing a
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
  [[ -f "$path" ]] || fail "missing jarvis-cli manifest: $path"
  version="$(sed -nE 's/^jarvis-core = .*version = "([^"]+)".*/\1/p' "$path" | head -n 1)"
  [[ -n "$version" ]] || fail "missing jarvis-core dependency version in jarvis-cli manifest: $path"
  printf '%s\n' "$version"
}

read_core_protocol_dependency_version() {
  local path="$1"
  local version
  [[ -f "$path" ]] || fail "missing jarvis-core manifest: $path"
  version="$(sed -nE 's/^jarvis-protocol = .*version = "([^"]+)".*/\1/p' "$path" | head -n 1)"
  [[ -n "$version" ]] || fail "missing jarvis-protocol dependency version in jarvis-core manifest: $path"
  printf '%s\n' "$version"
}

read_master_protocol_dependency_version() {
  local path="$1"
  local version
  [[ -f "$path" ]] || fail "missing jarvis-master manifest: $path"
  version="$(sed -nE 's/^jarvis-protocol = .*version = "([^"]+)".*/\1/p' "$path" | head -n 1)"
  [[ -n "$version" ]] || fail "missing jarvis-protocol dependency version in jarvis-master manifest: $path"
  printf '%s\n' "$version"
}

read_agent_dependency_version() {
  local path="$1"
  local dependency="$2"
  local version
  [[ -f "$path" ]] || fail "missing jarvis-agent manifest: $path"
  version="$(sed -nE "s/^${dependency} = .*version = \"([^\"]+)\".*/\\1/p" "$path" | head -n 1)"
  [[ -n "$version" ]] || fail "missing $dependency dependency version in jarvis-agent manifest: $path"
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

PROTOCOL_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/jarvis-protocol/Cargo.toml" "jarvis-protocol")"
MASTER_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/jarvis-master/Cargo.toml" "jarvis-master")"
AGENT_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/jarvis-agent/Cargo.toml" "jarvis-agent")"
CORE_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/jarvis-core/Cargo.toml" "jarvis-core")"
CLI_VERSION="$(read_toml_package_version "$ROOT_DIR/crates/jarvis-cli/Cargo.toml" "jarvis-cli")"
MASTER_PROTOCOL_DEPENDENCY_VERSION="$(read_master_protocol_dependency_version "$ROOT_DIR/crates/jarvis-master/Cargo.toml")"
CORE_PROTOCOL_DEPENDENCY_VERSION="$(read_core_protocol_dependency_version "$ROOT_DIR/crates/jarvis-core/Cargo.toml")"
CLI_CORE_DEPENDENCY_VERSION="$(read_cli_core_dependency_version "$ROOT_DIR/crates/jarvis-cli/Cargo.toml")"
AGENT_CORE_DEPENDENCY_VERSION="$(read_agent_dependency_version "$ROOT_DIR/crates/jarvis-agent/Cargo.toml" "jarvis-core")"
AGENT_PROTOCOL_DEPENDENCY_VERSION="$(read_agent_dependency_version "$ROOT_DIR/crates/jarvis-agent/Cargo.toml" "jarvis-protocol")"

if [[ "$CORE_VERSION" != "$PROTOCOL_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$MASTER_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$AGENT_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$MASTER_PROTOCOL_DEPENDENCY_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$CLI_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$CORE_PROTOCOL_DEPENDENCY_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$CLI_CORE_DEPENDENCY_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$AGENT_CORE_DEPENDENCY_VERSION" ]] ||
  [[ "$CORE_VERSION" != "$AGENT_PROTOCOL_DEPENDENCY_VERSION" ]]; then
  fail "release version mismatch: jarvis-protocol=$PROTOCOL_VERSION, jarvis-master=$MASTER_VERSION, jarvis-agent=$AGENT_VERSION, jarvis-core=$CORE_VERSION, jarvis-cli=$CLI_VERSION, jarvis-master jarvis-protocol dependency=$MASTER_PROTOCOL_DEPENDENCY_VERSION, jarvis-core jarvis-protocol dependency=$CORE_PROTOCOL_DEPENDENCY_VERSION, jarvis-agent jarvis-core dependency=$AGENT_CORE_DEPENDENCY_VERSION, jarvis-agent jarvis-protocol dependency=$AGENT_PROTOCOL_DEPENDENCY_VERSION, jarvis-cli jarvis-core dependency=$CLI_CORE_DEPENDENCY_VERSION"
fi

if [[ "$CHECK_ONLY" == true ]]; then
  printf 'Jarvis release version consistency: ok (%s)\n' "$CORE_VERSION"
  printf 'Proof boundary: Rust package metadata agreement only; no app was built, signed, notarized, stapled, installed, launched, or manually validated.\n'
else
  printf '%s\n' "$CORE_VERSION"
fi
