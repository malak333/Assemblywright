import Foundation

public protocol AssemblywrightMacBridgeSession: Sendable {
    var connectionEpoch: UInt64 { get }
    func send(_ request: AssemblywrightMacBridgeHTTPRequest) async throws -> AssemblywrightMacBridgeHTTPResponse
    func cancel() async
}

extension AssemblywrightMacAuthenticatedBridgeSession: AssemblywrightMacBridgeSession {}

public protocol AssemblywrightMacBridgeConnecting: Sendable {
    func connect(profile: AssemblywrightMacBridgeProfile) async throws -> any AssemblywrightMacBridgeSession
}

public struct AssemblywrightMacDefaultBridgeConnector: AssemblywrightMacBridgeConnecting, Sendable {
    private let transport: AssemblywrightMacMTLSBridgeTransport

    public init(transport: AssemblywrightMacMTLSBridgeTransport = AssemblywrightMacMTLSBridgeTransport()) {
        self.transport = transport
    }

    public func connect(profile: AssemblywrightMacBridgeProfile) async throws -> any AssemblywrightMacBridgeSession {
        try await transport.connect(profile: profile)
    }
}

public enum AssemblywrightMacBridgeSupervisorPhase: String, Codable, Equatable, Sendable {
    case authenticated
    case backingOff = "backing_off"
    case stopped
}

public enum AssemblywrightMacFeatureConveyorLifecycleStatus: String, Codable, Equatable, Sendable {
    case queued
    case implementing
    case validating
    case reviewing
    case publishing
    case verifyingMain = "verifying_main"
    case succeeded
    case cancelled
    case abandoned
    case quarantined
}

public enum AssemblywrightMacFeatureConveyorGuidanceState: String, Codable, Equatable, Sendable {
    case idle
    case ready
    case blocked
    case inProgress = "in_progress"
}

public enum AssemblywrightMacFeatureConveyorGuidanceReason: String, Codable, Equatable, Sendable {
    case queueEmpty = "queue_empty"
    case headDependencySatisfied = "head_dependency_satisfied"
    case headDependencyUnsatisfied = "head_dependency_unsatisfied"
    case activeFeatureLeased = "active_feature_leased"
    case activeRequiresReconciliation = "active_requires_reconciliation"
    case emergencyPaused = "emergency_paused"
}

public enum AssemblywrightMacFeatureConveyorNextOwnerAction: String, Codable, Equatable, Sendable {
    case prepareApprovedFeature = "prepare_approved_feature"
    case awaitOwnerControlSurface = "await_owner_control_surface"
    case resolveHeadDependency = "resolve_head_dependency"
    case wait
    case reconcileActiveFeature = "reconcile_active_feature"
    case resumeEmergencyPause = "resume_emergency_pause"
}

public struct AssemblywrightMacFeatureConveyorStatus: Codable, Equatable, Sendable {
    public static let expectedSchemaVersion: UInt64 = 7
    public static let maximumFeatures = 100
    public static let maximumVisibleFeatures: UInt64 = 101

    public struct Counts: Codable, Equatable, Sendable {
        public let queued: UInt64
        public let implementing: UInt64
        public let validating: UInt64
        public let reviewing: UInt64
        public let publishing: UInt64
        public let verifyingMain: UInt64
        public let succeeded: UInt64
        public let cancelled: UInt64
        public let abandoned: UInt64
        public let quarantined: UInt64

        enum CodingKeys: String, CodingKey, CaseIterable {
            case queued, implementing, validating, reviewing, publishing
            case verifyingMain = "verifying_main"
            case succeeded, cancelled, abandoned, quarantined
        }

        fileprivate var orderedValues: [UInt64] {
            [
                queued, implementing, validating, reviewing, publishing,
                verifyingMain, succeeded, cancelled, abandoned, quarantined
            ]
        }

        fileprivate func count(for status: AssemblywrightMacFeatureConveyorLifecycleStatus) -> UInt64 {
            switch status {
            case .queued: queued
            case .implementing: implementing
            case .validating: validating
            case .reviewing: reviewing
            case .publishing: publishing
            case .verifyingMain: verifyingMain
            case .succeeded: succeeded
            case .cancelled: cancelled
            case .abandoned: abandoned
            case .quarantined: quarantined
            }
        }
    }

    public struct Feature: Codable, Equatable, Sendable {
        public let featureID: UUID
        public let specificationRevision: UInt64
        public let lifecycleRevision: UInt64
        public let queuePosition: UInt64
        public let status: AssemblywrightMacFeatureConveyorLifecycleStatus
        public let leasePresent: Bool
        public let effectPossible: Bool

        enum CodingKeys: String, CodingKey, CaseIterable {
            case featureID = "feature_id"
            case specificationRevision = "specification_revision"
            case lifecycleRevision = "lifecycle_revision"
            case queuePosition = "queue_position"
            case status
            case leasePresent = "lease_present"
            case effectPossible = "effect_possible"
        }
    }

    public struct OwnerGuidance: Codable, Equatable, Sendable {
        public let state: AssemblywrightMacFeatureConveyorGuidanceState
        public let reasonCode: AssemblywrightMacFeatureConveyorGuidanceReason
        public let nextOwnerAction: AssemblywrightMacFeatureConveyorNextOwnerAction
        public let featureID: UUID?
        public let specificationRevision: UInt64?
        public let lifecycleRevision: UInt64?
        public let queueRevision: UInt64
        public let emergencyPauseRevision: UInt64

        enum CodingKeys: String, CodingKey, CaseIterable {
            case state
            case reasonCode = "reason_code"
            case nextOwnerAction = "next_owner_action"
            case featureID = "feature_id"
            case specificationRevision = "specification_revision"
            case lifecycleRevision = "lifecycle_revision"
            case queueRevision = "queue_revision"
            case emergencyPauseRevision = "emergency_pause_revision"
        }

