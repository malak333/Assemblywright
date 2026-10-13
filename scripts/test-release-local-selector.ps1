[CmdletBinding()]
param(
    [string]$EvidencePath = ""
)

$ErrorActionPreference = "Stop"
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$selector = Join-Path $scriptRoot "select-release-local.ps1"
if (-not (Test-Path -LiteralPath $selector -PathType Leaf)) {
    throw "Release-local selector is missing."
}

$tempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
$testRoot = [System.IO.Path]::GetFullPath((Join-Path $tempBase ("assemblywright-release-local-selector-" + [guid]::NewGuid().ToString("N"))))
$testRootParent = [System.IO.Directory]::GetParent($testRoot)
if ($null -eq $testRootParent -or
    -not $testRootParent.FullName.Equals($tempBase, [System.StringComparison]::OrdinalIgnoreCase) -or
    -not ([System.IO.Path]::GetFileName($testRoot) -match '^assemblywright-release-local-selector-[0-9a-f]{32}$')) {
    throw "Refusing to create selector tests outside the expected temporary directory."
}
[System.IO.Directory]::CreateDirectory($testRoot) | Out-Null
$results = New-Object System.Collections.Generic.List[object]

function New-EventFile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [object]$Value
    )

    $path = Join-Path $testRoot ($Name + ".json")
    $Value | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $path -Encoding UTF8
    return $path
}

function Invoke-SelectorCase {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$EventName,

        [Parameter(Mandatory = $true)]
        [string]$EventPath,

        [string]$Ref = "",

        [Parameter(Mandatory = $true)]
        [bool]$ExpectedRunMac,

        [Parameter(Mandatory = $true)]
        [string]$ExpectedDecision,

        [Parameter(Mandatory = $true)]
        [string]$ExpectedReason
    )

    $raw = & $selector -EventName $EventName -EventPath $EventPath -Ref $Ref
    try {
        $actual = ($raw | Out-String) | ConvertFrom-Json -ErrorAction Stop
    }
    catch {
        throw "Selector returned malformed output for ${Name}: $raw"
    }

    if ([bool]$actual.run_mac -ne $ExpectedRunMac) {
        throw "$Name run_mac mismatch: expected $ExpectedRunMac, got $($actual.run_mac)."
    }
    if ([string]$actual.decision -ne $ExpectedDecision) {
        throw "$Name decision mismatch: expected $ExpectedDecision, got $($actual.decision)."
    }
    if ([string]$actual.reason -ne $ExpectedReason) {
        throw "$Name reason mismatch: expected $ExpectedReason, got $($actual.reason)."
    }

    $expectedCheckName = if ($ExpectedDecision -eq "ignore") {
        "Release local gate (irrelevant label event)"
    }
    else {
        "Release local gate"
    }
    if ([string]$actual.check_name -ne $expectedCheckName) {
        throw "$Name check_name mismatch: expected $expectedCheckName, got $($actual.check_name)."
    }
    if ($ExpectedDecision -eq "ignore" -and [string]$actual.check_name -eq "Release local gate") {
        throw "$Name must not publish the protected Mac check context."
    }

    $results.Add([ordered]@{
        name = $Name
        run_mac = [bool]$actual.run_mac
        decision = [string]$actual.decision
        reason = [string]$actual.reason
        check_name = [string]$actual.check_name
    }) | Out-Null
}

