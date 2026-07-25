#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUTPUT_DIR="${ASSEMBLYWRIGHT_RELEASE_HANDOFF_DIR:-$ROOT_DIR/target/release-external-handoff}"
ENDPOINT="${ASSEMBLYWRIGHT_RELEASE_HANDOFF_ENDPOINT:-http://127.0.0.1:7787}"
CANONICAL_VERSION="$("$ROOT_DIR/scripts/release-version.sh")"
VERSION="${ASSEMBLYWRIGHT_RELEASE_HANDOFF_VERSION:-$CANONICAL_VERSION}"
CHECK_ONLY=false
WRITE=false
SELF_TEST=false

usage() {
  cat <<'USAGE'
Usage: scripts/release-external-handoff.sh [--check|--write DIR|--self-test]

Prepare a single release-operator handoff directory for the external Assemblywright
production evidence gates.

--check validates repo-owned handoff prerequisites and prints the external
evidence sequence. It does not write files.

--write DIR writes sourceable env templates and read-only JSON snapshots:
  release-live-device-qa.env
  release-evidence-bundle.env
  release-readiness.json
  release-evidence-status.json
  signed-distribution-runbook.json
  live-device-runbook.json
  release-evidence-checklist.md
  release-handoff-manifest.json
  README.md

--self-test writes the handoff into a temporary directory and verifies that the
templates, snapshots, checklist, and digest manifest are present with validation
flags still defaulted false.

Optional:
  ASSEMBLYWRIGHT_RELEASE_HANDOFF_DIR       Default output directory for --write
  ASSEMBLYWRIGHT_RELEASE_HANDOFF_ENDPOINT  Endpoint used by CLI read-only snapshots.
                                   Defaults to http://127.0.0.1:7787; the CLI
                                   falls back to local read-only metadata when
                                   the endpoint is unavailable.
  ASSEMBLYWRIGHT_RELEASE_HANDOFF_VERSION   Optional explicit version guard. If set, it
                                   must match scripts/release-version.sh.

Proof boundary: this script generates operator handoff files, read-only
snapshots, and a digest manifest only. It does not sign, notarize, staple, install, Finder-launch,
validate live-device behavior,
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

require_handoff_version_consistency() {
  if [[ -n "${ASSEMBLYWRIGHT_RELEASE_HANDOFF_VERSION:-}" && "$ASSEMBLYWRIGHT_RELEASE_HANDOFF_VERSION" != "$CANONICAL_VERSION" ]]; then
    fail "ASSEMBLYWRIGHT_RELEASE_HANDOFF_VERSION must match canonical release version $CANONICAL_VERSION"
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

require_json_string_contains() {
  local label="$1"
  local path="$2"
  local key="$3"
  local expected="$4"
  require_file "$label" "$path"
  python3 - "$path" "$key" "$expected" <<'PY'
import json
import sys

path, key, expected = sys.argv[1:4]
try:
    with open(path, encoding="utf-8") as handle:
        data = json.load(handle)
except Exception as exc:
    raise SystemExit(f"{path} is not valid JSON: {exc}")

value = data.get(key)
if not isinstance(value, str):
    raise SystemExit(f"{path} top-level key {key} is not a string")
if expected not in value:
    raise SystemExit(f"{path} top-level key {key} does not contain {expected!r}")
PY
}

require_manifest_integrity() {
  local label="$1"
  local output_dir="$2"
  local manifest_path="$output_dir/release-handoff-manifest.json"
  local expected_version
  local expected_commit
  expected_version="$("$ROOT_DIR/scripts/release-version.sh")"
  expected_commit="$(git rev-parse HEAD)"
  require_file "$label" "$manifest_path"
  python3 - "$manifest_path" "$output_dir" "$expected_version" "$expected_commit" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

manifest_path = Path(sys.argv[1])
output_dir = Path(sys.argv[2])
expected_version = sys.argv[3]
expected_commit = sys.argv[4]

with manifest_path.open(encoding="utf-8") as handle:
    manifest = json.load(handle)

expected_files = [
    "release-live-device-qa.env",
    "release-evidence-bundle.env",
    "release-readiness.json",
    "release-evidence-status.json",
    "signed-distribution-runbook.json",
    "live-device-runbook.json",
    "evidence-bundle-runbook.json",
    "release-evidence-checklist.md",
    "README.md",
]

if manifest.get("release_version") != expected_version:
    raise SystemExit(
        f"manifest release_version mismatch: expected {expected_version}, "
        f"got {manifest.get('release_version')!r}"
    )
if manifest.get("git_commit") != expected_commit:
    raise SystemExit(
        f"manifest git_commit mismatch: expected {expected_commit}, "
        f"got {manifest.get('git_commit')!r}"
    )

entries = manifest.get("files")
if not isinstance(entries, list):
    raise SystemExit("manifest files must be a list")

by_path = {entry.get("path"): entry for entry in entries if isinstance(entry, dict)}
if sorted(by_path) != sorted(expected_files):
    raise SystemExit(
        f"manifest files mismatch: expected {sorted(expected_files)}, "
        f"got {sorted(by_path)}"
    )

for name in expected_files:
    path = output_dir / name
    if not path.is_file():
        raise SystemExit(f"manifest references missing handoff file: {name}")
    data = path.read_bytes()
    entry = by_path[name]
    expected_sha = hashlib.sha256(data).hexdigest()
    if entry.get("sha256") != expected_sha:
        raise SystemExit(f"manifest sha256 mismatch for {name}")
    if entry.get("bytes") != len(data):
        raise SystemExit(f"manifest byte count mismatch for {name}")
PY
}

write_readme() {
  local output_dir="$1"
  local generated_at
  generated_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  cat >"$output_dir/README.md" <<EOF
# Assemblywright External Release Handoff

Generated at: $generated_at

This directory contains sourceable templates and read-only snapshots for the
external/manual evidence gates that remain before a production-ready Assemblywright
claim.

## Files

- \`release-live-device-qa.env\`: fill on the clean release Mac after installed
  app, Finder launch, restart, and manual release QA have actually passed.
- \`release-evidence-bundle.env\`: fill after signed/notarized distribution,
  live-device QA and report archival have actually completed.
- \`release-readiness.json\`, \`release-evidence-status.json\`, and the three
  \`*-runbook.json\` files: read-only snapshots from the current checkout.
- \`release-evidence-checklist.md\`: exact remaining evidence fields and artifact
  paths to fill before the final doctor assertion.
- \`release-handoff-manifest.json\`: generation metadata plus SHA-256 digests for
  every handoff file, so operators can archive and compare the package as a
  single bounded artifact.

## Ordered Release Sequence

1. Run the signed distribution lane with Developer ID and notarytool credentials:
   \`ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' ASSEMBLYWRIGHT_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh\`
   Or use Apple ID credentials instead of a stored profile:
   \`ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' ASSEMBLYWRIGHT_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' ASSEMBLYWRIGHT_NOTARYTOOL_APPLE_ID='apple-id@example.com' ASSEMBLYWRIGHT_NOTARYTOOL_TEAM_ID='TEAMID1234' ASSEMBLYWRIGHT_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh\`
2. Install the signed, notarized package into \`/Applications\` on a clean Mac
   profile and complete the live-device checks. Then run
   \`set -a && source release-live-device-qa.env && set +a\` followed by
   \`./scripts/release-live-device-qa.sh --assert-complete\`.
4. Archive the signed distribution, signed provenance, live-device QA report,
   and supporting external evidence in a durable
   release location. Then run
   \`set -a && source release-evidence-bundle.env && set +a\` followed by
   \`./scripts/release-evidence-bundle.sh --bundle\`.
5. Run \`./scripts/release-evidence-doctor.sh --assert-complete\`.
6. Start or restart the packaged app with
   \`ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external\` and the explicit
   \`ASSEMBLYWRIGHT_MAC_ENABLE_IPC_CLI_HANDOFF=true\` operator-mode opt-in, export
   \`ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT='<release-core-endpoint>'\` and
   \`ASSEMBLYWRIGHT_IPC_TOKEN_FILE='<app-owned-ipc-session-auth.json>'\`, then run
   \`ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release evidence-status --endpoint "\${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}"\`
   and \`ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release readiness --endpoint "\${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}"\`.

Proof boundary: these files are handoff scaffolding only. They do not prove that
signing, notarization, stapling, installation, Finder launch, live-device QA,
or final evidence archival have
been performed.
EOF
}

write_evidence_checklist() {
  local output_dir="$1"
  cat >"$output_dir/release-evidence-checklist.md" <<EOF
# Assemblywright External Evidence Checklist

This checklist is generated from the current checkout. It names the external
evidence that must exist before a production-ready claim. It is not evidence by
itself.

## Signed Distribution Evidence

- \`target/distribution/Assemblywright-$VERSION.zip\`: Developer ID signed and notarized
  app zip containing the signed app bundle; stapling is validated against the
  app bundle itself, not the zip container.
- \`target/distribution/Assemblywright-$VERSION.pkg\`: Developer ID Installer signed,
  notarized, stapled \`/Applications\` installer package.
- \`target/distribution/Assemblywright-$VERSION-signed-provenance.json\`: provenance report
  generated by \`./scripts/package-distribution.sh\`, including preserved
  notarytool log paths and SHA-256 digests.

## Live Device QA Evidence

Fill \`release-live-device-qa.env\` only after the clean release Mac has actually
validated install, Finder/LaunchServices launch, Developer Mode bridge status,
restart behavior, and manual release QA.

Required owner-recorded device evidence:

- \`ASSEMBLYWRIGHT_QA_OWNER_NAME\`, \`ASSEMBLYWRIGHT_QA_DEVICE_LABEL\`, and
  \`ASSEMBLYWRIGHT_QA_PROFILE_LABEL\`: who validated, on which device, in which profile.
- \`ASSEMBLYWRIGHT_QA_DEVICE_CHECK_STARTED_AT\` and
  \`ASSEMBLYWRIGHT_QA_DEVICE_CHECK_COMPLETED_AT\`: UTC timestamps ending in \`Z\`.
- Clean-profile, Finder launch, restart, and manual release QA evidence notes
  must contain real observations, not placeholders.

## Final Evidence Bundle

Fill \`release-evidence-bundle.env\` only after signed distribution,
live-device QA and durable archival are complete.

- \`ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_URI\` must point at the durable archive
  containing signed artifacts, signed provenance, QA reports, final bundle, and
  supporting external evidence.
- Keep \`ASSEMBLYWRIGHT_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=true\` for production bundle
  generation.
- Run \`./scripts/release-evidence-doctor.sh --assert-complete\` after bundle
  generation, then restart the core with
  \`ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external\` before the final readiness
  query.

Proof boundary: this checklist is operator guidance only; it does not sign,
notarize, staple, install, Finder-launch, validate live device behavior, review
or archive evidence.
EOF
}

write_manifest() {
  local output_dir="$1"
  local generated_at
  local git_commit
  generated_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  git_commit="$(git rev-parse HEAD)"

  python3 - "$output_dir" "$generated_at" "$VERSION" "$git_commit" "$ENDPOINT" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

output_dir = Path(sys.argv[1])
generated_at, version, git_commit, endpoint = sys.argv[2:6]
files = [
    "release-live-device-qa.env",
    "release-evidence-bundle.env",
    "release-readiness.json",
    "release-evidence-status.json",
    "signed-distribution-runbook.json",
    "live-device-runbook.json",
    "evidence-bundle-runbook.json",
    "release-evidence-checklist.md",
    "README.md",
]

entries = []
for name in files:
    data = (output_dir / name).read_bytes()
    entries.append(
        {
            "path": name,
            "sha256": hashlib.sha256(data).hexdigest(),
            "bytes": len(data),
        }
    )

manifest = {
    "schema_version": 1,
    "evidence_type": "release_external_handoff_manifest",
    "generated_at": generated_at,
    "release_version": version,
    "git_commit": git_commit,
    "snapshot_endpoint": endpoint,
    "proof_boundary": (
        "Handoff manifest and per-file digests only; this does not prove signing, "
        "notarization, stapling, installation, Finder launch, live-device QA, "
        "or final evidence archival."
    ),
    "files": entries,
}

(output_dir / "release-handoff-manifest.json").write_text(
    json.dumps(manifest, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

check_prerequisites() {
  require_command cargo
  require_command python3
  require_command date
  [[ -x ./scripts/release-live-device-qa.sh ]] || fail "missing executable scripts/release-live-device-qa.sh"
  [[ -x ./scripts/release-evidence-bundle.sh ]] || fail "missing executable scripts/release-evidence-bundle.sh"
  [[ -x ./scripts/package-distribution.sh ]] || fail "missing executable scripts/package-distribution.sh"
}

print_check() {
  cat <<'CHECK'
Assemblywright external release handoff preflight: ok

This repo can prepare the operator handoff files, but production readiness still
requires external evidence:
- Developer ID signing, notarization, and stapling for app zip and installer.
- Clean-profile /Applications install and Finder/LaunchServices launch.
- Installed-app launch, Developer Mode bridge status, restart, and manual
  release QA on a real Mac.
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
  run ./scripts/release-evidence-bundle.sh --write-template "$output_dir/release-evidence-bundle.env"

  run env ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -q -p assemblywright-cli -- release readiness --json --endpoint "$ENDPOINT" >"$output_dir/release-readiness.json"
  run cargo run -q -p assemblywright-cli -- release evidence-status --json --endpoint "$ENDPOINT" >"$output_dir/release-evidence-status.json"
  run cargo run -q -p assemblywright-cli -- release signed-distribution-runbook --json --endpoint "$ENDPOINT" >"$output_dir/signed-distribution-runbook.json"
  run cargo run -q -p assemblywright-cli -- release live-device-runbook --json --endpoint "$ENDPOINT" >"$output_dir/live-device-runbook.json"
  run cargo run -q -p assemblywright-cli -- release evidence-bundle-runbook --json --endpoint "$ENDPOINT" >"$output_dir/evidence-bundle-runbook.json"
  write_evidence_checklist "$output_dir"
  write_readme "$output_dir"
  write_manifest "$output_dir"

  printf '\nAssemblywright external release handoff written: %s\n' "$output_dir"
  printf 'Proof boundary: handoff templates, read-only snapshots, and digest manifest only; no external release validation was performed.\n'
}

self_test() {
  local tmp_dir
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/assemblywright-release-handoff.XXXXXX")"
  trap "rm -rf '$tmp_dir'" EXIT

  "$0" --write "$tmp_dir/handoff" >/dev/null

  require_file_contains "live-device template" "$tmp_dir/handoff/release-live-device-qa.env" "ASSEMBLYWRIGHT_QA_CLEAN_PROFILE_VALIDATED=false"
  require_file_contains "live-device template" "$tmp_dir/handoff/release-live-device-qa.env" "ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT"
  require_file_contains "live-device template" "$tmp_dir/handoff/release-live-device-qa.env" 'ASSEMBLYWRIGHT_QA_DEVICE_CHECK_STARTED_AT=""'
  require_file_contains "live-device template" "$tmp_dir/handoff/release-live-device-qa.env" "release evidence-status"
  require_file_contains "live-device template" "$tmp_dir/handoff/release-live-device-qa.env" "release readiness"
  require_file_contains "evidence-bundle template" "$tmp_dir/handoff/release-evidence-bundle.env" "ASSEMBLYWRIGHT_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=false"
  require_file_contains "evidence-bundle template" "$tmp_dir/handoff/release-evidence-bundle.env" "ASSEMBLYWRIGHT_EVIDENCE_OVERWRITE_OUTPUT=false"
  require_file_contains "evidence-bundle template" "$tmp_dir/handoff/release-evidence-bundle.env" 'ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_URI=""'
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "ASSEMBLYWRIGHT_DEVELOPER_ID_APPLICATION"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "set -a && source release-live-device-qa.env && set +a"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "./scripts/release-live-device-qa.sh --assert-complete"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "set -a && source release-evidence-bundle.env && set +a"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "./scripts/release-evidence-bundle.sh --bundle"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "./scripts/release-evidence-doctor.sh --assert-complete"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT='<release-core-endpoint>'"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" 'ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release evidence-status --endpoint "${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}"'
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" 'ASSEMBLYWRIGHT_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p assemblywright-cli -- release readiness --endpoint "${ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT:?set ASSEMBLYWRIGHT_RELEASE_CORE_ENDPOINT}"'
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "Proof boundary"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "release-evidence-checklist.md"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "release-handoff-manifest.json"
  require_file_contains "handoff checklist" "$tmp_dir/handoff/release-evidence-checklist.md" "ASSEMBLYWRIGHT_QA_DEVICE_CHECK_STARTED_AT"
  require_file_contains "handoff checklist" "$tmp_dir/handoff/release-evidence-checklist.md" "Developer Mode bridge status"
  require_file_contains "handoff checklist" "$tmp_dir/handoff/release-evidence-checklist.md" "ASSEMBLYWRIGHT_EVIDENCE_REPORTS_ARCHIVE_URI"
  require_json_key "handoff manifest" "$tmp_dir/handoff/release-handoff-manifest.json" "files"
  require_json_string_contains "handoff manifest" "$tmp_dir/handoff/release-handoff-manifest.json" "evidence_type" "release_external_handoff_manifest"
  require_manifest_integrity "handoff manifest" "$tmp_dir/handoff"
  require_json_key "readiness snapshot" "$tmp_dir/handoff/release-readiness.json" "production_ready"
  require_json_string_contains "readiness snapshot" "$tmp_dir/handoff/release-readiness.json" "readiness_scope" "external release evidence status"
  require_json_key "evidence-status snapshot" "$tmp_dir/handoff/release-evidence-status.json" "complete"
  require_json_key "signed-distribution runbook snapshot" "$tmp_dir/handoff/signed-distribution-runbook.json" "commands"
  require_json_key "live-device runbook snapshot" "$tmp_dir/handoff/live-device-runbook.json" "commands"
  require_json_key "evidence-bundle runbook snapshot" "$tmp_dir/handoff/evidence-bundle-runbook.json" "commands"
  require_json_string_contains "evidence-bundle runbook snapshot" "$tmp_dir/handoff/evidence-bundle-runbook.json" "proof_boundary" "does not generate the final bundle"

  local mismatch_log="$tmp_dir/version-mismatch.log"
  if ASSEMBLYWRIGHT_RELEASE_HANDOFF_VERSION="0.0.0-test" "$0" --write "$tmp_dir/version-mismatch" >"$mismatch_log" 2>&1; then
    fail "handoff self-test expected version mismatch to fail"
  fi
  require_file_contains "version mismatch output" "$mismatch_log" "ASSEMBLYWRIGHT_RELEASE_HANDOFF_VERSION must match canonical release version"

  printf 'Assemblywright external release handoff self-test: ok\n'
  printf 'Proof boundary: temporary templates, checklist, read-only snapshots, and digest manifest only; no external release validation was performed.\n'
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
require_handoff_version_consistency

if [[ "$CHECK_ONLY" == true ]]; then
  print_check
elif [[ "$WRITE" == true ]]; then
  write_handoff "$OUTPUT_DIR"
else
  self_test
fi
