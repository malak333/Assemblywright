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
    if ($exitCode -ne 0) { throw "The process-capture fixture failed with exit code $exitCode`: $stderr" }
    $expected = 'PASS iterations=320 successful=160 failed=160 launch_failure=observed'
    if ($stdout -ne $expected) { throw "The process-capture fixture returned unexpected output: $stdout" }
    if ($stderr.Length -ne 0) { throw "The process-capture fixture emitted unexpected stderr: $stderr" }

    $evidence = [ordered]@{
        schema_version = 1
        status = 'passed'
        iterations = 320
        successful_children = 160
        failing_children = 160
        launch_failure = 'observed'
        assertions = @('stdout_lossless', 'stderr_lossless', 'exit_code_exact', 'fast_exit_completed', 'launch_failure_faulted', 'process_handles_released')
        process_support_sha256 = (Get-FileHash -LiteralPath $processSupport -Algorithm SHA256).Hash.ToLowerInvariant()
        fixture_sha256 = (Get-FileHash -LiteralPath $fixtureSource -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $evidencePath = Join-Path $evidenceRoot 'windows-process-capture.json'
    [System.IO.File]::WriteAllText($evidencePath, ($evidence | ConvertTo-Json -Depth 4), (New-Object System.Text.UTF8Encoding($false)))
    Write-Host "Windows process-capture regression passed: $evidencePath"
} finally {
    $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
    if (-not $resolvedTestRoot.StartsWith($temporaryRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing to remove a process-capture path outside the system temporary directory.'
    }
    if (Test-Path -LiteralPath $resolvedTestRoot) { Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force }
}
