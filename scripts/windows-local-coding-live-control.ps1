param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Check", "Prepare", "ClaimAndDispatch", "Integrate", "Cancel", "Abandon", "Cleanup")]
    [string]$Action,

    [string]$DataDir = (Join-Path $env:LOCALAPPDATA "Assemblywright\master"),
    [ValidatePattern("^127\.0\.0\.1:[0-9]{1,5}$")]
    [string]$Endpoint = "127.0.0.1:7791",
    [string]$SourceRepository = "C:\Users\mike\Codex\Assemblywright",
    [string]$ProofRepository = "C:\Users\mike\Codex\Assemblywright-local-coding-live-proof",

    [UInt64]$OwnerControlDesignationRevision,
    [string]$RepositoryId,
    [string]$FeatureId,
    [string]$HeadCommit,
    [string]$LocalCodingDeviceId,
    [UInt64]$LocalCodingRegistryRevision,
    [UInt64]$ExpectedLifecycleRevision,
    [UInt64]$ExpectedQueueRevision,
    [UInt64]$ExpectedEmergencyPauseRevision,
    [string]$TaskId,
    [string]$StepId,
    [UInt64]$SucceededSequence,
    [ValidatePattern("^[0-9a-f]{64}$")]
    [string]$MacCleanupSha256,

    [switch]$ConfirmAction
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

foreach ($gitEnvironmentEntry in @(Get-ChildItem Env: | Where-Object { $_.Name -like "GIT_*" })) {
    Remove-Item -LiteralPath "Env:$($gitEnvironmentEntry.Name)"
}
$env:GIT_CONFIG_GLOBAL = "NUL"
$env:GIT_CONFIG_SYSTEM = "NUL"
$env:GIT_CONFIG_NOSYSTEM = "1"
$env:GIT_ATTR_NOSYSTEM = "1"
$env:GIT_TERMINAL_PROMPT = "0"
$env:GIT_OPTIONAL_LOCKS = "0"

$protocolVersion = 5
$masterSchemaVersion = 19
$featureConveyorProjectionSchemaVersion = 9
$ownerControlSchemaVersion = 1
$uuidPattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
$commitPattern = "^[0-9a-f]{40}$"
$proofLeaf = "Assemblywright-local-coding-live-proof"

function Get-Sha256Bytes {
    param([Parameter(Mandatory = $true)][string]$Text)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        return @($algorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text)))
    } finally {
        $algorithm.Dispose()
    }
}

function Get-Sha256FileBytes {
    param([Parameter(Mandatory = $true)][string]$Path)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        return @($algorithm.ComputeHash([IO.File]::ReadAllBytes($Path)))
    } finally {
        $algorithm.Dispose()
    }
}

function Get-GitBlobSha256Bytes {
    param(
        [Parameter(Mandatory = $true)][string]$Repository,
        [Parameter(Mandatory = $true)][string]$Commit,
        [Parameter(Mandatory = $true)][string]$BlobPath
    )
    if (
        $Commit -notmatch $commitPattern -or
        $BlobPath -cne "README.md" -or
        $Repository.Contains('"')
    ) {
        throw "The immutable Git blob binding was not exact."
    }
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = "git"
    $startInfo.Arguments = "--no-replace-objects -c core.fsmonitor=false -c core.hooksPath=NUL -C `"$Repository`" cat-file blob `"$Commit`:$BlobPath`""
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        $process.Dispose()
        throw "Git did not start for the immutable blob binding."
    }
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $digest = @($algorithm.ComputeHash($process.StandardOutput.BaseStream))
        $errorOutput = $process.StandardError.ReadToEnd()
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) {
            throw "Git rejected the immutable blob binding."
        }
        return $digest
    } finally {
        $algorithm.Dispose()
        if (-not $process.HasExited) {
            $process.Kill()
            $process.WaitForExit()
        }
        $process.Dispose()
    }
}

function Convert-BytesToHex {
    param([Parameter(Mandatory = $true)]$Bytes)
    return -join @($Bytes | ForEach-Object { ([byte]$_).ToString("x2") })
}

function Assert-Digest {
    param([Parameter(Mandatory = $true)]$Value, [Parameter(Mandatory = $true)][string]$Label)
    $bytes = @($Value)
    if ($bytes.Count -ne 32 -or -not ($bytes | Where-Object { [byte]$_ -ne 0 })) {
        throw "$Label was not an exact nonzero SHA-256 digest."
    }
}

function Assert-ExactKeys {
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string[]]$Keys,
        [Parameter(Mandatory = $true)][string]$Label
    )
    if ($Value -is [System.Collections.IDictionary]) {
        $actual = @($Value.Keys | ForEach-Object { [string]$_ } | Sort-Object -CaseSensitive)
    } else {
        $actual = @($Value.PSObject.Properties.Name | Sort-Object -CaseSensitive)
    }
    $expected = @($Keys | Sort-Object -CaseSensitive)
    if ($actual.Count -ne $expected.Count) {
        throw "$Label returned an unexpected JSON shape."
    }
    for ($index = 0; $index -lt $expected.Count; $index += 1) {
        if ($actual[$index] -cne $expected[$index]) {
            throw "$Label returned an unexpected JSON shape."
        }
    }
}

function Resolve-ProofPaths {
    $source = [IO.Path]::GetFullPath($SourceRepository)
    $proof = [IO.Path]::GetFullPath($ProofRepository)
    if (
        [IO.Path]::GetFileName($proof) -cne $proofLeaf -or
        $proof -eq $source -or
        [IO.Path]::GetPathRoot($proof) -eq $proof
    ) {
        throw "The disposable proof checkout path is not the exact bounded proof leaf."
    }
    return [ordered]@{ source = $source; proof = $proof }
}

function Assert-NoReparseComponents {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [bool]$AllowMissingLeaf = $false
    )
    $full = [IO.Path]::GetFullPath($Path)
    $root = [IO.Path]::GetPathRoot($full)
    $rootItem = Get-Item -LiteralPath $root -Force
    if (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "The bounded path root is a reparse point."
    }
    $parts = @($full.Substring($root.Length) -split "[\\/]" | Where-Object { $_.Length -gt 0 })
    $current = $root
    for ($index = 0; $index -lt $parts.Count; $index += 1) {
        $current = Join-Path $current $parts[$index]
        if (-not (Test-Path -LiteralPath $current)) {
            if ($AllowMissingLeaf -and $index -eq ($parts.Count - 1)) {
                return
            }
            throw "A bounded path component is missing."
        }
        $item = Get-Item -LiteralPath $current -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "A bounded path component is a reparse point."
        }
        if ($index -lt ($parts.Count - 1) -and -not $item.PSIsContainer) {
            throw "A bounded path component is not a directory."
        }
    }
}

function Assert-NoReparseTree {
    param([Parameter(Mandatory = $true)][string]$Root)
    Assert-NoReparseComponents $Root $false
    $pending = [System.Collections.Generic.Stack[string]]::new()
    $pending.Push([IO.Path]::GetFullPath($Root))
    while ($pending.Count -gt 0) {
        $directory = $pending.Pop()
        foreach ($item in @(Get-ChildItem -LiteralPath $directory -Force)) {
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "The disposable checkout contains a reparse entry."
            }
            if ($item.PSIsContainer) {
                $pending.Push($item.FullName)
            }
        }
    }
}

function Remove-BoundedCommitGraphCache {
    param([Parameter(Mandatory = $true)][string]$Repository)
    $cache = Join-Path $Repository ".git\objects\info\commit-graphs"
    if (-not (Test-Path -LiteralPath $cache)) {
        return
    }
    Assert-NoReparseComponents $cache $false
    $cacheItem = Get-Item -LiteralPath $cache -Force
    if (-not $cacheItem.PSIsContainer) {
        throw "The disposable commit-graph cache was not a directory."
    }
    foreach ($entry in @(Get-ChildItem -LiteralPath $cache -Force)) {
        if (
            $entry.PSIsContainer -or
            ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            ($entry.Name -cne "commit-graph-chain" -and
                $entry.Name -cnotmatch "^graph-([0-9a-f]{40}|[0-9a-f]{64})\.graph$")
        ) {
            throw "The disposable commit-graph cache contained an unsupported entry."
        }
        Remove-Item -LiteralPath $entry.FullName -Force
    }
    Remove-Item -LiteralPath $cache -Force
    if (Test-Path -LiteralPath $cache) {
        throw "The disposable commit-graph cache was not removed."
    }
}

