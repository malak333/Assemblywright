import Foundation

public struct JarvisSchedulerNotificationRequest: Equatable, Identifiable, Sendable {
    public let id: String
    public let schedulerJobId: UUID
    public let title: String
    public let body: String
    public let notificationKind: String
    public let threadIdentifier: String
    public let schedulerNotificationOccurrenceId: UUID?
    public let schedulerNotificationRevision: UInt64?

    public init(
        id: String,
        schedulerJobId: UUID,
        title: String,
        body: String,
        notificationKind: String,
        threadIdentifier: String,
        schedulerNotificationOccurrenceId: UUID? = nil,
        schedulerNotificationRevision: UInt64? = nil
    ) {
        self.id = id
        self.schedulerJobId = schedulerJobId
        self.title = title
        self.body = body
        self.notificationKind = notificationKind
        self.threadIdentifier = threadIdentifier
        self.schedulerNotificationOccurrenceId = schedulerNotificationOccurrenceId
        self.schedulerNotificationRevision = schedulerNotificationRevision
    }
}

public struct JarvisSchedulerNotificationAcknowledgement: Equatable, Sendable {
    public let id: UUID
    public let revision: UInt64
    public let disposition: JarvisSchedulerNotificationAcknowledgementDisposition

    public init(
        id: UUID,
        revision: UInt64,
        disposition: JarvisSchedulerNotificationAcknowledgementDisposition
    ) {
        self.id = id
        self.revision = revision
        self.disposition = disposition
    }
}

public protocol JarvisSchedulerNotificationAdapter: Sendable {
    func authorizationStatus() async -> JarvisSchedulerNotificationAuthorization
    func requestAuthorization() async throws -> Bool
    func deliver(_ request: JarvisSchedulerNotificationRequest) async throws
}

public enum JarvisSchedulerNotificationAuthorization: Equatable, Sendable {
    case notDetermined
    case authorized
    case denied
}

public enum JarvisSchedulerNotificationStatus: Equatable, Sendable {
    case notRequested
    case authorized
    case denied
    case delivered(Int)
    case failed(String)

    public var label: String {
        switch self {
        case .notRequested:
            return "Notifications not requested"
        case .authorized:
            return "Notifications authorized"
        case .denied:
            return "Notifications denied"
        case let .delivered(count):
            return "\(count) notification(s) delivered"
        case let .failed(reason):
            return "Notification delivery failed: \(reason)"
        }
    }
}

@MainActor
public final class SchedulerNotificationModel: ObservableObject {
    @Published public private(set) var status: JarvisSchedulerNotificationStatus
    @Published public private(set) var lastDeliveredRequests: [JarvisSchedulerNotificationRequest]
    @Published public private(set) var isWorking: Bool

    private let adapter: any JarvisSchedulerNotificationAdapter
    private var deliveredIds: Set<String>
    private var deliveredIdOrder: [String]
    private let deliveredHistoryLimit: Int

    public init(
        adapter: any JarvisSchedulerNotificationAdapter,
        deliveredHistoryLimit: Int = 256
    ) {
        self.adapter = adapter
        self.status = .notRequested
        self.lastDeliveredRequests = []
        self.isWorking = false
        self.deliveredIds = []
        self.deliveredIdOrder = []
        self.deliveredHistoryLimit = max(1, deliveredHistoryLimit)
    }

    @discardableResult
    public func requestAuthorization() async -> Bool {
        guard !isWorking else { return false }
        isWorking = true
        defer { isWorking = false }

        do {
            let authorized = try await adapter.requestAuthorization()
            status = authorized ? .authorized : .denied
            return authorized
        } catch {
            status = .failed(String(describing: error))
            return false
        }
    }

    @discardableResult
    public func notify(attention: JarvisSchedulerAttentionSummary) async -> Int {
        guard !isWorking else { return 0 }
        isWorking = true
        defer { isWorking = false }

        guard attention.attentionRequired else {
            status = .delivered(0)
            lastDeliveredRequests = []
            return 0
        }

        do {
            let authorized = try await adapter.requestAuthorization()
            guard authorized else {
                status = .denied
                lastDeliveredRequests = []
                return 0
            }

            return try await deliver(attention: attention)
        } catch {
            status = .failed(String(describing: error))
            lastDeliveredRequests = []
            return 0
        }
    }

    @discardableResult
    public func notifyIfAuthorized(
        attention: JarvisSchedulerAttentionSummary,
        shouldContinue: @escaping @MainActor () -> Bool = { true }
    ) async -> Int {
        guard attention.attentionRequired else { return 0 }
        guard !isWorking else { return 0 }
        guard shouldContinue(), !Task.isCancelled else { return 0 }
        isWorking = true
        defer { isWorking = false }

        switch await adapter.authorizationStatus() {
        case .authorized:
            guard shouldContinue(), !Task.isCancelled else { return 0 }
            do {
                return try await deliver(
                    attention: attention,
                    shouldContinue: shouldContinue
                )
            } catch {
                if shouldContinue(), !Task.isCancelled {
                    status = .failed(String(describing: error))
                    lastDeliveredRequests = []
                }
                return 0
            }
        case .denied:
            if shouldContinue(), !Task.isCancelled {
                status = .denied
            }
            return 0
        case .notDetermined:
            if shouldContinue(), !Task.isCancelled {
                status = .notRequested
            }
            return 0
        }
    }

