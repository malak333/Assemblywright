import Foundation

public struct JarvisEndpoint: Equatable, Sendable {
    public var baseURL: URL

    public init(baseURL: URL = URL(string: "http://127.0.0.1:7787")!) {
        self.baseURL = baseURL
    }

    public func url(path: String) -> URL {
        let trimmed = path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        guard let separator = trimmed.firstIndex(of: "?") else {
            return baseURL.appending(path: trimmed)
        }

        var components = URLComponents(
            url: baseURL.appending(path: String(trimmed[..<separator])),
            resolvingAgainstBaseURL: false
        )!
        components.percentEncodedQuery = String(trimmed[trimmed.index(after: separator)...])
        return components.url!
    }
}

public struct JarvisHealth: Decodable, Equatable, Sendable {
    public var status: String
    public var version: String
    public var contract: JarvisContractMetadata?
    public var emergencyPaused: Bool
    public var emergencyPauseReason: String?
    public var schedulerJobs: Int
    public var commandRuntime: String

    public init(
        status: String,
        version: String,
        contract: JarvisContractMetadata? = nil,
        emergencyPaused: Bool,
        emergencyPauseReason: String?,
        schedulerJobs: Int,
        commandRuntime: String
    ) {
        self.status = status
        self.version = version
        self.contract = contract
        self.emergencyPaused = emergencyPaused
        self.emergencyPauseReason = emergencyPauseReason
        self.schedulerJobs = schedulerJobs
        self.commandRuntime = commandRuntime
    }

    enum CodingKeys: String, CodingKey {
        case status
        case version
        case contract
        case emergencyPaused = "emergency_paused"
        case emergencyPauseReason = "emergency_pause_reason"
        case schedulerJobs = "scheduler_jobs"
        case commandRuntime = "command_runtime"
    }
}

public struct JarvisContractMetadata: Decodable, Equatable, Sendable {
    public var name: String
    public var version: Int
    public var coreVersion: String

    enum CodingKeys: String, CodingKey {
        case name
        case version
        case coreVersion = "core_version"
    }
}

public struct JarvisContractEndpoint: Decodable, Equatable, Identifiable, Sendable {
    public var method: String
    public var path: String
    public var repositoryRequired: Bool
    public var redacted: Bool

    public var id: String { "\(method) \(path)" }

    enum CodingKeys: String, CodingKey {
        case method
        case path
        case repositoryRequired = "repository_required"
        case redacted
    }
}

public struct JarvisContractResponse: Decodable, Equatable, Sendable {
    public var contract: JarvisContractMetadata
    public var endpoints: [JarvisContractEndpoint]
    public var safeInspectionPaths: [String]

    public var exposesApprovalActions: Bool {
        endpoints.contains { endpoint in
            endpoint.path.localizedCaseInsensitiveContains("approval")
        }
    }

    enum CodingKeys: String, CodingKey {
        case contract
        case endpoints
        case safeInspectionPaths = "safe_inspection_paths"
    }
}

public struct JarvisDiagnosticsExport: Decodable, Equatable, Sendable {
    public var generatedAt: String
    public var redaction: String
    public var health: JarvisHealth
    public var schedulerJobs: [JarvisDiagnosticSchedulerJob]
    public var repositoryBacked: Bool
    public var schemaVersion: Int?
    public var taskCount: Int?
    public var auditEntryCount: Int?
    public var activeMemoryItemCount: Int?

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case redaction
        case health
        case schedulerJobs = "scheduler_jobs"
        case repositoryBacked = "repository_backed"
        case schemaVersion = "schema_version"
        case taskCount = "task_count"
        case auditEntryCount = "audit_entry_count"
        case activeMemoryItemCount = "active_memory_item_count"
    }
}

public struct JarvisDiagnosticSchedulerJob: Decodable, Equatable, Identifiable, Sendable {
    public var id: UUID
    public var name: String
    public var trigger: JarvisSchedulerTrigger
    public var status: String
    public var createdAt: String
    public var updatedAt: String
    public var cancelledAt: String?
    public var cancellationReasonPresent: Bool

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case trigger
        case status
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case cancelledAt = "cancelled_at"
        case cancellationReasonPresent = "cancellation_reason_present"
    }
}

public struct JarvisCommandRequest: Encodable, Equatable, Sendable {
    public var input: String
    public var dryRun: Bool

    public init(input: String, dryRun: Bool = true) {
        self.input = input
        self.dryRun = dryRun
    }

    enum CodingKeys: String, CodingKey {
        case input
        case dryRun = "dry_run"
    }
}

