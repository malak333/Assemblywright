#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CHECK_ONLY=false

usage() {
  cat <<'USAGE'
Usage: scripts/release-version-consistency.sh --check

Validate that Assemblywright release scripts can derive a single canonical release
version from Rust package metadata before distribution or evidence gates run.
USAGE
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
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

if [[ "$CHECK_ONLY" != true ]]; then
  fail "--check is required"
fi

"$ROOT_DIR/scripts/release-version.sh" --check

VERSION="$("$ROOT_DIR/scripts/release-version.sh")"
printf 'Assemblywright release script version gate: ok (%s)\n' "$VERSION"
printf 'Proof boundary: release-version derivation only; no app was built, signed, notarized, stapled, installed, launched, or manually validated.\n'
