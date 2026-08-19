param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Check", "Provision", "Run")]
    [string]$Action,
    [switch]$ConfirmAction
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$sourceRepository = "C:\Users\mike\Codex\Assemblywright"
$dataDir = Join-Path $env:LOCALAPPDATA "Assemblywright\master"
$providerRoot = Join-Path $dataDir "review-provider"
$serviceName = "AssemblywrightMaster"
$endpoint = "127.0.0.1:7791"
$providerId = "openai.codex"
$modelId = "gpt-5.6-sol"
$codexVersion = "0.148.0"
$protocolVersion = 5
$masterSchemaVersion = 19
$shaPattern = "^[0-9a-f]{64}$"
$commitPattern = "^[0-9a-f]{40}$"

foreach ($entry in @(Get-ChildItem Env: | Where-Object { $_.Name -like "GIT_*" })) {
    Remove-Item -LiteralPath "Env:$($entry.Name)"
}
$env:GIT_CONFIG_GLOBAL = "NUL"
$env:GIT_CONFIG_SYSTEM = "NUL"
$env:GIT_CONFIG_NOSYSTEM = "1"
$env:GIT_ATTR_NOSYSTEM = "1"
$env:GIT_TERMINAL_PROMPT = "0"
$env:GIT_OPTIONAL_LOCKS = "0"

function Invoke-Git {
    param([string[]]$Arguments)
    $output = @(& git --no-replace-objects -c core.fsmonitor=false -c core.hooksPath=NUL -c core.autocrlf=true -C $sourceRepository @Arguments 2>&1)
    if ($LASTEXITCODE -ne 0) { throw "Git rejected the fixed review-provider operation." }
    return ($output -join "`n").Trim()
}

function Assert-NoReparseComponents {
    param([Parameter(Mandatory = $true)][string]$Path, [bool]$AllowMissingLeaf = $false)
    if ($Path.StartsWith("\\?\", [StringComparison]::Ordinal)) {
        throw "A fixed review-provider path used an unsupported extended namespace."
    }
    $candidate = $Path
    if ($candidate -notmatch '^[A-Za-z]:\\') { throw "A fixed review-provider path was not drive-qualified." }
    $full = [IO.Path]::GetFullPath($candidate)
    $root = [IO.Path]::GetPathRoot($full)
    if ([string]::IsNullOrEmpty($root)) { throw "A fixed review-provider path was not rooted." }
    $current = $root
    $parts = @($full.Substring($root.Length) -split "[\\/]" | Where-Object { $_.Length -gt 0 })
    for ($index = 0; $index -lt $parts.Count; $index += 1) {
        $current = Join-Path $current $parts[$index]
        if (-not (Test-Path -LiteralPath $current)) {
            if ($AllowMissingLeaf -and $index -eq ($parts.Count - 1)) { return }
            throw "A fixed review-provider path component is missing."
        }
        $item = Get-Item -LiteralPath $current -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "A fixed review-provider path component is a reparse point."
        }
    }
}

function Assert-ExactKeys {
    param($Value, [string[]]$Keys, [string]$Label)
    if ($Value -is [System.Collections.IDictionary]) {
        $actual = @($Value.Keys | ForEach-Object { [string]$_ } | Sort-Object -CaseSensitive)
    } else {
        $actual = @($Value.PSObject.Properties.Name | Sort-Object -CaseSensitive)
    }
    $expected = @($Keys | Sort-Object -CaseSensitive)
    if ($actual.Count -ne $expected.Count) { throw "$Label returned an unexpected JSON shape." }
    for ($index = 0; $index -lt $expected.Count; $index += 1) {
        if ($actual[$index] -cne $expected[$index]) { throw "$Label returned an unexpected JSON shape." }
    }
}

