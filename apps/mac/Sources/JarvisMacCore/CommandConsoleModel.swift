import Foundation

@MainActor
public final class CommandConsoleModel: ObservableObject {
    @Published public private(set) var health: JarvisHealth?
    @Published public private(set) var transcript: [TranscriptEntry]
    @Published public private(set) var activity: [ActivityEntry]
    @Published public private(set) var isPaused: Bool
    @Published public private(set) var pauseStatus: JarvisPauseResponse?
    @Published public private(set) var isWorking: Bool
    @Published public private(set) var isCancelling: Bool
    @Published public private(set) var activeCancellationID: UUID?
    @Published public private(set) var cancellationStatus: String?
    @Published public private(set) var lastError: String?
    @Published public private(set) var isDegraded: Bool
    @Published public private(set) var degradedReason: String?
    @Published public var memoryContextEnabled: Bool
    @Published public var installedWasmToolsEnabled: Bool
    @Published public var toolExecutionEnabled: Bool

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.transcript = []
        self.activity = []
        self.isPaused = false
        self.pauseStatus = nil
        self.isWorking = false
        self.isCancelling = false
        self.activeCancellationID = nil
        self.cancellationStatus = nil
        self.lastError = nil
        self.isDegraded = false
        self.degradedReason = nil
        self.memoryContextEnabled = false
        self.installedWasmToolsEnabled = false
        self.toolExecutionEnabled = false
    }

    public func refreshHealth() async {
        await run {
            let health = try await self.client.health()
            self.health = health
            self.isPaused = health.emergencyPaused
            self.pauseStatus = JarvisPauseResponse(
                paused: health.emergencyPaused,
                reason: health.emergencyPauseReason,
                pausedAt: nil,
                resumedAt: nil,
                cancelledSchedulerJobs: 0
            )
            self.clearDegradedMode()
        }
    }

    public func markDegraded(_ reason: String) {
        health = nil
        isDegraded = true
        degradedReason = reason
    }

    public func refreshPauseStatus() async {
        await run {
            let response = try await self.client.pauseStatus()
            self.pauseStatus = response
            self.isPaused = response.paused
        }
    }

    public func submit(
        input: String,
        dryRun: Bool = true,
        memoryContext: Bool? = nil,
        installedWasmTools: Bool? = nil
    ) async {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        guard !isDegraded else {
            lastError = degradedReason ?? "Jarvis core is unavailable."
            return
        }
        guard activeCancellationID == nil, !isWorking else {
            lastError = "A command is already active; cancel it or wait for completion before submitting another."
            return
        }

        transcript.append(TranscriptEntry(role: .user, text: trimmed))
        let cancellationID = UUID()
        activeCancellationID = cancellationID
        cancellationStatus = nil
        defer {
            if activeCancellationID == cancellationID {
                activeCancellationID = nil
            }
        }
        await run {
            let response = try await self.client.submit(
                JarvisCommandRequest(
                    input: trimmed,
                    dryRun: dryRun,
                    memoryContext: memoryContext ?? self.memoryContextEnabled,
                    installedWasmTools: installedWasmTools ?? self.installedWasmToolsEnabled,
                    cancellationID: cancellationID
                )
            )
            self.transcript.append(
                TranscriptEntry(role: .assistant, text: response.message)
            )
            self.activity.insert(contentsOf: ActivityEntry.entries(from: response), at: 0)
        }
    }

    public func cancelActiveCommand() async {
        guard let cancellationID = activeCancellationID, !isCancelling else { return }
        isCancelling = true
        lastError = nil
        defer { isCancelling = false }

        do {
            let response = try await client.cancelCommand(cancellationID: cancellationID)
            cancellationStatus = response.outcome
            if !response.activeExecutionFound {
                lastError = "The command was no longer active when cancellation reached the core."
            }
        } catch {
            lastError = String(describing: error)
            markDegraded("Command cancellation failed: \(error)")
        }
    }

    public func pause() async {
        await run {
            let response = try await self.client.pause(reason: "user requested from Mac shell")
            self.isPaused = response.paused
            self.pauseStatus = response
        }
    }

    public func resume() async {
        await run {
            let response = try await self.client.resume()
            self.isPaused = response.paused
            self.pauseStatus = response
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) async {
        isWorking = true
        lastError = nil
        defer { isWorking = false }

        do {
            try await operation()
            clearDegradedMode()
        } catch {
            let message = String(describing: error)
            lastError = message
            isDegraded = true
            degradedReason = message
        }
    }

    private func clearDegradedMode() {
        isDegraded = false
        degradedReason = nil
    }
}

public struct ActivityEntry: Identifiable, Equatable, Sendable {
    public var id: UUID
    public var title: String
    public var detail: String
    public var badge: String

    public init(id: UUID = UUID(), title: String, detail: String, badge: String) {
        self.id = id
        self.title = title
        self.detail = detail
        self.badge = badge
    }

    public static func entries(from response: JarvisCommandResponse) -> [ActivityEntry] {
        var entries: [ActivityEntry] = [
            ActivityEntry(
                id: response.task.id,
                title: "Task \(response.task.status)",
                detail: response.task.userInput,
                badge: response.accepted ? "accepted" : "blocked"
            )
        ]

        if let route = response.route {
            entries.append(
                ActivityEntry(
                    title: "Route \(route.provider)",
                    detail: "\(route.model): \(route.reason)",
                    badge: "model"
                )
            )
        }

        entries.append(contentsOf: response.steps.map { step in
            ActivityEntry(
                title: "Step \(step.index + 1)",
                detail: step.message,
                badge: step.complete ? "complete" : "running"
            )
        })

        entries.append(contentsOf: response.pluginResults.map { result in
            ActivityEntry(
                title: "\(result.metadata.pluginId).\(result.metadata.action)",
                detail: "risk: \(result.metadata.riskTier), approval: \(result.metadata.approvalStatus)",
                badge: result.status
            )
        })

        entries.append(contentsOf: response.auditEntries.map { audit in
            ActivityEntry(
                id: audit.id,
                title: audit.eventType,
                detail: audit.summary,
                badge: "audit"
            )
        })

        return entries
    }
}

public struct TranscriptEntry: Identifiable, Equatable, Sendable {
    public enum Role: String, Equatable, Sendable {
        case user
        case assistant
    }

    public var id: UUID
    public var role: Role
    public var text: String

    public init(id: UUID = UUID(), role: Role, text: String) {
        self.id = id
        self.role = role
        self.text = text
    }
}
