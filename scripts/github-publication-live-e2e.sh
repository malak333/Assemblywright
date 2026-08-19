#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

if [[ "${1:---check}" == "--check" ]]; then
  [[ "$#" -le 1 ]] || fail "the GitHub-publication harness accepts no extra arguments"
  command -v python3 >/dev/null 2>&1 || fail "python3 is unavailable"
  printf 'Assemblywright GitHub-publication live E2E harness check: ok\n'
  printf 'Proof boundary: static harness shape only; no GitHub operation ran.\n'
  exit 0
fi

[[ "$#" -eq 1 && "$1" == "--run" ]] \
  || fail "the GitHub-publication harness accepts only --check or internal --run"
[[ "${ASSEMBLYWRIGHT_GITHUB_PUBLICATION_INTERNAL_STDIN_V1:-}" == \
  "assemblywright.github-publication-live-proof.v1" ]] \
  || fail "live execution requires the fixed proof controller"
[[ "${ASSEMBLYWRIGHT_GITHUB_PUBLICATION_RECEIPT_FD:-}" == "3" ]] \
  || fail "live execution requires the isolated receipt descriptor"
[[ "${ASSEMBLYWRIGHT_GITHUB_PUBLICATION_EXPECTED_HEAD:-}" =~ ^[0-9a-f]{40}$ ]] \
  || fail "the expected source commit is unavailable"

printf 'assemblywright_github_publication_windows_run_required action=Run confirm=true receipt_fd=3\n'
receipt=""
IFS= read -r -u 3 receipt || fail "the sanitized Windows receipt was incomplete"
[[ -n "$receipt" && "${#receipt}" -le 8192 ]] \
  || fail "the sanitized Windows receipt was empty or oversized"

printf '%s\n' "$receipt" | python3 -c '
import json, os, re, sys, time

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
    "master_schema_version", "repository", "base_branch", "publication_commit",
    "resulting_main_commit", "pull_request_number", "pull_request_url_sha256",
    "branch_name_sha256", "required_checks_sha256", "post_merge_checks_sha256",
    "master_executable_sha256", "git_version", "git_executable_sha256", "gh_version", "gh_executable_sha256",
    "observed_at_ms",
}
sha = re.compile(r"^[0-9a-f]{64}$")
commit = re.compile(r"^[0-9a-f]{40}$")
now = int(time.time() * 1000)
if not isinstance(value, dict) or set(value) != expected:
    raise SystemExit(1)
if (type(value["schema_version"]) is not int or value["schema_version"] != 1 or
    value["status"] != "github_publication_windows_live_passed" or
    value["source_head"] != os.environ["ASSEMBLYWRIGHT_GITHUB_PUBLICATION_EXPECTED_HEAD"] or
    type(value["protocol_version"]) is not int or value["protocol_version"] != 5 or
    type(value["master_schema_version"]) is not int or value["master_schema_version"] != 19 or
    value["repository"] != "malak333/Assemblywright" or value["base_branch"] != "main" or
    not isinstance(value["publication_commit"], str) or not commit.fullmatch(value["publication_commit"]) or
    not isinstance(value["resulting_main_commit"], str) or not commit.fullmatch(value["resulting_main_commit"]) or
    value["source_head"] in (value["publication_commit"], value["resulting_main_commit"]) or
    value["resulting_main_commit"] == value["publication_commit"] or
    type(value["pull_request_number"]) is not int or value["pull_request_number"] < 1 or
    value["git_version"] != "2.55.0.windows.2" or value["gh_version"] != "2.96.0" or
    type(value["observed_at_ms"]) is not int or value["observed_at_ms"] < now - 3600000 or
    value["observed_at_ms"] > now + 30000):
    raise SystemExit(1)
for key in ("pull_request_url_sha256", "branch_name_sha256", "required_checks_sha256",
            "post_merge_checks_sha256", "master_executable_sha256", "git_executable_sha256", "gh_executable_sha256"):
    if not isinstance(value[key], str) or not sha.fullmatch(value[key]):
        raise SystemExit(1)
print("assemblywright_github_publication_live_e2e_ok", end="")
for key in ("source_head", "publication_commit", "resulting_main_commit", "protocol_version",
            "master_schema_version", "repository", "base_branch", "pull_request_number",
            "pull_request_url_sha256", "branch_name_sha256", "required_checks_sha256",
            "post_merge_checks_sha256", "master_executable_sha256", "git_version", "git_executable_sha256", "gh_version",
            "gh_executable_sha256", "observed_at_ms"):
    print(f" {key}={value[key]}", end="")
print()
' || fail "the sanitized Windows receipt failed strict validation"