function Assert-SnapshotCompatibleObjectStore {
    param([Parameter(Mandatory = $true)][string]$Repository)
    $objects = Join-Path $Repository ".git\objects"
    Assert-NoReparseComponents $objects $false
    foreach ($directory in @(Get-ChildItem -LiteralPath $objects -Force)) {
        if (
            -not $directory.PSIsContainer -or
            ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0
        ) {
            throw "The disposable Git object store contained an unsupported root entry."
        }
        foreach ($entry in @(Get-ChildItem -LiteralPath $directory.FullName -Force)) {
            if (
                $entry.PSIsContainer -or
                ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0
            ) {
                throw "The disposable Git object store was not snapshot-compatible."
            }
        }
    }
}

function Invoke-Git {
    param([Parameter(Mandatory = $true)][string]$Repository, [Parameter(Mandatory = $true)][string[]]$Arguments)
    $output = @(& git --no-replace-objects -c core.fsmonitor=false -c core.hooksPath=NUL -C $Repository @Arguments 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "Git rejected the bounded live-proof operation."
    }
    return ($output -join "`n").Trim()
}

function Assert-SourceRepositoryEligible {
    param([Parameter(Mandatory = $true)][string]$Repository)
    $head = Invoke-Git $Repository @("rev-parse", "refs/heads/main")
    $originMain = Invoke-Git $Repository @("rev-parse", "refs/remotes/origin/main")
    $branch = Invoke-Git $Repository @("branch", "--show-current")
    $status = Invoke-Git $Repository @("status", "--porcelain=v1", "--untracked-files=all")
    $tracked = Invoke-Git $Repository @("ls-files", "-v", "--")
    $trackedLines = @($tracked -split "`r?`n" | Where-Object { $_.Length -gt 0 })
    if (
        $head -notmatch $commitPattern -or
        $head -cne $originMain -or
        $branch -cne "main" -or
        $status.Length -ne 0 -or
        $trackedLines.Count -eq 0 -or
        @($trackedLines | Where-Object { $_ -cnotmatch "^H " }).Count -ne 0
    ) {
        throw "The Windows source checkout is not exact clean main at origin/main with normal tracked-index state."
    }
    return $head
}

function Invoke-ExactPost {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)]$Body)
    $parameters = @{
        Method = "Post"
        Uri = "$script:baseUri$Path"
        Headers = $script:headers
        ContentType = "application/json"
        Body = ($Body | ConvertTo-Json -Compress -Depth 12)
    }
    return Invoke-RestMethod @parameters
}

function Invoke-ExactGet {
    param([Parameter(Mandatory = $true)][string]$Path)
    return Invoke-RestMethod -Method Get -Uri "$script:baseUri$Path" -Headers $script:headers
}

function Get-ConveyorStatus {
    $status = Invoke-ExactGet -Path "/v1/feature-conveyor/status"
    Assert-ExactKeys $status @(
        "schema_version", "queue_revision", "startup_quarantine_count", "counts_by_status",
        "visible_feature_count", "features_truncated", "features", "owner_guidance"
    ) "Feature Conveyor status"
    if (
        [UInt64]$status.schema_version -ne $featureConveyorProjectionSchemaVersion -or
        [UInt64]$status.queue_revision -ne [UInt64]$status.owner_guidance.queue_revision -or
        [UInt64]$status.owner_guidance.emergency_pause_revision -lt 0
    ) {
        throw "The Feature Conveyor status binding was invalid."
    }
    return $status
}

function Get-MasterHealth {
    $health = Invoke-ExactGet -Path "/health"
    if (
        [UInt64]$health.protocol_version -ne $protocolVersion -or
        [UInt64]$health.schema_version -ne $masterSchemaVersion -or
        $health.state.queued_steps -ne 0 -or
        $health.state.leased_steps -ne 0 -or
        $health.state.active_attempts -ne 0
    ) {
        throw "The Windows master retained active distributed work."
    }
    return $health
}

function Read-ProofMarker {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedSource
    )
    Assert-NoReparseComponents $ExpectedSource $false
    Assert-NoReparseComponents $Path $false
    Assert-NoReparseComponents (Join-Path $Path ".git") $false
    $markerPath = Join-Path $Path ".git\assemblywright-local-coding-live-proof"
    Assert-NoReparseComponents $markerPath $false
    if (-not (Test-Path -LiteralPath $markerPath -PathType Leaf)) {
        throw "The disposable checkout marker was absent."
    }
    $markerItem = Get-Item -LiteralPath $markerPath -Force
    if ($markerItem.Length -eq 0 -or $markerItem.Length -gt 4096) {
        throw "The disposable checkout marker was empty or oversized."
    }
    $marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json
    Assert-ExactKeys $marker @(
        "schema_version", "status", "source_repository", "proof_repository",
        "repository_id", "feature_id", "head_commit", "queue_revision",
        "emergency_pause_revision", "owner_control_designation_revision"
    ) "Disposable checkout marker"
    if (
        [UInt64]$marker.schema_version -ne 2 -or
        $marker.status -ne "local_coding_disposable_checkout" -or
        $marker.source_repository -cne $ExpectedSource -or
        $marker.proof_repository -cne $Path -or
        $marker.repository_id -notmatch $uuidPattern -or
        $marker.feature_id -notmatch $uuidPattern -or
        $marker.head_commit -notmatch $commitPattern -or
        [UInt64]$marker.owner_control_designation_revision -eq 0
    ) {
        throw "The disposable checkout marker binding drifted."
    }
    $sourceHead = Assert-SourceRepositoryEligible $ExpectedSource
    if ($sourceHead -ne $marker.head_commit) {
        throw "The marker-bound source main identity drifted."
    }
    return $marker
}

function Write-ProofMarkerAtomically {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Document
    )
    $gitDirectory = Join-Path $Path ".git"
    Assert-NoReparseComponents $gitDirectory $false
    Assert-ExactKeys $Document @(
        "schema_version", "status", "source_repository", "proof_repository",
        "repository_id", "feature_id", "head_commit", "queue_revision",
        "emergency_pause_revision", "owner_control_designation_revision"
    ) "Disposable checkout marker publication"
    $markerPath = Join-Path $gitDirectory "assemblywright-local-coding-live-proof"
    if (Test-Path -LiteralPath $markerPath) {
        throw "The disposable checkout marker publication target already exists."
    }
    $markerJson = $Document | ConvertTo-Json -Compress
    $markerBytes = [Text.UTF8Encoding]::new($false).GetBytes($markerJson)
    if ($markerBytes.Length -eq 0 -or $markerBytes.Length -gt 4096) {
        throw "The disposable checkout marker publication was empty or oversized."
    }
    $temporaryPath = Join-Path $gitDirectory ".assemblywright-local-coding-live-proof-$([Guid]::NewGuid().ToString('N')).tmp"
    $stream = $null
    try {
        $stream = [IO.FileStream]::new(
            $temporaryPath,
            [IO.FileMode]::CreateNew,
            [IO.FileAccess]::Write,
            [IO.FileShare]::None,
            4096,
            [IO.FileOptions]::WriteThrough
        )
        $stream.Write($markerBytes, 0, $markerBytes.Length)
        $stream.Flush($true)
        $stream.Dispose()
        $stream = $null
        if ([IO.File]::ReadAllText($temporaryPath) -cne $markerJson) {
            throw "The disposable checkout marker temporary bytes drifted."
        }
        [IO.File]::Move($temporaryPath, $markerPath)
        if ([IO.File]::ReadAllText($markerPath) -cne $markerJson) {
            throw "The atomically published disposable checkout marker drifted."
        }
    } finally {
        if ($null -ne $stream) { $stream.Dispose() }
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath -Force
        }
    }
}

