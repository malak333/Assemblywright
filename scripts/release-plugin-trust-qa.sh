#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPORT_PATH="${JARVIS_PLUGIN_QA_REPORT_PATH:-$ROOT_DIR/target/release-plugin-trust-qa-report.json}"
CHECK_ONLY=false
ASSERT_COMPLETE=false
SELF_TEST=false

usage() {
  cat <<'USAGE'
Usage: scripts/release-plugin-trust-qa.sh [--check|--assert-complete|--self-test]

Prepare or assert the installed-plugin trust release QA gate for Jarvis.

--check validates repo-owned plugin trust prerequisites and prints the manual
marketplace, malware-analysis, OS sandbox, and egress checks required before
any marketplace or third-party plugin safety claim.

--assert-complete verifies that the owner has explicitly recorded each plugin
trust validation flag below as true, has recorded evidence-note fields, and
writes a JSON evidence report:
  JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED=true
  JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED=true
  JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED=true
  JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED=true
  JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED=true
  JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED=true
  JARVIS_PLUGIN_QA_OWNER_NAME
  JARVIS_PLUGIN_QA_REVIEW_STARTED_AT
  JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT
  JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE
  JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE
  JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE
  JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE
  JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE
  JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE

--self-test exercises the assertion/report mechanics with fake validation flags
without claiming real marketplace, malware-analysis, sandbox, or egress proof.

Optional:
  JARVIS_PLUGIN_QA_REPORT_PATH     Defaults to target/release-plugin-trust-qa-report.json
  JARVIS_PLUGIN_QA_REVIEW_SOURCE   Defaults to owner-asserted-manual-review

This script records manual proof boundaries only. It does not operate a plugin
marketplace, run a malware scanner, install a macOS sandbox profile, or enforce
host-level network egress restrictions.
USAGE
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
}

require_file_contains() {
  local label="$1"
  local path="$2"
  local expected="$3"
  [[ -f "$path" ]] || fail "missing $label: $path"
  if ! grep -F "$expected" "$path" >/dev/null 2>&1; then
    fail "$label does not mention required text: $expected"
  fi
}

require_true() {
  local name="$1"
  local value="${!name:-}"
  [[ "$value" == "true" ]] || fail "$name must be set to true after manual validation"
}

require_non_empty_env() {
  local name="$1"
  local value="${!name:-}"
  [[ -n "${value//[[:space:]]/}" ]] || fail "$name must be set after manual validation"
}

require_utc_env_timestamp() {
  local name="$1"
  local value="${!name:-}"
  require_non_empty_env "$name"
  python3 - "$name" "$value" <<'PY'
from datetime import datetime
import sys

name, value = sys.argv[1:3]
if not value.endswith("Z"):
    raise SystemExit(f"{name} must be a UTC RFC3339 timestamp ending in Z")
try:
    datetime.fromisoformat(value.replace("Z", "+00:00"))
except ValueError as exc:
    raise SystemExit(f"{name} must be a UTC RFC3339 timestamp") from exc
PY
}

require_utc_env_timestamp_order() {
  local start_name="$1"
  local completed_name="$2"
  local started="${!start_name:-}"
  local completed="${!completed_name:-}"
  require_utc_env_timestamp "$start_name"
  require_utc_env_timestamp "$completed_name"
  python3 - "$start_name" "$started" "$completed_name" "$completed" <<'PY'
from datetime import datetime
import sys

start_name, started, completed_name, completed = sys.argv[1:5]
started_at = datetime.fromisoformat(started.replace("Z", "+00:00"))
completed_at = datetime.fromisoformat(completed.replace("Z", "+00:00"))
if completed_at < started_at:
    raise SystemExit(f"{completed_name} must be greater than or equal to {start_name}")
PY
}

