import Foundation

@MainActor
public final class MemoryManagerModel: ObservableObject {
    @Published public private(set) var items: [JarvisMemoryItem]
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.items = []
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh(includeDeleted: Bool = false) async {
        await run {
            self.items = try await self.client.listMemoryItems(includeDeleted: includeDeleted)
        }
    }

    public func create(category: String, key: String, value: String, provenance: String, sensitivity: String) async {
        await run {
            let item = try await self.client.createMemoryItem(
                JarvisCreateMemoryItemRequest(
                    category: category,
                    key: key,
                    value: value,
                    provenance: provenance,
                    sensitivity: sensitivity
                )
            )
            self.items.insert(item, at: 0)
        }
    }

    public func review(id: UUID) async {
        await replace(id: id) {
            try await self.client.reviewMemoryItem(id: id)
        }
    }

    public func delete(id: UUID) async {
        await replace(id: id) {
            try await self.client.deleteMemoryItem(id: id)
        }
    }

    private func replace(id: UUID, operation: @escaping () async throws -> JarvisMemoryItem) async {
        await run {
            let item = try await operation()
            if let index = self.items.firstIndex(where: { $0.id == id }) {
                self.items[index] = item
            }
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) async {
        isLoading = true
        lastError = nil
        defer { isLoading = false }

        do {
            try await operation()
        } catch {
            lastError = String(describing: error)
        }
    }
}

@MainActor
public final class PluginManagerModel: ObservableObject {
    @Published public private(set) var manifests: [JarvisPluginManifest]
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.manifests = []
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh() async {
        isLoading = true
        lastError = nil
        defer { isLoading = false }

        do {
            manifests = try await client.listPluginManifests()
        } catch {
            lastError = String(describing: error)
        }
    }
}

@MainActor
public final class SchedulerModel: ObservableObject {
    @Published public private(set) var jobs: [JarvisSchedulerJob]
    @Published public private(set) var selectedJob: JarvisSchedulerJob?
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.jobs = []
        self.selectedJob = nil
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh() async {
        await run {
            self.jobs = try await self.client.listSchedulerJobs()
        }
    }

    public func scheduleManual(name: String, command: String) async {
        await schedule(name: name, command: command, trigger: .manual)
    }

    public func scheduleOnce(name: String, command: String, runAt: String) async {
        await schedule(name: name, command: command, trigger: .onceAt(runAt: runAt))
    }

    public func scheduleInterval(name: String, command: String, everySeconds: UInt64) async {
        await schedule(name: name, command: command, trigger: .interval(everySeconds: everySeconds))
    }

    public func select(id: UUID) async {
        await run {
            self.selectedJob = try await self.client.schedulerJob(id: id)
        }
    }

    public func schedule(name: String, command: String, trigger: JarvisSchedulerTrigger) async {
        await run {
            let job = try await self.client.createSchedulerJob(
                JarvisCreateSchedulerJobRequest(name: name, command: command, trigger: trigger)
            )
            self.selectedJob = job
            self.jobs.insert(job, at: 0)
        }
    }

    public func cancel(id: UUID) async {
        await run {
            let job = try await self.client.cancelSchedulerJob(id: id)
            if let index = self.jobs.firstIndex(where: { $0.id == id }) {
                self.jobs[index] = job
            }
            if self.selectedJob?.id == id {
                self.selectedJob = job
            }
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) async {
        isLoading = true
        lastError = nil
        defer { isLoading = false }

        do {
            try await operation()
        } catch {
            lastError = String(describing: error)
        }
    }
}

@MainActor
public final class RunManagementModel: ObservableObject {
    @Published public private(set) var tasks: [JarvisTask]
    @Published public private(set) var auditEntries: [JarvisAuditEntry]
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.tasks = []
        self.auditEntries = []
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh() async {
        await run {
            async let tasks = self.client.listTasks()
            async let audit = self.client.listAuditEntries(taskId: nil)
            self.tasks = try await tasks
            self.auditEntries = try await audit
        }
    }

    public func refreshAudit(taskId: UUID?) async {
        await run {
            self.auditEntries = try await self.client.listAuditEntries(taskId: taskId)
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) async {
        isLoading = true
        lastError = nil
        defer { isLoading = false }

        do {
            try await operation()
        } catch {
            lastError = String(describing: error)
        }
    }
}

@MainActor
public final class ApprovalManagementModel: ObservableObject {
    @Published public private(set) var contract: JarvisContractResponse?
    @Published public private(set) var pendingItems: [JarvisApprovalQueueItem]
    @Published public private(set) var permissionSurface: JarvisPermissionSurfaceState
    @Published public private(set) var lastDecision: JarvisPendingApproval?
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    public var supportsApprovalActions: Bool {
        contract?.exposesApprovalActions == true
    }

