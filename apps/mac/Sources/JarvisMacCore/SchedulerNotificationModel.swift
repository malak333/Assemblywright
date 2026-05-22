import Foundation

public struct JarvisSchedulerNotificationRequest: Equatable, Identifiable, Sendable {
    public let id: String
    public let schedulerJobId: UUID
    public let title: String
    public let body: String
    public let notificationKind: String
    public let threadIdentifier: String

    public init(
        id: String,
        schedulerJobId: UUID,
        title: String,
        body: String,
        notificationKind: String,
        threadIdentifier: String
    ) {
        self.id = id
        self.schedulerJobId = schedulerJobId
        self.title = title
        self.body = body
        self.notificationKind = notificationKind
        self.threadIdentifier = threadIdentifier
    }
}

public protocol JarvisSchedulerNotificationAdapter: Sendable {
    func requestAuthorization() async throws -> Bool
    func deliver(_ request: JarvisSchedulerNotificationRequest) async throws
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

    public init(adapter: any JarvisSchedulerNotificationAdapter) {
        self.adapter = adapter
        self.status = .notRequested
        self.lastDeliveredRequests = []
        self.isWorking = false
        self.deliveredIds = []
    }

    @discardableResult
    public func requestAuthorization() async -> Bool {
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

            let requests = notificationRequests(for: attention)
                .filter { !deliveredIds.contains($0.id) }
            for request in requests {
                try await adapter.deliver(request)
                deliveredIds.insert(request.id)
            }

            lastDeliveredRequests = requests
            status = .delivered(requests.count)
            return requests.count
        } catch {
            status = .failed(String(describing: error))
            lastDeliveredRequests = []
            return 0
        }
    }

    public func resetDeliveredHistory() {
        deliveredIds = []
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
                JarvisSchedulerNotificationRequest(
                    id: "scheduler-\(item.id.uuidString)-\(item.notificationKind)",
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
}
