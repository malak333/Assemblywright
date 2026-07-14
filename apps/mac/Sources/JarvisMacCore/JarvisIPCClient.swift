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
    public var localModelProvider: String
    public var localModel: String
    public var localEndpointConfigured: Bool
    public var chatgptEnabled: Bool
    public var chatgptAuthMode: String
    public var chatgptModel: String
    public var chatgptRequiresApproval: Bool

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        status = try container.decode(String.self, forKey: .status)
        version = try container.decode(String.self, forKey: .version)
        contract = try container.decodeIfPresent(JarvisContractMetadata.self, forKey: .contract)
        emergencyPaused = try container.decode(Bool.self, forKey: .emergencyPaused)
        emergencyPauseReason = try container.decodeIfPresent(String.self, forKey: .emergencyPauseReason)
        schedulerJobs = try container.decode(Int.self, forKey: .schedulerJobs)
        commandRuntime = try container.decode(String.self, forKey: .commandRuntime)
        localModelProvider = try container.decodeIfPresent(String.self, forKey: .localModelProvider) ?? "fake"
        localModel = try container.decodeIfPresent(String.self, forKey: .localModel) ?? "fake-local-model"
        localEndpointConfigured = try container.decodeIfPresent(Bool.self, forKey: .localEndpointConfigured) ?? false
        chatgptEnabled = try container.decodeIfPresent(Bool.self, forKey: .chatgptEnabled) ?? false
        chatgptAuthMode = try container.decodeIfPresent(String.self, forKey: .chatgptAuthMode) ?? "api_key"
        chatgptModel = try container.decodeIfPresent(String.self, forKey: .chatgptModel) ?? "chatgpt-disabled"
        chatgptRequiresApproval = try container.decodeIfPresent(Bool.self, forKey: .chatgptRequiresApproval) ?? true
    }

    public init(
        status: String,
        version: String,
        contract: JarvisContractMetadata? = nil,
        emergencyPaused: Bool,
        emergencyPauseReason: String?,
        schedulerJobs: Int,
        commandRuntime: String,
        localModelProvider: String = "fake",
        localModel: String = "fake-local-model",
        localEndpointConfigured: Bool = false,
        chatgptEnabled: Bool = false,
        chatgptAuthMode: String = "api_key",
        chatgptModel: String = "chatgpt-disabled",
        chatgptRequiresApproval: Bool = true
    ) {
        self.status = status
        self.version = version
        self.contract = contract
        self.emergencyPaused = emergencyPaused
        self.emergencyPauseReason = emergencyPauseReason
        self.schedulerJobs = schedulerJobs
        self.commandRuntime = commandRuntime
        self.localModelProvider = localModelProvider
        self.localModel = localModel
        self.localEndpointConfigured = localEndpointConfigured
        self.chatgptEnabled = chatgptEnabled
        self.chatgptAuthMode = chatgptAuthMode
        self.chatgptModel = chatgptModel
        self.chatgptRequiresApproval = chatgptRequiresApproval
    }

    enum CodingKeys: String, CodingKey {
        case status
        case version
        case contract
        case emergencyPaused = "emergency_paused"
        case emergencyPauseReason = "emergency_pause_reason"
        case schedulerJobs = "scheduler_jobs"
        case commandRuntime = "command_runtime"
        case localModelProvider = "local_model_provider"
        case localModel = "local_model"
        case localEndpointConfigured = "local_endpoint_configured"
        case chatgptEnabled = "chatgpt_enabled"
        case chatgptAuthMode = "chatgpt_auth_mode"
        case chatgptModel = "chatgpt_model"
        case chatgptRequiresApproval = "chatgpt_requires_approval"
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

public struct JarvisContractFeature: Decodable, Equatable, Identifiable, Sendable {
    public var key: String
    public var status: String
    public var proof: String
    public var boundary: String

    public var id: String { key }
}

public struct JarvisContractCompatibility: Decodable, Equatable, Sendable {
    public var minimumSupportedVersion: Int
    public var currentVersion: Int
    public var additiveChangesAllowed: Bool
    public var breakingChangePolicy: String
    public var deprecationPolicy: String
    public var clientRequirements: [String]
    public var removedEndpoints: [String]
    public var deprecatedEndpoints: [String]

    public var supportsCurrentClient: Bool {
        minimumSupportedVersion <= currentVersion
    }

    enum CodingKeys: String, CodingKey {
        case minimumSupportedVersion = "minimum_supported_version"
        case currentVersion = "current_version"
        case additiveChangesAllowed = "additive_changes_allowed"
        case breakingChangePolicy = "breaking_change_policy"
        case deprecationPolicy = "deprecation_policy"
        case clientRequirements = "client_requirements"
        case removedEndpoints = "removed_endpoints"
        case deprecatedEndpoints = "deprecated_endpoints"
    }
}

public struct JarvisContractResponse: Decodable, Equatable, Sendable {
    public var contract: JarvisContractMetadata
    public var compatibility: JarvisContractCompatibility?
    public var endpoints: [JarvisContractEndpoint]
    public var safeInspectionPaths: [String]
    public var features: [JarvisContractFeature]

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        contract = try container.decode(JarvisContractMetadata.self, forKey: .contract)
        compatibility = try container.decodeIfPresent(
            JarvisContractCompatibility.self,
            forKey: .compatibility
        )
        endpoints = try container.decode([JarvisContractEndpoint].self, forKey: .endpoints)
        safeInspectionPaths = try container.decode([String].self, forKey: .safeInspectionPaths)
        features = try container.decodeIfPresent([JarvisContractFeature].self, forKey: .features) ?? []
    }

    public var exposesApprovalActions: Bool {
        exposesApprovalApproveAction || exposesApprovalDenyAction
    }

    public var exposesApprovalExecuteAction: Bool {
        endpoints.contains { endpoint in
            endpoint.method.uppercased() == "POST" && endpoint.path == "/approvals/:id/execute"
        }
    }

    public var exposesApprovalList: Bool {
        endpoints.contains { endpoint in
            endpoint.method.uppercased() == "GET" && endpoint.path == "/approvals"
        }
    }

    public var exposesPermissionGrantSummary: Bool {
        endpoints.contains { endpoint in
            endpoint.method.uppercased() == "GET" && endpoint.path == "/permissions/grants"
        }
    }

    public var exposesPermissionPolicyReview: Bool {
        endpoints.contains { endpoint in
            endpoint.method.uppercased() == "GET" && endpoint.path == "/permissions/policy-review"
        }
    }

    public var exposesMemoryRetentionPlan: Bool {
        endpoints.contains { endpoint in
            endpoint.method.uppercased() == "GET" && endpoint.path == "/memory/retention-plan"
        }
    }

    public var exposesReleaseReadiness: Bool {
        endpoints.contains { endpoint in
            endpoint.method.uppercased() == "GET" && endpoint.path == "/release/readiness"
        }
    }

    public var exposesReleaseEvidenceStatus: Bool {
        endpoints.contains { endpoint in
            endpoint.method.uppercased() == "GET" && endpoint.path == "/release/evidence-status"
        }
    }

    public var exposesReleaseRunbooks: Bool {
        let paths = Set(endpoints.filter { $0.method.uppercased() == "GET" }.map(\.path))
        return paths.contains("/release/live-device-runbook")
            && paths.contains("/release/signed-distribution-runbook")
            && paths.contains("/release/plugin-trust-runbook")
            && paths.contains("/release/evidence-bundle-runbook")
    }

    public var exposesApprovalApproveAction: Bool {
        endpoints.contains { endpoint in
            endpoint.method.uppercased() == "POST" && endpoint.path == "/approvals/:id/approve"
        }
    }

    public var exposesApprovalDenyAction: Bool {
        endpoints.contains { endpoint in
            endpoint.method.uppercased() == "POST" && endpoint.path == "/approvals/:id/deny"
        }
    }

    enum CodingKeys: String, CodingKey {
        case contract
        case compatibility
        case endpoints
        case safeInspectionPaths = "safe_inspection_paths"
        case features
    }
}