        public func encode(to encoder: Encoder) throws {
            var container = encoder.container(keyedBy: CodingKeys.self)
            try container.encode(state, forKey: .state)
            try container.encode(reasonCode, forKey: .reasonCode)
            try container.encode(nextOwnerAction, forKey: .nextOwnerAction)
            if let featureID {
                try container.encode(featureID, forKey: .featureID)
            } else {
                try container.encodeNil(forKey: .featureID)
            }
            if let specificationRevision {
                try container.encode(specificationRevision, forKey: .specificationRevision)
            } else {
                try container.encodeNil(forKey: .specificationRevision)
            }
            if let lifecycleRevision {
                try container.encode(lifecycleRevision, forKey: .lifecycleRevision)
            } else {
                try container.encodeNil(forKey: .lifecycleRevision)
            }
            try container.encode(queueRevision, forKey: .queueRevision)
            try container.encode(emergencyPauseRevision, forKey: .emergencyPauseRevision)
        }
    }

    public let schemaVersion: UInt64
    public let queueRevision: UInt64
    public let startupQuarantineCount: UInt64
    public let countsByStatus: Counts
    public let visibleFeatureCount: UInt64
    public let featuresTruncated: Bool
    public let features: [Feature]
    public let ownerGuidance: OwnerGuidance

    enum CodingKeys: String, CodingKey, CaseIterable {
        case schemaVersion = "schema_version"
        case queueRevision = "queue_revision"
        case startupQuarantineCount = "startup_quarantine_count"
        case countsByStatus = "counts_by_status"
        case visibleFeatureCount = "visible_feature_count"
        case featuresTruncated = "features_truncated"
        case features
        case ownerGuidance = "owner_guidance"
    }
}

public struct AssemblywrightMacBridgeSupervisorSnapshot: Codable, Equatable, Sendable {
    public let phase: AssemblywrightMacBridgeSupervisorPhase
    public let deviceID: String
    public let masterEndpoint: String
    public let connectionEpoch: UInt64?
    public let consecutiveFailures: UInt32
    public let nextDelayMilliseconds: UInt64
    public let masterStatus: String?
    public let maintenanceActive: Bool?
    public let emergencyPaused: Bool?
    public let protocolVersion: UInt16?
    public let schemaVersion: Int64?
    public let featureConveyor: AssemblywrightMacFeatureConveyorStatus?
    public let errorCode: String?

    enum CodingKeys: String, CodingKey {
        case phase
        case deviceID = "device_id"
        case masterEndpoint = "master_endpoint"
        case connectionEpoch = "connection_epoch"
        case consecutiveFailures = "consecutive_failures"
        case nextDelayMilliseconds = "next_delay_ms"
        case masterStatus = "master_status"
        case maintenanceActive = "maintenance_active"
        case emergencyPaused = "emergency_paused"
        case protocolVersion = "protocol_version"
        case schemaVersion = "schema_version"
        case featureConveyor = "feature_conveyor"
        case errorCode = "error_code"
    }

    public static func decodeStrict(_ data: Data) throws -> Self {
        guard !data.isEmpty,
              data.count <= AssemblywrightDeveloperBridgeProcessLifecycle.maximumLineBytes else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        var duplicateScanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
        try duplicateScanner.validateNoDuplicateObjectKeysRecursively()
        var keyScanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
        let rawKeys = try keyScanner.scanTopLevelKeys()
        guard Set(rawKeys).count == rawKeys.count else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let phaseText = object["phase"] as? String,
              let phase = AssemblywrightMacBridgeSupervisorPhase(rawValue: phaseText) else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        let authenticated = Set([
            "phase", "device_id", "master_endpoint", "connection_epoch",
            "consecutive_failures", "next_delay_ms", "master_status",
            "maintenance_active", "emergency_paused", "protocol_version",
            "schema_version", "feature_conveyor"
        ])
        let backingOff = Set([
            "phase", "device_id", "master_endpoint", "consecutive_failures",
            "next_delay_ms", "error_code"
        ])
        let stopped = Set([
            "phase", "device_id", "master_endpoint", "consecutive_failures",
            "next_delay_ms"
        ])
        let expectedKeys: Set<String>
        switch phase {
        case .authenticated: expectedKeys = authenticated
        case .backingOff: expectedKeys = backingOff
        case .stopped: expectedKeys = stopped
        }
        guard Set(object.keys) == expectedKeys else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        let snapshot: Self
        do {
            snapshot = try JSONDecoder().decode(Self.self, from: data)
        } catch {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        let canonicalUUID = UUID(uuidString: snapshot.deviceID)?.uuidString.lowercased()
        guard canonicalUUID == snapshot.deviceID.lowercased(),
              !snapshot.masterEndpoint.isEmpty,
              snapshot.masterEndpoint.utf8.count <= 512,
              !snapshot.masterEndpoint.contains(where: { $0.isWhitespace || $0.isNewline }) else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        switch snapshot.phase {
        case .authenticated:
            guard let featureObject = object["feature_conveyor"] as? [String: Any],
                  let featureConveyor = snapshot.featureConveyor else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
            try AssemblywrightMacRemoteFeatureConveyorStatus.validate(
                featureConveyor,
                object: featureObject
            )
            guard snapshot.connectionEpoch.map({ $0 > 0 }) == true,
                  snapshot.consecutiveFailures == 0,
                  snapshot.nextDelayMilliseconds > 0,
                  snapshot.nextDelayMilliseconds <= 60_000,
                  snapshot.maintenanceActive != nil,
                  snapshot.emergencyPaused != nil,
                  snapshot.masterStatus == (
                    snapshot.maintenanceActive == true
                        ? "maintenance"
                        : snapshot.emergencyPaused == true ? "paused" : "ok"
                  ),
                  snapshot.protocolVersion == AssemblywrightMacMTLSBridgeTransport.protocolVersion,
                  snapshot.schemaVersion.map({ $0 > 0 }) == true,
                  snapshot.schemaVersion == Int64(featureConveyor.schemaVersion),
                  snapshot.errorCode == nil else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        case .backingOff:
            guard snapshot.connectionEpoch == nil,
                  snapshot.consecutiveFailures > 0,
                  (1 ... AssemblywrightMacBridgeSupervisor.maximumBackoffMilliseconds)
                    .contains(snapshot.nextDelayMilliseconds),
                  [
                      "invalid_health", "bridge_unavailable", "connection_failed",
                      "event_relay_failed", "invalid_feature_conveyor_status"
                  ]
                    .contains(snapshot.errorCode),
                  snapshot.masterStatus == nil,
                  snapshot.maintenanceActive == nil,
                  snapshot.emergencyPaused == nil,
                  snapshot.protocolVersion == nil,
                  snapshot.schemaVersion == nil,
                  snapshot.featureConveyor == nil else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        case .stopped:
            guard snapshot.connectionEpoch == nil,
                  snapshot.nextDelayMilliseconds == 0,
                  snapshot.masterStatus == nil,
                  snapshot.maintenanceActive == nil,
                  snapshot.emergencyPaused == nil,
                  snapshot.protocolVersion == nil,
                  snapshot.schemaVersion == nil,
                  snapshot.featureConveyor == nil,
                  snapshot.errorCode == nil else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        }
        return snapshot
    }
}

struct AssemblywrightStrictJSONObjectKeyScanner {
    private let bytes: [UInt8]
    private var index = 0

