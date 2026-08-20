param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Check", "Run")]
    [string]$Action,
    [string]$ExpectedSourceHead = "",
    [switch]$ConfirmAction
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$sourceRepository = "C:\Users\mike\Codex\Assemblywright"
$dataDir = "C:\Users\mike\AppData\Local\Assemblywright\master"
$databasePath = Join-Path $dataDir "master.sqlite3"
$serviceName = "AssemblywrightMaster"
$serviceOwner = "MIKE-PC\mike"
$serviceOwnerAliases = @($serviceOwner, ".\mike")
$serviceOwnerSid = ([Security.Principal.NTAccount]$serviceOwner).Translate(
    [Security.Principal.SecurityIdentifier]
).Value
$endpoint = "127.0.0.1:7791"
$remoteEndpoint = "100.64.23.14:7792"
$protocolVersion = 5
$masterSchemaVersion = 19
$projectionSchemaVersion = 9
$gitSource = "C:\Program Files\Git\cmd\git.exe"
$gitExecutableSha256 = "22fead8244ef3a7225fb800099a4e43eca8bcec0466774917669599c2f19a05a"
$sqliteLibrary = "C:\Windows\System32\winsqlite3.dll"
$cargoExecutable = "C:\Users\mike\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc\bin\cargo.exe"
$rustcExecutable = "C:\Users\mike\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc\bin\rustc.exe"
$cargoExecutableSha256 = "dc19c8e6d66802d120bf0696b1924b748bd90f3ca16f21391e54a290ff12b7c5"
$rustcExecutableSha256 = "e3ebbd547ea7b73c034d588ba569602b379f3b05ad1a3b5f8dcfab9d4478d74a"
$cargoHome = "C:\Users\mike\.cargo"
$msvcEnvironmentScript = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
$msvcEnvironmentSha256 = "6b516d8fcf543c14b2d861e1f45661e0029230fe0dc48e86ce78522801822209"
$commandExecutable = "C:\Windows\System32\cmd.exe"
$fsutilExecutable = "C:\Windows\System32\fsutil.exe"
$recoveryRoot = Join-Path $dataDir ".restart-recovery-control-v1"
$commitPattern = "^[0-9a-f]{40}$"
$shaPattern = "^[0-9a-f]{64}$"
$controlMutexName = "Global\Assemblywright.RestartRecovery.Control.v1"

foreach ($entry in @(Get-ChildItem Env: | Where-Object {
    $_.Name -like "GIT_*" -or $_.Name -like "ASSEMBLYWRIGHT_*" -or
    $_.Name -like "CARGO_*" -or $_.Name -like "RUST*" -or
    $_.Name -like "SCCACHE_*" -or $_.Name -like "HTTP_*" -or $_.Name -like "HTTPS_*" -or
    $_.Name -like "ALL_PROXY" -or $_.Name -like "NO_PROXY" -or
    $_.Name -like "VSCMD*" -or $_.Name -like "VSINSTALLDIR" -or $_.Name -like "VCINSTALLDIR" -or
    $_.Name -like "WindowsSdkDir" -or $_.Name -like "WindowsSDKVersion" -or
    $_.Name -like "UniversalCRTSdkDir" -or $_.Name -like "UCRTVersion" -or
    $_.Name -like "CL" -or $_.Name -like "_CL_" -or $_.Name -like "LINK" -or $_.Name -like "_LINK_" -or
    $_.Name -like "LIB" -or $_.Name -like "INCLUDE" -or $_.Name -like "LIBPATH" -or $_.Name -like "PATH"
})) {
    Remove-Item -LiteralPath "Env:$($entry.Name)"
}
$env:GIT_CONFIG_GLOBAL = "NUL"
$env:GIT_CONFIG_SYSTEM = "NUL"
$env:GIT_CONFIG_NOSYSTEM = "1"
$env:GIT_ATTR_NOSYSTEM = "1"
$env:GIT_TERMINAL_PROMPT = "0"
$env:GIT_OPTIONAL_LOCKS = "0"
$env:Path = "C:\Windows\System32;C:\Windows"

