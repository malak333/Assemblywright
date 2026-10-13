[CmdletBinding()]
param(
    [ValidateSet('run', 'status', 'pause', 'resume', 'cancel')]
    [string]$Action = 'run',
    [string]$Job,
    [switch]$Demo,
    [string]$StateDir = (Join-Path $env:LOCALAPPDATA 'Assemblywright/local-pipeline'),
    [string]$RunnerPath,
    [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$binary = Join-Path $repositoryRoot 'target/debug/assemblywright-local.exe'
if ($RunnerPath) {
    $binary = [System.IO.Path]::GetFullPath($RunnerPath)
}
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Write-Utf8File([string]$Path, [string]$Content) {
    [System.IO.File]::WriteAllText($Path, $Content, $utf8)
}

function Invoke-CheckedGit([string]$Directory, [string[]]$Arguments) {
    & git -C $Directory @Arguments
    if ($LASTEXITCODE -ne 0) { throw 'Cannot prepare the local pipeline demonstration repository.' }
}

if ($Demo -and ($Job -or $Action -ne 'run')) {
    throw '-Demo is a run action and cannot be combined with -Job.'
}
if ($Action -eq 'run' -and -not $Demo -and -not $Job) {
    throw 'Supply -Job path/to/job.json or -Demo.'
}

if (-not $NoBuild) {
    Push-Location $repositoryRoot
    try {
        & cargo build --locked -p assemblywright-master --bin assemblywright-local
        if ($LASTEXITCODE -ne 0) { throw 'The Windows local pipeline build failed.' }
    } finally { Pop-Location }
}
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw 'The local runner is missing. Run this launcher without -NoBuild.'
}

if ($Demo) {
    $python = (Get-Command python -ErrorAction Stop).Source
    $demoRoot = Join-Path $repositoryRoot ('target/windows-local-demo/' + [guid]::NewGuid().ToString('N'))
    $sourceRoot = Join-Path $demoRoot 'source'
    New-Item -ItemType Directory -Path (Join-Path $sourceRoot 'src'), (Join-Path $sourceRoot 'tests') -Force | Out-Null
    Write-Utf8File (Join-Path $sourceRoot '.gitignore') "__pycache__/`n*.pyc`n"
    Write-Utf8File (Join-Path $sourceRoot 'src/slugify.py') @'
def slugify(text, max_length=80):
    raise NotImplementedError("Assemblywright must implement this feature")
'@
    Write-Utf8File (Join-Path $sourceRoot 'src/cli.py') @'
raise NotImplementedError("Assemblywright must implement the command line interface")
'@
    Write-Utf8File (Join-Path $sourceRoot 'tests/test_slugify.py') @'
import os
import subprocess
import sys
import unittest

sys.path.insert(0, os.path.abspath("src"))
from slugify import slugify


class SlugifyTests(unittest.TestCase):
    def test_words(self):
        self.assertEqual(slugify("Hello, Assemblywright!"), "hello-assemblywright")

    def test_unicode(self):
        self.assertEqual(slugify("Cr\u00e8me br\u00fbl\u00e9e d\u00e9j\u00e0 vu"), "creme-brulee-deja-vu")

    def test_separator_runs(self):
        self.assertEqual(slugify("  One___two --- THREE  "), "one-two-three")

    def test_empty(self):
        self.assertEqual(slugify(""), "")
        self.assertEqual(slugify("!!!"), "")

    def test_limit(self):
        self.assertEqual(slugify("alpha beta gamma", 10), "alpha-beta")
        self.assertEqual(slugify("alpha beta", 6), "alpha")
        self.assertEqual(slugify("alpha", 1), "a")

    def test_default_limit(self):
        self.assertEqual(len(slugify("a" * 100)), 80)

    def test_digits(self):
        self.assertEqual(slugify("Build 42.7"), "build-42-7")

    def test_bad_text(self):
        for value in [None, 42, ["a"]]:
            with self.subTest(value=value), self.assertRaises(TypeError):
                slugify(value)

    def test_bad_limits(self):
        for value in [0, -1, 1.5, "5", True, None]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                slugify("abc", value)

    def test_cli(self):
        result = subprocess.run([sys.executable, "src/cli.py", "Hello World"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "hello-world")

    def test_cli_limit(self):
        result = subprocess.run([sys.executable, "src/cli.py", "alpha beta", "--max-length", "6"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "alpha")

    def test_cli_invalid_limit(self):
        result = subprocess.run([sys.executable, "src/cli.py", "text", "--max-length", "0"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)


if __name__ == "__main__":
    unittest.main()
'@
    Invoke-CheckedGit $sourceRoot @('init', '--quiet', '--initial-branch=main')
    Invoke-CheckedGit $sourceRoot @('add', '--', '.')
    Invoke-CheckedGit $sourceRoot @('-c', 'user.name=Assemblywright Demo', '-c', 'user.email=demo@localhost', '-c', 'core.hooksPath=NUL', 'commit', '--quiet', '-m', 'Seed unimplemented local pipeline feature')
    $Job = Join-Path $demoRoot 'job.json'
    $demoJob = [ordered]@{
        repository = $sourceRoot
        specification = 'Implement src/slugify.py and src/cli.py using only the Python standard library. slugify(text, max_length=80) requires a string (otherwise TypeError) and a positive integer max_length (bool and all invalid limits raise ValueError). Normalize Unicode with NFKD and ASCII transliteration, lowercase, replace each run of non-alphanumeric ASCII characters with one hyphen, strip leading/trailing hyphens, truncate to max_length then strip trailing hyphens. Empty or punctuation-only input returns an empty string. The CLI takes a positional text and optional --max-length integer, prints the slug, and uses argparse exit code 2 for invalid limits. Do not change tests. Finish both the library and CLI.'
        allowed_files = @('src/slugify.py', 'src/cli.py')
        validation = @{ program = $python; args = @('-m', 'unittest', 'discover', '-s', 'tests', '-v'); timeout_seconds = 60 }
        endpoint = 'http://127.0.0.1:8080/v1'
        model = 'qwen3.6-35b-a3b'
        max_attempts = 3
    }
    Write-Utf8File $Job ($demoJob | ConvertTo-Json -Depth 6)
    Write-Host "Demonstration source: $sourceRoot"
    Write-Host "Job: $Job"
}

if ($Action -eq 'run') {
    $resolvedJob = (Resolve-Path -LiteralPath $Job).Path
    & $binary run --job $resolvedJob --state-dir $StateDir
} else {
    & $binary $Action --state-dir $StateDir
}
exit $LASTEXITCODE