function Assert-ProofRepositoryClean {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedSource,
        [Parameter(Mandatory = $true)][string]$ExpectedRepositoryId,
        [Parameter(Mandatory = $true)][string]$ExpectedFeatureId,
        [Parameter(Mandatory = $true)][string]$ExpectedHead
    )
    $marker = Read-ProofMarker $Path $ExpectedSource
    if (
        $marker.repository_id -ne $ExpectedRepositoryId -or
        $marker.feature_id -ne $ExpectedFeatureId -or
        $marker.head_commit -ne $ExpectedHead
    ) {
        throw "The expected disposable checkout binding did not match its marker."
    }
    $head = Invoke-Git $Path @("rev-parse", "HEAD")
    $branch = Invoke-Git $Path @("branch", "--show-current")
    $status = Invoke-Git $Path @("status", "--porcelain")
    if ($head -ne $ExpectedHead -or $branch -ne "main" -or $status.Length -ne 0) {
        throw "The disposable proof checkout drifted."
    }
}

function Assert-TransferStagingEmpty {
    $staging = Join-Path $DataDir "feature-conveyor-repository-snapshots\staging"
    if (Test-Path -LiteralPath $staging) {
        if (@(Get-ChildItem -LiteralPath $staging -Force).Count -ne 0) {
            throw "The Windows snapshot transfer staging directory was not empty."
        }
    }
}

function Wait-ExactLocalCodingEvents {
    param(
        [Parameter(Mandatory = $true)][string]$ExpectedTaskId,
        [Parameter(Mandatory = $true)][string]$ExpectedStepId,
        [Parameter(Mandatory = $true)][string]$ExpectedDeviceId,
        [int]$TimeoutSeconds = 300
    )
    $cursor = $null
    $stream = $null
    $expectedKinds = @("step_queued", "step_leased", "step_succeeded")
    $sequences = [ordered]@{}
    $connectionEpoch = $null
    $index = 0
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        $after = if ($cursor) { [UInt64]$cursor.sequence } else { [UInt64]0 }
        $query = [ordered]@{
            protocol_version = $protocolVersion
            connection_epoch = 1
            after = $cursor
            limit = 64
        }
        $batch = Invoke-ExactPost -Path "/v1/development/events/next" -Body $query
        if (
            [UInt64]$batch.protocol_version -ne $protocolVersion -or
            $batch.stream_id -notmatch $uuidPattern -or
            [UInt64]$batch.after_sequence -ne $after -or
            [UInt64]$batch.next_sequence -lt $after -or
            $batch.has_more -isnot [bool]
        ) {
            throw "The local-coding event batch metadata was invalid."
        }
        if ($stream -and $stream -ne $batch.stream_id) {
            throw "The local-coding event stream changed."
        }
        $stream = $batch.stream_id
        $pageSequence = $after
        foreach ($event in @($batch.events)) {
            $pageSequence += 1
            if ($event.cursor.stream_id -ne $stream -or [UInt64]$event.cursor.sequence -ne $pageSequence) {
                throw "The local-coding event page was not contiguous."
            }
            if ($event.task_id -ne $ExpectedTaskId -or $event.step_id -ne $ExpectedStepId) {
                continue
            }
            if ($index -ge $expectedKinds.Count -or $event.kind -ne $expectedKinds[$index]) {
                throw "The exact local-coding event order was invalid."
            }
            if ($event.kind -eq "step_queued") {
                if ($null -ne $event.device_id -or $null -ne $event.connection_epoch) {
                    throw "The queued local-coding event unexpectedly carried lease identity."
                }
            } else {
                if ($event.device_id -ne $ExpectedDeviceId -or [UInt64]$event.connection_epoch -eq 0) {
                    throw "The local-coding lease event was bound to the wrong worker."
                }
                if ($connectionEpoch -and [UInt64]$connectionEpoch -ne [UInt64]$event.connection_epoch) {
                    throw "The local-coding connection epoch drifted within one attempt."
                }
                $connectionEpoch = [UInt64]$event.connection_epoch
            }
            $sequences[$event.kind] = [UInt64]$event.cursor.sequence
            $index += 1
        }
        if ([UInt64]$batch.next_sequence -ne $pageSequence) {
            throw "The local-coding event page ended at an invalid cursor."
        }
        if ($index -eq $expectedKinds.Count -and -not $batch.has_more) {
            return [ordered]@{
                stream_id = $stream
                connection_epoch = [UInt64]$connectionEpoch
                sequences = $sequences
            }
        }
        $cursor = [ordered]@{ stream_id = $stream; sequence = [UInt64]$batch.next_sequence }
        if (-not $batch.has_more) { Start-Sleep -Milliseconds 250 }
    }
    throw "Timed out waiting for the exact local-coding lifecycle."
}

if ($Action -eq "Check") {
    $testDigest = Convert-BytesToHex (Get-Sha256Bytes "assemblywright-local-coding-live-control-check-v1")
    $checkLeaf = "assemblywright-local-coding-live-control-check-$([Guid]::NewGuid().ToString('N'))"
    $checkRepository = Join-Path ([IO.Path]::GetTempPath()) $checkLeaf
    New-Item -ItemType Directory -Path $checkRepository | Out-Null
    $checkHooks = Join-Path $checkRepository ".assemblywright-empty-hooks"
    New-Item -ItemType Directory -Path $checkHooks | Out-Null
    $checkGitConfig = @(
        "-c", "commit.gpgSign=false",
        "-c", "core.autocrlf=false",
        "-c", "core.hooksPath=$checkHooks",
        "-c", "core.safecrlf=false",
        "-c", "init.templateDir="
    )
    $priorErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        & git @checkGitConfig -C $checkRepository init --quiet 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Git init failed in the blob-binding regression." }
        & git @checkGitConfig -C $checkRepository config user.name "Assemblywright Live Control Check" 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Git user.name setup failed in the blob-binding regression." }
        & git @checkGitConfig -C $checkRepository config user.email "assemblywright-live-control-check@invalid" 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Git user.email setup failed in the blob-binding regression." }
        $checkReadme = Join-Path $checkRepository "README.md"
        [IO.File]::WriteAllBytes($checkReadme, [Text.Encoding]::UTF8.GetBytes("immutable blob`n"))
        & git @checkGitConfig -C $checkRepository add -- README.md 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Git add failed in the blob-binding regression." }
        & git @checkGitConfig -C $checkRepository commit --quiet -m "blob binding fixture" 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Git commit failed in the blob-binding regression." }
        $checkCommit = (& git @checkGitConfig -C $checkRepository rev-parse HEAD).Trim()
        if ($LASTEXITCODE -ne 0 -or $checkCommit -notmatch $commitPattern) {
            throw "Git revision lookup failed in the blob-binding regression."
        }

        $checkMarker = [ordered]@{
            schema_version = 2
            status = "local_coding_disposable_checkout"
            source_repository = $checkRepository
            proof_repository = $checkRepository
            repository_id = "11111111-1111-4111-8111-111111111111"
            feature_id = "22222222-2222-4222-8222-222222222222"
            head_commit = $checkCommit
            queue_revision = 1
            emergency_pause_revision = 0
            owner_control_designation_revision = 1
        }
        Write-ProofMarkerAtomically $checkRepository $checkMarker
        $checkMarkerPath = Join-Path $checkRepository ".git\assemblywright-local-coding-live-proof"
        $checkPublishedMarker = Get-Content -LiteralPath $checkMarkerPath -Raw | ConvertFrom-Json
        Assert-ExactKeys $checkPublishedMarker @(
            "schema_version", "status", "source_repository", "proof_repository",
            "repository_id", "feature_id", "head_commit", "queue_revision",
            "emergency_pause_revision", "owner_control_designation_revision"
        ) "Atomic ordered-dictionary marker regression"
        $wrongCaseRejected = $false
        try {
            Assert-ExactKeys ([ordered]@{ Schema_version = 2; status = "fixture" }) @(
                "schema_version", "status"
            ) "Wrong-case ordered-dictionary regression"
        } catch {
            if ($_.Exception.Message -cne "Wrong-case ordered-dictionary regression returned an unexpected JSON shape.") {
                throw
            }
            $wrongCaseRejected = $true
        }
        $compositeKeyRejected = $false
        try {
            Assert-ExactKeys ([ordered]@{ "a|b" = 1; c = 2 }) @(
                "a", "b|c"
            ) "Composite-key ordered-dictionary regression"
        } catch {
            if ($_.Exception.Message -cne "Composite-key ordered-dictionary regression returned an unexpected JSON shape.") {
                throw
            }
            $compositeKeyRejected = $true
        }
        if (-not $wrongCaseRejected -or -not $compositeKeyRejected) {
            throw "Exact ordered-dictionary key regressions were not rejected."
        }
        Remove-Item -LiteralPath $checkMarkerPath -Force

        [IO.File]::WriteAllBytes($checkReadme, [Text.Encoding]::UTF8.GetBytes("immutable blob`r`n"))
        $blobDigest = Convert-BytesToHex (Get-GitBlobSha256Bytes $checkRepository $checkCommit "README.md")
        $expectedBlobDigest = Convert-BytesToHex (Get-Sha256Bytes "immutable blob`n")
        $worktreeAlgorithm = [Security.Cryptography.SHA256]::Create()
        try {
            $worktreeDigest = Convert-BytesToHex (@($worktreeAlgorithm.ComputeHash([IO.File]::ReadAllBytes($checkReadme))))
        } finally {
            $worktreeAlgorithm.Dispose()
        }
        $invalidPathRejected = $false
        try {
            Get-GitBlobSha256Bytes $checkRepository $checkCommit "readme.md" | Out-Null
        } catch {
            $invalidPathRejected = $true
        }
        if (
            $blobDigest -cne $expectedBlobDigest -or
            $blobDigest -ceq $worktreeDigest -or
            -not $invalidPathRejected
        ) {
            throw "Immutable Git blob binding did not reject CRLF working-tree or path drift."
        }
    } finally {
        $ErrorActionPreference = $priorErrorActionPreference
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd(
            [IO.Path]::DirectorySeparatorChar,
            [IO.Path]::AltDirectorySeparatorChar
        )
        $checkParent = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($checkRepository)).TrimEnd(
            [IO.Path]::DirectorySeparatorChar,
            [IO.Path]::AltDirectorySeparatorChar
        )
        if ($checkParent -cne $tempRoot -or [IO.Path]::GetFileName($checkRepository) -cne $checkLeaf) {
            throw "The blob-binding regression temporary path escaped its exact bounded leaf."
        }
        Remove-Item -LiteralPath $checkRepository -Recurse -Force
    }
    if (
        $testDigest.Length -ne 64 -or
        $protocolVersion -ne 5 -or
        $masterSchemaVersion -ne 19 -or
        $featureConveyorProjectionSchemaVersion -ne 9 -or
        $ownerControlSchemaVersion -ne 1
    ) {
        throw "Local-coding live controller self-check failed."
    }
    '{"atomic_marker_publication_regression":"verified","exact_key_negative_regressions":"verified","git_blob_crlf_regression":"verified","schema_version":1,"status":"local_coding_live_control_ready"}'
    exit 0
}