function Assert-NoReparseComponents {
    param([Parameter(Mandatory = $true)][string]$Path)
    if ($Path.StartsWith("\\?\", [StringComparison]::Ordinal) -or $Path -notmatch '^[A-Za-z]:\\') {
        throw "A fixed restart-recovery path used an unsupported namespace."
    }
    $full = [IO.Path]::GetFullPath($Path)
    $root = [IO.Path]::GetPathRoot($full)
    $current = $root
    foreach ($part in @($full.Substring($root.Length) -split "[\\/]" | Where-Object { $_.Length -gt 0 })) {
        $current = Join-Path $current $part
        if (-not (Test-Path -LiteralPath $current)) { throw "A fixed restart-recovery path component is missing." }
        if (((Get-Item -LiteralPath $current -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "A fixed restart-recovery path component is a reparse point."
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

function Set-OwnerSystemAcl {
    param([Parameter(Mandatory = $true)][string]$Path, [switch]$Directory)
    $owner = New-Object Security.Principal.NTAccount($serviceOwner)
    $system = New-Object Security.Principal.SecurityIdentifier([Security.Principal.WellKnownSidType]::LocalSystemSid, $null)
    $acl = if ($Directory) { New-Object Security.AccessControl.DirectorySecurity } else { New-Object Security.AccessControl.FileSecurity }
    $acl.SetOwner($owner)
    $acl.SetAccessRuleProtection($true, $false)
    $inheritance = if ($Directory) {
        [Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [Security.AccessControl.InheritanceFlags]::ObjectInherit
    } else { [Security.AccessControl.InheritanceFlags]::None }
    foreach ($identity in @($owner, $system)) {
        $rule = if ($Directory) {
            New-Object Security.AccessControl.FileSystemAccessRule($identity, [Security.AccessControl.FileSystemRights]::FullControl, $inheritance, [Security.AccessControl.PropagationFlags]::None, [Security.AccessControl.AccessControlType]::Allow)
        } else {
            New-Object Security.AccessControl.FileSystemAccessRule($identity, [Security.AccessControl.FileSystemRights]::FullControl, [Security.AccessControl.AccessControlType]::Allow)
        }
        [void]$acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}

function Assert-OwnerSystemFileIdentity {
    param([Parameter(Mandatory = $true)][string]$Path)
    Assert-NoReparseComponents $Path
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "A restart-recovery executable was not an ordinary file."
    }
    Assert-NoReparseComponents $fsutilExecutable
    $links = @(& $fsutilExecutable hardlink list $Path 2>$null | Where-Object { $_.Trim().Length -gt 0 })
    if ($LASTEXITCODE -ne 0 -or $links.Count -ne 1) { throw "A restart-recovery executable was not single-link." }
    $acl = Get-Acl -LiteralPath $Path
    if (-not $acl.AreAccessRulesProtected -or $acl.Owner -cne $serviceOwner) {
        throw "A restart-recovery executable did not have the exact protected owner."
    }
    $allowed = @($serviceOwner, "NT AUTHORITY\SYSTEM")
    foreach ($rule in @($acl.Access)) {
        if ($rule.IsInherited -or $rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or
            $allowed -cnotcontains [string]$rule.IdentityReference -or
            (($rule.FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -ne [Security.AccessControl.FileSystemRights]::FullControl)) {
            throw "A restart-recovery executable ACL was not limited to owner and SYSTEM full control."
        }
    }
    foreach ($identity in $allowed) {
        if (@($acl.Access | Where-Object { [string]$_.IdentityReference -ceq $identity }).Count -ne 1) {
            throw "A restart-recovery executable ACL principal set was not exact."
        }
    }
}

function Assert-PinnedOwnerToolIdentity {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$ExpectedSha256, [Parameter(Mandatory = $true)][string]$Label)
    if ($ExpectedSha256 -notmatch $shaPattern) {
        throw "Owner setup must pin the fixed $Label SHA-256 in the committed Windows control before Check or Run."
    }
    Assert-OwnerSystemFileIdentity $Path
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    if ($actual -cne $ExpectedSha256) { throw "The fixed $Label digest was not exact." }
    return $actual
}

function Assert-PinnedMsvcEnvironmentIdentity {
    Assert-NoReparseComponents $msvcEnvironmentScript
    Assert-NoReparseComponents $fsutilExecutable
    if ($msvcEnvironmentSha256 -notmatch $shaPattern) {
        throw "Owner setup must pin the fixed MSVC environment SHA-256 in the committed Windows control before Check or Run."
    }
    $item = Get-Item -LiteralPath $msvcEnvironmentScript -Force
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "The fixed MSVC environment was not an ordinary file."
    }
    $links = @(& $fsutilExecutable hardlink list $msvcEnvironmentScript 2>$null | Where-Object { $_.Trim().Length -gt 0 })
    if ($LASTEXITCODE -ne 0 -or $links.Count -ne 1) { throw "The fixed MSVC environment was not single-link." }
    $acl = Get-Acl -LiteralPath $msvcEnvironmentScript
    if ($acl.Owner -cne "NT SERVICE\TrustedInstaller" -and $acl.Owner -cne "BUILTIN\Administrators") {
        throw "The fixed MSVC environment owner was not protected."
    }
    $writeMask = [Security.AccessControl.FileSystemRights]::WriteData -bor [Security.AccessControl.FileSystemRights]::AppendData -bor
        [Security.AccessControl.FileSystemRights]::WriteAttributes -bor [Security.AccessControl.FileSystemRights]::WriteExtendedAttributes -bor
        [Security.AccessControl.FileSystemRights]::Delete -bor [Security.AccessControl.FileSystemRights]::ChangePermissions -bor
        [Security.AccessControl.FileSystemRights]::TakeOwnership
    foreach ($rule in @($acl.Access)) {
        $identity = [string]$rule.IdentityReference
        if ($rule.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and
            ($rule.FileSystemRights -band $writeMask) -ne 0 -and
            $identity -cne "NT SERVICE\TrustedInstaller" -and $identity -cne "BUILTIN\Administrators" -and
            $identity -cne "NT AUTHORITY\SYSTEM") {
            throw "The fixed MSVC environment granted write authority outside protected principals."
        }
    }
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $msvcEnvironmentScript).Hash.ToLowerInvariant()
    if ($actual -cne $msvcEnvironmentSha256) { throw "The fixed MSVC environment digest was not exact." }
    return $actual
}

function Invoke-WithRestartRecoveryControlLock {
    param([Parameter(Mandatory = $true)][scriptblock]$Operation)
    $currentSid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $systemSid = New-Object Security.Principal.SecurityIdentifier([Security.Principal.WellKnownSidType]::LocalSystemSid, $null)
    $security = New-Object Security.AccessControl.MutexSecurity
    $security.SetAccessRuleProtection($true, $false)
    foreach ($sid in @($currentSid, $systemSid)) {
        [void]$security.AddAccessRule((New-Object Security.AccessControl.MutexAccessRule(
            $sid, [Security.AccessControl.MutexRights]::FullControl, [Security.AccessControl.AccessControlType]::Allow
        )))
    }
    $createdNew = $false
    $mutex = $null
    $acquired = $false
    try {
        $mutex = [Threading.Mutex]::new($false, $controlMutexName, [ref]$createdNew, $security)
        try { $acquired = $mutex.WaitOne(0, $false) }
        catch [Threading.AbandonedMutexException] {
            $acquired = $true
            throw "A prior restart-recovery control operation requires owner reconciliation."
        }
        if (-not $acquired) { throw "Another restart-recovery control operation is active." }
        & $Operation
    } finally {
        if ($acquired -and $null -ne $mutex) { try { $mutex.ReleaseMutex() } catch { } }
        if ($null -ne $mutex) { $mutex.Dispose() }
    }
}

function Invoke-Git {
    param([string[]]$Arguments)
    Assert-NoReparseComponents $gitSource
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $gitSource).Hash.ToLowerInvariant() -cne $gitExecutableSha256) {
        throw "The fixed Windows Git executable identity was not exact."
    }
    $output = @(& $gitSource --no-replace-objects -c core.fsmonitor=false -c core.hooksPath=NUL -c core.attributesFile=NUL -C $sourceRepository @Arguments 2>&1)
    if ($LASTEXITCODE -ne 0) { throw "Git rejected the fixed restart-recovery observation." }
    return ($output -join "`n").Trim()
}

function Get-ExactSourceHead {
    Assert-NoReparseComponents $sourceRepository
    $head = Invoke-Git @("rev-parse", "refs/heads/main")
    $origin = Invoke-Git @("rev-parse", "refs/remotes/origin/main")
    $branch = Invoke-Git @("branch", "--show-current")
    $remote = Invoke-Git @("remote", "get-url", "origin")
    $status = Invoke-Git @("status", "--porcelain=v1", "--untracked-files=all")
    $tracked = @((Invoke-Git @("ls-files", "-v", "--")) -split "`r?`n" | Where-Object { $_.Length -gt 0 })
    if ($head -notmatch $commitPattern -or $head -cne $origin -or $branch -cne "main" -or
        ($remote -cne "https://github.com/malak333/Assemblywright" -and $remote -cne "https://github.com/malak333/Assemblywright.git") -or
        $status.Length -ne 0 -or $tracked.Count -eq 0 -or
        @($tracked | Where-Object { $_ -cnotmatch "^H " }).Count -ne 0) {
        throw "The Windows checkout is not exact clean main at origin/main with normal tracked-index state."
    }
    return $head
}

function Get-ExactMasterService {
    $service = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    if ($null -eq $service) { throw "The fixed Windows master service owner was not exact." }
    $reportedOwner = [string]$service.StartName
    if ($serviceOwnerAliases -cnotcontains $reportedOwner) {
        throw "The fixed Windows master service owner was not exact."
    }
    $normalizedOwner = if ($reportedOwner -ceq ".\mike") { $serviceOwner } else { $reportedOwner }
    try {
        $reportedOwnerSid = ([Security.Principal.NTAccount]$normalizedOwner).Translate(
            [Security.Principal.SecurityIdentifier]
        ).Value
    } catch {
        throw "The fixed Windows master service owner SID was unavailable."
    }
    if ($reportedOwnerSid -cne $serviceOwnerSid) {
        throw "The fixed Windows master service owner SID was not exact."
    }
    $match = [regex]::Match([string]$service.PathName, '^(?:"([^"]+assemblywright-master\.exe)"|(\S+assemblywright-master\.exe))(?=\s|$)')
    if (-not $match.Success) { throw "The fixed Windows master service image was not exact." }
    $captured = if ($match.Groups[1].Success) { $match.Groups[1].Value } else { $match.Groups[2].Value }
    $usesExtendedNamespace = $captured.StartsWith("\\?\", [StringComparison]::Ordinal)
    $actual = [IO.Path]::GetFullPath($captured)
    if ($usesExtendedNamespace) { $actual = $actual.Substring(4) }
    $expected = [IO.Path]::GetFullPath((Join-Path $sourceRepository "target\release\assemblywright-master.exe"))
    if ($actual -cne $expected) { throw "The fixed Windows master service image was not exact." }
    $argumentTail = ([string]$service.PathName).Substring($match.Length).Trim()
    $serviceDataDir = if ($usesExtendedNamespace) { "\\?\$dataDir" } else { $dataDir }
    $expectedTail = "--data-dir $serviceDataDir service-run --service-name $serviceName --bind $endpoint --service-identity $serviceOwner --remote-bind $remoteEndpoint"
    if ($argumentTail -cne $expectedTail) { throw "The fixed Windows master service data, bind, identity, or remote-bind arguments were not exact." }
    Assert-NoReparseComponents $expected
    [ordered]@{ Executable = $expected; ProcessId = [UInt32]$service.ProcessId; State = [string]$service.State }
}

function Wait-ExactServiceState {
    param([Parameter(Mandatory = $true)][ValidateSet("Running", "Stopped")][string]$State)
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds(45)
    do {
        $service = Get-ExactMasterService
        if ($service.State -ceq $State -and (($State -ceq "Stopped" -and [UInt64]$service.ProcessId -eq 0) -or
            ($State -ceq "Running" -and [UInt64]$service.ProcessId -ne 0))) { return $service }
        Start-Sleep -Milliseconds 250
    } while ([DateTimeOffset]::UtcNow -lt $deadline)
    throw "The fixed Windows master service did not reach its bounded expected state."
}

function Start-ExactServiceHealthy {
    Start-Service -Name $serviceName
    $service = Wait-ExactServiceState -State Running
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds(45)
    do {
        try {
            $health = Invoke-MasterHealth $service.Executable
            if ([UInt64]$health.process_id -eq [UInt64]$service.ProcessId) {
                return [ordered]@{ Service = $service; Health = $health }
            }
        } catch { }
        Start-Sleep -Milliseconds 250
        $service = Get-ExactMasterService
    } while ([DateTimeOffset]::UtcNow -lt $deadline)
    throw "The fixed Windows master service did not become healthy within the bounded readiness window."
}

function Stop-ExactService {
    $service = Get-ExactMasterService
    if ($service.State -ceq "Running") { Stop-Service -Name $serviceName -Force }
    [void](Wait-ExactServiceState -State Stopped)
}

function Assert-NoSQLiteSidecars {
    foreach ($suffix in @("-wal", "-shm", "-journal")) {
        if (Test-Path -LiteralPath "$databasePath$suffix") {
            throw "The stopped authoritative database retained a SQLite sidecar."
        }
    }
}

function Initialize-PrivateRecoveryRoot {
    if (Test-Path -LiteralPath $recoveryRoot) {
        throw "A prior restart-recovery restoration directory requires owner reconciliation."
    }
    [void](New-Item -ItemType Directory -Path $recoveryRoot)
    Set-OwnerSystemAcl -Path $recoveryRoot -Directory
    Assert-NoReparseComponents $recoveryRoot
}

function Invoke-ExactOfflineMasterBuild {
    $cargoSha = Assert-PinnedOwnerToolIdentity -Path $cargoExecutable -ExpectedSha256 $cargoExecutableSha256 -Label "Windows Cargo"
    $rustcSha = Assert-PinnedOwnerToolIdentity -Path $rustcExecutable -ExpectedSha256 $rustcExecutableSha256 -Label "Windows rustc"
    $msvcSha = Assert-PinnedMsvcEnvironmentIdentity
    Assert-NoReparseComponents $cargoHome
    Assert-NoReparseComponents $commandExecutable
    Assert-NoReparseComponents $sourceRepository
    $manifestPath = Join-Path $sourceRepository "Cargo.toml"
    Assert-NoReparseComponents $manifestPath
    foreach ($config in @(
        (Join-Path $cargoHome "config"), (Join-Path $cargoHome "config.toml"),
        (Join-Path $sourceRepository ".cargo\config"), (Join-Path $sourceRepository ".cargo\config.toml"),
        "C:\Users\mike\Codex\.cargo\config", "C:\Users\mike\Codex\.cargo\config.toml",
        "C:\Users\mike\.cargo\config", "C:\Users\mike\.cargo\config.toml",
        "C:\Users\.cargo\config", "C:\Users\.cargo\config.toml",
        "C:\.cargo\config", "C:\.cargo\config.toml"
    )) {
        if (Test-Path -LiteralPath $config) { throw "A Cargo configuration could redirect the fixed offline build." }
    }
    $cargoVersion = (& $cargoExecutable --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $cargoVersion -cne "cargo 1.95.0 (f2d3ce0bd 2026-03-21)") {
        throw "The fixed Windows Cargo version was not exact."
    }
    $environmentLines = @(& $commandExecutable /d /s /c "`"call `"$msvcEnvironmentScript`" >nul && set`"" 2>&1)
    if ($LASTEXITCODE -ne 0) { throw "The fixed MSVC environment setup failed." }
    $buildEnvironment = @{}
    foreach ($line in $environmentLines) {
        $separator = $line.IndexOf("=")
        if ($separator -gt 0) { $buildEnvironment[$line.Substring(0, $separator)] = $line.Substring($separator + 1) }
    }
    foreach ($required in @("PATH", "INCLUDE", "LIB", "LIBPATH")) {
        if (-not $buildEnvironment.ContainsKey($required) -or [string]$buildEnvironment[$required] -match '(?i)\\temp\\|\\tmp\\') {
            throw "The fixed MSVC environment was incomplete or unsafe."
        }
        Set-Item -LiteralPath "Env:$required" -Value ([string]$buildEnvironment[$required])
    }
    $env:CARGO_HOME = $cargoHome
    $env:CARGO_NET_OFFLINE = "true"
    $env:RUSTC = $rustcExecutable
    $env:RUSTC_WRAPPER = ""
    $env:RUSTC_WORKSPACE_WRAPPER = ""
    $target = Join-Path $recoveryRoot "build-target"
    $buildLog = Join-Path $recoveryRoot "cargo-build.log"
    Push-Location -LiteralPath $sourceRepository
    try {
        & $cargoExecutable build --manifest-path $manifestPath --locked --offline --release -p assemblywright-master --bin assemblywright-master --target-dir $target *> $buildLog
        if ($LASTEXITCODE -ne 0) { throw "The fixed exact-source offline master build failed." }
    } finally { Pop-Location }
    Set-OwnerSystemAcl -Path $buildLog
    if ((Get-Item -LiteralPath $buildLog).Length -gt 1048576) { throw "The fixed build log exceeded its private bound." }
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $cargoExecutable).Hash.ToLowerInvariant() -cne $cargoSha) {
        throw "The fixed Windows Cargo identity changed during the build."
    }
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $rustcExecutable).Hash.ToLowerInvariant() -cne $rustcSha -or
        (Get-FileHash -Algorithm SHA256 -LiteralPath $msvcEnvironmentScript).Hash.ToLowerInvariant() -cne $msvcSha) {
        throw "The fixed Windows compiler or MSVC environment identity changed during the build."
    }
    $built = Join-Path $target "release\assemblywright-master.exe"
    Set-OwnerSystemAcl -Path $built
    Assert-OwnerSystemFileIdentity $built
    [ordered]@{ Executable = $built; CargoSha256 = $cargoSha; RustcSha256 = $rustcSha; MsvcEnvironmentSha256 = $msvcSha; ExecutableSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $built).Hash.ToLowerInvariant() }
}

