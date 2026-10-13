[CmdletBinding()]
param([switch]$NoBuild, [switch]$LiveDemo)

$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path -Parent $PSScriptRoot
if (-not $NoBuild) {
    & (Join-Path $PSScriptRoot 'build-windows-gui.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'The GUI build failed.' }
}
$guiExe = Join-Path $repositoryRoot 'target/windows-gui/Assemblywright.exe'
if (-not (Test-Path -LiteralPath $guiExe)) { throw 'Build the Windows GUI first.' }
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AssemblywrightGuiNativeTest {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr parameter, string text);
}
'@
$utf8 = New-Object System.Text.UTF8Encoding($false)
$testId = [guid]::NewGuid().ToString('N')
$evidenceRoot = Join-Path $repositoryRoot ('target/windows-gui-tests/' + $testId)
$temporaryBase = [System.IO.Path]::GetFullPath((Join-Path $env:TEMP 'assemblywright-gui-tests'))
$testRoot = Join-Path $temporaryBase $testId
New-Item -ItemType Directory -Path $testRoot, $evidenceRoot -Force | Out-Null
$fixtureTarget = Join-Path $repositoryRoot 'target/windows-gui-chatgpt-fixture'
$priorCargoTarget = $env:CARGO_TARGET_DIR
try {
    $env:CARGO_TARGET_DIR = $fixtureTarget
    & cargo build --locked -p assemblywright-master --bin assemblywright-local --features local-chatgpt-test-fixture
    if ($LASTEXITCODE -ne 0) { throw 'The isolated ChatGPT fixture runner build failed.' }
} finally {
    $env:CARGO_TARGET_DIR = $priorCargoTarget
}
$productionBundle = Split-Path -Parent $guiExe
$testBundle = Join-Path $testRoot 'fixture bundle'
New-Item -ItemType Directory -Path $testBundle -Force | Out-Null
foreach ($runtimeFile in @('Assemblywright.exe', 'windows-local-pipeline.ps1')) {
    Copy-Item -LiteralPath (Join-Path $productionBundle $runtimeFile) -Destination (Join-Path $testBundle $runtimeFile)
}
Copy-Item -LiteralPath (Join-Path $fixtureTarget 'debug/assemblywright-local.exe') -Destination (Join-Path $testBundle 'assemblywright-local-real.exe')
$fixtureCompiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
if (-not (Test-Path -LiteralPath $fixtureCompiler)) { $fixtureCompiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework/v4.0.30319/csc.exe' }
& $fixtureCompiler /nologo /target:exe /warnaserror+ /reference:System.Web.Extensions.dll "/out:$(Join-Path $testBundle 'assemblywright-github.exe')" (Join-Path $PSScriptRoot 'fixtures/windows_gui_github_fixture.cs')
if ($LASTEXITCODE -ne 0) { throw 'The isolated GUI GitHub fixture did not compile.' }
& $fixtureCompiler /nologo /target:exe /warnaserror+ /reference:System.Core.dll /reference:System.Web.Extensions.dll "/out:$(Join-Path $testBundle 'assemblywright-local.exe')" (Join-Path $PSScriptRoot 'fixtures/windows_gui_local_fixture.cs')
if ($LASTEXITCODE -ne 0) { throw 'The isolated GUI local/ChatGPT fixture did not compile.' }
$guiExe = Join-Path $testBundle 'Assemblywright.exe'
$python = (Get-Command python -ErrorAction Stop).Source
$guiInstances = New-Object System.Collections.Generic.List[object]
$modelProcess = $null
$chatGptFixturePath = Join-Path $testRoot 'native-chatgpt-fixture.json'
$chatGptFixture = [ordered]@{
    catalog = [ordered]@{ status = 'ok'; account_type = 'chatgpt'; models = @(
        [ordered]@{ id = 'gpt-plan'; name = 'GPT Plan'; reasoning_efforts = @('high'); default_reasoning_effort = 'high' },
        [ordered]@{ id = 'gpt-review'; name = 'GPT Review'; reasoning_efforts = @('xhigh'); default_reasoning_effort = 'xhigh' }
    ) }
    plan_text = 'Implement only the approved feature and preserve the existing acceptance test.'
    review_text = '{"approve":true,"findings":[]}'
    chat_text = 'Implement answer() to return the integer 42 and keep the existing acceptance test green.'
    malformed_chat = $false
    delay_stage = $null
    delay_ms = 0
}
[System.IO.File]::WriteAllText($chatGptFixturePath, ($chatGptFixture | ConvertTo-Json -Depth 6 -Compress), $utf8)
$priorChatGptFixture = $env:ASSEMBLYWRIGHT_CHATGPT_TEST_FIXTURE
$env:ASSEMBLYWRIGHT_CHATGPT_TEST_FIXTURE = $chatGptFixturePath

function Write-Utf8File([string]$Path, [string]$Text) {
    [System.IO.File]::WriteAllText($Path, $Text, $utf8)
}

function Read-TestJson([string]$Path) {
    $stream = $null
    $reader = $null
    try {
        $share = [System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete
        $stream = New-Object System.IO.FileStream($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, $share)
        $reader = New-Object System.IO.StreamReader($stream, $utf8)
        return $reader.ReadToEnd() | ConvertFrom-Json
    } catch { return $null }
    finally { if ($reader) { $reader.Dispose() } elseif ($stream) { $stream.Dispose() } }
}

function Wait-Until([scriptblock]$Check, [string]$Failure, [int]$Seconds = 30) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $value = & $Check
        if ($value) { return $value }
        Start-Sleep -Milliseconds 200
    }
    throw $Failure
}

function Find-Control($Window, [string]$Name) {
    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::AutomationIdProperty, $Name
    )
    $control = $Window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if (-not $control) {
        $condition = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::NameProperty, $Name
        )
        $control = $Window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    }
    if (-not $control) { throw "GUI control is missing: $Name" }
    return $control
}

function Click-Control($Window, [string]$Name) {
    $control = Wait-Until {
        try {
            $candidate = Find-Control $Window $Name
            if ($candidate.Current.IsEnabled) { return $candidate }
        } catch { }
        return $null
    } "GUI control did not become enabled: $Name" 5
    $pattern = $control.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    $pattern.Invoke()
}

function Set-ControlText($Window, [string]$Name, [string]$Text) {
    $control = Find-Control $Window $Name
    $pattern = $null
    if ($control.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
        $pattern.SetValue($Text)
        return
    }
    # Framework multiline edit controls expose TextPattern only. Set their
    # native text through the identified test window, preserving change events.
    if ($control.Current.ControlType -eq [System.Windows.Automation.ControlType]::Document -and
        $control.Current.ProcessId -eq $Window.Current.ProcessId -and
        $control.Current.NativeWindowHandle -ne 0 -and $control.Current.IsEnabled) {
        $result = [AssemblywrightGuiNativeTest]::SendMessage(
            [IntPtr]$control.Current.NativeWindowHandle, 0x000C, [IntPtr]::Zero, $Text
        )
        if ($result -eq [IntPtr]::Zero) { throw "The test edit control rejected its text: $Name" }
        return
    }
    throw "GUI edit control cannot accept text: $Name"
}