public struct JarvisReleaseReadiness: Decodable, Equatable, Sendable {
    public var generatedAt: String
    public var productionReady: Bool
    public var evidenceModeEnabled: Bool
    public var readinessScope: String
    public var verifiedFeatureCount: Int
    public var pendingFeatureCount: Int
    public var implementedFeatures: [JarvisReleaseReadinessFeature]
    public var pendingFeatures: [JarvisReleaseReadinessFeature]
    public var blockingManualGates: [String]
    public var recommendedVerificationCommands: [String]
    public var proofBoundary: String

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case productionReady = "production_ready"
        case evidenceModeEnabled = "evidence_mode_enabled"
        case readinessScope = "readiness_scope"
        case verifiedFeatureCount = "verified_feature_count"
        case pendingFeatureCount = "pending_feature_count"
        case implementedFeatures = "implemented_features"
        case pendingFeatures = "pending_features"
        case blockingManualGates = "blocking_manual_gates"
        case recommendedVerificationCommands = "recommended_verification_commands"
        case proofBoundary = "proof_boundary"
    }
}

public struct JarvisReleaseReadinessFeature: Decodable, Equatable, Identifiable, Sendable {
    public var key: String
    public var status: String
    public var proof: String
    public var boundary: String

    public var id: String { key }
}

public struct JarvisReleaseEvidenceStatus: Decodable, Equatable, Sendable {
    public var generatedAt: String
    public var complete: Bool
    public var satisfiedCount: Int
    public var missingCount: Int
    public var invalidCount: Int
    public var items: [JarvisReleaseEvidenceStatusItem]
    public var proofBoundary: String

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case complete
        case satisfiedCount = "satisfied_count"
        case missingCount = "missing_count"
        case invalidCount = "invalid_count"
        case items
        case proofBoundary = "proof_boundary"
    }
}

public struct JarvisReleaseEvidenceStatusItem: Decodable, Equatable, Identifiable, Sendable {
    public var key: String
    public var label: String
    public var path: String
    public var kind: String
    public var status: String
    public var requiredForProduction: Bool
    public var manualGate: Bool
    public var detail: String

    public var id: String { key }

    enum CodingKeys: String, CodingKey {
        case key
        case label
        case path
        case kind
        case status
        case requiredForProduction = "required_for_production"
        case manualGate = "manual_gate"
        case detail
    }
}

public struct JarvisReleaseRunbook: Decodable, Equatable, Identifiable, Sendable {
    public var generatedAt: String
    public var generatedFrom: String
    public var runbook: String
    public var productionReady: Bool
    public var liveVoiceFeature: JarvisReleaseReadinessFeature?
    public var evidenceItems: [JarvisReleaseEvidenceStatusItem]
    public var commands: [String]
    public var manualChecks: [String]
    public var proofBoundary: String

    public var id: String { runbook }

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case generatedFrom = "generated_from"
        case runbook
        case productionReady = "production_ready"
        case liveVoiceFeature = "live_voice_feature"
        case evidenceItems = "evidence_items"
        case commands
        case manualChecks = "manual_checks"
        case proofBoundary = "proof_boundary"
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
    public var unreviewedMemoryItemCount: Int?
    public var sensitiveMemoryItemCount: Int?

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
        case unreviewedMemoryItemCount = "unreviewed_memory_item_count"
        case sensitiveMemoryItemCount = "sensitive_memory_item_count"
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
    public var memoryContext: Bool

    public init(input: String, dryRun: Bool = true, memoryContext: Bool = false) {
        self.input = input
        self.dryRun = dryRun
        self.memoryContext = memoryContext
    }

    enum CodingKeys: String, CodingKey {
        case input
        case dryRun = "dry_run"
        case memoryContext = "memory_context"
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

public struct JarvisActivityStatusCount: Decodable, Equatable, Sendable {
    public var status: String
    public var count: Int
}

public struct JarvisActivityTaskSummary: Decodable, Equatable, Identifiable, Sendable {
    public var id: UUID
    public var sessionId: UUID
    public var status: String
    public var createdAt: String
    public var updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id
        case sessionId = "session_id"
        case status
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

public struct JarvisActivitySummary: Decodable, Equatable, Sendable {
    public var generatedAt: String
    public var repositoryBacked: Bool
    public var taskCount: Int
    public var auditEntryCount: Int
    public var activeTaskCount: Int
    public var statusCounts: [JarvisActivityStatusCount]
    public var recentTasks: [JarvisActivityTaskSummary]
    public var recentAuditEntries: [JarvisAuditEntry]

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case repositoryBacked = "repository_backed"
        case taskCount = "task_count"
        case auditEntryCount = "audit_entry_count"
        case activeTaskCount = "active_task_count"
        case statusCounts = "status_counts"
        case recentTasks = "recent_tasks"
        case recentAuditEntries = "recent_audit_entries"
    }
}

public struct JarvisActivityProgressEvent: Decodable, Equatable, Sendable {
    public var auditId: UUID
    public var taskId: UUID?
    public var createdAt: String
    public var kind: String?
    public var pluginId: String?
    public var action: String?
    public var provider: String?
    public var model: String?
    public var sessionId: UUID?
    public var sequence: Int?
    public var stage: String?
    public var message: String?
    public var byteCount: Int?
    public var charCount: Int?
    public var finalChunk: Bool?
    public var contentRedacted: Bool?
    public var providerNative: Bool?
    public var stderrRedacted: Bool

    enum CodingKeys: String, CodingKey {
        case auditId = "audit_id"
        case taskId = "task_id"
        case createdAt = "created_at"
        case kind
        case pluginId = "plugin_id"
        case action
        case provider
        case model
        case sessionId = "session_id"
        case sequence
        case stage
        case message
        case byteCount = "byte_count"
        case charCount = "char_count"
        case finalChunk = "final_chunk"
        case contentRedacted = "content_redacted"
        case providerNative = "provider_native"
        case stderrRedacted = "stderr_redacted"
    }
}

public struct JarvisActivityEvent: Equatable, Identifiable, Sendable {
    public var sequence: Int
    public var event: String
    public var summary: JarvisActivitySummary?
    public var progress: JarvisActivityProgressEvent?
    public var error: String?

    public var id: Int { sequence }