function Invoke-MasterHealth {
    param([string]$Executable)
    $raw = (& $Executable --data-dir $dataDir health --endpoint $endpoint | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $raw.Length -eq 0 -or $raw.Length -gt 8192) { throw "The Windows master health check failed." }
    $health = $raw | ConvertFrom-Json
    Assert-ExactKeys $health @(
        "status", "mode", "host_mode", "service_identity", "maintenance_active", "maintenance_reason",
        "emergency_paused", "protocol_version", "schema_version", "process_id", "started_at_ms",
        "startup_reconciliation", "state", "boundary"
    ) "Windows master health"
    if ([UInt64]$health.protocol_version -ne $protocolVersion -or [UInt64]$health.schema_version -ne $masterSchemaVersion -or
        $health.status -cne "ok" -or $health.mode -cne "developer_foundation" -or
        $health.host_mode -cne "windows_service" -or $health.service_identity -cne $serviceOwner -or
        $health.emergency_paused -ne $false -or $health.maintenance_active -ne $false -or
        [UInt64]$health.state.queued_steps -ne 0 -or [UInt64]$health.state.leased_steps -ne 0 -or
        [UInt64]$health.state.active_attempts -ne 0 -or [UInt64]$health.process_id -eq 0) {
        throw "The Windows master health bindings were invalid or distributed state was not empty."
    }
    return $health
}

