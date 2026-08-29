#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONTROLLER="$ROOT_DIR/scripts/windows-repository-onboarding.ps1"

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

[[ -f "$CONTROLLER" ]] || fail "missing Windows repository-onboarding controller"

for required in \
  '[ValidateSet("Plan", "Check", "Approve", "SelfTest")]' \
  '[switch]$ConfirmRegistration' \
  '[switch]$ConfirmCloudDisclosure' \
  '[switch]$ConfirmAutonomousPublication' \
  'Plan requires only -RepositoryPath and accepts no approval switches.' \
  'Approve requires separate -ConfirmRegistration, -ConfirmCloudDisclosure, and -ConfirmAutonomousPublication switches.' \
  '$planLifetimeMs = [UInt64](24 * 60 * 60 * 1000)' \
  '$planDirectoryLeaf = "repository-onboarding-plans"' \
  'Assert-FixedLocalPath' \
  '[IO.DriveType]::Fixed' \
  'Assert-NoReparseComponents' \
  'Worktree, linked, or submodule Git metadata is not eligible for onboarding.' \
  '(Join-Path $Repository ".gitmodules")' \
  '(Join-Path $GitDirectory "config.worktree")' \
  'refs/heads/main' \
  'exact clean standard main checkout with normal tracked-index state' \
  'FileFlagOpenReparsePoint' \
  'information.NumberOfLinks != 1' \
  'FileShareRead' \
  'SetAccessRuleProtection($true, $false)' \
  'The onboarding plan ACL was not limited to owner and SYSTEM.' \
  'Get-CanonicalPlanLine' \
  'Get-ApprovalPlanDocument' \
  'assemblywright.repository-onboarding.$Purpose.v1' \
  'Get-PreflightFingerprintHex' \
  'assemblywright.repository-preflight.v1' \
  '/v1/feature-conveyor/repository-grants' \
  '/v1/feature-conveyor/repositories/$RepositoryId/grants' \
  '/v1/feature-conveyor/repository-preflight' \
  'The current repository grant was not the exact resumable revision-1 plan binding.' \
  '[void](Assert-RepositoryEligible ([string]$plan.repository_path) ([string]$plan.head_commit))' \
  'rerun Check before any deliberate retry' \
  'Assert-GrantSetPauseEpoch' \
  'The repository-onboarding Emergency Pause epoch changed.' \
  '$initialPauseRevision = [UInt64]$set.emergency_pause_revision' \
  '[UInt64]$receipt.emergency_pause_revision -ne $ExpectedPauseRevision' \
  '$receiptStatus = "repository_onboarding_ready"' \
  'registration_grant_revision = 1' \
  'cloud_disclosure_grant_revision = 1' \
  'autonomous_publication_grant_revision = 1' \
  'approval_plan_sha256 = [string]$plan.approval_plan_sha256' \
  'preflight_fingerprint_sha256 = $fingerprint' \
  'The authoring receipt was not path-free.' \
  'repository_onboarding_self_test_passed' \
  'exact_shape_negative' \
  'pause_epoch_churn_negative' \
  'root_gitmodules_negative' \
  'config_worktree_negative' \
  'revision_resume_negative'; do
  grep -Fq -- "$required" "$CONTROLLER" \
    || fail "Windows repository-onboarding controller omitted: $required"
done

if grep -Fq -- '/v1/distributed/' "$CONTROLLER" \
  || grep -Eiq -- 'sqlite|master\.sqlite3|create[[:space:]]+repository|git[[:space:]]+init|git[[:space:]]+clone' "$CONTROLLER" \
  || grep -Eq -- 'Authorization[[:space:]]*=.*\$[Rr]epository|Bearer.*Write-(Host|Output)' "$CONTROLLER"; then
  fail "Windows repository-onboarding controller crossed persistence, creation, remote, or token-output boundaries"
fi

plan_branch="$(grep -nF 'if ($Action -eq "Plan")' "$CONTROLLER" | cut -d: -f1)"
token_open="$(grep -nF 'Open-OwnerLoopback' "$CONTROLLER" | tail -1 | cut -d: -f1)"
confirmation="$(grep -nF 'Approve requires separate -ConfirmRegistration' "$CONTROLLER" | cut -d: -f1)"
grant_post="$(grep -nF 'Invoke-ExactPost "/v1/feature-conveyor/repository-grants"' "$CONTROLLER" | cut -d: -f1)"
preflight_post="$(grep -nF 'Invoke-ExactPost "/v1/feature-conveyor/repository-preflight"' "$CONTROLLER" | cut -d: -f1)"
initial_pause="$(grep -nF '$initialPauseRevision = [UInt64]$set.emergency_pause_revision' "$CONTROLLER" | cut -d: -f1)"
grant_loop="$(grep -nF 'foreach ($kind in @("registration", "cloud_disclosure", "autonomous_publication"))' "$CONTROLLER" | tail -1 | cut -d: -f1)"
repository_recheck="$(grep -nF '[void](Assert-RepositoryEligible ([string]$plan.repository_path) ([string]$plan.head_commit))' "$CONTROLLER" | tail -1 | cut -d: -f1)"
receipt_write="$(grep -nF 'Write-PrivateFileAtomically $loaded.Paths.Receipt $receiptLine' "$CONTROLLER" | cut -d: -f1)"
receipt_output="$(grep -nF '    $receiptLine' "$CONTROLLER" | tail -1 | cut -d: -f1)"

[[ "$plan_branch" =~ ^[0-9]+$ && "$token_open" =~ ^[0-9]+$ \
  && "$confirmation" =~ ^[0-9]+$ && "$grant_post" =~ ^[0-9]+$ \
  && "$preflight_post" =~ ^[0-9]+$ && "$initial_pause" =~ ^[0-9]+$ \
  && "$grant_loop" =~ ^[0-9]+$ && "$repository_recheck" =~ ^[0-9]+$ \
  && "$receipt_write" =~ ^[0-9]+$ && "$receipt_output" =~ ^[0-9]+$ \
  && "$confirmation" -lt "$token_open" \
  && "$initial_pause" -lt "$grant_loop" \
  && "$grant_post" -lt "$preflight_post" \
  && "$preflight_post" -lt "$repository_recheck" \
  && "$repository_recheck" -lt "$receipt_write" \
  && "$receipt_write" -lt "$receipt_output" ]] \
  || fail "approval, revalidation, preflight, and receipt publication ordering drifted"

receipt_block="$(sed -n '/function Get-CanonicalAuthoringReceiptLine {/,/^}/p' "$CONTROLLER")"
grep -Fq 'repository_id = [string]$Receipt.repository_id' <<<"$receipt_block" \
  || fail "authoring receipt omitted repository identity"
if grep -Eiq -- 'repository_path|plan_id|owner_approval|token|endpoint' <<<"$receipt_block"; then
  fail "authoring receipt leaked private or non-contract fields"
fi

if command -v pwsh >/dev/null 2>&1; then
  self_test_data_dir="$(mktemp -d "${TMPDIR:-/tmp}/assemblywright-repository-onboarding-self-test.XXXXXX")"
  trap 'rm -rf -- "$self_test_data_dir"' EXIT
  pwsh -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass \
    -File "$CONTROLLER" -Action SelfTest -DataDir "$self_test_data_dir" >/dev/null
fi

printf 'Assemblywright Windows repository-onboarding self-check: ok\n'