function Get-SourceHead {
    Assert-NoReparseComponents $sourceRepository $false
    $head = Invoke-Git @("rev-parse", "refs/heads/main")
    $origin = Invoke-Git @("rev-parse", "refs/remotes/origin/main")
    $branch = Invoke-Git @("branch", "--show-current")
    $unstaged = Invoke-Git @("diff", "--name-only", "--no-ext-diff", "--")
    $staged = Invoke-Git @("diff", "--cached", "--name-only", "--no-ext-diff", "--")
    $untracked = Invoke-Git @("ls-files", "--others", "--exclude-standard", "--")
    $trackedOutput = Invoke-Git @("ls-files", "-v", "--")
    $tracked = @($trackedOutput -split "`r?`n" | Where-Object { $_.Length -gt 0 })
    $hasAbnormalTrackedEntry = @($tracked | Where-Object { $_ -cnotmatch "^H " }).Count -ne 0
    $invalidSource = $head -notmatch $commitPattern
    $invalidOrigin = $head -cne $origin
    $invalidBranch = $branch -cne "main"
    $dirtyWorktree = $unstaged.Length -ne 0 -or $staged.Length -ne 0 -or $untracked.Length -ne 0
    $emptyOrAbnormalIndex = $tracked.Count -eq 0 -or $hasAbnormalTrackedEntry
    if ($invalidSource -or $invalidOrigin -or $invalidBranch -or
        $dirtyWorktree -or $emptyOrAbnormalIndex) {
        throw "The Windows checkout is not exact clean main at origin/main with normal tracked-index state."
    }
    return $head
}

