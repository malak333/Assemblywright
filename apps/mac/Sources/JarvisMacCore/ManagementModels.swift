import Foundation

@MainActor
public final class ReleaseReadinessModel: ObservableObject {
    @Published public private(set) var readiness: JarvisReleaseReadiness?
    @Published public private(set) var evidenceStatus: JarvisReleaseEvidenceStatus?
    @Published public private(set) var releaseRunbooks: [JarvisReleaseRunbook]
    @Published public private(set) var releaseRunbookWarning: String?
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.readiness = nil
        self.evidenceStatus = nil
        self.releaseRunbooks = []
        self.releaseRunbookWarning = nil
        self.isLoading = false
        self.lastError = nil
    }

    public var isShowingStaleReadiness: Bool {
        readiness != nil && lastError != nil
    }

    public var effectiveProductionReady: Bool {
        guard lastError == nil,
              readiness?.productionReady == true,
              evidenceStatus?.complete == true,
              evidenceStatus?.missingCount == 0,
              evidenceStatus?.invalidCount == 0 else {
            return false
        }

        return evidenceStatus?.items.allSatisfy { item in
            item.status == "present"
        } == true
    }

    public func refresh() async {
        isLoading = true
        lastError = nil
        releaseRunbookWarning = nil
        defer { isLoading = false }

        do {
            async let readiness = client.releaseReadiness()
            async let evidenceStatus = client.releaseEvidenceStatus()

            self.readiness = try await readiness
            self.evidenceStatus = try await evidenceStatus

            do {
                async let signedDistributionRunbook = client.releaseSignedDistributionRunbook()
                async let liveDeviceRunbook = client.releaseLiveDeviceRunbook()
                async let pluginTrustRunbook = client.releasePluginTrustRunbook()
                async let evidenceBundleRunbook = client.releaseEvidenceBundleRunbook()
                self.releaseRunbooks = try await [
                    signedDistributionRunbook,
                    liveDeviceRunbook,
                    pluginTrustRunbook,
                    evidenceBundleRunbook,
                ]
            } catch {
                self.releaseRunbooks = []
                self.releaseRunbookWarning = "Release runbooks could not be loaded: \(String(describing: error))"
            }
        } catch {
            lastError = String(describing: error)
        }
    }
}

@MainActor
public final class MemoryManagerModel: ObservableObject {
    @Published public private(set) var items: [JarvisMemoryItem]
    @Published public private(set) var classification: JarvisMemoryClassificationSummary?
    @Published public private(set) var retentionPlan: JarvisMemoryRetentionPlan?
    @Published public private(set) var selectedItem: JarvisMemoryItem?
    @Published public private(set) var includeDeleted: Bool
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.items = []
        self.classification = nil
        self.retentionPlan = nil
        self.selectedItem = nil
        self.includeDeleted = false
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh(includeDeleted: Bool = false) async {
        self.includeDeleted = includeDeleted
        await run {
            async let items = self.client.listMemoryItems(includeDeleted: includeDeleted)
            async let classification = self.client.memoryClassification(includeDeleted: includeDeleted)
            async let retentionPlan = self.client.memoryRetentionPlan()
            self.items = try await items
            self.classification = try await classification
            self.retentionPlan = try await retentionPlan
            if let selectedItem = self.selectedItem,
               let refreshed = self.items.first(where: { $0.id == selectedItem.id }) {
                self.selectedItem = refreshed
            } else if !includeDeleted, self.selectedItem?.deletedAt != nil {
                self.selectedItem = nil
            }
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
            self.selectedItem = item
            try await self.refreshMemorySummaries()
        }
    }

    public func load(id: UUID) async {
        await replace(id: id) {
            try await self.client.memoryItem(id: id)
        }
    }