function Get-ConveyorStatus {
    $tokenPath = Join-Path $dataDir "development.token"
    Assert-NoReparseComponents $tokenPath
    $token = (Get-Content -LiteralPath $tokenPath -Raw).Trim()
    if ($token.Length -lt 32 -or $token.Length -gt 256) { throw "The owner-loopback token shape was invalid." }
    $status = Invoke-RestMethod -Method Get -Uri "http://$endpoint/v1/feature-conveyor/status" -Headers @{ Authorization = "Bearer $token" }
    Assert-ExactKeys $status @(
        "schema_version", "queue_revision", "startup_quarantine_count", "counts_by_status",
        "visible_feature_count", "features_truncated", "features", "owner_guidance"
    ) "Feature Conveyor status"
    if ([UInt64]$status.schema_version -ne $projectionSchemaVersion -or [UInt64]$status.visible_feature_count -ne 0 -or
        $status.features_truncated -ne $false -or @($status.features).Count -ne 0 -or
        $status.owner_guidance.state -cne "idle" -or $status.owner_guidance.reason_code -cne "queue_empty" -or
        $null -ne $status.owner_guidance.feature_id -or
        [UInt64]$status.queue_revision -ne [UInt64]$status.owner_guidance.queue_revision) {
        throw "The Feature Conveyor was not empty and idle."
    }
    foreach ($property in @($status.counts_by_status.PSObject.Properties)) {
        if ([UInt64]$property.Value -ne 0) { throw "The Feature Conveyor retained nonterminal work." }
    }
    return $status
}