public enum JarvisJSONValue: Codable, Equatable, Sendable {
    case object([String: JarvisJSONValue])
    case array([JarvisJSONValue])
    case string(String)
    case number(Double)
    case bool(Bool)
    case null

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let object = try? container.decode([String: JarvisJSONValue].self) {
            self = .object(object)
        } else if let array = try? container.decode([JarvisJSONValue].self) {
            self = .array(array)
        } else if let bool = try? container.decode(Bool.self) {
            self = .bool(bool)
        } else if let number = try? container.decode(Double.self) {
            self = .number(number)
        } else {
            self = .string(try container.decode(String.self))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case let .object(object):
            try container.encode(object)
        case let .array(array):
            try container.encode(array)
        case let .string(string):
            try container.encode(string)
        case let .number(number):
            try container.encode(number)
        case let .bool(bool):
            try container.encode(bool)
        case .null:
            try container.encodeNil()
        }
    }
}

public struct JarvisTask: Decodable, Equatable, Identifiable, Sendable {
    public var id: UUID
    public var sessionId: UUID
    public var userInput: String
    public var status: String
    public var createdAt: String?
    public var updatedAt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case sessionId = "session_id"
        case userInput = "user_input"
        case status
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

public struct JarvisAuditEntry: Decodable, Equatable, Identifiable, Sendable {
    public var id: UUID
    public var taskId: UUID?
    public var eventType: String
    public var summary: String
    public var payload: JarvisJSONValue?
    public var createdAt: String

    enum CodingKeys: String, CodingKey {
        case id
        case taskId = "task_id"
        case eventType = "event_type"
        case summary
        case payload
        case createdAt = "created_at"
    }
}

public struct JarvisRuntimeStep: Decodable, Equatable, Sendable {
    public var index: Int
    public var message: String
    public var complete: Bool
}

public struct JarvisModelRoute: Decodable, Equatable, Sendable {
    public var provider: String
    public var model: String
    public var reason: String
}

public struct JarvisPluginCallMetadata: Decodable, Equatable, Sendable {
    public var pluginId: String
    public var action: String
    public var permissions: [String]
    public var riskTier: String
    public var approvalRequired: Bool
    public var approvalStatus: String
    public var proactive: Bool
    public var memoryAccess: String
    public var modelAccess: String
    public var timeoutMilliseconds: Int
    public var cancellation: String
    public var auditFields: [String]

    enum CodingKeys: String, CodingKey {
        case pluginId = "plugin_id"
        case action
        case permissions
        case riskTier = "risk_tier"
        case approvalRequired = "approval_required"
        case approvalStatus = "approval_status"
        case proactive
        case memoryAccess = "memory_access"
        case modelAccess = "model_access"
        case timeoutMilliseconds = "timeout_ms"
        case cancellation
        case auditFields = "audit_fields"
    }
}

public struct JarvisPluginCallResult: Decodable, Equatable, Sendable {
    public var status: String
    public var output: JarvisJSONValue?
    public var metadata: JarvisPluginCallMetadata
}

public struct JarvisMemoryItem: Codable, Equatable, Identifiable, Sendable {
    public var id: UUID
    public var category: String
    public var key: String
    public var value: String
    public var provenance: String
    public var sensitivity: String
    public var createdAt: String
    public var updatedAt: String
    public var reviewedAt: String?
    public var deletedAt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case category
        case key
        case value
        case provenance
        case sensitivity
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case reviewedAt = "reviewed_at"
        case deletedAt = "deleted_at"
    }
}

public struct JarvisMemoryMutationRequest: Encodable, Equatable, Sendable {
    public var value: String
    public var provenance: String
    public var sensitivity: String

    public init(value: String, provenance: String, sensitivity: String) {
        self.value = value
        self.provenance = provenance
        self.sensitivity = sensitivity
    }
}

public struct JarvisCreateMemoryItemRequest: Encodable, Equatable, Sendable {
    public var category: String
    public var key: String
    public var value: String
    public var provenance: String
    public var sensitivity: String

    public init(category: String, key: String, value: String, provenance: String, sensitivity: String) {
        self.category = category
        self.key = key
        self.value = value
        self.provenance = provenance
        self.sensitivity = sensitivity
    }
}

public struct JarvisPluginManifest: Decodable, Equatable, Identifiable, Sendable {
    public var id: String
    public var name: String
    public var version: String
    public var source: String
    public var author: String
    public var actions: [JarvisPluginActionManifest]
}

