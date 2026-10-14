[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$sourceRoot = Split-Path -Parent $PSScriptRoot
$temporaryBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
$testRoot = [System.IO.Path]::GetFullPath((Join-Path $temporaryBase ('assemblywright-selector-cli-' + [guid]::NewGuid().ToString('N'))))
$runtimePath = [System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
$results = New-Object System.Collections.Generic.List[object]
$failure = $null
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Quote-PowerShellLiteral([string]$Value) { return "'" + $Value.Replace("'", "''") + "'" }

function Require([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }

function Run-Scenario([string]$Name, [string]$InitialExitCode, [bool]$InjectFailure, [bool]$InjectCleanupFailure = $false) {
    $scenarioRoot = Join-Path $testRoot $Name
    [System.IO.Directory]::CreateDirectory($scenarioRoot) | Out-Null
    $testPath = Join-Path $scenarioRoot 'test-release-local-selector.ps1'
    $selectorPath = Join-Path $scenarioRoot 'select-release-local.ps1'
    $proofPath = Join-Path $scenarioRoot 'matrix.json'
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'test-release-local-selector.ps1') -Destination $testPath
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'select-release-local.ps1') -Destination $selectorPath
    if ($InjectFailure) { [System.IO.File]::WriteAllText($selectorPath, "throw 'Injected native CLI selector rejection'", $utf8) }
    $command = '$ErrorActionPreference = ''Stop''; $global:LASTEXITCODE = ' + $InitialExitCode + '; & ' + (Quote-PowerShellLiteral $testPath) + ' -EvidencePath ' + (Quote-PowerShellLiteral $proofPath) + '; if ($LASTEXITCODE -ne 0) { throw ''Selector CLI did not report exit zero after passing.'' }'
    if ($InjectCleanupFailure) {
        # Delete the already-guarded fixture tree normally, then reject cleanup.
        # The child function exists only in this disposable native test process.
        $cleanupFixture = 'function global:Remove-Item { param([string]$LiteralPath,[switch]$Recurse,[switch]$Force); Microsoft.PowerShell.Management\Remove-Item -LiteralPath $LiteralPath -Recurse -Force; throw ''Injected native CLI cleanup rejection'' }; '
        $command = $cleanupFixture + $command
    }
    $start = New-Object System.Diagnostics.ProcessStartInfo
    $start.FileName = $runtimePath
    $start.Arguments = '-NoProfile -NonInteractive -EncodedCommand ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $start
    try {
        Require ($process.Start()) ('Could not start native selector CLI scenario: ' + $Name)
        $outputTask = $process.StandardOutput.ReadToEndAsync()
        $errorTask = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(120000)) {
            $process.Kill()
            $process.WaitForExit(10000) | Out-Null
            throw ('Native selector CLI scenario timed out: ' + $Name)
        }
        $output = $outputTask.GetAwaiter().GetResult()
        $errorOutput = $errorTask.GetAwaiter().GetResult()
        if ($InjectFailure -or $InjectCleanupFailure) {
            Require ($process.ExitCode -ne 0) 'A rejected selector must exit nonzero.'
            $expectedError = if ($InjectCleanupFailure) { 'Injected native CLI cleanup rejection' } else { 'Injected native CLI selector rejection' }
            Require ($errorOutput.Contains($expectedError)) 'The expected injected rejection was not retained.'
            Require (-not (Test-Path -LiteralPath $proofPath)) 'A rejected selector must not emit passing matrix evidence.'
            Require (-not $output.Contains('"passed"')) 'A rejected selector must not print a passing report.'
        }
        else {
            Require ($process.ExitCode -eq 0) ('Passed selector CLI failed for ' + $Name + ': ' + $errorOutput)
            Require (Test-Path -LiteralPath $proofPath) 'A successful selector CLI must emit fresh matrix evidence.'
            $matrix = Get-Content -LiteralPath $proofPath -Raw | ConvertFrom-Json
            Require ($matrix.status -eq 'passed' -and $matrix.scenario_count -eq 18) 'The native selector CLI did not complete all eighteen actual event cases.'
            Require ($output.Contains('"passed"')) 'The native selector CLI omitted its successful report.'
        }
        $results.Add([ordered]@{ name=$Name; status='passed'; native_exit_code=$process.ExitCode; initial_exit_code=$InitialExitCode; rejected=($InjectFailure -or $InjectCleanupFailure) })
    }
    finally { $process.Dispose() }
}

try {
    [System.IO.Directory]::CreateDirectory($testRoot) | Out-Null
    Run-Scenario 'unset_exit_code' '$null' $false
    Run-Scenario 'inherited_nonzero_exit_code' '37' $false
    Run-Scenario 'selector_rejection' '$null' $true
    Run-Scenario 'cleanup_rejection' '$null' $false $true
}
catch { $failure = $_ }
finally {
    $resolvedRoot = [System.IO.Path]::GetFullPath($testRoot)
    $parent = [System.IO.Directory]::GetParent($resolvedRoot)
    if ($null -eq $parent -or -not $parent.FullName.Equals($temporaryBase, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not ([System.IO.Path]::GetFileName($resolvedRoot) -match '^assemblywright-selector-cli-[0-9a-f]{32}$')) {
        throw 'Refusing to remove a selector CLI fixture outside its expected temporary directory.'
    }
    if (Test-Path -LiteralPath $resolvedRoot) { Remove-Item -LiteralPath $resolvedRoot -Recurse -Force }
}

$evidenceRoot = Join-Path $sourceRoot 'target/windows-ci'
[System.IO.Directory]::CreateDirectory($evidenceRoot) | Out-Null
$proof = [ordered]@{
    status = if ($failure) { 'failed' } else { 'passed' }
    powershell_version = $PSVersionTable.PSVersion.ToString()
    scenario_count = $results.Count
    scenarios = @($results | ForEach-Object { $_ })
    error = if ($failure) { $failure.Exception.Message } else { $null }
}
[System.IO.File]::WriteAllText((Join-Path $evidenceRoot 'windows-ci-scope-cli.json'), ($proof | ConvertTo-Json -Depth 6), $utf8)
if ($failure) { throw $failure }
Write-Host 'Native selector CLI regression passed: 4 scenarios.'
$global:LASTEXITCODE = 0
