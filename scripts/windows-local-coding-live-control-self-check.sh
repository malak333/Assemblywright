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
  '"Check", "Prepare", "ClaimAndDispatch", "Cancel", "Abandon", "Cleanup"' \
  '$masterSchemaVersion = 11' \
  'local_coding_live_control_ready' \
  '/v1/feature-conveyor/repository-grants' \
  '/v1/feature-conveyor/repository-preflight' \
  '/v1/feature-conveyor/repository-snapshot-claims' \
  '/v1/feature-conveyor/coding-dispatches' \
  '/v1/feature-conveyor/cancel-active-feature' \
  '/v1/feature-conveyor/abandon-and-advance' \
  'safe_reconciliation_sha256' \
  'mac_cleanup_sha256' \
  'local_coding_disposable_checkout' \
  'Assert-NoReparseComponents' \
  'Assert-NoReparseTree' \
  'non-resumable $kind grant revision' \
  'Cleanup without a checkout requires its exact prior binding.' \
  '$absentGrantCount += 1' \
  'grant_cleanup_status = "absent_or_revoked"' \
  'transfer_staging_empty' \
  'proof_checkout_removed'; do
  rg -Fq -- "$required" "$CONTROLLER" \
    || fail "Windows local-coding controller omitted: $required"
done

if rg -Fq -- '/v1/distributed/feature-conveyor/cancel-active-feature' "$CONTROLLER" \
  || rg -Fq -- '/v1/distributed/feature-conveyor/abandon-and-advance' "$CONTROLLER" \
  || rg -iq -- 'sqlite|master\.sqlite3' "$CONTROLLER"; then
  fail "Windows local-coding controller crossed the owner-local or persistence boundary"
fi

tree_check_line="$(rg -n -F 'Assert-NoReparseTree $paths.proof' "$CONTROLLER" | cut -d: -f1)"
delete_line="$(rg -n -F 'Remove-Item -LiteralPath $paths.proof -Recurse -Force' "$CONTROLLER" | cut -d: -f1)"
terminal_grants_line="$(rg -n -F 'Cleanup did not reach an absent-or-revoked terminal grant state.' "$CONTROLLER" | cut -d: -f1)"
prepare_status_line="$(rg -n -F '$status = Get-ConveyorStatus' "$CONTROLLER" | head -1 | cut -d: -f1)"
clone_line="$(rg -n -F '$cloneOutput = @(& git clone' "$CONTROLLER" | cut -d: -f1)"
marker_recovery_line="$(rg -n -F '$marker = Read-ProofMarker $paths.proof $paths.source' "$CONTROLLER" | tail -1 | cut -d: -f1)"
[[ "$tree_check_line" =~ ^[0-9]+$ && "$delete_line" =~ ^[0-9]+$ \
  && "$terminal_grants_line" =~ ^[0-9]+$ \
  && "$prepare_status_line" =~ ^[0-9]+$ && "$clone_line" =~ ^[0-9]+$ \
  && "$marker_recovery_line" =~ ^[0-9]+$ \
  && "$prepare_status_line" -lt "$clone_line" \
  && "$marker_recovery_line" -lt "$delete_line" \
  && "$tree_check_line" -lt "$delete_line" \
  && "$terminal_grants_line" -lt "$delete_line" ]] \
  || fail "destructive cleanup was not ordered after exact reparse and grant checks"

printf 'Assemblywright Windows local-coding live controller self-check: ok\n'
