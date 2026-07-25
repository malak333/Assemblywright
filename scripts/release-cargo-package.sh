#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

version="$("$ROOT_DIR/scripts/release-version.sh")"
package_dir="$ROOT_DIR/target/package"
protocol_crate="$package_dir/jarvis-protocol-$version.crate"
core_crate="$package_dir/jarvis-core-$version.crate"
cli_crate="$package_dir/jarvis-cli-$version.crate"

cleanup=""
if [[ "${1:-}" == "--keep-temp" ]]; then
  keep_temp=true
else
  keep_temp=false
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-package-verify.XXXXXX")"
cleanup="$tmp_dir"
trap 'if [[ -n "$cleanup" && "$keep_temp" != true ]]; then rm -rf "$cleanup"; fi' EXIT

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

run cargo package --workspace --allow-dirty --no-verify

if [[ ! -f "$protocol_crate" ]]; then
  printf 'error: expected packaged protocol crate at %s\n' "$protocol_crate" >&2
  exit 1
fi

if [[ ! -f "$core_crate" ]]; then
  printf 'error: expected packaged core crate at %s\n' "$core_crate" >&2
  exit 1
fi

if [[ ! -f "$cli_crate" ]]; then
  printf 'error: expected packaged CLI crate at %s\n' "$cli_crate" >&2
  exit 1
fi

tar -xzf "$protocol_crate" -C "$tmp_dir"
tar -xzf "$core_crate" -C "$tmp_dir"
tar -xzf "$cli_crate" -C "$tmp_dir"

protocol_dir="$tmp_dir/jarvis-protocol-$version"
cli_dir="$tmp_dir/jarvis-cli-$version"
core_dir="$tmp_dir/jarvis-core-$version"

if [[ ! -d "$protocol_dir" || ! -d "$cli_dir" || ! -d "$core_dir" ]]; then
  printf 'error: packaged crate extraction did not produce expected directories in %s\n' "$tmp_dir" >&2
  exit 1
fi

cat >>"$core_dir/Cargo.toml" <<PATCH

[patch.crates-io]
jarvis-protocol = { path = "../jarvis-protocol-$version" }
PATCH

cat >>"$cli_dir/Cargo.toml" <<PATCH

[patch.crates-io]
jarvis-core = { path = "../jarvis-core-$version" }
PATCH

run cargo check --manifest-path "$core_dir/Cargo.toml" --all-targets --features distributed-development
run cargo check --manifest-path "$cli_dir/Cargo.toml" --all-targets

if [[ "$keep_temp" == true ]]; then
  printf '\nKept package verification temp dir: %s\n' "$tmp_dir"
  cleanup=""
fi

printf '\nAssemblywright cargo packaging verification: ok\n'
