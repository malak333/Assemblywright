[CmdletBinding()]
param(
    [ValidateSet('Install','Start','Status','Stop','Smoke')][string]$Action = 'Start',
    [ValidatePattern('^[A-Za-z]:\\[A-Za-z0-9_.\\-]+$')][string]$Root = 'C:\a\local-ai-windows\runtime',
    [ValidateRange(1024,65535)][int]$Port = 18081,
    [switch]$Confirm
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$modelAlias = 'windows-coder'
$runtimeRelease = 'b10516'
$runtimeArchive = "llama-$runtimeRelease-bin-win-cuda-12.4-x64.zip"
$cudaArchive = 'cudart-llama-bin-win-cuda-12.4-x64.zip'
$modelFile = 'Qwen3.5-9B-Q4_K_M.gguf'
$runtimeSha256 = '96d64faeb5b8e655341f32b26ad3e51fbea8bff0bc8120ad3dbffdc0b05b8ad3'
$cudaSha256 = '8c79a9b226de4b3cacfd1f83d24f962d0773be79f1e7b75c6af4ded7e32ae1d6'
$modelRevision = '3885219b6810b007914f3a7950a8d1b469d598a5'
$modelSha256 = '03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8'
$runtimeUrl = "https://github.com/ggml-org/llama.cpp/releases/download/$runtimeRelease/$runtimeArchive"
$cudaUrl = "https://github.com/ggml-org/llama.cpp/releases/download/$runtimeRelease/$cudaArchive"
$modelUrl = "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/$modelRevision/$modelFile"

$rootPath = [IO.Path]::GetFullPath($Root).TrimEnd('\')
$binary = Join-Path $rootPath 'bin\llama-server.exe'
$model = Join-Path $rootPath "models\$modelFile"
$pidFile = Join-Path $rootPath 'run\server.pid'
$stderrLog = Join-Path $rootPath 'logs\server.stderr.log'
$baseUrl = "http://127.0.0.1:$Port"

function Get-ServerArguments {
    return @(
        '--model', $model,
        '--host', '127.0.0.1',
        '--port', [string]$Port,
        '--alias', $modelAlias,
        '--ctx-size', '262144',
        '--parallel', '1',
        '--n-gpu-layers', '99',
        '--flash-attn', 'on',
        '--cache-type-k', 'q4_0',
        '--cache-type-v', 'q4_0',
        '--threads', '12',
        '--threads-batch', '12',
        '--batch-size', '512',
        '--ubatch-size', '256',
        '--jinja',
        '--no-webui',
        '--log-file', $stderrLog,
        '--log-prefix'
    )
}

function Get-ExpectedCommandLine {
    return $binary + ' ' + ((Get-ServerArguments) -join ' ')
}

function Write-Proof([string]$Status, [hashtable]$Extra = @{}) {
    $proof = [ordered]@{
        status = $Status
        endpoint = "$baseUrl/v1"
        model = $modelAlias
        root = $rootPath
        loopback_only = $true
        runtime_release = $runtimeRelease
    }
    foreach ($entry in $Extra.GetEnumerator()) { $proof[$entry.Key] = $entry.Value }
    $proof | ConvertTo-Json -Compress
}

function Assert-Hash([string]$Path, [string]$Expected, [string]$Label) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "$Label is unavailable." }
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -cne $Expected) { throw "$Label failed its published SHA-256 check." }
}

function Download-File([string]$Url, [string]$Destination) {
    & curl.exe --location --fail --retry 3 --retry-delay 2 --silent --show-error --output $Destination $Url
    if ($LASTEXITCODE -ne 0) { throw 'A portable model component download failed.' }
}