    init(data: Data) {
        bytes = Array(data)
    }

    mutating func scanTopLevelKeys() throws -> [String] {
        skipWhitespace()
        try consume(0x7b)
        skipWhitespace()
        var keys: [String] = []
        if consumeIfPresent(0x7d) {
            skipWhitespace()
            try requireEnd()
            return keys
        }
        while true {
            skipWhitespace()
            keys.append(try parseString())
            skipWhitespace()
            try consume(0x3a)
            try skipValue()
            skipWhitespace()
            if consumeIfPresent(0x2c) { continue }
            try consume(0x7d)
            skipWhitespace()
            try requireEnd()
            return keys
        }
    }

    mutating func validateNoDuplicateObjectKeysRecursively() throws {
        skipWhitespace()
        try scanValueRejectingDuplicateObjectKeys()
        skipWhitespace()
        try requireEnd()
    }

    private mutating func scanValueRejectingDuplicateObjectKeys() throws {
        skipWhitespace()
        guard index < bytes.count else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        switch bytes[index] {
        case 0x7b:
            try consume(0x7b)
            skipWhitespace()
            var keys = Set<String>()
            if consumeIfPresent(0x7d) { return }
            while true {
                skipWhitespace()
                let key = try parseString()
                guard keys.insert(key).inserted else {
                    throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
                }
                skipWhitespace()
                try consume(0x3a)
                try scanValueRejectingDuplicateObjectKeys()
                skipWhitespace()
                if consumeIfPresent(0x2c) { continue }
                try consume(0x7d)
                return
            }
        case 0x5b:
            try consume(0x5b)
            skipWhitespace()
            if consumeIfPresent(0x5d) { return }
            while true {
                try scanValueRejectingDuplicateObjectKeys()
                skipWhitespace()
                if consumeIfPresent(0x2c) { continue }
                try consume(0x5d)
                return
            }
        case 0x22:
            _ = try parseString()
        default:
            let start = index
            while index < bytes.count,
                  !Self.isWhitespace(bytes[index]),
                  ![0x2c, 0x7d, 0x5d].contains(bytes[index]) {
                index += 1
            }
            guard index > start else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        }
    }

    private mutating func parseString() throws -> String {
        let start = index
        try consume(0x22)
        var escaped = false
        while index < bytes.count {
            let byte = bytes[index]
            index += 1
            if escaped {
                escaped = false
                continue
            }
            if byte == 0x5c {
                escaped = true
                continue
            }
            if byte == 0x22 {
                let encoded = Data(bytes[start ..< index])
                do {
                    return try JSONDecoder().decode(String.self, from: encoded)
                } catch {
                    throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
                }
            }
            if byte < 0x20 {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        }
        throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
    }

    private mutating func skipValue() throws {
        skipWhitespace()
        guard index < bytes.count else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        if bytes[index] == 0x22 {
            _ = try parseString()
            return
        }
        if bytes[index] == 0x7b || bytes[index] == 0x5b {
            try skipContainer()
            return
        }
        let start = index
        while index < bytes.count, bytes[index] != 0x2c, bytes[index] != 0x7d {
            index += 1
        }
        guard bytes[start ..< index].contains(where: { !Self.isWhitespace($0) }) else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
    }

    private mutating func skipContainer() throws {
        var stack: [UInt8] = []
        while index < bytes.count {
            let byte = bytes[index]
            if byte == 0x22 {
                _ = try parseString()
                continue
            }
            index += 1
            switch byte {
            case 0x7b: stack.append(0x7d)
            case 0x5b: stack.append(0x5d)
            case 0x7d, 0x5d:
                guard stack.popLast() == byte else {
                    throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
                }
                if stack.isEmpty { return }
            default:
                break
            }
        }
        throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
    }

    private mutating func skipWhitespace() {
        while index < bytes.count, Self.isWhitespace(bytes[index]) { index += 1 }
    }

    private mutating func consume(_ expected: UInt8) throws {
        guard consumeIfPresent(expected) else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
    }

    private mutating func consumeIfPresent(_ expected: UInt8) -> Bool {
        guard index < bytes.count, bytes[index] == expected else { return false }
        index += 1
        return true
    }

    private func requireEnd() throws {
        guard index == bytes.count else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
    }

    private static func isWhitespace(_ byte: UInt8) -> Bool {
        byte == 0x20 || byte == 0x09 || byte == 0x0a || byte == 0x0d
    }
}

struct AssemblywrightMacFixtureControlReceipt: Equatable, Sendable {
    enum Status: String, Sendable {
        case successObserved = "fixture_success_observed"
        case cancellationLeased = "fixture_cancellation_leased"
        case cancellationObserved = "fixture_cancellation_observed"
        case emergencyResumed = "fixture_emergency_resumed"
    }

