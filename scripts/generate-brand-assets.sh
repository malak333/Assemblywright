#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SVG_DIR="assets/brand/svg"
OUT_DIR="assets/brand/generated"
CHECK_ONLY=false

usage() {
  cat <<'USAGE'
Usage: scripts/generate-brand-assets.sh [--check]

Rasterize the Assemblywright brand SVG sources in assets/brand/svg into the
platform binaries under assets/brand/generated: the macOS .icns, menu bar
template PNGs, favicon PNGs, favicon.ico, and seal PNGs.

Every generated file is reproducible from the SVG sources. Edit the SVGs, rerun
this script, and commit both.

--check regenerates into a temporary directory and fails if the committed
output differs, so CI can prove the generated assets match their sources.
USAGE
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || fail "$2"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      CHECK_ONLY=true
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      fail "unknown argument: $1"
      ;;
  esac
done

require_tool rsvg-convert 'rsvg-convert not found; install with: brew install librsvg'
require_tool iconutil 'iconutil not found; it ships with the Xcode command line tools'
require_tool python3 'python3 not found; it ships with the Xcode command line tools'

for source in \
  app-icon.svg \
  app-icon-small.svg \
  app-icon-micro.svg \
  menubar-template.svg \
  favicon.svg \
  favicon-micro.svg \
  web-tile.svg \
  seal.svg; do
  [[ -f "$SVG_DIR/$source" ]] || fail "missing brand source: $SVG_DIR/$source"
done

render() {
  local source="$1"
  local size="$2"
  local target="$3"
  rsvg-convert --width "$size" --height "$size" --output "$target" "$SVG_DIR/$source"
}

render_recolored() {
  local source="$1"
  local size="$2"
  local color="$3"
  local target="$4"
  local tmp_svg
  tmp_svg="$(mktemp "${TMPDIR:-/tmp}/assemblywright-brand.XXXXXX.svg")"
  sed "s/currentColor/$color/g" "$SVG_DIR/$source" >"$tmp_svg"
  rsvg-convert --width "$size" --height "$size" --output "$target" "$tmp_svg"
  rm -f "$tmp_svg"
}

build_icns() {
  local dest_dir="$1"
  local iconset
  iconset="$(mktemp -d "${TMPDIR:-/tmp}/assemblywright-iconset.XXXXXX")/Assemblywright.iconset"
  mkdir -p "$iconset"

  render app-icon-micro.svg 16 "$iconset/icon_16x16.png"
  render app-icon-small.svg 32 "$iconset/icon_16x16@2x.png"
  render app-icon-small.svg 32 "$iconset/icon_32x32.png"
  render app-icon-small.svg 64 "$iconset/icon_32x32@2x.png"
  render app-icon.svg 128 "$iconset/icon_128x128.png"
  render app-icon.svg 256 "$iconset/icon_128x128@2x.png"
  render app-icon.svg 256 "$iconset/icon_256x256.png"
  render app-icon.svg 512 "$iconset/icon_256x256@2x.png"
  render app-icon.svg 512 "$iconset/icon_512x512.png"
  render app-icon.svg 1024 "$iconset/icon_512x512@2x.png"

  iconutil --convert icns --output "$dest_dir/Assemblywright.icns" "$iconset"
  rm -rf "$(dirname "$iconset")"
}

build_ico() {
  local dest_dir="$1"
  python3 - "$dest_dir" <<'PY'
import struct
import sys
from pathlib import Path

dest = Path(sys.argv[1])
sizes = (16, 32, 48)
images = [(size, (dest / f"favicon-{size}.png").read_bytes()) for size in sizes]

header = struct.pack("<HHH", 0, 1, len(images))
offset = len(header) + 16 * len(images)
entries = bytearray()
payload = bytearray()
for size, data in images:
    entries += struct.pack(
        "<BBBBHHII",
        0 if size >= 256 else size,
        0 if size >= 256 else size,
        0,
        0,
        1,
        32,
        len(data),
        offset,
    )
    payload += data
    offset += len(data)

(dest / "favicon.ico").write_bytes(header + bytes(entries) + bytes(payload))
PY
}

generate_into() {
  local dest_dir="$1"
  mkdir -p "$dest_dir"

  build_icns "$dest_dir"

  render menubar-template.svg 18 "$dest_dir/menubar-template.png"
  render menubar-template.svg 36 "$dest_dir/menubar-template@2x.png"
  render menubar-template.svg 54 "$dest_dir/menubar-template@3x.png"

  render favicon-micro.svg 16 "$dest_dir/favicon-16.png"
  render favicon.svg 32 "$dest_dir/favicon-32.png"
  render favicon.svg 48 "$dest_dir/favicon-48.png"
  render web-tile.svg 180 "$dest_dir/apple-touch-icon.png"
  render web-tile.svg 192 "$dest_dir/web-tile-192.png"
  render web-tile.svg 512 "$dest_dir/web-tile-512.png"

  build_ico "$dest_dir"

  render_recolored seal.svg 512 '#15181B' "$dest_dir/seal-ink-512.png"
  render_recolored seal.svg 512 '#F7F5F2' "$dest_dir/seal-chalk-512.png"
}

if [[ "$CHECK_ONLY" == true ]]; then
  scratch="$(mktemp -d "${TMPDIR:-/tmp}/assemblywright-brand-check.XXXXXX")"
  trap 'rm -rf "$scratch"' EXIT
  generate_into "$scratch"
  if ! diff --recursive --brief "$OUT_DIR" "$scratch" >/dev/null 2>&1; then
    diff --recursive --brief "$OUT_DIR" "$scratch" >&2 || true
    fail "generated brand assets are stale; rerun scripts/generate-brand-assets.sh"
  fi
  printf 'Brand assets match their SVG sources: ok\n'
  exit 0
fi

rm -rf "$OUT_DIR"
generate_into "$OUT_DIR"

printf 'Wrote brand assets to %s:\n' "$OUT_DIR"
ls -1 "$OUT_DIR" | sed 's/^/  /'