    public static func parseServerSentEvents(_ data: Data) throws -> [JarvisActivityEvent] {
        let stream = String(decoding: data, as: UTF8.self)
        let decoder = JSONDecoder()
        return try stream
            .components(separatedBy: "\n\n")
            .enumerated()
            .compactMap { index, block in
                let parsed = parseEventBlock(block)
                guard let event = parsed.event, let payload = parsed.data?.data(using: .utf8) else {
                    return nil
                }

                switch event {
                case "activity_summary":
                    return JarvisActivityEvent(
                        sequence: index,
                        event: event,
                        summary: try decoder.decode(JarvisActivitySummary.self, from: payload),
                        progress: nil,
                        error: nil
                    )
                case "activity_progress":
                    return JarvisActivityEvent(
                        sequence: index,
                        event: event,
                        summary: nil,
                        progress: try decoder.decode(JarvisActivityProgressEvent.self, from: payload),
                        error: nil
                    )
                case "activity_error":
                    _ = try decoder.decode(JarvisActivityEventError.self, from: payload)
                    return JarvisActivityEvent(
                        sequence: index,
                        event: event,
                        summary: nil,
                        progress: nil,
                        error: "Activity stream unavailable. Inspect the redacted audit timeline."
                    )
                default:
                    return JarvisActivityEvent(
                        sequence: index,
                        event: event,
                        summary: nil,
                        progress: nil,
                        error: "Unsupported activity event was discarded."
                    )
                }
            }
    }

    private static func parseEventBlock(_ block: String) -> (event: String?, data: String?) {
        var event: String?
        var dataLines: [String] = []

        for line in block.split(separator: "\n", omittingEmptySubsequences: false) {
            if line.hasPrefix("event:") {
                event = String(line.dropFirst("event:".count)).trimmingCharacters(in: .whitespaces)
            } else if line.hasPrefix("data:") {
                dataLines.append(String(line.dropFirst("data:".count)).trimmingCharacters(in: .whitespaces))
            }
        }

        return (event, dataLines.isEmpty ? nil : dataLines.joined(separator: "\n"))
    }
}

private struct JarvisActivityEventError: Decodable {
    var error: String
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

public struct JarvisMemoryClassificationSummary: Decodable, Equatable, Sendable {
    public var generatedAt: String
    public var includeDeleted: Bool
    public var totalCount: Int
    public var activeCount: Int
    public var deletedCount: Int
    public var reviewedCount: Int
    public var unreviewedActiveCount: Int
    public var sensitiveActiveCount: Int
    public var bySensitivity: [JarvisMemoryClassificationCount]
    public var byCategory: [JarvisMemoryClassificationCount]

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case includeDeleted = "include_deleted"
        case totalCount = "total_count"
        case activeCount = "active_count"
        case deletedCount = "deleted_count"
        case reviewedCount = "reviewed_count"
        case unreviewedActiveCount = "unreviewed_active_count"
        case sensitiveActiveCount = "sensitive_active_count"
        case bySensitivity = "by_sensitivity"
        case byCategory = "by_category"
    }
}

public struct JarvisMemoryClassificationCount: Decodable, Equatable, Identifiable, Sendable {
    public var label: String
    public var count: Int
    public var activeCount: Int
    public var deletedCount: Int
    public var unreviewedActiveCount: Int

    public var id: String { label }

    enum CodingKeys: String, CodingKey {
        case label
        case count
        case activeCount = "active_count"
        case deletedCount = "deleted_count"
        case unreviewedActiveCount = "unreviewed_active_count"
    }
}

public struct JarvisMemoryIndexStatus: Decodable, Equatable, Sendable {
    public var generatedAt: String
    public var state: String
    public var indexVersion: Int?
    public var rebuiltAt: String?
    public var activeRecordCount: Int
    public var indexedEntryCount: Int
    public var currentEntryCount: Int
    public var missingEntryCount: Int
    public var staleEntryCount: Int
    public var orphanedEntryCount: Int
    public var deletedProjectionCount: Int
    public var canonicalSource: String
    public var retrievalEnabled: Bool
    public var redaction: String
    public var detail: String

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case state
        case indexVersion = "index_version"
        case rebuiltAt = "rebuilt_at"
        case activeRecordCount = "active_record_count"
        case indexedEntryCount = "indexed_entry_count"
        case currentEntryCount = "current_entry_count"
        case missingEntryCount = "missing_entry_count"
        case staleEntryCount = "stale_entry_count"
        case orphanedEntryCount = "orphaned_entry_count"
        case deletedProjectionCount = "deleted_projection_count"
        case canonicalSource = "canonical_source"
        case retrievalEnabled = "retrieval_enabled"
        case redaction
        case detail
    }
}

public struct JarvisMemoryRetentionPlan: Decodable, Equatable, Sendable {
    public var generatedAt: String
    public var status: String
    public var candidateCount: Int
    public var unreviewedActiveCount: Int
    public var deletedSensitiveRetainedCount: Int
    public var nextRequiredAction: String
    public var automationEnabled: Bool
    public var valueRedactionRequired: Bool
    public var candidates: [JarvisMemoryRetentionCandidate]

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case status
        case candidateCount = "candidate_count"
        case unreviewedActiveCount = "unreviewed_active_count"
        case deletedSensitiveRetainedCount = "deleted_sensitive_retained_count"
        case nextRequiredAction = "next_required_action"
        case automationEnabled = "automation_enabled"
        case valueRedactionRequired = "value_redaction_required"
        case candidates
    }
}

public struct JarvisMemoryRetentionCandidate: Decodable, Equatable, Identifiable, Sendable {
    public var memoryId: UUID
    public var category: String
    public var key: String
    public var sensitivity: String
    public var status: String
    public var severity: String
    public var reason: String
    public var recommendedAction: String
    public var reviewedAt: String?
    public var deletedAt: String?

    public var id: UUID { memoryId }

    enum CodingKeys: String, CodingKey {
        case memoryId = "memory_id"
        case category
        case key
        case sensitivity
        case status
        case severity
        case reason
        case recommendedAction = "recommended_action"
        case reviewedAt = "reviewed_at"
        case deletedAt = "deleted_at"
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

public struct JarvisModelToolCatalog: Decodable, Equatable, Sendable {
    public var generatedAt: String
    public var source: String
    public var tools: [JarvisModelToolCatalogEntry]
    public var proofBoundary: String

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case source
        case tools
        case proofBoundary = "proof_boundary"
    }
}

/// A deliberately redacted operator view of a model-visible first-party tool.
/// Input schemas and execution metadata are decoded nowhere on this surface.
public struct JarvisModelToolCatalogEntry: Decodable, Equatable, Identifiable, Sendable {
    public var pluginId: String
    public var action: String
    public var description: String
    public var riskTier: String
    public var scopes: [String]
    public var proactive: Bool
    public var constraints: JarvisModelToolConstraints

    public var id: String { "\(pluginId).\(action)" }

    enum CodingKeys: String, CodingKey {
        case pluginId = "plugin_id"
        case action
        case description
        case riskTier = "risk_tier"
        case scopes
        case proactive
        case constraints
    }
}

public struct JarvisModelToolConstraints: Decodable, Equatable, Sendable {
    public var readOnly: Bool
    public var bounded: Bool
    public var noNetwork: Bool
    public var localModelOnly: Bool

    public var hasProductionReadOnlyBoundary: Bool {
        readOnly && bounded && noNetwork && localModelOnly
    }

    public var operatorBadge: String {
        hasProductionReadOnlyBoundary
            ? "bounded read-only • no network • local model only"
            : "review constraints"
    }

