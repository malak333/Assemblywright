param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("SelfTest", "Check", "Provision", "Run")]
    [string]$Action,
    [switch]$ConfirmAction
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$sourceRepository = "C:\Users\mike\Codex\Assemblywright"
$dataDir = Join-Path $env:LOCALAPPDATA "Assemblywright\master"
$publicationRoot = Join-Path $dataDir "github-publication"
$serviceName = "AssemblywrightMaster"
$endpoint = "127.0.0.1:7791"
$repository = "malak333/Assemblywright"
$baseBranch = "main"
$protocolVersion = 5
$masterSchemaVersion = 19
$projectionSchemaVersion = 9
$gitVersion = "2.55.0.windows.2"
$gitSource = "C:\Program Files\Git\cmd\git.exe"
$gitExecutableSha256 = "22fead8244ef3a7225fb800099a4e43eca8bcec0466774917669599c2f19a05a"
$ghVersion = "2.96.0"
$ghSource = "C:\Users\mike\AppData\Local\Microsoft\WinGet\Packages\GitHub.cli_Microsoft.Winget.Source_8wekyb3d8bbwe\bin\gh.exe"
$ghExecutableSha256 = "cd79f16203f1fbe56937c4c96e2b6eadd10549418dcb241d91576ac77af0ac8b"
$defaultGhHosts = "C:\Users\mike\AppData\Roaming\GitHub CLI\hosts.yml"
$shaPattern = "^[0-9a-f]{64}$"
$commitPattern = "^[0-9a-f]{40}$"
$controlMutexName = "Global\Assemblywright.GitHubPublication.Control.v1"
$requiredChecks = @(
    [ordered]@{
        id = "release-local"; workflow = "Assemblywright Release Local Gate"; context = "Release local gate"; app_id = 15368
        workflow_id = 282605278; workflow_path = ".github/workflows/release-local.yml"
        workflow_sha256 = "51e809a94f59193e213bdff6e49f3a86e612643f094e366055f42f8745026fd7"
    },
    [ordered]@{
        id = "protocol-windows"; workflow = "Assemblywright Windows Distributed Gate"; context = "Protocol, master, identity, mTLS, and SCM"; app_id = 15368
        workflow_id = 314849303; workflow_path = ".github/workflows/windows-protocol.yml"
        workflow_sha256 = "da1ebe295c34f3442ff2a3537ca617642c436b019cf5009843546fefb9f914a0"
    }
)

foreach ($entry in @(Get-ChildItem Env: | Where-Object {
    $_.Name -like "GIT_*" -or $_.Name -like "GH_*" -or $_.Name -like "GITHUB_*"
})) {
    Remove-Item -LiteralPath "Env:$($entry.Name)"
}
$env:GIT_CONFIG_GLOBAL = "NUL"
$env:GIT_CONFIG_SYSTEM = "NUL"
$env:GIT_CONFIG_NOSYSTEM = "1"
$env:GIT_ATTR_NOSYSTEM = "1"
$env:GIT_TERMINAL_PROMPT = "0"
$env:GIT_OPTIONAL_LOCKS = "0"

