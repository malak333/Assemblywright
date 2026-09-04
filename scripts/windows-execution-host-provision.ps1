[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('DryRun', 'Check', 'Apply', 'SelfTest')]
    [string]$Mode,
    [switch]$ConfirmStoppedServiceCeremony
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$masterService = 'AssemblywrightMaster'
$brokerService = 'AssemblywrightBroker'
$executorService = 'AssemblywrightExecutor'
$programData = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
if ([string]::IsNullOrWhiteSpace($programData)) { throw 'Canonical Common Application Data is unavailable.' }
$programFiles = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
if ([string]::IsNullOrWhiteSpace($programFiles)) { throw 'Canonical Program Files is unavailable.' }
$installRoot = Join-Path $programFiles 'Assemblywright'
$binRoot = Join-Path $installRoot 'bin'
$masterImage = Join-Path $binRoot 'assemblywright-master.exe'
$brokerImage = Join-Path $binRoot 'assemblywright-broker.exe'
$executorImage = Join-Path $binRoot 'assemblywright-executor.exe'
$releaseManifestPath = Join-Path $installRoot 'execution-host-release.json'
$diskReserveBytes = [UInt64]536870912
$hostRoot = Join-Path (Join-Path $programData 'Assemblywright') 'execution-host'
$configRoot = Join-Path $hostRoot 'config'
$stateRoot = Join-Path $hostRoot 'state'
$auditRoot = Join-Path $hostRoot 'audit'
$updateRoot = Join-Path $hostRoot 'update-staging'
$reserveRoot = Join-Path $hostRoot 'reservation'
$masterDataRoot = Join-Path $stateRoot 'master'
$brokerConfig = Join-Path $configRoot 'broker.json'
$executorConfig = Join-Path $configRoot 'executor.json'
$policyPath = Join-Path $configRoot 'host-security-policy.json'
$reservePath = Join-Path $reserveRoot 'control-plane.reserve'
$policyRegistry = 'HKLM:\SOFTWARE\Assemblywright\ExecutionHost'
$ifeoRoot = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options'

function Assert-Elevated {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Execution-host provisioning requires an elevated owner PowerShell.'
    }
}

function Assert-OrdinaryPath {
    param([Parameter(Mandatory = $true)][string]$Path, [bool]$AllowMissingLeaf = $false)
    if ($Path.StartsWith('\\?\', [StringComparison]::Ordinal) -or $Path -notmatch '^[A-Za-z]:\\') {
        throw 'A protected execution-host path used an unsupported namespace.'
    }
    $full = [IO.Path]::GetFullPath($Path)
    $root = [IO.Path]::GetPathRoot($full)
    $current = $root
    $parts = @($full.Substring($root.Length) -split '[\\/]' | Where-Object { $_.Length -gt 0 })
    for ($index = 0; $index -lt $parts.Count; $index += 1) {
        $current = Join-Path $current $parts[$index]
        if (-not (Test-Path -LiteralPath $current)) {
            if ($AllowMissingLeaf -and $index -eq ($parts.Count - 1)) { return }
            throw 'A protected execution-host path component is missing.'
        }
        $item = Get-Item -LiteralPath $current -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw 'A protected execution-host path component is a reparse point.'
        }
        if (-not $item.PSIsContainer) {
            $links = @(& fsutil.exe hardlink list $current 2>$null)
            if ($LASTEXITCODE -ne 0 -or $links.Count -ne 1) {
                throw 'A protected execution-host file was not ordinary and single-link.'
            }
        }
    }
}

function Get-ServiceRecord {
    param([Parameter(Mandatory = $true)][string]$Name)
    return Get-CimInstance Win32_Service -Filter "Name='$Name'" -ErrorAction SilentlyContinue
}

function Get-ServiceSid {
    param([Parameter(Mandatory = $true)][string]$Name)
    try {
        return ([Security.Principal.NTAccount]::new("NT SERVICE\$Name")).Translate(
            [Security.Principal.SecurityIdentifier]
        ).Value
    } catch {
        throw 'A required Windows service SID was unavailable.'
    }
}

function Invoke-Sc {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)
    $output = @(& sc.exe @Arguments 2>&1)
    if ($LASTEXITCODE -ne 0) { throw 'Windows Service Control Manager rejected the bounded operation.' }
    return $output
}

function Assert-ExactServiceDacl {
    param(
        [Parameter(Mandatory = $true)][string]$Sddl,
        [Parameter(Mandatory = $true)][string]$OwnSid,
        [Parameter(Mandatory = $true)][string]$FeatureSid
    )
    $descriptor = [Security.AccessControl.RawSecurityDescriptor]::new($Sddl)
    if ($null -eq $descriptor.DiscretionaryAcl) {
        throw 'A service definition lacked its protected DACL.'
    }
    [UInt32]$serviceAllAccess = 0x000F01FF
    [UInt32]$serviceObserve = 0x0002018D
    $expected = @(
        @{ Sid = $FeatureSid; Kind = 'AccessDenied'; Mask = $serviceAllAccess },
        @{ Sid = 'S-1-5-18'; Kind = 'AccessAllowed'; Mask = $serviceAllAccess },
        @{ Sid = 'S-1-5-32-544'; Kind = 'AccessAllowed'; Mask = $serviceAllAccess },
        @{ Sid = $OwnSid; Kind = 'AccessAllowed'; Mask = $serviceObserve }
    )
    $aces = @($descriptor.DiscretionaryAcl)
    if ($aces.Count -ne $expected.Count) {
        throw 'A service definition DACL was not the exact protected contract.'
    }
    for ($index = 0; $index -lt $expected.Count; $index += 1) {
        $ace = $aces[$index]
        if ($ace -isnot [Security.AccessControl.CommonAce]) {
            throw 'A service definition DACL contained an unsupported ACE type.'
        }
        $sid = $ace.SecurityIdentifier.Value
        $kind = [string]$ace.AceQualifier
        if ($sid -cne [string]$expected[$index].Sid -or
            $kind -cne [string]$expected[$index].Kind -or
            [UInt32]$ace.AccessMask -ne [UInt32]$expected[$index].Mask -or
            $ace.AceFlags -ne [Security.AccessControl.AceFlags]::None) {
            throw ("A service definition DACL was not the exact protected contract. " +
                "index=$index; identity=$sid; kind=$kind; mask=$([UInt32]$ace.AccessMask); flags=$($ace.AceFlags).")
        }
    }
}

function Assert-ServiceStopped {
    param([Parameter(Mandatory = $true)]$Service)
    if ($null -eq $Service -or $Service.State -cne 'Stopped') {
        throw 'The stopped-service ceremony requires every existing execution-host service to be stopped.'
    }
}