    enum CodingKeys: String, CodingKey {
        case readOnly = "read_only"
        case bounded
        case noNetwork = "no_network"
        case localModelOnly = "local_model_only"
    }
}

public struct JarvisInstalledPluginProvenance: Decodable, Equatable, Sendable {
    public var provenanceSchemaVersion: Int
    public var captureMethod: String
    public var manifestPath: String?
    public var manifestSha256: String?
    public var sourcePath: String?
    public var sourcePathCanonicalized: Bool
    public var subprocessCommandPath: String?
    public var subprocessCommandSha256: String?
    public var capturedAt: String
    public var lastVerifiedAt: String?
    public var integrityStatus: String
    public var originClaim: String?
    public var originClaimVerified: Bool

    public var needsReview: Bool {
        integrityStatus != "matches_install_snapshot" || !originClaimVerified
    }

    enum CodingKeys: String, CodingKey {
        case provenanceSchemaVersion = "provenance_schema_version"
        case captureMethod = "capture_method"
        case manifestPath = "manifest_path"
        case manifestSha256 = "manifest_sha256"
        case sourcePath = "source_path"
        case sourcePathCanonicalized = "source_path_canonicalized"
        case subprocessCommandPath = "subprocess_command_path"
        case subprocessCommandSha256 = "subprocess_command_sha256"
        case capturedAt = "captured_at"
        case lastVerifiedAt = "last_verified_at"
        case integrityStatus = "integrity_status"
        case originClaim = "origin_claim"
        case originClaimVerified = "origin_claim_verified"
    }
}

public struct JarvisInstalledPluginRecord: Decodable, Equatable, Identifiable, Sendable {
    public var id: String
    public var manifest: JarvisPluginManifest
    public var sourcePath: String?
    public var provenance: JarvisInstalledPluginProvenance
    public var executionEnabled: Bool
    public var executionGrant: String
    /// Redacted runtime metadata only. Older cores may omit these additive
    /// fields, so presentation must derive a conservative runtime label and
    /// treat every missing enforcement claim as false.
    public var runtimeKind: String?
    public var wasmConfinementEnforced: Bool?
    public var osSandboxEnforced: Bool?
    public var installedAt: String

    public var isExecutable: Bool {
        executionEnabled && executionGrant != "metadata_only" && provenance.integrityStatus == "matches_install_snapshot"
    }

    public var resolvedRuntimeKind: String {
        if let runtimeKind, !runtimeKind.isEmpty {
            return runtimeKind
        }
        switch manifest.source {
        case "local_wasm":
            return "wasm"
        case "local_subprocess":
            return "subprocess"
        default:
            return "metadata"
        }
    }

    public var confinementSummary: String {
        switch resolvedRuntimeKind {
        case "wasm":
            return wasmConfinementEnforced == true
                ? "WASM confined • no imports • no filesystem • no network"
                : "WASM confinement not verified"
        case "subprocess":
            return osSandboxEnforced == true
                ? "local subprocess • OS sandbox reported"
                : "local subprocess • not OS sandboxed"
        default:
            return "metadata only • not executable"
        }
    }

    public var hasEnforcedLanguageConfinement: Bool {
        resolvedRuntimeKind == "wasm" && wasmConfinementEnforced == true
    }

    enum CodingKeys: String, CodingKey {
        case id
        case manifest
        case sourcePath = "source_path"
        case provenance
        case executionEnabled = "execution_enabled"
        case executionGrant = "execution_grant"
        case runtimeKind = "runtime_kind"
        case wasmConfinementEnforced = "wasm_confinement_enforced"
        case osSandboxEnforced = "os_sandbox_enforced"
        case installedAt = "installed_at"
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

public struct JarvisSchedulerAttentionSummary: Decodable, Equatable, Sendable {
    public var generatedAt: String
    public var emergencyPaused: Bool
    public var attentionRequired: Bool
    public var dueCount: Int
    public var scheduledCount: Int
    public var runningCount: Int
    public var failedCount: Int
    public var nextDueAt: String?
    public var items: [JarvisSchedulerAttentionItem]

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case emergencyPaused = "emergency_paused"
        case attentionRequired = "attention_required"
        case dueCount = "due_count"
        case scheduledCount = "scheduled_count"
        case runningCount = "running_count"
        case failedCount = "failed_count"
        case nextDueAt = "next_due_at"
        case items
    }
}

public struct JarvisSchedulerAttentionItem: Decodable, Equatable, Identifiable, Sendable {
    public var id: UUID
    public var name: String
    public var trigger: JarvisSchedulerTrigger
    public var status: String
    public var due: Bool
    public var nextDueAt: String?
    public var notificationKind: String
    public var notificationReason: String

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case trigger
        case status
        case due
        case nextDueAt = "next_due_at"
        case notificationKind = "notification_kind"
        case notificationReason = "notification_reason"
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

public struct JarvisSchedulerRunResponse: Decodable, Equatable, Sendable {
    public var checkedAt: String
    public var limit: Int
    public var emergencyPaused: Bool
    public var executions: [JarvisSchedulerJobExecution]

    enum CodingKeys: String, CodingKey {
        case checkedAt = "checked_at"
        case limit
        case emergencyPaused = "emergency_paused"
        case executions
    }
}

public struct JarvisSchedulerJobExecution: Decodable, Equatable, Sendable {
    public var job: JarvisSchedulerJob
    public var task: JarvisTask
    public var accepted: Bool
    public var message: String
    public var auditEntries: [JarvisAuditEntry]

    enum CodingKeys: String, CodingKey {
        case job
        case task
        case accepted
        case message
        case auditEntries = "audit_entries"
    }
}

public struct JarvisSchedulerStaleRecoveryResponse: Decodable, Equatable, Sendable {
    public var checkedAt: String
    public var olderThanSeconds: UInt64
    public var limit: Int
    public var recovered: [JarvisSchedulerStaleRecoveryItem]

    enum CodingKeys: String, CodingKey {
        case checkedAt = "checked_at"
        case olderThanSeconds = "older_than_seconds"
        case limit
        case recovered
    }
}

public struct JarvisSchedulerStaleRecoveryItem: Decodable, Equatable, Sendable {
    public var job: JarvisDiagnosticSchedulerJob
    public var staleSince: String
    public var staleForSeconds: Int
    public var auditEntry: JarvisAuditEntry

    enum CodingKeys: String, CodingKey {
        case job
        case staleSince = "stale_since"
        case staleForSeconds = "stale_for_seconds"
        case auditEntry = "audit_entry"
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

public struct JarvisApprovalExecutionResponse: Decodable, Equatable, Sendable {
    public var accepted: Bool
    public var approval: JarvisPendingApproval
    public var task: JarvisTask
    public var auditEntry: JarvisAuditEntry
    public var auditEntries: [JarvisAuditEntry]
    public var pluginResults: [JarvisPluginCallResult]
    public var message: String

    enum CodingKeys: String, CodingKey {
        case accepted
        case approval
        case task
        case auditEntry = "audit_entry"
        case auditEntries = "audit_entries"
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
    public var executionAvailable: Bool
    public var action: String?
    public var requestedScopes: [String]
    public var riskTier: String?
    public var sensitivity: String?
    public var requestedAt: String?