    static let maximumBytes = 4 * 1_024

    let status: Status
    let taskID: UUID?
    let stepID: UUID?
    let streamID: UUID?
    let queuedSequence: UInt64?
    let leasedSequence: UInt64?
    let succeededSequence: UInt64?
    let requestedSequence: UInt64?
    let acknowledgedSequence: UInt64?
    let cancelledSequence: UInt64?
    let lateOutputWindowMilliseconds: UInt64?

    static func decodeStrict(_ data: Data) throws -> Self {
        var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
        let rawKeys = try scanner.scanTopLevelKeys()
        guard !data.isEmpty,
              data.count <= maximumBytes,
              Set(rawKeys).count == rawKeys.count,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              strictInteger(object["schema_version"]) == 1,
              let statusText = object["status"] as? String,
              let status = Status(rawValue: statusText) else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }

        let successKeys = Set([
            "schema_version", "status", "task_id", "step_id", "stream_id",
            "queued_sequence", "leased_sequence", "succeeded_sequence"
        ])
        let leasedKeys = Set([
            "schema_version", "status", "task_id", "step_id", "stream_id",
            "queued_sequence", "leased_sequence"
        ])
        let cancellationKeys = Set([
            "schema_version", "status", "task_id", "step_id", "stream_id",
            "requested_sequence", "acknowledged_sequence", "cancelled_sequence",
            "late_output_window_ms"
        ])
        let resumeKeys = Set(["schema_version", "status"])

        let taskID = strictUUID(object["task_id"])
        let stepID = strictUUID(object["step_id"])
        let streamID = strictUUID(object["stream_id"])
        let queued = strictInteger(object["queued_sequence"])
        let leased = strictInteger(object["leased_sequence"])
        let succeeded = strictInteger(object["succeeded_sequence"])
        let requested = strictInteger(object["requested_sequence"])
        let acknowledged = strictInteger(object["acknowledged_sequence"])
        let cancelled = strictInteger(object["cancelled_sequence"])
        let lateOutputWindow = strictInteger(object["late_output_window_ms"])

        switch status {
        case .successObserved:
            guard Set(object.keys) == successKeys,
                  taskID != nil, stepID != nil, streamID != nil,
                  let queued, let leased, let succeeded,
                  queued > 0, queued < leased, leased < succeeded else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        case .cancellationLeased:
            guard Set(object.keys) == leasedKeys,
                  taskID != nil, stepID != nil, streamID != nil,
                  let queued, let leased,
                  queued > 0, queued < leased else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        case .cancellationObserved:
            guard Set(object.keys) == cancellationKeys,
                  taskID != nil, stepID != nil, streamID != nil,
                  let requested, let acknowledged, let cancelled,
                  requested > 0, requested < acknowledged, acknowledged < cancelled,
                  lateOutputWindow == 7_000 else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        case .emergencyResumed:
            guard Set(object.keys) == resumeKeys else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        }

        return Self(
            status: status,
            taskID: taskID,
            stepID: stepID,
            streamID: streamID,
            queuedSequence: queued,
            leasedSequence: leased,
            succeededSequence: succeeded,
            requestedSequence: requested,
            acknowledgedSequence: acknowledged,
            cancelledSequence: cancelled,
            lateOutputWindowMilliseconds: lateOutputWindow
        )
    }

    private static func strictUUID(_ value: Any?) -> UUID? {
        guard let text = value as? String,
              text == text.lowercased(),
              let uuid = UUID(uuidString: text),
              uuid != UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)) else {
            return nil
        }
        return uuid
    }

    private static func strictInteger(_ value: Any?) -> UInt64? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else {
            return nil
        }
        let text = number.stringValue
        guard let parsed = UInt64(text), String(parsed) == text else {
            return nil
        }
        return parsed
    }
}

struct AssemblywrightMacMLXControlReceipt: Equatable, Sendable {
    enum Status: String, Sendable {
        case successObserved = "mlx_success_observed"
        case cancellationLeased = "mlx_cancellation_leased"
        case cancellationObserved = "mlx_cancellation_observed"
        case emergencyResumed = "mlx_emergency_resumed"
    }

    static let maximumBytes = 4 * 1_024

    let status: Status
    let taskID: UUID?
    let stepID: UUID?
    let streamID: UUID?
    let deviceID: UUID?
    let connectionEpoch: UInt64?
    let queuedSequence: UInt64?
    let leasedSequence: UInt64?
    let succeededSequence: UInt64?
    let requestedSequence: UInt64?
    let acknowledgedSequence: UInt64?
    let cancelledSequence: UInt64?
    let lateOutputWindowMilliseconds: UInt64?

    static func decodeStrict(_ data: Data) throws -> Self {
        var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
        let rawKeys = try scanner.scanTopLevelKeys()
        guard !data.isEmpty,
              data.count <= maximumBytes,
              Set(rawKeys).count == rawKeys.count,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              strictInteger(object["schema_version"]) == 1,
              let statusText = object["status"] as? String,
              let status = Status(rawValue: statusText) else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }

        let successKeys = Set([
            "schema_version", "status", "task_id", "step_id", "stream_id",
            "device_id", "connection_epoch", "queued_sequence",
            "leased_sequence", "succeeded_sequence"
        ])
        let leasedKeys = Set([
            "schema_version", "status", "task_id", "step_id", "stream_id",
            "device_id", "connection_epoch", "queued_sequence", "leased_sequence"
        ])
        let cancellationKeys = Set([
            "schema_version", "status", "task_id", "step_id", "stream_id",
            "device_id", "connection_epoch", "requested_sequence",
            "acknowledged_sequence", "cancelled_sequence", "late_output_window_ms"
        ])
        let resumeKeys = Set(["schema_version", "status"])

