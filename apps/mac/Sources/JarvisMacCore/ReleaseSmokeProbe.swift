import Foundation

public enum JarvisReleaseSmokeProbeStep: String, Equatable, Sendable {
    case health
    case initialCommand
    case taskLookup
    case taskList
    case taskAudit
    case diagnostics
    case schedulerCreate
    case schedulerBackgroundExecution
    case schedulerAudit
    case schedulerNotificationOutbox
    case schedulerNotificationAcknowledgement
    case pause
    case pausedStatus
    case pausedCommand
    case resume
    case resumedStatus
}

public enum JarvisReleaseSmokeProbeError: Error, Equatable, Sendable {
    case timedOut
    case validationFailed(JarvisReleaseSmokeProbeStep)
}

public struct JarvisReleaseSmokeProbe: Sendable {
    public static let successLine = "Assemblywright release smoke: default supervised Unix IPC route sequence verified"

    private static let commandInput = "Assemblywright release smoke deterministic dry-run check."
    private static let pauseReason = "Assemblywright release smoke emergency-pause check."
    private static let schedulerCommand = "Assemblywright release smoke scheduler background check."

    private let client: any JarvisCoreClient
    private let timeout: Duration

    public init(
        client: any JarvisCoreClient,
        timeout: Duration = .seconds(60)
    ) {
        self.client = client
        self.timeout = timeout
    }

    public func run() async throws -> String {
        try await withThrowingTaskGroup(of: String.self) { group in
            group.addTask {
                try await runSequence()
            }
            group.addTask {
                try await Task.sleep(for: timeout)
                throw JarvisReleaseSmokeProbeError.timedOut
            }

            defer { group.cancelAll() }
            guard let result = try await group.next() else {
                throw JarvisReleaseSmokeProbeError.timedOut
            }
            return result
        }
    }

    private func runSequence() async throws -> String {
        var resumeCleanupRequired = false

        do {
            let health = try await client.health()
            try require(
                health.status == "ok" && !health.emergencyPaused,
                step: .health
            )

            let initialCommand = try await client.submit(
                JarvisCommandRequest(
                    input: Self.commandInput,
                    dryRun: true,
                    memoryContext: false,
                    installedWasmTools: false
                )
            )
            try require(
                initialCommand.accepted && initialCommand.task.status == "completed",
                step: .initialCommand
            )

            let task = try await client.task(id: initialCommand.task.id)
            try require(
                task.id == initialCommand.task.id && task.status == initialCommand.task.status,
                step: .taskLookup
            )

            let tasks = try await client.listTasks()
            try require(
                tasks.contains(where: { $0.id == initialCommand.task.id }),
                step: .taskList
            )

            let auditEntries = try await client.listAuditEntries(taskId: initialCommand.task.id)
            try require(
                auditEntries.contains(where: { $0.taskId == initialCommand.task.id }),
                step: .taskAudit
            )

            let diagnostics = try await client.diagnosticsExport()
            try require(
                diagnostics.health.status == "ok"
                    && !diagnostics.redaction.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                step: .diagnostics
            )

            let dueAt = ISO8601DateFormatter().string(from: Date().addingTimeInterval(-1))
            let scheduledJob = try await client.createSchedulerJob(
                JarvisCreateSchedulerJobRequest(
                    name: "release smoke background scheduler",
                    command: Self.schedulerCommand,
                    trigger: .onceAt(runAt: dueAt)
                )
            )
            try require(scheduledJob.status == "scheduled", step: .schedulerCreate)

            var completedJob = scheduledJob
            for _ in 0..<100 where completedJob.status != "completed" {
                try await Task.sleep(for: .milliseconds(100))
                completedJob = try await client.schedulerJob(id: scheduledJob.id)
            }
            try require(completedJob.status == "completed", step: .schedulerBackgroundExecution)

            let schedulerAudit = try await client.listAuditEntries(taskId: nil)
            try require(
                schedulerAudit.contains(where: {
                    $0.eventType == "scheduler_job_completed"
                        && Self.payload($0.payload, containsSchedulerJobID: scheduledJob.id)
                }),
                step: .schedulerAudit
            )

            let pendingNotifications = try await client.pendingSchedulerNotificationOccurrences(
                limit: 64
            )
            guard let occurrence = pendingNotifications.first(where: {
                $0.schedulerJobId == scheduledJob.id
            }) else {
                throw JarvisReleaseSmokeProbeError.validationFailed(
                    .schedulerNotificationOutbox
                )
            }
            _ = try await client.acknowledgeSchedulerNotificationOccurrence(
                id: occurrence.id,
                request: JarvisSchedulerNotificationAcknowledgementRequest(
                    revision: occurrence.revision,
                    disposition: .suppressedNotAuthorized
                )
            )
            let remainingNotifications = try await client.pendingSchedulerNotificationOccurrences(
                limit: 64
            )
            try require(
                !remainingNotifications.contains(where: { $0.id == occurrence.id }),
                step: .schedulerNotificationAcknowledgement
            )

            // A failed pause response is ambiguous: the server may have applied the pause
            // before transport failed. Resume is therefore required from this point onward.
            resumeCleanupRequired = true
            let pause = try await client.pause(reason: Self.pauseReason)
            try require(pause.paused, step: .pause)

            let pausedStatus = try await client.pauseStatus()
            try require(pausedStatus.paused, step: .pausedStatus)

            let blockedCommand = try await client.submit(
                JarvisCommandRequest(
                    input: Self.commandInput,
                    dryRun: true,
                    memoryContext: false,
                    installedWasmTools: false
                )
            )
            let hasPauseAudit = ([blockedCommand.auditEntry] + blockedCommand.auditEntries)
                .contains(where: {
                    $0.taskId == blockedCommand.task.id
                        && $0.eventType == "emergency_pause_blocked"
                })
            try require(
                !blockedCommand.accepted
                    && blockedCommand.task.status == "blocked"
                    && hasPauseAudit,
                step: .pausedCommand
            )

            let resume = try await client.resume()
            try require(!resume.paused, step: .resume)

            let resumedStatus = try await client.pauseStatus()
            try require(!resumedStatus.paused, step: .resumedStatus)
            resumeCleanupRequired = false

            return Self.successLine
        } catch {
            if resumeCleanupRequired {
                _ = await Task.detached { [client] in
                    try? await client.resume()
                }.value
            }
            throw error
        }
    }

    private func require(
        _ condition: @autoclosure () -> Bool,
        step: JarvisReleaseSmokeProbeStep
    ) throws {
        guard condition() else {
            throw JarvisReleaseSmokeProbeError.validationFailed(step)
        }
    }

    private static func payload(
        _ payload: JarvisJSONValue?,
        containsSchedulerJobID id: UUID
    ) -> Bool {
        guard case let .object(object) = payload,
              case let .string(value) = object["scheduler_job_id"]
        else {
            return false
        }
        return value.lowercased() == id.uuidString.lowercased()
    }
}