function Assert-NoReparseComponents {
    param([Parameter(Mandatory = $true)][string]$Path, [bool]$AllowMissingLeaf = $false)
    if ($Path.StartsWith("\\?\", [StringComparison]::Ordinal) -or $Path -notmatch '^[A-Za-z]:\\') {
        throw "A fixed GitHub-publication path used an unsupported namespace."
    }
    $full = [IO.Path]::GetFullPath($Path)
    $root = [IO.Path]::GetPathRoot($full)
    $current = $root
    $parts = @($full.Substring($root.Length) -split "[\\/]" | Where-Object { $_.Length -gt 0 })
    for ($index = 0; $index -lt $parts.Count; $index += 1) {
        $current = Join-Path $current $parts[$index]
        if (-not (Test-Path -LiteralPath $current)) {
            if ($AllowMissingLeaf -and $index -eq ($parts.Count - 1)) { return }
            throw "A fixed GitHub-publication path component is missing."
        }
        $item = Get-Item -LiteralPath $current -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "A fixed GitHub-publication path component is a reparse point."
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

function Set-PrivateDirectoryAcl {
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
        [void]$acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($principal, "FullControl", "Allow")))
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}

function Invoke-WithGitHubPublicationControlLock {
    param([Parameter(Mandatory = $true)][scriptblock]$Operation)
    $currentSid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $systemSid = New-Object Security.Principal.SecurityIdentifier(
        [Security.Principal.WellKnownSidType]::LocalSystemSid,
        $null
    )
    $security = New-Object Security.AccessControl.MutexSecurity
    $security.SetAccessRuleProtection($true, $false)
    foreach ($sid in @($currentSid, $systemSid)) {
        $rule = New-Object Security.AccessControl.MutexAccessRule(
            $sid,
            [Security.AccessControl.MutexRights]::FullControl,
            [Security.AccessControl.AccessControlType]::Allow
        )
        [void]$security.AddAccessRule($rule)
    }
    $createdNew = $false
    $mutex = $null
    $acquired = $false
    try {
        $mutex = [System.Threading.Mutex]::new($false, $controlMutexName, [ref]$createdNew, $security)
        try {
            $acquired = $mutex.WaitOne(0, $false)
        } catch [System.Threading.AbandonedMutexException] {
            $acquired = $true
            throw "A prior GitHub-publication control operation requires owner reconciliation."
        }
        if (-not $acquired) {
            throw "Another GitHub-publication control operation is active."
        }
        & $Operation
    } finally {
        if ($acquired -and $null -ne $mutex) {
            try { $mutex.ReleaseMutex() } catch { }
        }
        if ($null -ne $mutex) { $mutex.Dispose() }
    }
}

function Assert-PrivateBoundary {
    param([string]$Path)
    Assert-NoReparseComponents $Path $false
    $acl = Get-Acl -LiteralPath $Path
    if (-not $acl.AreAccessRulesProtected) { throw "The GitHub-publication configuration boundary inherited access." }
    $allowed = @([Security.Principal.WindowsIdentity]::GetCurrent().Name, "NT AUTHORITY\SYSTEM")
    foreach ($rule in @($acl.Access)) {
        if ($rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or
            $allowed -cnotcontains [string]$rule.IdentityReference) {
            throw "The GitHub-publication configuration boundary was not owner/SYSTEM-private."
        }
    }
}

function Get-ExactTool {
    param([string]$Path, [string]$Sha256, [string]$Kind)
    Assert-NoReparseComponents $Path $false
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    if ($actual -cne $Sha256) { throw "The fixed $Kind executable digest was not exact." }
    return $Path
}

function Invoke-Git {
    param([string[]]$Arguments, [string]$Executable = $gitSource)
    $output = @(& $Executable --no-replace-objects -c core.fsmonitor=false -c core.hooksPath=NUL -c core.autocrlf=true -C $sourceRepository @Arguments 2>&1)
    if ($LASTEXITCODE -ne 0) { throw "Git rejected the fixed GitHub-publication operation." }
    return ($output -join "`n").Trim()
}

function Assert-ToolVersions {
    param([string]$Git, [string]$Gh)
    $gitOutput = (& $Git --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $gitOutput -cne "git version $gitVersion") {
        throw "The fixed Git version was not exact."
    }
    $ghLines = @(& $Gh version)
    $ghExitCode = $LASTEXITCODE
    $ghOutput = if ($ghLines.Count -gt 0) { ([string]$ghLines[0]).Trim() } else { "" }
    if ($ghExitCode -ne 0 -or $ghOutput -cnotmatch "^gh version $([regex]::Escape($ghVersion)) ") {
        throw "The fixed GitHub CLI version was not exact."
    }
}

function Invoke-NativeExitCodeSilently {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Arguments
    )
    $ErrorActionPreference = "SilentlyContinue"
    $priorExitCode = $global:LASTEXITCODE
    try {
        $global:LASTEXITCODE = $null
        & $Executable @Arguments *> $null
        $observedExitCode = $global:LASTEXITCODE
    } finally {
        $global:LASTEXITCODE = $priorExitCode
    }
    if ($null -eq $observedExitCode) { return $null }
    return [int]$observedExitCode
}