if ($null -eq ("Assemblywright.ReadOnlySqlite" -as [type])) {
    Assert-NoReparseComponents $sqliteLibrary
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
namespace Assemblywright {
  public static class ReadOnlySqlite {
    [DllImport(@"C:\Windows\System32\winsqlite3.dll", CallingConvention=CallingConvention.Cdecl)] static extern int sqlite3_open_v2(byte[] name, out IntPtr db, int flags, IntPtr vfs);
    [DllImport(@"C:\Windows\System32\winsqlite3.dll", CallingConvention=CallingConvention.Cdecl)] static extern int sqlite3_close(IntPtr db);
    [DllImport(@"C:\Windows\System32\winsqlite3.dll", CallingConvention=CallingConvention.Cdecl)] static extern int sqlite3_prepare_v2(IntPtr db, byte[] sql, int count, out IntPtr stmt, IntPtr tail);
    [DllImport(@"C:\Windows\System32\winsqlite3.dll", CallingConvention=CallingConvention.Cdecl)] static extern int sqlite3_step(IntPtr stmt);
    [DllImport(@"C:\Windows\System32\winsqlite3.dll", CallingConvention=CallingConvention.Cdecl)] static extern int sqlite3_finalize(IntPtr stmt);
    [DllImport(@"C:\Windows\System32\winsqlite3.dll", CallingConvention=CallingConvention.Cdecl)] static extern int sqlite3_column_count(IntPtr stmt);
    [DllImport(@"C:\Windows\System32\winsqlite3.dll", CallingConvention=CallingConvention.Cdecl)] static extern IntPtr sqlite3_column_text(IntPtr stmt, int col);
    static byte[] Utf8(string value) { var b=Encoding.UTF8.GetBytes(value+"\0"); return b; }
    public static string[] Query(string path, string sql) {
      IntPtr db=IntPtr.Zero, stmt=IntPtr.Zero; var rows=new List<string>();
      if (sqlite3_open_v2(Utf8(path), out db, 1, IntPtr.Zero)!=0) throw new InvalidOperationException("sqlite open rejected");
      try {
        if (sqlite3_prepare_v2(db, Utf8(sql), -1, out stmt, IntPtr.Zero)!=0) throw new InvalidOperationException("sqlite prepare rejected");
        int columns=sqlite3_column_count(stmt), result;
        while ((result=sqlite3_step(stmt))==100) {
          var values=new string[columns];
          for (int i=0;i<columns;i++) { var p=sqlite3_column_text(stmt,i); values[i]=p==IntPtr.Zero?"<null>":Marshal.PtrToStringAnsi(p); }
          rows.Add(String.Join("\u001f", values));
        }
        if (result!=101) throw new InvalidOperationException("sqlite step rejected");
        return rows.ToArray();
      } finally { if (stmt!=IntPtr.Zero) sqlite3_finalize(stmt); if (db!=IntPtr.Zero) sqlite3_close(db); }
    }
  }
}
'@
}