function Assert-ProductionServiceBinding {
    param(
        [Parameter(Mandatory = $true)]$Service,
        [Parameter(Mandatory = $true)][string]$ExpectedAccount,
        [Parameter(Mandatory = $true)][string]$ExpectedImage,
        [Parameter(Mandatory = $true)][string]$ExpectedCommand
    )
    if ($null -eq $Service -or $Service.StartName -cne $ExpectedAccount) {
        throw 'Owner-account service deployment is not the production execution-host substrate.'
    }
    $match = [regex]::Match([string]$Service.PathName, '^(?:"([^"]+)"|(\S+))(?=\s|$)')
    if (-not $match.Success) { throw 'A production service image binding was malformed.' }
    $image = if ($match.Groups[1].Success) { $match.Groups[1].Value } else { $match.Groups[2].Value }
    $full = [IO.Path]::GetFullPath($image)
    if ($full -cne [IO.Path]::GetFullPath($ExpectedImage)) {
        throw 'The service image was not the exact fixed production executable.'
    }
    Assert-OrdinaryPath $full $false
    $imagePath = [string](Get-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$($Service.Name)" -Name ImagePath).ImagePath
    if ($imagePath -cne $ExpectedCommand) {
        throw 'The service command was not the exact canonical role-specific argv.'
    }
}

function Assert-ServicePersistenceContract {
    param([Parameter(Mandatory = $true)][string]$Name, [Parameter(Mandatory = $true)][string[]]$RequiredPrivileges)
    $path = "HKLM:\SYSTEM\CurrentControlSet\Services\$Name"
    $values = Get-ItemProperty -LiteralPath $path
    if ([int]$values.Type -ne 16 -or [int]$values.Start -ne 4 -or [int]$values.ErrorControl -ne 1) {
        throw 'The service type, noninteractive mode, start mode, or error control drifted.'
    }
    foreach ($forbidden in @('DependOnService', 'FailureActions', 'FailureCommand', 'ServiceLaunchProtected')) {
        if ($null -ne $values.PSObject.Properties[$forbidden] -and $values.$forbidden) {
            throw 'The service contained unexpected dependency, recovery, or launch persistence.'
        }
    }
    if (Test-Path -LiteralPath (Join-Path $path 'TriggerInfo')) {
        throw 'The service contained an unexpected trigger.'
    }
    $actualPrivileges = @($values.RequiredPrivileges)
    if ($actualPrivileges.Count -ne $RequiredPrivileges.Count) { throw 'The service required-privilege set drifted.' }
    for ($index = 0; $index -lt $RequiredPrivileges.Count; $index += 1) {
        if ($actualPrivileges[$index] -cne $RequiredPrivileges[$index]) { throw 'The service required-privilege set drifted.' }
    }
}

function Assert-AllServicesStopped {
    foreach ($name in @($masterService, $brokerService, $executorService)) {
        Assert-ServiceStopped (Get-ServiceRecord $name)
    }
}

function Invoke-StoppedMutationCluster {
    param([Parameter(Mandatory = $true)][scriptblock]$Operation)
    Assert-AllServicesStopped
    & $Operation
    Assert-AllServicesStopped
}

function Assert-ExactKeys {
    param($Value, [string[]]$Keys, [string]$Label)
    $actual = @($Value.PSObject.Properties.Name | Sort-Object -CaseSensitive)
    $expected = @($Keys | Sort-Object -CaseSensitive)
    if ($actual.Count -ne $expected.Count) { throw "$Label had an unexpected shape." }
    for ($index = 0; $index -lt $expected.Count; $index += 1) {
        if ($actual[$index] -cne $expected[$index]) { throw "$Label had an unexpected shape." }
    }
}

function Get-VerifiedReleaseManifest {
    param(
        [Parameter(Mandatory = $true)][string]$MasterSid,
        [Parameter(Mandatory = $true)][string]$BrokerSid,
        [Parameter(Mandatory = $true)][string]$FeatureSid
    )
    foreach ($entry in @(
        @{ Path = $installRoot; ExecutorReadable = $true },
        @{ Path = $binRoot; ExecutorReadable = $true },
        @{ Path = $releaseManifestPath; ExecutorReadable = $false },
        @{ Path = $masterImage; ExecutorReadable = $false },
        @{ Path = $brokerImage; ExecutorReadable = $false },
        @{ Path = $executorImage; ExecutorReadable = $true },
        @{ Path = $brokerConfig; ExecutorReadable = $false },
        @{ Path = $executorConfig; ExecutorReadable = $true }
    )) {
        Assert-ProtectedAcl $entry.Path $MasterSid $BrokerSid $FeatureSid $entry.ExecutorReadable
    }
    foreach ($path in @($brokerConfig, $executorConfig)) {
        if (-not (Get-Item -LiteralPath $path -Force).IsReadOnly) {
            throw 'A service-host configuration was not read-only.'
        }
    }
    $manifest = Get-Content -LiteralPath $releaseManifestPath -Raw | ConvertFrom-Json
    Assert-ExactKeys $manifest @(
        'schema_version', 'signer_subject', 'signer_thumbprint',
        'master_sha256', 'broker_sha256', 'executor_sha256',
        'broker_config_sha256', 'executor_config_sha256'
    ) 'Execution-host release manifest'
    if ([UInt64]$manifest.schema_version -ne 1 -or
        [string]$manifest.signer_subject -notmatch '^.{1,512}$' -or
        [string]$manifest.signer_thumbprint -notmatch '^[0-9A-F]{40,64}$') {
        throw 'The execution-host release signing identity was malformed.'
    }
    foreach ($digest in @([string]$manifest.broker_config_sha256, [string]$manifest.executor_config_sha256)) {
        if ($digest -cnotmatch '^[0-9a-f]{64}$') { throw 'A service-host configuration digest was malformed.' }
    }
    if ((Get-FileHash -LiteralPath $brokerConfig -Algorithm SHA256).Hash.ToLowerInvariant() -cne [string]$manifest.broker_config_sha256 -or
        (Get-FileHash -LiteralPath $executorConfig -Algorithm SHA256).Hash.ToLowerInvariant() -cne [string]$manifest.executor_config_sha256) {
        throw 'A service-host configuration digest drifted.'
    }
    foreach ($entry in @(
        @{ Path = $masterImage; Digest = [string]$manifest.master_sha256 },
        @{ Path = $brokerImage; Digest = [string]$manifest.broker_sha256 },
        @{ Path = $executorImage; Digest = [string]$manifest.executor_sha256 }
    )) {
        if ($entry.Digest -cnotmatch '^[0-9a-f]{64}$' -or
            (Get-FileHash -LiteralPath $entry.Path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.Digest) {
            throw 'A fixed execution-host image digest drifted.'
        }
        $signature = Get-AuthenticodeSignature -LiteralPath $entry.Path
        if ($signature.Status -ne [Management.Automation.SignatureStatus]::Valid -or
            $null -eq $signature.SignerCertificate -or
            $signature.SignerCertificate.Subject -cne [string]$manifest.signer_subject -or
            $signature.SignerCertificate.Thumbprint -cne [string]$manifest.signer_thumbprint) {
            throw 'A fixed execution-host image lacked the exact valid signing identity.'
        }
    }
    return $manifest
}

function Get-ExpectedServiceCommands {
    param([Parameter(Mandatory = $true)]$Manifest)
    return @{
        Master = ('"{0}" --data-dir "{1}" service-run --service-name AssemblywrightMaster --bind 127.0.0.1:7791 --service-identity LocalSystem' -f $masterImage, $masterDataRoot)
        Broker = ('"{0}" --service-host --service-name AssemblywrightBroker --config "{1}" --config-sha256 {2}' -f $brokerImage, $brokerConfig, [string]$Manifest.broker_config_sha256)
        Executor = ('"{0}" --service-host --service-name AssemblywrightExecutor --config "{1}" --config-sha256 {2}' -f $executorImage, $executorConfig, [string]$Manifest.executor_config_sha256)
    }
}

function Set-ServiceSecurity {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$OwnSid,
        [Parameter(Mandatory = $true)][string]$FeatureSid
    )
    # Feature execution cannot query, start, stop, reconfigure, delete, or take ownership.
    $sddl = "D:(D;;GA;;;$FeatureSid)(A;;GA;;;SY)(A;;GA;;;BA)(A;;CCLCSWLOCRRC;;;$OwnSid)"
    [void](Invoke-Sc @('sdset', $Name, $sddl))
}

function Set-ProtectedDirectoryAcl {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$MasterSid,
        [Parameter(Mandatory = $true)][string]$BrokerSid,
        [Parameter(Mandatory = $true)][string]$FeatureSid,
        [bool]$ExecutorReadable = $false
    )
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetOwner([Security.Principal.NTAccount]::new('NT AUTHORITY\SYSTEM'))
    $acl.SetAccessRuleProtection($true, $false)
    $inheritance = [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
    $none = [Security.AccessControl.PropagationFlags]::None
    $denyRights = if ($ExecutorReadable) {
        [Security.AccessControl.FileSystemRights]'Write, Delete, ChangePermissions, TakeOwnership'
    } else {
        [Security.AccessControl.FileSystemRights]::FullControl
    }
    $deny = New-Object Security.AccessControl.FileSystemAccessRule(
        ([Security.Principal.SecurityIdentifier]::new($FeatureSid)),
        $denyRights,
        $inheritance, $none, [Security.AccessControl.AccessControlType]::Deny
    )
    [void]$acl.AddAccessRule($deny)
    if ($ExecutorReadable) {
        [void]$acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
            ([Security.Principal.SecurityIdentifier]::new($FeatureSid)),
            [Security.AccessControl.FileSystemRights]::ReadAndExecute,
            [Security.AccessControl.AccessControlType]::Allow
        )))
    }
    foreach ($sid in @('S-1-5-18', 'S-1-5-32-544', $MasterSid, $BrokerSid)) {
        $allow = New-Object Security.AccessControl.FileSystemAccessRule(
            ([Security.Principal.SecurityIdentifier]::new($sid)),
            [Security.AccessControl.FileSystemRights]::FullControl,
            $inheritance, $none, [Security.AccessControl.AccessControlType]::Allow
        )
        [void]$acl.AddAccessRule($allow)
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}

