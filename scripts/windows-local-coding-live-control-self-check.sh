#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONTROLLER="$ROOT_DIR/scripts/windows-local-coding-live-control.ps1"

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

[[ -f "$CONTROLLER" ]] || fail "missing Windows local-coding live controller"

for required in \
  '"Check", "Prepare", "ClaimAndDispatch", "Integrate", "Cancel", "Abandon", "Cleanup"' \
  '$masterSchemaVersion = 19' \
  '$featureConveyorProjectionSchemaVersion = 9' \
  '$ownerControlSchemaVersion = 1' \
  'Remove-Item -LiteralPath "Env:$($gitEnvironmentEntry.Name)"' \
  '$env:GIT_CONFIG_GLOBAL = "NUL"' \
  '$env:GIT_CONFIG_NOSYSTEM = "1"' \
  'acceptance = @("restricted-worker-live-attempt")' \
  'acceptance_criteria_count = 1' \
  'allowed_paths = @("README.md")' \
  'tool_id = "file.write.v1"' \
  'expected_before_sha256 = @(Get-GitBlobSha256Bytes $paths.proof $HeadCommit "README.md")' \
  'Assert-SourceRepositoryEligible' \
  'normal tracked-index state' \
  'git --no-replace-objects -c core.fsmonitor=false -c core.hooksPath=NUL' \
  'git --no-replace-objects -c core.autocrlf=false -c core.fsmonitor=false -c core.hooksPath=NUL' \
  'Immutable Git blob binding did not reject CRLF working-tree or path drift.' \
  'git_blob_crlf_regression' \
  'commit.gpgSign=false' \
  'core.autocrlf=false' \
  'core.hooksPath=$checkHooks' \
  '$ErrorActionPreference = $priorErrorActionPreference' \
  '$packetJson = $packetDocument | ConvertTo-Json -Compress -Depth 12' \
  '$packetDigest = @(Get-Sha256Bytes $packetJson)' \
  '[UInt64]$status.schema_version -ne $featureConveyorProjectionSchemaVersion' \
  'schema_version = $ownerControlSchemaVersion' \
  'local_coding_live_control_ready' \
  '/v1/feature-conveyor/repository-grants' \
  '/v1/feature-conveyor/repository-preflight' \
  '/v1/feature-conveyor/repository-snapshot-claims' \
  '/v1/feature-conveyor/coding-dispatches' \
  '/v1/feature-conveyor/features/$FeatureId/integration-plan' \
  '/v1/feature-conveyor/artifact-integrations' \
  'artifact_integration_candidate_frozen' \
  'candidate_remote_absent = $true' \
  'candidate_fsck_clean = $true' \
  'exact_retry_idempotent = $true' \
  '/v1/feature-conveyor/cancel-active-feature' \
  '/v1/feature-conveyor/abandon-and-advance' \
  'safe_reconciliation_sha256' \
  'mac_cleanup_sha256' \
  'local_coding_disposable_checkout' \
  'Assert-NoReparseComponents' \
  'Assert-NoReparseTree' \
  'Remove-BoundedCommitGraphCache' \
  'Assert-SnapshotCompatibleObjectStore' \
  'graph-([0-9a-f]{40}|[0-9a-f]{64})' \
  'clone --no-local --single-branch --branch main' \
  'schema_version = 2' \
  'queue_revision = $prepareQueueRevision' \
  'enqueue_already_committed = $enqueueAlreadyCommitted' \
  'An unmarked partial clone can be recovered only while the queue is empty.' \
  'The unmarked partial clone did not match the exact recoverable source.' \
  'Write-ProofMarkerAtomically $paths.proof $marker' \
  '[IO.FileOptions]::WriteThrough' \
  '$stream.Flush($true)' \
  '[IO.File]::Move($temporaryPath, $markerPath)' \
  '$partialRemotes -cne "origin"' \
  '@("remote", "get-url", "--all", "origin")' \
  'Prepare found state other than its empty baseline or one exact committed enqueue.' \
  'Prepare found a non-resumable $kind grant revision.' \
  'non-resumable $kind grant revision' \
  'Cleanup without a checkout requires its exact prior binding.' \
  '$absentGrantCount += 1' \
  'grant_cleanup_status = "absent_or_revoked"' \
  'transfer_staging_empty' \
  'proof_checkout_removed'; do
  grep -Fq -- "$required" "$CONTROLLER" \
    || fail "Windows local-coding controller omitted: $required"
