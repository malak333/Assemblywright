param(
    [Parameter(Mandatory = $true)]
    [ValidateSet(
        "EnqueueSuccess",
        "EnqueueCancellation",
        "EnqueueCancellationAndPause",
        "Pause",
        "Resume"
    )]
    [string]$Action,

    [string]$DataDir = (Join-Path $env:LOCALAPPDATA "Assemblywright\master"),

    [ValidatePattern("^127\.0\.0\.1:[0-9]{1,5}$")]
    [string]$Endpoint = "127.0.0.1:7791",

    [string]$TaskId,
    [string]$StepId,
    [string]$StreamId,
    [UInt64]$AfterSequence,
    [string]$ExpectedDeviceId,
    [UInt64]$ConnectionEpoch,

    [switch]$ConfirmAction
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if (-not $ConfirmAction) {
    throw "MLX live control requires -ConfirmAction for this Windows-local owner action."
}

$tokenPath = Join-Path $DataDir "development.token"
if (-not (Test-Path -LiteralPath $tokenPath -PathType Leaf)) {
    throw "The Windows-local development token is unavailable."
}
$token = (Get-Content -LiteralPath $tokenPath -Raw).Trim()
if ($token.Length -lt 32 -or $token.Length -gt 256 -or $token -notmatch "^[\x21-\x7e]+$") {
    throw "The Windows-local development token is invalid."
}
$headers = @{ Authorization = "Bearer $token" }
$baseUri = "http://$Endpoint"
$uuidPattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
# Must equal PROTOCOL_VERSION in crates/assemblywright-protocol/src/lib.rs.
# release-docs-drift-smoke.sh asserts that pairing.
$protocolVersion = 4

function Invoke-ExactPost {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$Body
    )
    Invoke-RestMethod `
        -Method Post `
        -Uri "$baseUri$Path" `
        -Headers $headers `
        -ContentType "application/json" `
        -Body $Body
}

function Wait-ExactMLXEvents {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ExpectedTaskId,
        [Parameter(Mandatory = $true)]
        [string]$ExpectedStepId,
        [Parameter(Mandatory = $true)]
        [string[]]$ExpectedKinds,
        [Parameter(Mandatory = $true)]
        [string]$ExpectedDeviceId,
        [string]$ExpectedStreamId,
        [UInt64]$ExpectedConnectionEpoch = 0,
        [UInt64]$InitialAfterSequence = 0,
        [int]$TimeoutSeconds = 240
    )

    $cursor = $null
    if ($ExpectedStreamId) {
        $cursor = [ordered]@{
            stream_id = $ExpectedStreamId
            sequence = $InitialAfterSequence
        }
    }
    $observedStreamId = $ExpectedStreamId
    $observedSequences = [ordered]@{}
    $observedConnectionEpoch = if ($ExpectedConnectionEpoch -gt 0) {
        $ExpectedConnectionEpoch
    } else {
        $null
    }
    $expectedIndex = 0
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)

    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        $requestedAfter = if ($cursor) { [UInt64]$cursor.sequence } else { [UInt64]0 }
        $query = [ordered]@{
            protocol_version = $protocolVersion
            connection_epoch = 1
            after = $cursor
            limit = 64
        }
        $batch = Invoke-ExactPost `
            -Path "/v1/development/events/next" `
            -Body ($query | ConvertTo-Json -Compress -Depth 4)
        if (
            $batch.protocol_version -ne $protocolVersion -or
            $batch.stream_id -notmatch $uuidPattern -or
            [UInt64]$batch.after_sequence -ne $requestedAfter -or
            [UInt64]$batch.next_sequence -lt $requestedAfter -or
            $batch.has_more -isnot [bool]
        ) {
            throw "The Windows master returned invalid event metadata."
        }
        if ($observedStreamId -and $batch.stream_id -ne $observedStreamId) {
            throw "The Windows master event stream changed during MLX observation."
        }
        $observedStreamId = $batch.stream_id

        $expectedPageSequence = $requestedAfter
        foreach ($event in @($batch.events)) {
            $expectedPageSequence += 1
            if (
                $event.cursor.stream_id -ne $observedStreamId -or
                [UInt64]$event.cursor.sequence -ne $expectedPageSequence
            ) {
                throw "The Windows master event page was not contiguous."
            }
            if ($event.task_id -ne $ExpectedTaskId -or $event.step_id -ne $ExpectedStepId) {
                continue
            }
            $eventIsUnbound = $event.kind -eq "step_queued" -or $event.kind -eq "step_cancelled"
            if ($eventIsUnbound) {
                if ($null -ne $event.device_id -or $null -ne $event.connection_epoch) {
                    throw "An unbound MLX event unexpectedly carried worker identity."
                }
            } else {
                if (
                    $event.device_id -ne $ExpectedDeviceId -or
                    [UInt64]$event.connection_epoch -eq 0
                ) {
                    throw "The MLX event was not bound to the expected Mac device."
                }
                if (
                    $null -ne $observedConnectionEpoch -and
                    [UInt64]$event.connection_epoch -ne [UInt64]$observedConnectionEpoch
                ) {
                    throw "The MLX event connection epoch changed within one attempt."
                }
                $observedConnectionEpoch = [UInt64]$event.connection_epoch
            }
            if ($expectedIndex -ge $ExpectedKinds.Count -or $event.kind -ne $ExpectedKinds[$expectedIndex]) {
                throw "The exact MLX event order was invalid."
            }
            if ($event.cursor.stream_id -ne $observedStreamId -or [UInt64]$event.cursor.sequence -le $InitialAfterSequence) {
                throw "The exact MLX event cursor was invalid."
            }
            $observedSequences[$event.kind] = [UInt64]$event.cursor.sequence
            $expectedIndex += 1
        }
        if ([UInt64]$batch.next_sequence -ne $expectedPageSequence) {
            throw "The Windows master event page ended at an invalid cursor."
        }
        if ($expectedIndex -eq $ExpectedKinds.Count -and -not $batch.has_more) {
            return [ordered]@{
                stream_id = $observedStreamId
                device_id = $ExpectedDeviceId
                connection_epoch = [UInt64]$observedConnectionEpoch
                sequences = $observedSequences
            }
        }

        $nextSequence = [UInt64]$batch.next_sequence
        if ($cursor -and $nextSequence -lt [UInt64]$cursor.sequence) {
            throw "The Windows master event cursor moved backwards."
        }
        $cursor = [ordered]@{
            stream_id = $observedStreamId
            sequence = $nextSequence
        }
        if (-not $batch.has_more) {
            Start-Sleep -Milliseconds 250
        }
    }
    throw "Timed out waiting for the exact MLX event lifecycle."
}

function Wait-NoExactMLXEvent {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ExpectedTaskId,
        [Parameter(Mandatory = $true)]
        [string]$ExpectedStepId,
        [Parameter(Mandatory = $true)]
        [string]$ExpectedStreamId,
        [Parameter(Mandatory = $true)]
        [UInt64]$AfterSequence,
        [int]$ObservationMilliseconds = 7000
    )

    $cursor = [ordered]@{
        stream_id = $ExpectedStreamId
        sequence = $AfterSequence
    }
    $deadline = [DateTimeOffset]::UtcNow.AddMilliseconds($ObservationMilliseconds)
    $postDeadlinePageCount = 0
    $maximumPostDeadlinePages = 256
    while ($true) {
        $requestedAfter = [UInt64]$cursor.sequence
        $queryStartedAfterDeadline = [DateTimeOffset]::UtcNow -ge $deadline
        $query = [ordered]@{
            protocol_version = $protocolVersion
            connection_epoch = 1
            after = $cursor
            limit = 64
        }
        $batch = Invoke-ExactPost `
            -Path "/v1/development/events/next" `
            -Body ($query | ConvertTo-Json -Compress -Depth 4)
        if (
            $batch.protocol_version -ne $protocolVersion -or
            $batch.stream_id -ne $ExpectedStreamId -or
            [UInt64]$batch.after_sequence -ne $requestedAfter -or
            [UInt64]$batch.next_sequence -lt $requestedAfter -or
            $batch.has_more -isnot [bool]
        ) {
            throw "The Windows master event cursor regressed during late-output observation."
        }
        $expectedSequence = $requestedAfter
        foreach ($event in @($batch.events)) {
            $expectedSequence += 1
            if (
                $event.cursor.stream_id -ne $ExpectedStreamId -or
                [UInt64]$event.cursor.sequence -ne $expectedSequence
            ) {
                throw "The Windows master event page was not contiguous during late-output observation."
            }
            if ($event.task_id -eq $ExpectedTaskId -and $event.step_id -eq $ExpectedStepId) {
                throw "The cancelled MLX emitted a late or duplicate task event."
            }
        }
        if ([UInt64]$batch.next_sequence -ne $expectedSequence) {
            throw "The Windows master event page ended at an invalid cursor."
        }
        $cursor = [ordered]@{
            stream_id = $ExpectedStreamId
            sequence = [UInt64]$batch.next_sequence
        }
        if ($queryStartedAfterDeadline) {
            $postDeadlinePageCount += 1
            if ($postDeadlinePageCount -gt $maximumPostDeadlinePages) {
                throw "The Windows master event stream did not reach a bounded durable head."
            }
        }
        if ($queryStartedAfterDeadline -and -not $batch.has_more) {
            return
        }
        if (-not $batch.has_more) {
            $remainingMilliseconds = [int][Math]::Ceiling(
                ($deadline - [DateTimeOffset]::UtcNow).TotalMilliseconds
            )
            if ($remainingMilliseconds -gt 0) {
                Start-Sleep -Milliseconds ([Math]::Min(250, $remainingMilliseconds))
            }
        }
    }
}

switch ($Action) {
    "EnqueueSuccess" {
        if ($ExpectedDeviceId -notmatch $uuidPattern) {
            throw "EnqueueSuccess requires the exact enrolled Mac device ID."
        }
        $task = [guid]::NewGuid().ToString().ToLowerInvariant()
        $step = [guid]::NewGuid().ToString().ToLowerInvariant()
        $request = [ordered]@{
            task_id = $task
            step_id = $step
            capability_id = "mlx.reasoning"
            sensitivity = "public"
            context = [ordered]@{
                operation = "generate_text"
                prompt = "Reply with the exact token ASSEMBLYWRIGHT_MLX_DISTRIBUTED_OK"
                max_tokens = 32
                temperature_milli = 0
            }
            lease_duration_ms = 60000
            deadline_after_ms = 300000
        }
        $accepted = Invoke-ExactPost `
            -Path "/v1/development/steps" `
            -Body ($request | ConvertTo-Json -Compress -Depth 4)
        if ($accepted.accepted -ne $true) {
            throw "The Windows master did not accept the success MLX."
        }
        $evidence = Wait-ExactMLXEvents `
            -ExpectedTaskId $task `
            -ExpectedStepId $step `
            -ExpectedKinds @("step_queued", "step_leased", "step_succeeded") `
            -ExpectedDeviceId $ExpectedDeviceId
        [ordered]@{
            schema_version = 1
            status = "mlx_success_observed"
            task_id = $task
            step_id = $step
            stream_id = $evidence.stream_id
            device_id = $evidence.device_id
            connection_epoch = $evidence.connection_epoch
            queued_sequence = $evidence.sequences.step_queued
            leased_sequence = $evidence.sequences.step_leased
            succeeded_sequence = $evidence.sequences.step_succeeded
        } | ConvertTo-Json -Compress
    }
    "EnqueueCancellation" {
        if ($ExpectedDeviceId -notmatch $uuidPattern) {
            throw "EnqueueCancellation requires the exact enrolled Mac device ID."
        }
        $task = [guid]::NewGuid().ToString().ToLowerInvariant()
        $step = [guid]::NewGuid().ToString().ToLowerInvariant()
        $request = [ordered]@{
            task_id = $task
            step_id = $step
            capability_id = "mlx.reasoning"
            sensitivity = "public"
            context = [ordered]@{
                operation = "generate_text"
                prompt = "Output the decimal integers 1 through 10000 in order, separated by a single space. Do not summarize, explain, abbreviate, or stop before 10000."
                max_tokens = 512
                temperature_milli = 0
            }
            lease_duration_ms = 60000
            deadline_after_ms = 300000
        }
        $accepted = Invoke-ExactPost `
            -Path "/v1/development/steps" `
            -Body ($request | ConvertTo-Json -Compress -Depth 4)
        if ($accepted.accepted -ne $true) {
            throw "The Windows master did not accept the cancellation MLX."
        }
        $evidence = Wait-ExactMLXEvents `
            -ExpectedTaskId $task `
            -ExpectedStepId $step `
            -ExpectedKinds @("step_queued", "step_leased") `
            -ExpectedDeviceId $ExpectedDeviceId
        [ordered]@{
            schema_version = 1
            status = "mlx_cancellation_leased"
            task_id = $task
            step_id = $step
            stream_id = $evidence.stream_id
            device_id = $evidence.device_id
            connection_epoch = $evidence.connection_epoch
            queued_sequence = $evidence.sequences.step_queued
            leased_sequence = $evidence.sequences.step_leased
        } | ConvertTo-Json -Compress
    }
    "EnqueueCancellationAndPause" {
        if ($ExpectedDeviceId -notmatch $uuidPattern) {
            throw "EnqueueCancellationAndPause requires the exact enrolled Mac device ID."
        }
        $task = [guid]::NewGuid().ToString().ToLowerInvariant()
        $step = [guid]::NewGuid().ToString().ToLowerInvariant()
        $request = [ordered]@{
            task_id = $task
            step_id = $step
            capability_id = "mlx.reasoning"
            sensitivity = "public"
            context = [ordered]@{
                operation = "generate_text"
                prompt = "Output the decimal integers 1 through 10000 in order, separated by a single space. Do not summarize, explain, abbreviate, or stop before 10000."
                max_tokens = 512
                temperature_milli = 0
            }
            lease_duration_ms = 60000
            deadline_after_ms = 300000
        }
        $accepted = Invoke-ExactPost `
            -Path "/v1/development/steps" `
            -Body ($request | ConvertTo-Json -Compress -Depth 4)
        if ($accepted.accepted -ne $true) {
            throw "The Windows master did not accept the cancellation MLX."
        }
        $leased = Wait-ExactMLXEvents `
            -ExpectedTaskId $task `
            -ExpectedStepId $step `
            -ExpectedKinds @("step_queued", "step_leased") `
            -ExpectedDeviceId $ExpectedDeviceId
        [ordered]@{
            schema_version = 1
            status = "mlx_cancellation_leased"
            task_id = $task
            step_id = $step
            stream_id = $leased.stream_id
            device_id = $leased.device_id
            connection_epoch = $leased.connection_epoch
            queued_sequence = $leased.sequences.step_queued
            leased_sequence = $leased.sequences.step_leased
        } | ConvertTo-Json -Compress

        $paused = Invoke-ExactPost `
            -Path "/v1/development/emergency-pause/activate" `
            -Body "{}"
        if ($paused.emergency_paused -ne $true) {
            throw "The Windows master did not enter MLX emergency pause."
        }
        $cancelled = Wait-ExactMLXEvents `
            -ExpectedTaskId $task `
            -ExpectedStepId $step `
            -ExpectedKinds @(
                "step_cancellation_requested",
                "step_cancellation_acknowledged",
                "step_cancelled"
            ) `
            -ExpectedDeviceId $ExpectedDeviceId `
            -ExpectedStreamId $leased.stream_id `
            -ExpectedConnectionEpoch $leased.connection_epoch `
            -InitialAfterSequence $leased.sequences.step_leased
        Wait-NoExactMLXEvent `
            -ExpectedTaskId $task `
            -ExpectedStepId $step `
            -ExpectedStreamId $leased.stream_id `
            -AfterSequence $cancelled.sequences.step_cancelled `
            -ObservationMilliseconds 7000
        [ordered]@{
            schema_version = 1
            status = "mlx_cancellation_observed"
            task_id = $task
            step_id = $step
            stream_id = $leased.stream_id
            device_id = $cancelled.device_id
            connection_epoch = $cancelled.connection_epoch
            requested_sequence = $cancelled.sequences.step_cancellation_requested
            acknowledged_sequence = $cancelled.sequences.step_cancellation_acknowledged
            cancelled_sequence = $cancelled.sequences.step_cancelled
            late_output_window_ms = 7000
        } | ConvertTo-Json -Compress
    }
    "Pause" {
        if (
            $TaskId -notmatch $uuidPattern -or
            $StepId -notmatch $uuidPattern -or
            $StreamId -notmatch $uuidPattern -or
            $ExpectedDeviceId -notmatch $uuidPattern -or
            $ConnectionEpoch -eq 0 -or
            $AfterSequence -eq 0
        ) {
            throw "Pause requires exact task, step, stream, and leased-sequence evidence."
        }
        $paused = Invoke-ExactPost `
            -Path "/v1/development/emergency-pause/activate" `
            -Body "{}"
        if ($paused.emergency_paused -ne $true) {
            throw "The Windows master did not enter MLX emergency pause."
        }
        $evidence = Wait-ExactMLXEvents `
            -ExpectedTaskId $TaskId `
            -ExpectedStepId $StepId `
            -ExpectedKinds @(
                "step_cancellation_requested",
                "step_cancellation_acknowledged",
                "step_cancelled"
            ) `
            -ExpectedDeviceId $ExpectedDeviceId `
            -ExpectedStreamId $StreamId `
            -ExpectedConnectionEpoch $ConnectionEpoch `
            -InitialAfterSequence $AfterSequence
        Wait-NoExactMLXEvent `
            -ExpectedTaskId $TaskId `
            -ExpectedStepId $StepId `
            -ExpectedStreamId $StreamId `
            -AfterSequence $evidence.sequences.step_cancelled `
            -ObservationMilliseconds 7000
        [ordered]@{
            schema_version = 1
            status = "mlx_cancellation_observed"
            task_id = $TaskId
            step_id = $StepId
            stream_id = $StreamId
            device_id = $evidence.device_id
            connection_epoch = $evidence.connection_epoch
            requested_sequence = $evidence.sequences.step_cancellation_requested
            acknowledged_sequence = $evidence.sequences.step_cancellation_acknowledged
            cancelled_sequence = $evidence.sequences.step_cancelled
            late_output_window_ms = 7000
        } | ConvertTo-Json -Compress
    }
    "Resume" {
        $resumed = Invoke-ExactPost `
            -Path "/v1/development/emergency-pause/resume" `
            -Body "{}"
        if ($resumed.emergency_paused -ne $false) {
            throw "The Windows master did not resume MLX admission."
        }
        '{"schema_version":1,"status":"mlx_emergency_resumed"}'
    }
}