    public func update(id: UUID, value: String, provenance: String, sensitivity: String) async {
        await replace(id: id) {
            try await self.client.updateMemoryItem(
                id: id,
                request: JarvisMemoryMutationRequest(
                    value: value,
                    provenance: provenance,
                    sensitivity: sensitivity
                )
            )
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

    public func restore(id: UUID) async {
        await replace(id: id) {
            try await self.client.restoreMemoryItem(id: id)
        }
    }

    private func replace(id: UUID, operation: @escaping () async throws -> JarvisMemoryItem) async {
        await run {
            let item = try await operation()
            if let index = self.items.firstIndex(where: { $0.id == id }) {
                if item.deletedAt == nil || self.includeDeleted {
                    self.items[index] = item
                } else {
                    self.items.remove(at: index)
                }
            } else if item.deletedAt == nil || self.includeDeleted {
                self.items.insert(item, at: 0)
            }

            if item.deletedAt == nil || self.includeDeleted {
                self.selectedItem = item
            } else if self.selectedItem?.id == id {
                self.selectedItem = nil
            }
            try await self.refreshMemorySummaries()
        }
    }

    private func refreshMemorySummaries() async throws {
        async let classification = client.memoryClassification(includeDeleted: includeDeleted)
        async let retentionPlan = client.memoryRetentionPlan()
        self.classification = try await classification
        self.retentionPlan = try await retentionPlan
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
    @Published public private(set) var installedPlugins: [JarvisInstalledPluginRecord]
    @Published public private(set) var installedRegistryWarning: String?
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.manifests = []
        self.installedPlugins = []
        self.installedRegistryWarning = nil
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh() async {
        isLoading = true
        lastError = nil
        defer { isLoading = false }

        do {
            self.manifests = try await client.listPluginManifests()

            do {
                self.installedPlugins = try await client.listInstalledPlugins()
                self.installedRegistryWarning = nil
            } catch {
                self.installedPlugins = []
                self.installedRegistryWarning = "Installed plugin registry unavailable: \(error)"
            }
        } catch {
            lastError = String(describing: error)
        }
    }
}

@MainActor
public final class SchedulerModel: ObservableObject {
    @Published public private(set) var jobs: [JarvisSchedulerJob]
    @Published public private(set) var attention: JarvisSchedulerAttentionSummary?
    @Published public private(set) var selectedJob: JarvisSchedulerJob?
    @Published public private(set) var lastRunDue: JarvisSchedulerRunResponse?
    @Published public private(set) var lastStaleRecovery: JarvisSchedulerStaleRecoveryResponse?
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.jobs = []
        self.attention = nil
        self.selectedJob = nil
        self.lastRunDue = nil
        self.lastStaleRecovery = nil
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh() async {
        await run {
            async let jobs = self.client.listSchedulerJobs()
            async let attention = self.client.schedulerAttention()
            self.jobs = try await jobs
            self.attention = try await attention
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
            self.attention = try await self.client.schedulerAttention()
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
            self.attention = try await self.client.schedulerAttention()
        }
    }

    public func runDue(limit: Int) async {
        await run {
            self.lastRunDue = try await self.client.runDueSchedulerJobs(limit: limit)
            async let jobs = self.client.listSchedulerJobs()
            async let attention = self.client.schedulerAttention()
            self.jobs = try await jobs
            self.attention = try await attention
        }
    }

    public func recoverStale(olderThanSeconds: UInt64, limit: Int) async {
        await run {
            self.lastStaleRecovery = try await self.client.recoverStaleSchedulerJobs(
                olderThanSeconds: olderThanSeconds,
                limit: limit
            )
            async let jobs = self.client.listSchedulerJobs()
            async let attention = self.client.schedulerAttention()
            self.jobs = try await jobs
            self.attention = try await attention
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
    @Published public private(set) var activitySummary: JarvisActivitySummary?
    @Published public private(set) var activityEvents: [JarvisActivityEvent]
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    private let client: any JarvisCoreClient

    public init(client: any JarvisCoreClient = JarvisIPCClient()) {
        self.client = client
        self.tasks = []
        self.auditEntries = []
        self.activitySummary = nil
        self.activityEvents = []
        self.isLoading = false
        self.lastError = nil
    }

    public func refresh() async {
        await run {
            async let tasks = self.client.listTasks()
            async let audit = self.client.listAuditEntries(taskId: nil)
            async let activitySummary = self.client.activitySummary()
            self.tasks = try await tasks
            self.auditEntries = try await audit
            self.activitySummary = try await activitySummary
        }
    }

    public func refreshAudit(taskId: UUID?) async {
        await run {
            self.auditEntries = try await self.client.listAuditEntries(taskId: taskId)
        }
    }

    public func watchActivity(maxEvents: Int = 2, intervalMilliseconds: Int = 500) async {
        await run {
            let events = try await self.client.activityEvents(
                maxEvents: maxEvents,
                intervalMilliseconds: intervalMilliseconds
            )
            self.activityEvents = events
            if let latest = events.compactMap(\.summary).last {
                self.activitySummary = latest
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
public final class ApprovalManagementModel: ObservableObject {
    @Published public private(set) var contract: JarvisContractResponse?
    @Published public private(set) var pendingItems: [JarvisApprovalQueueItem]
    @Published public private(set) var grantSummary: JarvisPermissionGrantSummary?
    @Published public private(set) var policyReview: JarvisPermissionPolicyReview?
    @Published public private(set) var permissionSurface: JarvisPermissionSurfaceState
    @Published public private(set) var lastDecision: JarvisPendingApproval?
    @Published public private(set) var lastExecution: JarvisApprovalExecutionResponse?
    @Published public private(set) var isLoading: Bool
    @Published public private(set) var lastError: String?

    public var supportsApprovalActions: Bool {
        contract?.exposesApprovalActions == true
    }

    public var supportsApprovalExecution: Bool {
        contract?.exposesApprovalExecuteAction == true
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
        self.grantSummary = nil
        self.policyReview = nil
        self.permissionSurface = .empty
        self.lastDecision = nil
        self.lastExecution = nil
        self.isLoading = false
        self.lastError = nil
        self.pluginManifests = []
    }

    public func refresh() async {
        await run {
            let loadedContract = try await self.client.contract()
            self.contract = loadedContract
            self.pluginManifests = try await self.client.listPluginManifests()
            self.grantSummary = loadedContract.exposesPermissionGrantSummary
                ? try await self.client.permissionGrantSummary()
                : nil
            self.policyReview = loadedContract.exposesPermissionPolicyReview
                ? try await self.client.permissionPolicyReview()
                : nil

            if loadedContract.exposesApprovalList {
                self.pendingItems = try await self.loadApprovalItems(contract: loadedContract)
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
                contract: loadedContract,
                grantSummary: self.grantSummary
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

    public func execute(id: UUID) async {
        guard supportsApprovalExecution else {
            lastError = "Core does not expose approved approval execution."
            return
        }

        await run {
            let execution = try await self.client.executeApproval(id: id)
            self.lastExecution = execution
            self.pendingItems.removeAll { $0.id == id }
            if self.contract?.exposesPermissionGrantSummary == true {
                self.grantSummary = try await self.client.permissionGrantSummary()
            }
            if self.contract?.exposesPermissionPolicyReview == true {
                self.policyReview = try await self.client.permissionPolicyReview()
            }
            self.permissionSurface = JarvisPermissionSurfaceState.current(
                pendingItems: self.pendingItems,
                pluginManifests: self.pluginManifests,
                contract: self.contract,
                grantSummary: self.grantSummary
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
            if let contract = self.contract, contract.exposesApprovalList {
                self.pendingItems = try await self.loadApprovalItems(contract: contract)
            } else {
                self.pendingItems.removeAll { $0.id == id }
            }
            if self.contract?.exposesPermissionGrantSummary == true {
                self.grantSummary = try await self.client.permissionGrantSummary()
            }
            if self.contract?.exposesPermissionPolicyReview == true {
                self.policyReview = try await self.client.permissionPolicyReview()
            }
            self.permissionSurface = JarvisPermissionSurfaceState.current(
                pendingItems: self.pendingItems,
                pluginManifests: self.pluginManifests,
                contract: self.contract,
                grantSummary: self.grantSummary
            )
        }
    }

    private func loadApprovalItems(contract: JarvisContractResponse) async throws -> [JarvisApprovalQueueItem] {
        let approvals = try await client.listApprovals(status: nil)
        var visibleApprovals: [JarvisPendingApproval] = []
        for approval in approvals
        where approval.status == "pending"
            || (approval.status == "approved" && contract.exposesApprovalExecuteAction)
        {
            if approval.status == "approved",
               contract.exposesApprovalExecuteAction,
               try await approvalHasExecutionAudit(approval) {
                continue
            }
            visibleApprovals.append(approval)
        }
        return JarvisApprovalQueueItem.pendingItems(
            approvals: visibleApprovals,
            contract: contract
        )
    }

    private func approvalHasExecutionAudit(_ approval: JarvisPendingApproval) async throws -> Bool {
        let entries = try await client.listAuditEntries(taskId: approval.taskId)
        return entries.contains { entry in
            guard entry.eventType == "approval_executed",
                  case let .object(payload)? = entry.payload,
                  case let .string(approvalId)? = payload["approval_id"] else {
                return false
            }
            return approvalId.caseInsensitiveCompare(approval.id.uuidString) == .orderedSame
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
    public static let textOnlyReason = "Voice capture is idle; typed transcript fallback is available."

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
            return submitTranscript()
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

    @discardableResult
    public func submitTranscript(source: String = "voice-transcript-scaffold") -> JarvisVoiceCommandHandoff? {
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
        let handoff = JarvisVoiceCommandHandoff(text: trimmed, source: source)
        lastHandoff = handoff
        transcriptDraft = ""
        captureState = .textOnly(reason: Self.textOnlyReason)
        return handoff
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
