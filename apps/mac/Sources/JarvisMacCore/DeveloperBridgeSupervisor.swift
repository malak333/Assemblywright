import Foundation

public protocol JarvisMacBridgeSession: Sendable {
    var connectionEpoch: UInt64 { get }
    func send(_ request: JarvisMacBridgeHTTPRequest) async throws -> JarvisMacBridgeHTTPResponse
    func cancel() async
}

extension JarvisMacAuthenticatedBridgeSession: JarvisMacBridgeSession {}

public protocol JarvisMacBridgeConnecting: Sendable {
    func connect(profile: JarvisMacBridgeProfile) async throws -> any JarvisMacBridgeSession
}

public struct JarvisMacDefaultBridgeConnector: JarvisMacBridgeConnecting, Sendable {
    private let transport: JarvisMacMTLSBridgeTransport

    public init(transport: JarvisMacMTLSBridgeTransport = JarvisMacMTLSBridgeTransport()) {
        self.transport = transport
    }

    public func connect(profile: JarvisMacBridgeProfile) async throws -> any JarvisMacBridgeSession {
        try await transport.connect(profile: profile)
    }
}

public enum JarvisMacBridgeSupervisorPhase: String, Codable, Equatable, Sendable {
    case authenticated
    case backingOff = "backing_off"
    case stopped
}

public struct JarvisMacBridgeSupervisorSnapshot: Codable, Equatable, Sendable {
    public let phase: JarvisMacBridgeSupervisorPhase
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
        case errorCode = "error_code"
    }

    public static func decodeStrict(_ data: Data) throws -> Self {
        var keyScanner = JarvisStrictJSONObjectKeyScanner(data: data)
        let rawKeys = try keyScanner.scanTopLevelKeys()
        guard Set(rawKeys).count == rawKeys.count else {
            throw JarvisDeveloperBridgeProcessError.invalidSnapshot
        }
        guard !data.isEmpty,
              data.count <= JarvisDeveloperBridgeProcessLifecycle.maximumLineBytes,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let phaseText = object["phase"] as? String,
              let phase = JarvisMacBridgeSupervisorPhase(rawValue: phaseText) else {
            throw JarvisDeveloperBridgeProcessError.invalidSnapshot
        }
        let authenticated = Set([
            "phase", "device_id", "master_endpoint", "connection_epoch",
            "consecutive_failures", "next_delay_ms", "master_status",
            "maintenance_active", "emergency_paused", "protocol_version",
            "schema_version"
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
            throw JarvisDeveloperBridgeProcessError.invalidSnapshot
        }
        let snapshot: Self
        do {
            snapshot = try JSONDecoder().decode(Self.self, from: data)
        } catch {
            throw JarvisDeveloperBridgeProcessError.invalidSnapshot
        }
        let canonicalUUID = UUID(uuidString: snapshot.deviceID)?.uuidString.lowercased()
        guard canonicalUUID == snapshot.deviceID.lowercased(),
              !snapshot.masterEndpoint.isEmpty,
              snapshot.masterEndpoint.utf8.count <= 512,
              !snapshot.masterEndpoint.contains(where: { $0.isWhitespace || $0.isNewline }) else {
            throw JarvisDeveloperBridgeProcessError.invalidSnapshot
        }
        switch snapshot.phase {
        case .authenticated:
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
                  snapshot.protocolVersion == JarvisMacMTLSBridgeTransport.protocolVersion,
                  snapshot.schemaVersion.map({ $0 > 0 }) == true,
                  snapshot.errorCode == nil else {
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
            }
        case .backingOff:
            guard snapshot.connectionEpoch == nil,
                  snapshot.consecutiveFailures > 0,
                  (1 ... JarvisMacBridgeSupervisor.maximumBackoffMilliseconds)
                    .contains(snapshot.nextDelayMilliseconds),
                  [
                      "invalid_health", "bridge_unavailable", "connection_failed",
                      "event_relay_failed"
                  ]
                    .contains(snapshot.errorCode),
                  snapshot.masterStatus == nil,
                  snapshot.maintenanceActive == nil,
                  snapshot.emergencyPaused == nil,
                  snapshot.protocolVersion == nil,
                  snapshot.schemaVersion == nil else {
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
            }
        case .stopped:
            guard snapshot.connectionEpoch == nil,
                  snapshot.nextDelayMilliseconds == 0,
                  snapshot.masterStatus == nil,
                  snapshot.maintenanceActive == nil,
                  snapshot.emergencyPaused == nil,
                  snapshot.protocolVersion == nil,
                  snapshot.schemaVersion == nil,
                  snapshot.errorCode == nil else {
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
            }
        }
        return snapshot
    }
}

