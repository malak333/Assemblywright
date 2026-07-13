#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUTPUT_DIR="${JARVIS_RELEASE_HANDOFF_DIR:-$ROOT_DIR/target/release-external-handoff}"
ENDPOINT="${JARVIS_RELEASE_HANDOFF_ENDPOINT:-http://127.0.0.1:7787}"
CANONICAL_VERSION="$("$ROOT_DIR/scripts/release-version.sh")"
VERSION="${JARVIS_RELEASE_HANDOFF_VERSION:-$CANONICAL_VERSION}"
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
  release-evidence-checklist.md
  release-handoff-manifest.json
  README.md

--self-test writes the handoff into a temporary directory and verifies that the
templates, snapshots, checklist, and digest manifest are present with validation
flags still defaulted false.

Optional:
  JARVIS_RELEASE_HANDOFF_DIR       Default output directory for --write
  JARVIS_RELEASE_HANDOFF_ENDPOINT  Endpoint used by CLI read-only snapshots.
                                   Defaults to http://127.0.0.1:7787; the CLI
                                   falls back to local read-only metadata when
                                   the endpoint is unavailable.
  JARVIS_RELEASE_HANDOFF_VERSION   Optional explicit version guard. If set, it
                                   must match scripts/release-version.sh.

Proof boundary: this script generates operator handoff files, read-only
snapshots, and a digest manifest only. It does not sign, notarize, staple, install, Finder-launch,
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