    public var limitationText: String? {
        supportsApprovalActions
            ? nil
            : "Core exposes approval evidence, but no approval decision endpoint is available yet."
    }

    private let client: any JarvisCoreClient
    private var pluginManifests: [JarvisPluginManifest]

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.contract = nil
        self.pendingItems = []
        self.permissionSurface = .empty
        self.lastDecision = nil
        self.isLoading = false
        self.lastError = nil
        self.pluginManifests = []
    }

    public func refresh() async {
        await run {
            let loadedContract = try await self.client.contract()
            self.contract = loadedContract
            self.pluginManifests = try await self.client.listPluginManifests()

            if loadedContract.exposesApprovalList {
                let approvals = try await self.client.listApprovals(status: "pending")
                self.pendingItems = JarvisApprovalQueueItem.pendingItems(
                    approvals: approvals,
                    contract: loadedContract
                )
            } else {
                async let tasks = self.client.listTasks()
                async let audit = self.client.listAuditEntries(taskId: nil)
                let loadedTasks = try await tasks
                let loadedAudit = try await audit
                self.pendingItems = JarvisApprovalQueueItem.pendingItems(
                    tasks: loadedTasks,
                    auditEntries: loadedAudit,
                    contract: loadedContract
                )
            }

            self.permissionSurface = JarvisPermissionSurfaceState.current(
                pendingItems: self.pendingItems,
                pluginManifests: self.pluginManifests,
                contract: loadedContract
            )
        }
    }

    public func approve(id: UUID, reason: String?) async {
        await decide(id: id) {
            try await self.client.approveApproval(
                id: id,
                request: JarvisApprovalDecisionRequest(decidedBy: "mac-ui", reason: normalizedReason(reason))
            )
        }
    }

    public func deny(id: UUID, reason: String?) async {
        await decide(id: id) {
            try await self.client.denyApproval(
                id: id,
                request: JarvisApprovalDecisionRequest(decidedBy: "mac-ui", reason: normalizedReason(reason))
            )
        }
    }

    private func decide(
        id: UUID,
        operation: @escaping () async throws -> JarvisPendingApproval
    ) async {
        guard supportsApprovalActions else {
            lastError = "Core does not expose approval decision endpoints."
            return
        }

        await run {
            let decision = try await operation()
            self.lastDecision = decision
            self.pendingItems.removeAll { $0.id == id }
            self.permissionSurface = JarvisPermissionSurfaceState.current(
                pendingItems: self.pendingItems,
                pluginManifests: self.pluginManifests,
                contract: self.contract
            )
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) async {
        isLoading = true
        lastError = nil
        defer { isLoading = false }

        do {
            try await operation()
        } catch {
            lastError = String(describing: error)
        }
    }
}

private func normalizedReason(_ reason: String?) -> String? {
    let trimmed = reason?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return trimmed.isEmpty ? nil : trimmed
}

@MainActor
public final class DiagnosticsModel: ObservableObject {
    @Published public private(set) var export: JarvisDiagnosticsExport?
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.export = nil
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh() async {
        isLoading = true
        lastError = nil
        defer { isLoading = false }

        do {
            export = try await client.diagnosticsExport()
        } catch {
            lastError = String(describing: error)
        }
    }
}

public enum JarvisVoiceCaptureState: Equatable, Sendable {
    case textOnly(reason: String)
    case stagingTranscript(source: String)
    case interrupted(reason: String)
    case degraded(reason: String)
    case unavailable(reason: String)
}

public enum JarvisVoiceAction: Equatable, Sendable {
    case beginTranscript
    case updateTranscript(String)
    case submitTranscript
    case interruptTranscript(String)
    case resumeInterruptedTranscript
    case cancelTranscript
    case markDegraded(String)
    case markUnavailable(String)
    case resetTextOnly
}

public struct JarvisVoiceCommandHandoff: Equatable, Identifiable, Sendable {
    public var id: UUID
    public var text: String
    public var source: String
    public var dryRun: Bool

    public init(id: UUID = UUID(), text: String, source: String = "voice-transcript-scaffold", dryRun: Bool = true) {
        self.id = id
        self.text = text
        self.source = source
        self.dryRun = dryRun
    }
}

@MainActor
public final class VoiceStateModel: ObservableObject {
    public static let textOnlyReason = "Speech recognition is not implemented in this Swift shell yet."