struct JarvisStrictJSONObjectKeyScanner {
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
                    throw JarvisDeveloperBridgeProcessError.invalidSnapshot
                }
            }
            if byte < 0x20 {
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
            }
        }
        throw JarvisDeveloperBridgeProcessError.invalidSnapshot
    }

    private mutating func skipValue() throws {
        skipWhitespace()
        guard index < bytes.count else {
            throw JarvisDeveloperBridgeProcessError.invalidSnapshot
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
            throw JarvisDeveloperBridgeProcessError.invalidSnapshot
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
                    throw JarvisDeveloperBridgeProcessError.invalidSnapshot
                }
                if stack.isEmpty { return }
            default:
                break
            }
        }
        throw JarvisDeveloperBridgeProcessError.invalidSnapshot
    }

    private mutating func skipWhitespace() {
        while index < bytes.count, Self.isWhitespace(bytes[index]) { index += 1 }
    }

    private mutating func consume(_ expected: UInt8) throws {
        guard consumeIfPresent(expected) else {
            throw JarvisDeveloperBridgeProcessError.invalidSnapshot
        }
    }

    private mutating func consumeIfPresent(_ expected: UInt8) -> Bool {
        guard index < bytes.count, bytes[index] == expected else { return false }
        index += 1
        return true
    }

    private func requireEnd() throws {
        guard index == bytes.count else {
            throw JarvisDeveloperBridgeProcessError.invalidSnapshot
        }
    }

    private static func isWhitespace(_ byte: UInt8) -> Bool {
        byte == 0x20 || byte == 0x09 || byte == 0x0a || byte == 0x0d
    }
}