        let taskID = strictUUID(object["task_id"])
        let stepID = strictUUID(object["step_id"])
        let streamID = strictUUID(object["stream_id"])
        let deviceID = strictUUID(object["device_id"])
        let connectionEpoch = strictInteger(object["connection_epoch"])
        let queued = strictInteger(object["queued_sequence"])
        let leased = strictInteger(object["leased_sequence"])
        let succeeded = strictInteger(object["succeeded_sequence"])
        let requested = strictInteger(object["requested_sequence"])
        let acknowledged = strictInteger(object["acknowledged_sequence"])
        let cancelled = strictInteger(object["cancelled_sequence"])
        let lateOutputWindow = strictInteger(object["late_output_window_ms"])

        switch status {
        case .successObserved:
            guard Set(object.keys) == successKeys,
                  taskID != nil, stepID != nil, streamID != nil, deviceID != nil,
                  connectionEpoch.map({ $0 > 0 }) == true,
                  let queued, let leased, let succeeded,
                  queued > 0, queued < leased, leased < succeeded else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        case .cancellationLeased:
            guard Set(object.keys) == leasedKeys,
                  taskID != nil, stepID != nil, streamID != nil, deviceID != nil,
                  connectionEpoch.map({ $0 > 0 }) == true,
                  let queued, let leased,
                  queued > 0, queued < leased else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        case .cancellationObserved:
            guard Set(object.keys) == cancellationKeys,
                  taskID != nil, stepID != nil, streamID != nil, deviceID != nil,
                  connectionEpoch.map({ $0 > 0 }) == true,
                  let requested, let acknowledged, let cancelled,
                  requested > 0, requested < acknowledged, acknowledged < cancelled,
                  lateOutputWindow == 7_000 else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        case .emergencyResumed:
            guard Set(object.keys) == resumeKeys else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
        }

        return Self(
            status: status,
            taskID: taskID,
            stepID: stepID,
            streamID: streamID,
            deviceID: deviceID,
            connectionEpoch: connectionEpoch,
            queuedSequence: queued,
            leasedSequence: leased,
            succeededSequence: succeeded,
            requestedSequence: requested,
            acknowledgedSequence: acknowledged,
            cancelledSequence: cancelled,
            lateOutputWindowMilliseconds: lateOutputWindow
        )
    }

    private static func strictUUID(_ value: Any?) -> UUID? {
        guard let text = value as? String,
              text == text.lowercased(),
              let uuid = UUID(uuidString: text),
              uuid != UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)) else {
            return nil
        }
        return uuid
    }

    private static func strictInteger(_ value: Any?) -> UInt64? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else {
            return nil
        }
        let text = number.stringValue
        guard let parsed = UInt64(text), String(parsed) == text else {
            return nil
        }
        return parsed
    }
}