function Set-ProtectedFileAcl {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$MasterSid,
        [Parameter(Mandatory = $true)][string]$BrokerSid,
        [Parameter(Mandatory = $true)][string]$FeatureSid,
        [bool]$ExecutorReadable = $false
    )
    $acl = New-Object Security.AccessControl.FileSecurity
    $acl.SetOwner([Security.Principal.NTAccount]::new('NT AUTHORITY\SYSTEM'))
    $acl.SetAccessRuleProtection($true, $false)
    $denyRights = if ($ExecutorReadable) {
        [Security.AccessControl.FileSystemRights]'Write, Delete, ChangePermissions, TakeOwnership'
    } else {
        [Security.AccessControl.FileSystemRights]::FullControl
    }
    $deny = New-Object Security.AccessControl.FileSystemAccessRule(
        ([Security.Principal.SecurityIdentifier]::new($FeatureSid)),
        $denyRights,
        [Security.AccessControl.AccessControlType]::Deny
    )
    [void]$acl.AddAccessRule($deny)
    if ($ExecutorReadable) {
        [void]$acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
            ([Security.Principal.SecurityIdentifier]::new($FeatureSid)),
            [Security.AccessControl.FileSystemRights]::ReadAndExecute,
            [Security.AccessControl.AccessControlType]::Allow
        )))
    }
    foreach ($sid in @('S-1-5-18', 'S-1-5-32-544', $MasterSid, $BrokerSid)) {
        $allow = New-Object Security.AccessControl.FileSystemAccessRule(
            ([Security.Principal.SecurityIdentifier]::new($sid)),
            [Security.AccessControl.FileSystemRights]::FullControl,
            [Security.AccessControl.AccessControlType]::Allow
        )
        [void]$acl.AddAccessRule($allow)
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}

function Get-ResourcePolicy {
    $computer = Get-CimInstance Win32_ComputerSystem
    $processors = [int]$computer.NumberOfLogicalProcessors
    $memory = [UInt64]$computer.TotalPhysicalMemory
    if ($processors -lt 1 -or $memory -lt 2147483648) {
        throw 'The host cannot retain the minimum control-plane resource headroom.'
    }
    $cpuRate = if ($processors -eq 1) { 5000 } else { 9000 }
    $memoryReserve = [UInt64][Math]::Max(1073741824, [Math]::Floor($memory * 0.10))
    return [ordered]@{
        schema_version = 1
        windows_executor_job_required = $true
        executor_cpu_rate_hard_cap = $cpuRate
        executor_commit_limit_bytes = $memory - $memoryReserve
        executor_active_process_limit = 128
        control_plane_reserved_logical_processors = 1
        control_plane_reserved_commit_bytes = $memoryReserve
        control_plane_reserved_process_slots = 32
        control_plane_disk_reserve_bytes = $diskReserveBytes
        effect_activation_requires_exact_policy_attestation = $true
    }
}