try {
    $prNoLabel = New-EventFile -Name "pr-no-label" -Value @{
        action = "synchronize"
        pull_request = @{ labels = @() }
    }
    $prWithLabel = New-EventFile -Name "pr-with-label" -Value @{
        action = "synchronize"
        pull_request = @{ labels = @(@{ name = "windows-only" }) }
    }
    $prOpenedLabel = New-EventFile -Name "pr-opened-label" -Value @{
        action = "opened"
        pull_request = @{ labels = @(@{ name = "Windows-Only" }) }
    }
    $labeledWindows = New-EventFile -Name "labeled-windows" -Value @{
        action = "labeled"
        label = @{ name = "windows-only" }
        pull_request = @{ labels = @(@{ name = "windows-only" }) }
    }
    $unlabeledWindows = New-EventFile -Name "unlabeled-windows" -Value @{
        action = "unlabeled"
        label = @{ name = "windows-only" }
        pull_request = @{ labels = @() }
    }
    $labeledUnrelated = New-EventFile -Name "labeled-unrelated" -Value @{
        action = "labeled"
        label = @{ name = "documentation" }
        pull_request = @{ labels = @() }
    }
    $unlabeledUnrelated = New-EventFile -Name "unlabeled-unrelated" -Value @{
        action = "unlabeled"
        label = @{ name = "documentation" }
        pull_request = @{ labels = @() }
    }
    $labeledMissingName = New-EventFile -Name "labeled-missing-name" -Value @{
        action = "labeled"
        pull_request = @{ labels = @() }
    }
    $unlabeledWhitespaceName = New-EventFile -Name "unlabeled-whitespace-name" -Value @{
        action = "unlabeled"
        label = @{ name = "   " }
        pull_request = @{ labels = @() }
    }
    $unknownPrAction = New-EventFile -Name "unknown-pr-action" -Value @{
        action = "edited"
        pull_request = @{ labels = @(@{ name = "windows-only" }) }
    }
    $mainMarker = New-EventFile -Name "main-marker" -Value @{
        ref = "refs/heads/main"
        head_commit = @{ message = "Merge Windows lane [windows-only]" }
    }
    $mainDefault = New-EventFile -Name "main-default" -Value @{
        ref = "refs/heads/main"
        head_commit = @{ message = "Merge complete feature" }
    }
    $mainWrongCase = New-EventFile -Name "main-wrong-case" -Value @{
        ref = "refs/heads/main"
        head_commit = @{ message = "Merge Windows lane [WINDOWS-ONLY]" }
    }
    $offMainMarker = New-EventFile -Name "off-main-marker" -Value @{
        ref = "refs/heads/feature"
        head_commit = @{ message = "Feature work [windows-only]" }
    }
    $manual = New-EventFile -Name "manual" -Value @{}
    $unknown = New-EventFile -Name "unknown" -Value @{}
    $malformed = Join-Path $testRoot "malformed.json"
    Set-Content -LiteralPath $malformed -Value "{" -Encoding UTF8
    $missing = Join-Path $testRoot "missing.json"

    Invoke-SelectorCase -Name "pr_synchronize_full" -EventName "pull_request" -EventPath $prNoLabel -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "pull_request_full"
    Invoke-SelectorCase -Name "pr_synchronize_windows_only" -EventName "pull_request" -EventPath $prWithLabel -ExpectedRunMac $false -ExpectedDecision "skip" -ExpectedReason "pull_request_windows_only"
    Invoke-SelectorCase -Name "pr_opened_case_insensitive_label" -EventName "pull_request" -EventPath $prOpenedLabel -ExpectedRunMac $false -ExpectedDecision "skip" -ExpectedReason "pull_request_windows_only"
    Invoke-SelectorCase -Name "pr_labeled_windows_only" -EventName "pull_request" -EventPath $labeledWindows -ExpectedRunMac $false -ExpectedDecision "skip" -ExpectedReason "pull_request_windows_only"
    Invoke-SelectorCase -Name "pr_unlabeled_windows_only" -EventName "pull_request" -EventPath $unlabeledWindows -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "pull_request_windows_only_removed"
    Invoke-SelectorCase -Name "pr_labeled_unrelated" -EventName "pull_request" -EventPath $labeledUnrelated -ExpectedRunMac $false -ExpectedDecision "ignore" -ExpectedReason "unrelated_label_event"
    Invoke-SelectorCase -Name "pr_unlabeled_unrelated" -EventName "pull_request" -EventPath $unlabeledUnrelated -ExpectedRunMac $false -ExpectedDecision "ignore" -ExpectedReason "unrelated_label_event"
    Invoke-SelectorCase -Name "pr_labeled_missing_name_fail_full" -EventName "pull_request" -EventPath $labeledMissingName -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "missing_label_fail_full"
    Invoke-SelectorCase -Name "pr_unlabeled_whitespace_name_fail_full" -EventName "pull_request" -EventPath $unlabeledWhitespaceName -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "missing_label_fail_full"
    Invoke-SelectorCase -Name "pr_unknown_action_fail_full" -EventName "pull_request" -EventPath $unknownPrAction -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "unknown_pull_request_action_fail_full"
    Invoke-SelectorCase -Name "push_main_marker" -EventName "push" -EventPath $mainMarker -Ref "refs/heads/main" -ExpectedRunMac $false -ExpectedDecision "skip" -ExpectedReason "main_commit_windows_only"
    Invoke-SelectorCase -Name "push_main_default" -EventName "push" -EventPath $mainDefault -Ref "refs/heads/main" -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "main_push_full"
    Invoke-SelectorCase -Name "push_marker_is_case_sensitive" -EventName "push" -EventPath $mainWrongCase -Ref "refs/heads/main" -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "main_push_full"
    Invoke-SelectorCase -Name "push_off_main_fail_full" -EventName "push" -EventPath $offMainMarker -Ref "refs/heads/feature" -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "non_main_push_fail_full"
    Invoke-SelectorCase -Name "manual_always_full" -EventName "workflow_dispatch" -EventPath $manual -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "manual_full"
    Invoke-SelectorCase -Name "unknown_event_fail_full" -EventName "schedule" -EventPath $unknown -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "unknown_event_fail_full"
    Invoke-SelectorCase -Name "malformed_event_fail_full" -EventName "push" -EventPath $malformed -Ref "refs/heads/main" -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "malformed_event_fail_full"
    Invoke-SelectorCase -Name "missing_event_fail_full" -EventName "push" -EventPath $missing -Ref "refs/heads/main" -ExpectedRunMac $true -ExpectedDecision "run" -ExpectedReason "missing_event_fail_full"

    $report = [ordered]@{
        status = "passed"
        scenario_count = $results.Count
        scenarios = @($results | ForEach-Object { $_ })
    }

    if ($EvidencePath) {
        $parent = Split-Path -Parent $EvidencePath
        if ($parent) {
            [System.IO.Directory]::CreateDirectory($parent) | Out-Null
        }
        $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $EvidencePath -Encoding UTF8
    }

    $report | ConvertTo-Json -Depth 8
}
finally {
    if (Test-Path -LiteralPath $testRoot) {
        $cleanupRoot = [System.IO.Path]::GetFullPath((Resolve-Path -LiteralPath $testRoot).Path)
        $cleanupParent = [System.IO.Directory]::GetParent($cleanupRoot)
        if ($null -eq $cleanupParent -or
            -not $cleanupParent.FullName.Equals($tempBase, [System.StringComparison]::OrdinalIgnoreCase) -or
            -not ([System.IO.Path]::GetFileName($cleanupRoot) -match '^assemblywright-release-local-selector-[0-9a-f]{32}$')) {
            throw "Refusing to recursively remove an unexpected selector test path."
        }
        Remove-Item -LiteralPath $testRoot -Recurse -Force
    }
}
