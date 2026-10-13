[CmdletBinding()]
param([switch]$LiveDemo)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repositoryRoot
try {
    & (Join-Path $PSScriptRoot 'build-windows-gui.ps1')
    $guiExe = Join-Path $repositoryRoot 'target/windows-gui/Assemblywright.exe'
    $unitEvidence = Join-Path $repositoryRoot 'target/windows-gui/gui-unit-tests.json'
    $selfTest = Start-Process -FilePath $guiExe -ArgumentList @('--self-test', ('"' + $unitEvidence + '"')) -WindowStyle Hidden -PassThru -Wait
    try {
        if ($selfTest.ExitCode -ne 0) { throw 'Windows GUI unit tests failed.' }
    } finally { $selfTest.Dispose() }
    $units = Get-Content -LiteralPath $unitEvidence -Raw | ConvertFrom-Json
    if ($units.status -ne 'passed') { throw 'Windows GUI unit evidence did not report success.' }
    if (@($units.passed).Count -ne 14) { throw "Windows GUI unit evidence reported $(@($units.passed).Count) tests instead of 14." }

    & (Join-Path $PSScriptRoot 'test-windows-gui.ps1') -NoBuild -LiveDemo:$LiveDemo
    & cargo test -p assemblywright-master --bin assemblywright-local --bin assemblywright-github --test windows_local_pipeline_e2e --test windows_local_github_e2e --locked
    if ($LASTEXITCODE -ne 0) { throw 'Windows local runner regression tests failed.' }
    & 'C:/Program Files/Git/bin/bash.exe' ./scripts/release-docs-drift-smoke.sh
    if ($LASTEXITCODE -ne 0) { throw 'Documentation contract check failed.' }
    & git diff --check
    if ($LASTEXITCODE -ne 0) { throw 'Git whitespace check failed.' }
    Write-Host 'Windows GUI gate passed.'
} finally { Pop-Location }