function Invoke-SqliteRows {
    param([string]$Path, [string]$Sql)
    Assert-NoReparseComponents $Path
    return @([Assemblywright.ReadOnlySqlite]::Query($Path, $Sql))
}

function Get-DatabaseSnapshot {
    Assert-NoReparseComponents $databasePath
    $integrity = @(Invoke-SqliteRows $databasePath "PRAGMA integrity_check")
    if ($integrity.Count -ne 1 -or $integrity[0] -cne "ok") { throw "The live master database failed SQLite integrity_check." }
    $evidence = @(Invoke-SqliteRows $databasePath "SELECT category,revision,evidence_id,origin,lower(hex(receipt_sha256)),observed_at_ms,emergency_pause_revision FROM feature_activation_evidence ORDER BY category,revision")
    $queue = @(Invoke-SqliteRows $databasePath "SELECT queue_revision FROM feature_conveyor_state WHERE singleton=1")
    $pause = @(Invoke-SqliteRows $databasePath "SELECT integer_value FROM master_metadata WHERE key='emergency_pause_revision'")
    $designation = @(Invoke-SqliteRows $databasePath "SELECT designation_revision FROM feature_owner_control_state WHERE singleton=1")
    if ($queue.Count -ne 1 -or $pause.Count -ne 1 -or $designation.Count -ne 1 -or
        $queue[0] -notmatch '^[0-9]+$' -or $pause[0] -notmatch '^[0-9]+$' -or $designation[0] -notmatch '^[0-9]+$') {
        throw "The frozen queue, pause, or designation singleton shape was invalid."
    }
    $continuity = @()
    $continuity += $queue
    $continuity += Invoke-SqliteRows $databasePath "SELECT key,integer_value FROM master_metadata WHERE key IN ('emergency_paused','emergency_pause_revision') ORDER BY key"
    $continuity += Invoke-SqliteRows $databasePath "SELECT designation_revision,coalesce(owner_bridge_device_id,''),coalesce(owner_bridge_registry_revision,0) FROM feature_owner_control_state WHERE singleton=1"
    $continuity += $evidence
    $activation = @(Invoke-SqliteRows $databasePath "SELECT activation_id,queue_revision,owner_control_designation_revision,emergency_pause_revision,repository_gate_evidence_id,restricted_worker_evidence_id,review_provider_evidence_id,github_publication_evidence_id,restart_recovery_evidence_id,control_event_streaming_evidence_id,activated_at_ms FROM feature_orchestration_activation WHERE singleton=1")
    $continuity += $activation
    $backups = @()
    foreach ($item in @(Get-ChildItem -LiteralPath $dataDir -Filter "master.pre-v*.sqlite3" -File | Sort-Object Name)) {
        if ($backups.Count -ge 32 -or $item.Name -notmatch '^master\.pre-v[0-9]+\.[0-9a-f-]{36}\.sqlite3$' -or
            ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "A migration backup had an unsupported identity or the bounded count was exceeded."
        }
        $backupIntegrity = @(Invoke-SqliteRows $item.FullName "PRAGMA integrity_check")
        if ($backupIntegrity.Count -ne 1 -or $backupIntegrity[0] -cne "ok") { throw "A migration backup failed SQLite integrity_check." }
        $backups += "$($item.Name)$([char]0x1f)$((Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash.ToLowerInvariant())"
    }
    $evidenceText = ($evidence -join "`n")
    $continuityText = ($continuity -join "`n")
    $backupText = ($backups -join "`n")
    [ordered]@{
        EvidenceSha256 = (Get-StringSha256 $evidenceText)
        ContinuitySha256 = (Get-StringSha256 $continuityText)
        BackupCount = [UInt64]$backups.Count
        BackupsSha256 = (Get-StringSha256 $backupText)
        ActivationStatus = if ($activation.Count -eq 0) { "inactive" } elseif ($activation.Count -eq 1) { "active" } else { throw "Activation singleton shape was invalid." }
        QueueRevision = [UInt64]$queue[0]
        EmergencyPauseRevision = [UInt64]$pause[0]
        DesignationRevision = [UInt64]$designation[0]
        RawContinuity = $continuityText
        RawBackups = $backupText
    }
}

