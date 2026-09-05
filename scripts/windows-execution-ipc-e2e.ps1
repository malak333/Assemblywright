[CmdletBinding()]
param([switch]$Confirm)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $Confirm) { throw 'The disposable Windows execution IPC E2E requires -Confirm.' }

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The disposable Windows execution IPC E2E requires elevated SCM access.'
}

$baseSuffix = [Guid]::NewGuid().ToString('N').Substring(0,10)
$createdServices = [Collections.Generic.List[string]]::new()
$roots = [Collections.Generic.List[string]]::new()
$brokerImage = Join-Path $PSScriptRoot '..\target\debug\assemblywright-broker.exe'
$executorImage = Join-Path $PSScriptRoot '..\target\debug\assemblywright-executor.exe'
$masterFixture = Join-Path $PSScriptRoot '..\target\debug\examples\windows_execution_ipc_master_service_fixture.exe'

function Invoke-Sc([string[]]$Arguments) {
    $output = @(& sc.exe @Arguments 2>&1)
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        if ($Arguments[0] -ceq 'start' -and $Arguments.Count -eq 2) {
            throw "SCM start failed for service $($Arguments[1]) (exit=$exitCode): $($output -join ' ')"
        }
        throw "SCM command failed: $($Arguments[0]) (exit=$exitCode)"
    }
    return $output
}

function Get-ServiceSid([string]$Name) {
    return ([Security.Principal.NTAccount]::new("NT SERVICE\$Name")).Translate(
        [Security.Principal.SecurityIdentifier]
    ).Value
}

function Wait-ServiceState([string]$Name, [string]$Expected, [int]$TimeoutSeconds = 30) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $service = Get-CimInstance Win32_Service -Filter "Name='$Name'" -ErrorAction SilentlyContinue
        if ($null -ne $service -and $service.State -ceq $Expected) { return $service }
        if (
            $Expected -ceq 'Running' -and
            $null -ne $service -and
            $service.State -ceq 'Stopped' -and
            ($service.ExitCode -ne 0 -or $service.ServiceSpecificExitCode -ne 0)
        ) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    $observed = if ($null -eq $service) {
        'missing'
    } else {
        "state=$($service.State),exit=$($service.ExitCode),service_exit=$($service.ServiceSpecificExitCode),pid=$($service.ProcessId)"
    }
    throw "Service $Name did not reach $Expected ($observed)."
}

function Remove-FixtureService([string]$Name) {
    & sc.exe stop $Name 2>$null | Out-Null
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $service = Get-CimInstance Win32_Service -Filter "Name='$Name'" -ErrorAction SilentlyContinue
        if ($null -eq $service -or ($service.State -ceq 'Stopped' -and $service.ProcessId -eq 0)) {
            break
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($null -ne $service -and ($service.State -cne 'Stopped' -or $service.ProcessId -ne 0)) {
        throw "Fixture service $Name did not stop and exit during cleanup."
    }
    & sc.exe delete $Name 2>$null | Out-Null
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $service = Get-CimInstance Win32_Service -Filter "Name='$Name'" -ErrorAction SilentlyContinue
        if ($null -eq $service) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Fixture service $Name remained registered after cleanup."
}

function Protect-Tree([string]$Root, [string]$BrokerSid, [string]$ExecutorSid) {
    $brokerRoot = Join-Path $Root 'broker'
    $executorRoot = Join-Path $Root 'executor'
    $receiptRoot = Join-Path $Root 'receipt'
    New-Item -ItemType Directory -Force -Path $brokerRoot,$executorRoot,$receiptRoot | Out-Null
    & icacls.exe $Root '/inheritance:r' '/grant:r' '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Failed to protect the IPC fixture root.' }
    & icacls.exe $Root '/grant' "*$ExecutorSid`:(RX)" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Failed to grant Executor root traversal.' }
    & icacls.exe $brokerRoot '/grant' "*$BrokerSid`:(OI)(CI)F" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Failed to grant the Broker fixture root.' }
    & icacls.exe $executorRoot '/grant' "*$ExecutorSid`:(OI)(CI)F" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Failed to grant the Executor fixture root.' }
}