    public init(
        id: UUID = UUID(),
        taskId: UUID?,
        title: String,
        detail: String,
        source: String,
        approvalStatus: String,
        actionAvailable: Bool,
        executionAvailable: Bool = false,
        action: String? = nil,
        requestedScopes: [String] = [],
        riskTier: String? = nil,
        sensitivity: String? = nil,
        requestedAt: String? = nil
    ) {
        self.id = id
        self.taskId = taskId
        self.title = title
        self.detail = detail
        self.source = source
        self.approvalStatus = approvalStatus
        self.actionAvailable = actionAvailable
        self.executionAvailable = executionAvailable
        self.action = action
        self.requestedScopes = requestedScopes
        self.riskTier = riskTier
        self.sensitivity = sensitivity
        self.requestedAt = requestedAt
    }

    public init(
        approval: JarvisPendingApproval,
        actionAvailable: Bool,
        executionAvailable: Bool = false
    ) {
        self.init(
            id: approval.id,
            taskId: approval.taskId,
            title: approval.action,
            detail: approval.reason,
            source: "approval",
            approvalStatus: approval.status,
            actionAvailable: actionAvailable && approval.status == "pending",
            executionAvailable: executionAvailable && approval.status == "approved",
            action: approval.action,
            requestedScopes: approval.requestedScopes,
            riskTier: approval.riskTier,
            sensitivity: approval.sensitivity,
            requestedAt: approval.requestedAt
        )
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

    public static func pendingItems(
        approvals: [JarvisPendingApproval],
        contract: JarvisContractResponse?
    ) -> [JarvisApprovalQueueItem] {
        let supportsApprovalActions = contract?.exposesApprovalActions == true
        let supportsApprovalExecution = contract?.exposesApprovalExecuteAction == true
        return approvals.map {
            JarvisApprovalQueueItem(
                approval: $0,
                actionAvailable: supportsApprovalActions,
                executionAvailable: supportsApprovalExecution
            )
        }
    }

    private static func approvalStatus(from payload: JarvisJSONValue?) -> String {
        guard case let .object(object) = payload,
              case let .string(status) = object["approval_status"] else {
            return "pending"
        }
        return status
    }
}

public enum JarvisPermissionSurfaceStatus: String, Equatable, Sendable {
    case clear
    case reviewRequired
    case inspectionOnly
}

public struct JarvisPermissionRiskCount: Equatable, Sendable {
    public var riskTier: String
    public var count: Int

    public init(riskTier: String, count: Int) {
        self.riskTier = riskTier
        self.count = count
    }
}

public struct JarvisPermissionSurfaceState: Equatable, Sendable {
    public var status: JarvisPermissionSurfaceStatus
    public var approvalActionsAvailable: Bool
    public var pendingApprovalCount: Int
    public var actionableApprovalCount: Int
    public var inspectionOnlyApprovalCount: Int
    public var approvedGrantCount: Int
    public var deniedGrantCount: Int
    public var installedPluginGrantCount: Int
    public var executableInstalledPluginGrantCount: Int
    public var unverifiedInstalledPluginGrantCount: Int
    public var sideEffectsRequireApproval: Bool
    public var declaredScopes: [String]
    public var riskTierCounts: [JarvisPermissionRiskCount]
    public var proactiveActionCount: Int

    public var summaryText: String {
        switch status {
        case .clear:
            return "No pending approvals. \(declaredScopes.count) declared permission scope(s) are visible."
        case .reviewRequired:
            return "\(actionableApprovalCount) approval request(s) need a decision before Jarvis can continue."
        case .inspectionOnly:
            return "\(pendingApprovalCount) approval signal(s) are visible, but this core cannot accept decisions yet."
        }
    }

    public init(
        status: JarvisPermissionSurfaceStatus,
        approvalActionsAvailable: Bool,
        pendingApprovalCount: Int,
        actionableApprovalCount: Int,
        inspectionOnlyApprovalCount: Int,
        approvedGrantCount: Int,
        deniedGrantCount: Int,
        installedPluginGrantCount: Int,
        executableInstalledPluginGrantCount: Int,
        unverifiedInstalledPluginGrantCount: Int,
        sideEffectsRequireApproval: Bool,
        declaredScopes: [String],
        riskTierCounts: [JarvisPermissionRiskCount],
        proactiveActionCount: Int
    ) {
        self.status = status
        self.approvalActionsAvailable = approvalActionsAvailable
        self.pendingApprovalCount = pendingApprovalCount
        self.actionableApprovalCount = actionableApprovalCount
        self.inspectionOnlyApprovalCount = inspectionOnlyApprovalCount
        self.approvedGrantCount = approvedGrantCount
        self.deniedGrantCount = deniedGrantCount
        self.installedPluginGrantCount = installedPluginGrantCount
        self.executableInstalledPluginGrantCount = executableInstalledPluginGrantCount
        self.unverifiedInstalledPluginGrantCount = unverifiedInstalledPluginGrantCount
        self.sideEffectsRequireApproval = sideEffectsRequireApproval
        self.declaredScopes = declaredScopes
        self.riskTierCounts = riskTierCounts
        self.proactiveActionCount = proactiveActionCount
    }

    public static let empty = JarvisPermissionSurfaceState(
        status: .clear,
        approvalActionsAvailable: false,
        pendingApprovalCount: 0,
        actionableApprovalCount: 0,
        inspectionOnlyApprovalCount: 0,
        approvedGrantCount: 0,
        deniedGrantCount: 0,
        installedPluginGrantCount: 0,
        executableInstalledPluginGrantCount: 0,
        unverifiedInstalledPluginGrantCount: 0,
        sideEffectsRequireApproval: true,
        declaredScopes: [],
        riskTierCounts: [],
        proactiveActionCount: 0
    )

    public static func current(
        pendingItems: [JarvisApprovalQueueItem],
        pluginManifests: [JarvisPluginManifest],
        contract: JarvisContractResponse?,
        grantSummary: JarvisPermissionGrantSummary? = nil
    ) -> JarvisPermissionSurfaceState {
        let approvalActionsAvailable = contract?.exposesApprovalActions == true
        let pendingApprovalCount = pendingItems.filter { $0.approvalStatus == "pending" }.count
        let actionableApprovalCount = pendingItems.filter { $0.approvalStatus == "pending" && $0.actionAvailable }.count
        let inspectionOnlyApprovalCount = pendingApprovalCount - actionableApprovalCount
        let approvedGrantCount = grantSummary?.count(for: "approved") ?? 0
        let deniedGrantCount = grantSummary?.count(for: "denied") ?? 0
        let installedPluginGrantCount = grantSummary?.installedPluginGrants.count ?? 0
        let executableInstalledPluginGrantCount = grantSummary?.executableInstalledPluginCount ?? 0
        let unverifiedInstalledPluginGrantCount = grantSummary?.unverifiedInstalledPluginCount ?? 0
        let sideEffectsRequireApproval = grantSummary?.sideEffectsRequireApproval ?? true
        let actions = pluginManifests.flatMap(\.actions)
        let declaredScopes = Array(Set(actions.flatMap(\.permissions))).sorted()
        let riskTierCounts = Dictionary(grouping: actions, by: \.riskTier)
            .map { JarvisPermissionRiskCount(riskTier: $0.key, count: $0.value.count) }
            .sorted { lhs, rhs in lhs.riskTier < rhs.riskTier }
        let proactiveActionCount = actions.filter(\.proactive).count
        let status: JarvisPermissionSurfaceStatus

        if actionableApprovalCount > 0 {
            status = .reviewRequired
        } else if pendingApprovalCount > 0 {
            status = .inspectionOnly
        } else {
            status = .clear
        }

        return JarvisPermissionSurfaceState(
            status: status,
            approvalActionsAvailable: approvalActionsAvailable,
            pendingApprovalCount: pendingApprovalCount,
            actionableApprovalCount: actionableApprovalCount,
            inspectionOnlyApprovalCount: inspectionOnlyApprovalCount,
            approvedGrantCount: approvedGrantCount,
            deniedGrantCount: deniedGrantCount,
            installedPluginGrantCount: installedPluginGrantCount,
            executableInstalledPluginGrantCount: executableInstalledPluginGrantCount,
            unverifiedInstalledPluginGrantCount: unverifiedInstalledPluginGrantCount,
            sideEffectsRequireApproval: sideEffectsRequireApproval,
            declaredScopes: declaredScopes,
            riskTierCounts: riskTierCounts,
            proactiveActionCount: proactiveActionCount
        )
    }
}

public struct JarvisPendingApproval: Decodable, Equatable, Identifiable, Sendable {
    public var id: UUID
    public var taskId: UUID
    public var action: String
    public var requestedScopes: [String]
    public var riskTier: String
    public var sensitivity: String
    public var status: String
    public var reason: String
    public var requestedAt: String
    public var decidedAt: String?
    public var decidedBy: String?
    public var decisionReason: String?

