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
        case protocolVersion = "protocol_version"
        case schemaVersion = "schema_version"
        case errorCode = "error_code"
    }
}

public actor JarvisMacBridgeSupervisor {
    public static let healthPath = "/health"
    public static let healthMaximumBytes = 64 * 1_024
    public static let normalPollDelayMilliseconds: UInt64 = 5_000
    public static let maximumBackoffMilliseconds: UInt64 = 30_000

    private let profile: JarvisMacBridgeProfile
    private let connector: any JarvisMacBridgeConnecting
    private var session: (any JarvisMacBridgeSession)?
    private var consecutiveFailures: UInt32 = 0
    private var stopped = false

    public init(
        profile: JarvisMacBridgeProfile,
        connector: any JarvisMacBridgeConnecting = JarvisMacDefaultBridgeConnector()
    ) {
        self.profile = profile
        self.connector = connector
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
            protocolVersion: nil,
            schemaVersion: nil,
            errorCode: nil
        )
    }

    private static func redactedErrorCode(for error: Error) -> String {
        if error is JarvisMacRemoteMasterHealthError { return "invalid_health" }
        if error is JarvisMacDeveloperBridgeError { return "bridge_unavailable" }
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
        let expectedStatus = health.maintenanceActive ? "maintenance" : "ok"
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