function Set-ControlPlanePriority {
    param([Parameter(Mandatory = $true)][string]$ImageName)
    $path = Join-Path (Join-Path $ifeoRoot $ImageName) 'PerfOptions'
    New-Item -Path $path -Force | Out-Null
    New-ItemProperty -Path $path -Name CpuPriorityClass -PropertyType DWord -Value 3 -Force | Out-Null
    New-ItemProperty -Path $path -Name IoPriority -PropertyType DWord -Value 2 -Force | Out-Null
    New-ItemProperty -Path $path -Name PagePriority -PropertyType DWord -Value 5 -Force | Out-Null
}

function Ensure-DiskReserve {
    param([Parameter(Mandatory = $true)][string]$Path, [UInt64]$Length)
    if (Test-Path -LiteralPath $Path) {
        Assert-OrdinaryPath $Path $false
        $existing = Get-Item -LiteralPath $Path -Force
        if ([UInt64]$existing.Length -ne $Length -or
            ($existing.Attributes -band [IO.FileAttributes]::SparseFile) -ne 0 -or
            ($existing.Attributes -band [IO.FileAttributes]::Compressed) -ne 0) {
            throw 'A pre-existing control-plane disk reserve was not exact.'
        }
        return $false
    }
    Assert-OrdinaryPath (Split-Path -Parent $Path) $false
    $drive = Get-PSDrive -Name ([IO.Path]::GetPathRoot($Path).Substring(0,1))
    if ([UInt64]$drive.Free -lt ($Length + 1073741824)) {
        throw 'The host lacks disk headroom for the protected control-plane reserve.'
    }
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $stream.SetLength(0)
        $buffer = New-Object byte[] 1048576
        [UInt64]$written = 0
        while ($written -lt $Length) {
            $remaining = $Length - $written
            $count = [int][Math]::Min([UInt64]$buffer.Length, $remaining)
            $stream.Write($buffer, 0, $count)
            $written += [UInt64]$count
        }
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
    }
    $attributes = (Get-Item -LiteralPath $Path -Force).Attributes
    if (($attributes -band [IO.FileAttributes]::SparseFile) -ne 0 -or
        ($attributes -band [IO.FileAttributes]::Compressed) -ne 0) {
        throw 'The control-plane disk reserve was sparse or compressed.'
    }
    Assert-OrdinaryPath $Path $false
    return $true
}

function Ensure-ExactPolicyFile {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][byte[]]$Bytes)
    $fileShaAlgorithm = [Security.Cryptography.SHA256]::Create()
    try { $expected = [BitConverter]::ToString($fileShaAlgorithm.ComputeHash($Bytes)).Replace('-', '').ToLowerInvariant() } finally { $fileShaAlgorithm.Dispose() }
    if (Test-Path -LiteralPath $Path) {
        Assert-OrdinaryPath $Path $false
        if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected) {
            throw 'A pre-existing execution-host policy was not exact.'
        }
        return $false
    }
    Assert-OrdinaryPath (Split-Path -Parent $Path) $false
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($Bytes, 0, $Bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
    Assert-OrdinaryPath $Path $false
    return $true
}

function Assert-ProtectedAcl {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$MasterSid,
        [Parameter(Mandatory = $true)][string]$BrokerSid,
        [Parameter(Mandatory = $true)][string]$FeatureSid,
        [bool]$ExecutorReadable = $false
    )
    Assert-OrdinaryPath $Path $false
    $acl = Get-Acl -LiteralPath $Path
    if ($acl.Owner -cnotmatch '^(NT AUTHORITY\\SYSTEM|S-1-5-18)$' -or -not $acl.AreAccessRulesProtected) {
        throw 'Protected execution-host ownership or inheritance drifted.'
    }
    $item = Get-Item -LiteralPath $Path -Force
    $containerInheritance = [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit'
    $noInheritance = [Security.AccessControl.InheritanceFlags]::None
    $expectedPropagation = [Security.AccessControl.PropagationFlags]::None
    $full = [Security.AccessControl.FileSystemRights]::FullControl
    $mutation = [Security.AccessControl.FileSystemRights]'Write, Delete, ChangePermissions, TakeOwnership'
    # FileSystemAccessRule persists Allow ReadAndExecute with Synchronize.
    # Validate the exact native representation rather than the constructor input.
    $read = [Security.AccessControl.FileSystemRights]::ReadAndExecute -bor
        [Security.AccessControl.FileSystemRights]::Synchronize
    $expected = @{
        "$FeatureSid|Deny" = if ($ExecutorReadable) { $mutation } else { $full }
        'S-1-5-18|Allow' = $full
        'S-1-5-32-544|Allow' = $full
        "$MasterSid|Allow" = $full
        "$BrokerSid|Allow" = $full
    }
    if ($ExecutorReadable) { $expected["$FeatureSid|Allow"] = $read }
    $seen = @{}
    foreach ($rule in @($acl.Access)) {
        $sid = $rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value
        $kind = [string]$rule.AccessControlType
        $key = "$sid|$kind"
        $expectedInheritance = if ($item.PSIsContainer -and $key -cne "$FeatureSid|Allow") {
            $containerInheritance
        } else {
            $noInheritance
        }
        if (-not $expected.ContainsKey($key) -or $seen.ContainsKey($key) -or
            [UInt32]$rule.FileSystemRights -ne [UInt32]$expected[$key] -or
            $rule.IsInherited -or
            $rule.InheritanceFlags -ne $expectedInheritance -or
            $rule.PropagationFlags -ne $expectedPropagation) {
            throw ('A protected execution-host ACL contained an unexpected or broad entry. ' +
                'identity={0}; kind={1}; rights={2}; inherited={3}; inheritance={4}; propagation={5}.' -f
                $sid, $kind, [UInt32]$rule.FileSystemRights, $rule.IsInherited,
                [string]$rule.InheritanceFlags, [string]$rule.PropagationFlags)
        }
        $seen[$key] = $true
    }
    if ($seen.Count -ne $expected.Count) { throw 'The exact protected execution-host ACL was incomplete.' }
}

function Assert-ServiceContract {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$ExpectedAccount,
        [Parameter(Mandatory = $true)][int]$ExpectedSidType,
        [Parameter(Mandatory = $true)][string]$FeatureSid
    )
    $service = Get-ServiceRecord $Name
    if ($null -eq $service -or $service.StartName -cne $ExpectedAccount) { throw 'A service identity drifted.' }
    $sidType = (Get-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$Name" -Name ServiceSidType).ServiceSidType
    if ([int]$sidType -ne $ExpectedSidType) { throw 'A service SID type drifted.' }
    $sddl = (@(Invoke-Sc @('sdshow', $Name)) -join '').Trim()
    $ownSid = Get-ServiceSid $Name
    Assert-ExactServiceDacl $sddl $ownSid $FeatureSid
}