require_not_future_utc_env_timestamp() {
  local name="$1"
  local value="${!name:-}"
  require_utc_env_timestamp "$name"
  python3 - "$name" "$value" <<'PY'
from datetime import datetime, timezone
import sys

name, value = sys.argv[1:3]
timestamp = datetime.fromisoformat(value.replace("Z", "+00:00"))
now = datetime.now(timezone.utc)
if timestamp > now:
    raise SystemExit(f"{name} must not be later than the generated report timestamp")
PY
}

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

write_report() {
  local generated_at
  local escaped_boundary
  local escaped_source
  local escaped_owner
  local escaped_started
  local escaped_completed
  local escaped_marketplace_note
  local escaped_malware_note
  local escaped_sandbox_note
  local escaped_egress_note
  local escaped_signed_publisher_note
  local escaped_manual_note
  require_command python3
  generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  escaped_boundary="$(json_escape "Owner-recorded marketplace review, malware scan, OS sandbox, egress enforcement, signed publisher policy, and manual plugin trust evidence only; no repo-local command can prove an external marketplace, malware scanner, or host-level sandbox deployment.")"
  escaped_source="$(json_escape "${JARVIS_PLUGIN_QA_REVIEW_SOURCE:-owner-asserted-manual-review}")"
  escaped_owner="$(json_escape "$JARVIS_PLUGIN_QA_OWNER_NAME")"
  escaped_started="$(json_escape "$JARVIS_PLUGIN_QA_REVIEW_STARTED_AT")"
  escaped_completed="$(json_escape "$JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT")"
  escaped_marketplace_note="$(json_escape "$JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE")"
  escaped_malware_note="$(json_escape "$JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE")"
  escaped_sandbox_note="$(json_escape "$JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE")"
  escaped_egress_note="$(json_escape "$JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE")"
  escaped_signed_publisher_note="$(json_escape "$JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE")"
  escaped_manual_note="$(json_escape "$JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE")"

  mkdir -p "$(dirname "$REPORT_PATH")"
  cat >"$REPORT_PATH" <<EOF
{
  "generated_at": "$generated_at",
  "review_source": "$escaped_source",
  "validation_flags": {
    "marketplace_review": true,
    "malware_scan": true,
    "os_sandbox": true,
    "egress_enforcement": true,
    "signed_publisher_policy": true,
    "manual_trust_review": true
  },
  "owner_recorded_plugin_trust_evidence": {
    "owner_name": "$escaped_owner",
    "review_started_at": "$escaped_started",
    "review_completed_at": "$escaped_completed",
    "marketplace_evidence_note": "$escaped_marketplace_note",
    "malware_scan_evidence_note": "$escaped_malware_note",
    "os_sandbox_evidence_note": "$escaped_sandbox_note",
    "egress_evidence_note": "$escaped_egress_note",
    "signed_publisher_evidence_note": "$escaped_signed_publisher_note",
    "manual_review_evidence_note": "$escaped_manual_note"
  },
  "proof_boundary": "$escaped_boundary"
}
EOF
  python3 -m json.tool "$REPORT_PATH" >/dev/null
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      CHECK_ONLY=true
      shift
      ;;
    --assert-complete)
      ASSERT_COMPLETE=true
      shift
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

if { [[ "$CHECK_ONLY" == true ]] && { [[ "$ASSERT_COMPLETE" == true ]] || [[ "$SELF_TEST" == true ]]; }; } ||
  { [[ "$ASSERT_COMPLETE" == true ]] && [[ "$SELF_TEST" == true ]]; }; then
  fail "--check, --assert-complete, and --self-test are mutually exclusive"
fi

if [[ "$CHECK_ONLY" != true && "$ASSERT_COMPLETE" != true && "$SELF_TEST" != true ]]; then
  usage
  exit 0
fi

require_command grep
require_command python3

