[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$MasterExe,
    [Parameter(Mandatory = $true)][string]$DataDir,
    [ValidatePattern('^[A-Za-z0-9_-]{1,64}$')][string]$ServiceName = 'AssemblywrightMaster',
    [switch]$Confirm
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $Confirm) { throw 'The real Codex planning-containment probe requires -Confirm.' }

if ($null -eq ('AssemblywrightProbeCommandLine' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class AssemblywrightProbeCommandLine {
    [DllImport("shell32.dll", SetLastError = true)]
    private static extern IntPtr CommandLineToArgvW(string commandLine, out int argumentCount);

    [DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr memory);

    public static string[] Split(string commandLine) {
        int count;
        IntPtr arguments = CommandLineToArgvW(commandLine, out count);
        if (arguments == IntPtr.Zero) {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        try {
            if (count < 1 || count > 32) {
                throw new InvalidOperationException("The service command line argument count is invalid.");
            }
            var result = new List<string>(count);
            for (int index = 0; index < count; index++) {
                IntPtr argument = Marshal.ReadIntPtr(arguments, index * IntPtr.Size);
                if (argument == IntPtr.Zero) {
                    throw new InvalidOperationException("The service command line contains a null argument.");
                }
                result.Add(Marshal.PtrToStringUni(argument));
            }
            return result.ToArray();
        } finally {
            LocalFree(arguments);
        }
    }
}
'@
}

function Get-ExactServiceHealthEndpoint {
    param(
        [Parameter(Mandatory = $true)][string]$ExactMaster,
        [Parameter(Mandatory = $true)][string]$ExactData,
        [Parameter(Mandatory = $true)][string]$ExactServiceName
    )
    $configuration = Get-CimInstance -ClassName Win32_Service -Filter "Name='$ExactServiceName'" -ErrorAction Stop
    if ($null -eq $configuration -or [string]::IsNullOrWhiteSpace($configuration.PathName)) {
        throw 'The exact production service configuration is unavailable.'
    }
    $arguments = [AssemblywrightProbeCommandLine]::Split($configuration.PathName)
    if ($arguments.Count -notin @(10,12) `
        -or $arguments[1] -cne '--data-dir' `
        -or $arguments[3] -cne 'service-run' `
        -or $arguments[4] -cne '--service-name' `
        -or $arguments[5] -cne $ExactServiceName `
        -or $arguments[6] -cne '--bind' `
        -or $arguments[8] -cne '--service-identity' `
        -or [string]::IsNullOrWhiteSpace($arguments[9])) {
        throw 'The exact production service command contract drifted.'
    }
    $configuredMaster = (Resolve-Path -LiteralPath $arguments[0] -ErrorAction Stop).ProviderPath
    $configuredData = (Resolve-Path -LiteralPath $arguments[2] -ErrorAction Stop).ProviderPath
    if ($configuredMaster -ine (Resolve-Path -LiteralPath $ExactMaster).ProviderPath `
        -or $configuredData -ine (Resolve-Path -LiteralPath $ExactData).ProviderPath) {
        throw 'The exact production service path binding drifted.'
    }
    if ($arguments.Count -eq 12) {
        $remoteEndpoint = ConvertTo-StrictIPEndPoint -Value $arguments[11]
        if ($arguments[10] -cne '--remote-bind' `
            -or $remoteEndpoint.Port -eq 0 `
            -or $remoteEndpoint.Address.Equals([Net.IPAddress]::Any) `
            -or $remoteEndpoint.Address.Equals([Net.IPAddress]::IPv6Any)) {
            throw 'The exact production remote bind contract drifted.'
        }
    }
    $healthEndpoint = ConvertTo-StrictIPEndPoint -Value $arguments[7]
    if ($healthEndpoint.Port -eq 0 `
        -or -not [Net.IPAddress]::IsLoopback($healthEndpoint.Address)) {
        throw 'The exact production health bind is not a nonzero loopback endpoint.'
    }
    $healthEndpoint.ToString()
}

function ConvertTo-StrictIPEndPoint {
    param([Parameter(Mandatory = $true)][string]$Value)
    $match = [Text.RegularExpressions.Regex]::Match(
        $Value,
        '^(?:\[(?<address>[^\]]+)\]|(?<address>[^:]+)):(?<port>[0-9]{1,5})$',
        [Text.RegularExpressions.RegexOptions]::CultureInvariant
    )
    if (-not $match.Success) { throw 'The service endpoint syntax is invalid.' }
    $address = $null
    if (-not [Net.IPAddress]::TryParse($match.Groups['address'].Value, [ref]$address)) {
        throw 'The service endpoint address is invalid.'
    }
    $port = 0
    if (-not [int]::TryParse($match.Groups['port'].Value, [ref]$port) -or $port -lt 1 -or $port -gt 65535) {
        throw 'The service endpoint port is invalid.'
    }
    [Net.IPEndPoint]::new($address, $port)
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The real Codex planning-containment probe requires an elevated owner PowerShell.'
}

$master = [IO.Path]::GetFullPath($MasterExe)
$data = [IO.Path]::GetFullPath($DataDir)
if (-not (Test-Path -LiteralPath $master -PathType Leaf)) { throw 'The exact master executable is unavailable.' }
if (-not (Test-Path -LiteralPath $data -PathType Container)) { throw 'The exact master data directory is unavailable.' }
$healthEndpoint = Get-ExactServiceHealthEndpoint -ExactMaster $master -ExactData $data -ExactServiceName $ServiceName

$service = Get-Service -Name $ServiceName -ErrorAction Stop
if ($service.Status -notin @([ServiceProcess.ServiceControllerStatus]::Running,[ServiceProcess.ServiceControllerStatus]::Stopped)) {
    throw 'The production service is in a transitional state.'
}
$restart = $service.Status -eq [ServiceProcess.ServiceControllerStatus]::Running
$receipt = $null

try {
    if ($restart) {
        & $master --data-dir $data service stop --service-name $ServiceName | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'The exact production service did not stop.' }
    }
    $service.Refresh()
    if ($service.Status -ne [ServiceProcess.ServiceControllerStatus]::Stopped) {
        throw 'The exact production service is not stopped.'
    }

    $receiptLines = @(& $master --data-dir $data planning-provider-native-probe --service-name $ServiceName --confirm)
    if ($LASTEXITCODE -ne 0 -or $receiptLines.Count -ne 1) {
        throw 'The native planning-provider probe did not return one receipt.'
    }
    $receipt = $receiptLines[0] | ConvertFrom-Json
    $expected = @(
        'schema_version','receipt_domain','status','outcome','service_name_sha256','service_runtime_binding_sha256',
        'binding_revision','provider_profile_name','provider_profile_sid',
        'service_account_sid','current_token_sid','provisioning_owner_sid','health_endpoint',
        'brainstorming_binding_sha256','catalog_sha256','codex_executable_sha256',
        'output_schema_sha256','probe_contract_sha256','login_exit_code',
        'login_diagnostic_code','exec_exit_code','exec_diagnostic_code','output_sha256',
        'live_evidence_required'
    )
    $actual = @($receipt.PSObject.Properties.Name)
    if (@(Compare-Object -ReferenceObject $expected -DifferenceObject $actual).Count -ne 0) {
        throw 'The native planning-provider receipt shape drifted.'
    }
    $allowedOutcomes = @(
        'login_failed','cancelled_during_login','cancelled_before_exec',
        'exec_failed','cancelled_during_exec','structured_output_rejected','succeeded'
    )
    if ($receipt.schema_version -ne 1 `
        -or $receipt.receipt_domain -cne 'assemblywright.windows-planning-real-codex-probe.v1' `
        -or $receipt.status -cne 'planning_provider_native_probe' `
        -or $receipt.outcome -cnotin $allowedOutcomes) {
        throw 'The native planning-provider receipt identity is invalid.'
    }
    foreach ($hash in @(
        $receipt.service_name_sha256,$receipt.service_runtime_binding_sha256,
        $receipt.brainstorming_binding_sha256,$receipt.catalog_sha256,
        $receipt.codex_executable_sha256,$receipt.output_schema_sha256,$receipt.probe_contract_sha256
    )) {
        if ($hash -cnotmatch '^[0-9a-f]{64}$') { throw 'The native planning-provider hash binding is invalid.' }
    }
    if ($null -ne $receipt.output_sha256 -and $receipt.output_sha256 -cnotmatch '^[0-9a-f]{64}$') {
        throw 'The native planning-provider output hash binding is invalid.'
    }
    if ($receipt.provider_profile_name -cne 'Assemblywright.Planning.Provider.v1') {
        throw 'The native planning-provider profile binding is invalid.'
    }
    foreach ($sid in @($receipt.provider_profile_sid,$receipt.service_account_sid,$receipt.current_token_sid,$receipt.provisioning_owner_sid)) {
        if ($sid -cnotmatch '^S-1-[0-9]+(?:-[0-9]+)+$') { throw 'The native planning-provider SID binding is invalid.' }
    }
    if ($receipt.service_account_sid -cne $receipt.current_token_sid `
        -or $receipt.current_token_sid -cne $receipt.provisioning_owner_sid) {
        throw 'The native planning-provider owner SID bindings drifted.'
    }
    if ($receipt.health_endpoint -cnotmatch '^(?:127\.0\.0\.1|\[::1\]):[0-9]{1,5}$') {
        throw 'The native planning-provider health binding is invalid.'
    }
    if ($receipt.health_endpoint -cne $healthEndpoint) {
        throw 'The native planning-provider health binding differs from the SCM preflight.'
    }
} finally {
    if ($restart) {
        & $master --data-dir $data service start --service-name $ServiceName | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'The exact production service did not restart after the probe.' }
        & $master --data-dir $data health --endpoint $healthEndpoint | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'The production runtime did not recover after the probe.' }
    }
}

if ($null -eq $receipt) { throw 'The native planning-provider probe produced no receipt.' }
$receipt | ConvertTo-Json -Compress