function Get-StringSha256 {
    param([string]$Value)
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Value)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($algorithm.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant() }
    finally { $algorithm.Dispose() }
}

function Assert-SameSnapshot {
    param($Before, $After)
    if ($Before.EvidenceSha256 -cne $After.EvidenceSha256 -or
        $Before.ContinuitySha256 -cne $After.ContinuitySha256 -or
        [UInt64]$Before.BackupCount -ne [UInt64]$After.BackupCount -or
        $Before.BackupsSha256 -cne $After.BackupsSha256 -or
        $Before.ActivationStatus -cne $After.ActivationStatus -or
        [UInt64]$Before.QueueRevision -ne [UInt64]$After.QueueRevision -or
        [UInt64]$Before.EmergencyPauseRevision -ne [UInt64]$After.EmergencyPauseRevision -or
        [UInt64]$Before.DesignationRevision -ne [UInt64]$After.DesignationRevision -or
        $Before.RawContinuity -cne $After.RawContinuity -or $Before.RawBackups -cne $After.RawBackups) {
        throw "Queue, pause, activation evidence, activation, designation, or migration-backup continuity changed across restart."
    }
}

function Invoke-Check {
    if ($ConfirmAction -or $ExpectedSourceHead.Length -ne 0) { throw "Check accepts no confirmation or expected source HEAD." }
    if (Test-Path -LiteralPath $recoveryRoot) { throw "A prior restart-recovery restoration directory requires owner reconciliation." }
    $head = Get-ExactSourceHead
    $service = Get-ExactMasterService
    Assert-OwnerSystemFileIdentity $service.Executable
    $cargoSha = Assert-PinnedOwnerToolIdentity -Path $cargoExecutable -ExpectedSha256 $cargoExecutableSha256 -Label "Windows Cargo"
    $rustcSha = Assert-PinnedOwnerToolIdentity -Path $rustcExecutable -ExpectedSha256 $rustcExecutableSha256 -Label "Windows rustc"
    $msvcSha = Assert-PinnedMsvcEnvironmentIdentity
    $cargoVersion = (& $cargoExecutable --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $cargoVersion -cne "cargo 1.95.0 (f2d3ce0bd 2026-03-21)") {
        throw "The fixed Windows Cargo version was not exact."
    }
    $health = Invoke-MasterHealth $service.Executable
    [void](Get-ConveyorStatus)
    $database = Get-DatabaseSnapshot
    [ordered]@{
        schema_version = 1
        status = "restart_recovery_windows_check_passed"
        source_head = $head
        protocol_version = [UInt64]$health.protocol_version
        master_schema_version = [UInt64]$health.schema_version
        service_executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $service.Executable).Hash.ToLowerInvariant()
        windows_cargo_executable_sha256 = $cargoSha
        windows_rustc_executable_sha256 = $rustcSha
        windows_msvc_environment_sha256 = $msvcSha
        migration_backup_count = [UInt64]$database.BackupCount
    } | ConvertTo-Json -Compress
}

