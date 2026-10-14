[CmdletBinding()]
param([switch]$NoBuild)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$bundle = Join-Path $repositoryRoot 'target/windows-gui'
$installDir = Join-Path $env:LOCALAPPDATA 'Assemblywright/WindowsApp'
if (-not $NoBuild) {
    & (Join-Path $PSScriptRoot 'build-windows-gui.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'The Windows GUI build failed.' }
}
if (-not (Test-Path -LiteralPath (Join-Path $bundle 'Assemblywright.exe'))) {
    throw 'Build the Windows GUI before installing it.'
}

New-Item -ItemType Directory -Path $installDir -Force | Out-Null
# Install only runtime files, preserving local settings and unrelated files.
foreach ($bundleFile in @('Assemblywright.exe', 'assemblywright-local.exe', 'assemblywright-github.exe', 'windows-local-pipeline.ps1')) {
    $sourceFile = Join-Path $bundle $bundleFile
    if (-not (Test-Path -LiteralPath $sourceFile -PathType Leaf)) { throw "The GUI bundle is missing $bundleFile." }
    Copy-Item -LiteralPath $sourceFile -Destination (Join-Path $installDir $bundleFile) -Force
}
$installedExe = Join-Path $installDir 'Assemblywright.exe'
$shortcutShell = New-Object -ComObject WScript.Shell
$desktop = [Environment]::GetFolderPath('Desktop')
$programs = [Environment]::GetFolderPath('Programs')
foreach ($shortcutDirectory in @($desktop, $programs)) {
    if (-not $shortcutDirectory) { continue }
    New-Item -ItemType Directory -Path $shortcutDirectory -Force | Out-Null
    $shortcut = $shortcutShell.CreateShortcut((Join-Path $shortcutDirectory 'Assemblywright.lnk'))
    $shortcut.TargetPath = $installedExe
    $shortcut.WorkingDirectory = $installDir
    $shortcut.Description = 'Assemblywright - Windows local feature pipeline'
    $shortcut.IconLocation = "$installedExe,0"
    $shortcut.Save()
}
Write-Host "Installed: $installedExe"
Write-Host 'Assemblywright shortcuts are available on the Desktop and in Start.'