    enum CodingKeys: String, CodingKey {
        case id
        case taskId = "task_id"
        case action
        case requestedScopes = "requested_scopes"
        case riskTier = "risk_tier"
        case sensitivity
        case status
        case reason
        case requestedAt = "requested_at"
        case decidedAt = "decided_at"
        case decidedBy = "decided_by"
        case decisionReason = "decision_reason"
    }
}

public struct JarvisApprovalDecisionRequest: Encodable, Equatable, Sendable {
    public var decidedBy: String
    public var reason: String?

    public init(decidedBy: String, reason: String?) {
        self.decidedBy = decidedBy
        self.reason = reason
    }

    enum CodingKeys: String, CodingKey {
        case decidedBy = "decided_by"
        case reason
    }
}

public struct JarvisPermissionGrantSummary: Decodable, Equatable, Sendable {
    public var generatedAt: String
    public var approvalCounts: [JarvisApprovalStatusCount]
    public var latestApprovals: [JarvisPendingApproval]
    public var installedPluginGrants: [JarvisInstalledPluginGrantSurface]
    public var highRiskPendingCount: Int
    public var executableInstalledPluginCount: Int
    public var unverifiedInstalledPluginCount: Int
    public var sideEffectsRequireApproval: Bool

    public func count(for status: String) -> Int {
        approvalCounts.first { $0.status == status }?.count ?? 0
    }

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case approvalCounts = "approval_counts"
        case latestApprovals = "latest_approvals"
        case installedPluginGrants = "installed_plugin_grants"
        case highRiskPendingCount = "high_risk_pending_count"
        case executableInstalledPluginCount = "executable_installed_plugin_count"
        case unverifiedInstalledPluginCount = "unverified_installed_plugin_count"
        case sideEffectsRequireApproval = "side_effects_require_approval"
    }
}

public struct JarvisPermissionPolicyReview: Decodable, Equatable, Sendable {
    public var generatedAt: String
    public var status: String
    public var reviewItemCount: Int
    public var highRiskPendingCount: Int
    public var executableInstalledPluginCount: Int
    public var unverifiedInstalledPluginCount: Int
    public var unreviewedMemoryItemCount: Int
    public var sensitiveMemoryItemCount: Int
    public var sideEffectsRequireApproval: Bool
    public var items: [JarvisPermissionPolicyReviewItem]

    enum CodingKeys: String, CodingKey {
        case generatedAt = "generated_at"
        case status
        case reviewItemCount = "review_item_count"
        case highRiskPendingCount = "high_risk_pending_count"
        case executableInstalledPluginCount = "executable_installed_plugin_count"
        case unverifiedInstalledPluginCount = "unverified_installed_plugin_count"
        case unreviewedMemoryItemCount = "unreviewed_memory_item_count"
        case sensitiveMemoryItemCount = "sensitive_memory_item_count"
        case sideEffectsRequireApproval = "side_effects_require_approval"
        case items
    }
}

public struct JarvisPermissionPolicyReviewItem: Decodable, Equatable, Identifiable, Sendable {
    public var itemType: String
    public var severity: String
    public var title: String
    public var detail: String
    public var approvalId: UUID?
    public var pluginId: String?
    public var memoryId: UUID?
    public var action: String?

    public var id: String {
        [
            itemType,
            severity,
            approvalId?.uuidString ?? "",
            pluginId ?? "",
            memoryId?.uuidString ?? "",
            action ?? title
        ].joined(separator: ":")
    }

    enum CodingKeys: String, CodingKey {
        case itemType = "item_type"
        case severity
        case title
        case detail
        case approvalId = "approval_id"
        case pluginId = "plugin_id"
        case memoryId = "memory_id"
        case action
    }
}

public struct JarvisApprovalStatusCount: Decodable, Equatable, Sendable {
    public var status: String
    public var count: Int
}

public struct JarvisInstalledPluginGrantSurface: Decodable, Equatable, Identifiable, Sendable {
    public var pluginId: String
    public var name: String
    public var executionEnabled: Bool
    public var executionGrant: String
    public var integrityStatus: String
    public var captureMethod: String
    public var lastVerifiedAt: String?
    public var originClaim: String?
    public var originClaimVerified: Bool
    public var installedAt: String
    public var actionCount: Int
    public var highRiskActionCount: Int

    public var id: String { pluginId }
    public var needsProvenanceReview: Bool { integrityStatus != "matches_install_snapshot" }