function Assert-GhAuthentication {
    param([string]$Gh, [string]$ConfigDirectory)
    Assert-NoReparseComponents $ConfigDirectory $false
    $hosts = Join-Path $ConfigDirectory "hosts.yml"
    Assert-NoReparseComponents $hosts $false
    $hostsText = Get-Content -LiteralPath $hosts -Raw
    if ($hostsText -match '(?im)^\s*(oauth_token|token)\s*:' -or
        $hostsText -match '(?i)(ghp_|github_pat_|gho_|ghu_|ghs_|ghr_)') {
        throw "GitHub CLI reauthentication is required."
    }
    $prior = $env:GH_CONFIG_DIR
    try {
        $env:GH_CONFIG_DIR = $ConfigDirectory
        $ghAuthExitCode = Invoke-NativeExitCodeSilently `
            -Executable $Gh -Arguments @("auth", "status", "--hostname", "github.com")
        if ($null -eq $ghAuthExitCode -or $ghAuthExitCode -ne 0) {
            throw "GitHub CLI reauthentication is required."
        }
    } finally {
        if ($null -eq $prior) { Remove-Item Env:GH_CONFIG_DIR -ErrorAction SilentlyContinue } else { $env:GH_CONFIG_DIR = $prior }
    }
}

function Invoke-SelfTest {
    $cmd = Join-Path $env:SystemRoot "System32\cmd.exe"
    $where = Join-Path $env:SystemRoot "System32\where.exe"
    $success = Invoke-NativeExitCodeSilently -Executable $cmd `
        -Arguments @("/d", "/c", "echo expected 1>&2")
    $rejected = Invoke-NativeExitCodeSilently -Executable $where `
        -Arguments @("assemblywright-definitely-missing-executable.exe")
    $missing = Join-Path ([IO.Path]::GetTempPath()) "assemblywright-missing-gh-auth-status.exe"
    if (Test-Path -LiteralPath $missing) { throw "GitHub-auth self-test fixture already existed." }
    $notLaunched = Invoke-NativeExitCodeSilently -Executable $missing -Arguments @()
    if ($success -ne 0 -or $rejected -ne 1 -or $null -ne $notLaunched) {
        throw "GitHub-auth native exit-code self-test failed."
    }
    [ordered]@{
        schema_version = 1
        status = "github_publication_windows_self_test_passed"
    } | ConvertTo-Json -Compress
}

function Get-SourceHead {
    param([bool]$RequireOriginEquality = $true)
    Assert-NoReparseComponents $sourceRepository $false
    $head = Invoke-Git @("rev-parse", "refs/heads/main")
    $origin = Invoke-Git @("rev-parse", "refs/remotes/origin/main")
    $branch = Invoke-Git @("branch", "--show-current")
    $remote = Invoke-Git @("remote", "get-url", "origin")
    $unstaged = Invoke-Git @("diff", "--name-only", "--no-ext-diff", "--")
    $staged = Invoke-Git @("diff", "--cached", "--name-only", "--no-ext-diff", "--")
    $untracked = Invoke-Git @("ls-files", "--others", "--exclude-standard", "--")
    $tracked = @((Invoke-Git @("ls-files", "-v", "--")) -split "`r?`n" | Where-Object { $_.Length -gt 0 })
    if ($head -notmatch $commitPattern -or ($RequireOriginEquality -and $head -cne $origin) -or $branch -cne "main" -or
        ($remote -cne "https://github.com/malak333/Assemblywright" -and $remote -cne "https://github.com/malak333/Assemblywright.git") -or
        $unstaged.Length -ne 0 -or $staged.Length -ne 0 -or $untracked.Length -ne 0 -or
        $tracked.Count -eq 0 -or @($tracked | Where-Object { $_ -cnotmatch "^H " }).Count -ne 0) {
        throw "The Windows checkout is not exact clean main at origin/main with normal tracked-index state."
    }
    return $head
}

function Get-MasterExecutable {
    $service = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    if ($null -eq $service -or $service.StartName -notmatch "(^|\\)mike$") {
        throw "The fixed Windows master service identity is unavailable."
    }
    $match = [regex]::Match([string]$service.PathName, '^(?:"([^"]+assemblywright-master\.exe)"|(\S+assemblywright-master\.exe))(?=\s|$)')
    if (-not $match.Success) { throw "The Windows master service image path was not exact." }
    $captured = if ($match.Groups[1].Success) { $match.Groups[1].Value } else { $match.Groups[2].Value }
    $actual = [IO.Path]::GetFullPath($captured)
    if ($actual.StartsWith("\\?\", [StringComparison]::Ordinal)) { $actual = $actual.Substring(4) }
    $expected = [IO.Path]::GetFullPath((Join-Path $sourceRepository "target\release\assemblywright-master.exe"))
    if ($actual -cne $expected) { throw "The Windows master is not the exact source-checkout release executable." }
    Assert-NoReparseComponents $expected $false
    return $expected
}

function Invoke-MasterHealth {
    param([string]$Executable)
    $raw = (& $Executable --data-dir $dataDir health --endpoint $endpoint | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $raw.Length -eq 0 -or $raw.Length -gt 8192) { throw "The Windows master health check failed." }
    $health = $raw | ConvertFrom-Json
    if ([UInt64]$health.protocol_version -ne $protocolVersion -or [UInt64]$health.schema_version -ne $masterSchemaVersion -or
        $health.status -cne "ok" -or $health.emergency_paused -ne $false -or
        [UInt64]$health.state.queued_steps -ne 0 -or [UInt64]$health.state.leased_steps -ne 0 -or
        [UInt64]$health.state.active_attempts -ne 0) { throw "The Windows master health bindings were invalid." }
    return $health
}

function Assert-ConveyorQuiescent {
    $tokenPath = Join-Path $dataDir "development.token"
    Assert-NoReparseComponents $tokenPath $false
    $token = (Get-Content -LiteralPath $tokenPath -Raw).Trim()
    if ($token.Length -lt 32 -or $token.Length -gt 256) { throw "The owner-loopback token shape was invalid." }
    $status = Invoke-RestMethod -Method Get -Uri "http://$endpoint/v1/feature-conveyor/status" -Headers @{ Authorization = "Bearer $token" }
    if ([UInt64]$status.schema_version -ne $projectionSchemaVersion -or [UInt64]$status.visible_feature_count -ne 0 -or
        @($status.features).Count -ne 0 -or $status.owner_guidance.state -cne "idle" -or
        $status.owner_guidance.reason_code -cne "queue_empty" -or $null -ne $status.owner_guidance.feature_id) {
        throw "The Feature Conveyor was not quiescent before GitHub publication."
    }
    foreach ($property in @($status.counts_by_status.PSObject.Properties)) {
        if ([UInt64]$property.Value -ne 0) { throw "The Feature Conveyor retained nonterminal work before GitHub publication." }
    }
}

function Get-DeployedAssets {
    Assert-PrivateBoundary $publicationRoot
    $gh = $ghSource
    $git = $gitSource
    [void](Get-ExactTool $gh $ghExecutableSha256 "GitHub CLI")
    [void](Get-ExactTool $git $gitExecutableSha256 "Git")
    Assert-ToolVersions $git $gh
    Assert-GhAuthentication $gh (Join-Path $publicationRoot "gh-config")
    $configurationPath = Join-Path $publicationRoot "publication.json"
    Assert-NoReparseComponents $configurationPath $false
    $configuration = Get-Content -LiteralPath $configurationPath -Raw | ConvertFrom-Json
    Assert-ExactKeys $configuration @("schema_version", "enabled", "repository", "base_branch", "merge_strategy", "post_merge_gate", "required_checks", "master_executable_sha256", "gh_executable_sha256", "git_executable_sha256") "GitHub-publication configuration"
    $master = Get-MasterExecutable
    $masterSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $master).Hash.ToLowerInvariant()
    if ([UInt64]$configuration.schema_version -ne 1 -or $configuration.enabled -ne $true -or
        $configuration.repository -cne $repository -or $configuration.base_branch -cne $baseBranch -or
        $configuration.merge_strategy -cne "merge" -or $configuration.post_merge_gate -cne "release-local" -or
        [string]$configuration.master_executable_sha256 -cnotmatch $shaPattern -or
        [string]$configuration.master_executable_sha256 -cne $masterSha -or
        $configuration.gh_executable_sha256 -cne $ghExecutableSha256 -or
        $configuration.git_executable_sha256 -cne $gitExecutableSha256 -or @($configuration.required_checks).Count -ne 2) {
        throw "The deployed GitHub-publication configuration was not exact."
    }
    for ($index = 0; $index -lt 2; $index += 1) {
        Assert-ExactKeys $configuration.required_checks[$index] @("id", "workflow", "context", "app_id", "workflow_id", "workflow_path", "workflow_sha256") "Required check"
        foreach ($name in @("id", "workflow", "context", "app_id", "workflow_id", "workflow_path", "workflow_sha256")) {
            if ([string]$configuration.required_checks[$index].$name -cne [string]$requiredChecks[$index][$name]) {
                throw "The deployed GitHub required checks were not exact."
            }
        }
    }
    return [ordered]@{ Gh = $gh; Git = $git; Master = $master; MasterSha256 = $masterSha; ConfigurationPath = $configurationPath }
}

function Invoke-Check {
    $head = Get-SourceHead
    $master = Get-MasterExecutable
    $gh = Get-ExactTool $ghSource $ghExecutableSha256 "GitHub CLI"
    $git = Get-ExactTool $gitSource $gitExecutableSha256 "Git"
    Assert-ToolVersions $git $gh
    Assert-GhAuthentication $gh (Split-Path -Parent $defaultGhHosts)
    [void](Invoke-MasterHealth $master)
    Assert-ConveyorQuiescent
    if (Test-Path -LiteralPath $publicationRoot) { [void](Get-DeployedAssets) }
    [ordered]@{
        schema_version = 1
        status = "github_publication_windows_check_passed"
        source_head = $head
        repository = $repository
        base_branch = $baseBranch
        git_version = $gitVersion
        gh_version = $ghVersion
    } | ConvertTo-Json -Compress
}

function Restore-PreviousPublicationDeployment {
    param(
        [Parameter(Mandatory = $true)][string]$Master,
        [Parameter(Mandatory = $true)][string]$Previous,
        [Parameter(Mandatory = $true)][bool]$HadPrevious,
        [Parameter(Mandatory = $true)][bool]$SwapAttempted,
        [Parameter(Mandatory = $true)][string]$MasterBackup,
        [Parameter(Mandatory = $true)][string]$OriginalMasterSha256,
        [Parameter(Mandatory = $true)][bool]$BuildAttempted,
        [Parameter(Mandatory = $true)][bool]$ServiceWasRunning
    )
    try {
        $service = Get-Service -Name $serviceName
        if ($service.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Stopped) {
            Stop-Service -Name $serviceName -Force -ErrorAction Stop
        }
        if ($SwapAttempted) {
            if (Test-Path -LiteralPath $publicationRoot) {
                Assert-NoReparseComponents $publicationRoot $false
                Remove-Item -LiteralPath $publicationRoot -Recurse -Force -ErrorAction Stop
            }
            if ($HadPrevious) {
                if (-not (Test-Path -LiteralPath $Previous)) {
                    throw "The prior GitHub-publication deployment was unavailable during rollback."
                }
                Assert-NoReparseComponents $Previous $false
                Move-Item -LiteralPath $Previous -Destination $publicationRoot -ErrorAction Stop
            } elseif (Test-Path -LiteralPath $Previous) {
                throw "An unexpected prior GitHub-publication deployment appeared during rollback."
            }
        }
        if ($BuildAttempted) {
            if (-not (Test-Path -LiteralPath $MasterBackup)) {
                throw "The prior Windows master executable was unavailable during rollback."
            }
            Assert-NoReparseComponents $MasterBackup $false
            Copy-Item -LiteralPath $MasterBackup -Destination $Master -Force -ErrorAction Stop
            $restoredMasterSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $Master).Hash.ToLowerInvariant()
            if ($restoredMasterSha -cne $OriginalMasterSha256) {
                throw "The prior Windows master executable digest was not restored."
            }
        }
        if ($ServiceWasRunning) {
            Start-Service -Name $serviceName -ErrorAction Stop
            [void](Invoke-MasterHealth $Master)
            if ($HadPrevious) { [void](Get-DeployedAssets) }
        } else {
            $service = Get-Service -Name $serviceName
            if ($service.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Stopped) {
                throw "The Windows master service state changed during rollback."
            }
        }
        if (Test-Path -LiteralPath $MasterBackup) {
            Assert-NoReparseComponents $MasterBackup $false
            Remove-Item -LiteralPath $MasterBackup -Force -ErrorAction Stop
        }
    } catch {
        try {
            $service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
            if ($null -ne $service -and $service.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Stopped) {
                Stop-Service -Name $serviceName -Force -ErrorAction SilentlyContinue
            }
        } catch {
            # The fixed rollback error below is intentionally the only emitted detail.
        }
        throw "GitHub-publication provisioning rollback failed."
    }
}

function Invoke-Provision {
    if (-not $ConfirmAction) { throw "Provision requires -ConfirmAction." }
    $head = Get-SourceHead $false
    $master = Get-MasterExecutable
    $originalMasterSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $master).Hash.ToLowerInvariant()
    if ($originalMasterSha256 -notmatch $shaPattern) { throw "The Windows master executable digest was malformed." }
    $gh = Get-ExactTool $ghSource $ghExecutableSha256 "GitHub CLI"
    $git = Get-ExactTool $gitSource $gitExecutableSha256 "Git"
    Assert-ToolVersions $git $gh
    [void](Invoke-Git -Arguments @("fetch", "--no-tags", "origin", "+refs/heads/main:refs/remotes/origin/main") -Executable $git)
    $head = Get-SourceHead $true
    Assert-NoReparseComponents $defaultGhHosts $false
    $hostsText = Get-Content -LiteralPath $defaultGhHosts -Raw
    if ($hostsText -match '(?im)^\s*(oauth_token|token)\s*:' -or $hostsText -match '(?i)(ghp_|github_pat_|gho_|ghu_|ghs_|ghr_)') {
        throw "GitHub CLI reauthentication is required."
    }
    [void](Invoke-MasterHealth $master); Assert-ConveyorQuiescent
    $serviceWasRunning = (Get-Service -Name $serviceName).Status -eq [System.ServiceProcess.ServiceControllerStatus]::Running
    if (-not $serviceWasRunning) { throw "The Windows master service was not running before provisioning." }
    $hadPrevious = Test-Path -LiteralPath $publicationRoot
    if ($hadPrevious) { [void](Get-DeployedAssets) }
    $staging = "$publicationRoot.staging"; $previous = "$publicationRoot.previous"
    $masterBackup = Join-Path $dataDir "github-publication-master.previous"
    Assert-NoReparseComponents $staging $true
    if (Test-Path -LiteralPath $staging) {
        throw "A prior GitHub-publication staging artifact requires owner reconciliation."
    }
    foreach ($recoveryArtifact in @($previous, $masterBackup)) {
        Assert-NoReparseComponents $recoveryArtifact $true
        if (Test-Path -LiteralPath $recoveryArtifact) {
            throw "A prior GitHub-publication recovery artifact requires owner reconciliation."
        }
    }
    $stagingOwned = $false
    try {
        New-Item -ItemType Directory -Path $staging -ErrorAction Stop | Out-Null
        $stagingOwned = $true
        Set-PrivateDirectoryAcl $staging
        $ghConfig = Join-Path $staging "gh-config"
        New-Item -ItemType Directory -Path $ghConfig | Out-Null; Set-PrivateDirectoryAcl $ghConfig
        [IO.File]::WriteAllText((Join-Path $ghConfig "hosts.yml"), $hostsText, [Text.UTF8Encoding]::new($false))
        Set-PrivateFileAcl (Join-Path $ghConfig "hosts.yml")
        Assert-GhAuthentication $gh $ghConfig
        $swapAttempted = $false
        $buildAttempted = $false
        try {
            Copy-Item -LiteralPath $master -Destination $masterBackup -ErrorAction Stop
            Set-PrivateFileAcl $masterBackup
            Stop-Service -Name $serviceName -Force -ErrorAction Stop
            Push-Location -LiteralPath $sourceRepository
            $buildAttempted = $true
            try { & cargo build --locked --release -p assemblywright-master --bin assemblywright-master; if ($LASTEXITCODE -ne 0) { throw "The pinned Windows master build failed." } }
            finally { Pop-Location }
            $masterExecutableSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $master).Hash.ToLowerInvariant()
            if ($masterExecutableSha256 -notmatch $shaPattern) { throw "The built Windows master executable digest was malformed." }
            $configuration = [ordered]@{
                schema_version = 1; enabled = $true; repository = $repository; base_branch = $baseBranch
                merge_strategy = "merge"; post_merge_gate = "release-local"; required_checks = $requiredChecks
                master_executable_sha256 = $masterExecutableSha256
                gh_executable_sha256 = $ghExecutableSha256; git_executable_sha256 = $gitExecutableSha256
            }
            $configurationPath = Join-Path $staging "publication.json"
            [IO.File]::WriteAllText($configurationPath, ($configuration | ConvertTo-Json -Compress -Depth 5), [Text.UTF8Encoding]::new($false))
            Set-PrivateFileAcl $configurationPath
            if ($hadPrevious) {
                Move-Item -LiteralPath $publicationRoot -Destination $previous -ErrorAction Stop
                $swapAttempted = $true
            }
            $swapAttempted = $true
            Move-Item -LiteralPath $staging -Destination $publicationRoot -ErrorAction Stop
            $stagingOwned = $false
            Start-Service -Name $serviceName -ErrorAction Stop
            [void](Invoke-MasterHealth $master)
            [void](Get-DeployedAssets)
        } catch {
            try {
                Restore-PreviousPublicationDeployment -Master $master -Previous $previous `
                    -HadPrevious $hadPrevious -SwapAttempted $swapAttempted `
                    -MasterBackup $masterBackup -OriginalMasterSha256 $originalMasterSha256 `
                    -BuildAttempted $buildAttempted `
                    -ServiceWasRunning $serviceWasRunning
            } catch {
                throw "GitHub-publication provisioning rollback failed."
            }
            throw "GitHub-publication provisioning failed; the previous deployment and service state were restored."
        }
        try {
            if ($hadPrevious) {
                Assert-NoReparseComponents $previous $false
                Remove-Item -LiteralPath $previous -Recurse -Force -ErrorAction Stop
            }
            Assert-NoReparseComponents $masterBackup $false
            Remove-Item -LiteralPath $masterBackup -Force -ErrorAction Stop
        } catch {
            throw "GitHub-publication provisioning passed but recovery-artifact cleanup requires owner reconciliation."
        }
        [ordered]@{ schema_version = 1; status = "github_publication_windows_provisioned"; source_head = $head; repository = $repository; base_branch = $baseBranch } | ConvertTo-Json -Compress
    } finally {
        if ($stagingOwned -and (Test-Path -LiteralPath $staging)) {
            Assert-NoReparseComponents $staging $false
            Remove-Item -LiteralPath $staging -Recurse -Force
        }
    }
}

function Invoke-Run {
    if (-not $ConfirmAction) { throw "Run requires -ConfirmAction." }
    $head = Get-SourceHead $false
    $master = Get-MasterExecutable
    $health = Invoke-MasterHealth $master
    $assets = Get-DeployedAssets
    [void](Invoke-Git -Arguments @("fetch", "--no-tags", "origin", "+refs/heads/main:refs/remotes/origin/main") -Executable $assets.Git)
    $head = Get-SourceHead $true
    Assert-ConveyorQuiescent
    $configurationSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $assets.ConfigurationPath).Hash.ToLowerInvariant()
    $raw = (& $master --data-dir $dataDir github-publication-proof --confirm --expected-source-head $head | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $raw.Length -eq 0 -or $raw.Length -gt 4096) { throw "The fixed GitHub-publication proof command failed." }
    $proof = $raw | ConvertFrom-Json
    Assert-ExactKeys $proof @("schema_version", "status", "repository", "base_branch", "source_head", "publication_commit", "resulting_main_commit", "pull_request_number", "pull_request_url_sha256", "branch_name_sha256", "required_checks_sha256", "post_merge_checks_sha256", "master_executable_sha256", "observed_at_ms") "GitHub-publication live proof"
    if ([UInt64]$proof.schema_version -ne 1 -or $proof.status -cne "github_publication_live_proof_passed" -or
        $proof.repository -cne $repository -or $proof.base_branch -cne $baseBranch -or $proof.source_head -cne $head -or
        [string]$proof.publication_commit -cnotmatch $commitPattern -or [string]$proof.resulting_main_commit -cnotmatch $commitPattern -or
        $proof.source_head -ceq $proof.publication_commit -or $proof.source_head -ceq $proof.resulting_main_commit -or
        $proof.resulting_main_commit -ceq $proof.publication_commit -or
        [string]$proof.master_executable_sha256 -cne [string]$assets.MasterSha256 -or
        [UInt64]$proof.pull_request_number -lt 1 -or [UInt64]$proof.observed_at_ms -gt [UInt64]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() + 30000)) {
        throw "The GitHub-publication proof bindings were invalid."
    }
    foreach ($name in @("pull_request_url_sha256", "branch_name_sha256", "required_checks_sha256", "post_merge_checks_sha256", "master_executable_sha256")) {
        if ([string]$proof.$name -cnotmatch $shaPattern) { throw "The GitHub-publication proof digest was malformed." }
    }
    $remote = @(& $assets.Git ls-remote --exit-code "https://github.com/malak333/Assemblywright.git" "refs/heads/main" 2>&1)
    if ($LASTEXITCODE -ne 0 -or $remote.Count -ne 1 -or ($remote[0] -split "\s+")[0] -cne [string]$proof.resulting_main_commit) {
        throw "GitHub main did not equal the reported protected merge commit."
    }
    if ($configurationSha -cne (Get-FileHash -Algorithm SHA256 -LiteralPath $assets.ConfigurationPath).Hash.ToLowerInvariant() -or
        (Get-FileHash -Algorithm SHA256 -LiteralPath $assets.Gh).Hash.ToLowerInvariant() -cne $ghExecutableSha256 -or
        (Get-FileHash -Algorithm SHA256 -LiteralPath $assets.Git).Hash.ToLowerInvariant() -cne $gitExecutableSha256 -or
        (Get-FileHash -Algorithm SHA256 -LiteralPath $assets.Master).Hash.ToLowerInvariant() -cne [string]$assets.MasterSha256) {
        throw "The GitHub-publication configuration or executable identity changed during proof."
    }
    [ordered]@{
        schema_version = 1; status = "github_publication_windows_live_passed"; source_head = $head
        protocol_version = [UInt64]$health.protocol_version; master_schema_version = [UInt64]$health.schema_version
        repository = $proof.repository; base_branch = $proof.base_branch; publication_commit = $proof.publication_commit
        resulting_main_commit = $proof.resulting_main_commit; pull_request_number = [UInt64]$proof.pull_request_number
        pull_request_url_sha256 = $proof.pull_request_url_sha256; branch_name_sha256 = $proof.branch_name_sha256
        required_checks_sha256 = $proof.required_checks_sha256; post_merge_checks_sha256 = $proof.post_merge_checks_sha256
        master_executable_sha256 = $proof.master_executable_sha256
        git_version = $gitVersion; git_executable_sha256 = $gitExecutableSha256
        gh_version = $ghVersion; gh_executable_sha256 = $ghExecutableSha256; observed_at_ms = [UInt64]$proof.observed_at_ms
    } | ConvertTo-Json -Compress
}

Invoke-WithGitHubPublicationControlLock {
    switch ($Action) {
        "SelfTest" { Invoke-SelfTest }
        "Check" { Invoke-Check }
        "Provision" { Invoke-Provision }
        "Run" { Invoke-Run }
    }
}
