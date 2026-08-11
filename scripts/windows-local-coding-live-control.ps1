param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Check", "Prepare", "ClaimAndDispatch", "Cancel", "Abandon", "Cleanup")]
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

$protocolVersion = 3
$masterSchemaVersion = 11
$featureConveyorProjectionSchemaVersion = 8
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
    $actual = @($Value.PSObject.Properties.Name | Sort-Object)
    $expected = @($Keys | Sort-Object)
    if (($actual -join "|") -ne ($expected -join "|")) {
        throw "$Label returned an unexpected JSON shape."
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
                $entry.Name -cnotmatch "^graph-[0-9a-f]{64}\.graph$")
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
    $output = @(& git -C $Repository @Arguments 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "Git rejected the bounded live-proof operation."
    }
    return ($output -join "`n").Trim()
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
        "repository_id", "feature_id", "head_commit"
    ) "Disposable checkout marker"
    if (
        [UInt64]$marker.schema_version -ne 1 -or
        $marker.status -ne "local_coding_disposable_checkout" -or
        $marker.source_repository -cne $ExpectedSource -or
        $marker.proof_repository -cne $Path -or
        $marker.repository_id -notmatch $uuidPattern -or
        $marker.feature_id -notmatch $uuidPattern -or
        $marker.head_commit -notmatch $commitPattern
    ) {
        throw "The disposable checkout marker binding drifted."
    }
    $sourceHead = Invoke-Git $ExpectedSource @("rev-parse", "refs/heads/main")
    $sourceOriginHead = Invoke-Git $ExpectedSource @("rev-parse", "refs/remotes/origin/main")
    if ($sourceHead -ne $marker.head_commit -or $sourceOriginHead -ne $marker.head_commit) {
        throw "The marker-bound source main identity drifted."
    }
    return $marker
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
    if (
        $testDigest.Length -ne 64 -or
        $protocolVersion -ne 3 -or
        $masterSchemaVersion -ne 11 -or
        $featureConveyorProjectionSchemaVersion -ne 8 -or
        $ownerControlSchemaVersion -ne 1
    ) {
        throw "Local-coding live controller self-check failed."
    }
    '{"schema_version":1,"status":"local_coding_live_control_ready"}'
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
        Assert-NoReparseComponents $paths.proof $true
        if (Test-Path -LiteralPath $paths.proof) {
            throw "The disposable proof checkout already exists."
        }
        $status = Get-ConveyorStatus
        if (
            $status.owner_guidance.reason_code -ne "queue_empty" -or
            $status.visible_feature_count -ne 0 -or
            $status.features_truncated -ne $false
        ) {
            throw "Prepare requires an unpaused empty Feature Conveyor."
        }
        $main = Invoke-Git $paths.source @("rev-parse", "refs/heads/main")
        $originMain = Invoke-Git $paths.source @("rev-parse", "refs/remotes/origin/main")
        if ($main -notmatch $commitPattern -or $main -ne $originMain) {
            throw "The Windows source main ref does not exactly match origin/main."
        }
        $repository = [guid]::NewGuid().ToString().ToLowerInvariant()
        $feature = [guid]::NewGuid().ToString().ToLowerInvariant()
        $cloneErrorActionPreference = $ErrorActionPreference
        try {
            # Windows PowerShell 5 surfaces a native process's stderr as
            # ErrorRecord values. Git writes normal clone progress there, so
            # capture it and make the native exit code the sole verdict.
            $ErrorActionPreference = "Continue"
            $cloneOutput = @(& git clone --no-local --single-branch --branch main $paths.source $paths.proof 2>&1)
            $cloneExitCode = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $cloneErrorActionPreference
        }
        if ($cloneExitCode -ne 0) { throw "Git could not create the standalone disposable checkout." }
        Assert-NoReparseComponents $paths.proof $false
        Invoke-Git $paths.proof @("remote", "remove", "origin") | Out-Null
        Remove-BoundedCommitGraphCache $paths.proof
        Assert-SnapshotCompatibleObjectStore $paths.proof
        $marker = [ordered]@{
            schema_version = 1
            status = "local_coding_disposable_checkout"
            source_repository = $paths.source
            proof_repository = $paths.proof
            repository_id = $repository
            feature_id = $feature
            head_commit = $main
        }
        $markerPath = Join-Path $paths.proof ".git\assemblywright-local-coding-live-proof"
        [IO.File]::WriteAllText(
            $markerPath,
            ($marker | ConvertTo-Json -Compress),
            [Text.UTF8Encoding]::new($false)
        )
        Assert-ProofRepositoryClean $paths.proof $paths.source $repository $feature $main

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
        foreach ($kind in $grantKinds) {
            $grantScope = if ($kind -eq "registration") {
                $scopeDigest
            } else {
                @(Get-Sha256Bytes "assemblywright.local-coding-live.$kind.scope.v1`0$repository")
            }
            $approval = @(Get-Sha256Bytes "assemblywright.local-coding-live.$kind.owner-approval.v1`0$repository")
            $request = [ordered]@{
                schema_version = $ownerControlSchemaVersion
                expected_current_revision = 0
                expected_emergency_pause_revision = [UInt64]$status.owner_guidance.emergency_pause_revision
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
            if ($recorded.status -ne "recorded" -or [UInt64]$recorded.revision -ne 1) {
                throw "The Windows master did not record the exact $kind grant."
            }
        }
        $preflightRequest = [ordered]@{
            schema_version = $ownerControlSchemaVersion
            scope = $scope
            scope_sha256 = $scopeDigest
            registration_grant_revision = 1
            expected_emergency_pause_revision = [UInt64]$status.owner_guidance.emergency_pause_revision
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
            acceptance_criteria = @("execute the fixed contained-coding README fixture and retain no workspace")
            feature_kind = "contained_coding_live_proof"
            outcome = "prove one owner-approved snapshot-bound local-coding attempt"
        }
        $manifestJson = $manifest | ConvertTo-Json -Compress
        $approvedRequest = [ordered]@{
            schema_version = $ownerControlSchemaVersion
            expected_queue_revision = [UInt64]$status.queue_revision
            owner_control_designation_revision = $OwnerControlDesignationRevision
            emergency_pause_revision = [UInt64]$status.owner_guidance.emergency_pause_revision
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
            queue_revision = [UInt64]$status.queue_revision
            emergency_pause_revision = [UInt64]$status.owner_guidance.emergency_pause_revision
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
        $packetDigest = @(Get-Sha256Bytes "assemblywright.local-coding-live.work-packet.v1`0$FeatureId`0$packet")
        $dispatchRequest = [ordered]@{
            schema_version = $ownerControlSchemaVersion
            feature_id = $FeatureId
            specification_revision = 1
            expected_lifecycle_revision = [UInt64]$claim.lifecycle_revision
            feature_lease_id = $claim.lease_id
            snapshot_id = $claim.snapshot_id
            snapshot_sha256 = @($claim.snapshot_sha256)
            work_packet_sha256 = $packetDigest
            work_packet = [ordered]@{ packet_id = $packet; ordinal = 1; acceptance_criteria_count = 1 }
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