function Get-MasterExecutable {
    $service = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    if ($null -eq $service -or $service.StartName -notmatch "(^|\\)mike$") {
        throw "The fixed Windows master service identity is unavailable."
    }
    $match = [regex]::Match(
        [string]$service.PathName,
        '^(?:"([^"]+assemblywright-master\.exe)"|(\S+assemblywright-master\.exe))(?=\s|$)'
    )
    if (-not $match.Success) { throw "The Windows master service image path was not exact." }
    $capturedExecutable = if ($match.Groups[1].Success) {
        $match.Groups[1].Value
    } else {
        $match.Groups[2].Value
    }
    $executable = [IO.Path]::GetFullPath($capturedExecutable)
    $comparisonExecutable = if ($executable.StartsWith("\\?\", [StringComparison]::Ordinal)) {
        $executable.Substring(4)
    } else {
        $executable
    }
    $expected = [IO.Path]::GetFullPath((Join-Path $sourceRepository "target\release\assemblywright-master.exe"))
    if ($comparisonExecutable -cne $expected) { throw "The Windows master is not the exact source-checkout release executable." }
    Assert-NoReparseComponents $expected $false
    return $expected
}

function Invoke-MasterHealth {
    param([string]$Executable)
    $raw = (& $Executable --data-dir $dataDir health --endpoint $endpoint | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $raw.Length -eq 0 -or $raw.Length -gt 8192) { throw "The Windows master health check failed." }
    $health = $raw | ConvertFrom-Json
    if ([UInt64]$health.protocol_version -ne $protocolVersion -or
        [UInt64]$health.schema_version -ne $masterSchemaVersion -or
        $health.status -cne "ok" -or $health.emergency_paused -ne $false -or
        [UInt64]$health.state.queued_steps -ne 0 -or [UInt64]$health.state.leased_steps -ne 0 -or
        [UInt64]$health.state.active_attempts -ne 0) {
        throw "The Windows master health bindings were invalid."
    }
    return $health
}

function Assert-ConveyorQuiescent {
    $tokenPath = Join-Path $dataDir "development.token"
    Assert-NoReparseComponents $tokenPath $false
    $token = (Get-Content -LiteralPath $tokenPath -Raw).Trim()
    if ($token.Length -lt 32 -or $token.Length -gt 256) { throw "The owner-loopback token shape was invalid." }
    $status = Invoke-RestMethod -Method Get -Uri "http://$endpoint/v1/feature-conveyor/status" -Headers @{ Authorization = "Bearer $token" }
    if ([UInt64]$status.schema_version -ne 9 -or [UInt64]$status.visible_feature_count -ne 0 -or
        @($status.features).Count -ne 0 -or $null -ne $status.owner_guidance.active_feature_id) {
        throw "The Feature Conveyor was not quiescent before provider provisioning."
    }
    foreach ($property in @($status.counts_by_status.PSObject.Properties)) {
        if ([UInt64]$property.Value -ne 0) { throw "The Feature Conveyor retained nonterminal work before provider provisioning." }
    }
}

function Set-PrivateAcl {
    param([string]$Path)
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($principal in @($identity, "NT AUTHORITY\SYSTEM")) {
        $rule = New-Object Security.AccessControl.FileSystemAccessRule(
            $principal, "FullControl", "ContainerInherit,ObjectInherit", "None", "Allow"
        )
        [void]$acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}

function Set-PrivateFileAcl {
    param([string]$Path)
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    $acl = New-Object Security.AccessControl.FileSecurity
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($principal in @($identity, "NT AUTHORITY\SYSTEM")) {
        $rule = New-Object Security.AccessControl.FileSystemAccessRule($principal, "FullControl", "Allow")
        [void]$acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}

function Get-CodexNativeExecutable {
    $npmRoot = (& npm root -g).Trim()
    if ($LASTEXITCODE -ne 0 -or $npmRoot.Length -eq 0) { throw "The pinned npm root was unavailable." }
    $packageRoot = Join-Path $npmRoot "@openai\codex-win32-x64"
    Assert-NoReparseComponents $packageRoot $false
    $matches = @(Get-ChildItem -LiteralPath $packageRoot -Filter "codex.exe" -File -Recurse)
    if ($matches.Count -ne 1) { throw "The pinned Windows Codex native executable was not unique." }
    Assert-NoReparseComponents $matches[0].FullName $false
    return $matches[0].FullName
}

function Invoke-Check {
    $head = Get-SourceHead
    $master = Get-MasterExecutable
    $auth = "C:\Users\mike\.codex\auth.json"
    Assert-NoReparseComponents $auth $false
    if ((Get-Item -LiteralPath $auth -Force).Length -le 0) { throw "The fixed Codex authentication file is empty." }
    [ordered]@{
        schema_version = 1
        status = "review_provider_windows_check_passed"
        source_head = $head
        service_executable = $master
        provider_id = $providerId
        model_id = $modelId
        codex_version = $codexVersion
    } | ConvertTo-Json -Compress
}

function Invoke-Provision {
    if (-not $ConfirmAction) { throw "Provision requires -ConfirmAction." }
    $head = Get-SourceHead
    $master = Get-MasterExecutable
    & npm install -g "@openai/codex@$codexVersion" --ignore-scripts --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw "The pinned Codex installation failed." }
    $codex = Get-CodexNativeExecutable
    $version = (& $codex --version).Trim()
    if ($LASTEXITCODE -ne 0 -or $version -notmatch "(^| )$([regex]::Escape($codexVersion))($| )") {
        throw "The native Codex version was not the fixed pinned version."
    }
    Assert-NoReparseComponents "C:\Users\mike\.codex" $false
    Assert-NoReparseComponents "C:\Users\mike\.codex\auth.json" $false
    Set-PrivateAcl "C:\Users\mike\.codex"
    Set-PrivateFileAcl "C:\Users\mike\.codex\auth.json"
    [void](Invoke-MasterHealth $master)
    Assert-ConveyorQuiescent
    Stop-Service -Name $serviceName -Force
    try {
        Push-Location -LiteralPath $sourceRepository
        try {
            & cargo build --locked --release -p assemblywright-master --bins
            if ($LASTEXITCODE -ne 0) { throw "The pinned Windows master/provider build failed." }
        } finally {
            Pop-Location
        }
        $adapter = Join-Path $sourceRepository "target\release\assemblywright-review-provider.exe"
        $schema = Join-Path $sourceRepository "crates\assemblywright-master\resources\review-output-schema.json"
        Assert-NoReparseComponents $adapter $false
        Assert-NoReparseComponents $schema $false
        $staging = "$providerRoot.staging"
        $previous = "$providerRoot.previous"
        foreach ($bounded in @($staging, $previous)) {
            Assert-NoReparseComponents $bounded $true
            if (Test-Path -LiteralPath $bounded) { Remove-Item -LiteralPath $bounded -Recurse -Force }
        }
        New-Item -ItemType Directory -Path $staging | Out-Null
        Set-PrivateAcl $staging
        Copy-Item -LiteralPath $adapter -Destination (Join-Path $staging "review-provider.exe")
        Copy-Item -LiteralPath $codex -Destination (Join-Path $staging "codex.exe")
        Copy-Item -LiteralPath $schema -Destination (Join-Path $staging "review-output-schema.json")
        $codexSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $staging "codex.exe")).Hash.ToLowerInvariant()
        $schemaSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $staging "review-output-schema.json")).Hash.ToLowerInvariant()
        $adapterSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $staging "review-provider.exe")).Hash.ToLowerInvariant()
        $serviceSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $master).Hash.ToLowerInvariant()
        $configuration = [ordered]@{
            schema_version = 2
            provider_id = $providerId
            model_id = $modelId
            max_input_tokens = 262144
            review_provider_executable_sha256 = $adapterSha
            codex_adapter = [ordered]@{
                kind = "codex_exec_v1"
                codex_home = "C:\Users\mike\.codex"
                codex_executable_sha256 = $codexSha
                output_schema_sha256 = $schemaSha
            }
        }
        [IO.File]::WriteAllText((Join-Path $staging "provider.json"), ($configuration | ConvertTo-Json -Compress -Depth 5), [Text.UTF8Encoding]::new($false))
        $deployment = [ordered]@{
            schema_version = 1
            source_head = $head
            service_executable_sha256 = $serviceSha
            review_provider_executable_sha256 = $adapterSha
            codex_executable_sha256 = $codexSha
            output_schema_sha256 = $schemaSha
            codex_version = $codexVersion
        }
        [IO.File]::WriteAllText((Join-Path $staging "deployment.json"), ($deployment | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
        $priorProviderMoved = $false
        if (Test-Path -LiteralPath $providerRoot) {
            Move-Item -LiteralPath $providerRoot -Destination $previous
            $priorProviderMoved = $true
        }
        try {
            Move-Item -LiteralPath $staging -Destination $providerRoot
        } catch {
            if ($priorProviderMoved -and -not (Test-Path -LiteralPath $providerRoot)) {
                Move-Item -LiteralPath $previous -Destination $providerRoot
            }
            throw
        }
    } finally {
        Start-Service -Name $serviceName
    }
    [void](Invoke-MasterHealth $master)
    if (Test-Path -LiteralPath "$providerRoot.previous") {
        Remove-Item -LiteralPath "$providerRoot.previous" -Recurse -Force
    }
    [ordered]@{
        schema_version = 1
        status = "review_provider_windows_provisioned"
        source_head = $head
        provider_id = $providerId
        model_id = $modelId
        codex_version = $codexVersion
    } | ConvertTo-Json -Compress
}