require_file_contains "plugin contract" "$ROOT_DIR/docs/plugin-contract.md" "WASM, OS-level network sandboxing, and malware-analysis trust remain target"
require_file_contains "plugin contract" "$ROOT_DIR/docs/plugin-contract.md" "marketplace approval, malware safety, OS-level process/network sandboxing"
require_file_contains "release checklist" "$ROOT_DIR/docs/release-checklist.md" "marketplace plugin review, malware"
require_file_contains "release checklist" "$ROOT_DIR/docs/release-checklist.md" "analysis, or OS sandbox"
require_file_contains "readme" "$ROOT_DIR/README.md" "marketplace plugin trust, OS-level plugin network sandboxing"
require_file_contains "plugin runtime" "$ROOT_DIR/crates/jarvis-core/src/plugin.rs" "network_access declared_hosts requires allowed_hosts"
require_file_contains "cross-process E2E" "$ROOT_DIR/crates/jarvis-cli/tests/local_ipc_e2e.rs" "network_subprocess_e2e"

if [[ "$SELF_TEST" == true ]]; then
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/jarvis-plugin-trust-qa-self-test.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT
  fixture_report="$tmp_dir/release-plugin-trust-qa-report.json"

  JARVIS_PLUGIN_QA_REPORT_PATH="$fixture_report" \
    JARVIS_PLUGIN_QA_REVIEW_SOURCE="self-test-fixture" \
    JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED=true \
    JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED=true \
    JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED=true \
    JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED=true \
    JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED=true \
    JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED=true \
    JARVIS_PLUGIN_QA_OWNER_NAME="Jarvis Plugin QA Self-Test" \
    JARVIS_PLUGIN_QA_REVIEW_STARTED_AT="2026-05-22T16:10:00Z" \
    JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT="2026-05-22T16:20:00Z" \
    JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE="Marketplace review fixture was observed." \
    JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE="Malware scan fixture was observed." \
    JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE="OS sandbox fixture was observed." \
    JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE="Egress fixture was observed." \
    JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE="Signed publisher policy fixture was observed." \
    JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE="Manual trust review fixture was observed." \
    "$0" --assert-complete >/dev/null
  require_file_contains "plugin trust QA self-test report" "$fixture_report" '"marketplace_review": true'
  require_file_contains "plugin trust QA self-test report" "$fixture_report" '"review_source": "self-test-fixture"'
  require_file_contains "plugin trust QA self-test report" "$fixture_report" '"owner_recorded_plugin_trust_evidence"'
  require_file_contains "plugin trust QA self-test report" "$fixture_report" '"egress_evidence_note": "Egress fixture was observed."'
  require_file_contains "plugin trust QA self-test report" "$fixture_report" '"proof_boundary"'

  if JARVIS_PLUGIN_QA_REPORT_PATH="$tmp_dir/blank-evidence-report.json" \
    JARVIS_PLUGIN_QA_REVIEW_SOURCE="self-test-fixture" \
    JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED=true \
    JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED=true \
    JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED=true \
    JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED=true \
    JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED=true \
    JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED=true \
    JARVIS_PLUGIN_QA_OWNER_NAME="Jarvis Plugin QA Self-Test" \
    JARVIS_PLUGIN_QA_REVIEW_STARTED_AT="2026-05-22T16:10:00Z" \
    JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT="2026-05-22T16:20:00Z" \
    JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE="Marketplace review fixture was observed." \
    JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE="Malware scan fixture was observed." \
    JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE="OS sandbox fixture was observed." \
    JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE="" \
    JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE="Signed publisher policy fixture was observed." \
    JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE="Manual trust review fixture was observed." \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "plugin trust QA self-test expected blank egress evidence to be rejected"
  fi
  if JARVIS_PLUGIN_QA_REPORT_PATH="$tmp_dir/non-utc-timestamp-report.json" \
    JARVIS_PLUGIN_QA_REVIEW_SOURCE="self-test-fixture" \
    JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED=true \
    JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED=true \
    JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED=true \
    JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED=true \
    JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED=true \
    JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED=true \
    JARVIS_PLUGIN_QA_OWNER_NAME="Jarvis Plugin QA Self-Test" \
    JARVIS_PLUGIN_QA_REVIEW_STARTED_AT="2026-05-22T16:10:00-04:00" \
    JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT="2026-05-22T16:20:00Z" \
    JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE="Marketplace review fixture was observed." \
    JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE="Malware scan fixture was observed." \
    JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE="OS sandbox fixture was observed." \
    JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE="Egress fixture was observed." \
    JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE="Signed publisher policy fixture was observed." \
    JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE="Manual trust review fixture was observed." \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "plugin trust QA self-test expected non-UTC review timestamp to be rejected"
  fi
  if JARVIS_PLUGIN_QA_REPORT_PATH="$tmp_dir/reversed-timestamp-report.json" \
    JARVIS_PLUGIN_QA_REVIEW_SOURCE="self-test-fixture" \
    JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED=true \
    JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED=true \
    JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED=true \
    JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED=true \
    JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED=true \
    JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED=true \
    JARVIS_PLUGIN_QA_OWNER_NAME="Jarvis Plugin QA Self-Test" \
    JARVIS_PLUGIN_QA_REVIEW_STARTED_AT="2026-05-22T16:20:00Z" \
    JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT="2026-05-22T16:10:00Z" \
    JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE="Marketplace review fixture was observed." \
    JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE="Malware scan fixture was observed." \
    JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE="OS sandbox fixture was observed." \
    JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE="Egress fixture was observed." \
    JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE="Signed publisher policy fixture was observed." \
    JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE="Manual trust review fixture was observed." \
    "$0" --assert-complete >/dev/null 2>&1; then
    fail "plugin trust QA self-test expected reversed review timestamps to be rejected"
  fi
  printf 'Jarvis plugin trust QA self-test: ok\n'
  printf 'Proof boundary: fake flags and evidence notes validate assertion/report mechanics only; no marketplace, malware, sandbox, or egress validation was performed.\n'
  exit 0