function New-Fixture(
    [string]$Scenario,
    [int]$Index,
    [bool]$WrongSid = $false,
    [bool]$LocalServiceClient = $false
) {
    $suffix = "$baseSuffix$Index"
    $masterName = "AssemblywrightMasterE2E$suffix"
    $brokerName = "AssemblywrightBrokerE2E$suffix"
    $executorName = "AssemblywrightExecutorE2E$suffix"
    $wrongName = "AssemblywrightMasterE2EW$suffix"
    $fixtureTemp = if ($LocalServiceClient) {
        Join-Path $env:SystemRoot 'Temp'
    } else {
        [IO.Path]::GetTempPath()
    }
    $root = Join-Path $fixtureTemp "AssemblywrightExecutionIpcE2E-$suffix"
    $roots.Add($root)
    New-Item -ItemType Directory -Path $root | Out-Null

    Invoke-Sc @('create',$brokerName,'binPath=',"`"$brokerImage`" --service-host --service-name $brokerName --config pending --config-sha256 $('0' * 64)",'start=','demand','obj=','LocalSystem') | Out-Null
    $createdServices.Add($brokerName)
    Invoke-Sc @('sidtype',$brokerName,'unrestricted') | Out-Null
    Invoke-Sc @('create',$executorName,'binPath=',"`"$executorImage`" --service-host --service-name $executorName --config pending --config-sha256 $('0' * 64)",'start=','demand','obj=','NT AUTHORITY\LocalService') | Out-Null
    $createdServices.Add($executorName)
    Invoke-Sc @('sidtype',$executorName,'restricted') | Out-Null
    $masterAccount = if ($LocalServiceClient) { 'NT AUTHORITY\LocalService' } else { 'LocalSystem' }
    Invoke-Sc @('create',$masterName,'binPath=',"`"$masterFixture`" --service-name $masterName --pipe pending --broker-sid S-1-5-18 --receipt pending --scenario $Scenario",'start=','demand','obj=',$masterAccount) | Out-Null
    $createdServices.Add($masterName)
    Invoke-Sc @('sidtype',$masterName,'unrestricted') | Out-Null
    if ($WrongSid) {
        Invoke-Sc @('create',$wrongName,'binPath=',"`"$masterFixture`" --service-name $wrongName --pipe pending --broker-sid S-1-5-18 --receipt pending --scenario wrong_sid",'start=','demand','obj=','LocalSystem') | Out-Null
        $createdServices.Add($wrongName)
        Invoke-Sc @('sidtype',$wrongName,'unrestricted') | Out-Null
    }

    $runningMasterSid = Get-ServiceSid $masterName
    $masterSid = if ($WrongSid) { Get-ServiceSid $wrongName } else { $runningMasterSid }
    $brokerSid = Get-ServiceSid $brokerName
    $executorSid = Get-ServiceSid $executorName
    Protect-Tree $root $brokerSid $executorSid
    $masterRunImage = $masterFixture
    if ($LocalServiceClient) {
        $masterRunImage = Join-Path $root 'assemblywright-master-fixture.exe'
        Copy-Item -LiteralPath $masterFixture -Destination $masterRunImage
        if (
            (Get-FileHash -LiteralPath $masterFixture -Algorithm SHA256).Hash -cne
            (Get-FileHash -LiteralPath $masterRunImage -Algorithm SHA256).Hash
        ) { throw 'LocalService Master fixture copy did not preserve exact executable bytes.' }
        & icacls.exe $root '/grant' "*$runningMasterSid`:(RX)" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Failed to grant LocalService Master root traversal.' }
        & icacls.exe $masterRunImage '/grant:r' "*$runningMasterSid`:(RX)" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Failed to grant LocalService Master fixture execution.' }
    }
    $brokerRoot = Join-Path $root 'broker'
    $executorRoot = Join-Path $root 'executor'
    $receipt = Join-Path (Join-Path $root 'receipt') 'receipt.json'
    if ($LocalServiceClient) {
        & icacls.exe (Join-Path $root 'receipt') '/grant' "*$runningMasterSid`:(OI)(CI)F" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Failed to grant the hostile LocalService fixture its receipt path.' }
    }
    $brokerConfig = Join-Path $brokerRoot 'config.json'
    $brokerState = Join-Path $brokerRoot 'ipc.journal'
    $brokerSeed = Join-Path $brokerRoot 'ack.seed'
    $executorConfig = Join-Path $executorRoot 'config.json'
    $executorState = Join-Path $executorRoot 'ipc.journal'
    $executorSeed = Join-Path $executorRoot 'ack.seed'
    $brokerRunImage = Join-Path $brokerRoot 'assemblywright-broker.exe'
    $executorRunImage = Join-Path $executorRoot 'assemblywright-executor.exe'
    Copy-Item -LiteralPath $brokerImage -Destination $brokerRunImage
    Copy-Item -LiteralPath $executorImage -Destination $executorRunImage
    $brokerPipe = "\\.\pipe\Assemblywright.MasterBroker.$suffix"
    $executorPipe = "\\.\pipe\Assemblywright.BrokerExecutor.$suffix"

    $brokerDigest = (@(& cargo run --quiet --locked -p assemblywright-broker --example windows_broker_ipc_config_fixture -- $brokerConfig $brokerRunImage $executorRunImage $brokerState $brokerSeed $brokerPipe $masterSid $executorPipe $executorSid $brokerSid) | Select-Object -Last 1).Trim()
    if ($LASTEXITCODE -ne 0 -or $brokerDigest -notmatch '^[0-9a-f]{64}$') { throw 'Broker IPC fixture generation failed.' }
    $executorDigest = (@(& cargo run --quiet --locked -p assemblywright-executor --example windows_executor_ipc_config_fixture -- $executorConfig $executorRunImage $brokerRunImage $executorState $executorSeed $executorPipe $brokerSid $executorSid) | Select-Object -Last 1).Trim()
    if ($LASTEXITCODE -ne 0 -or $executorDigest -notmatch '^[0-9a-f]{64}$') { throw 'Executor IPC fixture generation failed.' }
    & attrib.exe +R $brokerConfig
    & attrib.exe +R $executorConfig
    Invoke-Sc @('config',$brokerName,'binPath=',"`"$brokerRunImage`" --service-host --service-name $brokerName --config `"$brokerConfig`" --config-sha256 $brokerDigest") | Out-Null
    Invoke-Sc @('config',$executorName,'binPath=',"`"$executorRunImage`" --service-host --service-name $executorName --config `"$executorConfig`" --config-sha256 $executorDigest") | Out-Null
    $clientPipe = if ($LocalServiceClient) { $executorPipe } else { $brokerPipe }
    $clientServerSid = if ($LocalServiceClient) { $executorSid } else { $brokerSid }
    Invoke-Sc @('config',$masterName,'binPath=',"`"$masterRunImage`" --service-name $masterName --pipe $clientPipe --broker-sid $clientServerSid --receipt `"$receipt`" --scenario $Scenario") | Out-Null

    Invoke-Sc @('start',$executorName) | Out-Null
    Wait-ServiceState $executorName 'Running' | Out-Null
    Invoke-Sc @('start',$brokerName) | Out-Null
    Wait-ServiceState $brokerName 'Running' | Out-Null
    Invoke-Sc @('start',$masterName) | Out-Null
    Wait-ServiceState $masterName 'Stopped' | Out-Null
    if (-not (Test-Path -LiteralPath $receipt -PathType Leaf)) {
        $failurePath = [IO.Path]::ChangeExtension($receipt, 'failure.txt')
        $failure = if (Test-Path -LiteralPath $failurePath -PathType Leaf) {
            Get-Content -LiteralPath $failurePath -Raw
        } else { 'no_exchange_diagnostic' }
        $brokerStatus = Get-CimInstance Win32_Service -Filter "Name='$brokerName'" -ErrorAction SilentlyContinue
        $executorStatus = Get-CimInstance Win32_Service -Filter "Name='$executorName'" -ErrorAction SilentlyContinue
        $brokerDiagnostic = if ($null -eq $brokerStatus) { 'missing' } else {
            $serviceExitHex = '0x{0:X8}' -f [uint32]$brokerStatus.ServiceSpecificExitCode
            "state=$($brokerStatus.State),exit=$($brokerStatus.ExitCode),service_exit=$($brokerStatus.ServiceSpecificExitCode)/$serviceExitHex,pid=$($brokerStatus.ProcessId)"
        }
        $executorDiagnostic = if ($null -eq $executorStatus) { 'missing' } else {
            $serviceExitHex = '0x{0:X8}' -f [uint32]$executorStatus.ServiceSpecificExitCode
            "state=$($executorStatus.State),exit=$($executorStatus.ExitCode),service_exit=$($executorStatus.ServiceSpecificExitCode)/$serviceExitHex,pid=$($executorStatus.ProcessId)"
        }
        throw "Scenario $Scenario emitted no receipt: $failure; broker=[$brokerDiagnostic]; executor=[$executorDiagnostic]"
    }
    $result = Get-Content -Raw -LiteralPath $receipt | ConvertFrom-Json
    if ($result.effects_applied -ne 0) { throw "Scenario $Scenario reported an effect." }
    if ($Scenario -in @('valid','replay','delayed_write','delayed_read')) {
        if ($result.status -cne 'windows_execution_ipc_inert_roundtrip_passed' -or $result.rejected_before_ack) {
            throw "Scenario $Scenario failed the inert roundtrip."
        }
    } elseif ($Scenario -ceq 'stalled_read') {
        $brokerStatus = Get-CimInstance Win32_Service -Filter "Name='$brokerName'" -ErrorAction SilentlyContinue
        $deliveryTimeoutCode = [Convert]::ToUInt32('A8000079', 16)
        if (
            $result.status -cne 'windows_execution_ipc_delivery_timeout_observed' -or
            $result.rejected_before_ack -or
            $null -eq $brokerStatus -or
            $brokerStatus.State -cne 'Stopped' -or
            [uint32]$brokerStatus.ServiceSpecificExitCode -ne $deliveryTimeoutCode
        ) { throw 'Stalled response reader did not produce the exact bounded Broker delivery timeout.' }
    } elseif ($result.status -cne 'windows_execution_ipc_rejected' -or -not $result.rejected_before_ack) {
        throw "Scenario $Scenario did not reject before acknowledgement."
    }
    return [pscustomobject]@{
        Master = $masterName; Broker = $brokerName; Executor = $executorName
        Receipt = $receipt; Root = $root; BrokerAck = $result.broker_ack_id
        ExecutorAck = $result.executor_ack_id; BrokerPipe = $brokerPipe; BrokerSid = $brokerSid
        BrokerFrame = $result.broker_frame_sha256; ExecutorFrame = $result.executor_frame_sha256
    }
}

try {
    & cargo build --locked -p assemblywright-broker --bin assemblywright-broker --example windows_broker_ipc_config_fixture
    if ($LASTEXITCODE -ne 0) { throw 'Failed to build Broker IPC fixtures.' }
    & cargo build --locked -p assemblywright-executor --bin assemblywright-executor --example windows_executor_ipc_config_fixture
    if ($LASTEXITCODE -ne 0) { throw 'Failed to build Executor IPC fixtures.' }
    & cargo build --locked -p assemblywright-master --example windows_execution_ipc_master_service_fixture
    if ($LASTEXITCODE -ne 0) { throw 'Failed to build Master IPC fixture.' }

    $valid = New-Fixture 'valid' 0
    Invoke-Sc @('stop',$valid.Broker) | Out-Null
    Wait-ServiceState $valid.Broker 'Stopped' | Out-Null
    Invoke-Sc @('stop',$valid.Executor) | Out-Null
    Wait-ServiceState $valid.Executor 'Stopped' | Out-Null
    Invoke-Sc @('start',$valid.Executor) | Out-Null
    Wait-ServiceState $valid.Executor 'Running' | Out-Null
    Invoke-Sc @('start',$valid.Broker) | Out-Null
    Wait-ServiceState $valid.Broker 'Running' | Out-Null
    Invoke-Sc @('config',$valid.Master,'binPath=',"`"$masterFixture`" --service-name $($valid.Master) --pipe $($valid.BrokerPipe) --broker-sid $($valid.BrokerSid) --receipt `"$($valid.Receipt)`" --scenario replay") | Out-Null
    Invoke-Sc @('start',$valid.Master) | Out-Null
    Wait-ServiceState $valid.Master 'Stopped' | Out-Null
    $replay = Get-Content -Raw -LiteralPath $valid.Receipt | ConvertFrom-Json
    if ($replay.status -cne 'windows_execution_ipc_inert_roundtrip_passed' -or
        $replay.broker_ack_id -cne $valid.BrokerAck -or $replay.executor_ack_id -cne $valid.ExecutorAck) {
        throw 'Restart replay did not return the exact durable acknowledgements.'
    }

    New-Fixture 'delayed_write' 1 | Out-Null
    New-Fixture 'delayed_read' 2 | Out-Null
    $stalled = New-Fixture 'stalled_read' 3
    Invoke-Sc @('start',$stalled.Broker) | Out-Null
    Wait-ServiceState $stalled.Broker 'Running' | Out-Null
    Invoke-Sc @('config',$stalled.Master,'binPath=',"`"$masterFixture`" --service-name $($stalled.Master) --pipe $($stalled.BrokerPipe) --broker-sid $($stalled.BrokerSid) --receipt `"$($stalled.Receipt)`" --scenario replay") | Out-Null
    Invoke-Sc @('start',$stalled.Master) | Out-Null
    Wait-ServiceState $stalled.Master 'Stopped' | Out-Null
    $stalledReplay = Get-Content -Raw -LiteralPath $stalled.Receipt | ConvertFrom-Json
    if (
        $stalledReplay.status -cne 'windows_execution_ipc_inert_roundtrip_passed' -or
        $stalledReplay.rejected_before_ack -or
        (Compare-Object @($stalled.BrokerFrame) @($stalledReplay.broker_frame_sha256)) -or
        (Compare-Object @($stalled.ExecutorFrame) @($stalledReplay.executor_frame_sha256)) -or
        ($null -ne $stalled.BrokerAck -and $stalledReplay.broker_ack_id -cne $stalled.BrokerAck) -or
        ($null -ne $stalled.ExecutorAck -and $stalledReplay.executor_ack_id -cne $stalled.ExecutorAck)
    ) { throw 'Stalled-reader restart did not recover the exact durable frame acknowledgements.' }
    Invoke-Sc @('stop',$stalled.Broker) | Out-Null
    Wait-ServiceState $stalled.Broker 'Stopped' | Out-Null
    Invoke-Sc @('start',$stalled.Broker) | Out-Null
    Wait-ServiceState $stalled.Broker 'Running' | Out-Null
    Invoke-Sc @('start',$stalled.Master) | Out-Null
    Wait-ServiceState $stalled.Master 'Stopped' | Out-Null
    $stalledReplayAgain = Get-Content -Raw -LiteralPath $stalled.Receipt | ConvertFrom-Json
    if (
        $stalledReplayAgain.broker_ack_id -cne $stalledReplay.broker_ack_id -or
        $stalledReplayAgain.executor_ack_id -cne $stalledReplay.executor_ack_id
    ) { throw 'Stalled-reader durable acknowledgement IDs changed after restart replay.' }
    $index = 4
    foreach ($scenario in @('unsigned','tampered','gap','stale','stale_authority')) {
        New-Fixture $scenario $index | Out-Null
        $index += 1
    }
    New-Fixture 'wrong_sid' $index $true | Out-Null
    $index += 1
    New-Fixture 'localservice_dacl_denied' $index $true $true | Out-Null

    [ordered]@{
        schema_version = 1
        status = 'windows_execution_ipc_native_e2e_passed'
        three_service_scm_roundtrip = $true
        service_sid_peer_authentication = $true
        server_self_sid_dacl_binding = $true
        unrelated_localservice_open_and_write_dac_denied = $true
        client_impersonation_level = 'identification_only'
        delayed_client_write_authenticated = $true
        delayed_response_reader_authenticated = $true
        stalled_response_reader_timed_out = $true
        stalled_reader_restart_exact_ack_replay = $true
        remote_clients_rejected_by_pipe_mode = $true
        unsigned_tampered_gap_stale_authority_rejected = $true
        restart_exact_ack_replay = $true
        effects_applied = 0
    } | ConvertTo-Json -Compress
} finally {
    $cleanupFailures = [Collections.Generic.List[string]]::new()
    foreach ($name in $createdServices) {
        try { Remove-FixtureService $name } catch { $cleanupFailures.Add($_.Exception.Message) }
    }
    foreach ($root in $roots) {
        $userTempFixture = "$([IO.Path]::GetTempPath())AssemblywrightExecutionIpcE2E-*"
        $systemTempFixture = "$(Join-Path $env:SystemRoot 'Temp')\AssemblywrightExecutionIpcE2E-*"
        if ($root -like $userTempFixture -or $root -like $systemTempFixture) {
            try {
                Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction Stop
                if (Test-Path -LiteralPath $root) {
                    throw "Fixture root remained after cleanup: $root"
                }
            } catch {
                $cleanupFailures.Add($_.Exception.Message)
            }
        }
    }
    if ($cleanupFailures.Count -ne 0) {
        throw "Windows execution IPC E2E cleanup failed: $($cleanupFailures -join '; ')"
    }
}