function Invoke-Run {
    if (-not $ConfirmAction) { throw "Run requires -ConfirmAction." }
    $head = Get-SourceHead
    $master = Get-MasterExecutable
    $health = Invoke-MasterHealth $master
    Assert-NoReparseComponents $providerRoot $false
    $configuration = Get-Content -LiteralPath (Join-Path $providerRoot "provider.json") -Raw | ConvertFrom-Json
    $configurationFileSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $providerRoot "provider.json")).Hash.ToLowerInvariant()
    Assert-ExactKeys $configuration @(
        "schema_version", "provider_id", "model_id", "max_input_tokens",
        "review_provider_executable_sha256", "codex_adapter"
    ) "Review-provider configuration"
    if ([UInt64]$configuration.schema_version -ne 2 -or
        $configuration.provider_id -cne $providerId -or $configuration.model_id -cne $modelId -or
        [UInt64]$configuration.max_input_tokens -ne 262144) {
        throw "The deployed review-provider configuration was not the pinned schema-v2 selection."
    }
    Assert-ExactKeys $configuration.codex_adapter @(
        "kind", "codex_home", "codex_executable_sha256", "output_schema_sha256"
    ) "Review-provider Codex adapter configuration"
    if ($configuration.codex_adapter.kind -cne "codex_exec_v1" -or
        $configuration.codex_adapter.codex_home -cne "C:\Users\mike\.codex") {
        throw "The deployed Codex adapter authority was not exact."
    }
    $deployment = Get-Content -LiteralPath (Join-Path $providerRoot "deployment.json") -Raw | ConvertFrom-Json
    $deploymentFileSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $providerRoot "deployment.json")).Hash.ToLowerInvariant()
    Assert-ExactKeys $deployment @(
        "schema_version", "source_head", "service_executable_sha256",
        "review_provider_executable_sha256", "codex_executable_sha256",
        "output_schema_sha256", "codex_version"
    ) "Review-provider deployment binding"
    $adapterSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $providerRoot "review-provider.exe")).Hash.ToLowerInvariant()
    $codexSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $providerRoot "codex.exe")).Hash.ToLowerInvariant()
    $schemaSha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $providerRoot "review-output-schema.json")).Hash.ToLowerInvariant()
    $serviceSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $master).Hash.ToLowerInvariant()
    if ([UInt64]$deployment.schema_version -ne 1 -or $deployment.source_head -cne $head -or
        $deployment.codex_version -cne $codexVersion -or
        $serviceSha -cne [string]$deployment.service_executable_sha256 -or
        $adapterSha -cne [string]$deployment.review_provider_executable_sha256 -or
        $codexSha -cne [string]$deployment.codex_executable_sha256 -or
        $schemaSha -cne [string]$deployment.output_schema_sha256 -or
        $adapterSha -cne [string]$configuration.review_provider_executable_sha256 -or
        $codexSha -cne [string]$configuration.codex_adapter.codex_executable_sha256 -or
        $schemaSha -cne [string]$configuration.codex_adapter.output_schema_sha256) {
        throw "The deployed review-provider asset digests drifted."
    }
    $raw = (& $master --data-dir $dataDir review-provider-proof --confirm | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $raw.Length -eq 0 -or $raw.Length -gt 4096) {
        throw "The selected review-provider proof command failed."
    }
    $proof = $raw | ConvertFrom-Json
    Assert-ExactKeys $proof @(
        "schema_version", "status", "provider_id", "model_id",
        "approval_packet_sha256", "approval_output_sha256",
        "rejection_packet_sha256", "rejection_output_sha256", "observed_at_ms"
    ) "Review-provider live proof"
    if ([UInt64]$proof.schema_version -ne 1 -or
        $proof.status -cne "review_provider_live_proof_passed" -or
        $proof.provider_id -cne $providerId -or $proof.model_id -cne $modelId -or
        [UInt64]$proof.observed_at_ms -gt [UInt64]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() + 30000)) {
        throw "The selected review-provider proof bindings were invalid."
    }
    foreach ($name in @("approval_packet_sha256", "approval_output_sha256", "rejection_packet_sha256", "rejection_output_sha256")) {
        if ([string]$proof.$name -cnotmatch $shaPattern) { throw "The review-provider proof digest was malformed." }
    }
    Assert-NoReparseComponents $providerRoot $false
    if ($configurationFileSha -cne (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $providerRoot "provider.json")).Hash.ToLowerInvariant() -or
        $deploymentFileSha -cne (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $providerRoot "deployment.json")).Hash.ToLowerInvariant() -or
        $serviceSha -cne (Get-FileHash -Algorithm SHA256 -LiteralPath $master).Hash.ToLowerInvariant() -or
        $adapterSha -cne (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $providerRoot "review-provider.exe")).Hash.ToLowerInvariant() -or
        $codexSha -cne (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $providerRoot "codex.exe")).Hash.ToLowerInvariant() -or
        $schemaSha -cne (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $providerRoot "review-output-schema.json")).Hash.ToLowerInvariant()) {
        throw "The selected provider or service identity changed during live proof."
    }
    [ordered]@{
        schema_version = 1
        status = "review_provider_windows_live_passed"
        source_head = $head
        protocol_version = [UInt64]$health.protocol_version
        master_schema_version = [UInt64]$health.schema_version
        service_executable_sha256 = $serviceSha
        review_provider_executable_sha256 = $adapterSha
        codex_executable_sha256 = $codexSha
        output_schema_sha256 = $schemaSha
        provider_id = $proof.provider_id
        model_id = $proof.model_id
        approval_packet_sha256 = $proof.approval_packet_sha256
        approval_output_sha256 = $proof.approval_output_sha256
        rejection_packet_sha256 = $proof.rejection_packet_sha256
        rejection_output_sha256 = $proof.rejection_output_sha256
        observed_at_ms = [UInt64]$proof.observed_at_ms
    } | ConvertTo-Json -Compress
}

switch ($Action) {
    "Check" { Invoke-Check }
    "Provision" { Invoke-Provision }
    "Run" { Invoke-Run }
}
