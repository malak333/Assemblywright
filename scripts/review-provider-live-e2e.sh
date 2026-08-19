#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

if [[ "${1:---check}" == "--check" ]]; then
  [[ "$#" -le 1 ]] || fail "the review-provider harness accepts no extra arguments"
  command -v python3 >/dev/null 2>&1 || fail "python3 is unavailable"
  printf 'Assemblywright review-provider live E2E harness check: ok\n'
  printf 'Proof boundary: static harness shape only; no provider call ran.\n'
  exit 0
fi

[[ "$#" -eq 1 && "$1" == "--run" ]] || fail "the review-provider harness accepts only --check or internal --run"
[[ "${ASSEMBLYWRIGHT_REVIEW_PROVIDER_INTERNAL_STDIN_V1:-}" == "assemblywright.review-provider-live-proof.v1" ]] \
  || fail "live execution requires the fixed proof controller"
[[ "${ASSEMBLYWRIGHT_REVIEW_PROVIDER_RECEIPT_FD:-}" == "3" ]] \
  || fail "live execution requires the isolated receipt descriptor"
[[ "${ASSEMBLYWRIGHT_REVIEW_PROVIDER_EXPECTED_HEAD:-}" =~ ^[0-9a-f]{40}$ ]] \
  || fail "the expected Windows source commit is unavailable"

printf 'assemblywright_review_provider_windows_run_required action=Run confirm=true receipt_fd=3\n'
receipt=""
IFS= read -r -u 3 receipt || fail "the sanitized Windows receipt was incomplete"
[[ -n "$receipt" && "${#receipt}" -le 8192 ]] || fail "the sanitized Windows receipt was empty or oversized"

printf '%s\n' "$receipt" | python3 -c '
import json, re, sys, time

def pairs(items):
    result = {}
    for key, value in items:
        if key in result:
            raise ValueError("duplicate key")
        result[key] = value
    return result

raw = sys.stdin.buffer.read(8193)
if not raw or len(raw) > 8192 or b"\n" in raw.rstrip(b"\n"):
    raise SystemExit(1)
try:
    value = json.loads(raw, object_pairs_hook=pairs)
except Exception:
    raise SystemExit(1)
expected = {
    "schema_version", "status", "source_head", "protocol_version",
    "master_schema_version", "provider_id", "model_id",
    "service_executable_sha256", "review_provider_executable_sha256",
    "codex_executable_sha256", "output_schema_sha256",
    "approval_packet_sha256", "approval_output_sha256",
    "rejection_packet_sha256", "rejection_output_sha256", "observed_at_ms",
}
sha = re.compile(r"^[0-9a-f]{64}$")
head = re.compile(r"^[0-9a-f]{40}$")
now = int(time.time() * 1000)
if not isinstance(value, dict) or set(value) != expected:
    raise SystemExit(1)
if (type(value["schema_version"]) is not int or value["schema_version"] != 1 or
    value["status"] != "review_provider_windows_live_passed" or
    not isinstance(value["source_head"], str) or not head.fullmatch(value["source_head"]) or
    value["source_head"] != __import__("os").environ["ASSEMBLYWRIGHT_REVIEW_PROVIDER_EXPECTED_HEAD"] or
    type(value["protocol_version"]) is not int or value["protocol_version"] != 5 or
    type(value["master_schema_version"]) is not int or value["master_schema_version"] != 19 or
    value["provider_id"] != "openai.codex" or value["model_id"] != "gpt-5.6-sol" or
    type(value["observed_at_ms"]) is not int or value["observed_at_ms"] < now - 3600000 or
    value["observed_at_ms"] > now + 30000):
    raise SystemExit(1)
for key in ("service_executable_sha256", "review_provider_executable_sha256", "codex_executable_sha256", "output_schema_sha256", "approval_packet_sha256", "approval_output_sha256", "rejection_packet_sha256", "rejection_output_sha256"):
    if not isinstance(value[key], str) or not sha.fullmatch(value[key]):
        raise SystemExit(1)
print("assemblywright_review_provider_live_e2e_ok", end="")
for key in ("source_head", "protocol_version", "master_schema_version", "provider_id", "model_id", "service_executable_sha256", "review_provider_executable_sha256", "codex_executable_sha256", "output_schema_sha256", "approval_packet_sha256", "approval_output_sha256", "rejection_packet_sha256", "rejection_output_sha256", "observed_at_ms"):
    print(f" {key}={value[key]}", end="")
print()
' || fail "the sanitized Windows receipt failed strict validation"