fi

if [[ "$CHECK_ONLY" == true ]]; then
  cat <<'CHECKLIST'
Jarvis plugin trust QA preflight: ok

Repo-owned plugin trust checks already covered by the local release gate:
- Installed plugin manifests validate local metadata and provenance snapshots.
- Installed plugin execution remains disabled until an explicit execution grant.
- Publisher origin and Ed25519 signature verification are available as local
  operator checks, but they do not prove marketplace approval.
- Network-capable plugin actions require declared hosts and explicit
  subprocess_stdio_network enablement.
- Subprocess plugins run with a cleared environment and deterministic allowlist.

Manual plugin trust checks still required before marketplace safety language:
- Run the marketplace review workflow for every public plugin listing.
- Preserve malware scan evidence for distributed plugin archives and updates.
- Validate signed publisher policy for trusted publisher keys and revocation.
- Validate the macOS sandbox profile or equivalent OS-level confinement.
- Validate host-level egress enforcement with a network-deny fixture and a
  declared-host allow fixture.
- Record all JARVIS_PLUGIN_QA_* flags as true, then rerun this script with
  --assert-complete on the validated release machine with owner, timestamp, and
  evidence-note fields populated.
- Preserve the generated JSON report from --assert-complete as release evidence.

Proof boundary: preflight and runbook only; no marketplace review, malware scan,
OS sandbox deployment, or network egress enforcement was performed by --check.
CHECKLIST
  exit 0
fi

require_true JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED
require_true JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED
require_true JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED
require_true JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED
require_true JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED
require_true JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED
require_non_empty_env JARVIS_PLUGIN_QA_OWNER_NAME
require_utc_env_timestamp_order JARVIS_PLUGIN_QA_REVIEW_STARTED_AT JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT
require_not_future_utc_env_timestamp JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT
require_non_empty_env JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE
require_non_empty_env JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE
require_non_empty_env JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE
require_non_empty_env JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE
require_non_empty_env JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE
require_non_empty_env JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE
write_report

cat <<EOF
Jarvis plugin trust QA assertion: complete
Report: $REPORT_PATH
Proof boundary: owner-recorded marketplace review, malware scan, OS sandbox,
egress enforcement, signed publisher policy, and manual trust review evidence
only; this still does not prove those systems are available in the repo-local
test environment.
EOF
