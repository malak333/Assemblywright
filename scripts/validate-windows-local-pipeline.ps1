[CmdletBinding()]
param([switch]$LiveDemo)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot

function Invoke-CargoGate([string[]]$Arguments) {
    Write-Host ('Checking: cargo ' + ($Arguments -join ' '))
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) { throw ('Gate failed: cargo ' + ($Arguments -join ' ')) }
}

Push-Location $repositoryRoot
try {
    $scriptErrors = $null
    $scriptTokens = $null
    [System.Management.Automation.Language.Parser]::ParseFile(
        (Join-Path $PSScriptRoot 'windows-local-pipeline.ps1'),
        [ref]$scriptTokens, [ref]$scriptErrors
    ) | Out-Null
    if ($scriptErrors.Count -gt 0) { throw ($scriptErrors | Out-String) }

    Invoke-CargoGate @('fmt', '--all', '--check')
    Invoke-CargoGate @('clippy', '-p', 'assemblywright-protocol', '--all-targets', '--locked', '--', '-D', 'warnings')
    Invoke-CargoGate @('clippy', '-p', 'assemblywright-master', '--all-targets', '--locked', '--', '-D', 'warnings')
    Invoke-CargoGate @('test', '-p', 'assemblywright-protocol', '--locked')
    Invoke-CargoGate @('test', '-p', 'assemblywright-master', '--locked')
    & git diff --check
    if ($LASTEXITCODE -ne 0) { throw 'Git diff whitespace check failed.' }

    $bash = Join-Path $env:ProgramFiles 'Git/bin/bash.exe'
    if (-not (Test-Path -LiteralPath $bash)) {
        $bash = (Get-Command bash -ErrorAction Stop).Source
    }
    & $bash ./scripts/release-docs-drift-smoke.sh
    if ($LASTEXITCODE -ne 0) { throw 'Documentation contract check failed.' }

    if ($LiveDemo) {
        # Use a child PowerShell because the launcher propagates the runner exit.
        & powershell.exe -NoProfile -File (Join-Path $PSScriptRoot 'windows-local-pipeline.ps1') -Demo
        if ($LASTEXITCODE -ne 0) { throw 'Live local-model demonstration failed.' }
    }
    Write-Host 'Windows local pipeline gate passed.'
} finally { Pop-Location }