public actor AssemblywrightMacBridgeSupervisor {
    public static let healthPath = "/health"
    public static let featureConveyorPath = "/v1/distributed/feature-conveyor/status"
    public static let healthMaximumBytes = 64 * 1_024
    public static let featureConveyorMaximumBytes = 64 * 1_024
    public static let normalPollDelayMilliseconds: UInt64 = 5_000
    public static let maximumBackoffMilliseconds: UInt64 = 30_000

    private let profile: AssemblywrightMacBridgeProfile
    private let connector: any AssemblywrightMacBridgeConnecting
    private let eventRelay: (any AssemblywrightMacBridgeEventRelaying)?
    private var session: (any AssemblywrightMacBridgeSession)?
    private var consecutiveFailures: UInt32 = 0
    private var stopped = false

    public init(
        profile: AssemblywrightMacBridgeProfile,
        connector: any AssemblywrightMacBridgeConnecting = AssemblywrightMacDefaultBridgeConnector(),
        eventRelay: (any AssemblywrightMacBridgeEventRelaying)? = nil
    ) {
        self.profile = profile
        self.connector = connector
        self.eventRelay = eventRelay
    }

    public func sample() async -> AssemblywrightMacBridgeSupervisorSnapshot {
        guard !stopped else { return stoppedSnapshot() }
        do {
            let activeSession: any AssemblywrightMacBridgeSession
            if let session {
                activeSession = session
            } else {
                activeSession = try await connector.connect(profile: profile)
                session = activeSession
            }
            let response = try await activeSession.send(
                AssemblywrightMacBridgeHTTPRequest(method: "GET", path: Self.healthPath)
            )
            let health = try AssemblywrightMacRemoteMasterHealth.decode(response)
            let featureResponse = try await activeSession.send(
                AssemblywrightMacBridgeHTTPRequest(
                    method: "GET",
                    path: Self.featureConveyorPath
                )
            )
            let featureConveyor = try AssemblywrightMacRemoteFeatureConveyorStatus.decode(
                featureResponse
            )
            guard health.schemaVersion == Int64(featureConveyor.schemaVersion),
                  health.emergencyPaused
                    == (featureConveyor.ownerGuidance.reasonCode == .emergencyPaused) else {
                throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
            }
            if let eventRelay {
                let progress = try await eventRelay.relayEvents(using: activeSession)
                if progress.requiresFreshConnection {
                    await activeSession.cancel()
                    session = nil
                }
            }
            consecutiveFailures = 0
            return AssemblywrightMacBridgeSupervisorSnapshot(
                phase: .authenticated,
                deviceID: profile.deviceID,
                masterEndpoint: profile.masterEndpoint,
                connectionEpoch: activeSession.connectionEpoch,
                consecutiveFailures: 0,
                nextDelayMilliseconds: Self.normalPollDelayMilliseconds,
                masterStatus: health.status,
                maintenanceActive: health.maintenanceActive,
                emergencyPaused: health.emergencyPaused,
                protocolVersion: health.protocolVersion,
                schemaVersion: health.schemaVersion,
                featureConveyor: featureConveyor,
                errorCode: nil
            )
        } catch is CancellationError {
            await stop()
            return stoppedSnapshot()
        } catch {
            if let session { await session.cancel() }
            session = nil
            consecutiveFailures = consecutiveFailures.saturatingIncremented()
            return AssemblywrightMacBridgeSupervisorSnapshot(
                phase: .backingOff,
                deviceID: profile.deviceID,
                masterEndpoint: profile.masterEndpoint,
                connectionEpoch: nil,
                consecutiveFailures: consecutiveFailures,
                nextDelayMilliseconds: Self.backoffMilliseconds(for: consecutiveFailures),
                masterStatus: nil,
                maintenanceActive: nil,
                emergencyPaused: nil,
                protocolVersion: nil,
                schemaVersion: nil,
                featureConveyor: nil,
                errorCode: Self.redactedErrorCode(for: error)
            )
        }
    }

    public func stop() async {
        stopped = true
        if let session { await session.cancel() }
        session = nil
    }

    /// Cancels only the current authenticated connection. The next sample must
    /// establish a new TLS/exporter-bound application session.
    public func reconnectBeforeNextSample() async {
        guard !stopped else { return }
        if let session { await session.cancel() }
        session = nil
    }

    public static func backoffMilliseconds(for consecutiveFailures: UInt32) -> UInt64 {
        guard consecutiveFailures > 0 else { return normalPollDelayMilliseconds }
        let exponent = min(consecutiveFailures - 1, 5)
        return min(UInt64(1_000) << exponent, maximumBackoffMilliseconds)
    }

    private func stoppedSnapshot() -> AssemblywrightMacBridgeSupervisorSnapshot {
        AssemblywrightMacBridgeSupervisorSnapshot(
            phase: .stopped,
            deviceID: profile.deviceID,
            masterEndpoint: profile.masterEndpoint,
            connectionEpoch: nil,
            consecutiveFailures: consecutiveFailures,
            nextDelayMilliseconds: 0,
            masterStatus: nil,
            maintenanceActive: nil,
            emergencyPaused: nil,
            protocolVersion: nil,
            schemaVersion: nil,
            featureConveyor: nil,
            errorCode: nil
        )
    }

    private static func redactedErrorCode(for error: Error) -> String {
        if error is AssemblywrightMacRemoteMasterHealthError { return "invalid_health" }
        if error is AssemblywrightMacRemoteFeatureConveyorStatusError {
            return "invalid_feature_conveyor_status"
        }
        if error is AssemblywrightMacDeveloperBridgeError { return "bridge_unavailable" }
        if error is AssemblywrightMacDeveloperEventRelayError { return "event_relay_failed" }
        return "connection_failed"
    }
}

private enum AssemblywrightMacRemoteMasterHealthError: Error {
    case invalid
}

private enum AssemblywrightMacRemoteFeatureConveyorStatusError: Error {
    case invalid
}