function Download-SegmentedModel([string]$Url, [string]$Destination) {
    [long]$totalBytes = 5680522464
    [int]$segmentCount = 8
    [long]$segmentSize = [math]::Ceiling($totalBytes / $segmentCount)
    $processes = @()
    $parts = @()
    $ranges = @()
    try {
        for ($index = 0; $index -lt $segmentCount; $index++) {
            [long]$first = $index * $segmentSize
            [long]$last = [math]::Min($totalBytes - 1, $first + $segmentSize - 1)
            $part = "$Destination.part$index"
            $header = "$part.headers"
            $parts += $part
            $ranges += @([pscustomobject]@{ First=$first; Last=$last; Part=$part; Header=$header })
            $processes += Start-Process -FilePath 'curl.exe' -ArgumentList @(
                '--location','--fail','--retry','3','--retry-delay','2','--silent','--show-error',
                '--range',"$first-$last",'--max-filesize',[string]($last-$first+1),
                '--dump-header',$header,'--output',$part,$Url
            ) -WindowStyle Hidden -PassThru
        }
        foreach ($process in $processes) {
            $process.WaitForExit()
            if ($process.ExitCode -ne 0) { throw 'A segmented model download failed.' }
        }
    } finally {
        foreach ($process in $processes) {
            if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
        }
        foreach ($process in $processes) {
            if (-not $process.HasExited) { $process.WaitForExit() }
        }
    }
    foreach ($range in $ranges) {
        [long]$expectedLength = $range.Last - $range.First + 1
        if ((Get-Item -LiteralPath $range.Part).Length -ne $expectedLength) {
            throw 'A model segment did not have its exact requested length.'
        }
        $headers = Get-Content -LiteralPath $range.Header -Raw
        $expectedRange = "content-range:\s*bytes\s+$($range.First)-$($range.Last)/$totalBytes"
        if ($headers -notmatch '(?im)^HTTP/\S+\s+206\b' -or $headers -notmatch "(?im)^$expectedRange\s*$") {
            throw 'A model segment response did not prove its exact HTTP byte range.'
        }
    }
    $output = [IO.File]::Open($Destination,[IO.FileMode]::Create,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try {
        foreach ($part in $parts) {
            $input = [IO.File]::OpenRead($part)
            try { $input.CopyTo($output) } finally { $input.Dispose() }
        }
    } finally {
        $output.Dispose()
    }
    foreach ($range in $ranges) { Remove-Item -LiteralPath $range.Part,$range.Header -Force }
}

function Read-ManagedProcess {
    if (-not (Test-Path -LiteralPath $pidFile -PathType Leaf)) { return $null }
    $stored = (Get-Content -LiteralPath $pidFile -Raw).Trim()
    if ($stored -notmatch '^[0-9]+$') { throw 'The Windows model PID file is malformed.' }
    $process = Get-CimInstance Win32_Process -Filter "ProcessId = $stored" -ErrorAction SilentlyContinue
    if ($null -eq $process) { return $null }
    $expected = [IO.Path]::GetFullPath($binary)
    if ([IO.Path]::GetFullPath([string]$process.ExecutablePath) -cne $expected) {
        throw 'The recorded PID belongs to a different executable.'
    }
    if ([string]$process.CommandLine -cne (Get-ExpectedCommandLine)) {
        throw 'The recorded PID belongs to a different Windows model launch configuration.'
    }
    return $process
}

function Read-Health {
    try {
        $health = Invoke-RestMethod -Method Get -Uri "$baseUrl/health" -TimeoutSec 3
        $models = Invoke-RestMethod -Method Get -Uri "$baseUrl/v1/models" -TimeoutSec 3
        if (-not ($models.data | Where-Object { $_.id -ceq $modelAlias })) {
            throw 'The listener does not advertise the configured Windows model alias.'
        }
        return $health
    } catch {
        return $null
    }
}

function Test-LoopbackListener {
    $client = [Net.Sockets.TcpClient]::new()
    try {
        $client.Connect('127.0.0.1',$Port)
        return $true
    } catch {
        return $false
    } finally {
        $client.Dispose()
    }
}

function Read-LoopbackListenerPid {
    $listeners = @(Get-NetTCPConnection -LocalAddress '127.0.0.1' -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
    if ($listeners.Count -eq 0) { return $null }
    $owners = @($listeners | Select-Object -ExpandProperty OwningProcess -Unique)
    if ($owners.Count -ne 1) { throw 'The loopback port has ambiguous listener ownership.' }
    return [int]$owners[0]
}

if ($Action -ceq 'Install') {
    if (-not $Confirm) { throw 'Installing the portable Windows model requires -Confirm.' }
    if (Test-Path -LiteralPath $rootPath) { throw 'The Windows model root already exists; installation will not overwrite it.' }
    $parent = Split-Path -Parent $rootPath
    $stage = "$rootPath.setup"
    if (Test-Path -LiteralPath $stage) { throw 'The Windows model setup staging path already exists.' }
    New-Item -ItemType Directory -Path $parent,$stage,(Join-Path $stage 'downloads'),(Join-Path $stage 'bin'),(Join-Path $stage 'models'),(Join-Path $stage 'run'),(Join-Path $stage 'logs') -Force | Out-Null
    try {
        $runtimeDownload = Join-Path $stage "downloads\$runtimeArchive"
        $cudaDownload = Join-Path $stage "downloads\$cudaArchive"
        $modelDownload = Join-Path $stage "downloads\$modelFile"
        Download-File $runtimeUrl $runtimeDownload
        Download-File $cudaUrl $cudaDownload
        Download-SegmentedModel $modelUrl $modelDownload
        Assert-Hash $runtimeDownload $runtimeSha256 'The llama.cpp CUDA archive'
        Assert-Hash $cudaDownload $cudaSha256 'The CUDA runtime archive'
        Assert-Hash $modelDownload $modelSha256 'The Qwen model'
        Expand-Archive -LiteralPath $runtimeDownload -DestinationPath (Join-Path $stage 'bin') -Force
        Expand-Archive -LiteralPath $cudaDownload -DestinationPath (Join-Path $stage 'bin') -Force
        Move-Item -LiteralPath $modelDownload -Destination (Join-Path $stage "models\$modelFile")
        if (-not (Test-Path -LiteralPath (Join-Path $stage 'bin\llama-server.exe') -PathType Leaf)) {
            throw 'The verified llama.cpp archive did not contain llama-server.exe.'
        }
        Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $stage 'start.ps1')
        [ordered]@{
            schema_version = 1
            endpoint = "$baseUrl/v1"
            model = $modelAlias
            model_revision = $modelRevision
            runtime_release = $runtimeRelease
            runtime_archive_sha256 = $runtimeSha256
            cuda_archive_sha256 = $cudaSha256
            model_sha256 = $modelSha256
            context_size = 262144
            gpu_layers = 99
            parallel_slots = 1
        } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $stage 'manifest.json') -Encoding UTF8
        Remove-Item -LiteralPath (Join-Path $stage 'downloads') -Recurse -Force
        Move-Item -LiteralPath $stage -Destination $rootPath
    } catch {
        if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
        throw
    }
    Write-Proof 'installed' @{ model_sha256 = $modelSha256; runtime_archive_sha256 = $runtimeSha256; cuda_archive_sha256 = $cudaSha256 }
    exit 0
}

