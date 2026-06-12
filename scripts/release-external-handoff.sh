#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUTPUT_DIR="${JARVIS_RELEASE_HANDOFF_DIR:-$ROOT_DIR/target/release-external-handoff}"
ENDPOINT="${JARVIS_RELEASE_HANDOFF_ENDPOINT:-http://127.0.0.1:7787}"
CHECK_ONLY=false
WRITE=false
SELF_TEST=false

usage() {
  cat <<'USAGE'
Usage: scripts/release-external-handoff.sh [--check|--write DIR|--self-test]

Prepare a single release-operator handoff directory for the external Jarvis
production evidence gates.

--check validates repo-owned handoff prerequisites and prints the external
evidence sequence. It does not write files.

--write DIR writes sourceable env templates and read-only JSON snapshots:
  release-live-device-qa.env
  release-plugin-trust-qa.env
  release-evidence-bundle.env
  release-readiness.json
  release-evidence-status.json
  signed-distribution-runbook.json
  live-device-runbook.json
  plugin-trust-runbook.json
  README.md

--self-test writes the handoff into a temporary directory and verifies that the
templates and snapshots are present with validation flags still defaulted false.

Optional:
  JARVIS_RELEASE_HANDOFF_DIR       Default output directory for --write
  JARVIS_RELEASE_HANDOFF_ENDPOINT  Endpoint used by CLI read-only snapshots.
                                   Defaults to http://127.0.0.1:7787; the CLI
                                   falls back to local read-only metadata when
                                   the endpoint is unavailable.

Proof boundary: this script generates operator handoff files and read-only
snapshots only. It does not sign, notarize, staple, install, Finder-launch,
validate live microphone/Speech/audio/notifications, run marketplace review,
scan malware, enforce an OS sandbox, enforce host-level egress, or archive final
production evidence.
USAGE
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

run() {
  printf '==> %s\n' "$*" >&2
  "$@"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
}

require_file() {
  local label="$1"
  local path="$2"
  [[ -f "$path" ]] || fail "missing $label: $path"
}

require_file_contains() {
  local label="$1"
  local path="$2"
  local expected="$3"
  require_file "$label" "$path"
  if ! grep -F "$expected" "$path" >/dev/null 2>&1; then
    fail "$label does not mention required text: $expected"
  fi
}

require_json_key() {
  local label="$1"
  local path="$2"
  local key="$3"
  require_file "$label" "$path"
  python3 - "$path" "$key" <<'PY'
import json
import sys

path, key = sys.argv[1:3]
try:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
except Exception as exc:
    raise SystemExit(f"{path} is not valid JSON: {exc}")

if key not in data:
    raise SystemExit(f"{path} is missing top-level key {key}")
PY
}

write_readme() {
  local output_dir="$1"
  local generated_at
  generated_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  cat >"$output_dir/README.md" <<EOF
# Jarvis External Release Handoff

Generated at: $generated_at

This directory contains sourceable templates and read-only snapshots for the
external/manual evidence gates that remain before a production-ready Jarvis
claim.

## Files

- \`release-live-device-qa.env\`: fill on the clean release Mac after installed
  app, Finder launch, live microphone/Speech, transcript handoff, live audio,
  notification, restart, and manual release QA have actually passed.
- \`release-plugin-trust-qa.env\`: fill after marketplace review, malware scan,
  signed publisher policy review, OS sandbox validation, host-level egress deny
  and declared-host allow fixtures, and manual plugin trust review have actually
  passed.
- \`release-evidence-bundle.env\`: fill after signed/notarized distribution,
  live-device QA, plugin-trust QA, and report archival have actually completed.
- \`release-readiness.json\`, \`release-evidence-status.json\`, and the three
  \`*-runbook.json\` files: read-only snapshots from the current checkout.

## Ordered Release Sequence

1. Run the signed distribution lane with Developer ID and notarytool credentials:
   \`JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh\`
2. Install the signed, notarized package into \`/Applications\` on a clean Mac
   profile and complete the live-device checks. Then source
   \`release-live-device-qa.env\` and run
   \`./scripts/release-live-device-qa.sh --assert-complete\`.
3. Complete the plugin trust review checks. Then source
   \`release-plugin-trust-qa.env\` and run
   \`./scripts/release-plugin-trust-qa.sh --assert-complete\`.
4. Archive the signed distribution, signed provenance, live-device QA report,
   plugin-trust QA report, and supporting external evidence in a durable
   release location. Then source \`release-evidence-bundle.env\` and run
   \`./scripts/release-evidence-bundle.sh --bundle\`.
5. Run \`./scripts/release-evidence-doctor.sh --assert-complete\`.
6. Start or restart the release core with
   \`JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external\`, then run
   \`cargo run -p jarvis-cli -- release evidence-status --endpoint <release-core-endpoint>\`
   and \`cargo run -p jarvis-cli -- release readiness --endpoint <release-core-endpoint>\`.

Proof boundary: these files are handoff scaffolding only. They do not prove that
signing, notarization, stapling, installation, Finder launch, live-device QA,
plugin trust QA, host-level egress enforcement, or final evidence archival have
been performed.
EOF
}

check_prerequisites() {
  require_command cargo
  require_command python3
  require_command date
  [[ -x ./scripts/release-live-device-qa.sh ]] || fail "missing executable scripts/release-live-device-qa.sh"
  [[ -x ./scripts/release-plugin-trust-qa.sh ]] || fail "missing executable scripts/release-plugin-trust-qa.sh"
  [[ -x ./scripts/release-evidence-bundle.sh ]] || fail "missing executable scripts/release-evidence-bundle.sh"
  [[ -x ./scripts/package-distribution.sh ]] || fail "missing executable scripts/package-distribution.sh"
}

print_check() {
  cat <<'CHECK'
Jarvis external release handoff preflight: ok

This repo can prepare the operator handoff files, but production readiness still
requires external evidence:
- Developer ID signing, notarization, and stapling for app zip and installer.
- Clean-profile /Applications install and Finder/LaunchServices launch.
- Live microphone, Speech permission, transcript handoff, audio output,
  notification delivery, restart, and manual release QA on a real Mac.
- Marketplace review, malware scan, signed publisher policy, OS sandbox
  validation, host-level egress deny/allow fixtures, and manual plugin trust QA.
- Durable archival of signed artifacts, reports, and supporting evidence.

Write the handoff directory:
  ./scripts/release-external-handoff.sh --write target/release-external-handoff

Proof boundary: preflight only; no external release validation was performed.
CHECK
}

write_handoff() {
  local output_dir="$1"
  mkdir -p "$output_dir"

  run ./scripts/release-live-device-qa.sh --write-template "$output_dir/release-live-device-qa.env"
  run ./scripts/release-plugin-trust-qa.sh --write-template "$output_dir/release-plugin-trust-qa.env"
  run ./scripts/release-evidence-bundle.sh --write-template "$output_dir/release-evidence-bundle.env"

  run cargo run -q -p jarvis-cli -- release readiness --json --endpoint "$ENDPOINT" >"$output_dir/release-readiness.json"
  run cargo run -q -p jarvis-cli -- release evidence-status --json --endpoint "$ENDPOINT" >"$output_dir/release-evidence-status.json"
  run cargo run -q -p jarvis-cli -- release signed-distribution-runbook --json --endpoint "$ENDPOINT" >"$output_dir/signed-distribution-runbook.json"
  run cargo run -q -p jarvis-cli -- release live-device-runbook --json --endpoint "$ENDPOINT" >"$output_dir/live-device-runbook.json"
  run cargo run -q -p jarvis-cli -- release plugin-trust-runbook --json --endpoint "$ENDPOINT" >"$output_dir/plugin-trust-runbook.json"
  write_readme "$output_dir"

  printf '\nJarvis external release handoff written: %s\n' "$output_dir"
  printf 'Proof boundary: handoff templates and read-only snapshots only; no external release validation was performed.\n'
}

self_test() {
  local tmp_dir
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-release-handoff.XXXXXX")"
  trap "rm -rf '$tmp_dir'" EXIT

  "$0" --write "$tmp_dir/handoff" >/dev/null

  require_file_contains "live-device template" "$tmp_dir/handoff/release-live-device-qa.env" "JARVIS_QA_CLEAN_PROFILE_VALIDATED=false"
  require_file_contains "plugin-trust template" "$tmp_dir/handoff/release-plugin-trust-qa.env" "JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED=false"
  require_file_contains "evidence-bundle template" "$tmp_dir/handoff/release-evidence-bundle.env" "JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=false"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "Proof boundary"
  require_json_key "readiness snapshot" "$tmp_dir/handoff/release-readiness.json" "production_ready"
  require_json_key "evidence-status snapshot" "$tmp_dir/handoff/release-evidence-status.json" "complete"
  require_json_key "signed-distribution runbook snapshot" "$tmp_dir/handoff/signed-distribution-runbook.json" "commands"
  require_json_key "live-device runbook snapshot" "$tmp_dir/handoff/live-device-runbook.json" "commands"
  require_json_key "plugin-trust runbook snapshot" "$tmp_dir/handoff/plugin-trust-runbook.json" "commands"

  printf 'Jarvis external release handoff self-test: ok\n'
  printf 'Proof boundary: temporary templates and read-only snapshots only; no external release validation was performed.\n'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      CHECK_ONLY=true
      shift
      ;;
    --write)
      WRITE=true
      [[ $# -ge 2 ]] || fail "--write requires an output directory"
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --self-test)
      SELF_TEST=true
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

mode_count=0
[[ "$CHECK_ONLY" == true ]] && mode_count=$((mode_count + 1))
[[ "$WRITE" == true ]] && mode_count=$((mode_count + 1))
[[ "$SELF_TEST" == true ]] && mode_count=$((mode_count + 1))
[[ "$mode_count" -eq 1 ]] || fail "choose exactly one mode: --check, --write DIR, or --self-test"

check_prerequisites

if [[ "$CHECK_ONLY" == true ]]; then
  print_check
elif [[ "$WRITE" == true ]]; then
  write_handoff "$OUTPUT_DIR"
else
  self_test
fi