public struct JarvisPluginActionManifest: Decodable, Equatable, Sendable {
    public var name: String
    public var description: String
    public var permissions: [String]
    public var riskTier: String
    public var inputSchema: JarvisJSONValue
    public var outputSchema: JarvisJSONValue
    public var proactive: Bool
    public var memoryAccess: String
    public var modelAccess: String
    public var auditFields: [String]
    public var timeout: JarvisPluginTimeout
    public var cancellation: String

    enum CodingKeys: String, CodingKey {
        case name
        case description
        case permissions
        case riskTier = "risk_tier"
        case inputSchema = "input_schema"
        case outputSchema = "output_schema"
        case proactive
        case memoryAccess = "memory_access"
        case modelAccess = "model_access"
        case auditFields = "audit_fields"
        case timeout
        case cancellation
    }
}

public struct JarvisPluginTimeout: Decodable, Equatable, Sendable {
    public var timeoutMilliseconds: Int

    enum CodingKeys: String, CodingKey {
        case timeoutMilliseconds = "timeout_ms"
    }
}

public enum JarvisSchedulerTrigger: Codable, Equatable, Sendable {
    case manual
    case onceAt(runAt: String)
    case interval(everySeconds: UInt64)

    enum CodingKeys: String, CodingKey {
        case manual
        case onceAt = "once_at"
        case runAt = "run_at"
        case interval
        case everySeconds = "every_seconds"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let value = try? container.decode(String.self), value == CodingKeys.manual.rawValue {
            self = .manual
            return
        }

        let keyed = try decoder.container(keyedBy: CodingKeys.self)
        if let once = try? keyed.nestedContainer(keyedBy: CodingKeys.self, forKey: .onceAt) {
            self = .onceAt(runAt: try once.decode(String.self, forKey: .runAt))
        } else {
            let interval = try keyed.nestedContainer(keyedBy: CodingKeys.self, forKey: .interval)
            self = .interval(everySeconds: try interval.decode(UInt64.self, forKey: .everySeconds))
        }
    }

    public func encode(to encoder: Encoder) throws {
        switch self {
        case .manual:
            var container = encoder.singleValueContainer()
            try container.encode(CodingKeys.manual.rawValue)
        case let .onceAt(runAt):
            var container = encoder.container(keyedBy: CodingKeys.self)
            var once = container.nestedContainer(keyedBy: CodingKeys.self, forKey: .onceAt)
            try once.encode(runAt, forKey: .runAt)
        case let .interval(everySeconds):
            var container = encoder.container(keyedBy: CodingKeys.self)
            var interval = container.nestedContainer(keyedBy: CodingKeys.self, forKey: .interval)
            try interval.encode(everySeconds, forKey: .everySeconds)
        }
    }
}

public struct JarvisSchedulerJob: Codable, Equatable, Identifiable, Sendable {
    public var id: UUID
    public var name: String
    public var command: String
    public var trigger: JarvisSchedulerTrigger
    public var status: String
    public var createdAt: String
    public var updatedAt: String
    public var cancelledAt: String?
    public var cancellationReason: String?

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case command
        case trigger
        case status
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case cancelledAt = "cancelled_at"
        case cancellationReason = "cancellation_reason"
    }
}

public struct JarvisCreateSchedulerJobRequest: Encodable, Equatable, Sendable {
    public var name: String
    public var command: String
    public var trigger: JarvisSchedulerTrigger

    public init(name: String, command: String, trigger: JarvisSchedulerTrigger) {
        self.name = name
        self.command = command
        self.trigger = trigger
    }
}

public struct JarvisCommandResponse: Decodable, Equatable, Sendable {
    public var accepted: Bool
    public var task: JarvisTask
    public var auditEntry: JarvisAuditEntry
    public var auditEntries: [JarvisAuditEntry]
    public var route: JarvisModelRoute?
    public var steps: [JarvisRuntimeStep]
    public var pluginResults: [JarvisPluginCallResult]
    public var message: String

    enum CodingKeys: String, CodingKey {
        case accepted
        case task
        case auditEntry = "audit_entry"
        case auditEntries = "audit_entries"
        case route
        case steps
        case pluginResults = "plugin_results"
        case message
    }
}

public struct JarvisApprovalQueueItem: Equatable, Identifiable, Sendable {
    public var id: UUID
    public var taskId: UUID?
    public var title: String
    public var detail: String
    public var source: String
    public var approvalStatus: String
    public var actionAvailable: Bool

    public init(
        id: UUID = UUID(),
        taskId: UUID?,
        title: String,
        detail: String,
        source: String,
        approvalStatus: String,
        actionAvailable: Bool
    ) {
        self.id = id
        self.taskId = taskId
        self.title = title
        self.detail = detail
        self.source = source
        self.approvalStatus = approvalStatus
        self.actionAvailable = actionAvailable
    }

