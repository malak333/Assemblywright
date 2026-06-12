#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

usage() {
  cat <<'USAGE'
Usage: scripts/packaged-app-release-smoke.sh [--help]

Deprecated compatibility wrapper. The canonical local packaged-app launch proof is:

  ./scripts/package-distribution.sh --unsigned-launch-check

That command builds the release app layout, creates the unsigned installer
payload, launches the release-built app executable with an isolated HOME, and
verifies bundled-core IPC smoke through the distribution layout.
USAGE
}

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
  "")
    ;;
  *)
    printf 'error: unsupported argument for deprecated wrapper: %s\n' "$1" >&2
    usage >&2
    exit 1
    ;;
esac

printf 'warning: scripts/packaged-app-release-smoke.sh is deprecated; running ./scripts/package-distribution.sh --unsigned-launch-check instead\n' >&2
exec "$ROOT_DIR/scripts/package-distribution.sh" --unsigned-launch-check