private enum AssemblywrightMacRemoteFeatureConveyorStatus {
    static func decode(
        _ response: AssemblywrightMacBridgeHTTPResponse
    ) throws -> AssemblywrightMacFeatureConveyorStatus {
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= AssemblywrightMacBridgeSupervisor.featureConveyorMaximumBytes else {
            throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
        }
        do {
            var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: response.body)
            try scanner.validateNoDuplicateObjectKeysRecursively()
        } catch {
            throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
        }
        guard let object = try? JSONSerialization.jsonObject(with: response.body) as? [String: Any]
        else {
            throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
        }
        let status: AssemblywrightMacFeatureConveyorStatus
        do {
            status = try JSONDecoder().decode(
                AssemblywrightMacFeatureConveyorStatus.self,
                from: response.body
            )
            try validate(status, object: object)
        } catch {
            throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
        }
        return status
    }

    static func validate(
        _ status: AssemblywrightMacFeatureConveyorStatus,
        object: [String: Any]
    ) throws {
        guard Set(object.keys) == Set(AssemblywrightMacFeatureConveyorStatus.CodingKeys.all),
              strictInteger(object["schema_version"])
                == AssemblywrightMacFeatureConveyorStatus.expectedSchemaVersion,
              strictInteger(object["queue_revision"]) == status.queueRevision,
              strictInteger(object["startup_quarantine_count"])
                == status.startupQuarantineCount,
              strictInteger(object["visible_feature_count"]) == status.visibleFeatureCount,
              strictBoolean(object["features_truncated"]) == status.featuresTruncated,
              let countObject = object["counts_by_status"] as? [String: Any],
              let featureObjects = object["features"] as? [[String: Any]],
              let guidanceObject = object["owner_guidance"] as? [String: Any],
              Set(countObject.keys)
                == Set(AssemblywrightMacFeatureConveyorStatus.Counts.CodingKeys.all),
              featureObjects.count == status.features.count,
              Set(guidanceObject.keys)
                == Set(AssemblywrightMacFeatureConveyorStatus.OwnerGuidance.CodingKeys.all),
              status.schemaVersion
                == AssemblywrightMacFeatureConveyorStatus.expectedSchemaVersion,
              status.startupQuarantineCount <= 1,
              status.visibleFeatureCount
                <= AssemblywrightMacFeatureConveyorStatus.maximumVisibleFeatures,
              status.features.count <= AssemblywrightMacFeatureConveyorStatus.maximumFeatures,
              status.featuresTruncated
                == (status.visibleFeatureCount > UInt64(status.features.count)),
              !status.featuresTruncated
                || status.features.count == AssemblywrightMacFeatureConveyorStatus.maximumFeatures,
              status.featuresTruncated || status.visibleFeatureCount == UInt64(status.features.count),
              status.countsByStatus.succeeded == 0,
              status.countsByStatus.abandoned == 0 else {
            throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
        }

        for (key, expected) in zip(
            AssemblywrightMacFeatureConveyorStatus.Counts.CodingKeys.all,
            status.countsByStatus.orderedValues
        ) where strictInteger(countObject[key]) != expected {
            throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
        }
        var total: UInt64 = 0
        for value in status.countsByStatus.orderedValues {
            let addition = total.addingReportingOverflow(value)
            guard !addition.overflow else {
                throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
            }
            total = addition.partialValue
        }
        guard total == status.visibleFeatureCount,
              total - status.countsByStatus.queued <= 1 else {
            throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
        }

        var featureIDs = Set<UUID>()
        var previousQueuePosition: UInt64?
        var observedCounts: [AssemblywrightMacFeatureConveyorLifecycleStatus: UInt64] = [:]
        for (feature, featureObject) in zip(status.features, featureObjects) {
            guard Set(featureObject.keys)
                    == Set(AssemblywrightMacFeatureConveyorStatus.Feature.CodingKeys.all),
                  strictUUID(featureObject["feature_id"]) == feature.featureID,
                  strictInteger(featureObject["specification_revision"])
                    == feature.specificationRevision,
                  strictInteger(featureObject["lifecycle_revision"]) == feature.lifecycleRevision,
                  strictInteger(featureObject["queue_position"]) == feature.queuePosition,
                  featureObject["status"] as? String == feature.status.rawValue,
                  strictBoolean(featureObject["lease_present"]) == feature.leasePresent,
                  strictBoolean(featureObject["effect_possible"]) == feature.effectPossible,
                  feature.specificationRevision > 0,
                  feature.lifecycleRevision > 0,
                  feature.queuePosition > 0,
                  previousQueuePosition.map({ $0 < feature.queuePosition }) ?? true,
                  featureIDs.insert(feature.featureID).inserted else {
                throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
            }
            switch feature.status {
            case .queued:
                guard !feature.leasePresent, !feature.effectPossible else {
                    throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
                }
            case .implementing, .validating, .reviewing, .publishing, .verifyingMain:
                guard feature.leasePresent else {
                    throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
                }
            case .cancelled, .quarantined:
                guard feature.leasePresent, feature.effectPossible else {
                    throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
                }
            case .succeeded, .abandoned:
                throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
            }
            observedCounts[feature.status, default: 0] += 1
            previousQueuePosition = feature.queuePosition
        }
        for (observedStatus, observedCount) in observedCounts {
            guard observedCount <= status.countsByStatus.count(for: observedStatus) else {
                throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
            }
        }
        if !status.featuresTruncated {
            for lifecycleStatus in [
                AssemblywrightMacFeatureConveyorLifecycleStatus.queued,
                .implementing, .validating, .reviewing, .publishing, .verifyingMain,
                .cancelled, .quarantined
            ] {
                guard observedCounts[lifecycleStatus, default: 0]
                        == status.countsByStatus.count(for: lifecycleStatus) else {
                    throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
                }
            }
        }

        let guidance = status.ownerGuidance
        guard guidance.queueRevision == status.queueRevision,
              strictInteger(guidanceObject["queue_revision"]) == guidance.queueRevision,
              strictInteger(guidanceObject["emergency_pause_revision"])
                == guidance.emergencyPauseRevision,
              guidanceObject["state"] as? String == guidance.state.rawValue,
              guidanceObject["reason_code"] as? String == guidance.reasonCode.rawValue,
              guidanceObject["next_owner_action"] as? String
                == guidance.nextOwnerAction.rawValue,
              strictOptionalUUID(guidanceObject["feature_id"]) == guidance.featureID,
              strictOptionalInteger(guidanceObject["specification_revision"])
                == guidance.specificationRevision,
              strictOptionalInteger(guidanceObject["lifecycle_revision"])
                == guidance.lifecycleRevision else {
            throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
        }

        let hasNoIdentity = guidance.featureID == nil
            && guidance.specificationRevision == nil
            && guidance.lifecycleRevision == nil
        let hasCompleteIdentity = guidance.featureID != nil
            && guidance.specificationRevision.map({ $0 > 0 }) == true
            && guidance.lifecycleRevision.map({ $0 > 0 }) == true
        switch (guidance.state, guidance.reasonCode, guidance.nextOwnerAction) {
        case (.idle, .queueEmpty, .prepareApprovedFeature):
            guard hasNoIdentity, status.visibleFeatureCount == 0 else {
                throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
            }
        case (.blocked, .emergencyPaused, .resumeEmergencyPause):
            guard hasNoIdentity else {
                throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
            }
        case (.ready, .headDependencySatisfied, .awaitOwnerControlSurface),
             (.blocked, .headDependencyUnsatisfied, .resolveHeadDependency):
            guard hasCompleteIdentity,
                  let feature = status.features.first,
                  feature.featureID == guidance.featureID,
                  feature.specificationRevision == guidance.specificationRevision,
                  feature.lifecycleRevision == guidance.lifecycleRevision,
                  feature.status == .queued,
                  !feature.leasePresent else {
                throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
            }
        case (.blocked, .activeRequiresReconciliation, .reconcileActiveFeature):
            guard hasCompleteIdentity,
                  let feature = status.features.first,
                  feature.featureID == guidance.featureID,
                  feature.specificationRevision == guidance.specificationRevision,
                  feature.lifecycleRevision == guidance.lifecycleRevision,
                  [.cancelled, .quarantined].contains(feature.status),
                  feature.leasePresent,
                  feature.effectPossible else {
                throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
            }
        case (.inProgress, .activeFeatureLeased, .wait):
            guard hasCompleteIdentity,
                  let feature = status.features.first,
                  feature.featureID == guidance.featureID,
                  feature.specificationRevision == guidance.specificationRevision,
                  feature.lifecycleRevision == guidance.lifecycleRevision,
                  [
                      AssemblywrightMacFeatureConveyorLifecycleStatus.implementing,
                      .validating, .reviewing, .publishing, .verifyingMain
                  ].contains(feature.status),
                  feature.leasePresent else {
                throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
            }
        default:
            throw AssemblywrightMacRemoteFeatureConveyorStatusError.invalid
        }
    }

    private static func strictBoolean(_ value: Any?) -> Bool? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) == CFBooleanGetTypeID() else {
            return nil
        }
        return number.boolValue
    }

    private static func strictInteger(_ value: Any?) -> UInt64? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else {
            return nil
        }
        let text = number.stringValue
        guard let parsed = UInt64(text), String(parsed) == text else { return nil }
        return parsed
    }

    private static func strictOptionalInteger(_ value: Any?) -> UInt64?? {
        if value is NSNull { return .some(nil) }
        guard let integer = strictInteger(value) else { return nil }
        return .some(integer)
    }

    private static func strictUUID(_ value: Any?) -> UUID? {
        guard let text = value as? String,
              text == text.lowercased(),
              let uuid = UUID(uuidString: text),
              uuid.uuidString.lowercased() == text,
              uuid != UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)) else {
            return nil
        }
        return uuid
    }

    private static func strictOptionalUUID(_ value: Any?) -> UUID?? {
        if value is NSNull { return .some(nil) }
        guard let uuid = strictUUID(value) else { return nil }
        return .some(uuid)
    }
}