done

if grep -Fq -- '/v1/distributed/feature-conveyor/cancel-active-feature' "$CONTROLLER" \
  || grep -Fq -- '/v1/distributed/feature-conveyor/abandon-and-advance' "$CONTROLLER" \
  || grep -Fq -- 'assemblywright.local-coding-live.work-packet.v1' "$CONTROLLER" \
  || grep -Fq -- 'work_packet = [ordered]@{ packet_id = $packet; ordinal = 1; acceptance_criteria_count = 1 }' "$CONTROLLER" \
  || grep -Eiq -- 'sqlite|master\.sqlite3' "$CONTROLLER"; then
  fail "Windows local-coding controller crossed the owner-local or persistence boundary"
fi

grep -Fq -- 'retain no workspace' "$CONTROLLER" \
  && fail "Windows local-coding controller retained the stale protocol-v4 no-retention criterion"

tree_check_line="$(grep -nF 'Assert-NoReparseTree $paths.proof' "$CONTROLLER" | tail -1 | cut -d: -f1)"
delete_line="$(grep -nF 'Remove-Item -LiteralPath $paths.proof -Recurse -Force' "$CONTROLLER" | tail -1 | cut -d: -f1)"
terminal_grants_line="$(grep -nF 'Cleanup did not reach an absent-or-revoked terminal grant state.' "$CONTROLLER" | cut -d: -f1)"
prepare_status_line="$(grep -nF '$status = Get-ConveyorStatus' "$CONTROLLER" | head -1 | cut -d: -f1)"
clone_line="$(grep -nF '$cloneOutput = @(& git --no-replace-objects' "$CONTROLLER" | cut -d: -f1)"
clone_policy_line="$(grep -nF '$ErrorActionPreference = "Continue"' "$CONTROLLER" | tail -1 | cut -d: -f1)"
clone_exit_line="$(grep -nF '$cloneExitCode = $LASTEXITCODE' "$CONTROLLER" | cut -d: -f1)"
marker_recovery_line="$(grep -nF '$marker = Read-ProofMarker $paths.proof $paths.source' "$CONTROLLER" | tail -1 | cut -d: -f1)"
normalize_line="$(grep -nF 'Remove-BoundedCommitGraphCache $paths.proof' "$CONTROLLER" | tail -1 | cut -d: -f1)"
snapshot_compatibility_line="$(grep -nF 'Assert-SnapshotCompatibleObjectStore $paths.proof' "$CONTROLLER" | tail -1 | cut -d: -f1)"
claim_marker_line="$(grep -nF 'Assert-ProofRepositoryClean $paths.proof $paths.source $RepositoryId $FeatureId $HeadCommit' "$CONTROLLER" | head -1 | cut -d: -f1)"
claim_line="$(grep -nF '$claim = Invoke-ExactPost -Path "/v1/feature-conveyor/repository-snapshot-claims"' "$CONTROLLER" | cut -d: -f1)"
[[ "$tree_check_line" =~ ^[0-9]+$ && "$delete_line" =~ ^[0-9]+$ \
  && "$terminal_grants_line" =~ ^[0-9]+$ \
  && "$prepare_status_line" =~ ^[0-9]+$ && "$clone_line" =~ ^[0-9]+$ \
  && "$clone_policy_line" =~ ^[0-9]+$ && "$clone_exit_line" =~ ^[0-9]+$ \
  && "$marker_recovery_line" =~ ^[0-9]+$ \
  && "$normalize_line" =~ ^[0-9]+$ && "$snapshot_compatibility_line" =~ ^[0-9]+$ \
  && "$claim_marker_line" =~ ^[0-9]+$ \
  && "$claim_line" =~ ^[0-9]+$ \
  && "$prepare_status_line" -lt "$clone_line" \
  && "$clone_policy_line" -lt "$clone_line" && "$clone_line" -lt "$clone_exit_line" \
  && "$marker_recovery_line" -lt "$delete_line" \
  && "$claim_marker_line" -lt "$normalize_line" \
  && "$normalize_line" -lt "$snapshot_compatibility_line" \
  && "$snapshot_compatibility_line" -lt "$claim_line" \
  && "$tree_check_line" -lt "$delete_line" \
  && "$terminal_grants_line" -lt "$delete_line" ]] \
  || fail "destructive cleanup was not ordered after exact reparse and grant checks"

printf 'Assemblywright Windows local-coding live controller self-check: ok\n'
