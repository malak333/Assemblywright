[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$sourceRoot = Split-Path -Parent $PSScriptRoot
$validateSource = Join-Path $PSScriptRoot 'validate-windows-gui.ps1'
$evidenceRoot = Join-Path $sourceRoot 'target/windows-ci'
$evidencePath = Join-Path $evidenceRoot 'windows-gui-gate.json'
$temporaryRoot = [System.IO.Path]::GetFullPath($env:TEMP)
$testRoot = [System.IO.Path]::GetFullPath((Join-Path $temporaryRoot ('assemblywright-windows-gui-gate-' + [guid]::NewGuid().ToString('N'))))
$utf8 = New-Object System.Text.UTF8Encoding($false)
$results = New-Object System.Collections.Generic.List[object]
$failure = $null

function Write-Utf8File([string]$Path, [string]$Text) {
    $parent = Split-Path -Parent $Path
    if ($parent) { New-Item -ItemType Directory -Path $parent -Force | Out-Null }
    [System.IO.File]::WriteAllText($Path, $Text, $utf8)
}

function Require([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Set-ScenarioEnvironment([hashtable]$Scenario, [string]$ScenarioRoot, [string]$FakeGui) {
    $env:AW_GATE_MARKERS = Join-Path $ScenarioRoot 'markers'
    $env:AW_GATE_FAKE_GUI = $FakeGui
    $env:AW_GATE_BUILD_EXIT = [string]$(if ($Scenario.ContainsKey('BuildExit')) { $Scenario.BuildExit } else { 0 })
    $env:AW_GATE_CAPTURE_EXIT = [string]$(if ($Scenario.ContainsKey('CaptureExit')) { $Scenario.CaptureExit } else { 0 })
    $env:AW_GATE_SELFTEST_MODE = [string]$(if ($Scenario.ContainsKey('SelfTestMode')) { $Scenario.SelfTestMode } else { 'pass' })
    $env:AW_GATE_SELFTEST_EXIT = [string]$(if ($Scenario.ContainsKey('SelfTestExit')) { $Scenario.SelfTestExit } else { 0 })
    $env:AW_GATE_E2E_MODE = [string]$(if ($Scenario.ContainsKey('E2eMode')) { $Scenario.E2eMode } else { 'pass' })
    $env:AW_GATE_E2E_EXIT = [string]$(if ($Scenario.ContainsKey('E2eExit')) { $Scenario.E2eExit } else { 0 })
    $env:AW_GATE_CARGO_EXIT = '0'
    $env:AW_GATE_DOCS_EXIT = '0'
    $env:AW_GATE_GIT_EXIT = '0'
    New-Item -ItemType Directory -Path $env:AW_GATE_MARKERS -Force | Out-Null
}

function Write-FakeRepository([string]$Repository) {
    $scripts = Join-Path $Repository 'scripts'
    $bin = Join-Path $Repository 'fake-bin'
    New-Item -ItemType Directory -Path $scripts, $bin -Force | Out-Null
    Copy-Item -LiteralPath $validateSource -Destination (Join-Path $scripts 'validate-windows-gui.ps1')
    Require `
        ((Get-FileHash -LiteralPath $validateSource -Algorithm SHA256).Hash -eq
            (Get-FileHash -LiteralPath (Join-Path $scripts 'validate-windows-gui.ps1') -Algorithm SHA256).Hash) `
        'The isolated wrapper copy did not match the production gate source.'

    Write-Utf8File (Join-Path $scripts 'build-windows-gui.ps1') @'
$ErrorActionPreference = 'Stop'
Add-Content -LiteralPath (Join-Path $env:AW_GATE_MARKERS 'order.txt') -Value 'build'
if ([int]$env:AW_GATE_BUILD_EXIT -ne 0) { exit ([int]$env:AW_GATE_BUILD_EXIT) }
$target = Join-Path (Split-Path -Parent $PSScriptRoot) 'target/windows-gui'
New-Item -ItemType Directory -Path $target -Force | Out-Null
Copy-Item -LiteralPath $env:AW_GATE_FAKE_GUI -Destination (Join-Path $target 'Assemblywright.exe') -Force
exit 0
'@
    Write-Utf8File (Join-Path $scripts 'test-windows-process-capture.ps1') @'
$ErrorActionPreference = 'Stop'
Add-Content -LiteralPath (Join-Path $env:AW_GATE_MARKERS 'order.txt') -Value 'capture'
exit ([int]$env:AW_GATE_CAPTURE_EXIT)
'@
    Write-Utf8File (Join-Path $scripts 'test-windows-gui.ps1') @'
[CmdletBinding()]
param([switch]$NoBuild, [switch]$LiveDemo, [string]$EvidencePath)
$ErrorActionPreference = 'Stop'
Add-Content -LiteralPath (Join-Path $env:AW_GATE_MARKERS 'order.txt') -Value 'e2e'
$repository = Split-Path -Parent $PSScriptRoot
$base = [System.IO.Path]::GetFullPath((Join-Path $repository 'target/windows-gui-tests'))
$path = if ([string]::IsNullOrWhiteSpace($EvidencePath)) { '' } else { [System.IO.Path]::GetFullPath($EvidencePath) }
$parentName = if ($path) { Split-Path -Leaf (Split-Path -Parent $path) } else { '' }
$prefix = $base.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
$valid = $NoBuild -and $path.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase) -and
    [System.IO.Path]::GetFileName($path) -ceq 'gui-e2e.json' -and $parentName -match '^[0-9a-f]{32}$' -and
    -not (Test-Path -LiteralPath $path)
$invocation = [ordered]@{ valid = [bool]$valid; live_demo = [bool]$LiveDemo; evidence_was_fresh = -not (Test-Path -LiteralPath $path) }
[System.IO.File]::WriteAllText(
    (Join-Path $env:AW_GATE_MARKERS 'e2e-invocation.json'),
    ($invocation | ConvertTo-Json -Compress),
    (New-Object System.Text.UTF8Encoding($false))
)
if (-not $valid) { exit 97 }
$mode = $env:AW_GATE_E2E_MODE
if ($mode -ne 'missing') {
    if ($mode -eq 'malformed') {
        $text = '{'
    } else {
        $status = if ($mode -eq 'failed') { 'failed' } else { 'passed' }
        $runId = if ($mode -eq 'live_missing_run') { '' } else { 'fixture-run-1' }
        $implementationModel = if ($mode -eq 'live_wrong_implementation') { 'forged-local' } else { 'gui-fixture' }
        $implementationEffort = if ($mode -eq 'live_wrong_implementation_effort') { 'high' } else { 'low' }
        $orchestratorModel = if ($mode -eq 'live_wrong_orchestrator') { 'forged-plan' } else { 'gpt-plan' }
        $orchestratorEffort = if ($mode -eq 'live_wrong_orchestrator_effort') { 'low' } else { 'high' }
        $reviewerModel = if ($mode -eq 'live_wrong_reviewer') { 'forged-review' } else { 'gpt-review' }
        $reviewerEffort = if ($mode -eq 'live_wrong_reviewer_effort') { 'high' } else { 'xhigh' }
        $attempts = if ($mode -eq 'live_wrong_attempts') { 3 } else { 2 }
        $timeout = if ($mode -eq 'live_wrong_timeout') { 48 } else { 47 }
        $acceptance = if ($mode -eq 'live_wrong_acceptance') { 11 } else { 12 }
        $value = [ordered]@{ status = $status }
        if ($LiveDemo -or $mode -like 'live_*') {
            $value.extended_fixture_run_id = $runId
            $value.extended_fixture_saved_job = [ordered]@{
                implementation_model = $implementationModel
                implementation_reasoning_effort = $implementationEffort
                orchestrator_model = $orchestratorModel
                orchestrator_reasoning_effort = $orchestratorEffort
                reviewer_model = $reviewerModel
                reviewer_reasoning_effort = $reviewerEffort
                max_attempts = $attempts
                timeout_seconds = $timeout
                acceptance_tests = $acceptance
            }
        }
        $text = $value | ConvertTo-Json -Depth 4
    }
    [System.IO.File]::WriteAllText($path, $text, (New-Object System.Text.UTF8Encoding($false)))
}
exit ([int]$env:AW_GATE_E2E_EXIT)
'@
    Write-Utf8File (Join-Path $scripts 'release-docs-drift-smoke.sh') @'
#!/usr/bin/env bash
printf 'docs\n' >> "$AW_GATE_MARKERS/order.txt"
exit "${AW_GATE_DOCS_EXIT:-0}"
'@
    Write-Utf8File (Join-Path $bin 'cargo.cmd') @'
@echo off
echo cargo>>"%AW_GATE_MARKERS%\order.txt"
exit /b %AW_GATE_CARGO_EXIT%
'@
    Write-Utf8File (Join-Path $bin 'git.cmd') @'
@echo off
echo git>>"%AW_GATE_MARKERS%\order.txt"
exit /b %AW_GATE_GIT_EXIT%
'@
}

function Run-Scenario([hashtable]$Scenario, [string]$FakeGui, [string]$PowerShellPath) {
    $scenarioRoot = Join-Path $testRoot $Scenario.Name
    $repository = Join-Path $scenarioRoot 'repository'
    New-Item -ItemType Directory -Path $repository -Force | Out-Null
    Write-FakeRepository $repository
    Set-ScenarioEnvironment $Scenario $scenarioRoot $FakeGui
    if ($Scenario.ContainsKey('StaleGui') -and $Scenario.StaleGui) {
        Write-Utf8File `
            (Join-Path $repository 'target/windows-gui-tests/stale/gui-e2e.json') `
            '{"status":"passed","extended_fixture_run_id":"stale","extended_fixture_saved_job":{"acceptance_tests":12}}'
    }
    if ($Scenario.ContainsKey('StaleUnit') -and $Scenario.StaleUnit) {
        Write-Utf8File `
            (Join-Path $repository 'target/windows-gui-tests/stale/gui-unit-tests.json') `
            '{"status":"passed","passed":[1,2,3,4,5,6,7,8,9,10,11,12,13,14]}'
    }

    $originalPath = $env:PATH
    $env:PATH = (Join-Path $repository 'fake-bin') + [System.IO.Path]::PathSeparator + $originalPath
    $stdout = Join-Path $scenarioRoot 'stdout.log'
    $stderr = Join-Path $scenarioRoot 'stderr.log'
    $arguments = @('-NoLogo', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', ('"' + (Join-Path $repository 'scripts/validate-windows-gui.ps1') + '"'))
    if ($Scenario.ContainsKey('LiveDemo') -and $Scenario.LiveDemo) { $arguments += '-LiveDemo' }
    try {
        $process = Start-Process -FilePath $PowerShellPath -ArgumentList $arguments -WorkingDirectory $repository `
            -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru -Wait
        try { $exitCode = $process.ExitCode } finally { $process.Dispose() }
    } finally {
        $env:PATH = $originalPath
    }
    $output = [System.IO.File]::ReadAllText($stdout) + [System.IO.File]::ReadAllText($stderr)
    $diagnosticOutput = $output -replace '(?m)^\s*\|\s?', ''
    $normalizedOutput = ($diagnosticOutput -replace '\s+', ' ').Trim()
    $orderPath = Join-Path $env:AW_GATE_MARKERS 'order.txt'
    $order = if (Test-Path -LiteralPath $orderPath) { @([System.IO.File]::ReadAllLines($orderPath) | ForEach-Object { $_.Trim() } | Where-Object { $_ }) } else { @() }
    $expectedOrder = @($Scenario.ExpectedOrder)
    Require (($order -join ',') -ceq ($expectedOrder -join ',')) `
        "Scenario $($Scenario.Name) ran the wrong commands: actual=$($order -join ',') expected=$($expectedOrder -join ','); output=$output"
    if ($Scenario.ExpectSuccess) {
        Require ($exitCode -eq 0) "Scenario $($Scenario.Name) failed with exit $exitCode`: $output"
        Require ($normalizedOutput.Contains('Windows GUI gate passed.')) "Scenario $($Scenario.Name) omitted its success result: $output"
    } else {
        Require ($exitCode -ne 0) "Scenario $($Scenario.Name) unexpectedly succeeded: $output"
        if ($Scenario.ContainsKey('ExpectedMessage')) {
            $normalizedExpected = ($Scenario.ExpectedMessage -replace '\s+', ' ').Trim()
            Require ($normalizedOutput.Contains($normalizedExpected)) `
                "Scenario $($Scenario.Name) omitted its failure reason '$($Scenario.ExpectedMessage)': $output"
        }
    }

    $freshRoots = @(Get-ChildItem -LiteralPath (Join-Path $repository 'target/windows-gui-tests') -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^[0-9a-f]{32}$' })
    Require ($freshRoots.Count -eq 1) "Scenario $($Scenario.Name) did not use exactly one fresh GUID evidence directory."
    $unitEvidence = Join-Path $freshRoots[0].FullName 'gui-unit-tests.json'
    $guiEvidence = Join-Path $freshRoots[0].FullName 'gui-e2e.json'
    if ($Scenario.ContainsKey('ExpectUnitPassArtifact') -and $Scenario.ExpectUnitPassArtifact) {
        Require (Test-Path -LiteralPath $unitEvidence -PathType Leaf) "Scenario $($Scenario.Name) omitted its fresh unit artifact."
        Require (((Get-Content -Raw $unitEvidence | ConvertFrom-Json).status) -eq 'passed') "Scenario $($Scenario.Name) unit artifact was not passing."
    }
    if ($Scenario.ContainsKey('ExpectGuiPassArtifact') -and $Scenario.ExpectGuiPassArtifact) {
        Require (Test-Path -LiteralPath $guiEvidence -PathType Leaf) "Scenario $($Scenario.Name) omitted its fresh GUI artifact."
        Require (((Get-Content -Raw $guiEvidence | ConvertFrom-Json).status) -eq 'passed') "Scenario $($Scenario.Name) GUI artifact was not passing."
    }
    if ($Scenario.ContainsKey('ExpectFreshGuiMissing') -and $Scenario.ExpectFreshGuiMissing) {
        Require (-not (Test-Path -LiteralPath $guiEvidence)) "Scenario $($Scenario.Name) unexpectedly emitted fresh GUI evidence."
        Require (Test-Path -LiteralPath (Join-Path $repository 'target/windows-gui-tests/stale/gui-e2e.json')) `
            "Scenario $($Scenario.Name) lost its stale control artifact."
    }
    $e2eInvocationPath = Join-Path $env:AW_GATE_MARKERS 'e2e-invocation.json'
    if ($order -contains 'e2e') {
        Require (Test-Path -LiteralPath $e2eInvocationPath) "Scenario $($Scenario.Name) omitted E2E invocation evidence."
        $invocation = Get-Content -Raw $e2eInvocationPath | ConvertFrom-Json
        Require ($invocation.valid -eq $true -and $invocation.evidence_was_fresh -eq $true) `
            "Scenario $($Scenario.Name) did not receive a fresh bounded -EvidencePath."
        Require ([bool]$invocation.live_demo -eq [bool]$Scenario.LiveDemo) `
            "Scenario $($Scenario.Name) forwarded the wrong LiveDemo value."
    }
    $leakedProcesses = @(Get-Process | ForEach-Object {
        try {
            if ($_.Path -and $_.Path.StartsWith($scenarioRoot, [System.StringComparison]::OrdinalIgnoreCase)) { $_ }
        } catch { }
    })
    Require ($leakedProcesses.Count -eq 0) "Scenario $($Scenario.Name) left a disposable fixture process running."
    $results.Add([ordered]@{
        name = $Scenario.Name
        status = 'passed'
        wrapper_exit_code = $exitCode
        commands = $order
        live_demo = [bool]$Scenario.LiveDemo
    })
}

$scenarioEnvironmentNames = @(
    'AW_GATE_MARKERS', 'AW_GATE_FAKE_GUI', 'AW_GATE_BUILD_EXIT', 'AW_GATE_CAPTURE_EXIT',
    'AW_GATE_SELFTEST_MODE', 'AW_GATE_SELFTEST_EXIT', 'AW_GATE_E2E_MODE', 'AW_GATE_E2E_EXIT',
    'AW_GATE_CARGO_EXIT', 'AW_GATE_DOCS_EXIT', 'AW_GATE_GIT_EXIT'
)
$priorEnvironment = @{}
foreach ($name in $scenarioEnvironmentNames) { $priorEnvironment[$name] = [System.Environment]::GetEnvironmentVariable($name, 'Process') }

try {
    Require (Test-Path -LiteralPath $validateSource -PathType Leaf) 'The Windows GUI gate source is missing.'
    Require (Test-Path -LiteralPath 'C:/Program Files/Git/bin/bash.exe' -PathType Leaf) 'Git Bash is required for the native gate regression.'
    New-Item -ItemType Directory -Path $testRoot, $evidenceRoot -Force | Out-Null
    $fixtureSource = Join-Path $testRoot 'fake-gui.cs'
    Write-Utf8File $fixtureSource @'
using System;
using System.IO;
using System.Text;
internal static class FakeGui {
    private static int Main(string[] arguments) {
        string markers = Environment.GetEnvironmentVariable("AW_GATE_MARKERS");
        Directory.CreateDirectory(markers);
        File.AppendAllText(Path.Combine(markers, "order.txt"), "selftest" + Environment.NewLine);
        if (arguments.Length != 2 || arguments[0] != "--self-test") return 96;
        string evidence = Path.GetFullPath(arguments[1]);
        File.WriteAllText(Path.Combine(markers, "selftest-evidence-path.txt"), evidence, new UTF8Encoding(false));
        string mode = Environment.GetEnvironmentVariable("AW_GATE_SELFTEST_MODE") ?? "pass";
        if (mode != "missing") {
            Directory.CreateDirectory(Path.GetDirectoryName(evidence));
            string text = mode == "malformed" ? "{" : mode == "failed"
                ? "{\"status\":\"failed\",\"passed\":[]}"
                : mode == "wrong_count"
                ? "{\"status\":\"passed\",\"passed\":[\"t01\",\"t02\",\"t03\",\"t04\",\"t05\",\"t06\",\"t07\",\"t08\",\"t09\",\"t10\",\"t11\",\"t12\",\"t13\"]}"
                : "{\"status\":\"passed\",\"passed\":[\"t01\",\"t02\",\"t03\",\"t04\",\"t05\",\"t06\",\"t07\",\"t08\",\"t09\",\"t10\",\"t11\",\"t12\",\"t13\",\"t14\"]}";
            File.WriteAllText(evidence, text, new UTF8Encoding(false));
        }
        int exitCode;
        return int.TryParse(Environment.GetEnvironmentVariable("AW_GATE_SELFTEST_EXIT"), out exitCode) ? exitCode : 0;
    }
}
'@
    $compiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
    if (-not (Test-Path -LiteralPath $compiler -PathType Leaf)) { $compiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework/v4.0.30319/csc.exe' }
    $fakeGui = Join-Path $testRoot 'fake-gui.exe'
    & $compiler /nologo /target:exe /warnaserror+ "/out:$fakeGui" $fixtureSource
    if ($LASTEXITCODE -ne 0) { throw 'The fake Windows GUI did not compile.' }
    $powerShellPath = (Get-Process -Id $PID).Path
    $scenarios = @(
        @{ Name = 'build_exit_1'; BuildExit = 1; ExpectedOrder = @('build'); ExpectSuccess = $false; ExpectedMessage = 'The Windows GUI build failed.' },
        @{ Name = 'capture_exit_1'; CaptureExit = 1; ExpectedOrder = @('build','capture'); ExpectSuccess = $false; ExpectedMessage = 'The Windows process-capture regression failed.' },
        @{ Name = 'self_test_exit_1_with_pass'; SelfTestExit = 1; ExpectedOrder = @('build','capture','selftest'); ExpectSuccess = $false; ExpectedMessage = 'Windows GUI unit tests failed.'; ExpectUnitPassArtifact = $true },
        @{ Name = 'self_test_missing_with_stale'; SelfTestMode = 'missing'; StaleUnit = $true; ExpectedOrder = @('build','capture','selftest'); ExpectSuccess = $false },
        @{ Name = 'self_test_failed_artifact'; SelfTestMode = 'failed'; ExpectedOrder = @('build','capture','selftest'); ExpectSuccess = $false; ExpectedMessage = 'Windows GUI unit evidence did not report success.' },
        @{ Name = 'self_test_malformed_artifact'; SelfTestMode = 'malformed'; ExpectedOrder = @('build','capture','selftest'); ExpectSuccess = $false },
        @{ Name = 'self_test_wrong_count'; SelfTestMode = 'wrong_count'; ExpectedOrder = @('build','capture','selftest'); ExpectSuccess = $false; ExpectedMessage = 'tests instead of 14.' },
        @{ Name = 'e2e_exit_1_with_pass'; E2eExit = 1; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The Windows GUI E2E journey failed.'; ExpectGuiPassArtifact = $true },
        @{ Name = 'e2e_missing_with_stale'; E2eMode = 'missing'; StaleGui = $true; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The Windows GUI E2E journey did not emit fresh completion evidence.'; ExpectFreshGuiMissing = $true },
        @{ Name = 'e2e_failed_artifact'; E2eMode = 'failed'; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The Windows GUI E2E evidence did not report success.' },
        @{ Name = 'e2e_malformed_artifact'; E2eMode = 'malformed'; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false },
        @{ Name = 'live_missing_run'; E2eMode = 'live_missing_run'; LiveDemo = $true; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The extended Windows GUI journey did not prove its exact saved roles, completed demo and all twelve acceptance tests.' },
        @{ Name = 'live_wrong_implementation'; E2eMode = 'live_wrong_implementation'; LiveDemo = $true; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The extended Windows GUI journey did not prove its exact saved roles, completed demo and all twelve acceptance tests.' },
        @{ Name = 'live_wrong_implementation_effort'; E2eMode = 'live_wrong_implementation_effort'; LiveDemo = $true; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The extended Windows GUI journey did not prove its exact saved roles, completed demo and all twelve acceptance tests.' },
        @{ Name = 'live_wrong_orchestrator'; E2eMode = 'live_wrong_orchestrator'; LiveDemo = $true; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The extended Windows GUI journey did not prove its exact saved roles, completed demo and all twelve acceptance tests.' },
        @{ Name = 'live_wrong_orchestrator_effort'; E2eMode = 'live_wrong_orchestrator_effort'; LiveDemo = $true; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The extended Windows GUI journey did not prove its exact saved roles, completed demo and all twelve acceptance tests.' },
        @{ Name = 'live_wrong_reviewer'; E2eMode = 'live_wrong_reviewer'; LiveDemo = $true; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The extended Windows GUI journey did not prove its exact saved roles, completed demo and all twelve acceptance tests.' },
        @{ Name = 'live_wrong_reviewer_effort'; E2eMode = 'live_wrong_reviewer_effort'; LiveDemo = $true; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The extended Windows GUI journey did not prove its exact saved roles, completed demo and all twelve acceptance tests.' },
        @{ Name = 'live_wrong_attempts'; E2eMode = 'live_wrong_attempts'; LiveDemo = $true; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The extended Windows GUI journey did not prove its exact saved roles, completed demo and all twelve acceptance tests.' },
        @{ Name = 'live_wrong_timeout'; E2eMode = 'live_wrong_timeout'; LiveDemo = $true; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The extended Windows GUI journey did not prove its exact saved roles, completed demo and all twelve acceptance tests.' },
        @{ Name = 'live_wrong_acceptance_count'; E2eMode = 'live_wrong_acceptance'; LiveDemo = $true; ExpectedOrder = @('build','capture','selftest','e2e'); ExpectSuccess = $false; ExpectedMessage = 'The extended Windows GUI journey did not prove its exact saved roles, completed demo and all twelve acceptance tests.' },
        @{ Name = 'live_complete_fresh_proof'; LiveDemo = $true; ExpectedOrder = @('build','capture','selftest','e2e','cargo','docs','git'); ExpectSuccess = $true; ExpectUnitPassArtifact = $true; ExpectGuiPassArtifact = $true },
        @{ Name = 'normal_complete_fresh_proof'; LiveDemo = $false; ExpectedOrder = @('build','capture','selftest','e2e','cargo','docs','git'); ExpectSuccess = $true; ExpectUnitPassArtifact = $true; ExpectGuiPassArtifact = $true }
    )
    foreach ($scenario in $scenarios) { Run-Scenario $scenario $fakeGui $powerShellPath }
} catch {
    $failure = $_
} finally {
    foreach ($name in $scenarioEnvironmentNames) {
        if ($null -eq $priorEnvironment[$name]) { [System.Environment]::SetEnvironmentVariable($name, $null, 'Process') }
        else { [System.Environment]::SetEnvironmentVariable($name, [string]$priorEnvironment[$name], 'Process') }
    }
    $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
    $temporaryPrefix = $temporaryRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $resolvedTestRoot.StartsWith($temporaryPrefix, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not (Split-Path -Leaf $resolvedTestRoot).StartsWith('assemblywright-windows-gui-gate-', [System.StringComparison]::Ordinal)) {
        throw 'Refusing to remove a GUI gate test directory outside the intended temporary root.'
    }
    if (Test-Path -LiteralPath $resolvedTestRoot) { Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force }
}

$evidence = [ordered]@{
    schema_version = 1
    status = if ($failure) { 'failed' } else { 'passed' }
    powershell_edition = $PSVersionTable.PSEdition
    powershell_version = $PSVersionTable.PSVersion.ToString()
    scenarios = $results
    failure = if ($failure) { $failure.Exception.Message } else { $null }
}
Write-Utf8File $evidencePath ($evidence | ConvertTo-Json -Depth 6)
if ($failure) { throw $failure }
Write-Host "Windows GUI gate regression passed: $evidencePath"