    @Published public private(set) var captureState: JarvisVoiceCaptureState
    @Published public private(set) var transcriptDraft: String
    @Published public private(set) var lastHandoff: JarvisVoiceCommandHandoff?
    @Published public private(set) var lastError: String?
    @Published public private(set) var actionHistory: [JarvisVoiceAction]
    @Published public private(set) var isPushToTalkEnabled: Bool

    public init(
        captureState: JarvisVoiceCaptureState = .textOnly(reason: VoiceStateModel.textOnlyReason)
    ) {
        self.captureState = captureState
        self.transcriptDraft = ""
        self.lastHandoff = nil
        self.lastError = nil
        self.actionHistory = []
        self.isPushToTalkEnabled = false
    }

    public var statusText: String {
        switch captureState {
        case let .textOnly(reason):
            return "Text-only voice scaffold: \(reason)"
        case let .stagingTranscript(source):
            return "Voice transcript staging: \(source)"
        case let .interrupted(reason):
            return "Voice transcript interrupted: \(reason)"
        case let .degraded(reason):
            return "Voice degraded to typed transcript fallback: \(reason)"
        case let .unavailable(reason):
            return "Voice unavailable: \(reason)"
        }
    }

    @discardableResult
    public func apply(_ action: JarvisVoiceAction) -> JarvisVoiceCommandHandoff? {
        actionHistory.append(action)
        lastError = nil

        switch action {
        case .beginTranscript:
            guard !isUnavailable else {
                lastError = "Voice input is unavailable; reset to text-only before staging a transcript."
                return nil
            }
            transcriptDraft = ""
            lastHandoff = nil
            captureState = .stagingTranscript(source: "typed transcript parity path")
            return nil
        case let .updateTranscript(text):
            guard !isUnavailable else {
                lastError = "Voice input is unavailable; transcript update ignored."
                return nil
            }
            transcriptDraft = text
            if case .stagingTranscript = captureState {
                return nil
            }
            captureState = .stagingTranscript(source: "typed transcript parity path")
            return nil
        case .submitTranscript:
            guard !isUnavailable else {
                lastError = "Voice input is unavailable; transcript cannot be submitted."
                return nil
            }
            guard !isInterrupted else {
                lastError = "Voice transcript is interrupted; resume or cancel before submitting."
                return nil
            }
            let trimmed = transcriptDraft.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else {
                lastError = "Transcript is empty."
                return nil
            }
            let handoff = JarvisVoiceCommandHandoff(text: trimmed)
            lastHandoff = handoff
            transcriptDraft = ""
            captureState = .textOnly(reason: Self.textOnlyReason)
            return handoff
        case let .interruptTranscript(reason):
            guard case .stagingTranscript = captureState else {
                lastError = "No active transcript is available to interrupt."
                return nil
            }
            lastHandoff = nil
            captureState = .interrupted(reason: reason)
            isPushToTalkEnabled = false
            return nil
        case .resumeInterruptedTranscript:
            guard isInterrupted else {
                lastError = "No interrupted transcript is available to resume."
                return nil
            }
            captureState = .stagingTranscript(source: "typed transcript parity path")
            return nil
        case .cancelTranscript:
            transcriptDraft = ""
            lastHandoff = nil
            if !isUnavailable {
                captureState = .textOnly(reason: Self.textOnlyReason)
            }
            return nil
        case let .markDegraded(reason):
            transcriptDraft = ""
            lastHandoff = nil
            captureState = .degraded(reason: reason)
            isPushToTalkEnabled = false
            return nil
        case let .markUnavailable(reason):
            transcriptDraft = ""
            lastHandoff = nil
            captureState = .unavailable(reason: reason)
            isPushToTalkEnabled = false
            return nil
        case .resetTextOnly:
            transcriptDraft = ""
            lastHandoff = nil
            captureState = .textOnly(reason: Self.textOnlyReason)
            isPushToTalkEnabled = false
            return nil
        }
    }

    public func interruptTranscript(reason: String) {
        apply(.interruptTranscript(reason))
    }

    public func markDegraded(reason: String) {
        apply(.markDegraded(reason))
    }

    public func setUnavailable(reason: String) {
        apply(.markUnavailable(reason))
    }

    public func resetTextOnly() {
        apply(.resetTextOnly)
    }

    private var isUnavailable: Bool {
        if case .unavailable = captureState {
            return true
        }
        return false
    }

    private var isInterrupted: Bool {
        if case .interrupted = captureState {
            return true
        }
        return false
    }
}