    public func notifyPendingOccurrencesIfAuthorized(
        _ occurrences: [JarvisSchedulerNotificationOccurrence],
        shouldContinue: @escaping @MainActor () -> Bool = { true }
    ) async -> [JarvisSchedulerNotificationAcknowledgement] {
        guard !occurrences.isEmpty, !isWorking else { return [] }
        guard shouldContinue(), !Task.isCancelled else { return [] }
        isWorking = true
        defer { isWorking = false }

        let authorization = await adapter.authorizationStatus()
        guard shouldContinue(), !Task.isCancelled else { return [] }
        switch authorization {
        case .notDetermined:
            status = .notRequested
            lastDeliveredRequests = []
            return occurrences.map {
                JarvisSchedulerNotificationAcknowledgement(
                    id: $0.id,
                    revision: $0.revision,
                    disposition: .suppressedNotAuthorized
                )
            }
        case .denied:
            status = .denied
            lastDeliveredRequests = []
            return occurrences.map {
                JarvisSchedulerNotificationAcknowledgement(
                    id: $0.id,
                    revision: $0.revision,
                    disposition: .suppressedNotAuthorized
                )
            }
        case .authorized:
            break
        }

        var acknowledgements: [JarvisSchedulerNotificationAcknowledgement] = []
        var deliveredRequests: [JarvisSchedulerNotificationRequest] = []
        for occurrence in occurrences {
            guard shouldContinue(), !Task.isCancelled else { break }
            let request = notificationRequest(for: occurrence)
            do {
                try await adapter.deliver(request)
                deliveredRequests.append(request)
                acknowledgements.append(
                    JarvisSchedulerNotificationAcknowledgement(
                        id: occurrence.id,
                        revision: occurrence.revision,
                        disposition: .submittedToNotificationCenter
                    )
                )
            } catch {
                if shouldContinue(), !Task.isCancelled {
                    status = .failed(String(describing: error))
                    lastDeliveredRequests = deliveredRequests
                }
                return acknowledgements
            }
        }
        if shouldContinue(), !Task.isCancelled {
            lastDeliveredRequests = deliveredRequests
            status = .delivered(deliveredRequests.count)
        }
        return acknowledgements
    }

    public func resetDeliveredHistory() {
        deliveredIds = []
        deliveredIdOrder = []
        lastDeliveredRequests = []
        status = .authorized
    }

    public func notificationRequests(
        for attention: JarvisSchedulerAttentionSummary
    ) -> [JarvisSchedulerNotificationRequest] {
        attention.items
            .filter { item in
                item.notificationKind == "due_now"
                    || item.notificationKind == "failed"
                    || item.notificationKind == "blocked_by_emergency_pause"
            }
            .map { item in
                let occurrence = item.nextDueAt ?? "terminal"
                return JarvisSchedulerNotificationRequest(
                    id: "scheduler-\(item.id.uuidString)-\(item.notificationKind)-\(occurrence)",
                    schedulerJobId: item.id,
                    title: notificationTitle(for: item),
                    body: item.notificationReason,
                    notificationKind: item.notificationKind,
                    threadIdentifier: "jarvis.scheduler"
                )
            }
    }

    private func notificationTitle(for item: JarvisSchedulerAttentionItem) -> String {
        switch item.notificationKind {
        case "blocked_by_emergency_pause":
            return "Scheduler job blocked by pause: \(item.name)"
        case "failed":
            return "Scheduler job failed: \(item.name)"
        default:
            return "Scheduler job ready: \(item.name)"
        }
    }

    private func notificationRequest(
        for occurrence: JarvisSchedulerNotificationOccurrence
    ) -> JarvisSchedulerNotificationRequest {
        let title: String
        let body: String
        switch occurrence.notificationKind {
        case "blocked_by_emergency_pause":
            title = "Scheduler job blocked by pause: \(occurrence.name)"
            body = "A due scheduler job is waiting, but emergency pause is active."
        case "failed":
            title = "Scheduler job failed: \(occurrence.name)"
            body = "A scheduler job failed and needs review before stronger production claims."
        default:
            title = "Scheduler job due: \(occurrence.name)"
            body = "A scheduler job became due and entered the audited execution path."
        }
        return JarvisSchedulerNotificationRequest(
            id: "scheduler-occurrence-\(occurrence.id.uuidString)-r\(occurrence.revision)",
            schedulerJobId: occurrence.schedulerJobId,
            title: title,
            body: body,
            notificationKind: occurrence.notificationKind,
            threadIdentifier: "jarvis.scheduler",
            schedulerNotificationOccurrenceId: occurrence.id,
            schedulerNotificationRevision: occurrence.revision
        )
    }

    private func deliver(
        attention: JarvisSchedulerAttentionSummary,
        shouldContinue: @escaping @MainActor () -> Bool = { true }
    ) async throws -> Int {
        let requests = notificationRequests(for: attention)
            .filter { !deliveredIds.contains($0.id) }
        var deliveredCount = 0
        for request in requests {
            guard shouldContinue(), !Task.isCancelled else { return deliveredCount }
            try await adapter.deliver(request)
            recordDeliveredID(request.id)
            deliveredCount += 1
        }

        lastDeliveredRequests = requests
        status = .delivered(deliveredCount)
        return deliveredCount
    }


    private func recordDeliveredID(_ id: String) {
        guard deliveredIds.insert(id).inserted else { return }
        deliveredIdOrder.append(id)
        while deliveredIdOrder.count > deliveredHistoryLimit {
            deliveredIds.remove(deliveredIdOrder.removeFirst())
        }
    }
}
