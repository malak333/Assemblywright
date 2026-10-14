[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$EventName,

    [Parameter(Mandatory = $true)]
    [string]$EventPath,

    [string]$Ref = ""
)

$selectionResult = [ordered]@{
    run_mac = $true
    decision = "run"
    reason = "fail_full"
    check_name = "Release local gate"
}

function Write-Decision {
    param(
        [Parameter(Mandatory = $true)]
        [bool]$RunMac,

        [Parameter(Mandatory = $true)]
        [ValidateSet("run", "skip", "ignore")]
        [string]$Decision,

        [Parameter(Mandatory = $true)]
        [string]$Reason
    )

    $selectionResult["run_mac"] = $RunMac
    $selectionResult["decision"] = $Decision
    $selectionResult["reason"] = $Reason
    $selectionResult["check_name"] = if ($Decision -eq "ignore") {
        "Release local gate (irrelevant label event)"
    }
    else {
        "Release local gate"
    }
    $selectionResult | ConvertTo-Json -Compress
}

if ($EventName -eq "workflow_dispatch") {
    Write-Decision -RunMac $true -Decision "run" -Reason "manual_full"
    return
}

if (-not (Test-Path -LiteralPath $EventPath -PathType Leaf)) {
    Write-Decision -RunMac $true -Decision "run" -Reason "missing_event_fail_full"
    return
}

try {
    $event = Get-Content -LiteralPath $EventPath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
}
catch {
    Write-Decision -RunMac $true -Decision "run" -Reason "malformed_event_fail_full"
    return
}

if ($EventName -eq "push") {
    if ($Ref -ne "refs/heads/main" -or [string]$event.ref -ne "refs/heads/main") {
        Write-Decision -RunMac $true -Decision "run" -Reason "non_main_push_fail_full"
        return
    }

    $message = [string]$event.head_commit.message
    if ($message.IndexOf("[windows-only]", [System.StringComparison]::Ordinal) -ge 0) {
        Write-Decision -RunMac $false -Decision "skip" -Reason "main_commit_windows_only"
        return
    }

    Write-Decision -RunMac $true -Decision "run" -Reason "main_push_full"
    return
}

if ($EventName -eq "pull_request") {
    $action = [string]$event.action
    $labelName = [string]$event.label.name

    if ($action -eq "labeled" -or $action -eq "unlabeled") {
        if ([string]::IsNullOrWhiteSpace($labelName)) {
            Write-Decision -RunMac $true -Decision "run" -Reason "missing_label_fail_full"
            return
        }

        if (-not $labelName.Equals("windows-only", [System.StringComparison]::OrdinalIgnoreCase)) {
            Write-Decision -RunMac $false -Decision "ignore" -Reason "unrelated_label_event"
            return
        }

        if ($action -eq "labeled") {
            Write-Decision -RunMac $false -Decision "skip" -Reason "pull_request_windows_only"
            return
        }

        Write-Decision -RunMac $true -Decision "run" -Reason "pull_request_windows_only_removed"
        return
    }

    if ($action -ne "opened" -and $action -ne "reopened" -and $action -ne "synchronize") {
        Write-Decision -RunMac $true -Decision "run" -Reason "unknown_pull_request_action_fail_full"
        return
    }

    $hasWindowsOnlyLabel = $false
    foreach ($label in @($event.pull_request.labels)) {
        if ([string]$label.name -and ([string]$label.name).Equals("windows-only", [System.StringComparison]::OrdinalIgnoreCase)) {
            $hasWindowsOnlyLabel = $true
            break
        }
    }

    if ($hasWindowsOnlyLabel) {
        Write-Decision -RunMac $false -Decision "skip" -Reason "pull_request_windows_only"
        return
    }

    Write-Decision -RunMac $true -Decision "run" -Reason "pull_request_full"
    return
}

Write-Decision -RunMac $true -Decision "run" -Reason "unknown_event_fail_full"
