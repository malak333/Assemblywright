[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$processSupport = Join-Path $repositoryRoot 'apps/windows/ProcessSupport.cs'
$fixtureSource = Join-Path $PSScriptRoot 'fixtures/windows_gui_fast_exit_fixture.cs'
$evidenceRoot = Join-Path $repositoryRoot 'target/windows-ci'
$temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$testRoot = [System.IO.Path]::GetFullPath((Join-Path $temporaryRoot ('assemblywright-process-capture-' + [guid]::NewGuid().ToString('N'))))

if (-not $testRoot.StartsWith($temporaryRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'The process-capture test root escaped the system temporary directory.'
}
foreach ($source in @($processSupport, $fixtureSource)) {
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Required process-capture source is missing: $source" }
}
$compilerCandidates = @(
    (Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'),
    (Join-Path $env:WINDIR 'Microsoft.NET/Framework/v4.0.30319/csc.exe')
)
$compiler = $compilerCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (-not $compiler) { throw 'The Windows .NET Framework C# compiler was not found.' }

New-Item -ItemType Directory -Path $testRoot, $evidenceRoot -Force | Out-Null
try {
    $fixtureExe = Join-Path $testRoot 'windows-process-capture-fixture.exe'
    & $compiler /nologo /target:exe /platform:anycpu /langversion:5 /optimize+ /warnaserror+ /reference:System.Core.dll "/out:$fixtureExe" $processSupport $fixtureSource
    if ($LASTEXITCODE -ne 0) { throw 'The process-capture fixture did not compile.' }

    $stdoutPath = Join-Path $testRoot 'stdout.txt'
    $stderrPath = Join-Path $testRoot 'stderr.txt'
    $fixture = Start-Process -FilePath $fixtureExe -WindowStyle Hidden -PassThru -Wait -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    try { $exitCode = $fixture.ExitCode } finally { $fixture.Dispose() }
    $stdout = if (Test-Path -LiteralPath $stdoutPath) { [System.IO.File]::ReadAllText($stdoutPath).Trim() } else { '' }
    $stderr = if (Test-Path -LiteralPath $stderrPath) { [System.IO.File]::ReadAllText($stderrPath).Trim() } else { '' }

    $stdoutLines = @($stdout -split "`r?`n" | Where-Object { $_.Length -ne 0 })
    $diagnosticLine = @($stdoutLines | Where-Object { $_ -like 'DIAGNOSTICS *' } | Select-Object -First 1)
    $negativeLine = @($stdoutLines | Where-Object { $_ -like 'NEGATIVE_CONTROL *' } | Select-Object -First 1)
    $passLine = 'PASS iterations=320 successful=160 failed=160 launch_failure=observed retained_handle_negative_control=rejected'
    $diagnosticPattern = '^DIAGNOSTICS stabilization=(?<samples>[0-9]+:[0-9]+(?:,[0-9]+:[0-9]+)*) baseline_handles=(?<baseline_handles>[0-9]+) baseline_threads=(?<baseline_threads>[0-9]+) final_handles=(?<final_handles>[0-9]+) final_threads=(?<final_threads>[0-9]+) tolerance=32$'
    $negativePattern = '^NEGATIVE_CONTROL baseline_handles=(?<baseline_handles>[0-9]+) retained_handles=(?<retained_handles>[0-9]+) released_handles=(?<released_handles>[0-9]+) rejected=(?<rejected>true|false) recovered=(?<recovered>true|false)$'
    $diagnosticMatch = if ($diagnosticLine.Count -eq 1) { [regex]::Match($diagnosticLine[0], $diagnosticPattern) } else { $null }
    $negativeMatch = if ($negativeLine.Count -eq 1) { [regex]::Match($negativeLine[0], $negativePattern) } else { $null }
    $stabilizationSamples = @()
    if ($null -ne $diagnosticMatch -and $diagnosticMatch.Success) {
        $stabilizationSamples = @($diagnosticMatch.Groups['samples'].Value -split ',' | ForEach-Object {
            $parts = $_ -split ':'
            [ordered]@{ handles = [int]$parts[0]; threads = [int]$parts[1] }
        })
    } elseif ($diagnosticLine.Count -eq 1 -and $diagnosticLine[0] -match '^DIAGNOSTICS stabilization=(?<samples>[0-9]+:[0-9]+(?:,[0-9]+:[0-9]+)*) stabilization_failed=true$') {
        $stabilizationSamples = @($Matches['samples'] -split ',' | ForEach-Object {
            $parts = $_ -split ':'
            [ordered]@{ handles = [int]$parts[0]; threads = [int]$parts[1] }
        })
    }

    $failure = $null
    if ($exitCode -ne 0) {
        $failure = "The process-capture fixture failed with exit code $exitCode`: $stderr"
    } elseif ($stdoutLines.Count -ne 3 -or $stdoutLines[2] -cne $passLine) {
        $failure = "The process-capture fixture returned unexpected output: $stdout"
    } elseif ($null -eq $diagnosticMatch -or -not $diagnosticMatch.Success) {
        $failure = 'The process-capture fixture omitted its bounded runtime diagnostics.'
    } elseif ($null -eq $negativeMatch -or -not $negativeMatch.Success -or
        $negativeMatch.Groups['rejected'].Value -cne 'true' -or $negativeMatch.Groups['recovered'].Value -cne 'true') {
        $failure = 'The process-capture fixture did not reject and release retained process handles.'
    } elseif ($stderr.Length -ne 0) {
        $failure = "The process-capture fixture emitted unexpected stderr: $stderr"
    }

    $stderrTail = if ($stderr.Length -le 65536) { $stderr } else { '[truncated]' + $stderr.Substring($stderr.Length - 65536) }
    $confirmedAssertions = @()
    if ($null -eq $failure) {
        $confirmedAssertions = @('stdout_lossless', 'stderr_lossless', 'exit_code_exact', 'fast_exit_completed', 'launch_failure_faulted', 'process_handles_released', 'runtime_stabilized', 'retained_process_handles_rejected')
    }

    $evidence = [ordered]@{
        schema_version = 2
        status = if ($null -eq $failure) { 'passed' } else { 'failed' }
        fixture_exit_code = $exitCode
        requested_iterations = 320
        requested_successful_children = 160
        requested_failing_children = 160
        iterations = if ($null -eq $failure) { 320 } else { $null }
        successful_children = if ($null -eq $failure) { 160 } else { $null }
        failing_children = if ($null -eq $failure) { 160 } else { $null }
        launch_failure = if ($null -eq $failure) { 'observed' } else { $null }
        handle_tolerance = 32
        stabilization_samples = $stabilizationSamples
        baseline_handles = if ($null -ne $diagnosticMatch -and $diagnosticMatch.Success) { [int]$diagnosticMatch.Groups['baseline_handles'].Value } else { $null }
        baseline_threads = if ($null -ne $diagnosticMatch -and $diagnosticMatch.Success) { [int]$diagnosticMatch.Groups['baseline_threads'].Value } else { $null }
        final_handles = if ($null -ne $diagnosticMatch -and $diagnosticMatch.Success) { [int]$diagnosticMatch.Groups['final_handles'].Value } else { $null }
        final_threads = if ($null -ne $diagnosticMatch -and $diagnosticMatch.Success) { [int]$diagnosticMatch.Groups['final_threads'].Value } else { $null }
        retained_handle_negative_control = if ($null -ne $negativeMatch -and $negativeMatch.Success) { [ordered]@{
            baseline_handles = [int]$negativeMatch.Groups['baseline_handles'].Value
            retained_handles = [int]$negativeMatch.Groups['retained_handles'].Value
            released_handles = [int]$negativeMatch.Groups['released_handles'].Value
            rejected = [bool]::Parse($negativeMatch.Groups['rejected'].Value)
            recovered = [bool]::Parse($negativeMatch.Groups['recovered'].Value)
        } } else { $null }
        stderr_tail = $stderrTail
        assertions = $confirmedAssertions
        process_support_sha256 = (Get-FileHash -LiteralPath $processSupport -Algorithm SHA256).Hash.ToLowerInvariant()
        fixture_sha256 = (Get-FileHash -LiteralPath $fixtureSource -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $evidencePath = Join-Path $evidenceRoot 'windows-process-capture.json'
    [System.IO.File]::WriteAllText($evidencePath, ($evidence | ConvertTo-Json -Depth 4), (New-Object System.Text.UTF8Encoding($false)))
    if ($null -ne $failure) { throw $failure }
    Write-Host "Windows process-capture regression passed: $evidencePath"
} finally {
    $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
    if (-not $resolvedTestRoot.StartsWith($temporaryRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing to remove a process-capture path outside the system temporary directory.'
    }
    if (Test-Path -LiteralPath $resolvedTestRoot) { Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force }
}