function Invoke-Run {
    if (-not $ConfirmAction) { throw "Run requires -ConfirmAction." }
    if ($ExpectedSourceHead -notmatch $commitPattern) { throw "Run requires the controller-reported expected source HEAD." }
    $head = Get-ExactSourceHead
    if ($head -cne $ExpectedSourceHead) { throw "The Windows source HEAD did not match the controller-reported expected HEAD." }
    $service = Get-ExactMasterService
    Assert-OwnerSystemFileIdentity $service.Executable
    $serviceSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $service.Executable).Hash.ToLowerInvariant()
    $preHealth = Invoke-MasterHealth $service.Executable
    $preStatus = Get-ConveyorStatus
    if ([UInt64]$service.ProcessId -ne [UInt64]$preHealth.process_id -or [UInt64]$service.ProcessId -eq 0 -or $service.State -cne "Running") {
        throw "The pre-restart SCM and health process identities were not exact."
    }

    $originalBackup = Join-Path $recoveryRoot "original-assemblywright-master.exe"
    $backupReady = $false
    $restorationComplete = $false
    try {
        Initialize-PrivateRecoveryRoot
        Copy-Item -LiteralPath $service.Executable -Destination $originalBackup
        Set-OwnerSystemAcl -Path $originalBackup
        Assert-OwnerSystemFileIdentity $originalBackup
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $originalBackup).Hash.ToLowerInvariant() -cne $serviceSha) {
            throw "The private original service recovery copy was not exact."
        }
        $backupReady = $true
        Stop-ExactService
        Assert-NoSQLiteSidecars
        $preDatabase = Get-DatabaseSnapshot
        $frozenDatabaseSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $databasePath).Hash.ToLowerInvariant()

        $build = Invoke-ExactOfflineMasterBuild
        if ($build.ExecutableSha256 -cne $serviceSha) {
            throw "The exact-source rebuilt service did not match the installed service executable."
        }
        if ((Get-ExactSourceHead) -cne $head) { throw "The Windows source identity changed during the fixed build." }
        Copy-Item -LiteralPath $build.Executable -Destination $service.Executable -Force
        Set-OwnerSystemAcl -Path $service.Executable
        Assert-OwnerSystemFileIdentity $service.Executable
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $service.Executable).Hash.ToLowerInvariant() -cne $serviceSha) {
            throw "The rebuilt service installation was not exact."
        }

        $recovered = Start-ExactServiceHealthy
        $recoveredStatus = Get-ConveyorStatus
        if ([UInt64]$recovered.Health.process_id -eq [UInt64]$preHealth.process_id -or
            [UInt64]$recovered.Health.started_at_ms -le [UInt64]$preHealth.started_at_ms) {
            throw "The rebuilt service did not produce one new healthy process."
        }
        Stop-ExactService
        Assert-NoSQLiteSidecars
        $postDatabase = Get-DatabaseSnapshot
        $postFrozenDatabaseSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $databasePath).Hash.ToLowerInvariant()
        if ($postFrozenDatabaseSha -cne $frozenDatabaseSha) {
            throw "The fully frozen authoritative database changed across recovery."
        }
        Assert-SameSnapshot $preDatabase $postDatabase

        Copy-Item -LiteralPath $originalBackup -Destination $service.Executable -Force
        Set-OwnerSystemAcl -Path $service.Executable
        Assert-OwnerSystemFileIdentity $service.Executable
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $service.Executable).Hash.ToLowerInvariant() -cne $serviceSha) {
            throw "The original service executable was not restored exactly."
        }
        $final = Start-ExactServiceHealthy
        $finalStatus = Get-ConveyorStatus
        if ([UInt64]$final.Health.process_id -eq [UInt64]$preHealth.process_id -or
            [UInt64]$final.Health.process_id -eq [UInt64]$recovered.Health.process_id -or
            [UInt64]$final.Health.started_at_ms -le [UInt64]$recovered.Health.started_at_ms) {
            throw "The restored service did not produce a distinct final healthy process."
        }
        foreach ($status in @($recoveredStatus, $finalStatus)) {
            if ([UInt64]$preStatus.queue_revision -ne [UInt64]$status.queue_revision -or
                [UInt64]$preStatus.owner_guidance.emergency_pause_revision -ne [UInt64]$status.owner_guidance.emergency_pause_revision -or
                [UInt64]$postDatabase.QueueRevision -ne [UInt64]$status.queue_revision -or
                [UInt64]$postDatabase.EmergencyPauseRevision -ne [UInt64]$status.owner_guidance.emergency_pause_revision) {
                throw "Queue or Emergency Pause revision changed across controlled recovery and restoration."
            }
        }
        if ((Get-ExactSourceHead) -cne $head) { throw "The Windows source identity changed during restart proof." }
        $restorationComplete = $true
        $observed = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        [ordered]@{
            schema_version = 1
            status = "restart_recovery_windows_live_passed"
            source_head = $head
            protocol_version = [UInt64]$final.Health.protocol_version
            master_schema_version = [UInt64]$final.Health.schema_version
            service_executable_sha256 = $serviceSha
            windows_cargo_executable_sha256 = $build.CargoSha256
            windows_rustc_executable_sha256 = $build.RustcSha256
            windows_msvc_environment_sha256 = $build.MsvcEnvironmentSha256
            frozen_database_sha256 = $frozenDatabaseSha
            pre_process_id = [UInt64]$preHealth.process_id
            post_process_id = [UInt64]$final.Health.process_id
            queue_revision = [UInt64]$postDatabase.QueueRevision
            emergency_pause_revision = [UInt64]$postDatabase.EmergencyPauseRevision
            owner_control_designation_revision = [UInt64]$postDatabase.DesignationRevision
            activation_status = $postDatabase.ActivationStatus
            activation_evidence_sha256 = $postDatabase.EvidenceSha256
            migration_backup_count = [UInt64]$postDatabase.BackupCount
            migration_backups_sha256 = $postDatabase.BackupsSha256
            continuity_sha256 = $postDatabase.ContinuitySha256
            observed_at_ms = $observed
        } | ConvertTo-Json -Compress
    } finally {
        if (-not $restorationComplete -and $backupReady) {
            try {
                Stop-ExactService
                Copy-Item -LiteralPath $originalBackup -Destination $service.Executable -Force
                Set-OwnerSystemAcl -Path $service.Executable
                Assert-OwnerSystemFileIdentity $service.Executable
                if ((Get-FileHash -Algorithm SHA256 -LiteralPath $service.Executable).Hash.ToLowerInvariant() -cne $serviceSha) {
                    throw "The fail-closed service restoration digest was not exact."
                }
                [void](Start-ExactServiceHealthy)
            } catch {
                throw "Restart-recovery failed and exact healthy service restoration also failed; owner reconciliation is required."
            }
        }
        if ($restorationComplete -and (Test-Path -LiteralPath $recoveryRoot)) {
            Remove-Item -LiteralPath $recoveryRoot -Recurse -Force
        }
    }
}

Invoke-WithRestartRecoveryControlLock {
    switch ($Action) {
        "Check" { Invoke-Check }
        "Run" { Invoke-Run }
    }
}