require_handoff_version_consistency() {
  if [[ -n "${JARVIS_RELEASE_HANDOFF_VERSION:-}" && "$JARVIS_RELEASE_HANDOFF_VERSION" != "$CANONICAL_VERSION" ]]; then
    fail "JARVIS_RELEASE_HANDOFF_VERSION must match canonical release version $CANONICAL_VERSION"
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
    "release-plugin-trust-qa.env",
    "release-evidence-bundle.env",
    "release-readiness.json",
    "release-evidence-status.json",
    "signed-distribution-runbook.json",
    "live-device-runbook.json",
    "plugin-trust-runbook.json",
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
- \`release-evidence-checklist.md\`: exact remaining evidence fields and artifact
  paths to fill before the final doctor assertion.
- \`release-handoff-manifest.json\`: generation metadata plus SHA-256 digests for
  every handoff file, so operators can archive and compare the package as a
  single bounded artifact.

## Ordered Release Sequence

1. Run the signed distribution lane with Developer ID and notarytool credentials:
   \`JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh\`
   Or use Apple ID credentials instead of a stored profile:
   \`JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_APPLE_ID='apple-id@example.com' JARVIS_NOTARYTOOL_TEAM_ID='TEAMID1234' JARVIS_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh\`
2. Install the signed, notarized package into \`/Applications\` on a clean Mac
   profile and complete the live-device checks. Then run
   \`set -a && source release-live-device-qa.env && set +a\` followed by
   \`./scripts/release-live-device-qa.sh --assert-complete\`.
3. Complete the plugin trust review checks. Then run
   \`set -a && source release-plugin-trust-qa.env && set +a\` followed by
   \`./scripts/release-plugin-trust-qa.sh --assert-complete\`.
4. Archive the signed distribution, signed provenance, live-device QA report,
   plugin-trust QA report, and supporting external evidence in a durable
   release location. Then run
   \`set -a && source release-evidence-bundle.env && set +a\` followed by
   \`./scripts/release-evidence-bundle.sh --bundle\`.
5. Run \`./scripts/release-evidence-doctor.sh --assert-complete\`.
6. Start or restart the release core with
   \`JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external\`, export
   \`JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>'\` and
   \`JARVIS_IPC_TOKEN_FILE='<app-owned-ipc-session-auth.json>'\`, then run
   \`JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint "\${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}"\`
   and \`JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint "\${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}"\`.

Proof boundary: these files are handoff scaffolding only. They do not prove that
signing, notarization, stapling, installation, Finder launch, live-device QA,
plugin trust QA, host-level egress enforcement, or final evidence archival have
been performed.
EOF
}

write_evidence_checklist() {
  local output_dir="$1"
  cat >"$output_dir/release-evidence-checklist.md" <<EOF
# Jarvis External Evidence Checklist

This checklist is generated from the current checkout. It names the external
evidence that must exist before a production-ready claim. It is not evidence by
itself.

## Signed Distribution Evidence

- \`target/distribution/Jarvis-$VERSION.zip\`: Developer ID signed and notarized
  app zip containing the signed app bundle; stapling is validated against the
  app bundle itself, not the zip container.
- \`target/distribution/Jarvis-$VERSION.pkg\`: Developer ID Installer signed,
  notarized, stapled \`/Applications\` installer package.
- \`target/distribution/Jarvis-$VERSION-signed-provenance.json\`: provenance report
  generated by \`./scripts/package-distribution.sh\`, including preserved
  notarytool log paths and SHA-256 digests.

## Live Device QA Evidence

Fill \`release-live-device-qa.env\` only after the clean release Mac has actually
validated install, Finder/LaunchServices launch, microphone permission, Speech
permission, spoken transcript handoff, live audio output, scheduler
notification delivery, restart behavior, and manual release QA.

Required command/evidence binding:

- \`JARVIS_RELEASE_CORE_ENDPOINT\`: the same release core endpoint used for command
  capture and post-report evidence-status/readiness checks.
- \`JARVIS_IPC_TOKEN_FILE\`: the app-owned owner-only IPC credential-file path;
  export the path only and never copy the bearer value into handoff evidence.
- \`JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID\`: \`task:<uuid>\` or \`audit:<uuid>\`
  returned by the live command evidence capture.

Required scheduler notification observation:

- \`JARVIS_QA_NOTIFICATION_KIND\`: one of \`due_now\`, \`failed\`, or
  \`blocked_by_emergency_pause\`.
- \`JARVIS_QA_NOTIFICATION_TITLE\`: observed notification title.
- \`JARVIS_QA_NOTIFICATION_BODY\`: observed notification body.
- \`JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER\`: \`jarvis.scheduler\`.
- \`JARVIS_QA_NOTIFICATION_OBSERVED_AT\`: UTC timestamp ending in \`Z\`.

## Plugin Trust QA Evidence

Fill \`release-plugin-trust-qa.env\` only after marketplace review, malware
scan, signed publisher policy review, OS sandbox validation, host-level egress
deny/allow fixtures, and manual trust review have actually passed.

Each category must have a durable artifact URI plus SHA-256 digest:

- \`JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_URI\` and
  \`JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_SHA256\`
- \`JARVIS_PLUGIN_QA_MALWARE_SCAN_ARTIFACT_URI\` and
  \`JARVIS_PLUGIN_QA_MALWARE_SCAN_ARTIFACT_SHA256\`
- \`JARVIS_PLUGIN_QA_OS_SANDBOX_ARTIFACT_URI\` and
  \`JARVIS_PLUGIN_QA_OS_SANDBOX_ARTIFACT_SHA256\`
- \`JARVIS_PLUGIN_QA_EGRESS_ARTIFACT_URI\` and
  \`JARVIS_PLUGIN_QA_EGRESS_ARTIFACT_SHA256\`
- \`JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_ARTIFACT_URI\` and
  \`JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_ARTIFACT_SHA256\`
- \`JARVIS_PLUGIN_QA_MANUAL_REVIEW_ARTIFACT_URI\` and
  \`JARVIS_PLUGIN_QA_MANUAL_REVIEW_ARTIFACT_SHA256\`

## Final Evidence Bundle

Fill \`release-evidence-bundle.env\` only after signed distribution,
live-device QA, plugin-trust QA, and durable archival are complete.

- \`JARVIS_EVIDENCE_REPORTS_ARCHIVE_URI\` must point at the durable archive
  containing signed artifacts, signed provenance, QA reports, final bundle, and
  supporting external evidence.
- Keep \`JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES=true\` for production bundle
  generation.
- Run \`./scripts/release-evidence-doctor.sh --assert-complete\` after bundle
  generation, then restart the core with
  \`JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external\` before the final readiness
  query.

Proof boundary: this checklist is operator guidance only; it does not sign,
notarize, staple, install, Finder-launch, validate live device behavior, review
plugin trust, enforce egress, or archive evidence.
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
    "release-plugin-trust-qa.env",
    "release-evidence-bundle.env",
    "release-readiness.json",
    "release-evidence-status.json",
    "signed-distribution-runbook.json",
    "live-device-runbook.json",
    "plugin-trust-runbook.json",
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
        "plugin trust QA, host-level egress enforcement, or final evidence archival."
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
  structured scheduler notification observation, restart, and manual release QA
  on a real Mac.
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

  run env JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -q -p jarvis-cli -- release readiness --json --endpoint "$ENDPOINT" >"$output_dir/release-readiness.json"
  run cargo run -q -p jarvis-cli -- release evidence-status --json --endpoint "$ENDPOINT" >"$output_dir/release-evidence-status.json"
  run cargo run -q -p jarvis-cli -- release signed-distribution-runbook --json --endpoint "$ENDPOINT" >"$output_dir/signed-distribution-runbook.json"
  run cargo run -q -p jarvis-cli -- release live-device-runbook --json --endpoint "$ENDPOINT" >"$output_dir/live-device-runbook.json"
  run cargo run -q -p jarvis-cli -- release plugin-trust-runbook --json --endpoint "$ENDPOINT" >"$output_dir/plugin-trust-runbook.json"
  run cargo run -q -p jarvis-cli -- release evidence-bundle-runbook --json --endpoint "$ENDPOINT" >"$output_dir/evidence-bundle-runbook.json"
  write_evidence_checklist "$output_dir"
  write_readme "$output_dir"
  write_manifest "$output_dir"

  printf '\nJarvis external release handoff written: %s\n' "$output_dir"
  printf 'Proof boundary: handoff templates, read-only snapshots, and digest manifest only; no external release validation was performed.\n'
}

self_test() {
  local tmp_dir
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-release-handoff.XXXXXX")"
  trap "rm -rf '$tmp_dir'" EXIT

  "$0" --write "$tmp_dir/handoff" >/dev/null

  require_file_contains "live-device template" "$tmp_dir/handoff/release-live-device-qa.env" "JARVIS_QA_CLEAN_PROFILE_VALIDATED=false"
  require_file_contains "live-device template" "$tmp_dir/handoff/release-live-device-qa.env" "JARVIS_RELEASE_CORE_ENDPOINT"
  require_file_contains "live-device template" "$tmp_dir/handoff/release-live-device-qa.env" 'JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID=""'
  require_file_contains "live-device template" "$tmp_dir/handoff/release-live-device-qa.env" "release evidence-status"
  require_file_contains "live-device template" "$tmp_dir/handoff/release-live-device-qa.env" "release readiness"
  require_file_contains "plugin-trust template" "$tmp_dir/handoff/release-plugin-trust-qa.env" "JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED=false"
  require_file_contains "plugin-trust template" "$tmp_dir/handoff/release-plugin-trust-qa.env" 'JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_URI=""'
  require_file_contains "plugin-trust template" "$tmp_dir/handoff/release-plugin-trust-qa.env" 'JARVIS_PLUGIN_QA_MALWARE_SCAN_ARTIFACT_SHA256=""'
  require_file_contains "plugin-trust template" "$tmp_dir/handoff/release-plugin-trust-qa.env" 'JARVIS_PLUGIN_QA_MANUAL_REVIEW_ARTIFACT_URI=""'
  require_file_contains "evidence-bundle template" "$tmp_dir/handoff/release-evidence-bundle.env" "JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=false"
  require_file_contains "evidence-bundle template" "$tmp_dir/handoff/release-evidence-bundle.env" "JARVIS_EVIDENCE_OVERWRITE_OUTPUT=false"
  require_file_contains "evidence-bundle template" "$tmp_dir/handoff/release-evidence-bundle.env" 'JARVIS_EVIDENCE_REPORTS_ARCHIVE_URI=""'
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "JARVIS_DEVELOPER_ID_APPLICATION"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "set -a && source release-live-device-qa.env && set +a"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "./scripts/release-live-device-qa.sh --assert-complete"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "set -a && source release-plugin-trust-qa.env && set +a"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "./scripts/release-plugin-trust-qa.sh --assert-complete"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "set -a && source release-evidence-bundle.env && set +a"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "./scripts/release-evidence-bundle.sh --bundle"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "./scripts/release-evidence-doctor.sh --assert-complete"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>'"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" 'JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint "${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}"'
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" 'JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint "${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}"'
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "Proof boundary"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "release-evidence-checklist.md"
  require_file_contains "handoff readme" "$tmp_dir/handoff/README.md" "release-handoff-manifest.json"
  require_file_contains "handoff checklist" "$tmp_dir/handoff/release-evidence-checklist.md" "JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER"
  require_file_contains "handoff checklist" "$tmp_dir/handoff/release-evidence-checklist.md" "jarvis.scheduler"
  require_file_contains "handoff checklist" "$tmp_dir/handoff/release-evidence-checklist.md" "JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_URI"
  require_file_contains "handoff checklist" "$tmp_dir/handoff/release-evidence-checklist.md" "JARVIS_EVIDENCE_REPORTS_ARCHIVE_URI"
  require_json_key "handoff manifest" "$tmp_dir/handoff/release-handoff-manifest.json" "files"
  require_json_string_contains "handoff manifest" "$tmp_dir/handoff/release-handoff-manifest.json" "evidence_type" "release_external_handoff_manifest"
  require_manifest_integrity "handoff manifest" "$tmp_dir/handoff"
  require_json_key "readiness snapshot" "$tmp_dir/handoff/release-readiness.json" "production_ready"
  require_json_string_contains "readiness snapshot" "$tmp_dir/handoff/release-readiness.json" "readiness_scope" "external release evidence status"
  require_json_key "evidence-status snapshot" "$tmp_dir/handoff/release-evidence-status.json" "complete"
  require_json_key "signed-distribution runbook snapshot" "$tmp_dir/handoff/signed-distribution-runbook.json" "commands"
  require_json_key "live-device runbook snapshot" "$tmp_dir/handoff/live-device-runbook.json" "commands"
  require_json_key "plugin-trust runbook snapshot" "$tmp_dir/handoff/plugin-trust-runbook.json" "commands"
  require_json_key "evidence-bundle runbook snapshot" "$tmp_dir/handoff/evidence-bundle-runbook.json" "commands"
  require_json_string_contains "evidence-bundle runbook snapshot" "$tmp_dir/handoff/evidence-bundle-runbook.json" "proof_boundary" "does not generate the final bundle"

  local mismatch_log="$tmp_dir/version-mismatch.log"
  if JARVIS_RELEASE_HANDOFF_VERSION="0.0.0-test" "$0" --write "$tmp_dir/version-mismatch" >"$mismatch_log" 2>&1; then
    fail "handoff self-test expected version mismatch to fail"
  fi
  require_file_contains "version mismatch output" "$mismatch_log" "JARVIS_RELEASE_HANDOFF_VERSION must match canonical release version"

  printf 'Jarvis external release handoff self-test: ok\n'
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