    enum CodingKeys: String, CodingKey {
        case pluginId = "plugin_id"
        case name
        case executionEnabled = "execution_enabled"
        case executionGrant = "execution_grant"
        case integrityStatus = "integrity_status"
        case captureMethod = "capture_method"
        case lastVerifiedAt = "last_verified_at"
        case originClaim = "origin_claim"
        case originClaimVerified = "origin_claim_verified"
        case installedAt = "installed_at"
        case actionCount = "action_count"
        case highRiskActionCount = "high_risk_action_count"
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
    func releaseReadiness() async throws -> JarvisReleaseReadiness
    func releaseEvidenceStatus() async throws -> JarvisReleaseEvidenceStatus
    func releaseLiveDeviceRunbook() async throws -> JarvisReleaseRunbook
    func releaseSignedDistributionRunbook() async throws -> JarvisReleaseRunbook
    func releasePluginTrustRunbook() async throws -> JarvisReleaseRunbook
    func releaseEvidenceBundleRunbook() async throws -> JarvisReleaseRunbook
    func submit(_ command: JarvisCommandRequest) async throws -> JarvisCommandResponse
    func pause(reason: String) async throws -> JarvisPauseResponse
    func resume() async throws -> JarvisPauseResponse
    func pauseStatus() async throws -> JarvisPauseResponse
    func listTasks() async throws -> [JarvisTask]
    func task(id: UUID) async throws -> JarvisTask
    func listAuditEntries(taskId: UUID?) async throws -> [JarvisAuditEntry]
    func activitySummary() async throws -> JarvisActivitySummary
    func activityEvents(maxEvents: Int, intervalMilliseconds: Int) async throws -> [JarvisActivityEvent]
    func listMemoryItems(includeDeleted: Bool) async throws -> [JarvisMemoryItem]
    func memoryClassification(includeDeleted: Bool) async throws -> JarvisMemoryClassificationSummary
    func memoryIndexStatus() async throws -> JarvisMemoryIndexStatus
    func rebuildMemoryIndex() async throws -> JarvisMemoryIndexStatus
    func memoryRetentionPlan() async throws -> JarvisMemoryRetentionPlan
    func createMemoryItem(_ request: JarvisCreateMemoryItemRequest) async throws -> JarvisMemoryItem
    func memoryItem(id: UUID) async throws -> JarvisMemoryItem
    func updateMemoryItem(id: UUID, request: JarvisMemoryMutationRequest) async throws -> JarvisMemoryItem
    func reviewMemoryItem(id: UUID) async throws -> JarvisMemoryItem
    func deleteMemoryItem(id: UUID) async throws -> JarvisMemoryItem
    func restoreMemoryItem(id: UUID) async throws -> JarvisMemoryItem
    func listPluginManifests() async throws -> [JarvisPluginManifest]
    func modelToolCatalog() async throws -> JarvisModelToolCatalog
    func listInstalledPlugins() async throws -> [JarvisInstalledPluginRecord]
    func listSchedulerJobs() async throws -> [JarvisSchedulerJob]
    func schedulerAttention() async throws -> JarvisSchedulerAttentionSummary
    func schedulerJob(id: UUID) async throws -> JarvisSchedulerJob
    func createSchedulerJob(_ request: JarvisCreateSchedulerJobRequest) async throws -> JarvisSchedulerJob
    func cancelSchedulerJob(id: UUID) async throws -> JarvisSchedulerJob
    func runDueSchedulerJobs(limit: Int) async throws -> JarvisSchedulerRunResponse
    func recoverStaleSchedulerJobs(olderThanSeconds: UInt64, limit: Int) async throws -> JarvisSchedulerStaleRecoveryResponse
    func trustedWakeStatus() async throws -> JarvisTrustedWakeStatus
    func trustedWakeAttention() async throws -> [JarvisTrustedWakeAttentionItem]
    func resolveTrustedWakeAttention(id: UUID, request: JarvisTrustedWakeResolutionRequest) async throws -> JarvisTrustedWakeAttentionItem
    func setTrustedWakeEnabled(_ request: JarvisTrustedWakeRuleEnablement) async throws -> JarvisTrustedWakeRule
    func prepareTrustedWakeKeyControl(_ request: JarvisTrustedWakeKeyControlPrepareRequest) async throws -> JarvisTrustedWakeKeyControlPrepareResponse
    func cancelTrustedWakeKeyControl(_ request: JarvisTrustedWakeKeyControlCancelRequest) async throws -> JarvisTrustedWakeRule
    func submitTrustedWake(_ envelope: JarvisTrustedWakeEnvelope) async throws -> JarvisTrustedWakeEventResponse
    func diagnosticsExport() async throws -> JarvisDiagnosticsExport
    func permissionGrantSummary() async throws -> JarvisPermissionGrantSummary
    func permissionPolicyReview() async throws -> JarvisPermissionPolicyReview
    func listApprovals(status: String?) async throws -> [JarvisPendingApproval]
    func approval(id: UUID) async throws -> JarvisPendingApproval
    func approveApproval(id: UUID, request: JarvisApprovalDecisionRequest) async throws -> JarvisPendingApproval
    func denyApproval(id: UUID, request: JarvisApprovalDecisionRequest) async throws -> JarvisPendingApproval
    func executeApproval(id: UUID) async throws -> JarvisApprovalExecutionResponse
}

public final class JarvisIPCClient: JarvisCoreClient {
    private let endpoint: JarvisEndpoint
    private let session: URLSession
    private let authorization: JarvisIPCSessionAuthorization
    private let encoder: JSONEncoder
    private let decoder: JSONDecoder

    public init(
        endpoint: JarvisEndpoint = JarvisEndpoint(),
        session: URLSession = .shared,
        authorization: JarvisIPCSessionAuthorization = JarvisIPCSessionAuthorization()
    ) {
        self.endpoint = endpoint
        self.session = session
        self.authorization = authorization
        self.encoder = JSONEncoder()
        self.decoder = JSONDecoder()
    }

    public func health() async throws -> JarvisHealth {
        try await send(path: "/health", method: "GET", body: Optional<Data>.none)
    }

    public func contract() async throws -> JarvisContractResponse {
        try await send(path: "/contract", method: "GET", body: Optional<Data>.none)
    }

    public func releaseReadiness() async throws -> JarvisReleaseReadiness {
        try await send(path: "/release/readiness", method: "GET", body: Optional<Data>.none)
    }

    public func releaseEvidenceStatus() async throws -> JarvisReleaseEvidenceStatus {
        try await send(path: "/release/evidence-status", method: "GET", body: Optional<Data>.none)
    }

    public func releaseLiveDeviceRunbook() async throws -> JarvisReleaseRunbook {
        try await send(path: "/release/live-device-runbook", method: "GET", body: Optional<Data>.none)
    }

    public func releaseSignedDistributionRunbook() async throws -> JarvisReleaseRunbook {
        try await send(path: "/release/signed-distribution-runbook", method: "GET", body: Optional<Data>.none)
    }

    public func releasePluginTrustRunbook() async throws -> JarvisReleaseRunbook {
        try await send(path: "/release/plugin-trust-runbook", method: "GET", body: Optional<Data>.none)
    }

    public func releaseEvidenceBundleRunbook() async throws -> JarvisReleaseRunbook {
        try await send(path: "/release/evidence-bundle-runbook", method: "GET", body: Optional<Data>.none)
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

    public func activitySummary() async throws -> JarvisActivitySummary {
        try await send(path: "/activity/summary", method: "GET", body: Optional<Data>.none)
    }

    public func activityEvents(maxEvents: Int = 2, intervalMilliseconds: Int = 500) async throws -> [JarvisActivityEvent] {
        let boundedMaxEvents = min(max(maxEvents, 1), 16)
        let boundedInterval = max(intervalMilliseconds, 100)
        let path = "/activity/events?max_events=\(boundedMaxEvents)&interval_ms=\(boundedInterval)"
        var request = URLRequest(url: endpoint.url(path: path))
        request.httpMethod = "GET"
        request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        try authorize(&request)

        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw JarvisIPCError.invalidResponse
        }

        guard (200..<300).contains(http.statusCode) else {
            throw JarvisIPCError.httpStatus(http.statusCode, Self.safeErrorBody(data, statusCode: http.statusCode))
        }

        return try JarvisActivityEvent.parseServerSentEvents(data)
    }

    public func listMemoryItems(includeDeleted: Bool = false) async throws -> [JarvisMemoryItem] {
        let path = includeDeleted ? "/memory?include_deleted=true" : "/memory"
        return try await send(path: path, method: "GET", body: Optional<Data>.none)
    }

    public func memoryClassification(includeDeleted: Bool = false) async throws -> JarvisMemoryClassificationSummary {
        let path = includeDeleted ? "/memory/classification?include_deleted=true" : "/memory/classification"
        return try await send(path: path, method: "GET", body: Optional<Data>.none)
    }