function Set-ProtectedRegistryAcl {
    param([string]$MasterSid, [string]$BrokerSid, [string]$FeatureSid)
    $acl = New-Object Security.AccessControl.RegistrySecurity
    $acl.SetOwner([Security.Principal.NTAccount]::new('NT AUTHORITY\SYSTEM'))
    $acl.SetAccessRuleProtection($true, $false)
    [void]$acl.AddAccessRule((New-Object Security.AccessControl.RegistryAccessRule(
        ([Security.Principal.SecurityIdentifier]::new($FeatureSid)), 'FullControl', 'Deny'
    )))
    foreach ($sid in @('S-1-5-18', 'S-1-5-32-544', $MasterSid, $BrokerSid)) {
        [void]$acl.AddAccessRule((New-Object Security.AccessControl.RegistryAccessRule(
            ([Security.Principal.SecurityIdentifier]::new($sid)), 'FullControl', 'Allow'
        )))
    }
    Set-Acl -LiteralPath $policyRegistry -AclObject $acl
}

function Assert-ProtectedRegistryAcl {
    param([string]$MasterSid, [string]$BrokerSid, [string]$FeatureSid)
    $acl = Get-Acl -LiteralPath $policyRegistry
    if ($acl.Owner -cnotmatch '^(NT AUTHORITY\\SYSTEM|S-1-5-18)$' -or -not $acl.AreAccessRulesProtected) {
        throw 'The execution-host registry ACL ownership or inheritance drifted.'
    }
    $expected = @{ $FeatureSid = 'Deny'; 'S-1-5-18' = 'Allow'; 'S-1-5-32-544' = 'Allow'; $MasterSid = 'Allow'; $BrokerSid = 'Allow' }
    $seen = @{}
    foreach ($rule in @($acl.Access)) {
        $sid = $rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value
        if (-not $expected.ContainsKey($sid) -or $expected[$sid] -cne [string]$rule.AccessControlType -or
            $seen.ContainsKey($sid) -or (($rule.RegistryRights -band [Security.AccessControl.RegistryRights]::FullControl) -ne [Security.AccessControl.RegistryRights]::FullControl)) {
            throw 'The execution-host registry ACL contained an unexpected entry.'
        }
        $seen[$sid] = $true
    }
    if ($seen.Count -ne $expected.Count) { throw 'The exact execution-host registry ACL was incomplete.' }
}

function Assert-RegistryPolicyValues {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$ExpectedPolicySha256)
    $registry = Get-ItemProperty -LiteralPath $Path
    if ([int]$registry.EffectsEnabled -ne 0 -or [int]$registry.SchemaVersion -ne 1 -or
        [string]$registry.PolicySha256 -cne $ExpectedPolicySha256) {
        throw 'The execution-host registry gate or policy binding drifted.'
    }
}

function Get-PathFreeReceipt {
    param([Parameter(Mandatory = $true)][string]$Status, [bool]$Applied)
    return [ordered]@{
        schema_version = 1
        status = $Status
        service_sid_model = 'localsystem_with_distinct_service_sids'
        executor_identity = 'localservice_with_restricted_service_sid'
        protected_acl = 'system_owned_noninheriting_feature_denied'
        resource_policy = 'windows_job_limits_priority_and_allocated_disk_reserve'
        production_effects_enabled = $false
        applied = $Applied
    }
}

function Invoke-DryRun {
    if ($ConfirmStoppedServiceCeremony) {
        throw 'DryRun accepts no ceremony confirmation.'
    }
    [void](Get-ResourcePolicy)
    Get-PathFreeReceipt 'execution_host_dry_run_passed' $false
}

