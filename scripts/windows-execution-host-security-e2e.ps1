[CmdletBinding()]
param(
    [switch]$Confirm
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $Confirm) { throw 'The disposable execution-host security E2E requires -Confirm.' }

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The disposable execution-host security E2E requires elevated Windows SCM access.'
}

$suffix = [Guid]::NewGuid().ToString('N').Substring(0,12)
$masterName = "AssemblywrightHostE2EM$suffix"
$brokerName = "AssemblywrightHostE2EB$suffix"
$featureName = "AssemblywrightHostE2EF$suffix"
$realBrokerName = "AssemblywrightBrokerE2E$suffix"
$realExecutorName = "AssemblywrightExecutorE2E$suffix"
$root = Join-Path ([IO.Path]::GetTempPath()) "AssemblywrightHostSecurityE2E-$suffix"
$allowedRoot = Join-Path ([IO.Path]::GetTempPath()) "AssemblywrightHostSecurityAllowed-$suffix"
$payload = Join-Path $allowedRoot 'hostile-feature.ps1'
$marker = Join-Path $allowedRoot 'feature-ran.marker'
$executorReadableRoot = Join-Path $allowedRoot 'executor-launch'
$executorReadableFile = Join-Path $executorReadableRoot 'executor-readable.json'
$executorSiblingCanary = Join-Path $executorReadableRoot 'sibling-canary.json'
$protectedFile = Join-Path $root 'protected.json'
$reserveFile = Join-Path $root 'reserve.bin'
$serviceCreated = New-Object System.Collections.Generic.List[string]
$registryFixture = "HKLM:\SOFTWARE\Assemblywright\ExecutionHostE2E-$suffix"
$provisioner = Join-Path $PSScriptRoot 'windows-execution-host-provision.ps1'

function Invoke-Sc {
    param([string[]]$Arguments)
    $output = @(& sc.exe @Arguments 2>&1)
    if ($LASTEXITCODE -ne 0) { throw 'The disposable SCM operation failed.' }
    return $output
}

function Get-ServiceSid {
    param([string]$Name)
    return ([Security.Principal.NTAccount]::new("NT SERVICE\$Name")).Translate(
        [Security.Principal.SecurityIdentifier]
    ).Value
}

function Assert-FeatureServiceDeny {
    param([string]$Sddl, [string]$FeatureSid)
    $descriptor = [Security.AccessControl.RawSecurityDescriptor]::new($Sddl)
    [UInt32]$serviceAllAccess = 0x000F01FF
    $matching = 0
    foreach ($ace in $descriptor.DiscretionaryAcl) {
        if ($ace.SecurityIdentifier.Value -ceq $FeatureSid) {
            if ($ace -isnot [Security.AccessControl.CommonAce] -or
                $ace.AceQualifier -ne [Security.AccessControl.AceQualifier]::AccessDenied -or
                [UInt32]$ace.AccessMask -ne $serviceAllAccess -or
                $ace.AceFlags -ne [Security.AccessControl.AceFlags]::None) {
                throw ("The hostile feature SID could alter a protected service definition. observed_sddl=$Sddl")
            }
            $matching += 1
        }
    }
    if ($matching -ne 1) {
        throw ("The hostile feature SID could alter a protected service definition. observed_sddl=$Sddl")
    }
}

function Wait-ServiceState {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Expected,
        [int]$TimeoutSeconds = 40
    )
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $service = Get-CimInstance Win32_Service -Filter "Name='$Name'" -ErrorAction SilentlyContinue
        if ($null -ne $service -and $service.State -ceq $Expected) { return $service }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'A disposable real service host did not reach its exact expected state.'
}

