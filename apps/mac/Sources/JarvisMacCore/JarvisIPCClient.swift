import Foundation

public struct JarvisEndpoint: Equatable, Sendable {
    public var baseURL: URL

    public init(baseURL: URL = URL(string: "http://127.0.0.1:7787")!) {
        self.baseURL = baseURL
    }

    public func url(path: String) -> URL {
        baseURL.appending(path: path.trimmingCharacters(in: CharacterSet(charactersIn: "/")))
    }
}

public struct JarvisHealth: Decodable, Equatable, Sendable {
    public var status: String
    public var version: String
    public var emergencyPaused: Bool
    public var emergencyPauseReason: String?
    public var schedulerJobs: Int
    public var commandRuntime: String

    enum CodingKeys: String, CodingKey {
        case status
        case version
        case emergencyPaused = "emergency_paused"
        case emergencyPauseReason = "emergency_pause_reason"
        case schedulerJobs = "scheduler_jobs"
        case commandRuntime = "command_runtime"
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

public struct JarvisTask: Decodable, Equatable, Sendable {
    public var id: UUID
    public var sessionId: UUID
    public var userInput: String
    public var status: String

    enum CodingKeys: String, CodingKey {
        case id
        case sessionId = "session_id"
        case userInput = "user_input"
        case status
    }
}

public struct JarvisAuditEntry: Decodable, Equatable, Identifiable, Sendable {
    public var id: UUID
    public var taskId: UUID?
    public var eventType: String
    public var summary: String
    public var createdAt: String

    enum CodingKeys: String, CodingKey {
        case id
        case taskId = "task_id"
        case eventType = "event_type"
        case summary
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
    public var riskTier: String
    public var approvalStatus: String

    enum CodingKeys: String, CodingKey {
        case pluginId = "plugin_id"
        case action
        case riskTier = "risk_tier"
        case approvalStatus = "approval_status"
    }
}

public struct JarvisPluginCallResult: Decodable, Equatable, Sendable {
    public var status: String
    public var metadata: JarvisPluginCallMetadata
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

public struct JarvisPauseRequest: Encodable, Equatable, Sendable {
    public var reason: String

    public init(reason: String) {
        self.reason = reason
    }
}

public struct JarvisPauseResponse: Decodable, Equatable, Sendable {
    public var paused: Bool
    public var reason: String?
    public var cancelledSchedulerJobs: Int

    enum CodingKeys: String, CodingKey {
        case paused
        case reason
        case cancelledSchedulerJobs = "cancelled_scheduler_jobs"
    }
}

public enum JarvisIPCError: Error, Equatable {
    case invalidResponse
    case httpStatus(Int, String)
}

public final class JarvisIPCClient: Sendable {
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