function Select-ControlOption($Window, [string]$Name, [string]$Option) {
    $control = Find-Control $Window $Name
    $expand = $null
    if ($control.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$expand)) {
        $expand.Expand()
        try {
            $item = Wait-Until {
                $condition = New-Object System.Windows.Automation.PropertyCondition(
                    [System.Windows.Automation.AutomationElement]::NameProperty, $Option
                )
                $items = $control.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
                foreach ($candidate in $items) {
                    $selection = $null
                    if ($candidate.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection)) { return $candidate }
                }
                return $null
            } "The $Name selector did not expose $Option."
            $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
        } finally { $expand.Collapse() }
        return
    }
    $selection = $null
    if ($control.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection)) { $selection.Select(); return }
    $toggle = $null
    if ($control.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$toggle)) { $toggle.Toggle(); return }
    throw "GUI selector cannot accept a choice: $Name"
}

function Select-Tab($Window, [string]$Name, [string]$Label) {
    $tab = Wait-Until {
        $condition = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::NameProperty, $Label
        )
        $items = $Window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
        foreach ($item in $items) {
            $pattern = $null
            if ($item.Current.IsEnabled -and $item.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) { return $item }
        }
        try {
            $item = Find-Control $Window $Name
            if ($item.Current.IsEnabled) { return $item }
        } catch { }
        return $null
    } "The $Label tab did not become available." 5
    $pattern = $tab.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
    $pattern.Select()
}

function Select-FirstListItem($Window, [string]$Name) {
    $list = Find-Control $Window $Name
    $item = Wait-Until {
        $items = $list.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        foreach ($candidate in $items) {
            $selection = $null
            if ($candidate.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection)) { return $candidate }
        }
        return $null
    } "The $Name list did not expose a selectable entry."
    $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
}

function Read-ControlText($Window, [string]$Name) {
    $control = Find-Control $Window $Name
    $pattern = $null
    if ($control.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern, [ref]$pattern)) {
        return $pattern.DocumentRange.GetText(-1)
    }
    if ($control.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
        return $pattern.Current.Value
    }
    return $control.Current.Name
}

function Find-Dialog($Gui, [string]$Id) {
    return Wait-Until {
        $dialogCondition = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::AutomationIdProperty, $Id
        )
        $ownedDialog = $Gui.Window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $dialogCondition)
        if ($ownedDialog) { return $ownedDialog }
        $dialogNameCondition = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::NameProperty, $Id
        )
        $ownedDialog = $Gui.Window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $dialogNameCondition)
        if ($ownedDialog) { return $ownedDialog }
        $condition = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $Gui.Process.Id
        )
        $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
            [System.Windows.Automation.TreeScope]::Children, $condition
        )
        foreach ($window in $windows) {
            if ($window.Current.AutomationId -eq $Id -or $window.Current.Name -eq $Id) { return $window }
        }
        return $null
    } "The $Id dialog did not open."
}

function Read-Report([string]$State) {
    $path = Join-Path $State 'latest.json'
    if (Test-Path -LiteralPath $path) {
        try { return Read-TestJson $path } catch { return $null }
    }
    return $null
}

function Start-Gui([string]$State, [string]$JobPath) {
    $arguments = '--test-mode --state-dir "' + $State + '" --screenshot "' + (Join-Path $State 'window.png') + '"'
    if ($JobPath) { $arguments += ' --job "' + $JobPath + '"' }
    $process = Start-Process -FilePath $guiExe -ArgumentList $arguments -WindowStyle Hidden -PassThru
    $window = Wait-Until {
        if ($process.HasExited) { throw 'The GUI exited before its window was ready.' }
        $condition = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $process.Id
        )
        [System.Windows.Automation.AutomationElement]::RootElement.FindFirst(
            [System.Windows.Automation.TreeScope]::Children, $condition
        )
    } 'The GUI window was not exposed through Windows UI Automation.'
    $instance = [pscustomobject]@{ Process = $process; Window = $window; State = $State }
    $guiInstances.Add($instance)
    return $instance
}

function Close-Gui($Gui) {
    if ($Gui.Process.HasExited) { return }
    $pattern = $Gui.Window.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern)
    $pattern.Close()
    if (-not $Gui.Process.WaitForExit(10000)) { throw 'The GUI did not finish its normal close handler.' }
}

function Wait-Run([string]$State, [string]$Expected, [int]$Seconds = 30) {
    return Wait-Until {
        $report = Read-Report $State
        if ($report -and $report.status -eq $Expected) { return $report }
        if ($report -and $report.status -in @('failed', 'interrupted') -and $Expected -ne $report.status) {
            throw ("GUI run failed: " + $report.message)
        }
        return $null
    } "The GUI did not produce a $Expected run." $Seconds
}

function Assert-GuiCompleted($Gui) {
    Wait-Until {
        (Find-Control $Gui.Window 'CandidateButton').Current.IsEnabled -and
        (Find-Control $Gui.Window 'ReportButton').Current.IsEnabled
    } 'The GUI did not enable the completed candidate/report controls.' | Out-Null
    Select-Tab $Gui.Window 'ActivityTab' 'Activity'
    $log = Wait-Until {
        try { return Find-Control $Gui.Window 'LogBox' } catch { return $null }
    } 'The Activity tab did not expose the progress log.'
    $pattern = $log.GetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern)
    if ($pattern.DocumentRange.GetText(-1) -notmatch '\[implementation\]') {
        throw 'The GUI did not display the actual runner progress log.'
    }
}