function Assert-AccessDeniedByDacl {
    param([string]$Path, [string]$Sid)
    $acl = Get-Acl -LiteralPath $Path
    $denies = @($acl.Access | Where-Object {
        $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -ceq $Sid -and
        $_.AccessControlType -eq [Security.AccessControl.AccessControlType]::Deny -and
        (($_.FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -eq
            [Security.AccessControl.FileSystemRights]::FullControl)
    })
    if ($denies.Count -ne 1) { throw 'The hostile feature SID was not denied by the native filesystem DACL.' }
}

try {
    $dryRun = (& $provisioner -Mode DryRun | Out-String).Trim() | ConvertFrom-Json
    if ($dryRun.status -cne 'execution_host_dry_run_passed' -or $dryRun.production_effects_enabled -ne $false) {
        throw 'The production provisioner DryRun contract failed.'
    }
    $checkExecuted = $false
    try {
        $check = (& $provisioner -Mode Check | Out-String).Trim() | ConvertFrom-Json
        if ($check.status -cne 'execution_host_check_passed') { throw 'The production Check receipt was malformed.' }
        $checkExecuted = $true
    } catch {
        # A host without the exact service substrate has one accepted precondition
        # rejection. Parser, undefined-variable, or unrelated policy failures must
        # never be laundered into successful E2E evidence.
        if ($_.Exception.Message -cnotin @(
            'Required execution-host service substrate is not installed.',
            'Owner-account service deployment is not the production execution-host substrate.'
        )) {
            throw
        }
        $checkExecuted = $true
    }
    if (-not $checkExecuted) { throw 'The production provisioner Check was not executed.' }
    $selfTest = (& $provisioner -Mode SelfTest | Out-String).Trim() | ConvertFrom-Json
    if ($selfTest.status -cne 'execution_host_self_test_passed' -or
        $selfTest.hostile_hardlink_rejected_unchanged -ne $true -or
        $selfTest.hostile_symlink_rejected_unchanged -ne $true -or
        $selfTest.effects_enabled_drift_rejected_unchanged -ne $true -or
        $selfTest.valid_disposable_service_contract_passed -ne $true -or
        $selfTest.reordered_service_dacl_rejected -ne $true -or
        $selfTest.hostile_service_argv_and_persistence_drift_rejected -ne $true -or
        $selfTest.executor_readonly_acl_contract_passed -ne $true -or
        $selfTest.non_inheritable_acl_drift_rejected -ne $true -or
        $selfTest.inheritable_executor_read_acl_drift_rejected -ne $true -or
        $selfTest.executor_inherited_sibling_read_denied -ne $true) {
        throw 'The production provisioner SelfTest contract failed.'
    }

    New-Item -Path $registryFixture -Force | Out-Null
    New-ItemProperty -Path $registryFixture -Name EffectsEnabled -PropertyType DWord -Value 1 -Force | Out-Null
    if ([int](Get-ItemProperty -LiteralPath $registryFixture).EffectsEnabled -eq 0) {
        throw 'The effects-enabled drift fixture did not become hostile.'
    }

    $prestate = Join-Path $allowedRoot 'prestate.bin'
    $hardlink = Join-Path $allowedRoot 'prestate-hardlink.bin'
    $symlink = Join-Path $allowedRoot 'prestate-symlink.bin'
    $command = Join-Path $env:SystemRoot 'System32\cmd.exe'
    foreach ($service in @(
        @{ Name = $masterName; Account = 'LocalSystem'; SidType = 'unrestricted' },
        @{ Name = $brokerName; Account = 'LocalSystem'; SidType = 'unrestricted' },
        @{ Name = $featureName; Account = 'NT AUTHORITY\LocalService'; SidType = 'restricted' }
    )) {
        [void](Invoke-Sc @('create', $service.Name, 'binPath=', ('"{0}" /c exit 0' -f $command), 'start=', 'disabled', 'obj=', $service.Account))
        $serviceCreated.Add($service.Name)
        [void](Invoke-Sc @('sidtype', $service.Name, $service.SidType))
    }
    $masterSid = Get-ServiceSid $masterName
    $brokerSid = Get-ServiceSid $brokerName
    $featureSid = Get-ServiceSid $featureName
    foreach ($service in @(
        @{ Name = $masterName; Sid = $masterSid },
        @{ Name = $brokerName; Sid = $brokerSid }
    )) {
        [void](Invoke-Sc @('sdset', $service.Name, "D:(D;;GA;;;$featureSid)(A;;GA;;;SY)(A;;GA;;;BA)(A;;CCLCSWLOCRRC;;;$($service.Sid))"))
        $sddl = (@(Invoke-Sc @('sdshow', $service.Name)) -join '')
        Assert-FeatureServiceDeny $sddl $featureSid
    }
    New-Item -ItemType Directory -Path $root,$allowedRoot | Out-Null
    [IO.File]::WriteAllText($prestate, 'hostile-prestate', [Text.UTF8Encoding]::new($false))
    & fsutil.exe hardlink create $hardlink $prestate | Out-Null
    if ($LASTEXITCODE -ne 0 -or @(& fsutil.exe hardlink list $prestate).Count -lt 2) {
        throw 'The hostile hardlink prestate was not detected.'
    }
    New-Item -ItemType SymbolicLink -Path $symlink -Target $prestate | Out-Null
    if (((Get-Item -LiteralPath $symlink -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) {
        throw 'The hostile symlink prestate was not detected.'
    }
    [IO.File]::WriteAllText($protectedFile, '{"schema_version":1}', [Text.UTF8Encoding]::new($false))
    $stream = [IO.File]::Open($reserveFile, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $buffer = New-Object byte[] 1048576
        for ($i = 0; $i -lt 32; $i += 1) { $stream.Write($buffer, 0, $buffer.Length) }
        $stream.Flush($true)
    } finally { $stream.Dispose() }
    foreach ($path in @($root, $protectedFile, $reserveFile)) {
        $acl = Get-Acl -LiteralPath $path
        $acl.SetOwner([Security.Principal.NTAccount]::new('NT AUTHORITY\SYSTEM'))
        $acl.SetAccessRuleProtection($true, $false)
        $rule = if ((Get-Item -LiteralPath $path).PSIsContainer) {
            New-Object Security.AccessControl.FileSystemAccessRule(
                ([Security.Principal.SecurityIdentifier]::new($featureSid)), 'FullControl',
                'ContainerInherit,ObjectInherit', 'None', 'Deny'
            )
        } else {
            New-Object Security.AccessControl.FileSystemAccessRule(
                ([Security.Principal.SecurityIdentifier]::new($featureSid)), 'FullControl', 'Deny'
            )
        }
        [void]$acl.AddAccessRule($rule)
        foreach ($trusted in @('S-1-5-18', 'S-1-5-32-544', $masterSid, $brokerSid)) {
            $allow = if ((Get-Item -LiteralPath $path).PSIsContainer) {
                New-Object Security.AccessControl.FileSystemAccessRule(
                    ([Security.Principal.SecurityIdentifier]::new($trusted)), 'FullControl',
                    'ContainerInherit,ObjectInherit', 'None', 'Allow'
                )
            } else {
                New-Object Security.AccessControl.FileSystemAccessRule(
                    ([Security.Principal.SecurityIdentifier]::new($trusted)), 'FullControl', 'Allow'
                )
            }
            [void]$acl.AddAccessRule($allow)
        }
        Set-Acl -LiteralPath $path -AclObject $acl
        Assert-AccessDeniedByDacl $path $featureSid
    }
    # A restricted service SID appears in the token's restricted SID list. Grant
    # both LocalService and the exact service SID on this disposable scratch area
    # so a marker proves that the hostile payload really executed under that token.
    $allowedAcl = New-Object Security.AccessControl.DirectorySecurity
    $allowedAcl.SetAccessRuleProtection($true, $false)
    foreach ($trusted in @('S-1-5-18', 'S-1-5-32-544', 'S-1-5-19', $featureSid)) {
        $allow = New-Object Security.AccessControl.FileSystemAccessRule(
            ([Security.Principal.SecurityIdentifier]::new($trusted)), 'FullControl',
            'ContainerInherit,ObjectInherit', 'None', 'Allow'
        )
        [void]$allowedAcl.AddAccessRule($allow)
    }
    Set-Acl -LiteralPath $allowedRoot -AclObject $allowedAcl
    New-Item -ItemType Directory -Path $executorReadableRoot | Out-Null
    $executorReadableRootAcl = New-Object Security.AccessControl.DirectorySecurity
    $executorReadableRootAcl.SetOwner([Security.Principal.NTAccount]::new('NT AUTHORITY\SYSTEM'))
    $executorReadableRootAcl.SetAccessRuleProtection($true, $false)
    [void]$executorReadableRootAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
        ([Security.Principal.SecurityIdentifier]::new($featureSid)),
        'Write,Delete,ChangePermissions,TakeOwnership',
        'ContainerInherit,ObjectInherit', 'None', 'Deny'
    )))
    [void]$executorReadableRootAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
        ([Security.Principal.SecurityIdentifier]::new($featureSid)), 'ReadAndExecute', 'Allow'
    )))
    foreach ($trusted in @('S-1-5-18', 'S-1-5-32-544', $masterSid, $brokerSid)) {
        [void]$executorReadableRootAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
            ([Security.Principal.SecurityIdentifier]::new($trusted)), 'FullControl',
            'ContainerInherit,ObjectInherit', 'None', 'Allow'
        )))
    }
    Set-Acl -LiteralPath $executorReadableRoot -AclObject $executorReadableRootAcl
    [IO.File]::WriteAllText($executorReadableFile, '{"schema_version":1}', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($executorSiblingCanary, 'sibling-secret', [Text.UTF8Encoding]::new($false))
    $executorReadableAcl = New-Object Security.AccessControl.FileSecurity
    $executorReadableAcl.SetOwner([Security.Principal.NTAccount]::new('NT AUTHORITY\SYSTEM'))
    $executorReadableAcl.SetAccessRuleProtection($true, $false)
    [void]$executorReadableAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
        ([Security.Principal.SecurityIdentifier]::new($featureSid)),
        'Write,Delete,ChangePermissions,TakeOwnership', 'Deny'
    )))
    [void]$executorReadableAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
        ([Security.Principal.SecurityIdentifier]::new($featureSid)), 'ReadAndExecute', 'Allow'
    )))
    foreach ($trusted in @('S-1-5-18', 'S-1-5-32-544', $masterSid, $brokerSid)) {
        [void]$executorReadableAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
            ([Security.Principal.SecurityIdentifier]::new($trusted)), 'FullControl', 'Allow'
        )))
    }
    Set-Acl -LiteralPath $executorReadableFile -AclObject $executorReadableAcl
    $executorReadableHash = (Get-FileHash -LiteralPath $executorReadableFile -Algorithm SHA256).Hash

    # The hosted workflow runs this E2E before its general Cargo build. Build the
    # exact product binaries here, copy them into the disposable readable root,
    # and generate valid role configs through repository fixture programs.
    $repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
    Push-Location $repoRoot
    try {
        & cargo build --locked -p assemblywright-broker -p assemblywright-executor --bins
        if ($LASTEXITCODE -ne 0) { throw 'The disposable service-host binaries did not build.' }
        $builtBroker = Join-Path $repoRoot 'target\debug\assemblywright-broker.exe'
        $builtExecutor = Join-Path $repoRoot 'target\debug\assemblywright-executor.exe'
        $realBrokerImage = Join-Path $allowedRoot 'assemblywright-broker-e2e.exe'
        $realExecutorImage = Join-Path $allowedRoot 'assemblywright-executor-e2e.exe'
        Copy-Item -LiteralPath $builtBroker -Destination $realBrokerImage
        Copy-Item -LiteralPath $builtExecutor -Destination $realExecutorImage
        $realBrokerConfig = Join-Path $allowedRoot 'broker-runtime.json'
        $realExecutorConfig = Join-Path $allowedRoot 'executor-runtime.json'
        $brokerFixtureOutput = @(& cargo run --quiet --locked -p assemblywright-broker --example windows_broker_service_config_fixture -- $realBrokerConfig $realBrokerImage)
        if ($LASTEXITCODE -ne 0) { throw 'The disposable Broker service configuration fixture failed.' }
        $executorFixtureOutput = @(& cargo run --quiet --locked -p assemblywright-executor --example windows_executor_service_config_fixture -- $realExecutorConfig $realExecutorImage)
        if ($LASTEXITCODE -ne 0) { throw 'The disposable Executor service configuration fixture failed.' }
    } finally { Pop-Location }
    # The production loaders require a single-link, read-only configuration on
    # Windows. Exercise that real admission rule rather than a test-only bypass.
    (Get-Item -LiteralPath $realBrokerConfig -Force).IsReadOnly = $true
    (Get-Item -LiteralPath $realExecutorConfig -Force).IsReadOnly = $true
    $validBrokerConfigBytes = [IO.File]::ReadAllBytes($realBrokerConfig)
    $realBrokerDigest = ([string]$brokerFixtureOutput[-1]).Trim()
    $realExecutorDigest = ([string]$executorFixtureOutput[-1]).Trim()
    if ($realBrokerDigest -cnotmatch '^[0-9a-f]{64}$' -or $realExecutorDigest -cnotmatch '^[0-9a-f]{64}$') {
        throw 'A disposable service configuration fixture returned a malformed digest.'
    }

    $realBrokerCommand = ('"{0}" --service-host --service-name {1} --config "{2}" --config-sha256 {3}' -f
        $realBrokerImage, $realBrokerName, $realBrokerConfig, $realBrokerDigest)
    $realExecutorCommand = ('"{0}" --service-host --service-name {1} --config "{2}" --config-sha256 {3}' -f
        $realExecutorImage, $realExecutorName, $realExecutorConfig, $realExecutorDigest)
    [void](Invoke-Sc @('create', $realBrokerName, 'binPath=', $realBrokerCommand, 'start=', 'demand', 'obj=', 'LocalSystem'))
    $serviceCreated.Add($realBrokerName)
    [void](Invoke-Sc @('sidtype', $realBrokerName, 'unrestricted'))
    [void](Invoke-Sc @('create', $realExecutorName, 'binPath=', $realExecutorCommand, 'start=', 'demand', 'obj=', 'NT AUTHORITY\LocalService'))
    $serviceCreated.Add($realExecutorName)
    [void](Invoke-Sc @('sidtype', $realExecutorName, 'restricted'))
    # Restricted service tokens must satisfy both the ordinary LocalService ACL
    # and the restricted service-SID ACL. Grant only the randomized disposable
    # Executor SID on the disposable tree used by this lifecycle proof.
    $realExecutorSid = Get-ServiceSid $realExecutorName
    $realExecutorAcl = Get-Acl -LiteralPath $allowedRoot
    [void]$realExecutorAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
        ([Security.Principal.SecurityIdentifier]::new($realExecutorSid)),
        'Write,Delete,ChangePermissions,TakeOwnership',
        'ContainerInherit,ObjectInherit', 'None', 'Deny'
    )))
    [void]$realExecutorAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
        ([Security.Principal.SecurityIdentifier]::new($realExecutorSid)), 'ReadAndExecute', 'Allow'
    )))
    Set-Acl -LiteralPath $allowedRoot -AclObject $realExecutorAcl
    foreach ($executorLeaf in @($realExecutorImage, $realExecutorConfig)) {
        $leafAcl = New-Object Security.AccessControl.FileSecurity
        $leafAcl.SetOwner([Security.Principal.NTAccount]::new('NT AUTHORITY\SYSTEM'))
        $leafAcl.SetAccessRuleProtection($true, $false)
        [void]$leafAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
            ([Security.Principal.SecurityIdentifier]::new($realExecutorSid)),
            'Write,Delete,ChangePermissions,TakeOwnership', 'Deny'
        )))
        [void]$leafAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
            ([Security.Principal.SecurityIdentifier]::new($realExecutorSid)), 'ReadAndExecute', 'Allow'
        )))
        foreach ($trusted in @('S-1-5-18', 'S-1-5-32-544', $masterSid, $brokerSid)) {
            [void]$leafAcl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule(
                ([Security.Principal.SecurityIdentifier]::new($trusted)), 'FullControl', 'Allow'
            )))
        }
        Set-Acl -LiteralPath $executorLeaf -AclObject $leafAcl
    }
    foreach ($realService in @($realBrokerName, $realExecutorName)) {
        [void](Invoke-Sc @('start', $realService))
        $running = Wait-ServiceState $realService 'Running'
        if ([UInt32]$running.ProcessId -eq 0) { throw 'A disposable real service host reported RUNNING without a process.' }
        [void](Invoke-Sc @('stop', $realService))
        $stopped = Wait-ServiceState $realService 'Stopped'
        if ([UInt32]$stopped.ExitCode -ne 0) { throw 'A disposable real service host reported a nonzero clean-stop code.' }
    }

    # A matching byte digest is insufficient: ServiceMain must construct the
    # real runtime and reject a semantically invalid schema before RUNNING.
    (Get-Item -LiteralPath $realBrokerConfig -Force).IsReadOnly = $false
    $semanticConfig = Get-Content -LiteralPath $realBrokerConfig -Raw | ConvertFrom-Json
    $semanticConfig.schema_version = 0
    $semanticBytes = [Text.UTF8Encoding]::new($false).GetBytes(
        ($semanticConfig | ConvertTo-Json -Compress -Depth 32)
    )
    [IO.File]::WriteAllBytes($realBrokerConfig, $semanticBytes)
    (Get-Item -LiteralPath $realBrokerConfig -Force).IsReadOnly = $true
    $semanticDigest = (Get-FileHash -LiteralPath $realBrokerConfig -Algorithm SHA256).Hash.ToLowerInvariant()
    $semanticCommand = ('"{0}" --service-host --service-name {1} --config "{2}" --config-sha256 {3}' -f
        $realBrokerImage, $realBrokerName, $realBrokerConfig, $semanticDigest)
    [void](Invoke-Sc @('config', $realBrokerName, 'binPath=', $semanticCommand))
    & sc.exe start $realBrokerName 2>&1 | Out-Null
    [void](Wait-ServiceState $realBrokerName 'Stopped')
    $semanticQuery = (@(& sc.exe query $realBrokerName 2>&1) -join "`n")
    if ($semanticQuery -notmatch 'SERVICE_EXIT_CODE\s*:\s*1') {
        throw 'The real Broker service host accepted a digest-matched invalid runtime schema.'
    }
    (Get-Item -LiteralPath $realBrokerConfig -Force).IsReadOnly = $false
    [IO.File]::WriteAllBytes($realBrokerConfig, $validBrokerConfigBytes)
    (Get-Item -LiteralPath $realBrokerConfig -Force).IsReadOnly = $true
    [void](Invoke-Sc @('config', $realBrokerName, 'binPath=', $realBrokerCommand))

    # Exercise hostile config digest and argv through SCM against the actual
    # binaries. Neither failure may transiently satisfy the RUNNING contract.
    $badDigest = '00' * 32
    $badBrokerCommand = ('"{0}" --service-host --service-name {1} --config "{2}" --config-sha256 {3}' -f
        $realBrokerImage, $realBrokerName, $realBrokerConfig, $badDigest)
    [void](Invoke-Sc @('config', $realBrokerName, 'binPath=', $badBrokerCommand))
    & sc.exe start $realBrokerName 2>&1 | Out-Null
    $badDigestStartExit = $LASTEXITCODE
    $badDigestStopped = Wait-ServiceState $realBrokerName 'Stopped'
    $badDigestQuery = (@(& sc.exe query $realBrokerName 2>&1) -join "`n")
    if ($badDigestStartExit -eq 0 -and [UInt32]$badDigestStopped.ExitCode -eq 0 -or
        $badDigestQuery -notmatch 'SERVICE_EXIT_CODE\s*:\s*1') {
        throw 'The real Broker service host did not reject a hostile configuration digest.'
    }
    $badExecutorCommand = "$realExecutorCommand extra"
    [void](Invoke-Sc @('config', $realExecutorName, 'binPath=', $badExecutorCommand))
    & sc.exe start $realExecutorName 2>&1 | Out-Null
    $badArgvStartExit = $LASTEXITCODE
    $badArgvStopped = Wait-ServiceState $realExecutorName 'Stopped'
    if ($badArgvStartExit -eq 0 -and [UInt32]$badArgvStopped.ProcessId -ne 0) {
        throw 'The real Executor service host accepted hostile extra argv.'
    }

    $payloadText = @"