    public static func pendingItems(
        tasks: [JarvisTask],
        auditEntries: [JarvisAuditEntry],
        contract: JarvisContractResponse?
    ) -> [JarvisApprovalQueueItem] {
        let supportsApprovalActions = contract?.exposesApprovalActions == true
        let taskItems = tasks
            .filter { $0.status == "waiting_for_approval" }
            .map { task in
                JarvisApprovalQueueItem(
                    id: task.id,
                    taskId: task.id,
                    title: "Task waiting for approval",
                    detail: task.userInput,
                    source: "task",
                    approvalStatus: "pending",
                    actionAvailable: supportsApprovalActions
                )
            }

        let auditItems = auditEntries.compactMap { entry -> JarvisApprovalQueueItem? in
            guard entry.eventType.localizedCaseInsensitiveContains("approval") else {
                return nil
            }

            return JarvisApprovalQueueItem(
                id: entry.id,
                taskId: entry.taskId,
                title: entry.eventType,
                detail: entry.summary,
                source: "audit",
                approvalStatus: approvalStatus(from: entry.payload),
                actionAvailable: supportsApprovalActions
            )
        }

        return taskItems + auditItems
    }

    private static func approvalStatus(from payload: JarvisJSONValue?) -> String {
        guard case let .object(object) = payload,
              case let .string(status) = object["approval_status"] else {
            return "pending"
        }
        return status
    }
}

public struct JarvisPauseRequest: Encodable, Equatable, Sendable {
    public var reason: String

    public init(reason: String) {
        self.reason = reason
    }
}

public struct JarvisPauseResponse: Decodable, Equatable, Sendable {
    public var paused: Bool
    public var reason: String?
    public var pausedAt: String?
    public var resumedAt: String?
    public var cancelledSchedulerJobs: Int

    enum CodingKeys: String, CodingKey {
        case paused
        case reason
        case pausedAt = "paused_at"
        case resumedAt = "resumed_at"
        case cancelledSchedulerJobs = "cancelled_scheduler_jobs"
    }
}

public enum JarvisIPCError: Error, Equatable {
    case invalidResponse
    case httpStatus(Int, String)
}

public protocol JarvisCoreClient: Sendable {
    func health() async throws -> JarvisHealth
    func contract() async throws -> JarvisContractResponse
    func submit(_ command: JarvisCommandRequest) async throws -> JarvisCommandResponse
    func pause(reason: String) async throws -> JarvisPauseResponse
    func resume() async throws -> JarvisPauseResponse
    func pauseStatus() async throws -> JarvisPauseResponse
    func listTasks() async throws -> [JarvisTask]
    func task(id: UUID) async throws -> JarvisTask
    func listAuditEntries(taskId: UUID?) async throws -> [JarvisAuditEntry]
    func listMemoryItems(includeDeleted: Bool) async throws -> [JarvisMemoryItem]
    func createMemoryItem(_ request: JarvisCreateMemoryItemRequest) async throws -> JarvisMemoryItem
    func memoryItem(id: UUID) async throws -> JarvisMemoryItem
    func updateMemoryItem(id: UUID, request: JarvisMemoryMutationRequest) async throws -> JarvisMemoryItem
    func reviewMemoryItem(id: UUID) async throws -> JarvisMemoryItem
    func deleteMemoryItem(id: UUID) async throws -> JarvisMemoryItem
    func listPluginManifests() async throws -> [JarvisPluginManifest]
    func listSchedulerJobs() async throws -> [JarvisSchedulerJob]
    func schedulerJob(id: UUID) async throws -> JarvisSchedulerJob
    func createSchedulerJob(_ request: JarvisCreateSchedulerJobRequest) async throws -> JarvisSchedulerJob
    func cancelSchedulerJob(id: UUID) async throws -> JarvisSchedulerJob
    func diagnosticsExport() async throws -> JarvisDiagnosticsExport
}

public final class JarvisIPCClient: JarvisCoreClient {
    private let endpoint: JarvisEndpoint
    private let session: URLSession
    private let encoder: JSONEncoder
    private let decoder: JSONDecoder

    public init(endpoint: JarvisEndpoint = JarvisEndpoint(), session: URLSession = .shared) {
        self.endpoint = endpoint
        self.session = session
        self.encoder = JSONEncoder()
        self.decoder = JSONDecoder()
    }

    public func health() async throws -> JarvisHealth {
        try await send(path: "/health", method: "GET", body: Optional<Data>.none)
    }