struct JarvisMacFixtureControlReceipt: Equatable, Sendable {
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
        var scanner = JarvisStrictJSONObjectKeyScanner(data: data)
        let rawKeys = try scanner.scanTopLevelKeys()
        guard !data.isEmpty,
              data.count <= maximumBytes,
              Set(rawKeys).count == rawKeys.count,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              strictInteger(object["schema_version"]) == 1,
              let statusText = object["status"] as? String,
              let status = Status(rawValue: statusText) else {
            throw JarvisDeveloperBridgeProcessError.invalidSnapshot
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
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
            }
        case .cancellationLeased:
            guard Set(object.keys) == leasedKeys,
                  taskID != nil, stepID != nil, streamID != nil,
                  let queued, let leased,
                  queued > 0, queued < leased else {
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
            }
        case .cancellationObserved:
            guard Set(object.keys) == cancellationKeys,
                  taskID != nil, stepID != nil, streamID != nil,
                  let requested, let acknowledged, let cancelled,
                  requested > 0, requested < acknowledged, acknowledged < cancelled,
                  lateOutputWindow == 7_000 else {
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
            }
        case .emergencyResumed:
            guard Set(object.keys) == resumeKeys else {
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
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

struct JarvisMacMLXControlReceipt: Equatable, Sendable {
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
        var scanner = JarvisStrictJSONObjectKeyScanner(data: data)
        let rawKeys = try scanner.scanTopLevelKeys()
        guard !data.isEmpty,
              data.count <= maximumBytes,
              Set(rawKeys).count == rawKeys.count,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              strictInteger(object["schema_version"]) == 1,
              let statusText = object["status"] as? String,
              let status = Status(rawValue: statusText) else {
            throw JarvisDeveloperBridgeProcessError.invalidSnapshot
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
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
            }
        case .cancellationLeased:
            guard Set(object.keys) == leasedKeys,
                  taskID != nil, stepID != nil, streamID != nil, deviceID != nil,
                  connectionEpoch.map({ $0 > 0 }) == true,
                  let queued, let leased,
                  queued > 0, queued < leased else {
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
            }
        case .cancellationObserved:
            guard Set(object.keys) == cancellationKeys,
                  taskID != nil, stepID != nil, streamID != nil, deviceID != nil,
                  connectionEpoch.map({ $0 > 0 }) == true,
                  let requested, let acknowledged, let cancelled,
                  requested > 0, requested < acknowledged, acknowledged < cancelled,
                  lateOutputWindow == 7_000 else {
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
            }
        case .emergencyResumed:
            guard Set(object.keys) == resumeKeys else {
                throw JarvisDeveloperBridgeProcessError.invalidSnapshot
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

public actor JarvisMacBridgeSupervisor {
    public static let healthPath = "/health"
    public static let healthMaximumBytes = 64 * 1_024
    public static let normalPollDelayMilliseconds: UInt64 = 5_000
    public static let maximumBackoffMilliseconds: UInt64 = 30_000

    private let profile: JarvisMacBridgeProfile
    private let connector: any JarvisMacBridgeConnecting
    private let eventRelay: (any JarvisMacBridgeEventRelaying)?
    private var session: (any JarvisMacBridgeSession)?
    private var consecutiveFailures: UInt32 = 0
    private var stopped = false

    public init(
        profile: JarvisMacBridgeProfile,
        connector: any JarvisMacBridgeConnecting = JarvisMacDefaultBridgeConnector(),
        eventRelay: (any JarvisMacBridgeEventRelaying)? = nil
    ) {
        self.profile = profile
        self.connector = connector
        self.eventRelay = eventRelay
    }

    public func sample() async -> JarvisMacBridgeSupervisorSnapshot {
        guard !stopped else { return stoppedSnapshot() }
        do {
            let activeSession: any JarvisMacBridgeSession
            if let session {
                activeSession = session
            } else {
                activeSession = try await connector.connect(profile: profile)
                session = activeSession
            }
            let response = try await activeSession.send(
                JarvisMacBridgeHTTPRequest(method: "GET", path: Self.healthPath)
            )
            let health = try JarvisMacRemoteMasterHealth.decode(response)
            if let eventRelay {
                let progress = try await eventRelay.relayEvents(using: activeSession)
                if progress.requiresFreshConnection {
                    await activeSession.cancel()
                    session = nil
                }
            }
            consecutiveFailures = 0
            return JarvisMacBridgeSupervisorSnapshot(
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
                errorCode: nil
            )
        } catch is CancellationError {
            await stop()
            return stoppedSnapshot()
        } catch {
            if let session { await session.cancel() }
            session = nil
            consecutiveFailures = consecutiveFailures.saturatingIncremented()
            return JarvisMacBridgeSupervisorSnapshot(
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

    private func stoppedSnapshot() -> JarvisMacBridgeSupervisorSnapshot {
        JarvisMacBridgeSupervisorSnapshot(
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
            errorCode: nil
        )
    }

    private static func redactedErrorCode(for error: Error) -> String {
        if error is JarvisMacRemoteMasterHealthError { return "invalid_health" }
        if error is JarvisMacDeveloperBridgeError { return "bridge_unavailable" }
        if error is JarvisMacDeveloperEventRelayError { return "event_relay_failed" }
        return "connection_failed"
    }
}

private enum JarvisMacRemoteMasterHealthError: Error {
    case invalid
}

private struct JarvisMacRemoteMasterHealth: Decodable {
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

    static func decode(_ response: JarvisMacBridgeHTTPResponse) throws -> Self {
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= JarvisMacBridgeSupervisor.healthMaximumBytes,
              let topLevel = try JSONSerialization.jsonObject(with: response.body) as? [String: Any],
              Set(topLevel.keys) == Set(CodingKeys.all),
              let startup = topLevel["startup_reconciliation"] as? [String: Any],
              Set(startup.keys) == Set(StartupReconciliation.CodingKeys.all),
              let state = topLevel["state"] as? [String: Any],
              Set(state.keys) == Set(State.CodingKeys.all) else {
            throw JarvisMacRemoteMasterHealthError.invalid
        }
        let health: Self
        do {
            health = try JSONDecoder().decode(Self.self, from: response.body)
        } catch {
            throw JarvisMacRemoteMasterHealthError.invalid
        }
        let expectedStatus = health.maintenanceActive
            ? "maintenance"
            : health.emergencyPaused ? "paused" : "ok"
        guard health.status == expectedStatus,
              health.maintenanceActive || health.maintenanceReason == nil,
              health.mode == "developer_remote_master",
              !health.hostMode.isEmpty,
              !health.serviceIdentity.isEmpty,
              health.protocolVersion == JarvisMacMTLSBridgeTransport.protocolVersion,
              health.schemaVersion > 0,
              health.processID > 0,
              health.startedAtMilliseconds > 0,
              !health.boundary.isEmpty else {
            throw JarvisMacRemoteMasterHealthError.invalid
        }
        return health
    }
}

private extension CodingKey where Self: CaseIterable {
    static var all: [String] { allCases.map(\.stringValue) }
}

extension JarvisMacRemoteMasterHealth.CodingKeys: CaseIterable {}
extension JarvisMacRemoteMasterHealth.StartupReconciliation.CodingKeys: CaseIterable {}
extension JarvisMacRemoteMasterHealth.State.CodingKeys: CaseIterable {}

private extension UInt32 {
    func saturatingIncremented() -> UInt32 {
        self == .max ? self : self + 1
    }
}