private struct AssemblywrightMacRemoteMasterHealth: Decodable {
    let status: String
    let mode: String
    let hostMode: String
    let serviceIdentity: String
    let maintenanceActive: Bool
    let maintenanceReason: String?
    let emergencyPaused: Bool
    let protocolVersion: UInt16
    let schemaVersion: Int64
    let processID: UInt32
    let startedAtMilliseconds: UInt64
    let startupReconciliation: StartupReconciliation
    let state: State
    let boundary: String

    struct StartupReconciliation: Decodable {
        let disconnectedConnections: UInt64
        let abandonedAttempts: UInt64
        let requeuedSteps: UInt64

        enum CodingKeys: String, CodingKey {
            case disconnectedConnections = "disconnected_connections"
            case abandonedAttempts = "abandoned_attempts"
            case requeuedSteps = "requeued_steps"
        }
    }

    struct State: Decodable {
        let registeredDevices: UInt64
        let activeDeviceCertificates: UInt64
        let unconsumedEnrollmentGrants: UInt64
        let activeConnections: UInt64
        let queuedSteps: UInt64
        let leasedSteps: UInt64
        let terminalSteps: UInt64
        let activeAttempts: UInt64

        enum CodingKeys: String, CodingKey {
            case registeredDevices = "registered_devices"
            case activeDeviceCertificates = "active_device_certificates"
            case unconsumedEnrollmentGrants = "unconsumed_enrollment_grants"
            case activeConnections = "active_connections"
            case queuedSteps = "queued_steps"
            case leasedSteps = "leased_steps"
            case terminalSteps = "terminal_steps"
            case activeAttempts = "active_attempts"
        }
    }

    enum CodingKeys: String, CodingKey {
        case status
        case mode
        case hostMode = "host_mode"
        case serviceIdentity = "service_identity"
        case maintenanceActive = "maintenance_active"
        case maintenanceReason = "maintenance_reason"
        case emergencyPaused = "emergency_paused"
        case protocolVersion = "protocol_version"
        case schemaVersion = "schema_version"
        case processID = "process_id"
        case startedAtMilliseconds = "started_at_ms"
        case startupReconciliation = "startup_reconciliation"
        case state
        case boundary
    }

    static func decode(_ response: AssemblywrightMacBridgeHTTPResponse) throws -> Self {
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= AssemblywrightMacBridgeSupervisor.healthMaximumBytes,
              let topLevel = try JSONSerialization.jsonObject(with: response.body) as? [String: Any],
              Set(topLevel.keys) == Set(CodingKeys.all),
              let startup = topLevel["startup_reconciliation"] as? [String: Any],
              Set(startup.keys) == Set(StartupReconciliation.CodingKeys.all),
              let state = topLevel["state"] as? [String: Any],
              Set(state.keys) == Set(State.CodingKeys.all) else {
            throw AssemblywrightMacRemoteMasterHealthError.invalid
        }
        let health: Self
        do {
            health = try JSONDecoder().decode(Self.self, from: response.body)
        } catch {
            throw AssemblywrightMacRemoteMasterHealthError.invalid
        }
        let expectedStatus = health.maintenanceActive
            ? "maintenance"
            : health.emergencyPaused ? "paused" : "ok"
        guard health.status == expectedStatus,
              health.maintenanceActive || health.maintenanceReason == nil,
              health.mode == "developer_remote_master",
              !health.hostMode.isEmpty,
              !health.serviceIdentity.isEmpty,
              health.protocolVersion == AssemblywrightMacMTLSBridgeTransport.protocolVersion,
              health.schemaVersion > 0,
              health.processID > 0,
              health.startedAtMilliseconds > 0,
              !health.boundary.isEmpty else {
            throw AssemblywrightMacRemoteMasterHealthError.invalid
        }
        return health
    }
}

private extension CodingKey where Self: CaseIterable {
    static var all: [String] { allCases.map(\.stringValue) }
}

extension AssemblywrightMacRemoteMasterHealth.CodingKeys: CaseIterable {}
extension AssemblywrightMacRemoteMasterHealth.StartupReconciliation.CodingKeys: CaseIterable {}
extension AssemblywrightMacRemoteMasterHealth.State.CodingKeys: CaseIterable {}

private extension UInt32 {
    func saturatingIncremented() -> UInt32 {
        self == .max ? self : self + 1
    }
}