try {
    $portFile = Join-Path $testRoot 'port.txt'
    $requestsFile = Join-Path $testRoot 'requests.jsonl'
    $fakeModel = Join-Path $PSScriptRoot 'fixtures/windows_gui_fake_model.py'
    $modelProcess = Start-Process -FilePath $python -ArgumentList @(
        ('"' + $fakeModel + '"'), '--port-file', ('"' + $portFile + '"'),
        '--requests-file', ('"' + $requestsFile + '"')
    ) -WindowStyle Hidden -PassThru
    Wait-Until { Test-Path -LiteralPath $portFile } 'The loopback inference fixture did not start.' | Out-Null
    $port = (Get-Content -LiteralPath $portFile -Raw).Trim()
    $source = Join-Path $testRoot 'fixture with spaces'
    New-Item -ItemType Directory -Path $source -Force | Out-Null
    Write-Utf8File (Join-Path $source '.gitignore') "__pycache__/`n"
    Write-Utf8File (Join-Path $source 'feature.py') "def answer():`n    raise NotImplementedError()`n"
    & git -C $source init --quiet --initial-branch=main
    if ($LASTEXITCODE -ne 0) { throw 'Fixture Git initialization failed.' }
    & git -C $source add -- .
    if ($LASTEXITCODE -ne 0) { throw 'Fixture Git index creation failed.' }
    & git -C $source -c user.name=Assemblywright -c user.email=test@localhost -c core.hooksPath=NUL commit --quiet -m fixture
    if ($LASTEXITCODE -ne 0) { throw 'Fixture commit failed.' }
    $job = [ordered]@{
        repository = $source
        specification = 'Implement answer() in feature.py to return the integer 42.'
        allowed_files = @('feature.py')
        validation = @{ program = $python; args = @('-c', 'from feature import answer; assert answer() == 42'); timeout_seconds = 30 }
        endpoint = "http://127.0.0.1:$port/v1"
        model = 'gui-fixture'
        max_attempts = 1
    }
    $jobPath = Join-Path $testRoot 'success job.json'
    Write-Utf8File $jobPath ($job | ConvertTo-Json -Depth 6)
    $successState = Join-Path $testRoot 'success state'
    $successGui = Start-Gui $successState $jobPath
    Click-Control $successGui.Window 'RunFeatureButton'
    $successReport = Wait-Run $successState 'completed'
    Assert-GuiCompleted $successGui
    if ((Get-Content -LiteralPath (Join-Path $source 'feature.py') -Raw) -notmatch 'NotImplementedError') {
        throw 'The GUI run modified the original source.'
    }
    Close-Gui $successGui

    $job.validation.args = @('-c', 'import time; time.sleep(30)')
    $cancelJob = Join-Path $testRoot 'cancel job.json'
    Write-Utf8File $cancelJob ($job | ConvertTo-Json -Depth 6)
    $cancelState = Join-Path $testRoot 'cancel state'
    $cancelGui = Start-Gui $cancelState $cancelJob
    Click-Control $cancelGui.Window 'RunFeatureButton'
    Wait-Until {
        $report = Read-Report $cancelState
        $report -and $report.stage -eq 'validation'
    } 'The cancellation fixture did not reach validation.' | Out-Null
    Click-Control $cancelGui.Window 'CancelButton'
    $cancelReport = Wait-Run $cancelState 'cancelled'
    Wait-Until { (Find-Control $cancelGui.Window 'RunFeatureButton').Current.IsEnabled } 'The cancelled GUI did not become usable again.' | Out-Null
    Click-Control $cancelGui.Window 'PauseButton'
    Wait-Until { Test-Path -LiteralPath (Join-Path $cancelState 'pause.flag') } 'GUI pause did not reach the native runner.' | Out-Null
    Click-Control $cancelGui.Window 'ResumeButton'
    Wait-Until { -not (Test-Path -LiteralPath (Join-Path $cancelState 'pause.flag')) } 'GUI resume did not reach the native runner.' | Out-Null
    Close-Gui $cancelGui

    $closeState = Join-Path $testRoot 'active close state'
    $closeGui = Start-Gui $closeState $cancelJob
    Click-Control $closeGui.Window 'RunFeatureButton'
    Wait-Until {
        $report = Read-Report $closeState
        $report -and $report.stage -eq 'validation'
    } 'The active-close fixture did not reach validation.' | Out-Null
    Close-Gui $closeGui
    $closedReport = Wait-Run $closeState 'cancelled'

    $failureState = Join-Path $testRoot 'cancel failure state'
    $failureGui = Start-Gui $failureState $cancelJob
    Click-Control $failureGui.Window 'RunFeatureButton'
    Wait-Until {
        $report = Read-Report $failureState
        $report -and $report.stage -eq 'validation'
    } 'The cancellation failure fixture did not reach validation.' | Out-Null
    Select-Tab $failureGui.Window 'ActivityTab' 'Activity'
    Wait-Until {
        try { return Find-Control $failureGui.Window 'LogBox' } catch { return $null }
    } 'The failure Activity pane did not expose its log.' | Out-Null
    $controlLock = Join-Path $failureState 'control.lock'
    $originalAttributes = [System.IO.File]::GetAttributes($controlLock)
    try {
        [System.IO.File]::SetAttributes($controlLock, ($originalAttributes -bor [System.IO.FileAttributes]::ReadOnly))
        $closePattern = $failureGui.Window.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern)
        $closePattern.Close()
        Wait-Until {
            if ($failureGui.Process.HasExited) { throw 'The GUI closed while cancellation failed.' }
            $log = Find-Control $failureGui.Window 'LogBox'
            $pattern = $log.GetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern)
            $pattern.DocumentRange.GetText(-1) -match 'remains open'
        } 'The GUI did not surface cancellation failure while remaining open.' | Out-Null
        if ((Read-Report $failureState).status -ne 'running') { throw 'The failed close incorrectly changed the active run outcome.' }
    } finally {
        [System.IO.File]::SetAttributes($controlLock, $originalAttributes)
    }
    Close-Gui $failureGui
    $failureRecoveredReport = Wait-Run $failureState 'cancelled'

    $parityState = Join-Path $testRoot 'mac parity state'
    New-Item -ItemType Directory -Path $parityState -Force | Out-Null
    $legacySettings = [ordered]@{ schema_version = 1; endpoint = "http://127.0.0.1:$port/v1"; model = 'gui-fixture'; reviewer_model = 'gui-review-fixture'; reasoning_effort = 'none'; reviewer_reasoning_effort = 'none'; revision = 4; default_attempts = 3; timeout_seconds = 300 }
    Write-Utf8File (Join-Path $parityState 'gui-settings.json') ($legacySettings | ConvertTo-Json -Compress)
    $parityGui = Start-Gui $parityState $jobPath
    $githubHeader = Find-Control $parityGui.Window 'GitHubButton'
    $permissionsHeader = Find-Control $parityGui.Window 'SafetyButton'
    $settingsHeader = Find-Control $parityGui.Window 'SettingsButton'
    if ($githubHeader.Current.Name -ne 'GitHub publication' -or $permissionsHeader.Current.Name -ne 'Permissions' -or $settingsHeader.Current.Name -ne 'Settings' -or
        $githubHeader.Current.BoundingRectangle.X -ge $permissionsHeader.Current.BoundingRectangle.X -or
        $permissionsHeader.Current.BoundingRectangle.X -ge $settingsHeader.Current.BoundingRectangle.X) { throw 'The header does not match the Mac icon order and actions.' }
    Click-Control $parityGui.Window 'SettingsButton'
    $settingsDialog = Find-Dialog $parityGui 'SettingsDialog'
    foreach ($providerControl in @('SettingsOrchestratorProviderBox', 'SettingsReviewerProviderBox')) {
        try { Find-Control $settingsDialog $providerControl | Out-Null; throw 'Mac role settings exposed an unsupported provider selector.' } catch { if ($_.Exception.Message -eq 'Mac role settings exposed an unsupported provider selector.') { throw } }
    }
    Set-ControlText $settingsDialog 'SettingsEndpointBox' "http://127.0.0.1:$port/v1"
    Click-Control $settingsDialog 'SettingsReloadButton'
    $settingsDialog = Find-Dialog $parityGui 'SettingsDialog'
    Set-ControlText $settingsDialog 'SettingsLocalModelBox' 'gui-fixture'
    Select-ControlOption $settingsDialog 'SettingsLocalReasoningBox' 'low'
    Select-ControlOption $settingsDialog 'SettingsOrchestratorModelBox' 'GPT Plan'
    Select-ControlOption $settingsDialog 'SettingsReviewerModelBox' 'GPT Review'
    Select-ControlOption $settingsDialog 'SettingsOrchestratorReasoningBox' 'High'
    Select-ControlOption $settingsDialog 'SettingsReviewerReasoningBox' 'Extra high'
    Click-Control $settingsDialog 'SettingsSaveButton'
    $roleSettings = Wait-Until {
        $settingsPath = Join-Path $parityState 'gui-settings.json'
        if (Test-Path -LiteralPath $settingsPath) {
            $saved = Read-TestJson $settingsPath
            if ($saved.model -eq 'gui-fixture' -and $saved.reasoning_effort -eq 'low' -and $saved.orchestrator_provider -eq 'chatgpt' -and $saved.orchestrator_model -eq 'gpt-plan' -and $saved.orchestrator_reasoning_effort -eq 'high' -and $saved.reviewer_provider -eq 'chatgpt' -and $saved.reviewer_model -eq 'gpt-review' -and $saved.reviewer_reasoning_effort -eq 'xhigh') { return $saved }
        }
        return $null
    } 'The gear did not save separate model roles and reasoning selections.'
    $savedSettingsText = [System.IO.File]::ReadAllText((Join-Path $parityState 'gui-settings.json'), $utf8)
    Write-Utf8File (Join-Path $parityState 'chatgpt-fixture-models-error.txt') 'unavailable'
    Click-Control $parityGui.Window 'SettingsButton'
    $settingsDialog = Find-Dialog $parityGui 'SettingsDialog'
    Click-Control $settingsDialog 'SettingsReloadButton'
    $settingsDialog = Find-Dialog $parityGui 'SettingsDialog'
    Click-Control $settingsDialog 'SettingsCancelButton'
    if ([System.IO.File]::ReadAllText((Join-Path $parityState 'gui-settings.json'), $utf8) -ne $savedSettingsText) { throw 'Failed ChatGPT catalog reload changed saved role settings.' }
    Remove-Item -LiteralPath (Join-Path $parityState 'chatgpt-fixture-models-error.txt')
    Click-Control $parityGui.Window 'SafetyButton'
    $permissionsDialog = Find-Dialog $parityGui 'PermissionsDialog'
    Select-ControlOption $permissionsDialog 'PermissionsAutoRadio' ''
    Click-Control $permissionsDialog 'PermissionsSaveButton'
    Wait-Until { (Read-TestJson (Join-Path $parityState 'permissions.json')).mode -eq 'auto' } 'The shield did not save global Approve for me permissions.' | Out-Null
    Click-Control $parityGui.Window 'SafetyButton'
    $permissionsDialog = Find-Dialog $parityGui 'PermissionsDialog'
    Select-ControlOption $permissionsDialog 'PermissionsFullRadio' ''
    Click-Control $permissionsDialog 'PermissionsSaveButton'
    Wait-Until { (Read-TestJson (Join-Path $parityState 'permissions.json')).mode -eq 'full' } 'The shield did not restore Full access.' | Out-Null
    Click-Control $parityGui.Window 'GitHubButton'
    $githubDialog = Find-Dialog $parityGui 'GitHubDialog'
    Wait-Until {
        (Read-ControlText $githubDialog 'GitHubAccountStatusLabel') -match 'gui-owner'
    } 'The GitHub icon did not open account and publication setup.' | Out-Null
    Click-Control $githubDialog 'GitHubLoadRepositoriesButton'
    Wait-Until {
        (Read-ControlText $githubDialog 'GitHubStatusLabel') -match 'Repositories loaded'
    } 'The GitHub panel did not discover repositories.' | Out-Null
    Click-Control $githubDialog 'GitHubUseRepositoryButton'
    if ((Read-ControlText $githubDialog 'GitHubRepositoryBox') -ne 'gui-owner/feature-fixture') { throw 'Use project did not select the repository.' }
    Click-Control $githubDialog 'GitHubDoneButton'

    Select-Tab $parityGui.Window 'ChatTab' 'Chat'
    Set-ControlText $parityGui.Window 'ChatInputBox' 'Draft a feature for answer() returning 42.'
    Click-Control $parityGui.Window 'ChatSendButton'
    Wait-Until {
        (Read-ControlText $parityGui.Window 'ChatTranscriptBox') -match 'existing acceptance test'
    } 'Right-pane chat did not display the ChatGPT reply.' | Out-Null
    $chatCall = (Get-Content -LiteralPath (Join-Path $parityState 'chatgpt-fixture-calls.jsonl') | Select-Object -Last 1) | ConvertFrom-Json
    if ($chatCall.model -ne 'gpt-plan' -or $chatCall.reasoning_effort -ne 'high') { throw 'Right-pane chat did not use the saved ChatGPT orchestrator pair.' }
    Click-Control $parityGui.Window 'ChatUseFeatureButton'
    $draft = Read-ControlText $parityGui.Window 'SpecificationBox'
    if ($draft -notmatch 'return the integer 42') { throw 'Use as feature did not copy the chat draft into the editor.' }
    if (Test-Path -LiteralPath (Join-Path $parityState 'latest.json')) {
        throw 'Chat drafting created a native execution report without an owner Run action.'
    }
    $linePath = Join-Path $parityState 'assembly-line.json'
    if (Test-Path -LiteralPath $linePath) {
        $lineBefore = Read-TestJson $linePath
        if ($lineBefore.entries.Count -ne 0) { throw 'Chat drafting enqueued a feature without approval.' }
    }

    Click-Control $parityGui.Window 'AddToLineButton'
    Wait-Until {
        if (Test-Path -LiteralPath $linePath) {
            $line = Read-TestJson $linePath
            $line.entries.Count -eq 1
        }
    } 'The first approved feature was not added to the Assembly Line.' | Out-Null
    $firstFrozen = ((Read-TestJson $linePath).entries[0].frozen_job_json | ConvertFrom-Json)
    if ($firstFrozen.model -ne 'gui-fixture' -or $firstFrozen.reasoning_effort -ne 'low' -or $firstFrozen.orchestrator_provider -ne 'chatgpt' -or $firstFrozen.orchestrator_model -ne 'gpt-plan' -or $firstFrozen.orchestrator_reasoning_effort -ne 'high' -or $firstFrozen.reviewer_provider -ne 'chatgpt' -or $firstFrozen.reviewer_model -ne 'gpt-review' -or $firstFrozen.reviewer_reasoning_effort -ne 'xhigh') {
        throw 'The queued feature did not freeze local implementation and independent ChatGPT role pairs.'
    }
    Select-Tab $parityGui.Window 'FeatureEditorTab' 'Feature'
    Set-ControlText $parityGui.Window 'SpecificationBox' 'Implement answer() returning 42 as a second independent feature.'
    Click-Control $parityGui.Window 'AddToLineButton'
    Wait-Until {
        $line = Read-TestJson $linePath
        $line.entries.Count -eq 2
    } 'The second approved feature was not queued.' | Out-Null
    Select-Tab $parityGui.Window 'AssemblyLineTab' 'Assembly Line'
    Click-Control $parityGui.Window 'StartLineButton'
    $completedLine = Wait-Until {
        $line = Read-TestJson $linePath
        if (@($line.entries | Where-Object { $_.status -in @('Failed', 'Cancelled', 'Interrupted') }).Count -gt 0) {
            throw 'The Assembly Line failed to complete its approved features.'
        }
        if (@($line.entries | Where-Object { $_.status -eq 'Completed' }).Count -eq 2) { return $line }
        return $null
    } 'The Assembly Line did not complete both features in order.' 45
    if (-not $completedLine.entries[0].run_id -or -not $completedLine.entries[1].run_id -or
        $completedLine.entries[0].run_id -eq $completedLine.entries[1].run_id) {
        throw 'The Assembly Line did not retain separate native run bindings.'
    }

    Select-Tab $parityGui.Window 'ChatTab' 'Chat'
    $chatWaitingPath = Join-Path $parityState 'chatgpt-fixture-waiting.txt'
    if (Test-Path -LiteralPath $chatWaitingPath) { Remove-Item -LiteralPath $chatWaitingPath }
    Set-ControlText $parityGui.Window 'ChatInputBox' '__WAIT_FOR_CANCEL__'
    Click-Control $parityGui.Window 'ChatSendButton'
    Wait-Until { (Find-Control $parityGui.Window 'ChatStopButton').Current.IsEnabled } 'Chat did not enter its cancellable request state.' | Out-Null
    Wait-Until { Test-Path -LiteralPath $chatWaitingPath } 'The ChatGPT fixture child did not start.' | Out-Null
    $chatChild = Get-Process -Name 'assemblywright-local' -ErrorAction SilentlyContinue | Where-Object { try { $_.Path -eq (Join-Path $testBundle 'assemblywright-local.exe') } catch { $false } } | Select-Object -First 1
    if (-not $chatChild) { throw 'The cancellable ChatGPT adapter process was not observed.' }
    Click-Control $parityGui.Window 'ChatStopButton'
    Wait-Until { (Find-Control $parityGui.Window 'ChatSendButton').Current.IsEnabled } 'Stopping chat did not restore the composer.' | Out-Null
    Wait-Until { -not (Get-Process -Id $chatChild.Id -ErrorAction SilentlyContinue) } 'Stopping chat left the ChatGPT adapter process alive.' | Out-Null
    Remove-Item -LiteralPath $chatWaitingPath
    Set-ControlText $parityGui.Window 'ChatInputBox' '__WAIT_FOR_CANCEL__'
    Click-Control $parityGui.Window 'ChatSendButton'
    Click-Control $parityGui.Window 'PauseButton'
    Wait-Until {
        (Test-Path -LiteralPath (Join-Path $parityState 'pause.flag')) -and
        -not (Find-Control $parityGui.Window 'ChatSendButton').Current.IsEnabled -and
        -not (Find-Control $parityGui.Window 'ChatStopButton').Current.IsEnabled
    } 'Emergency Pause did not cancel and block chat generation.' | Out-Null
    Click-Control $parityGui.Window 'ResumeButton'
    Wait-Until {
        -not (Test-Path -LiteralPath (Join-Path $parityState 'pause.flag')) -and
        (Find-Control $parityGui.Window 'ChatSendButton').Current.IsEnabled
    } 'Resume did not restore the chat composer.' | Out-Null
    if (Test-Path -LiteralPath $chatWaitingPath) { Remove-Item -LiteralPath $chatWaitingPath }
    Set-ControlText $parityGui.Window 'ChatInputBox' '__WAIT_FOR_CANCEL__'
    Click-Control $parityGui.Window 'ChatSendButton'
    Wait-Until { Test-Path -LiteralPath $chatWaitingPath } 'The ChatGPT fixture child did not start before close.' | Out-Null
    $closeChatChild = Get-Process -Name 'assemblywright-local' -ErrorAction SilentlyContinue | Where-Object { try { $_.Path -eq (Join-Path $testBundle 'assemblywright-local.exe') } catch { $false } } | Select-Object -First 1
    if (-not $closeChatChild) { throw 'The close-bound ChatGPT adapter process was not observed.' }
    Close-Gui $parityGui
    Wait-Until { -not (Get-Process -Id $closeChatChild.Id -ErrorAction SilentlyContinue) } 'Closing the GUI left the ChatGPT adapter process alive.' | Out-Null
    $reopenedGui = Start-Gui $parityState ''
    Select-Tab $reopenedGui.Window 'ChatTab' 'Chat'
    if ((Read-ControlText $reopenedGui.Window 'ChatTranscriptBox') -notmatch 'existing acceptance test') {
        throw 'The chat history was not retained on normal restart.'
    }
    Close-Gui $reopenedGui

    $approvalCases = @()
    foreach ($choice in @('approve', 'reject')) {
        $approvalState = Join-Path $testRoot ("owner $choice state")
        $approvalGui = Start-Gui $approvalState $jobPath
        Click-Control $approvalGui.Window 'SafetyButton'
        $approvalPermissions = Find-Dialog $approvalGui 'PermissionsDialog'
        Select-ControlOption $approvalPermissions 'PermissionsAskRadio' ''
        Click-Control $approvalPermissions 'PermissionsSaveButton'
        Wait-Until {
            $permissionPath = Join-Path $approvalState 'permissions.json'
            (Test-Path -LiteralPath $permissionPath) -and (Read-TestJson $permissionPath).mode -eq 'ask'
        } 'Ask permissions did not reach native authority.' | Out-Null
        Click-Control $approvalGui.Window 'RunFeatureButton'
        $previousRequest = ''
        $actions = @()
        $requestCount = $(if ($choice -eq 'approve') { 2 } else { 1 })
        for ($requestIndex = 0; $requestIndex -lt $requestCount; $requestIndex++) {
            $pending = Wait-Until {
                $pendingPath = Join-Path $approvalState 'pending-approval.json'
                if (Test-Path -LiteralPath $pendingPath) {
                    $request = Read-TestJson $pendingPath
                    if ($request.id -ne $previousRequest -and $request.approved -eq $null) { return $request }
                }
                return $null
            } 'The native runner did not request the exact owner approval.'
            $approvalDialog = Find-Dialog $approvalGui 'ApprovalDialog'
            $actions += $pending.action
            Click-Control $approvalDialog $(if ($choice -eq 'approve') { 'ApprovalApproveButton' } else { 'ApprovalRejectButton' })
            $previousRequest = $pending.id
        }
        $approvalOutcome = Wait-Run $approvalState $(if ($choice -eq 'approve') { 'completed' } else { 'failed' })
        if ($choice -eq 'approve' -and ($actions -join ',') -ne 'apply_model_edits,run_validation') { throw 'Ask mode skipped its edit or validation approval.' }
        if ($choice -eq 'reject' -and $approvalOutcome.stage -ne 'approval_rejected') { throw 'Reject did not stop the exact pending action.' }
        $approvalCases += [ordered]@{ choice = $choice; actions = $actions; run_id = $approvalOutcome.run_id; status = $approvalOutcome.status }
        Close-Gui $approvalGui
    }

    $publicationCases = @()
    foreach ($publicationMode in @('success', 'verbatim-success', 'verbatim-reconcile', 'attention', 'wrong-run', 'wait')) {
        $publicationState = Join-Path $testRoot ("publication $publicationMode state")
        New-Item -ItemType Directory -Path $publicationState -Force | Out-Null
        Write-Utf8File (Join-Path $publicationState 'github-fixture-mode.txt') $publicationMode
        $publicationGui = Start-Gui $publicationState $jobPath
        Click-Control $publicationGui.Window 'GitHubButton'
        $publicationDialog = Find-Dialog $publicationGui 'GitHubDialog'
        Set-ControlText $publicationDialog 'GitHubRepositoryBox' 'gui-owner/feature-fixture'
        Set-ControlText $publicationDialog 'GitHubBaseBranchBox' 'main'
        Set-ControlText $publicationDialog 'GitHubProjectPathBox' $source
        Click-Control $publicationDialog 'GitHubSaveButton'
        Wait-Until {
            $connectionFile = Join-Path $publicationState 'github-fixture.json'
            (Test-Path -LiteralPath $connectionFile) -and (Read-TestJson $connectionFile).connection -ne $null
        } 'The GitHub panel did not save the explicit project connection.' | Out-Null
        Click-Control $publicationDialog 'GitHubDoneButton'
        Click-Control $publicationGui.Window 'AddToLineButton'
        $publicationLinePath = Join-Path $publicationState 'assembly-line.json'
        Wait-Until { (Test-Path -LiteralPath $publicationLinePath) -and (Read-TestJson $publicationLinePath).entries.Count -eq 1 } 'The connected feature was not queued.' | Out-Null
        $directPublicationSuccess = $publicationMode -in @('success', 'verbatim-success')
        if (-not $directPublicationSuccess) {
            Select-Tab $publicationGui.Window 'FeatureEditorTab' 'Feature'
            Click-Control $publicationGui.Window 'AddToLineButton'
            Wait-Until { (Read-TestJson $publicationLinePath).entries.Count -eq 2 } 'The second connected feature was not queued.' | Out-Null
        }
        Select-Tab $publicationGui.Window 'AssemblyLineTab' 'Assembly Line'
        Click-Control $publicationGui.Window 'StartLineButton'
        if ($publicationMode -eq 'wait') {
            Wait-Until { Test-Path -LiteralPath (Join-Path $publicationState 'publication-started.txt') } 'The publication did not reach its cancellable process boundary.' | Out-Null
            Click-Control $publicationGui.Window 'StopLineButton'
        }
        $publicationLine = Wait-Until {
            $line = Read-TestJson $publicationLinePath
            if (-not $line) { return $null }
            if ($line.entries[0].status -in @('Completed', 'Failed', 'Cancelled')) { return $line }
            return $null
        } 'The connected queue did not report a terminal publication outcome.'
        $publishedEntry = $publicationLine.entries[0]
        if ($publicationMode -eq 'verbatim-reconcile') {
            if ($publishedEntry.status -eq 'Completed' -or $publishedEntry.publication_status -ne 'attention' -or $publicationLine.entries[1].status -ne 'Pending') {
                throw 'An uncertain verbatim-path publication did not stop for explicit reconciliation.'
            }
            Select-FirstListItem $publicationGui.Window 'AssemblyLineList'
            Click-Control $publicationGui.Window 'ReconcilePublicationButton'
            $publicationLine = Wait-Until {
                $line = Read-TestJson $publicationLinePath
                if ($line.entries[0].status -eq 'Completed') { return $line }
                return $null
            } 'The durable verbatim-path publication did not reconcile through the native GUI.'
            $publishedEntry = $publicationLine.entries[0]
            if ($publicationLine.entries[1].status -ne 'Pending') { throw 'Publication reconciliation advanced the next frozen feature without Start Line.' }
        }
        if ($directPublicationSuccess -or $publicationMode -eq 'verbatim-reconcile') {
            if ($publishedEntry.status -ne 'Completed' -or $publishedEntry.publication_status -ne 'succeeded' -or
                $publishedEntry.publication_url -ne 'https://github.com/gui-owner/feature-fixture/pull/7' -or
                -not $publishedEntry.publication_merge_sha) { throw 'The exact publication receipt did not authorize completion.' }
            if ($publicationMode -eq 'success') {
                Click-Control $publicationGui.Window 'GitHubButton'
                $publicationDialog = Find-Dialog $publicationGui 'GitHubDialog'
                Click-Control $publicationDialog 'GitHubDisconnectButton'
                Wait-Until { $savedConnection = Read-TestJson (Join-Path $publicationState 'github-fixture.json'); $savedConnection -and $savedConnection.connection -eq $null } 'Disconnect did not remove the project binding.' | Out-Null
                Click-Control $publicationDialog 'GitHubDoneButton'
            }
        } else {
            if ($publishedEntry.status -eq 'Completed' -or $publicationLine.entries[1].status -ne 'Pending') { throw 'A failed, unbound, or cancelled publication advanced the queue.' }
        }
        $publicationCases += [ordered]@{ fixture_outcome = $publicationMode; run_id = $publishedEntry.run_id; status = $publishedEntry.status; publication_status = $publishedEntry.publication_status }
        Close-Gui $publicationGui
    }

    $preflightState = Join-Path $testRoot 'preflight stop state'
    New-Item -ItemType Directory -Path $preflightState -Force | Out-Null
    $shimDirectory = Join-Path $testRoot 'preflight git shim'
    New-Item -ItemType Directory -Path $shimDirectory -Force | Out-Null
    $shimExe = Join-Path $shimDirectory 'git.exe'
    $shimCompiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
    if (-not (Test-Path -LiteralPath $shimCompiler)) { $shimCompiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework/v4.0.30319/csc.exe' }
    & $shimCompiler /nologo /target:exe /warnaserror+ "/out:$shimExe" (Join-Path $PSScriptRoot 'fixtures/windows_gui_git_preflight.cs')
    if ($LASTEXITCODE -ne 0) { throw 'The native Git preflight fixture did not compile.' }
    $preflightMarker = Join-Path $preflightState 'git-waiting.txt'
    $preflightRelease = Join-Path $preflightState 'git-release.txt'
    Write-Utf8File $preflightMarker 'Do not block owner approval preflight.'
    $priorGitEnvironment = @{}
    foreach ($key in @('PATH', 'AW_GUI_TEST_REAL_GIT', 'AW_GUI_TEST_PREFLIGHT_MARKER', 'AW_GUI_TEST_PREFLIGHT_RELEASE')) {
        $priorGitEnvironment[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
    }
    try {
        [Environment]::SetEnvironmentVariable('AW_GUI_TEST_REAL_GIT', (Get-Command git.exe -ErrorAction Stop).Source, 'Process')
        [Environment]::SetEnvironmentVariable('AW_GUI_TEST_PREFLIGHT_MARKER', $preflightMarker, 'Process')
        [Environment]::SetEnvironmentVariable('AW_GUI_TEST_PREFLIGHT_RELEASE', $preflightRelease, 'Process')
        [Environment]::SetEnvironmentVariable('PATH', ($shimDirectory + ';' + $env:PATH), 'Process')
        $preflightGui = Start-Gui $preflightState $jobPath
    } finally {
        foreach ($key in $priorGitEnvironment.Keys) { [Environment]::SetEnvironmentVariable($key, $priorGitEnvironment[$key], 'Process') }
    }
    Click-Control $preflightGui.Window 'AddToLineButton'
    Wait-Until {
        $queuePath = Join-Path $preflightState 'assembly-line.json'
        if (Test-Path -LiteralPath $queuePath) { return (Read-TestJson $queuePath).entries.Count -eq 1 }
        return $null
    } 'The preflight fixture feature was not approved.' | Out-Null
    Remove-Item -LiteralPath $preflightMarker
    Select-Tab $preflightGui.Window 'AssemblyLineTab' 'Assembly Line'
    Click-Control $preflightGui.Window 'StartLineButton'
    Wait-Until { Test-Path -LiteralPath $preflightMarker } 'Git preflight did not reach its controlled process boundary.' | Out-Null
    Click-Control $preflightGui.Window 'StopLineButton'
    Write-Utf8File $preflightRelease 'release preflight after Stop'
    Wait-Until {
        $line = Read-TestJson (Join-Path $preflightState 'assembly-line.json')
        if (-not $line) { return $null }
        $line.entries[0].status -in @('Pending', 'Cancelled') -and
        (Find-Control $preflightGui.Window 'NewFeatureButton').Current.IsEnabled -and
        -not (Find-Control $preflightGui.Window 'StopLineButton').Current.IsEnabled
    } 'Stopping during Git preflight did not release the unlaunched queue entry.' | Out-Null
    if (Test-Path -LiteralPath (Join-Path $preflightState 'latest.json')) { throw 'The stopped preflight still launched a native feature run.' }
    Close-Gui $preflightGui

    $lineStopEvidence = @()
    foreach ($gateCase in @('failed', 'cancelled')) {
        $gateJob = Read-TestJson $jobPath
        $gateJob.validation.args = $(if ($gateCase -eq 'failed') { @('-c', 'raise SystemExit(7)') } else { @('-c', 'import time; time.sleep(30)') })
        $gateJobPath = Join-Path $testRoot ("line $gateCase job.json")
        Write-Utf8File $gateJobPath ($gateJob | ConvertTo-Json -Depth 6)
        $gateState = Join-Path $testRoot ("line $gateCase state")
        $gateGui = Start-Gui $gateState $gateJobPath
        Click-Control $gateGui.Window 'AddToLineButton'
        $gateLinePath = Join-Path $gateState 'assembly-line.json'
        Wait-Until {
            if (Test-Path -LiteralPath $gateLinePath) { return (Read-TestJson $gateLinePath).entries.Count -eq 1 }
            return $null
        } 'The first gate fixture feature was not queued.' | Out-Null
        Select-Tab $gateGui.Window 'FeatureEditorTab' 'Feature'
        Wait-Until {
            try { return Find-Control $gateGui.Window 'ValidationArgumentsBox' } catch { return $null }
        } 'The feature editor did not become available for the next queued job.' | Out-Null
        Set-ControlText $gateGui.Window 'SpecificationBox' 'Implement the second feature after explicit restart.'
        Set-ControlText $gateGui.Window 'ValidationArgumentsBox' "-c`r`nfrom feature import answer; assert answer() == 42"
        Click-Control $gateGui.Window 'AddToLineButton'
        Wait-Until { (Read-TestJson $gateLinePath).entries.Count -eq 2 } 'The second gate fixture feature was not queued.' | Out-Null
        Select-Tab $gateGui.Window 'AssemblyLineTab' 'Assembly Line'
        Click-Control $gateGui.Window 'StartLineButton'
        if ($gateCase -eq 'cancelled') {
            Wait-Until { $report = Read-Report $gateState; $report -and $report.stage -eq 'validation' } 'Queued cancellation did not reach real validation.' | Out-Null
            Click-Control $gateGui.Window 'StopLineButton'
        }
        $stoppedLine = Wait-Until {
            $line = Read-TestJson $gateLinePath
            if (-not $line) { return $null }
            if ($line.entries[1].status -ne 'Pending') { throw 'The queue advanced past a failed or stopped feature.' }
            if ($line.entries[0].status.ToLowerInvariant() -eq $gateCase) { return $line }
            return $null
        } "The first queued feature did not become $gateCase."
        Wait-Until { (Find-Control $gateGui.Window 'StartLineButton').Current.IsEnabled } 'The stopped line did not release its controls.' | Out-Null
        $stoppedRunId = $stoppedLine.entries[0].run_id
        Close-Gui $gateGui
        $gateRestartGui = Start-Gui $gateState ''
        Start-Sleep -Milliseconds 1200
        $restoredLine = Read-TestJson $gateLinePath
        if ($restoredLine.entries[0].run_id -ne $stoppedRunId -or $restoredLine.entries[1].status -ne 'Pending') {
            throw 'Restart silently launched or rewrote a stopped queue.'
        }
        Click-Control $gateRestartGui.Window 'StartLineButton'
        $restartedLine = Wait-Until {
            $line = Read-TestJson $gateLinePath
            if (-not $line) { return $null }
            if ($line.entries[1].status -eq 'Failed') { throw 'The explicitly restarted pending feature failed.' }
            if ($line.entries[1].status -eq 'Completed') { return $line }
            return $null
        } 'Explicit Start line did not complete the remaining approved feature.'
        $lineStopEvidence += [ordered]@{ outcome = $gateCase; stopped_run_id = $stoppedRunId; explicitly_restarted_run_id = $restartedLine.entries[1].run_id }
        Close-Gui $gateRestartGui
    }

    # This optional case remains fixture-backed. It expands the UI journey but is
    # never external proof of the authenticated ChatGPT account or local model.
    $extendedFixtureReport = $null
    if ($LiveDemo) {
        $liveState = Join-Path $testRoot 'live state'
        New-Item -ItemType Directory -Path $liveState -Force | Out-Null
        $liveSeedSettings = [ordered]@{
            schema_version = 1; endpoint = "http://localhost:$port/v1"; model = 'gui-review-fixture'; reasoning_effort = 'low'
            orchestrator_provider = 'chatgpt'; orchestrator_model = 'gpt-plan'; orchestrator_reasoning_effort = 'high'
            reviewer_provider = 'chatgpt'; reviewer_model = 'gpt-review'; reviewer_reasoning_effort = 'xhigh'
            revision = 11; default_attempts = 2; timeout_seconds = 47
        }
        Write-Utf8File (Join-Path $liveState 'gui-settings.json') ($liveSeedSettings | ConvertTo-Json -Compress)
        $liveGui = Start-Gui $liveState ''
        Click-Control $liveGui.Window 'SettingsButton'
        $liveSettingsDialog = Find-Dialog $liveGui 'SettingsDialog'
        Set-ControlText $liveSettingsDialog 'SettingsEndpointBox' "http://127.0.0.1:$port/v1"
        Click-Control $liveSettingsDialog 'SettingsReloadButton'
        $liveSettingsDialog = Find-Dialog $liveGui 'SettingsDialog'
        Set-ControlText $liveSettingsDialog 'SettingsLocalModelBox' 'gui-fixture'
        Select-ControlOption $liveSettingsDialog 'SettingsLocalReasoningBox' 'low'
        Select-ControlOption $liveSettingsDialog 'SettingsOrchestratorModelBox' 'GPT Plan'
        Select-ControlOption $liveSettingsDialog 'SettingsOrchestratorReasoningBox' 'High'
        Select-ControlOption $liveSettingsDialog 'SettingsReviewerModelBox' 'GPT Review'
        Select-ControlOption $liveSettingsDialog 'SettingsReviewerReasoningBox' 'Extra high'
        Click-Control $liveSettingsDialog 'SettingsSaveButton'
        $liveRoleSettings = Wait-Until {
            $settingsPath = Join-Path $liveState 'gui-settings.json'
            if (-not (Test-Path -LiteralPath $settingsPath)) { return $null }
            $saved = Read-TestJson $settingsPath
            if ($saved.endpoint -eq "http://127.0.0.1:$port/v1" -and $saved.model -eq 'gui-fixture' -and $saved.reasoning_effort -eq 'low' -and
                $saved.orchestrator_provider -eq 'chatgpt' -and $saved.orchestrator_model -eq 'gpt-plan' -and $saved.orchestrator_reasoning_effort -eq 'high' -and
                $saved.reviewer_provider -eq 'chatgpt' -and $saved.reviewer_model -eq 'gpt-review' -and $saved.reviewer_reasoning_effort -eq 'xhigh' -and
                $saved.default_attempts -eq 2 -and $saved.timeout_seconds -eq 47) { return $saved }
            return $null
        } 'The live demo fixture settings were not saved exactly.'
        Select-Tab $liveGui.Window 'ChatTab' 'Chat'
        Set-ControlText $liveGui.Window 'ChatInputBox' 'Give a concise three-sentence feature specification for a slugify library with Unicode normalization, a CLI, and acceptance tests. Do not write code.'
        Click-Control $liveGui.Window 'ChatSendButton'
        Wait-Until {
            (Find-Control $liveGui.Window 'ChatUseFeatureButton').Current.IsEnabled
        } 'The isolated ChatGPT fixture did not complete its GUI chat reply.' 120 | Out-Null
        $liveChat = Read-ControlText $liveGui.Window 'ChatTranscriptBox'
        if (Test-Path -LiteralPath (Join-Path $liveState 'latest.json')) {
            throw 'The fixture planning chat executed a feature without an owner action.'
        }
        Select-Tab $liveGui.Window 'FeatureEditorTab' 'Feature'
        Wait-Until {
            try { return Find-Control $liveGui.Window 'RunDemoButton' } catch { return $null }
        } 'The feature editor did not expose its demo control.' | Out-Null
        $demoRequestBaseline = if (Test-Path -LiteralPath $requestsFile) { @(Get-Content -LiteralPath $requestsFile).Count } else { 0 }
        Click-Control $liveGui.Window 'RunDemoButton'
        $extendedFixtureReport = Wait-Run $liveState 'completed' 180
        Assert-GuiCompleted $liveGui
        $demoJobPath = Wait-Until {
            $demoJobsRoot = Join-Path $testRoot 'target/windows-local-demo'
            if (-not (Test-Path -LiteralPath $demoJobsRoot)) { return $null }
            return Get-ChildItem -LiteralPath $demoJobsRoot -Filter 'job.json' -File -Recurse |
                Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1 -ExpandProperty FullName
        } 'The packaged demo did not retain its submitted job.'
        $submittedDemo = Read-TestJson $demoJobPath
        if ($submittedDemo.endpoint -ne "http://127.0.0.1:$port/v1" -or $submittedDemo.model -ne 'gui-fixture' -or $submittedDemo.reasoning_effort -ne 'low' -or
            $submittedDemo.orchestrator_provider -ne 'chatgpt' -or $submittedDemo.orchestrator_model -ne 'gpt-plan' -or $submittedDemo.orchestrator_reasoning_effort -ne 'high' -or
            $submittedDemo.reviewer_provider -ne 'chatgpt' -or $submittedDemo.reviewer_model -ne 'gpt-review' -or $submittedDemo.reviewer_reasoning_effort -ne 'xhigh' -or
            $submittedDemo.max_attempts -ne 2 -or $submittedDemo.validation.timeout_seconds -ne 47 -or
            $submittedDemo.validation.program -ne $python -or (@($submittedDemo.validation.args) -join "`n") -ne (@('-m', 'unittest', 'discover', '-s', 'tests', '-v') -join "`n")) {
            throw 'Run demo did not submit the exact saved implementation, role, attempt, and timeout settings.'
        }
        $demoRequests = @()
        if (Test-Path -LiteralPath $requestsFile) {
            $demoRequests = @(Get-Content -LiteralPath $requestsFile | Select-Object -Skip $demoRequestBaseline | ForEach-Object { $_ | ConvertFrom-Json })
        }
        if (@($demoRequests | Where-Object { $_.model -eq 'gui-fixture' -and $_.messages[-1].content -match 'src/slugify.py' }).Count -lt 1) {
            throw 'The packaged demo did not call the configured isolated local implementation model.'
        }
        $acceptanceStdout = Join-Path $testRoot 'demo-acceptance.stdout.log'
        $acceptanceStderr = Join-Path $testRoot 'demo-acceptance.stderr.log'
        $acceptanceProcess = Start-Process -FilePath $python -ArgumentList @('-m', 'unittest', 'discover', '-s', 'tests', '-v') -WorkingDirectory $extendedFixtureReport.candidate -WindowStyle Hidden -RedirectStandardOutput $acceptanceStdout -RedirectStandardError $acceptanceStderr -PassThru -Wait
        $acceptanceOutput = [System.IO.File]::ReadAllText($acceptanceStdout, $utf8) + [System.IO.File]::ReadAllText($acceptanceStderr, $utf8)
        if ($acceptanceProcess.ExitCode -ne 0 -or $acceptanceOutput -notmatch 'Ran 12 tests') {
            throw 'The completed demo candidate did not pass all twelve acceptance tests at the real process boundary.'
        }
        $acceptanceProcess.Dispose()
        Close-Gui $liveGui
    }
    $evidence = [ordered]@{
        status = 'passed'; boundary = 'Native WinForms controls to isolated fixture runners and real validation processes'
        proof_scope = 'fixture-only; no authenticated ChatGPT or live local-model claim'
        success_run_id = $successReport.run_id; cancelled_run_id = $cancelReport.run_id
        pause_resume = 'passed'; progress_log = 'passed'; source_preservation = 'passed'
        active_close_cancelled_run_id = $closedReport.run_id
        failed_cancel_keeps_window_open = 'passed'; recovered_close_run_id = $failureRecoveredReport.run_id
        header_icons = 'passed'; local_chat = 'passed'; chat_draft_has_no_execution_authority = 'passed'
        assembly_line_run_ids = @($completedLine.entries | ForEach-Object { $_.run_id })
        assembly_line_failure_cancel_restart = $lineStopEvidence
        assembly_line_preflight_stop_no_launch = 'passed'
        chat_stop_and_history = 'passed'
        chat_emergency_pause_resume = 'passed'
        owner_approval_cases = $approvalCases
        separate_role_settings = [ordered]@{ revision = $roleSettings.revision; implementation_model = $roleSettings.model; implementation_reasoning_effort = $roleSettings.reasoning_effort; orchestrator_provider = $roleSettings.orchestrator_provider; orchestrator_model = $roleSettings.orchestrator_model; orchestrator_reasoning_effort = $roleSettings.orchestrator_reasoning_effort; reviewer_provider = $roleSettings.reviewer_provider; reviewer_model = $roleSettings.reviewer_model; reviewer_reasoning_effort = $roleSettings.reviewer_reasoning_effort }
        github_publication_cases = $publicationCases
        extended_fixture_run_id = $(if ($extendedFixtureReport) { $extendedFixtureReport.run_id } else { $null })
        extended_fixture_chat = $(if ($LiveDemo) { 'passed' } else { $null })
        extended_fixture_saved_job = $(if ($LiveDemo) { [ordered]@{ endpoint = $submittedDemo.endpoint; implementation_model = $submittedDemo.model; implementation_reasoning_effort = $submittedDemo.reasoning_effort; orchestrator_model = $submittedDemo.orchestrator_model; orchestrator_reasoning_effort = $submittedDemo.orchestrator_reasoning_effort; reviewer_model = $submittedDemo.reviewer_model; reviewer_reasoning_effort = $submittedDemo.reviewer_reasoning_effort; max_attempts = $submittedDemo.max_attempts; timeout_seconds = $submittedDemo.validation.timeout_seconds; acceptance_tests = 12 } } else { $null })
    }
    $evidencePath = Join-Path $evidenceRoot 'gui-e2e.json'
    Write-Utf8File $evidencePath ($evidence | ConvertTo-Json -Depth 6)
    Write-Host "Windows GUI E2E passed: $evidencePath"
} catch {
    Write-Host $_.ScriptStackTrace
    $failureRecord = [ordered]@{ error = $_.Exception.Message; stack = $_.ScriptStackTrace; windows = @() }
    foreach ($failureInstance in $guiInstances) {
        if (-not $failureInstance.Process.HasExited) {
            $failureLog = ''
            try { $failureLog = Read-ControlText $failureInstance.Window 'LogBox' } catch { }
            $failureRecord.windows += [ordered]@{ state = $failureInstance.State; log = $failureLog }
        }
    }
    Write-Utf8File (Join-Path $evidenceRoot 'failure.json') ($failureRecord | ConvertTo-Json -Depth 6)
    throw
} finally {
    $env:ASSEMBLYWRIGHT_CHATGPT_TEST_FIXTURE = $priorChatGptFixture
    foreach ($gui in $guiInstances) {
        if (-not $gui.Process.HasExited) {
            try { Close-Gui $gui } catch {
                & (Join-Path (Split-Path -Parent $guiExe) 'assemblywright-local.exe') cancel --state-dir $gui.State | Out-Null
                $gui.Process.Kill()
                $gui.Process.WaitForExit()
            }
        }
        $gui.Process.Dispose()
    }
    if ($modelProcess) {
        if (-not $modelProcess.HasExited) { $modelProcess.Kill(); $modelProcess.WaitForExit() }
        $modelProcess.Dispose()
    }
    $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
    $temporaryPrefix = $temporaryBase.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $resolvedTestRoot.StartsWith($temporaryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing to remove a GUI test directory outside the intended temporary root.'
    }
    if (Test-Path -LiteralPath $resolvedTestRoot) { Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force }
}