    public func memoryIndexStatus() async throws -> JarvisMemoryIndexStatus {
        try await send(path: "/memory/index/status", method: "GET", body: Optional<Data>.none)
    }

    public func rebuildMemoryIndex() async throws -> JarvisMemoryIndexStatus {
        try await send(path: "/memory/index/rebuild", method: "POST", body: Optional<Data>.none)
    }

    public func memoryRetentionPlan() async throws -> JarvisMemoryRetentionPlan {
        try await send(path: "/memory/retention-plan", method: "GET", body: Optional<Data>.none)
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

    public func restoreMemoryItem(id: UUID) async throws -> JarvisMemoryItem {
        try await send(path: "/memory/\(id.uuidString)/restore", method: "POST", body: Optional<Data>.none)
    }

    public func listPluginManifests() async throws -> [JarvisPluginManifest] {
        try await send(path: "/plugins/manifests", method: "GET", body: Optional<Data>.none)
    }

    public func modelToolCatalog() async throws -> JarvisModelToolCatalog {
        try await send(path: "/tools/model", method: "GET", body: Optional<Data>.none)
    }

    public func listInstalledPlugins() async throws -> [JarvisInstalledPluginRecord] {
        try await send(path: "/plugins/installed", method: "GET", body: Optional<Data>.none)
    }

    public func listSchedulerJobs() async throws -> [JarvisSchedulerJob] {
        try await send(path: "/scheduler/jobs", method: "GET", body: Optional<Data>.none)
    }

    public func schedulerAttention() async throws -> JarvisSchedulerAttentionSummary {
        try await send(path: "/scheduler/attention", method: "GET", body: Optional<Data>.none)
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

    public func runDueSchedulerJobs(limit: Int = 8) async throws -> JarvisSchedulerRunResponse {
        let boundedLimit = max(limit, 1)
        return try await send(
            path: "/scheduler/run-due?limit=\(boundedLimit)",
            method: "POST",
            body: Optional<Data>.none
        )
    }

    public func recoverStaleSchedulerJobs(
        olderThanSeconds: UInt64 = 3600,
        limit: Int = 8
    ) async throws -> JarvisSchedulerStaleRecoveryResponse {
        let boundedOlderThanSeconds = max(olderThanSeconds, 1)
        let boundedLimit = max(limit, 1)
        return try await send(
            path: "/scheduler/recover-stale?older_than_seconds=\(boundedOlderThanSeconds)&limit=\(boundedLimit)",
            method: "POST",
            body: Optional<Data>.none
        )
    }

    public func trustedWakeStatus() async throws -> JarvisTrustedWakeStatus {
        try await send(path: "/system-wake/status", method: "GET", body: Optional<Data>.none)
    }

    public func trustedWakeAttention() async throws -> [JarvisTrustedWakeAttentionItem] {
        try await send(path: "/system-wake/attention", method: "GET", body: Optional<Data>.none)
    }

    public func resolveTrustedWakeAttention(
        id: UUID,
        request: JarvisTrustedWakeResolutionRequest
    ) async throws -> JarvisTrustedWakeAttentionItem {
        try await send(
            path: "/system-wake/events/\(id.uuidString)/resolve",
            method: "POST",
            body: encoder.encode(request)
        )
    }

    public func setTrustedWakeEnabled(
        _ request: JarvisTrustedWakeRuleEnablement
    ) async throws -> JarvisTrustedWakeRule {
        try await send(path: "/system-wake/rule", method: "POST", body: encoder.encode(request))
    }

    public func prepareTrustedWakeKeyControl(
        _ request: JarvisTrustedWakeKeyControlPrepareRequest
    ) async throws -> JarvisTrustedWakeKeyControlPrepareResponse {
        try await send(
            path: "/system-wake/key-control/prepare",
            method: "POST",
            body: encoder.encode(request)
        )
    }

    public func cancelTrustedWakeKeyControl(
        _ request: JarvisTrustedWakeKeyControlCancelRequest
    ) async throws -> JarvisTrustedWakeRule {
        try await send(
            path: "/system-wake/key-control/cancel",
            method: "POST",
            body: encoder.encode(request)
        )
    }

    public func submitTrustedWake(
        _ envelope: JarvisTrustedWakeEnvelope
    ) async throws -> JarvisTrustedWakeEventResponse {
        try await send(path: "/system-wake/events", method: "POST", body: encoder.encode(envelope))
    }

    public func diagnosticsExport() async throws -> JarvisDiagnosticsExport {
        try await send(path: "/diagnostics/export", method: "GET", body: Optional<Data>.none)
    }

    public func permissionGrantSummary() async throws -> JarvisPermissionGrantSummary {
        try await send(path: "/permissions/grants", method: "GET", body: Optional<Data>.none)
    }

    public func permissionPolicyReview() async throws -> JarvisPermissionPolicyReview {
        try await send(path: "/permissions/policy-review", method: "GET", body: Optional<Data>.none)
    }

    public func listApprovals(status: String? = nil) async throws -> [JarvisPendingApproval] {
        let path = status.map { "/approvals?status=\($0)" } ?? "/approvals"
        return try await send(path: path, method: "GET", body: Optional<Data>.none)
    }

    public func approval(id: UUID) async throws -> JarvisPendingApproval {
        try await send(path: "/approvals/\(id.uuidString)", method: "GET", body: Optional<Data>.none)
    }

    public func approveApproval(
        id: UUID,
        request: JarvisApprovalDecisionRequest
    ) async throws -> JarvisPendingApproval {
        try await send(path: "/approvals/\(id.uuidString)/approve", method: "POST", body: encoder.encode(request))
    }

    public func denyApproval(
        id: UUID,
        request: JarvisApprovalDecisionRequest
    ) async throws -> JarvisPendingApproval {
        try await send(path: "/approvals/\(id.uuidString)/deny", method: "POST", body: encoder.encode(request))
    }

    public func executeApproval(id: UUID) async throws -> JarvisApprovalExecutionResponse {
        try await send(path: "/approvals/\(id.uuidString)/execute", method: "POST", body: Optional<Data>.none)
    }

    private func send<Response: Decodable>(
        path: String,
        method: String,
        body: Data?
    ) async throws -> Response {
        var request = URLRequest(url: endpoint.url(path: path))
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        try authorize(&request)

        if let body {
            request.httpBody = body
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }

        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw JarvisIPCError.invalidResponse
        }

        guard (200..<300).contains(http.statusCode) else {
            throw JarvisIPCError.httpStatus(http.statusCode, Self.safeErrorBody(data, statusCode: http.statusCode))
        }

        return try decoder.decode(Response.self, from: data)
    }

    private func authorize(_ request: inout URLRequest) throws {
        if authorization.mode == .appSupervised {
            guard let url = request.url,
                  JarvisIPCSessionAuthorization.isStrictLoopbackEndpoint(url) else {
                throw JarvisIPCAuthorizationError.nonLoopbackEndpoint
            }
        }
        if let header = try authorization.authorizationHeader() {
            request.setValue(header, forHTTPHeaderField: "Authorization")
        }
    }

    private static func safeErrorBody(_ data: Data, statusCode: Int) -> String {
        if statusCode == 401 || statusCode == 403 {
            return "local IPC authorization failed"
        }
        return String(decoding: data, as: UTF8.self)
    }
}
