#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-packaged-supervision.XXXXXX")"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

APP_RESOURCES_BIN="$TMP_DIR/Jarvis.app/Contents/Resources/bin"
BUNDLED_CORE="$APP_RESOURCES_BIN/jarvis-cli"

run cargo build -p jarvis-cli

mkdir -p "$APP_RESOURCES_BIN"
cp "$ROOT_DIR/target/debug/jarvis" "$BUNDLED_CORE"
chmod 755 "$BUNDLED_CORE"

if [[ ! -x "$BUNDLED_CORE" ]]; then
  printf 'error: packaged supervision proof did not create an executable core at %s\n' "$BUNDLED_CORE" >&2
  exit 1
fi

run env JARVIS_MAC_CORE_EXECUTABLE="$BUNDLED_CORE" swift test --package-path apps/mac
run cargo run -p jarvis-cli -- smoke

printf '\nJarvis packaged supervision proof: ok (%s)\n' "$BUNDLED_CORE"