    public func contract() async throws -> JarvisContractResponse {
        try await send(path: "/contract", method: "GET", body: Optional<Data>.none)
    }

    public func submit(_ command: JarvisCommandRequest) async throws -> JarvisCommandResponse {
        try await send(path: "/commands", method: "POST", body: encoder.encode(command))
    }

    public func pause(reason: String) async throws -> JarvisPauseResponse {
        try await send(
            path: "/emergency-pause",
            method: "POST",
            body: encoder.encode(JarvisPauseRequest(reason: reason))
        )
    }

    public func resume() async throws -> JarvisPauseResponse {
        try await send(path: "/emergency-pause", method: "DELETE", body: Optional<Data>.none)
    }

    public func pauseStatus() async throws -> JarvisPauseResponse {
        try await send(path: "/emergency-pause", method: "GET", body: Optional<Data>.none)
    }

    public func listTasks() async throws -> [JarvisTask] {
        try await send(path: "/tasks", method: "GET", body: Optional<Data>.none)
    }

    public func task(id: UUID) async throws -> JarvisTask {
        try await send(path: "/tasks/\(id.uuidString)", method: "GET", body: Optional<Data>.none)
    }

    public func listAuditEntries(taskId: UUID? = nil) async throws -> [JarvisAuditEntry] {
        let path = taskId.map { "/tasks/\($0.uuidString)/audit" } ?? "/audit"
        return try await send(path: path, method: "GET", body: Optional<Data>.none)
    }

    public func listMemoryItems(includeDeleted: Bool = false) async throws -> [JarvisMemoryItem] {
        let path = includeDeleted ? "/memory?include_deleted=true" : "/memory"
        return try await send(path: path, method: "GET", body: Optional<Data>.none)
    }

    public func createMemoryItem(_ request: JarvisCreateMemoryItemRequest) async throws -> JarvisMemoryItem {
        try await send(path: "/memory", method: "POST", body: encoder.encode(request))
    }

    public func memoryItem(id: UUID) async throws -> JarvisMemoryItem {
        try await send(path: "/memory/\(id.uuidString)", method: "GET", body: Optional<Data>.none)
    }

    public func updateMemoryItem(id: UUID, request: JarvisMemoryMutationRequest) async throws -> JarvisMemoryItem {
        try await send(path: "/memory/\(id.uuidString)", method: "PATCH", body: encoder.encode(request))
    }

    public func reviewMemoryItem(id: UUID) async throws -> JarvisMemoryItem {
        try await send(path: "/memory/\(id.uuidString)/review", method: "POST", body: Optional<Data>.none)
    }

    public func deleteMemoryItem(id: UUID) async throws -> JarvisMemoryItem {
        try await send(path: "/memory/\(id.uuidString)", method: "DELETE", body: Optional<Data>.none)
    }

    public func listPluginManifests() async throws -> [JarvisPluginManifest] {
        try await send(path: "/plugins/manifests", method: "GET", body: Optional<Data>.none)
    }

    public func listSchedulerJobs() async throws -> [JarvisSchedulerJob] {
        try await send(path: "/scheduler/jobs", method: "GET", body: Optional<Data>.none)
    }

    public func schedulerJob(id: UUID) async throws -> JarvisSchedulerJob {
        try await send(path: "/scheduler/jobs/\(id.uuidString)", method: "GET", body: Optional<Data>.none)
    }

    public func createSchedulerJob(_ request: JarvisCreateSchedulerJobRequest) async throws -> JarvisSchedulerJob {
        try await send(path: "/scheduler/jobs", method: "POST", body: encoder.encode(request))
    }

    public func cancelSchedulerJob(id: UUID) async throws -> JarvisSchedulerJob {
        try await send(path: "/scheduler/jobs/\(id.uuidString)", method: "DELETE", body: Optional<Data>.none)
    }

    public func diagnosticsExport() async throws -> JarvisDiagnosticsExport {
        try await send(path: "/diagnostics/export", method: "GET", body: Optional<Data>.none)
    }

    private func send<Response: Decodable>(
        path: String,
        method: String,
        body: Data?
    ) async throws -> Response {
        var request = URLRequest(url: endpoint.url(path: path))
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Accept")

        if let body {
            request.httpBody = body
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }

        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw JarvisIPCError.invalidResponse
        }

        guard (200..<300).contains(http.statusCode) else {
            throw JarvisIPCError.httpStatus(http.statusCode, String(decoding: data, as: UTF8.self))
        }

        return try decoder.decode(Response.self, from: data)
    }
}