function Invoke-Check {
    if ($ConfirmStoppedServiceCeremony) {
        throw 'Check accepts no ceremony confirmation.'
    }
    Assert-Elevated
    # Existence is the first host-state precondition. Do not translate service
    # SIDs or touch release paths on a clean machine that has no service hosts.
    $master = Get-ServiceRecord $masterService
    $broker = Get-ServiceRecord $brokerService
    $executor = Get-ServiceRecord $executorService
    if ($null -eq $master -or $null -eq $broker -or $null -eq $executor) {
        throw 'Required execution-host service substrate is not installed.'
    }
    $masterSid = Get-ServiceSid $masterService
    $brokerSid = Get-ServiceSid $brokerService
    $featureSid = Get-ServiceSid $executorService
    $manifest = Get-VerifiedReleaseManifest $masterSid $brokerSid $featureSid
    $commands = Get-ExpectedServiceCommands $manifest
    Assert-ProductionServiceBinding $master 'LocalSystem' $masterImage $commands.Master
    Assert-ProductionServiceBinding $broker 'LocalSystem' $brokerImage $commands.Broker
    Assert-ProductionServiceBinding $executor 'NT AUTHORITY\LocalService' $executorImage $commands.Executor
    Assert-ServicePersistenceContract $masterService @('SeChangeNotifyPrivilege')
    Assert-ServicePersistenceContract $brokerService @('SeChangeNotifyPrivilege', 'SeBackupPrivilege', 'SeRestorePrivilege', 'SeTakeOwnershipPrivilege')
    Assert-ServicePersistenceContract $executorService @('SeChangeNotifyPrivilege')
    Assert-AllServicesStopped
    Assert-ServiceContract $masterService 'LocalSystem' 1 $featureSid
    Assert-ServiceContract $brokerService 'LocalSystem' 1 $featureSid
    Assert-ServiceContract $executorService 'NT AUTHORITY\LocalService' 3 $featureSid
    foreach ($entry in @(
        @{ Path = $hostRoot; ExecutorReadable = $true },
        @{ Path = $configRoot; ExecutorReadable = $true },
        @{ Path = $stateRoot; ExecutorReadable = $false },
        @{ Path = $auditRoot; ExecutorReadable = $false },
        @{ Path = $updateRoot; ExecutorReadable = $false },
        @{ Path = $reserveRoot; ExecutorReadable = $false },
        @{ Path = $policyPath; ExecutorReadable = $false },
        @{ Path = $reservePath; ExecutorReadable = $false }
    )) {
        Assert-ProtectedAcl $entry.Path $masterSid $brokerSid $featureSid $entry.ExecutorReadable
    }
    $expectedPolicy = Get-ResourcePolicy
    $expectedPolicyBytes = [Text.UTF8Encoding]::new($false).GetBytes(($expectedPolicy | ConvertTo-Json -Compress))
    $policyShaAlgorithm = [Security.Cryptography.SHA256]::Create()
    try { $expectedPolicySha = [BitConverter]::ToString($policyShaAlgorithm.ComputeHash($expectedPolicyBytes)).Replace('-', '').ToLowerInvariant() } finally { $policyShaAlgorithm.Dispose() }
    $policy = Get-Content -LiteralPath $policyPath -Raw | ConvertFrom-Json
    $reserve = Get-Item -LiteralPath $reservePath -Force
    if (($reserve.Attributes -band [IO.FileAttributes]::SparseFile) -ne 0 -or
        ($reserve.Attributes -band [IO.FileAttributes]::Compressed) -ne 0 -or
        [UInt64]$policy.control_plane_disk_reserve_bytes -ne [UInt64]$reserve.Length -or
        $policy.effect_activation_requires_exact_policy_attestation -ne $true -or
        $policy.windows_executor_job_required -ne $true) {
        throw 'The protected resource policy or allocated reserve drifted.'
    }
    if ((Get-FileHash -LiteralPath $policyPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expectedPolicySha) {
        throw 'The protected resource policy did not match current host capacity.'
    }
    Assert-RegistryPolicyValues $policyRegistry $expectedPolicySha
    Assert-ProtectedRegistryAcl $masterSid $brokerSid $featureSid
    foreach ($image in @('assemblywright-master.exe', 'assemblywright-broker.exe')) {
        $perf = Get-ItemProperty -LiteralPath (Join-Path (Join-Path $ifeoRoot $image) 'PerfOptions')
        if ([int]$perf.CpuPriorityClass -ne 3 -or [int]$perf.IoPriority -ne 2 -or [int]$perf.PagePriority -ne 5) {
            throw 'The control-plane process priority reservation drifted.'
        }
    }
    Get-PathFreeReceipt 'execution_host_check_passed' $false
}

function Invoke-Apply {
    Assert-Elevated
    if (-not $ConfirmStoppedServiceCeremony) { throw 'Apply requires -ConfirmStoppedServiceCeremony.' }
    Assert-AllServicesStopped
    # This is deliberately the first mutation. Any later partial failure leaves
    # production effects explicitly disabled.
    New-Item -Path $policyRegistry -Force | Out-Null
    New-ItemProperty -Path $policyRegistry -Name EffectsEnabled -PropertyType DWord -Value 0 -Force | Out-Null
    if ([int](Get-ItemProperty -LiteralPath $policyRegistry -Name EffectsEnabled).EffectsEnabled -ne 0) {
        throw 'The fail-closed effects registry gate could not be established.'
    }
    Assert-AllServicesStopped
    $master = Get-ServiceRecord $masterService
    $broker = Get-ServiceRecord $brokerService
    $executor = Get-ServiceRecord $executorService
    if ($null -eq $broker -or $null -eq $executor) {
        throw 'Apply cannot create service hosts; exact Broker and Executor services must already exist.'
    }
    $masterSid = Get-ServiceSid $masterService
    $brokerSid = Get-ServiceSid $brokerService
    $featureSid = Get-ServiceSid $executorService
    $manifest = Get-VerifiedReleaseManifest $masterSid $brokerSid $featureSid
    $commands = Get-ExpectedServiceCommands $manifest
    Assert-ProductionServiceBinding $master 'LocalSystem' $masterImage $commands.Master
    Assert-ProductionServiceBinding $broker 'LocalSystem' $brokerImage $commands.Broker
    Assert-ProductionServiceBinding $executor 'NT AUTHORITY\LocalService' $executorImage $commands.Executor
    Invoke-StoppedMutationCluster {
        foreach ($name in @($masterService, $brokerService, $executorService)) { [void](Invoke-Sc @('config', $name, 'start=', 'disabled')) }
    }
    Assert-ServicePersistenceContract $masterService @('SeChangeNotifyPrivilege')
    Assert-ServicePersistenceContract $brokerService @('SeChangeNotifyPrivilege', 'SeBackupPrivilege', 'SeRestorePrivilege', 'SeTakeOwnershipPrivilege')
    Assert-ServicePersistenceContract $executorService @('SeChangeNotifyPrivilege')
    Invoke-StoppedMutationCluster {
        [void](Invoke-Sc @('sidtype', $masterService, 'unrestricted'))
        [void](Invoke-Sc @('sidtype', $brokerService, 'unrestricted'))
        [void](Invoke-Sc @('sidtype', $executorService, 'restricted'))
    }
    Invoke-StoppedMutationCluster {
        foreach ($entry in @(
            @{ Name = $masterService; Sid = $masterSid },
            @{ Name = $brokerService; Sid = $brokerSid },
            @{ Name = $executorService; Sid = $featureSid }
        )) { Set-ServiceSecurity $entry.Name $entry.Sid $featureSid }
        Set-ProtectedRegistryAcl $masterSid $brokerSid $featureSid
    }
    Invoke-StoppedMutationCluster {
        foreach ($entry in @(
            @{ Path = $hostRoot; ExecutorReadable = $true },
            @{ Path = $configRoot; ExecutorReadable = $true },
            @{ Path = $stateRoot; ExecutorReadable = $false },
            @{ Path = $auditRoot; ExecutorReadable = $false },
            @{ Path = $updateRoot; ExecutorReadable = $false },
            @{ Path = $reserveRoot; ExecutorReadable = $false }
        )) {
            $path = $entry.Path
            if (-not (Test-Path -LiteralPath $path)) { New-Item -ItemType Directory -Path $path | Out-Null }
            Assert-OrdinaryPath $path $false
            Set-ProtectedDirectoryAcl $path $masterSid $brokerSid $featureSid $entry.ExecutorReadable
        }
    }
    $policy = Get-ResourcePolicy
    $policyBytes = [Text.UTF8Encoding]::new($false).GetBytes(($policy | ConvertTo-Json -Compress))
    $policyExists = Test-Path -LiteralPath $policyPath
    if ($policyExists) { Assert-ProtectedAcl $policyPath $masterSid $brokerSid $featureSid }
    Invoke-StoppedMutationCluster {
        [void](Ensure-ExactPolicyFile $policyPath $policyBytes)
        if (-not $policyExists) { Set-ProtectedFileAcl $policyPath $masterSid $brokerSid $featureSid }
    }
    $reserveExists = Test-Path -LiteralPath $reservePath
    if ($reserveExists) { Assert-ProtectedAcl $reservePath $masterSid $brokerSid $featureSid }
    Invoke-StoppedMutationCluster {
        [void](Ensure-DiskReserve $reservePath $diskReserveBytes)
        if (-not $reserveExists) { Set-ProtectedFileAcl $reservePath $masterSid $brokerSid $featureSid }
        Set-ControlPlanePriority 'assemblywright-master.exe'
        Set-ControlPlanePriority 'assemblywright-broker.exe'
        New-ItemProperty -Path $policyRegistry -Name SchemaVersion -PropertyType DWord -Value 1 -Force | Out-Null
        New-ItemProperty -Path $policyRegistry -Name PolicySha256 -PropertyType String -Value ((Get-FileHash -LiteralPath $policyPath -Algorithm SHA256).Hash.ToLowerInvariant()) -Force | Out-Null
        New-ItemProperty -Path $policyRegistry -Name EffectsEnabled -PropertyType DWord -Value 0 -Force | Out-Null
    }
    Assert-AllServicesStopped
    Assert-RegistryPolicyValues $policyRegistry ((Get-FileHash -LiteralPath $policyPath -Algorithm SHA256).Hash.ToLowerInvariant())
    Get-PathFreeReceipt 'execution_host_applied' $true
}

function Invoke-SelfTest {
    if ($ConfirmStoppedServiceCeremony) { throw 'SelfTest accepts no ceremony confirmation.' }
    Assert-Elevated
    $suffix = [Guid]::NewGuid().ToString('N')
    $scratch = Join-Path ([IO.Path]::GetTempPath()) "AssemblywrightExecutionHostSelfTest-$suffix"
    $registry = "HKCU:\Software\Assemblywright\ExecutionHostSelfTest-$suffix"
    $original = Join-Path $scratch 'original.bin'
    $hardlink = Join-Path $scratch 'policy-hardlink.json'
    $symlink = Join-Path $scratch 'reserve-symlink.bin'
    $readableDirectory = Join-Path $scratch 'executor-readable'
    $readableFile = Join-Path $readableDirectory 'executor.json'
    $readableSiblingCanary = Join-Path $readableDirectory 'sibling-canary.json'
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes('{"schema_version":1}')
    $serviceName = "AssemblywrightHostValidatorE2E$($suffix.Substring(0,12))"
    $serviceCreated = $false
    try {
        New-Item -ItemType Directory -Path $scratch | Out-Null
        [IO.File]::WriteAllText($original, 'hostile-prestate', [Text.UTF8Encoding]::new($false))
        & fsutil.exe hardlink create $hardlink $original | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'SelfTest could not create its disposable hardlink.' }
        $before = (Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash
        $hardlinkRejected = $false
        try { [void](Ensure-ExactPolicyFile $hardlink $bytes) } catch { $hardlinkRejected = $true }
        if (-not $hardlinkRejected -or (Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash -cne $before) {
            throw 'The real policy validator did not reject the hostile hardlink unchanged.'
        }

        New-Item -ItemType SymbolicLink -Path $symlink -Target $original | Out-Null
        $symlinkRejected = $false
        try { [void](Ensure-DiskReserve $symlink 33554432) } catch { $symlinkRejected = $true }
        if (-not $symlinkRejected -or (Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash -cne $before) {
            throw 'The real reserve validator did not reject the hostile symlink unchanged.'
        }

        New-Item -Path $registry -Force | Out-Null
        New-ItemProperty -Path $registry -Name EffectsEnabled -PropertyType DWord -Value 1 -Force | Out-Null
        New-ItemProperty -Path $registry -Name SchemaVersion -PropertyType DWord -Value 1 -Force | Out-Null
        New-ItemProperty -Path $registry -Name PolicySha256 -PropertyType String -Value ('0' * 64) -Force | Out-Null
        $registryRejected = $false
        try { Assert-RegistryPolicyValues $registry ('0' * 64) } catch { $registryRejected = $true }
        if (-not $registryRejected -or [int](Get-ItemProperty -LiteralPath $registry).EffectsEnabled -ne 1) {
            throw 'The real registry validator did not reject effects-enabled drift unchanged.'
        }

        # Windows system executables are commonly hard-linked into WinSxS. Copy
        # the inert fixture image into the disposable root so the production
        # single-link validator is exercised against its intended precondition.
        $fixtureImage = Join-Path $scratch 'fixture-service.exe'
        [IO.File]::Copy((Join-Path $env:SystemRoot 'System32\cmd.exe'), $fixtureImage, $false)
        $validCommand = if ($fixtureImage -match '\s') {
            ('"{0}" /c exit 0' -f $fixtureImage)
        } else {
            ('{0} /c exit 0' -f $fixtureImage)
        }
        [void](Invoke-Sc @('create', $serviceName, 'binPath=', $validCommand, 'start=', 'disabled', 'error=', 'normal', 'type=', 'own', 'obj=', 'LocalSystem'))
        $serviceCreated = $true
        [void](Invoke-Sc @('privs', $serviceName, 'SeChangeNotifyPrivilege'))
        [void](Invoke-Sc @('sidtype', $serviceName, 'unrestricted'))
        $fixtureSid = Get-ServiceSid $serviceName
        Set-ServiceSecurity $serviceName $fixtureSid $fixtureSid
        Assert-ProductionServiceBinding (Get-ServiceRecord $serviceName) 'LocalSystem' $fixtureImage $validCommand
        Assert-ServicePersistenceContract $serviceName @('SeChangeNotifyPrivilege')
        Assert-ServiceContract $serviceName 'LocalSystem' 1 $fixtureSid
        $reorderedServiceDaclRejected = $false
        $reorderedSddl = "D:(A;;CCLCSWLOCRRC;;;$fixtureSid)(D;;GA;;;$fixtureSid)(A;;GA;;;SY)(A;;GA;;;BA)"
        try { Assert-ExactServiceDacl $reorderedSddl $fixtureSid $fixtureSid } catch { $reorderedServiceDaclRejected = $true }
        if (-not $reorderedServiceDaclRejected) {
            throw 'The service DACL validator accepted an allow-before-deny descriptor.'
        }

        New-Item -ItemType Directory -Path $readableDirectory | Out-Null
        Set-ProtectedDirectoryAcl $readableDirectory 'S-1-5-19' 'S-1-5-20' $fixtureSid $true
        [IO.File]::WriteAllText($readableFile, '{"schema_version":1}', [Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText($readableSiblingCanary, 'sibling-secret', [Text.UTF8Encoding]::new($false))
        Set-ProtectedFileAcl $readableFile 'S-1-5-19' 'S-1-5-20' $fixtureSid $true
        Assert-ProtectedAcl $readableDirectory 'S-1-5-19' 'S-1-5-20' $fixtureSid $true
        Assert-ProtectedAcl $readableFile 'S-1-5-19' 'S-1-5-20' $fixtureSid $true
        $siblingReadAllows = @((Get-Acl -LiteralPath $readableSiblingCanary).Access | Where-Object {
            $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -ceq $fixtureSid -and
            $_.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and
            (($_.FileSystemRights -band [Security.AccessControl.FileSystemRights]::ReadData) -ne 0)
        })
        if ($siblingReadAllows.Count -ne 0) {
            throw 'The Executor ancestor grant leaked read access to an inherited sibling.'
        }

        $driftAcl = Get-Acl -LiteralPath $readableDirectory
        $trustedAllow = @($driftAcl.Access | Where-Object {
            $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -ceq 'S-1-5-20' -and
            $_.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow
        })
        if ($trustedAllow.Count -ne 1) { throw 'SelfTest could not select the trusted inheritable ACL entry.' }
        [void]$driftAcl.RemoveAccessRuleSpecific($trustedAllow[0])
        [void]$driftAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
            ([Security.Principal.SecurityIdentifier]::new('S-1-5-20')),
            [Security.AccessControl.FileSystemRights]::FullControl,
            [Security.AccessControl.InheritanceFlags]::None,
            [Security.AccessControl.PropagationFlags]::None,
            [Security.AccessControl.AccessControlType]::Allow
        )))
        Set-Acl -LiteralPath $readableDirectory -AclObject $driftAcl
        $nonInheritableAclDriftRejected = $false
        try { Assert-ProtectedAcl $readableDirectory 'S-1-5-19' 'S-1-5-20' $fixtureSid $true } catch { $nonInheritableAclDriftRejected = $true }
        if (-not $nonInheritableAclDriftRejected) {
            throw 'The exact ACL validator accepted non-inheritable directory drift.'
        }
        Set-ProtectedDirectoryAcl $readableDirectory 'S-1-5-19' 'S-1-5-20' $fixtureSid $true
        Assert-ProtectedAcl $readableDirectory 'S-1-5-19' 'S-1-5-20' $fixtureSid $true

        $driftAcl = Get-Acl -LiteralPath $readableDirectory
        $featureAllow = @($driftAcl.Access | Where-Object {
            $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -ceq $fixtureSid -and
            $_.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow
        })
        if ($featureAllow.Count -ne 1) { throw 'SelfTest could not select the executor traversal ACL entry.' }
        [void]$driftAcl.RemoveAccessRuleSpecific($featureAllow[0])
        [void]$driftAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
            ([Security.Principal.SecurityIdentifier]::new($fixtureSid)),
            [Security.AccessControl.FileSystemRights]::ReadAndExecute,
            [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
            [Security.AccessControl.PropagationFlags]::None,
            [Security.AccessControl.AccessControlType]::Allow
        )))
        Set-Acl -LiteralPath $readableDirectory -AclObject $driftAcl
        $inheritableExecutorReadAclDriftRejected = $false
        try { Assert-ProtectedAcl $readableDirectory 'S-1-5-19' 'S-1-5-20' $fixtureSid $true } catch { $inheritableExecutorReadAclDriftRejected = $true }
        if (-not $inheritableExecutorReadAclDriftRejected) {
            throw 'The exact ACL validator accepted an inheritable Executor read grant.'
        }
        Set-ProtectedDirectoryAcl $readableDirectory 'S-1-5-19' 'S-1-5-20' $fixtureSid $true
        Assert-ProtectedAcl $readableDirectory 'S-1-5-19' 'S-1-5-20' $fixtureSid $true

        $hostileArgvRejected = $false
        [void](Invoke-Sc @('config', $serviceName, 'binPath=', ($validCommand + ' extra')))
        try { Assert-ProductionServiceBinding (Get-ServiceRecord $serviceName) 'LocalSystem' $fixtureImage $validCommand } catch { $hostileArgvRejected = $true }
        [void](Invoke-Sc @('config', $serviceName, 'binPath=', $validCommand))
        if (-not $hostileArgvRejected) { throw 'The real service validator accepted hostile argv drift.' }

        foreach ($drift in @('start', 'type', 'failure', 'trigger')) {
            $rejected = $false
            switch ($drift) {
                'start' { [void](Invoke-Sc @('config', $serviceName, 'start=', 'demand')) }
                'type' { Set-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$serviceName" -Name Type -Value 272 }
                'failure' { [void](Invoke-Sc @('failure', $serviceName, 'reset=', '60', 'actions=', 'restart/1000')) }
                'trigger' { New-Item -Path "HKLM:\SYSTEM\CurrentControlSet\Services\$serviceName\TriggerInfo\0" -Force | Out-Null }
            }
            try { Assert-ServicePersistenceContract $serviceName @('SeChangeNotifyPrivilege') } catch { $rejected = $true }
            switch ($drift) {
                'start' { [void](Invoke-Sc @('config', $serviceName, 'start=', 'disabled')) }
                'type' { Set-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$serviceName" -Name Type -Value 16 }
                'failure' { Remove-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$serviceName" -Name FailureActions -ErrorAction SilentlyContinue }
                'trigger' { Remove-Item -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$serviceName\TriggerInfo" -Recurse -Force }
            }
            if (-not $rejected) { throw 'The real service persistence validator accepted hostile drift.' }
        }
        Assert-ServicePersistenceContract $serviceName @('SeChangeNotifyPrivilege')
        [ordered]@{
            schema_version = 1
            status = 'execution_host_self_test_passed'
            hostile_hardlink_rejected_unchanged = $true
            hostile_symlink_rejected_unchanged = $true
            effects_enabled_drift_rejected_unchanged = $true
            valid_disposable_service_contract_passed = $true
            reordered_service_dacl_rejected = $true
            hostile_service_argv_and_persistence_drift_rejected = $true
            executor_readonly_acl_contract_passed = $true
            non_inheritable_acl_drift_rejected = $true
            inheritable_executor_read_acl_drift_rejected = $true
            executor_inherited_sibling_read_denied = $true
            production_services_untouched = $true
            paths_disclosed = $false
        }
    } finally {
        if ($serviceCreated) { & sc.exe delete $serviceName | Out-Null }
        if (Test-Path -LiteralPath $registry) { Remove-Item -LiteralPath $registry -Recurse -Force }
        if (Test-Path -LiteralPath $scratch) { Remove-Item -LiteralPath $scratch -Recurse -Force }
    }
}

switch ($Mode) {
    'DryRun' { Invoke-DryRun | ConvertTo-Json -Compress }
    'Check' { Invoke-Check | ConvertTo-Json -Compress }
    'Apply' { Invoke-Apply | ConvertTo-Json -Compress }
    'SelfTest' { Invoke-SelfTest | ConvertTo-Json -Compress }
}
