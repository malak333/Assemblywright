[CmdletBinding()]
param([switch]$LiveDemo)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repositoryRoot
try {
    $evidenceRoot = Join-Path $repositoryRoot ('target/windows-gui-tests/' + [guid]::NewGuid().ToString('N'))
    if (Test-Path -LiteralPath $evidenceRoot) { throw 'The Windows GUI gate evidence directory is not fresh.' }
    New-Item -ItemType Directory -Path $evidenceRoot | Out-Null
    & (Join-Path $PSScriptRoot 'build-windows-gui.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'The Windows GUI build failed.' }
    & (Join-Path $PSScriptRoot 'test-windows-process-capture.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'The Windows process-capture regression failed.' }
    $guiExe = Join-Path $repositoryRoot 'target/windows-gui/Assemblywright.exe'
    $unitEvidence = Join-Path $evidenceRoot 'gui-unit-tests.json'
    $selfTest = Start-Process -FilePath $guiExe -ArgumentList @('--self-test', ('"' + $unitEvidence + '"')) -WindowStyle Hidden -PassThru -Wait
    try {
        if ($selfTest.ExitCode -ne 0) { throw 'Windows GUI unit tests failed.' }
    } finally { $selfTest.Dispose() }
    $units = Get-Content -LiteralPath $unitEvidence -Raw | ConvertFrom-Json
    if ($units.status -ne 'passed') { throw 'Windows GUI unit evidence did not report success.' }
    if (@($units.passed).Count -ne 14) { throw "Windows GUI unit evidence reported $(@($units.passed).Count) tests instead of 14." }

    $guiEvidence = Join-Path $evidenceRoot 'gui-e2e.json'
    & (Join-Path $PSScriptRoot 'test-windows-gui.ps1') -NoBuild -LiveDemo:$LiveDemo -EvidencePath $guiEvidence
    if ($LASTEXITCODE -ne 0) { throw 'The Windows GUI E2E journey failed.' }
    if (-not (Test-Path -LiteralPath $guiEvidence -PathType Leaf)) { throw 'The Windows GUI E2E journey did not emit fresh completion evidence.' }
    $journey = Get-Content -LiteralPath $guiEvidence -Raw | ConvertFrom-Json
    if ($journey.status -ne 'passed') { throw 'The Windows GUI E2E evidence did not report success.' }
    if ($LiveDemo) {
        $demoJob = $journey.extended_fixture_saved_job
        if ([string]::IsNullOrWhiteSpace($journey.extended_fixture_run_id) -or
            $demoJob.implementation_model -ne 'gui-fixture' -or $demoJob.implementation_reasoning_effort -ne 'low' -or
            $demoJob.orchestrator_model -ne 'gpt-plan' -or $demoJob.orchestrator_reasoning_effort -ne 'high' -or
            $demoJob.reviewer_model -ne 'gpt-review' -or $demoJob.reviewer_reasoning_effort -ne 'xhigh' -or
            $demoJob.max_attempts -ne 2 -or $demoJob.timeout_seconds -ne 47 -or $demoJob.acceptance_tests -ne 12) {
            throw 'The extended Windows GUI journey did not prove its exact saved roles, completed demo and all twelve acceptance tests.'
        }
    }
    & cargo test -p assemblywright-master --bin assemblywright-local --bin assemblywright-github --test windows_local_pipeline_e2e --test windows_local_github_e2e --locked
    if ($LASTEXITCODE -ne 0) { throw 'Windows local runner regression tests failed.' }
    & 'C:/Program Files/Git/bin/bash.exe' ./scripts/release-docs-drift-smoke.sh
    if ($LASTEXITCODE -ne 0) { throw 'Documentation contract check failed.' }
    & git diff --check
    if ($LASTEXITCODE -ne 0) { throw 'Git whitespace check failed.' }
    Write-Host 'Windows GUI gate passed.'
} finally { Pop-Location }