`$ErrorActionPreference = 'Continue'
`$result = 'started'
try {
    if ([IO.File]::ReadAllText('$($executorReadableFile.Replace("'", "''"))') -ceq '{"schema_version":1}') {
        `$result += ';read'
    }
} catch {}
try {
    if ([IO.File]::ReadAllText('$($executorSiblingCanary.Replace("'", "''"))') -ceq 'sibling-secret') {
        `$result += ';sibling-read'
    }
} catch {}
try { [IO.File]::WriteAllText('$($executorReadableFile.Replace("'", "''"))','hostile-overwrite',[Text.UTF8Encoding]::new(`$false)) } catch {}
try { [IO.File]::WriteAllText('$($protectedFile.Replace("'", "''"))','hostile-overwrite',[Text.UTF8Encoding]::new(`$false)) } catch {}
try { Remove-Item -LiteralPath '$($reserveFile.Replace("'", "''"))' -Force } catch {}
try { & sc.exe config '$masterName' start= auto | Out-Null } catch {}
[IO.File]::WriteAllText('$($marker.Replace("'", "''")),(`$result + ';attempted'),[Text.UTF8Encoding]::new(`$false))
"@
    [IO.File]::WriteAllText($payload, $payloadText, [Text.UTF8Encoding]::new($false))
    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $binPath = ('"{0}" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{1}"' -f $powershell, $payload)
    [void](Invoke-Sc @('config', $featureName, 'binPath=', $binPath))
    [void](Invoke-Sc @('config', $featureName, 'start=', 'demand'))
    $protectedHash = (Get-FileHash -LiteralPath $protectedFile -Algorithm SHA256).Hash
    $reserveLength = (Get-Item -LiteralPath $reserveFile).Length
    $masterStartMode = (Get-CimInstance Win32_Service -Filter "Name='$masterName'").StartMode
    # The payload is intentionally not a service host. SCM can report 1053 after
    # launching it; success is determined by the allowed marker plus unchanged
    # protected objects, never by that ambiguous service-start status.
    & sc.exe start $featureName 2>&1 | Out-Null
    $deadline = [DateTime]::UtcNow.AddSeconds(40)
    $observedMarker = 'missing'
    while ([DateTime]::UtcNow -lt $deadline) {
        if (Test-Path -LiteralPath $marker) {
            try {
                $observedMarker = Get-Content -LiteralPath $marker -Raw
                if ($observedMarker.EndsWith(';attempted', [StringComparison]::Ordinal)) { break }
            } catch {}
        }
        Start-Sleep -Milliseconds 100
    }
    if ($observedMarker -cne 'started;read;attempted') {
        throw "The hostile restricted-service payload did not prove execution. observed_marker=$observedMarker"
    }
    if ((Get-FileHash -LiteralPath $executorReadableFile -Algorithm SHA256).Hash -cne $executorReadableHash) {
        throw 'The restricted feature token mutated its read-only configuration grant.'
    }
    if ((Get-FileHash -LiteralPath $protectedFile -Algorithm SHA256).Hash -cne $protectedHash) {
        throw 'The hostile feature token altered a protected file.'
    }
    if (-not (Test-Path -LiteralPath $reserveFile) -or (Get-Item -LiteralPath $reserveFile).Length -ne $reserveLength) {
        throw 'The hostile feature token altered the protected disk reserve.'
    }
    if ((Get-CimInstance Win32_Service -Filter "Name='$masterName'").StartMode -cne $masterStartMode) {
        throw 'The hostile feature token altered a protected service definition.'
    }
    # Bounded pressure: retain an allocated reserve while creating CPU and memory load,
    # then require SCM queries for both protected services to remain responsive.
    $pressure = Start-Job -ScriptBlock {
        $blocks = New-Object System.Collections.Generic.List[byte[]]
        for ($i = 0; $i -lt 64; $i += 1) { $blocks.Add((New-Object byte[] 1048576)) }
        $until = [DateTime]::UtcNow.AddSeconds(2)
        while ([DateTime]::UtcNow -lt $until) { [Math]::Sqrt(123456789) | Out-Null }
    }
    try {
        $master = Get-Service -Name $masterName
        $broker = Get-Service -Name $brokerName
        if ($master.Status -ne 'Stopped' -or $broker.Status -ne 'Stopped') {
            throw 'A protected service became unqueryable under bounded resource pressure.'
        }
        Wait-Job -Job $pressure -Timeout 10 | Out-Null
    } finally {
        Stop-Job -Job $pressure -ErrorAction SilentlyContinue
        Remove-Job -Job $pressure -Force -ErrorAction SilentlyContinue
    }
    [ordered]@{
        schema_version = 1
        status = 'windows_execution_host_security_e2e_passed'
        distinct_service_sids = $true
        feature_protected_file_mutation_denied = $true
        feature_service_definition_mutation_denied = $true
        feature_reservation_mutation_denied = $true
        executor_configuration_read_allowed_mutation_denied = $true
        executor_inherited_sibling_read_denied = $true
        protected_services_queryable_under_bounded_pressure = $true
        production_services_untouched = $true
        production_provisioner_dry_run_and_check_executed = $true
        production_provisioner_self_test_passed = $true
        real_broker_service_host_started_running_and_stopped = $true
        real_executor_service_host_started_running_and_stopped = $true
        real_service_host_digest_and_argv_rejected = $true
        real_service_host_semantic_config_rejected = $true
        hostile_link_prestate_detected = $true
        effects_enabled_drift_detected = $true
        paths_disclosed = $false
    } | ConvertTo-Json -Compress
} finally {
    for ($index = $serviceCreated.Count - 1; $index -ge 0; $index -= 1) {
        & sc.exe delete $serviceCreated[$index] | Out-Null
    }
    if (Test-Path -LiteralPath $root) {
        & takeown.exe /F $root /R /D Y | Out-Null
        & icacls.exe $root /grant '*S-1-5-32-544:(OI)(CI)F' /T /C | Out-Null
        Remove-Item -LiteralPath $root -Recurse -Force
    }
    if (Test-Path -LiteralPath $allowedRoot) {
        Remove-Item -LiteralPath $allowedRoot -Recurse -Force
    }
    if (Test-Path -LiteralPath $registryFixture) {
        Remove-Item -LiteralPath $registryFixture -Recurse -Force
    }
}
