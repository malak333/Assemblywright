[CmdletBinding()]
param(
    [switch]$SkipCargo
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$sourceRoot = Join-Path $repositoryRoot 'apps/windows'
$bundleRoot = Join-Path $repositoryRoot 'target/windows-gui'
$runnerSource = Join-Path $repositoryRoot 'target/debug/assemblywright-local.exe'
$githubSource = Join-Path $repositoryRoot 'target/debug/assemblywright-github.exe'
$launcherSource = Join-Path $repositoryRoot 'scripts/windows-local-pipeline.ps1'
$compilerCandidates = @(
    (Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'),
    (Join-Path $env:WINDIR 'Microsoft.NET/Framework/v4.0.30319/csc.exe')
)
$compiler = $compilerCandidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (-not $compiler) {
    throw 'The Windows .NET Framework C# compiler was not found.'
}

if (-not $SkipCargo) {
    Push-Location $repositoryRoot
    try {
        & cargo build --locked -p assemblywright-master --bin assemblywright-local --bin assemblywright-github
        if ($LASTEXITCODE -ne 0) { throw 'The native local runner build failed.' }
    } finally {
        Pop-Location
    }
}
if (-not (Test-Path -LiteralPath $runnerSource -PathType Leaf)) {
    throw 'The native local runner is missing. Build without -SkipCargo first.'
}
if (-not (Test-Path -LiteralPath $githubSource -PathType Leaf)) {
    throw 'The native GitHub adapter is missing. Build without -SkipCargo first.'
}
if (-not (Test-Path -LiteralPath $launcherSource -PathType Leaf)) {
    throw 'The Windows local pipeline launcher is missing.'
}
if (-not (Select-String -LiteralPath $launcherSource -SimpleMatch '[string]$RunnerPath,' -Quiet)) {
    throw 'The Windows local pipeline launcher does not support the packaged -RunnerPath contract.'
}

New-Item -ItemType Directory -Path $bundleRoot -Force | Out-Null
$sources = Get-ChildItem -LiteralPath $sourceRoot -Filter '*.cs' -File | Sort-Object Name | Select-Object -ExpandProperty FullName
if (-not $sources) { throw 'No Windows GUI sources were found.' }

$output = Join-Path $bundleRoot 'Assemblywright.exe'
$references = @(
    '/reference:System.dll',
    '/reference:System.Core.dll',
    '/reference:System.Drawing.dll',
    '/reference:System.Windows.Forms.dll',
    '/reference:System.Web.Extensions.dll'
)
& $compiler /nologo /target:winexe /platform:anycpu /langversion:5 /optimize+ /warnaserror+ "/out:$output" @references @sources
if ($LASTEXITCODE -ne 0) { throw 'The Assemblywright Windows GUI build failed.' }

Copy-Item -LiteralPath $runnerSource -Destination (Join-Path $bundleRoot 'assemblywright-local.exe') -Force
Copy-Item -LiteralPath $githubSource -Destination (Join-Path $bundleRoot 'assemblywright-github.exe') -Force
Copy-Item -LiteralPath $launcherSource -Destination (Join-Path $bundleRoot 'windows-local-pipeline.ps1') -Force

$items = Get-ChildItem -LiteralPath $bundleRoot -File | Sort-Object Name | Select-Object Name, Length
$items | Format-Table -AutoSize
Write-Host "Windows GUI bundle: $bundleRoot"