if (-not $ConfirmAction) {
    throw "Local-coding live control requires -ConfirmAction for this Windows-local owner action."
}

$tokenPath = Join-Path $DataDir "development.token"
if (-not (Test-Path -LiteralPath $tokenPath -PathType Leaf)) {
    throw "The Windows-local development token is unavailable."
}
$token = (Get-Content -LiteralPath $tokenPath -Raw).Trim()
if ($token.Length -lt 32 -or $token.Length -gt 256 -or $token -notmatch "^[\x21-\x7e]+$") {
    throw "The Windows-local development token is invalid."
}
$script:headers = @{ Authorization = "Bearer $token" }
$script:baseUri = "http://$Endpoint"
$paths = Resolve-ProofPaths

switch ($Action) {
    "Prepare" {
        if ($OwnerControlDesignationRevision -eq 0) {
            throw "Prepare requires the exact current owner-control designation revision."
        }
        if (-not (Test-Path -LiteralPath $paths.source -PathType Container)) {
            throw "The source checkout is unavailable."
        }
        Assert-NoReparseComponents $paths.source $false
        $status = Get-ConveyorStatus
        $main = Assert-SourceRepositoryEligible $paths.source
        Assert-NoReparseComponents $paths.proof $true
        if (Test-Path -LiteralPath $paths.proof) {
            $markerPath = Join-Path $paths.proof ".git\assemblywright-local-coding-live-proof"
            if (-not (Test-Path -LiteralPath $markerPath -PathType Leaf)) {
                if (
                    $status.owner_guidance.reason_code -ne "queue_empty" -or
                    $status.visible_feature_count -ne 0 -or $status.features_truncated -ne $false
                ) {
                    throw "An unmarked partial clone can be recovered only while the queue is empty."
                }
                Assert-NoReparseTree $paths.proof
                $partialHead = Invoke-Git $paths.proof @("rev-parse", "HEAD")
                $partialBranch = Invoke-Git $paths.proof @("branch", "--show-current")
                $partialStatus = Invoke-Git $paths.proof @("status", "--porcelain=v1", "--untracked-files=all")
                $partialTracked = Invoke-Git $paths.proof @("ls-files", "-v", "--")
                $partialRemotes = Invoke-Git $paths.proof @("remote")
                $partialOrigin = Invoke-Git $paths.proof @("remote", "get-url", "--all", "origin")
                if (
                    $partialHead -ne $main -or $partialBranch -ne "main" -or
                    $partialStatus.Length -ne 0 -or $partialRemotes -cne "origin" -or
                    $partialOrigin -cne $paths.source -or
                    @($partialTracked -split "`r?`n" | Where-Object { $_.Length -gt 0 -and $_ -cnotmatch "^H " }).Count -ne 0
                ) {
                    throw "The unmarked partial clone did not match the exact recoverable source."
                }
                Remove-Item -LiteralPath $paths.proof -Recurse -Force
                if (Test-Path -LiteralPath $paths.proof) {
                    throw "The exact unmarked partial clone was not removed."
                }
            }
        }
        $resumedPreparation = Test-Path -LiteralPath $paths.proof
        if ($resumedPreparation) {
            $marker = Read-ProofMarker $paths.proof $paths.source
            if (
                [UInt64]$marker.owner_control_designation_revision -ne $OwnerControlDesignationRevision -or
                [UInt64]$marker.emergency_pause_revision -ne [UInt64]$status.owner_guidance.emergency_pause_revision
            ) {
                throw "The resumable preparation marker drifted from owner-control authority."
            }
            $repository = [string]$marker.repository_id
            $feature = [string]$marker.feature_id
            $main = [string]$marker.head_commit
            $prepareQueueRevision = [UInt64]$marker.queue_revision
            $preparePauseRevision = [UInt64]$marker.emergency_pause_revision
        } else {
            if (
                $status.owner_guidance.reason_code -ne "queue_empty" -or
                $status.visible_feature_count -ne 0 -or
                $status.features_truncated -ne $false
            ) {
                throw "Fresh Prepare requires an unpaused empty Feature Conveyor."
            }
            $repository = [guid]::NewGuid().ToString().ToLowerInvariant()
            $feature = [guid]::NewGuid().ToString().ToLowerInvariant()
            $prepareQueueRevision = [UInt64]$status.queue_revision
            $preparePauseRevision = [UInt64]$status.owner_guidance.emergency_pause_revision
            $cloneErrorActionPreference = $ErrorActionPreference
            try {
                # Windows PowerShell 5 surfaces a native process's stderr as
                # ErrorRecord values. Git writes normal clone progress there, so
                # capture it and make the native exit code the sole verdict.
                $ErrorActionPreference = "Continue"
                $cloneOutput = @(& git --no-replace-objects -c core.autocrlf=false -c core.fsmonitor=false -c core.hooksPath=NUL -c core.safecrlf=false -c init.templateDir= clone --no-local --single-branch --branch main $paths.source $paths.proof 2>&1)
                $cloneExitCode = $LASTEXITCODE
            } finally {
                $ErrorActionPreference = $cloneErrorActionPreference
            }
            if ($cloneExitCode -ne 0) {
                if (Test-Path -LiteralPath $paths.proof) {
                    Assert-NoReparseTree $paths.proof
                    Remove-Item -LiteralPath $paths.proof -Recurse -Force
                }
                throw "Git could not create the standalone disposable checkout."
            }
            Assert-NoReparseComponents $paths.proof $false
            $marker = [ordered]@{
                schema_version = 2
                status = "local_coding_disposable_checkout"
                source_repository = $paths.source
                proof_repository = $paths.proof
                repository_id = $repository
                feature_id = $feature
                head_commit = $main
                queue_revision = $prepareQueueRevision
                emergency_pause_revision = $preparePauseRevision
                owner_control_designation_revision = $OwnerControlDesignationRevision
            }
            Write-ProofMarkerAtomically $paths.proof $marker
        }
        $remotes = Invoke-Git $paths.proof @("remote")
        if ($remotes -eq "origin") {
            Invoke-Git $paths.proof @("remote", "remove", "origin") | Out-Null
        } elseif ($remotes.Length -ne 0) {
            throw "The resumable proof checkout contained an unexpected remote."
        }
        Remove-BoundedCommitGraphCache $paths.proof
        Assert-SnapshotCompatibleObjectStore $paths.proof
        Assert-ProofRepositoryClean $paths.proof $paths.source $repository $feature $main

        $visibleFeatures = @($status.features)
        $enqueueAlreadyCommitted = $false
        if ($status.visible_feature_count -eq 0 -and $visibleFeatures.Count -eq 0) {
            if (
                [UInt64]$status.queue_revision -ne $prepareQueueRevision -or
                $status.owner_guidance.reason_code -ne "queue_empty"
            ) {
                throw "The resumable empty queue drifted from its preparation baseline."
            }
        } elseif (
            $status.visible_feature_count -eq 1 -and $visibleFeatures.Count -eq 1 -and
            $status.features_truncated -eq $false -and
            $visibleFeatures[0].feature_id -eq $feature -and
            [UInt64]$visibleFeatures[0].specification_revision -eq 1 -and
            [UInt64]$visibleFeatures[0].lifecycle_revision -eq 1 -and
            $visibleFeatures[0].status -eq "queued" -and
            $visibleFeatures[0].lease_present -eq $false -and
            $visibleFeatures[0].effect_possible -eq $false -and
            [UInt64]$status.queue_revision -eq ($prepareQueueRevision + 1)
        ) {
            $enqueueAlreadyCommitted = $true
        } else {
            throw "Prepare found state other than its empty baseline or one exact committed enqueue."
        }

        $scope = [ordered]@{
            expected_base_branch = "main"
            expected_head_commit = $main
            repository_id = $repository
            repository_path = $paths.proof
        }
        $scopeJson = $scope | ConvertTo-Json -Compress
        $scopeDigest = @(Get-Sha256Bytes $scopeJson)
        $grantRevisions = [ordered]@{
            registration = 1
            cloud_disclosure = 1
            autonomous_publication = 1
        }
        $grantKinds = @("registration", "cloud_disclosure", "autonomous_publication")
        $grantSet = Invoke-ExactGet -Path "/v1/feature-conveyor/repositories/$repository/grants"
        if (
            $grantSet.repository_id -ne $repository -or $grantSet.emergency_paused -ne $false -or
            [UInt64]$grantSet.emergency_pause_revision -ne $preparePauseRevision
        ) {
            throw "The resumable repository-grant authority drifted."
        }
        foreach ($kind in $grantKinds) {
            $grantScope = if ($kind -eq "registration") {
                $scopeDigest
            } else {
                @(Get-Sha256Bytes "assemblywright.local-coding-live.$kind.scope.v1`0$repository")
            }
            $approval = @(Get-Sha256Bytes "assemblywright.local-coding-live.$kind.owner-approval.v1`0$repository")
            $currentGrant = $grantSet.$kind
            if ($null -eq $currentGrant) {
                if ($enqueueAlreadyCommitted) {
                    throw "The committed enqueue lost its exact $kind grant."
                }
                $request = [ordered]@{
                    schema_version = $ownerControlSchemaVersion
                    expected_current_revision = 0
                    expected_emergency_pause_revision = $preparePauseRevision
                    grant = [ordered]@{
                        repository_id = $repository
                        kind = $kind
                        revision = 1
                        scope_sha256 = $grantScope
                        owner_approval_sha256 = $approval
                        expires_at_ms = $null
                        revoked = $false
                    }
                }
                $recorded = Invoke-ExactPost -Path "/v1/feature-conveyor/repository-grants" -Body $request
                if (
                    $recorded.status -ne "recorded" -or [UInt64]$recorded.revision -ne 1 -or
                    (Convert-BytesToHex $recorded.scope_sha256) -cne (Convert-BytesToHex $grantScope) -or
                    (Convert-BytesToHex $recorded.owner_approval_sha256) -cne (Convert-BytesToHex $approval)
                ) {
                    throw "The Windows master did not record the exact $kind grant."
                }
            } elseif (
                [UInt64]$currentGrant.revision -ne 1 -or $currentGrant.revoked -ne $false -or
                $currentGrant.active -ne $true -or $null -ne $currentGrant.expires_at_ms -or
                (Convert-BytesToHex $currentGrant.scope_sha256) -cne (Convert-BytesToHex $grantScope) -or
                (Convert-BytesToHex $currentGrant.owner_approval_sha256) -cne (Convert-BytesToHex $approval)
            ) {
                throw "Prepare found a non-resumable $kind grant revision."
            }
        }
        $preflightRequest = [ordered]@{
            schema_version = $ownerControlSchemaVersion
            scope = $scope
            scope_sha256 = $scopeDigest
            registration_grant_revision = 1
            expected_emergency_pause_revision = $preparePauseRevision
        }
        $preflight = Invoke-ExactPost -Path "/v1/feature-conveyor/repository-preflight" -Body $preflightRequest
        Assert-Digest $preflight.preflight_fingerprint_sha256 "Repository preflight fingerprint"
        if (
            $preflight.status -ne "identity_eligible" -or
            $preflight.repository_id -ne $repository -or
            $preflight.head_commit -ne $main
        ) {
            throw "The repository preflight receipt drifted."
        }
        $manifest = [ordered]@{
            acceptance = @("restricted-worker-live-attempt")
            allowed_paths = @("README.md")
            outcome = "prove one owner-approved snapshot-bound local-coding attempt"
        }
        $manifestJson = $manifest | ConvertTo-Json -Compress
        $approvedRequest = [ordered]@{
            schema_version = $ownerControlSchemaVersion
            expected_queue_revision = $prepareQueueRevision
            owner_control_designation_revision = $OwnerControlDesignationRevision
            emergency_pause_revision = $preparePauseRevision
            specification = [ordered]@{
                feature_id = $feature
                revision = 1
                repository_id = $repository
                manifest = $manifest
                manifest_sha256 = @(Get-Sha256Bytes $manifestJson)
                design_sha256 = @(Get-Sha256Bytes "assemblywright.local-coding-live.design.v1`0$feature")
                brainstorming_sha256 = @(Get-Sha256Bytes "assemblywright.local-coding-live.brainstorming.v1`0$feature")
                owner_approval_sha256 = @(Get-Sha256Bytes "assemblywright.local-coding-live.owner-approval.v1`0$feature")
                grants = $grantRevisions
                provider_id = "local.review"
                model_id = "assemblywright-live-proof-v1"
                dependencies = @()
            }
        }
        $approvedJson = $approvedRequest | ConvertTo-Json -Compress -Depth 12
        [ordered]@{
            schema_version = 1
            status = "local_coding_repository_prepared"
            repository_id = $repository
            feature_id = $feature
            head_commit = $main
            scope_sha256 = Convert-BytesToHex $scopeDigest
            queue_revision = $prepareQueueRevision
            enqueue_queue_revision = if ($enqueueAlreadyCommitted) { [UInt64]$status.queue_revision } else { $null }
            enqueue_already_committed = $enqueueAlreadyCommitted
            emergency_pause_revision = $preparePauseRevision
            owner_control_designation_revision = $OwnerControlDesignationRevision
            preflight_fingerprint_sha256 = Convert-BytesToHex $preflight.preflight_fingerprint_sha256
            approved_request_sha256 = Convert-BytesToHex (Get-Sha256Bytes $approvedJson)
            approved_request_base64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($approvedJson))
        } | ConvertTo-Json -Compress
    }
    "ClaimAndDispatch" {
        if (
            $RepositoryId -notmatch $uuidPattern -or $FeatureId -notmatch $uuidPattern -or
            $HeadCommit -notmatch $commitPattern -or $LocalCodingDeviceId -notmatch $uuidPattern -or
            $LocalCodingRegistryRevision -eq 0 -or $ExpectedLifecycleRevision -eq 0
        ) {
            throw "ClaimAndDispatch requires exact enqueue and local-coding identity bindings."
        }
        Assert-ProofRepositoryClean $paths.proof $paths.source $RepositoryId $FeatureId $HeadCommit
        Remove-BoundedCommitGraphCache $paths.proof
        Assert-SnapshotCompatibleObjectStore $paths.proof
        Assert-ProofRepositoryClean $paths.proof $paths.source $RepositoryId $FeatureId $HeadCommit
        $scope = [ordered]@{
            expected_base_branch = "main"
            expected_head_commit = $HeadCommit
            repository_id = $RepositoryId
            repository_path = $paths.proof
        }
        $scopeJson = $scope | ConvertTo-Json -Compress
        $scopeDigest = @(Get-Sha256Bytes $scopeJson)
        $preflightRequest = [ordered]@{
            schema_version = $ownerControlSchemaVersion
            scope = $scope
            scope_sha256 = $scopeDigest
            registration_grant_revision = 1
            expected_emergency_pause_revision = $ExpectedEmergencyPauseRevision
        }
        $preflight = Invoke-ExactPost -Path "/v1/feature-conveyor/repository-preflight" -Body $preflightRequest
        if ($preflight.status -ne "identity_eligible" -or $preflight.head_commit -ne $HeadCommit) {
            throw "The dispatch-time repository preflight failed."
        }
        $claimRequest = [ordered]@{
            schema_version = $ownerControlSchemaVersion
            scope = $scope
            scope_sha256 = $scopeDigest
            expected_feature_id = $FeatureId
            expected_specification_revision = 1
            expected_queue_revision = $ExpectedQueueRevision
            expected_emergency_pause_revision = $ExpectedEmergencyPauseRevision
            grants = [ordered]@{ registration = 1; cloud_disclosure = 1; autonomous_publication = 1 }
            provider_id = "local.review"
            model_id = "assemblywright-live-proof-v1"
        }
        $claim = Invoke-ExactPost -Path "/v1/feature-conveyor/repository-snapshot-claims" -Body $claimRequest
        Assert-Digest $claim.snapshot_sha256 "Snapshot receipt"
        if (
            $claim.status -ne "snapshot_bound" -or $claim.feature_id -ne $FeatureId -or
            $claim.base_commit -ne $HeadCommit -or
            [UInt64]$claim.queue_revision -ne ($ExpectedQueueRevision + 1)
        ) {
            throw "The snapshot claim receipt drifted."
        }
        $packet = [guid]::NewGuid().ToString().ToLowerInvariant()
        $replacementBytes = [Text.Encoding]::UTF8.GetBytes("assemblywright contained coding fixture`n")
        $packetDocument = [ordered]@{
            acceptance_criteria_count = 1
            allowed_paths = @("README.md")
            operations = @(
                [ordered]@{
                    arguments = [ordered]@{
                        executable = $false
                        expected_before_sha256 = @(Get-GitBlobSha256Bytes $paths.proof $HeadCommit "README.md")
                        path = "README.md"
                        replacement_hex = Convert-BytesToHex $replacementBytes
                        replacement_sha256 = @(Get-Sha256Bytes "assemblywright contained coding fixture`n")
                    }
                    tool_id = "file.write.v1"
                }
            )
            ordinal = 1
            packet_id = $packet
        }
        # These ordered keys are the protocol-owned recursively sorted JSON
        # order. Hash the exact compact UTF-8 bytes Rust will independently
        # canonicalize and verify before dispatch.
        $packetJson = $packetDocument | ConvertTo-Json -Compress -Depth 12
        $packetDigest = @(Get-Sha256Bytes $packetJson)
        $dispatchRequest = [ordered]@{
            schema_version = $ownerControlSchemaVersion
            feature_id = $FeatureId
            specification_revision = 1
            expected_lifecycle_revision = [UInt64]$claim.lifecycle_revision
            feature_lease_id = $claim.lease_id
            snapshot_id = $claim.snapshot_id
            snapshot_sha256 = @($claim.snapshot_sha256)
            work_packet_sha256 = $packetDigest
            work_packet = $packetDocument
            device_id = $LocalCodingDeviceId
            device_registry_revision = $LocalCodingRegistryRevision
            expected_queue_revision = [UInt64]$claim.queue_revision
            expected_emergency_pause_revision = [UInt64]$claim.emergency_pause_revision
        }
        $dispatch = Invoke-ExactPost -Path "/v1/feature-conveyor/coding-dispatches" -Body $dispatchRequest
        if (
            $dispatch.status -ne "queued" -or $dispatch.feature_id -ne $FeatureId -or
            $dispatch.device_id -ne $LocalCodingDeviceId -or $dispatch.packet_id -ne $packet
        ) {
            throw "The coding dispatch receipt drifted."
        }
        $events = Wait-ExactLocalCodingEvents $dispatch.task_id $dispatch.step_id $LocalCodingDeviceId
        Get-MasterHealth | Out-Null
        Assert-TransferStagingEmpty
        Assert-ProofRepositoryClean $paths.proof $paths.source $RepositoryId $FeatureId $HeadCommit
        [ordered]@{
            schema_version = 1
            status = "local_coding_dispatch_succeeded"
            repository_id = $RepositoryId
            feature_id = $FeatureId
            lifecycle_revision = [UInt64]$dispatch.lifecycle_revision
            queue_revision = [UInt64]$dispatch.queue_revision
            emergency_pause_revision = [UInt64]$dispatch.emergency_pause_revision
            task_id = $dispatch.task_id
            step_id = $dispatch.step_id
            stream_id = $events.stream_id
            device_id = $LocalCodingDeviceId
            connection_epoch = $events.connection_epoch
            queued_sequence = $events.sequences.step_queued
            leased_sequence = $events.sequences.step_leased
            succeeded_sequence = $events.sequences.step_succeeded
            snapshot_sha256 = Convert-BytesToHex $dispatch.snapshot_sha256
            work_packet_sha256 = Convert-BytesToHex $dispatch.work_packet_sha256
            transfer_staging_empty = $true
            proof_checkout_clean = $true
        } | ConvertTo-Json -Compress
    }
    "Integrate" {
        if (
            $RepositoryId -notmatch $uuidPattern -or $FeatureId -notmatch $uuidPattern -or
            $HeadCommit -notmatch $commitPattern
        ) {
            throw "Integrate requires exact repository, feature, and base-commit bindings."
        }
        Assert-ProofRepositoryClean $paths.proof $paths.source $RepositoryId $FeatureId $HeadCommit
        $plan = Invoke-ExactGet -Path "/v1/feature-conveyor/features/$FeatureId/integration-plan"
        Assert-ExactKeys $plan @(
            "schema_version", "feature_id", "specification_revision", "lifecycle_revision",
            "feature_lease_id", "snapshot_id", "snapshot_sha256", "artifact_ids",
            "queue_revision", "emergency_pause_revision", "grants", "base_commit"
        ) "Artifact integration plan"
        $artifactIds = @($plan.artifact_ids)
        Assert-ExactKeys $plan.grants @(
            "registration", "cloud_disclosure", "autonomous_publication"
        ) "Artifact integration plan grants"
        if (
            [UInt64]$plan.schema_version -ne 1 -or $plan.feature_id -ne $FeatureId -or
            $plan.base_commit -ne $HeadCommit -or [UInt64]$plan.specification_revision -ne 1 -or
            [UInt64]$plan.lifecycle_revision -eq 0 -or
            $plan.feature_lease_id -notmatch $uuidPattern -or $plan.snapshot_id -notmatch $uuidPattern -or
            $artifactIds.Count -lt 1 -or $artifactIds.Count -gt 3 -or
            [UInt64]$plan.grants.registration -eq 0 -or
            [UInt64]$plan.grants.cloud_disclosure -eq 0 -or
            [UInt64]$plan.grants.autonomous_publication -eq 0
        ) {
            throw "The artifact integration plan drifted."
        }
        Assert-Digest $plan.snapshot_sha256 "Artifact integration snapshot"
        $sortedArtifactIds = @($artifactIds | Sort-Object)
        if (($artifactIds -join "|") -cne ($sortedArtifactIds -join "|")) {
            throw "The artifact integration IDs were not canonically sorted."
        }
        foreach ($artifactId in $artifactIds) {
            if ($artifactId -notmatch $uuidPattern) {
                throw "The artifact integration plan contained an invalid artifact ID."
            }
        }
        $integrationId = [guid]::NewGuid().ToString().ToLowerInvariant()
        $request = [ordered]@{
            schema_version = 1
            integration_id = $integrationId
            feature_id = $FeatureId
            specification_revision = [UInt64]$plan.specification_revision
            expected_lifecycle_revision = [UInt64]$plan.lifecycle_revision
            feature_lease_id = $plan.feature_lease_id
            snapshot_id = $plan.snapshot_id
            snapshot_sha256 = @($plan.snapshot_sha256)
            artifact_ids = $artifactIds
            expected_queue_revision = [UInt64]$plan.queue_revision
            expected_emergency_pause_revision = [UInt64]$plan.emergency_pause_revision
            grants = $plan.grants
            base_commit = $plan.base_commit
        }
        $receipt = Invoke-ExactPost -Path "/v1/feature-conveyor/artifact-integrations" -Body $request
        Assert-ExactKeys $receipt @(
            "schema_version", "integration_id", "feature_id", "specification_revision",
            "lifecycle_revision", "feature_lease_id", "snapshot_id", "snapshot_sha256",
            "artifact_set_sha256", "candidate_commit", "candidate_tree", "base_commit",
            "queue_revision", "emergency_pause_revision", "grants", "status"
        ) "Artifact integration receipt"
        Assert-Digest $receipt.artifact_set_sha256 "Artifact set receipt"
        Assert-Digest $receipt.snapshot_sha256 "Artifact integration receipt snapshot"
        Assert-ExactKeys $receipt.grants @(
            "registration", "cloud_disclosure", "autonomous_publication"
        ) "Artifact integration receipt grants"
        if (
            [UInt64]$receipt.schema_version -ne 1 -or $receipt.status -ne "candidate_frozen" -or
            $receipt.integration_id -ne $integrationId -or $receipt.feature_id -ne $FeatureId -or
            $receipt.candidate_commit -notmatch $commitPattern -or
            $receipt.candidate_tree -notmatch $commitPattern -or $receipt.base_commit -ne $HeadCommit -or
            $receipt.feature_lease_id -ne $plan.feature_lease_id -or
            $receipt.snapshot_id -ne $plan.snapshot_id -or
            (Convert-BytesToHex $receipt.snapshot_sha256) -cne (Convert-BytesToHex $plan.snapshot_sha256) -or
            [UInt64]$receipt.lifecycle_revision -ne ([UInt64]$plan.lifecycle_revision + 1) -or
            [UInt64]$receipt.queue_revision -ne [UInt64]$plan.queue_revision -or
            [UInt64]$receipt.emergency_pause_revision -ne [UInt64]$plan.emergency_pause_revision -or
            [UInt64]$receipt.grants.registration -ne [UInt64]$plan.grants.registration -or
            [UInt64]$receipt.grants.cloud_disclosure -ne [UInt64]$plan.grants.cloud_disclosure -or
            [UInt64]$receipt.grants.autonomous_publication -ne [UInt64]$plan.grants.autonomous_publication
        ) {
            throw "The artifact integration receipt drifted."
        }
        $candidate = Join-Path $DataDir "feature-conveyor-candidates\candidates\$integrationId"
        Assert-NoReparseTree $candidate
        $candidateHead = Invoke-Git $candidate @("rev-parse", "HEAD")
        $candidateTree = Invoke-Git $candidate @("rev-parse", "HEAD^{tree}")
        $candidateBranch = Invoke-Git $candidate @("branch", "--show-current")
        $candidateStatus = Invoke-Git $candidate @("status", "--porcelain")
        $candidateRemotes = Invoke-Git $candidate @("remote")
        $candidateFsck = Invoke-Git $candidate @("fsck", "--no-dangling")
        $candidateReadme = Join-Path $candidate "README.md"
        $candidateContentSha256 = Convert-BytesToHex (Get-Sha256Bytes "assemblywright contained coding fixture`n")
        $actualCandidateSha256 = Convert-BytesToHex (Get-Sha256FileBytes $candidateReadme)
        if (
            $candidateHead -ne $receipt.candidate_commit -or $candidateTree -ne $receipt.candidate_tree -or
            $candidateBranch.Length -ne 0 -or $candidateStatus.Length -ne 0 -or
            $candidateRemotes.Length -ne 0 -or $candidateFsck.Length -ne 0 -or
            $actualCandidateSha256 -cne $candidateContentSha256
        ) {
            throw "The frozen candidate worktree did not match its exact receipt."
        }
        $retry = Invoke-ExactPost -Path "/v1/feature-conveyor/artifact-integrations" -Body $request
        if (
            $retry.integration_id -ne $receipt.integration_id -or
            $retry.candidate_commit -ne $receipt.candidate_commit -or
            $retry.candidate_tree -ne $receipt.candidate_tree -or
            [UInt64]$retry.lifecycle_revision -ne [UInt64]$receipt.lifecycle_revision
        ) {
            throw "The exact artifact integration retry was not idempotent."
        }
        Assert-ProofRepositoryClean $paths.proof $paths.source $RepositoryId $FeatureId $HeadCommit
        [ordered]@{
            schema_version = 1
            status = "artifact_integration_candidate_frozen"
            repository_id = $RepositoryId
            feature_id = $FeatureId
            integration_id = $integrationId
            lifecycle_revision = [UInt64]$receipt.lifecycle_revision
            queue_revision = [UInt64]$receipt.queue_revision
            emergency_pause_revision = [UInt64]$receipt.emergency_pause_revision
            candidate_commit = $receipt.candidate_commit
            candidate_tree = $receipt.candidate_tree
            base_commit = $receipt.base_commit
            artifact_set_sha256 = Convert-BytesToHex $receipt.artifact_set_sha256
            candidate_detached = $true
            candidate_remote_absent = $true
            candidate_worktree_clean = $true
            candidate_fsck_clean = $true
            proof_checkout_clean = $true
            exact_retry_idempotent = $true
        } | ConvertTo-Json -Compress
    }
    "Cancel" {
        if ($FeatureId -notmatch $uuidPattern -or $ExpectedLifecycleRevision -eq 0) {
            throw "Cancel requires the exact active feature binding."
        }
        $request = [ordered]@{
            schema_version = $ownerControlSchemaVersion
            feature_id = $FeatureId
            expected_lifecycle_revision = $ExpectedLifecycleRevision
            expected_queue_revision = $ExpectedQueueRevision
            expected_emergency_pause_revision = $ExpectedEmergencyPauseRevision
        }
        $receipt = Invoke-ExactPost -Path "/v1/feature-conveyor/cancel-active-feature" -Body $request
        if (
            $receipt.status -ne "cancelled" -or $receipt.feature_id -ne $FeatureId -or
            $receipt.lease_retained -ne $true -or $receipt.advancement_authorized -ne $false -or
            [UInt64]$receipt.lifecycle_revision -ne ($ExpectedLifecycleRevision + 1) -or
            [UInt64]$receipt.queue_revision -ne $ExpectedQueueRevision
        ) {
            throw "The exact cancellation receipt drifted."
        }
        Get-MasterHealth | Out-Null
        [ordered]@{
            schema_version = 1
            status = "local_coding_feature_cancelled"
            feature_id = $FeatureId
            lifecycle_revision = [UInt64]$receipt.lifecycle_revision
            queue_revision = [UInt64]$receipt.queue_revision
            emergency_pause_revision = [UInt64]$receipt.emergency_pause_revision
            lease_retained = $true
            advancement_authorized = $false
        } | ConvertTo-Json -Compress
    }
    "Abandon" {
        if (
            $RepositoryId -notmatch $uuidPattern -or $FeatureId -notmatch $uuidPattern -or
            $HeadCommit -notmatch $commitPattern -or $ExpectedLifecycleRevision -eq 0 -or
            $TaskId -notmatch $uuidPattern -or $StepId -notmatch $uuidPattern -or
            $SucceededSequence -eq 0 -or $MacCleanupSha256 -notmatch "^[0-9a-f]{64}$"
        ) {
            throw "Abandon requires exact reconciliation evidence."
        }
        Assert-ProofRepositoryClean $paths.proof $paths.source $RepositoryId $FeatureId $HeadCommit
        Get-MasterHealth | Out-Null
        Assert-TransferStagingEmpty
        $reconciliation = [ordered]@{
            feature_id = $FeatureId
            head_commit = $HeadCommit
            mac_cleanup_sha256 = $MacCleanupSha256
            repository_id = $RepositoryId
            step_id = $StepId
            succeeded_sequence = $SucceededSequence
            task_id = $TaskId
            transfer_staging_empty = $true
            windows_checkout_clean = $true
        }
        $reconciliationJson = $reconciliation | ConvertTo-Json -Compress
        $request = [ordered]@{
            schema_version = $ownerControlSchemaVersion
            feature_id = $FeatureId
            expected_lifecycle_revision = $ExpectedLifecycleRevision
            expected_queue_revision = $ExpectedQueueRevision
            expected_emergency_pause_revision = $ExpectedEmergencyPauseRevision
            evidence = [ordered]@{
                safe_reconciliation_sha256 = @(Get-Sha256Bytes $reconciliationJson)
                merged = $false
                verified_healthy_main_sha256 = $null
            }
        }
        $receipt = Invoke-ExactPost -Path "/v1/feature-conveyor/abandon-and-advance" -Body $request
        if (
            $receipt.status -ne "abandoned" -or $receipt.feature_id -ne $FeatureId -or
            $receipt.lease_released -ne $true -or
            [UInt64]$receipt.lifecycle_revision -ne ($ExpectedLifecycleRevision + 1) -or
            [UInt64]$receipt.queue_revision -ne ($ExpectedQueueRevision + 1)
        ) {
            throw "The exact abandonment receipt drifted."
        }
        $status = Get-ConveyorStatus
        if ($status.visible_feature_count -ne 0 -or @($status.features).Count -ne 0 -or $status.owner_guidance.reason_code -ne "queue_empty") {
            throw "The Feature Conveyor did not become empty after abandonment."
        }
        Get-MasterHealth | Out-Null
        Assert-TransferStagingEmpty
        [ordered]@{
            schema_version = 1
            status = "local_coding_feature_abandoned"
            feature_id = $FeatureId
            lifecycle_revision = [UInt64]$receipt.lifecycle_revision
            queue_revision = [UInt64]$receipt.queue_revision
            emergency_pause_revision = [UInt64]$receipt.emergency_pause_revision
            safe_reconciliation_sha256 = Convert-BytesToHex $request.evidence.safe_reconciliation_sha256
            lease_released = $true
            queue_empty = $true
            transfer_staging_empty = $true
        } | ConvertTo-Json -Compress
    }
    "Cleanup" {
        $status = Get-ConveyorStatus
        if ($status.visible_feature_count -ne 0 -or @($status.features).Count -ne 0) {
            throw "Cleanup is blocked while the Feature Conveyor is nonempty."
        }
        Get-MasterHealth | Out-Null
        Assert-TransferStagingEmpty
        $checkoutPresent = Test-Path -LiteralPath $paths.proof
        if ($checkoutPresent) {
            $marker = Read-ProofMarker $paths.proof $paths.source
            if (
                ($RepositoryId -and $RepositoryId -ne $marker.repository_id) -or
                ($FeatureId -and $FeatureId -ne $marker.feature_id) -or
                ($HeadCommit -and $HeadCommit -ne $marker.head_commit)
            ) {
                throw "Cleanup arguments drifted from the exact disposable marker."
            }
            $effectiveRepositoryId = [string]$marker.repository_id
            $effectiveFeatureId = [string]$marker.feature_id
            $effectiveHeadCommit = [string]$marker.head_commit
            Assert-ProofRepositoryClean $paths.proof $paths.source $effectiveRepositoryId $effectiveFeatureId $effectiveHeadCommit
        } else {
            if (
                $RepositoryId -notmatch $uuidPattern -or
                $FeatureId -notmatch $uuidPattern -or
                $HeadCommit -notmatch $commitPattern
            ) {
                throw "Cleanup without a checkout requires its exact prior binding."
            }
            $effectiveRepositoryId = $RepositoryId
            $effectiveFeatureId = $FeatureId
            $effectiveHeadCommit = $HeadCommit
        }
        $grantSet = Invoke-ExactGet -Path "/v1/feature-conveyor/repositories/$effectiveRepositoryId/grants"
        $absentGrantCount = 0
        $revokedGrantCount = 0
        foreach ($kind in @("registration", "cloud_disclosure", "autonomous_publication")) {
            $current = $grantSet.$kind
            if ($null -eq $current) {
                $absentGrantCount += 1
                continue
            }
            if ([UInt64]$current.revision -eq 2 -and $current.revoked -eq $true) {
                $revokedGrantCount += 1
                continue
            }
            if ([UInt64]$current.revision -ne 1 -or $current.revoked -ne $false) {
                throw "Cleanup found a non-resumable $kind grant revision."
            }
            $request = [ordered]@{
                schema_version = $ownerControlSchemaVersion
                expected_current_revision = 1
                expected_emergency_pause_revision = [UInt64]$status.owner_guidance.emergency_pause_revision
                grant = [ordered]@{
                    repository_id = $effectiveRepositoryId
                    kind = $kind
                    revision = 2
                    scope_sha256 = @($current.scope_sha256)
                    owner_approval_sha256 = @($current.owner_approval_sha256)
                    expires_at_ms = $current.expires_at_ms
                    revoked = $true
                }
            }
            $revoked = Invoke-ExactPost -Path "/v1/feature-conveyor/repository-grants" -Body $request
            if ($revoked.status -ne "recorded" -or [UInt64]$revoked.revision -ne 2 -or $revoked.revoked -ne $true) {
                throw "The Windows master did not revoke the exact $kind grant."
            }
            $revokedGrantCount += 1
        }
        $terminalGrantSet = Invoke-ExactGet -Path "/v1/feature-conveyor/repositories/$effectiveRepositoryId/grants"
        foreach ($kind in @("registration", "cloud_disclosure", "autonomous_publication")) {
            $terminal = $terminalGrantSet.$kind
            if ($null -eq $terminal) {
                continue
            }
            if ([UInt64]$terminal.revision -ne 2 -or $terminal.revoked -ne $true) {
                throw "Cleanup did not reach an absent-or-revoked terminal grant state."
            }
        }
        if ($checkoutPresent) {
            Assert-ProofRepositoryClean $paths.proof $paths.source $effectiveRepositoryId $effectiveFeatureId $effectiveHeadCommit
            Assert-NoReparseTree $paths.proof
            Remove-Item -LiteralPath $paths.proof -Recurse -Force
        }
        if (Test-Path -LiteralPath $paths.proof) {
            throw "The disposable proof checkout was not removed."
        }
        [ordered]@{
            schema_version = 1
            status = "local_coding_live_cleanup_complete"
            repository_id = $effectiveRepositoryId
            feature_id = $effectiveFeatureId
            absent_grant_count = $absentGrantCount
            revoked_grant_count = $revokedGrantCount
            grant_cleanup_status = "absent_or_revoked"
            proof_checkout_removed = $true
        } | ConvertTo-Json -Compress
    }
}