if (-not (Test-Path -LiteralPath $rootPath -PathType Container)) { throw 'The portable Windows model is not installed.' }

if ($Action -ceq 'Status') {
    $process = Read-ManagedProcess
    $health = Read-Health
    $listenerPid = Read-LoopbackListenerPid
    if ($null -eq $process -and $null -ne $listenerPid) { Write-Proof 'untracked_listener'; exit 2 }
    if ($null -ne $process -and ($null -eq $health -or $listenerPid -ne [int]$process.ProcessId)) {
        Write-Proof 'unhealthy'; exit 2
    }
    if ($null -eq $process) { Write-Proof 'stopped'; exit 1 }
    Write-Proof 'ready' @{ pid = [int]$process.ProcessId }
    exit 0
}

if ($Action -ceq 'Stop') {
    $process = Read-ManagedProcess
    if ($null -eq $process -and (Test-LoopbackListener)) {
        throw 'The loopback port has an untracked listener; it will not be stopped.'
    }
    if ($null -ne $process) {
        Stop-Process -Id ([int]$process.ProcessId) -ErrorAction Stop
        Wait-Process -Id ([int]$process.ProcessId) -Timeout 15 -ErrorAction SilentlyContinue
    }
    if (Test-LoopbackListener) { throw 'The loopback listener remains after the managed process was stopped.' }
    if (Test-Path -LiteralPath $pidFile) { Remove-Item -LiteralPath $pidFile -Force }
    Write-Proof 'stopped'
    exit 0
}

if ($Action -ceq 'Smoke') {
    $process = Read-ManagedProcess
    $listenerPid = Read-LoopbackListenerPid
    if ($null -eq $process -or $listenerPid -ne [int]$process.ProcessId -or $null -eq (Read-Health)) {
        throw 'The Windows model is not ready.'
    }
    $body = @{
        model = $modelAlias
        messages = @(@{ role = 'user'; content = 'Reply with one line of Python that defines add(a, b) and returns their sum.' })
        temperature = 0
        max_tokens = 128
        chat_template_kwargs = @{ enable_thinking = $false }
    } | ConvertTo-Json -Depth 5
    $response = Invoke-RestMethod -Method Post -Uri "$baseUrl/v1/chat/completions" -ContentType 'application/json' -Body $body -TimeoutSec 120
    $content = [string]$response.choices[0].message.content
    if ([string]::IsNullOrWhiteSpace($content)) { throw 'The bounded Windows model inference returned no content.' }
    Write-Proof 'smoke_passed' @{ response_characters = $content.Length }
    exit 0
}

$existing = Read-ManagedProcess
$listenerPid = Read-LoopbackListenerPid
if ($null -ne $existing -and $listenerPid -eq [int]$existing.ProcessId -and $null -ne (Read-Health)) {
    Write-Proof 'ready' @{ pid = [int]$existing.ProcessId }
    exit 0
}
if ($null -ne $existing) { throw 'The managed Windows model process exists but is not healthy.' }
if (Test-LoopbackListener) { throw 'Port is occupied by an untracked listener.' }

Assert-Hash $model $modelSha256 'The Qwen model'
New-Item -ItemType Directory -Path (Split-Path -Parent $pidFile),(Split-Path -Parent $stderrLog) -Force | Out-Null
if (Test-Path -LiteralPath $stderrLog) { Remove-Item -LiteralPath $stderrLog -Force }
$commandLine = Get-ExpectedCommandLine
$created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $commandLine }
if ([int]$created.ReturnValue -ne 0 -or [int]$created.ProcessId -le 0) {
    throw 'Windows could not create the detached model process under the current account.'
}
$startedPid = [int]$created.ProcessId
[IO.File]::WriteAllText($pidFile,[string]$startedPid,[Text.Encoding]::ASCII)
for ($attempt = 0; $attempt -lt 120; $attempt++) {
    if ($null -ne (Read-Health)) { Write-Proof 'ready' @{ pid = $startedPid }; exit 0 }
    if ($null -eq (Read-ManagedProcess)) { throw 'The Windows model server exited during startup. Review the private stderr log.' }
    Start-Sleep -Milliseconds 500
}
throw 'The Windows model server did not become ready within 60 seconds.'
