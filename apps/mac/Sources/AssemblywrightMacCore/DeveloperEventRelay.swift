import Darwin
import CryptoKit
import Foundation
import Security

private actor AssemblywrightMacFixtureRaceResolution {
    private var resolved = false
    private var cancellationInProgress = false

    func markResolved() {
        resolved = true
    }

    func markCancellationInProgress() {
        cancellationInProgress = true
    }

    func isResolved() -> Bool {
        resolved
    }

    func isCancellationInProgress() -> Bool {
        cancellationInProgress
    }
}

public struct AssemblywrightMacDeveloperEventRelayConfiguration: Equatable, Sendable {
    public static let version = 4
    public static let maximumDocumentBytes = 16 * 1_024

    public let agentExecutableURL: URL
    public let agentDataDirectoryURL: URL
    public let fixtureJobsEnabled: Bool
    public let mlxJobsEnabled: Bool
    public let localCodingSnapshotsEnabled: Bool
    public let mlxExecutableURL: URL?
    public let mlxModelDirectoryURL: URL?
    public let mlxModelID: String?

    public init(
        agentExecutableURL: URL,
        agentDataDirectoryURL: URL,
        fixtureJobsEnabled: Bool = false,
        mlxJobsEnabled: Bool = false,
        localCodingSnapshotsEnabled: Bool = false,
        mlxExecutableURL: URL? = nil,
        mlxModelDirectoryURL: URL? = nil,
        mlxModelID: String? = nil
    ) {
        self.agentExecutableURL = agentExecutableURL.standardizedFileURL
        self.agentDataDirectoryURL = agentDataDirectoryURL.standardizedFileURL
        self.fixtureJobsEnabled = fixtureJobsEnabled
        self.mlxJobsEnabled = mlxJobsEnabled
        self.localCodingSnapshotsEnabled = localCodingSnapshotsEnabled
        self.mlxExecutableURL = mlxExecutableURL?.standardizedFileURL
        self.mlxModelDirectoryURL = mlxModelDirectoryURL?.standardizedFileURL
        self.mlxModelID = mlxModelID
    }

    public func encodeStartupDocument() throws -> Data {
        try validatePaths()
        let object: [String: Any] = [
            "version": Self.version,
            "agent_executable_path": agentExecutableURL.path,
            "agent_data_dir": agentDataDirectoryURL.path,
            "fixture_jobs_enabled": fixtureJobsEnabled,
            "mlx_jobs_enabled": mlxJobsEnabled,
            "local_coding_snapshots_enabled": localCodingSnapshotsEnabled,
            "mlx_executable_path": mlxExecutableURL?.path ?? NSNull(),
            "mlx_model_dir": mlxModelDirectoryURL?.path ?? NSNull(),
            "mlx_model_id": mlxModelID ?? NSNull()
        ]
        let data = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        guard data.count <= Self.maximumDocumentBytes else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument
        }
        return data
    }

    public static func decodeStartupDocument(_ data: Data) throws -> Self {
        var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
        guard !data.isEmpty, data.count <= maximumDocumentBytes,
              let keys = try? scanner.scanTopLevelKeys(),
              Set(keys).count == keys.count,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys) == Set([
                  "version", "agent_executable_path", "agent_data_dir",
                  "fixture_jobs_enabled", "mlx_jobs_enabled",
                  "local_coding_snapshots_enabled",
                  "mlx_executable_path", "mlx_model_dir", "mlx_model_id"
              ]),
              let version = object["version"] as? NSNumber,
              CFGetTypeID(version) != CFBooleanGetTypeID(),
              version.stringValue == String(Self.version),
              let executablePath = object["agent_executable_path"] as? String,
              let dataDirectoryPath = object["agent_data_dir"] as? String,
              let fixtureJobsEnabled = object["fixture_jobs_enabled"] as? Bool,
              let mlxJobsEnabled = object["mlx_jobs_enabled"] as? Bool,
              let localCodingSnapshotsEnabled =
                object["local_coding_snapshots_enabled"] as? Bool,
              let mlxExecutablePath = optionalString(object["mlx_executable_path"]),
              let mlxModelDirectoryPath = optionalString(object["mlx_model_dir"]),
              let mlxModelID = optionalString(object["mlx_model_id"]),
              mlxExecutablePath.map(isValidAbsolutePath) ?? true,
              mlxModelDirectoryPath.map(isValidAbsolutePath) ?? true else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument
        }
        let configuration = Self(
            agentExecutableURL: URL(fileURLWithPath: executablePath),
            agentDataDirectoryURL: URL(fileURLWithPath: dataDirectoryPath, isDirectory: true),
            fixtureJobsEnabled: fixtureJobsEnabled,
            mlxJobsEnabled: mlxJobsEnabled,
            localCodingSnapshotsEnabled: localCodingSnapshotsEnabled,
            mlxExecutableURL: mlxExecutablePath.map(URL.init(fileURLWithPath:)),
            mlxModelDirectoryURL: mlxModelDirectoryPath.map {
                URL(fileURLWithPath: $0, isDirectory: true)
            },
            mlxModelID: mlxModelID
        )
        try configuration.validatePaths()
        return configuration
    }

    public func validatePaths() throws {
        guard [fixtureJobsEnabled, mlxJobsEnabled, localCodingSnapshotsEnabled]
                .filter({ $0 }).count <= 1,
              mlxJobsEnabled
                ? mlxExecutableURL != nil
                    && mlxModelDirectoryURL != nil
                    && mlxModelID != nil
                : mlxExecutableURL == nil
                    && mlxModelDirectoryURL == nil
                    && mlxModelID == nil else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument
        }
        for url in [agentExecutableURL, agentDataDirectoryURL]
            + [mlxExecutableURL, mlxModelDirectoryURL].compactMap({ $0 })
        {
            guard url.isFileURL,
                  url.path.hasPrefix("/"),
                  !url.path.contains("\0"),
                  !url.path.split(separator: "/").contains(".."),
                  url.path.utf8.count <= 4 * 1_024 else {
                throw AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument
            }
        }
        if let mlxModelID {
            guard !mlxModelID.isEmpty,
                  mlxModelID.utf8.count <= 128,
                  mlxModelID.utf8.allSatisfy({ (0x20 ... 0x7e).contains($0) }) else {
                throw AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument
            }
        }
    }

    private static func optionalString(_ value: Any?) -> String?? {
        if value is NSNull { return .some(nil) }
        guard let value = value as? String else { return nil }
        return .some(value)
    }

    private static func isValidAbsolutePath(_ value: String) -> Bool {
        value.hasPrefix("/")
            && !value.contains("\0")
            && !value.split(separator: "/").contains("..")
            && value.utf8.count <= 4 * 1_024
    }
}

public enum AssemblywrightMacDeveloperEventRelayError: Error, Equatable, Sendable {
    case invalidStartupDocument
    case invalidAgentExecutable
    case invalidAgentSignature
    case invalidHelperIdentity
    case unsafeRuntimeDirectory
    case randomUnavailable
    case agentLaunchFailed
    case agentIdentityMismatch
    case agentUnavailable
    case invalidAgentResponse
    case invalidMasterResponse
    case eventCursorRejected
    case fixtureJobRejected
    case fixtureJobTimedOut
    case mlxJobRejected
    case mlxJobTimedOut
    case localCodingSnapshotRejected
    case localCodingSnapshotTimedOut
    case teardownFailed
}

public struct AssemblywrightMacDeveloperEventCursor: Codable, Equatable, Sendable {
    public let streamID: UUID
    public let sequence: UInt64

    enum CodingKeys: String, CodingKey {
        case streamID = "stream_id"
        case sequence
    }
}

public struct AssemblywrightMacDeveloperAgentCursorSnapshot: Codable, Equatable, Sendable {
    public let cursor: AssemblywrightMacDeveloperEventCursor?
    public let updatedAtMilliseconds: UInt64?

    enum CodingKeys: String, CodingKey {
        case cursor
        case updatedAtMilliseconds = "updated_at_ms"
    }
}

public enum AssemblywrightMacDeveloperAgentSnapshotChunkAcceptance: Equatable, Sendable {
    case nextOffset(UInt64)
    case result(Data)
}

public struct AssemblywrightMacDeveloperEventRelayProgress: Equatable, Sendable {
    public let cursor: AssemblywrightMacDeveloperEventCursor?
    public let acceptedEventCount: Int
    public let hasMore: Bool
    public let requiresFreshConnection: Bool
}

public enum AssemblywrightMacBridgeEventRelayRoutingMode: String, Equatable, Sendable {
    case metadataOnly = "metadata_only"
    case fixture
    case mlx
    case localCoding = "local_coding"
    case invalid
}

public protocol AssemblywrightMacBridgeEventRelaying: Sendable {
    var routingMode: AssemblywrightMacBridgeEventRelayRoutingMode { get }
    func relayEvents(
        using session: any AssemblywrightMacBridgeSession
    ) async throws -> AssemblywrightMacDeveloperEventRelayProgress
    func stop() async throws
}

public protocol AssemblywrightMacDeveloperAgentSession: Sendable {
    func health() async throws -> AssemblywrightMacDeveloperAgentCursorSnapshot
    func accept(batch: Data) async throws -> AssemblywrightMacDeveloperAgentCursorSnapshot
    func executeFixtureJob(_ job: Data) async throws -> Data
    func cancelFixtureJob(_ instruction: Data) async throws -> Data
    func executeMLXJob(_ job: Data) async throws -> Data
    func cancelMLXJob(_ instruction: Data) async throws -> Data
    func admitLocalCodingSnapshot(_ job: Data) async throws
    func acceptLocalCodingSnapshotChunk(
        _ chunk: Data
    ) async throws -> AssemblywrightMacDeveloperAgentSnapshotChunkAcceptance
    func cancelLocalCodingSnapshot(_ instruction: Data) async throws -> Data
    func stop() async throws
}

public protocol AssemblywrightMacDeveloperAgentLaunching: Sendable {
    func launch(
        configuration: AssemblywrightMacDeveloperEventRelayConfiguration
    ) async throws -> any AssemblywrightMacDeveloperAgentSession
}

actor AssemblywrightMacLocalCodingSessionRequests {
    private let session: any AssemblywrightMacBridgeSession
    private var available = true
    private var waiters: [CheckedContinuation<Void, Never>] = []

    init(session: any AssemblywrightMacBridgeSession) {
        self.session = session
    }

    func send(
        _ request: AssemblywrightMacBridgeHTTPRequest
    ) async throws -> AssemblywrightMacBridgeHTTPResponse {
        try await acquire()
        do {
            let response = try await session.send(request)
            release()
            return response
        } catch {
            release()
            throw error
        }
    }

    private func acquire() async throws {
        try Task.checkCancellation()
        if available {
            available = false
            return
        }
        await withCheckedContinuation { continuation in
            waiters.append(continuation)
        }
        do {
            try Task.checkCancellation()
        } catch {
            release()
            throw error
        }
    }

    private func release() {
        if waiters.isEmpty {
            available = true
        } else {
            waiters.removeFirst().resume()
        }
    }
}

public actor AssemblywrightMacDeveloperEventRelay: AssemblywrightMacBridgeEventRelaying {
    public static let remoteEventsPath = "/v1/distributed/events/next"
    public static let remoteLeasePath = "/v1/distributed/leases/next"
    public static let remoteResultPath = "/v1/distributed/results"
    public static let remoteCancellationPath = "/v1/distributed/cancellations/next"
    public static let remoteCancellationAcknowledgementPath =
        "/v1/distributed/cancellations/ack"
    public static let remoteSnapshotChunksPath =
        "/v1/distributed/feature-conveyor/snapshot-chunks"
    public static let remoteResultArtifactsPath =
        "/v1/distributed/feature-conveyor/result-artifacts"
    public static let maximumEventsPerBatch = 64

    nonisolated public let routingMode: AssemblywrightMacBridgeEventRelayRoutingMode
    private let configuration: AssemblywrightMacDeveloperEventRelayConfiguration
    private let deviceID: UUID?
    private let launcher: any AssemblywrightMacDeveloperAgentLaunching
    private var agent: (any AssemblywrightMacDeveloperAgentSession)?
    private var stopped = false

    public init(
        configuration: AssemblywrightMacDeveloperEventRelayConfiguration,
        deviceID: UUID? = nil,
        launcher: any AssemblywrightMacDeveloperAgentLaunching =
            FoundationAssemblywrightMacDeveloperAgentLauncher()
    ) {
        routingMode = Self.routingMode(for: configuration, deviceID: deviceID)
        self.configuration = configuration
        self.deviceID = deviceID
        self.launcher = launcher
    }

    private nonisolated static func routingMode(
        for configuration: AssemblywrightMacDeveloperEventRelayConfiguration,
        deviceID: UUID?
    ) -> AssemblywrightMacBridgeEventRelayRoutingMode {
        guard (try? configuration.validatePaths()) != nil else { return .invalid }
        let zeroDeviceID = UUID(
            uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
        )
        let hasExactDeviceID = deviceID.map({ $0 != zeroDeviceID }) == true
        if configuration.fixtureJobsEnabled {
            return hasExactDeviceID ? .fixture : .invalid
        }
        if configuration.mlxJobsEnabled {
            return hasExactDeviceID ? .mlx : .invalid
        }
        if configuration.localCodingSnapshotsEnabled {
            return hasExactDeviceID ? .localCoding : .invalid
        }
        return .metadataOnly
    }

    public func relayEvents(
        using session: any AssemblywrightMacBridgeSession
    ) async throws -> AssemblywrightMacDeveloperEventRelayProgress {
        guard !stopped else {
            throw AssemblywrightMacDeveloperEventRelayError.agentUnavailable
        }
        let activeAgent: any AssemblywrightMacDeveloperAgentSession
        if let agent {
            activeAgent = agent
        } else {
            let launched = try await launcher.launch(configuration: configuration)
            agent = launched
            activeAgent = launched
        }
        let before = try await activeAgent.health()
        if configuration.localCodingSnapshotsEnabled {
            guard let deviceID else {
                throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
            }
            let requiresFreshConnection = try await relayOneLocalCodingSnapshot(
                using: session,
                deviceID: deviceID,
                agent: activeAgent
            )
            return AssemblywrightMacDeveloperEventRelayProgress(
                cursor: before.cursor,
                acceptedEventCount: 0,
                hasMore: false,
                requiresFreshConnection: requiresFreshConnection
            )
        }
        let request = try Self.eventRequest(
            connectionEpoch: session.connectionEpoch,
            after: before.cursor
        )
        let response = try await session.send(
            AssemblywrightMacBridgeHTTPRequest(
                method: "POST",
                path: Self.remoteEventsPath,
                body: request
            )
        )
        let batch = try Self.validateBatchResponse(response)
        let accepted = try await activeAgent.accept(batch: response.body)
        guard accepted.cursor == batch.cursor else {
            throw AssemblywrightMacDeveloperEventRelayError.eventCursorRejected
        }
        if configuration.fixtureJobsEnabled {
            guard let deviceID else {
                throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
            }
            let requiresFreshConnection = try await relayOneFixtureJob(
                using: session,
                deviceID: deviceID,
                agent: activeAgent
            )
            return AssemblywrightMacDeveloperEventRelayProgress(
                cursor: batch.cursor,
                acceptedEventCount: batch.eventCount,
                hasMore: batch.hasMore,
                requiresFreshConnection: requiresFreshConnection
            )
        }
        if configuration.mlxJobsEnabled {
            guard let deviceID else {
                throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
            }
            let requiresFreshConnection = try await relayOneMLXJob(
                using: session,
                deviceID: deviceID,
                agent: activeAgent
            )
            return AssemblywrightMacDeveloperEventRelayProgress(
                cursor: batch.cursor,
                acceptedEventCount: batch.eventCount,
                hasMore: batch.hasMore,
                requiresFreshConnection: requiresFreshConnection
            )
        }
        return AssemblywrightMacDeveloperEventRelayProgress(
            cursor: batch.cursor,
            acceptedEventCount: batch.eventCount,
            hasMore: batch.hasMore,
            requiresFreshConnection: false
        )
    }

    public func stop() async throws {
        stopped = true
        guard let agent else { return }
        try await agent.stop()
        self.agent = nil
    }

    private struct ValidatedFixtureJob: Sendable {
        let body: Data
        let connectionEpoch: UInt64
        let sequence: UInt64
        let taskID: UUID
        let stepID: UUID
        let attemptID: UUID
        let leaseID: UUID
        let cancellationID: UUID
        let contextDigest: [UInt8]
        let input: String
        let leaseDurationMilliseconds: UInt64
        let deadlineAfterMilliseconds: UInt64
    }

    private struct ValidatedCancellation: Sendable {
        let body: Data
        let sequence: UInt64
    }

    private enum FixtureRaceOutcome: Sendable {
        case result(Data)
        case cancellation(ValidatedCancellation)
        case timedOut
        case settled
    }

    private struct ValidatedMLXJob: Sendable {
        let body: Data
        let connectionEpoch: UInt64
        let sequence: UInt64
        let taskID: UUID
        let stepID: UUID
        let attemptID: UUID
        let leaseID: UUID
        let cancellationID: UUID
        let contextDigest: [UInt8]
        let selectedModel: String
        let leaseDurationMilliseconds: UInt64
        let deadlineAfterMilliseconds: UInt64
    }

    private enum MLXRaceOutcome: Sendable {
        case result(Data)
        case cancellation(ValidatedCancellation)
        case timedOut
        case settled
    }

    private struct ValidatedLocalCodingJob: Sendable {
        let body: Data
        let connectionEpoch: UInt64
        let sequence: UInt64
        let taskID: UUID
        let stepID: UUID
        let attemptID: UUID
        let leaseID: UUID
        let cancellationID: UUID
        let contextDigest: [UInt8]
        let admissionDigest: [UInt8]
        let featureID: UUID
        let featureLeaseID: UUID
        let snapshotID: UUID
        let snapshotDigest: [UInt8]
        let workPacketDigest: [UInt8]
        let leaseDurationMilliseconds: UInt64
        let deadlineAfterMilliseconds: UInt64
    }

    private struct ValidatedLocalCodingCompletion: Sendable {
        let resultBody: Data
        let resultPayloadDigest: [UInt8]
        let artifactAdmissionBody: Data
        let artifactID: UUID
        let artifactDigest: [UInt8]
        let artifactSize: UInt64
    }

    private enum LocalCodingRaceOutcome: Sendable {
        case result(Data)
        case cancellation(ValidatedCancellation, acknowledgement: Data)
        case transferRejectedDuringCancellation
        case timedOut
        case settled
    }

    private func relayOneLocalCodingSnapshot(
        using session: any AssemblywrightMacBridgeSession,
        deviceID: UUID,
        agent: any AssemblywrightMacDeveloperAgentSession
    ) async throws -> Bool {
        let leaseRequest = try JSONSerialization.data(
            withJSONObject: [
                "device_id": deviceID.uuidString.lowercased(),
                "connection_epoch": NSNumber(value: session.connectionEpoch)
            ],
            options: [.sortedKeys]
        )
        let leased = try await session.send(
            AssemblywrightMacBridgeHTTPRequest(
                method: "POST",
                path: Self.remoteLeasePath,
                body: leaseRequest
            )
        )
        if leased.status == 204 {
            guard leased.body.isEmpty else {
                throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
            }
            return true
        }
        if leased.status == 503 {
            guard let object = Self.strictJSONObject(leased.body),
                  Set(object.keys) == Set(["error"]),
                  object["error"] as? String == "emergency_pause_blocks_work" else {
                throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
            }
            return false
        }
        guard leased.status == 200 else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
        let job = try Self.validateLocalCodingJob(
            leased.body,
            expectedConnectionEpoch: session.connectionEpoch,
            expectedDeviceID: deviceID
        )

        do {
            try await agent.admitLocalCodingSnapshot(job.body)
            let resolution = AssemblywrightMacFixtureRaceResolution()
            let sessionRequests = AssemblywrightMacLocalCodingSessionRequests(
                session: session
            )
            let outcome = try await withThrowingTaskGroup(
                of: LocalCodingRaceOutcome.self,
                returning: LocalCodingRaceOutcome.self
            ) { group in
                group.addTask {
                    do {
                        return .result(try await Self.transferLocalCodingSnapshot(
                            using: sessionRequests,
                            job: job,
                            agent: agent
                        ))
                    } catch AssemblywrightMacDeveloperEventRelayError
                        .localCodingSnapshotRejected
                    {
                        guard await resolution.isCancellationInProgress() else {
                            throw AssemblywrightMacDeveloperEventRelayError
                                .localCodingSnapshotRejected
                        }
                        return .transferRejectedDuringCancellation
                    }
                }
                group.addTask {
                    while !Task.isCancelled {
                        if await resolution.isResolved() { return .settled }
                        if let cancellation = try await Self.pollLocalCodingCancellation(
                            using: sessionRequests,
                            job: job
                        ) {
                            await resolution.markCancellationInProgress()
                            let acknowledgement = try await agent.cancelLocalCodingSnapshot(
                                cancellation.body
                            )
                            try Self.validateLocalCodingCancellationAcknowledgement(
                                acknowledgement,
                                instruction: cancellation,
                                job: job
                            )
                            return .cancellation(
                                cancellation,
                                acknowledgement: acknowledgement
                            )
                        }
                        if await resolution.isResolved() { return .settled }
                        try await Task.sleep(for: .milliseconds(25))
                    }
                    throw CancellationError()
                }
                group.addTask {
                    let timeout = min(
                        job.leaseDurationMilliseconds,
                        job.deadlineAfterMilliseconds
                    )
                    let clock = ContinuousClock()
                    let deadline = clock.now + .milliseconds(timeout)
                    while clock.now < deadline {
                        if await resolution.isResolved() { return .settled }
                        try await Task.sleep(
                            for: min(.milliseconds(25), clock.now.duration(to: deadline))
                        )
                    }
                    return await resolution.isResolved() ? .settled : .timedOut
                }
                guard var first = try await group.next() else {
                    throw AssemblywrightMacDeveloperEventRelayError
                        .localCodingSnapshotRejected
                }
                if case .transferRejectedDuringCancellation = first {
                    while let remaining = try await group.next() {
                        if case .cancellation = remaining {
                            first = remaining
                            break
                        }
                        if case .timedOut = remaining {
                            first = remaining
                            break
                        }
                    }
                }
                if case let .result(result) = first {
                    await resolution.markResolved()
                    var final: LocalCodingRaceOutcome = .result(result)
                    while let remaining = try await group.next() {
                        if case .cancellation = remaining { final = remaining }
                    }
                    return final
                }
                group.cancelAll()
                return first
            }

            switch outcome {
            case let .result(result):
                let resultDigest = try Self.validateLocalCodingResult(result, for: job)
                let accepted = try await session.send(
                    AssemblywrightMacBridgeHTTPRequest(
                        method: "POST",
                        path: Self.remoteResultPath,
                        body: result
                    )
                )
                try Self.validateAcceptedLocalCodingResult(
                    accepted,
                    for: job,
                    expectedPayloadDigest: resultDigest
                )
            case let .cancellation(_, acknowledgement):
                let accepted = try await session.send(
                    AssemblywrightMacBridgeHTTPRequest(
                        method: "POST",
                        path: Self.remoteCancellationAcknowledgementPath,
                        body: acknowledgement
                    )
                )
                try Self.validateAcceptedCancellation(accepted)
            case .timedOut:
                throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotTimedOut
            case .settled, .transferRejectedDuringCancellation:
                throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
            }
            return false
        } catch {
            do {
                try await agent.stop()
                self.agent = nil
            } catch {
                // Retain ownership so the supervisor can retry bounded teardown.
                throw AssemblywrightMacDeveloperEventRelayError.teardownFailed
            }
            throw error
        }
    }

    private static func transferLocalCodingSnapshot(
        using sessionRequests: AssemblywrightMacLocalCodingSessionRequests,
        job: ValidatedLocalCodingJob,
        agent: any AssemblywrightMacDeveloperAgentSession
    ) async throws -> Data {
        var offset: UInt64 = 0
        var expectedTotal: UInt64?
        while true {
            try Task.checkCancellation()
            let request = try localCodingChunkRequest(for: job, offset: offset)
            let response = try await sessionRequests.send(
                AssemblywrightMacBridgeHTTPRequest(
                    method: "POST",
                    path: remoteSnapshotChunksPath,
                    body: request
                )
            )
            let chunk = try validateLocalCodingChunk(
                response,
                for: job,
                expectedOffset: offset,
                expectedTotal: expectedTotal
            )
            expectedTotal = chunk.totalBytes
            let acceptance = try await agent.acceptLocalCodingSnapshotChunk(response.body)
            if chunk.complete {
                guard case let .result(completionBody) = acceptance else {
                    throw AssemblywrightMacDeveloperEventRelayError
                        .localCodingSnapshotRejected
                }
                let completion = try validateLocalCodingCompletion(
                    completionBody,
                    for: job
                )
                do {
                    let receipt = try await sessionRequests.send(
                        AssemblywrightMacBridgeHTTPRequest(
                            method: "POST",
                            path: remoteResultArtifactsPath,
                            body: completion.artifactAdmissionBody
                        )
                    )
                    try validateLocalCodingArtifactReceipt(
                        receipt,
                        completion: completion,
                        job: job
                    )
                } catch is CancellationError {
                    throw CancellationError()
                } catch {
                    // Admission denial and receipt drift participate in the
                    // same cancellation-aware transfer rejection as a final
                    // chunk denial. If cancellation has already won, the race
                    // still validates and posts its acknowledgement.
                    throw AssemblywrightMacDeveloperEventRelayError
                        .localCodingSnapshotRejected
                }
                return completion.resultBody
            }
            guard case let .nextOffset(agentOffset) = acceptance,
                  agentOffset == chunk.nextOffset else {
                throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
            }
            offset = chunk.nextOffset
        }
    }

    private static func pollLocalCodingCancellation(
        using sessionRequests: AssemblywrightMacLocalCodingSessionRequests,
        job: ValidatedLocalCodingJob
    ) async throws -> ValidatedCancellation? {
        let request = try JSONSerialization.data(
            withJSONObject: [
                "protocol_version": Int(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
                "connection_epoch": NSNumber(value: job.connectionEpoch)
            ],
            options: [.sortedKeys]
        )
        let response = try await sessionRequests.send(
            AssemblywrightMacBridgeHTTPRequest(
                method: "POST",
                path: remoteCancellationPath,
                body: request
            )
        )
        guard response.status == 200,
              let object = strictJSONObject(response.body) else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
        if Set(object.keys) == Set(["status"]),
           object["status"] as? String == "no_cancellation" {
            return nil
        }
        guard Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence",
                  "task_id", "step_id", "attempt_id", "lease_id",
                  "cancellation_id", "deadline_after_ms"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              let sequence = strictInteger(object["sequence"]),
              sequence > job.sequence,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              let deadline = strictInteger(object["deadline_after_ms"]),
              (1 ... 2_000).contains(deadline) else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
        return ValidatedCancellation(body: response.body, sequence: sequence)
    }

    private func relayOneFixtureJob(
        using session: any AssemblywrightMacBridgeSession,
        deviceID: UUID,
        agent: any AssemblywrightMacDeveloperAgentSession
    ) async throws -> Bool {
        let leaseRequest = try JSONSerialization.data(
            withJSONObject: [
                "device_id": deviceID.uuidString.lowercased(),
                "connection_epoch": NSNumber(value: session.connectionEpoch)
            ],
            options: [.sortedKeys]
        )
        let leased = try await session.send(
            AssemblywrightMacBridgeHTTPRequest(
                method: "POST",
                path: Self.remoteLeasePath,
                body: leaseRequest
            )
        )
        if leased.status == 204 {
            guard leased.body.isEmpty else {
                throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
            }
            return true
        }
        if leased.status == 503 {
            guard let object = try? JSONSerialization.jsonObject(with: leased.body)
                    as? [String: Any],
                  Set(object.keys) == Set(["error"]),
                  object["error"] as? String == "emergency_pause_blocks_work" else {
                throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
            }
            return false
        }
        guard leased.status == 200 else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
        let job = try Self.validateFixtureJob(
            leased.body,
            expectedConnectionEpoch: session.connectionEpoch
        )

        let resolution = AssemblywrightMacFixtureRaceResolution()
        let outcome = try await withThrowingTaskGroup(
            of: FixtureRaceOutcome.self,
            returning: FixtureRaceOutcome.self
        ) { group in
            group.addTask {
                .result(try await agent.executeFixtureJob(job.body))
            }
            group.addTask {
                while !Task.isCancelled {
                    if await resolution.isResolved() {
                        return .settled
                    }
                    if let cancellation = try await Self.pollFixtureCancellation(
                        using: session,
                        job: job
                    ) {
                        return .cancellation(cancellation)
                    }
                    if await resolution.isResolved() {
                        return .settled
                    }
                    try await Task.sleep(for: .milliseconds(25))
                }
                throw CancellationError()
            }
            group.addTask {
                let timeout = min(
                    job.leaseDurationMilliseconds,
                    job.deadlineAfterMilliseconds
                )
                let clock = ContinuousClock()
                let deadline = clock.now + .milliseconds(timeout)
                while clock.now < deadline {
                    if await resolution.isResolved() {
                        return .settled
                    }
                    try await Task.sleep(
                        for: min(.milliseconds(25), clock.now.duration(to: deadline))
                    )
                }
                return await resolution.isResolved() ? .settled : .timedOut
            }
            guard let first = try await group.next() else {
                throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
            }
            if case .result = first {
                // Cancellation polling shares this serialized mTLS session with
                // result submission. Allow an in-flight poll to finish normally;
                // cancelling its network task would close the session.
                await resolution.markResolved()
                while try await group.next() != nil {}
            } else {
                group.cancelAll()
            }
            return first
        }

        switch outcome {
        case let .result(result):
            let resultDigest = try Self.validateFixtureResult(result, for: job)
            let accepted = try await session.send(
                AssemblywrightMacBridgeHTTPRequest(
                    method: "POST",
                    path: Self.remoteResultPath,
                    body: result
                )
            )
            try Self.validateAcceptedFixtureResult(
                accepted,
                for: job,
                expectedPayloadDigest: resultDigest
            )
        case let .cancellation(instruction):
            let acknowledgement = try await agent.cancelFixtureJob(instruction.body)
            try Self.validateCancellationAcknowledgement(
                acknowledgement,
                instruction: instruction,
                job: job
            )
            let accepted = try await session.send(
                AssemblywrightMacBridgeHTTPRequest(
                    method: "POST",
                    path: Self.remoteCancellationAcknowledgementPath,
                    body: acknowledgement
                )
            )
            try Self.validateAcceptedCancellation(accepted)
        case .timedOut:
            try await agent.stop()
            self.agent = nil
            throw AssemblywrightMacDeveloperEventRelayError.fixtureJobTimedOut
        case .settled:
            throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
        }
        return false
    }

    private static func pollFixtureCancellation(
        using session: any AssemblywrightMacBridgeSession,
        job: ValidatedFixtureJob
    ) async throws -> ValidatedCancellation? {
        let request = try JSONSerialization.data(
            withJSONObject: [
                "protocol_version": Int(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
                "connection_epoch": NSNumber(value: session.connectionEpoch)
            ],
            options: [.sortedKeys]
        )
        let response = try await session.send(
            AssemblywrightMacBridgeHTTPRequest(
                method: "POST",
                path: remoteCancellationPath,
                body: request
            )
        )
        guard response.status == 200,
              let object = strictJSONObject(response.body) else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
        if Set(object.keys) == Set(["status"]),
           object["status"] as? String == "no_cancellation" {
            return nil
        }
        guard Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence",
                  "task_id", "step_id", "attempt_id", "lease_id",
                  "cancellation_id", "deadline_after_ms"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              let sequence = strictInteger(object["sequence"]),
              sequence > job.sequence,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              let deadline = strictInteger(object["deadline_after_ms"]),
              (1 ... 2_000).contains(deadline) else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
        return ValidatedCancellation(body: response.body, sequence: sequence)
    }

    private func relayOneMLXJob(
        using session: any AssemblywrightMacBridgeSession,
        deviceID: UUID,
        agent: any AssemblywrightMacDeveloperAgentSession
    ) async throws -> Bool {
        guard let selectedModel = configuration.mlxModelID else {
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
        }
        let leaseRequest = try JSONSerialization.data(
            withJSONObject: [
                "device_id": deviceID.uuidString.lowercased(),
                "connection_epoch": NSNumber(value: session.connectionEpoch)
            ],
            options: [.sortedKeys]
        )
        let leased = try await session.send(
            AssemblywrightMacBridgeHTTPRequest(
                method: "POST",
                path: Self.remoteLeasePath,
                body: leaseRequest
            )
        )
        if leased.status == 204 {
            guard leased.body.isEmpty else {
                throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
            }
            return true
        }
        if leased.status == 503 {
            guard let object = Self.strictJSONObject(leased.body),
                  Set(object.keys) == Set(["error"]),
                  object["error"] as? String == "emergency_pause_blocks_work" else {
                throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
            }
            return false
        }
        guard leased.status == 200 else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
        let job = try Self.validateMLXJob(
            leased.body,
            expectedConnectionEpoch: session.connectionEpoch,
            selectedModel: selectedModel
        )
        let resolution = AssemblywrightMacFixtureRaceResolution()
        let outcome = try await withThrowingTaskGroup(
            of: MLXRaceOutcome.self,
            returning: MLXRaceOutcome.self
        ) { group in
            group.addTask {
                .result(try await agent.executeMLXJob(job.body))
            }
            group.addTask {
                while !Task.isCancelled {
                    if await resolution.isResolved() { return .settled }
                    if let cancellation = try await Self.pollMLXCancellation(
                        using: session,
                        job: job
                    ) {
                        return .cancellation(cancellation)
                    }
                    if await resolution.isResolved() { return .settled }
                    try await Task.sleep(for: .milliseconds(25))
                }
                throw CancellationError()
            }
            group.addTask {
                let timeout = min(
                    job.leaseDurationMilliseconds,
                    job.deadlineAfterMilliseconds
                )
                let clock = ContinuousClock()
                let deadline = clock.now + .milliseconds(timeout)
                while clock.now < deadline {
                    if await resolution.isResolved() { return .settled }
                    try await Task.sleep(
                        for: min(.milliseconds(25), clock.now.duration(to: deadline))
                    )
                }
                return await resolution.isResolved() ? .settled : .timedOut
            }
            guard let first = try await group.next() else {
                throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
            }
            if case .result = first {
                await resolution.markResolved()
                while try await group.next() != nil {}
            } else {
                group.cancelAll()
            }
            return first
        }

        switch outcome {
        case let .result(result):
            let resultDigest = try Self.validateMLXResult(result, for: job)
            let accepted = try await session.send(
                AssemblywrightMacBridgeHTTPRequest(
                    method: "POST",
                    path: Self.remoteResultPath,
                    body: result
                )
            )
            try Self.validateAcceptedMLXResult(
                accepted,
                for: job,
                expectedPayloadDigest: resultDigest
            )
        case let .cancellation(instruction):
            let acknowledgement = try await agent.cancelMLXJob(instruction.body)
            try Self.validateMLXCancellationAcknowledgement(
                acknowledgement,
                instruction: instruction,
                job: job
            )
            let accepted = try await session.send(
                AssemblywrightMacBridgeHTTPRequest(
                    method: "POST",
                    path: Self.remoteCancellationAcknowledgementPath,
                    body: acknowledgement
                )
            )
            try Self.validateAcceptedCancellation(accepted)
        case .timedOut:
            try await agent.stop()
            self.agent = nil
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobTimedOut
        case .settled:
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
        }
        return false
    }

    private static func pollMLXCancellation(
        using session: any AssemblywrightMacBridgeSession,
        job: ValidatedMLXJob
    ) async throws -> ValidatedCancellation? {
        let request = try JSONSerialization.data(
            withJSONObject: [
                "protocol_version": Int(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
                "connection_epoch": NSNumber(value: session.connectionEpoch)
            ],
            options: [.sortedKeys]
        )
        let response = try await session.send(
            AssemblywrightMacBridgeHTTPRequest(
                method: "POST",
                path: remoteCancellationPath,
                body: request
            )
        )
        guard response.status == 200,
              let object = strictJSONObject(response.body) else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
        if Set(object.keys) == Set(["status"]),
           object["status"] as? String == "no_cancellation" {
            return nil
        }
        guard Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence",
                  "task_id", "step_id", "attempt_id", "lease_id",
                  "cancellation_id", "deadline_after_ms"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              let sequence = strictInteger(object["sequence"]),
              sequence > job.sequence,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              let deadline = strictInteger(object["deadline_after_ms"]),
              (1 ... 2_000).contains(deadline) else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
        return ValidatedCancellation(body: response.body, sequence: sequence)
    }

    private static func validateFixtureJob(
        _ body: Data,
        expectedConnectionEpoch: UInt64
    ) throws -> ValidatedFixtureJob {
        guard !body.isEmpty,
              body.count <= 16 * 1_024,
              let object = strictJSONObject(body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence",
                  "task_id", "step_id", "attempt_id", "lease_id",
                  "cancellation_id", "capability_id", "selected_model",
                  "sensitivity", "context_handling", "lease_duration_ms",
                  "deadline_after_ms", "context_sha256", "context"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == expectedConnectionEpoch,
              let sequence = strictInteger(object["sequence"]),
              sequence > 0,
              let taskID = strictUUID(object["task_id"]),
              let stepID = strictUUID(object["step_id"]),
              let attemptID = strictUUID(object["attempt_id"]),
              let leaseID = strictUUID(object["lease_id"]),
              let cancellationID = strictUUID(object["cancellation_id"]),
              object["capability_id"] as? String == "fixture.reasoning",
              object["selected_model"] as? String == "assemblywright-fixture-v1",
              object["sensitivity"] as? String == "public",
              object["context_handling"] as? String == "ephemeral_no_retention",
              let leaseDuration = strictInteger(object["lease_duration_ms"]),
              (1 ... 600_000).contains(leaseDuration),
              let deadline = strictInteger(object["deadline_after_ms"]),
              (1 ... 7_200_000).contains(deadline),
              let contextDigest = strictDigest(object["context_sha256"]),
              let context = object["context"] as? [String: Any],
              Set(context.keys) == Set(["operation", "input", "delay_ms"]),
              context["operation"] as? String == "synthetic_echo",
              let input = context["input"] as? String,
              input.utf8.count <= 4_096,
              let delay = strictInteger(context["delay_ms"]),
              delay <= 5_000,
              let contextData = try? protocolDigestJSON(context),
              contextData.count <= 8_192,
              Array(SHA256.hash(data: contextData)) == contextDigest else {
            throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
        }
        return ValidatedFixtureJob(
            body: body,
            connectionEpoch: expectedConnectionEpoch,
            sequence: sequence,
            taskID: taskID,
            stepID: stepID,
            attemptID: attemptID,
            leaseID: leaseID,
            cancellationID: cancellationID,
            contextDigest: contextDigest,
            input: input,
            leaseDurationMilliseconds: leaseDuration,
            deadlineAfterMilliseconds: deadline
        )
    }

    private static func validateFixtureResult(
        _ body: Data,
        for job: ValidatedFixtureJob
    ) throws -> [UInt8] {
        guard !body.isEmpty,
              body.count <= 16 * 1_024,
              let object = strictJSONObject(body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence",
                  "task_id", "step_id", "attempt_id", "lease_id",
                  "cancellation_id", "status", "context_sha256",
                  "payload_sha256", "payload"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              strictInteger(object["sequence"]).map({ $0 > job.sequence }) == true,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              object["status"] as? String == "completed",
              strictDigest(object["context_sha256"]) == job.contextDigest,
              let payloadDigest = strictDigest(object["payload_sha256"]),
              let payload = object["payload"] as? [String: Any],
              Set(payload.keys) == Set(["operation", "output", "synthetic"]),
              payload["operation"] as? String == "synthetic_echo",
              payload["output"] as? String == job.input,
              payload["synthetic"] as? Bool == true,
              let payloadData = try? protocolDigestJSON(payload),
              payloadData.count <= 8_192,
              Array(SHA256.hash(data: payloadData)) == payloadDigest else {
            throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
        }
        return payloadDigest
    }

    private static func validateMLXJob(
        _ body: Data,
        expectedConnectionEpoch: UInt64,
        selectedModel: String
    ) throws -> ValidatedMLXJob {
        guard !body.isEmpty,
              body.count <= 64 * 1_024,
              let object = strictJSONObject(body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence",
                  "task_id", "step_id", "attempt_id", "lease_id",
                  "cancellation_id", "capability_id", "selected_model",
                  "sensitivity", "context_handling", "lease_duration_ms",
                  "deadline_after_ms", "context_sha256", "context"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == expectedConnectionEpoch,
              let sequence = strictInteger(object["sequence"]),
              sequence > 0,
              let taskID = strictUUID(object["task_id"]),
              let stepID = strictUUID(object["step_id"]),
              let attemptID = strictUUID(object["attempt_id"]),
              let leaseID = strictUUID(object["lease_id"]),
              let cancellationID = strictUUID(object["cancellation_id"]),
              object["capability_id"] as? String == "mlx.reasoning",
              object["selected_model"] as? String == selectedModel,
              object["sensitivity"] as? String == "public",
              object["context_handling"] as? String == "ephemeral_no_retention",
              let leaseDuration = strictInteger(object["lease_duration_ms"]),
              (1 ... 600_000).contains(leaseDuration),
              let deadline = strictInteger(object["deadline_after_ms"]),
              (1 ... 7_200_000).contains(deadline),
              let contextDigest = strictDigest(object["context_sha256"]),
              let context = object["context"] as? [String: Any],
              Set(context.keys) == Set([
                  "operation", "prompt", "max_tokens", "temperature_milli"
              ]),
              context["operation"] as? String == "generate_text",
              let prompt = context["prompt"] as? String,
              !prompt.isEmpty,
              prompt.utf8.count <= 32 * 1_024,
              let maxTokens = strictInteger(context["max_tokens"]),
              (1 ... 512).contains(maxTokens),
              let temperature = strictInteger(context["temperature_milli"]),
              temperature <= 2_000,
              let contextData = try? protocolDigestJSON(context),
              contextData.count <= 40 * 1_024,
              Array(SHA256.hash(data: contextData)) == contextDigest else {
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
        }
        return ValidatedMLXJob(
            body: body,
            connectionEpoch: expectedConnectionEpoch,
            sequence: sequence,
            taskID: taskID,
            stepID: stepID,
            attemptID: attemptID,
            leaseID: leaseID,
            cancellationID: cancellationID,
            contextDigest: contextDigest,
            selectedModel: selectedModel,
            leaseDurationMilliseconds: leaseDuration,
            deadlineAfterMilliseconds: deadline
        )
    }

    private static func validateMLXResult(
        _ body: Data,
        for job: ValidatedMLXJob
    ) throws -> [UInt8] {
        guard !body.isEmpty,
              body.count <= 1_024 * 1_024,
              let object = strictJSONObject(body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence",
                  "task_id", "step_id", "attempt_id", "lease_id",
                  "cancellation_id", "status", "context_sha256",
                  "payload_sha256", "payload"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              strictInteger(object["sequence"]).map({ $0 > job.sequence }) == true,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              object["status"] as? String == "completed",
              strictDigest(object["context_sha256"]) == job.contextDigest,
              let payloadDigest = strictDigest(object["payload_sha256"]),
              let payload = object["payload"] as? [String: Any],
              Set(payload.keys) == Set(["operation", "output", "model"]),
              payload["operation"] as? String == "generate_text",
              let output = payload["output"] as? String,
              !output.isEmpty,
              output.utf8.count <= 768 * 1_024,
              payload["model"] as? String == job.selectedModel,
              let payloadData = try? protocolDigestJSON(payload),
              payloadData.count <= 800 * 1_024,
              Array(SHA256.hash(data: payloadData)) == payloadDigest else {
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
        }
        return payloadDigest
    }

    private struct ValidatedLocalCodingChunk {
        let totalBytes: UInt64
        let nextOffset: UInt64
        let complete: Bool
    }

    private static func validateLocalCodingJob(
        _ body: Data,
        expectedConnectionEpoch: UInt64,
        expectedDeviceID: UUID
    ) throws -> ValidatedLocalCodingJob {
        guard !body.isEmpty,
              body.count <= 16 * 1_024,
              let object = strictJSONObject(body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence",
                  "task_id", "step_id", "attempt_id", "lease_id",
                  "cancellation_id", "capability_id", "selected_model",
                  "sensitivity", "context_handling", "lease_duration_ms",
                  "deadline_after_ms", "context_sha256", "context"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == expectedConnectionEpoch,
              let sequence = strictInteger(object["sequence"]), sequence > 0,
              let taskID = strictUUID(object["task_id"]),
              let stepID = strictUUID(object["step_id"]),
              let attemptID = strictUUID(object["attempt_id"]),
              let leaseID = strictUUID(object["lease_id"]),
              let cancellationID = strictUUID(object["cancellation_id"]),
              object["capability_id"] as? String == "local.coding.v1",
              object["selected_model"] as? String == "assemblywright-local-coding-v1",
              object["sensitivity"] as? String == "workspace",
              object["context_handling"] as? String == "ephemeral_no_retention",
              let leaseDuration = strictInteger(object["lease_duration_ms"]),
              (1 ... 600_000).contains(leaseDuration),
              let deadline = strictInteger(object["deadline_after_ms"]),
              (1 ... 7_200_000).contains(deadline),
              let contextDigest = strictDigest(object["context_sha256"]),
              let context = object["context"] as? [String: Any],
              Set(context.keys) == Set([
                  "feature_id", "specification_revision", "lifecycle_revision",
                  "feature_lease_id", "snapshot_id", "snapshot_sha256",
                  "work_packet_sha256", "work_packet", "device_id",
                  "device_registry_revision", "queue_revision",
                  "emergency_pause_revision"
              ]),
              let featureID = strictUUID(context["feature_id"]),
              strictInteger(context["specification_revision"]).map({ $0 > 0 }) == true,
              strictInteger(context["lifecycle_revision"]).map({ $0 > 0 }) == true,
              let featureLeaseID = strictUUID(context["feature_lease_id"]),
              let snapshotID = strictUUID(context["snapshot_id"]),
              let snapshotDigest = strictDigest(context["snapshot_sha256"]),
              snapshotDigest != [UInt8](repeating: 0, count: 32),
              let workPacketDigest = strictDigest(context["work_packet_sha256"]),
              workPacketDigest != [UInt8](repeating: 0, count: 32),
              let workPacket = context["work_packet"] as? [String: Any],
              Set(workPacket.keys) == Set([
                  "packet_id", "ordinal", "acceptance_criteria_count"
              ]),
              strictUUID(workPacket["packet_id"]) != nil,
              strictInteger(workPacket["ordinal"]).map({ (1 ... 65_535).contains($0) })
                == true,
              strictInteger(workPacket["acceptance_criteria_count"])
                .map({ (1 ... 65_535).contains($0) }) == true,
              strictUUID(context["device_id"]) == expectedDeviceID,
              strictInteger(context["device_registry_revision"]).map({ $0 > 0 }) == true,
              strictInteger(context["queue_revision"]) != nil,
              strictInteger(context["emergency_pause_revision"]) != nil,
              let contextData = try? protocolDigestJSON(context),
              contextData.count <= 8 * 1_024,
              Array(SHA256.hash(data: contextData)) == contextDigest else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
        return ValidatedLocalCodingJob(
            body: body,
            connectionEpoch: expectedConnectionEpoch,
            sequence: sequence,
            taskID: taskID,
            stepID: stepID,
            attemptID: attemptID,
            leaseID: leaseID,
            cancellationID: cancellationID,
            contextDigest: contextDigest,
            admissionDigest: localCodingAdmissionDigest(
                protocolVersion: AssemblywrightMacMTLSBridgeTransport.protocolVersion,
                contextDigest: contextDigest,
                taskID: taskID,
                stepID: stepID,
                attemptID: attemptID,
                leaseID: leaseID,
                cancellationID: cancellationID,
                connectionEpoch: expectedConnectionEpoch,
                sequence: sequence,
                leaseDurationMilliseconds: leaseDuration,
                deadlineAfterMilliseconds: deadline
            ),
            featureID: featureID,
            featureLeaseID: featureLeaseID,
            snapshotID: snapshotID,
            snapshotDigest: snapshotDigest,
            workPacketDigest: workPacketDigest,
            leaseDurationMilliseconds: leaseDuration,
            deadlineAfterMilliseconds: deadline
        )
    }

    private static func localCodingChunkRequest(
        for job: ValidatedLocalCodingJob,
        offset: UInt64
    ) throws -> Data {
        try JSONSerialization.data(
            withJSONObject: [
                "protocol_version": Int(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
                "connection_epoch": NSNumber(value: job.connectionEpoch),
                "task_id": job.taskID.uuidString.lowercased(),
                "step_id": job.stepID.uuidString.lowercased(),
                "attempt_id": job.attemptID.uuidString.lowercased(),
                "lease_id": job.leaseID.uuidString.lowercased(),
                "cancellation_id": job.cancellationID.uuidString.lowercased(),
                "snapshot_id": job.snapshotID.uuidString.lowercased(),
                "snapshot_sha256": job.snapshotDigest,
                "offset": NSNumber(value: offset)
            ],
            options: [.sortedKeys]
        )
    }

    private static func validateLocalCodingChunk(
        _ response: AssemblywrightMacBridgeHTTPResponse,
        for job: ValidatedLocalCodingJob,
        expectedOffset: UInt64,
        expectedTotal: UInt64?
    ) throws -> ValidatedLocalCodingChunk {
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= 384 * 1_024,
              let object = strictJSONObject(response.body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "task_id", "step_id",
                  "attempt_id", "lease_id", "cancellation_id", "snapshot_id",
                  "snapshot_sha256", "offset", "total_bytes", "content_sha256",
                  "content_hex", "complete"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              strictUUID(object["snapshot_id"]) == job.snapshotID,
              strictDigest(object["snapshot_sha256"]) == job.snapshotDigest,
              strictInteger(object["offset"]) == expectedOffset,
              let total = strictInteger(object["total_bytes"]),
              total > 0, total <= 320 * 1_024 * 1_024,
              expectedTotal.map({ $0 == total }) ?? true,
              let contentDigest = strictDigest(object["content_sha256"]),
              let contentHex = object["content_hex"] as? String,
              !contentHex.isEmpty,
              contentHex.utf8.count % 2 == 0,
              contentHex.utf8.count / 2 <= 128 * 1_024,
              contentHex.utf8.allSatisfy({
                  (0x30 ... 0x39).contains($0) || (0x61 ... 0x66).contains($0)
              }),
              let content = decodeLowerHex(contentHex),
              Array(SHA256.hash(data: content)) == contentDigest,
              let nextOffset = checkedAdd(expectedOffset, UInt64(content.count)),
              nextOffset <= total,
              let complete = object["complete"] as? Bool,
              complete == (nextOffset == total) else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
        return ValidatedLocalCodingChunk(
            totalBytes: total,
            nextOffset: nextOffset,
            complete: complete
        )
    }

    private static func decodeLowerHex(_ value: String) -> Data? {
        var bytes = [UInt8]()
        bytes.reserveCapacity(value.utf8.count / 2)
        let encoded = Array(value.utf8)
        for pair in stride(from: 0, to: encoded.count, by: 2) {
            guard let high = lowerHexNibble(encoded[pair]),
                  let low = lowerHexNibble(encoded[pair + 1]) else { return nil }
            bytes.append((high << 4) | low)
        }
        return Data(bytes)
    }

    private static func lowerHexNibble(_ value: UInt8) -> UInt8? {
        switch value {
        case 0x30 ... 0x39: value - 0x30
        case 0x61 ... 0x66: value - 0x61 + 10
        default: nil
        }
    }

    private static func checkedAdd(_ left: UInt64, _ right: UInt64) -> UInt64? {
        let result = left.addingReportingOverflow(right)
        return result.overflow ? nil : result.partialValue
    }

    private static func localCodingFixtureAllowedPathsDigest() -> [UInt8] {
        let path = Data("README.md".utf8)
        var input = Data("assemblywright.local-coding-allowed-paths.v1\0".utf8)
        var count = UInt16(1).bigEndian
        Swift.withUnsafeBytes(of: &count) { input.append(contentsOf: $0) }
        var length = UInt64(path.count).bigEndian
        Swift.withUnsafeBytes(of: &length) { input.append(contentsOf: $0) }
        input.append(path)
        return Array(SHA256.hash(data: input))
    }

    private static func localCodingAdmissionDigest(
        protocolVersion: UInt16,
        contextDigest: [UInt8],
        taskID: UUID,
        stepID: UUID,
        attemptID: UUID,
        leaseID: UUID,
        cancellationID: UUID,
        connectionEpoch: UInt64,
        sequence: UInt64,
        leaseDurationMilliseconds: UInt64,
        deadlineAfterMilliseconds: UInt64
    ) -> [UInt8] {
        var transcript = Data("assemblywright.local-coding-admission.v1\0".utf8)
        var encodedProtocolVersion = protocolVersion.bigEndian
        Swift.withUnsafeBytes(of: &encodedProtocolVersion) {
            transcript.append(contentsOf: $0)
        }
        transcript.append(contentsOf: contextDigest)
        for identifier in [taskID, stepID, attemptID, leaseID, cancellationID] {
            var bytes = identifier.uuid
            Swift.withUnsafeBytes(of: &bytes) { transcript.append(contentsOf: $0) }
        }
        var encodedConnectionEpoch = connectionEpoch.bigEndian
        Swift.withUnsafeBytes(of: &encodedConnectionEpoch) {
            transcript.append(contentsOf: $0)
        }
        var encodedSequence = sequence.bigEndian
        Swift.withUnsafeBytes(of: &encodedSequence) { transcript.append(contentsOf: $0) }
        var encodedLeaseDuration = leaseDurationMilliseconds.bigEndian
        Swift.withUnsafeBytes(of: &encodedLeaseDuration) {
            transcript.append(contentsOf: $0)
        }
        var encodedDeadline = deadlineAfterMilliseconds.bigEndian
        Swift.withUnsafeBytes(of: &encodedDeadline) {
            transcript.append(contentsOf: $0)
        }
        return Array(SHA256.hash(data: transcript))
    }

    private static func validateLocalCodingResult(
        _ body: Data,
        for job: ValidatedLocalCodingJob
    ) throws -> [UInt8] {
        guard !body.isEmpty,
              body.count <= 32 * 1_024,
              let object = strictJSONObject(body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence", "task_id",
                  "step_id", "attempt_id", "lease_id", "cancellation_id", "status",
                  "context_sha256", "payload_sha256", "payload"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              strictInteger(object["sequence"]).map({ $0 > job.sequence }) == true,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              object["status"] as? String == "completed",
              strictDigest(object["context_sha256"]) == job.contextDigest,
              let payloadDigest = strictDigest(object["payload_sha256"]),
              let payload = object["payload"] as? [String: Any],
              Set(payload.keys) == Set([
                  "status", "work_packet_sha256", "admission_sha256",
                  "snapshot_sha256", "allowed_paths_sha256",
                  "changed_paths_sha256", "patch_sha256",
                  "artifact_id", "artifact_sha256", "artifact_size_bytes",
                  "changed_file_count", "test_status", "mutation_performed",
                  "workspace_retained", "ambiguous"
              ]),
              payload["status"] as? String == "contained_coding_completed",
              strictDigest(payload["work_packet_sha256"]) == job.workPacketDigest,
              strictDigest(payload["admission_sha256"]) == job.admissionDigest,
              strictDigest(payload["snapshot_sha256"]) == job.snapshotDigest,
              let allowedPathsDigest = strictDigest(payload["allowed_paths_sha256"]),
              allowedPathsDigest == localCodingFixtureAllowedPathsDigest(),
              strictDigest(payload["changed_paths_sha256"]) == allowedPathsDigest,
              let patchDigest = strictDigest(payload["patch_sha256"]),
              patchDigest != [UInt8](repeating: 0, count: 32),
              strictUUID(payload["artifact_id"]) != nil,
              strictDigest(payload["artifact_sha256"]) == patchDigest,
              strictInteger(payload["artifact_size_bytes"])
                .map({ (1 ... 64 * 1_024).contains($0) }) == true,
              strictInteger(payload["changed_file_count"]) == 1,
              payload["test_status"] as? String == "not_run",
              payload["mutation_performed"] as? Bool == true,
              payload["workspace_retained"] as? Bool == false,
              payload["ambiguous"] as? Bool == false,
              let payloadData = try? protocolDigestJSON(payload),
              Array(SHA256.hash(data: payloadData)) == payloadDigest else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
        return payloadDigest
    }

    private static func validateLocalCodingCompletion(
        _ body: Data,
        for job: ValidatedLocalCodingJob
    ) throws -> ValidatedLocalCodingCompletion {
        guard !body.isEmpty,
              body.count <= 160 * 1_024,
              let object = strictJSONObject(body),
              Set(object.keys) == Set(["result", "artifact"]),
              let result = object["result"] as? [String: Any],
              let resultSequence = strictInteger(result["sequence"]),
              resultSequence > job.sequence,
              let artifact = object["artifact"] as? [String: Any],
              Set(artifact.keys) == Set([
                  "artifact_id", "artifact_sha256", "artifact_size_bytes", "artifact_hex"
              ]),
              let artifactID = strictUUID(artifact["artifact_id"]),
              let artifactDigest = strictDigest(artifact["artifact_sha256"]),
              let artifactSize = strictInteger(artifact["artifact_size_bytes"]),
              (1 ... 64 * 1_024).contains(artifactSize),
              let artifactHex = artifact["artifact_hex"] as? String,
              artifactHex.utf8.count == Int(artifactSize) * 2,
              let artifactBytes = decodeLowerHex(artifactHex),
              UInt64(artifactBytes.count) == artifactSize,
              Array(SHA256.hash(data: artifactBytes)) == artifactDigest,
              validateCanonicalLocalCodingArtifact(artifactBytes),
              let resultPayload = result["payload"] as? [String: Any],
              strictUUID(resultPayload["artifact_id"]) == artifactID,
              strictDigest(resultPayload["patch_sha256"]) == artifactDigest,
              strictDigest(resultPayload["artifact_sha256"]) == artifactDigest,
              strictInteger(resultPayload["artifact_size_bytes"]) == artifactSize else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
        let resultBody = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys])
        let resultPayloadDigest = try validateLocalCodingResult(resultBody, for: job)
        let admission: [String: Any] = [
            "protocol_version": Int(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
            "connection_epoch": NSNumber(value: job.connectionEpoch),
            "sequence": NSNumber(value: resultSequence),
            "task_id": job.taskID.uuidString.lowercased(),
            "step_id": job.stepID.uuidString.lowercased(),
            "attempt_id": job.attemptID.uuidString.lowercased(),
            "lease_id": job.leaseID.uuidString.lowercased(),
            "cancellation_id": job.cancellationID.uuidString.lowercased(),
            "context_sha256": job.contextDigest,
            "feature_id": job.featureID.uuidString.lowercased(),
            "feature_lease_id": job.featureLeaseID.uuidString.lowercased(),
            "snapshot_id": job.snapshotID.uuidString.lowercased(),
            "snapshot_sha256": job.snapshotDigest,
            "work_packet_sha256": job.workPacketDigest,
            "artifact": artifact
        ]
        return ValidatedLocalCodingCompletion(
            resultBody: resultBody,
            resultPayloadDigest: resultPayloadDigest,
            artifactAdmissionBody: try JSONSerialization.data(
                withJSONObject: admission,
                options: [.sortedKeys]
            ),
            artifactID: artifactID,
            artifactDigest: artifactDigest,
            artifactSize: artifactSize
        )
    }

    private static func validateCanonicalLocalCodingArtifact(_ bytes: Data) -> Bool {
        let fixture = Data("assemblywright contained coding fixture\n".utf8)
        let replacementDigest = Array(SHA256.hash(data: fixture))
        guard let object = strictJSONObject(bytes),
              Set(object.keys) == Set([
                  "format", "path", "expected_before_sha256",
                  "replacement_sha256", "replacement_hex"
              ]),
              object["format"] as? String == "assemblywright.readme-replacement.v1",
              object["path"] as? String == "README.md",
              let beforeDigest = strictDigest(object["expected_before_sha256"]),
              beforeDigest != [UInt8](repeating: 0, count: 32),
              strictDigest(object["replacement_sha256"]) == replacementDigest,
              object["replacement_hex"] as? String == encodeLowerHex(fixture) else {
            return false
        }
        let before = beforeDigest.map(String.init).joined(separator: ",")
        let replacement = replacementDigest.map(String.init).joined(separator: ",")
        let expected = "{\"format\":\"assemblywright.readme-replacement.v1\","
            + "\"path\":\"README.md\",\"expected_before_sha256\":[\(before)],"
            + "\"replacement_sha256\":[\(replacement)],"
            + "\"replacement_hex\":\"\(encodeLowerHex(fixture))\"}"
        return bytes == Data(expected.utf8)
    }

    private static func validateLocalCodingArtifactReceipt(
        _ response: AssemblywrightMacBridgeHTTPResponse,
        completion: ValidatedLocalCodingCompletion,
        job: ValidatedLocalCodingJob
    ) throws {
        guard response.status == 200,
              let object = strictJSONObject(response.body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence", "task_id",
                  "step_id", "attempt_id", "lease_id", "cancellation_id",
                  "artifact_id", "artifact_sha256", "artifact_size_bytes", "status"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              strictInteger(object["sequence"]).map({ $0 > job.sequence }) == true,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              strictUUID(object["artifact_id"]) == completion.artifactID,
              strictDigest(object["artifact_sha256"]) == completion.artifactDigest,
              strictInteger(object["artifact_size_bytes"]) == completion.artifactSize,
              object["status"] as? String == "result_artifact_admitted" else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
    }

    private static func encodeLowerHex(_ data: Data) -> String {
        let digits = Array("0123456789abcdef".utf8)
        var output = [UInt8]()
        output.reserveCapacity(data.count * 2)
        for byte in data {
            output.append(digits[Int(byte >> 4)])
            output.append(digits[Int(byte & 0x0f)])
        }
        return String(decoding: output, as: UTF8.self)
    }

    private static func validateAcceptedLocalCodingResult(
        _ response: AssemblywrightMacBridgeHTTPResponse,
        for job: ValidatedLocalCodingJob,
        expectedPayloadDigest: [UInt8]
    ) throws {
        guard response.status == 200,
              let object = strictJSONObject(response.body),
              Set(object.keys) == Set([
                  "task_id", "step_id", "status", "payload_sha256"
              ]),
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              object["status"] as? String == "succeeded",
              strictDigest(object["payload_sha256"]) == expectedPayloadDigest else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
    }

    private static func validateLocalCodingCancellationAcknowledgement(
        _ body: Data,
        instruction: ValidatedCancellation,
        job: ValidatedLocalCodingJob
    ) throws {
        guard let object = strictJSONObject(body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence", "task_id",
                  "step_id", "attempt_id", "lease_id", "cancellation_id", "status"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              strictInteger(object["sequence"]).map({ $0 > instruction.sequence }) == true,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              object["status"] as? String == "cancelled" else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
    }

    private static func validateAcceptedMLXResult(
        _ response: AssemblywrightMacBridgeHTTPResponse,
        for job: ValidatedMLXJob,
        expectedPayloadDigest: [UInt8]
    ) throws {
        guard response.status == 200,
              let object = strictJSONObject(response.body),
              Set(object.keys) == Set([
                  "task_id", "step_id", "status", "payload_sha256"
              ]),
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              object["status"] as? String == "succeeded",
              strictDigest(object["payload_sha256"]) == expectedPayloadDigest else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
    }

    private static func validateMLXCancellationAcknowledgement(
        _ body: Data,
        instruction: ValidatedCancellation,
        job: ValidatedMLXJob
    ) throws {
        guard let object = strictJSONObject(body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence",
                  "task_id", "step_id", "attempt_id", "lease_id",
                  "cancellation_id", "status"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              strictInteger(object["sequence"]).map({ $0 > instruction.sequence }) == true,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              object["status"] as? String == "cancelled" else {
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
        }
    }

    private static func validateAcceptedFixtureResult(
        _ response: AssemblywrightMacBridgeHTTPResponse,
        for job: ValidatedFixtureJob,
        expectedPayloadDigest: [UInt8]
    ) throws {
        guard response.status == 200,
              let object = strictJSONObject(response.body),
              Set(object.keys) == Set([
                  "task_id", "step_id", "status", "payload_sha256"
              ]),
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              object["status"] as? String == "succeeded",
              strictDigest(object["payload_sha256"]) == expectedPayloadDigest else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
    }

    private static func validateCancellationAcknowledgement(
        _ body: Data,
        instruction: ValidatedCancellation,
        job: ValidatedFixtureJob
    ) throws {
        guard let object = strictJSONObject(body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence",
                  "task_id", "step_id", "attempt_id", "lease_id",
                  "cancellation_id", "status"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              strictInteger(object["sequence"]).map({ $0 > instruction.sequence }) == true,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              object["status"] as? String == "cancelled" else {
            throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
        }
    }

    private static func validateAcceptedCancellation(
        _ response: AssemblywrightMacBridgeHTTPResponse
    ) throws {
        guard response.status == 200,
              let object = strictJSONObject(response.body),
              Set(object.keys) == Set(["accepted", "status"]),
              object["accepted"] as? Bool == true,
              object["status"] as? String == "cancelled" else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
    }

    fileprivate static func strictJSONObject(_ data: Data) -> [String: Any]? {
        var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
        guard !data.isEmpty,
              (try? scanner.validateNoDuplicateObjectKeysRecursively()) != nil,
              let object = try? JSONSerialization.jsonObject(
                  with: data,
                  options: []
              ) as? [String: Any] else {
            return nil
        }
        return object
    }

    private static func strictUUID(_ value: Any?) -> UUID? {
        guard let text = value as? String,
              text == text.lowercased(),
              let value = UUID(uuidString: text),
              value != UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)) else {
            return nil
        }
        return value
    }

    private static func strictDigest(_ value: Any?) -> [UInt8]? {
        guard let values = value as? [NSNumber], values.count == 32 else {
            return nil
        }
        var digest = [UInt8]()
        digest.reserveCapacity(32)
        for value in values {
            guard CFGetTypeID(value) != CFBooleanGetTypeID() else { return nil }
            let text = value.stringValue
            guard let parsed = UInt8(text), String(parsed) == text else { return nil }
            digest.append(parsed)
        }
        return digest
    }

    private static func protocolDigestJSON(_ object: [String: Any]) throws -> Data {
        // Rust's serde_json leaves forward slashes unescaped. The protocol
        // digest binds those exact sorted JSON bytes, so Foundation must do
        // the same even when a model identifier or bounded text contains "/".
        try JSONSerialization.data(
            withJSONObject: object,
            options: [.sortedKeys, .withoutEscapingSlashes]
        )
    }

    private static func eventRequest(
        connectionEpoch: UInt64,
        after: AssemblywrightMacDeveloperEventCursor?
    ) throws -> Data {
        guard connectionEpoch > 0 else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
        var object: [String: Any] = [
            "protocol_version": Int(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
            "connection_epoch": NSNumber(value: connectionEpoch),
            "after": NSNull(),
            "limit": maximumEventsPerBatch
        ]
        if let after {
            object["after"] = [
                "stream_id": after.streamID.uuidString.lowercased(),
                "sequence": NSNumber(value: after.sequence)
            ]
        }
        return try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
    }

    private struct ValidatedBatch {
        let cursor: AssemblywrightMacDeveloperEventCursor
        let eventCount: Int
        let hasMore: Bool
    }

    private static func validateBatchResponse(
        _ response: AssemblywrightMacBridgeHTTPResponse
    ) throws -> ValidatedBatch {
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= DarwinAssemblywrightUnixSocketTransport.maximumRequestBodyBytes,
              let object = try JSONSerialization.jsonObject(with: response.body)
                as? [String: Any],
              Set(object.keys) == Set([
                  "protocol_version", "stream_id", "after_sequence",
                  "next_sequence", "events", "has_more"
              ]),
              Self.strictInteger(object["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              let streamText = object["stream_id"] as? String,
              let streamID = UUID(uuidString: streamText),
              streamText.lowercased() != "00000000-0000-0000-0000-000000000000",
              let afterSequence = Self.strictInteger(object["after_sequence"]),
              let nextSequence = Self.strictInteger(object["next_sequence"]),
              nextSequence >= afterSequence,
              let events = object["events"] as? [[String: Any]],
              events.count <= maximumEventsPerBatch,
              UInt64(events.count) == nextSequence - afterSequence,
              let hasMore = object["has_more"] as? Bool else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
        }
        var expectedSequence = afterSequence
        for event in events {
            expectedSequence += 1
            guard Set(event.keys) == Set([
                "protocol_version", "cursor", "occurred_at_ms", "kind",
                "task_id", "step_id", "device_id", "connection_epoch"
            ]),
            Self.strictInteger(event["protocol_version"])
                == UInt64(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
            Self.strictInteger(event["occurred_at_ms"]).map({ $0 > 0 }) == true,
            let cursor = event["cursor"] as? [String: Any],
            Set(cursor.keys) == Set(["stream_id", "sequence"]),
            cursor["stream_id"] as? String == streamText,
            Self.strictInteger(cursor["sequence"]) == expectedSequence,
            event["kind"] is String else {
                throw AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse
            }
        }
        return ValidatedBatch(
            cursor: AssemblywrightMacDeveloperEventCursor(
                streamID: streamID,
                sequence: nextSequence
            ),
            eventCount: events.count,
            hasMore: hasMore
        )
    }

    fileprivate static func strictInteger(_ value: Any?) -> UInt64? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else {
            return nil
        }
        let text = number.stringValue
        guard !text.isEmpty,
              text.utf8.allSatisfy({ (0x30 ... 0x39).contains($0) }) else {
            return nil
        }
        return UInt64(text)
    }
}

public struct FoundationAssemblywrightMacDeveloperAgentLauncher:
    AssemblywrightMacDeveloperAgentLaunching, Sendable
{
    public init() {}

    public func launch(
        configuration: AssemblywrightMacDeveloperEventRelayConfiguration
    ) async throws -> any AssemblywrightMacDeveloperAgentSession {
        try configuration.validatePaths()
        let agent = try SecurityAssemblywrightMacRelayCodeIdentity.staticIdentity(
            executableURL: configuration.agentExecutableURL
        )
        let helper = try SecurityAssemblywrightMacRelayCodeIdentity.currentProcessIdentity()
        return try await FoundationAssemblywrightMacDeveloperAgentSession.start(
            configuration: configuration,
            agentIdentity: agent,
            helperIdentity: helper
        )
    }
}

private struct AssemblywrightMacRelayCodeIdentity: Sendable {
    let executableURL: URL
    let identifier: String
    let cdHash: Data
    let requirement: String
}

private enum SecurityAssemblywrightMacRelayCodeIdentity {
    static func staticIdentity(executableURL: URL) throws -> AssemblywrightMacRelayCodeIdentity {
        let standardized = executableURL.standardizedFileURL
        var metadata = stat()
        guard standardized.isFileURL,
              standardized.path.hasPrefix("/"),
              lstat(standardized.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFREG,
              access(standardized.path, X_OK) == 0 else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidAgentExecutable
        }
        var code: SecStaticCode?
        guard SecStaticCodeCreateWithPath(standardized as CFURL, [], &code) == errSecSuccess,
              let code,
              SecStaticCodeCheckValidity(
                  code,
                  SecCSFlags(rawValue: kSecCSStrictValidate),
                  nil
              ) == errSecSuccess else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidAgentSignature
        }
        return try identity(from: code, expectedURL: standardized)
    }

    static func currentProcessIdentity() throws -> AssemblywrightMacRelayCodeIdentity {
        var dynamicCode: SecCode?
        guard SecCodeCopySelf([], &dynamicCode) == errSecSuccess,
              let dynamicCode,
              SecCodeCheckValidity(
                  dynamicCode,
                  SecCSFlags(rawValue: kSecCSStrictValidate),
                  nil
              ) == errSecSuccess else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidHelperIdentity
        }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(dynamicCode, [], &staticCode) == errSecSuccess,
              let staticCode else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidHelperIdentity
        }
        return try identity(from: staticCode, expectedURL: nil)
    }

    static func validateRunning(
        processIdentifier: Int32,
        expected: AssemblywrightMacRelayCodeIdentity
    ) throws {
        let attributes = [
            kSecGuestAttributePid as String: NSNumber(value: processIdentifier)
        ] as CFDictionary
        var dynamicCode: SecCode?
        guard SecCodeCopyGuestWithAttributes(nil, attributes, [], &dynamicCode)
                == errSecSuccess,
              let dynamicCode else {
            throw AssemblywrightMacDeveloperEventRelayError.agentIdentityMismatch
        }
        var requirement: SecRequirement?
        guard SecRequirementCreateWithString(
            expected.requirement as CFString,
            [],
            &requirement
        ) == errSecSuccess,
        let requirement,
        SecCodeCheckValidity(
            dynamicCode,
            SecCSFlags(rawValue: kSecCSStrictValidate),
            requirement
        ) == errSecSuccess else {
            throw AssemblywrightMacDeveloperEventRelayError.agentIdentityMismatch
        }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(dynamicCode, [], &staticCode) == errSecSuccess,
              let staticCode else {
            throw AssemblywrightMacDeveloperEventRelayError.agentIdentityMismatch
        }
        let actual = try identity(from: staticCode, expectedURL: expected.executableURL)
        guard actual.cdHash == expected.cdHash,
              actual.identifier == expected.identifier else {
            throw AssemblywrightMacDeveloperEventRelayError.agentIdentityMismatch
        }
    }

    private static func identity(
        from code: SecStaticCode,
        expectedURL: URL?
    ) throws -> AssemblywrightMacRelayCodeIdentity {
        var rawInformation: CFDictionary?
        guard SecCodeCopySigningInformation(
            code,
            SecCSFlags(rawValue: kSecCSSigningInformation),
            &rawInformation
        ) == errSecSuccess,
        let information = rawInformation as? [String: Any],
        let identifier = information[kSecCodeInfoIdentifier as String] as? String,
        !identifier.isEmpty,
        identifier.utf8.count <= 256,
        let cdHash = information[kSecCodeInfoUnique as String] as? Data,
        !cdHash.isEmpty,
        cdHash.count <= 64,
        let executableURL = information[kSecCodeInfoMainExecutable as String] as? URL else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidAgentSignature
        }
        let standardized = executableURL.standardizedFileURL
        if let expectedURL,
           standardized.path != expectedURL.standardizedFileURL.path {
            throw AssemblywrightMacDeveloperEventRelayError.invalidAgentSignature
        }
        let requirement =
            "identifier \"\(identifier)\" and cdhash H\"\(cdHash.assemblywrightHexString)\""
        var compiled: SecRequirement?
        guard SecRequirementCreateWithString(
            requirement as CFString,
            [],
            &compiled
        ) == errSecSuccess,
        let compiled,
        SecStaticCodeCheckValidity(
            code,
            SecCSFlags(rawValue: kSecCSStrictValidate),
            compiled
        ) == errSecSuccess else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidAgentSignature
        }
        return AssemblywrightMacRelayCodeIdentity(
            executableURL: standardized,
            identifier: identifier,
            cdHash: cdHash,
            requirement: requirement
        )
    }
}

private actor FoundationAssemblywrightMacDeveloperAgentSession:
    AssemblywrightMacDeveloperAgentSession
{
    private static let maximumAgentStartupBytes = 16 * 1_024
    private let process: Process
    private let runtimeDirectoryURL: URL
    private let socketURL: URL
    private let bearerToken: String
    private let transport: DarwinAssemblywrightUnixSocketTransport
    private let mlxExecutionTransport: DarwinAssemblywrightUnixSocketTransport
    private let configurationFixtureJobsEnabled: Bool
    private let configurationMLXJobsEnabled: Bool
    private let configurationLocalCodingSnapshotsEnabled: Bool
    private var stopped = false

    private init(
        process: Process,
        runtimeDirectoryURL: URL,
        socketURL: URL,
        bearerToken: String,
        transport: DarwinAssemblywrightUnixSocketTransport,
        mlxExecutionTransport: DarwinAssemblywrightUnixSocketTransport,
        configurationFixtureJobsEnabled: Bool,
        configurationMLXJobsEnabled: Bool,
        configurationLocalCodingSnapshotsEnabled: Bool
    ) {
        self.process = process
        self.runtimeDirectoryURL = runtimeDirectoryURL
        self.socketURL = socketURL
        self.bearerToken = bearerToken
        self.transport = transport
        self.mlxExecutionTransport = mlxExecutionTransport
        self.configurationFixtureJobsEnabled = configurationFixtureJobsEnabled
        self.configurationMLXJobsEnabled = configurationMLXJobsEnabled
        self.configurationLocalCodingSnapshotsEnabled =
            configurationLocalCodingSnapshotsEnabled
    }

    static func start(
        configuration: AssemblywrightMacDeveloperEventRelayConfiguration,
        agentIdentity: AssemblywrightMacRelayCodeIdentity,
        helperIdentity: AssemblywrightMacRelayCodeIdentity
    ) async throws -> FoundationAssemblywrightMacDeveloperAgentSession {
        let runtimeDirectoryURL = try makeRuntimeDirectory()
        let socketURL = runtimeDirectoryURL.appendingPathComponent("relay.sock")
        guard socketURL.path.utf8.count < 104 else {
            try? FileManager.default.removeItem(at: runtimeDirectoryURL)
            throw AssemblywrightMacDeveloperEventRelayError.unsafeRuntimeDirectory
        }
        let bearer = try randomBearer()
        let peerPolicy = AssemblywrightIPCPeerIdentityPolicy(
            profile: .adhocExact,
            peerCodeRequirement: helperIdentity.requirement,
            coreCodeRequirement: agentIdentity.requirement,
            expectedCoreCDHash: agentIdentity.cdHash,
            expectedCoreExecutableURL: agentIdentity.executableURL
        )
        let transport = DarwinAssemblywrightUnixSocketTransport(
            // A valid fixture may deliberately wait for up to five seconds.
            // Keep bounded framing and scheduling overhead outside that budget.
            timeoutSeconds: 10,
            peerIdentityPolicy: { peerPolicy }
        )
        let mlxExecutionTransport = DarwinAssemblywrightUnixSocketTransport(
            // The protocol permits a ten-minute MLX lease. The agent owns the
            // earlier lease/deadline timeout and process-group cleanup.
            timeoutSeconds: 610,
            peerIdentityPolicy: { peerPolicy }
        )
        let process = Process()
        let input = Pipe()
        process.executableURL = agentIdentity.executableURL
        process.arguments = [
            "--data-dir", configuration.agentDataDirectoryURL.path, "serve"
        ]
        process.environment = [:]
        process.standardInput = input
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            try? FileManager.default.removeItem(at: runtimeDirectoryURL)
            throw AssemblywrightMacDeveloperEventRelayError.agentLaunchFailed
        }
        do {
            try SecurityAssemblywrightMacRelayCodeIdentity.validateRunning(
                processIdentifier: process.processIdentifier,
                expected: agentIdentity
            )
            let startup: [String: Any] = [
                "version": 2,
                "supervised_parent_pid": Int(getpid()),
                "socket_path": socketURL.path,
                "peer_code_requirement": helperIdentity.requirement,
                "peer_identity_profile": AssemblywrightIPCPeerIdentityProfile.adhocExact.rawValue,
                "bearer_token": bearer,
                "fixture_jobs_enabled": configuration.fixtureJobsEnabled,
                "mlx_jobs_enabled": configuration.mlxJobsEnabled,
                "local_coding_snapshots_enabled":
                    configuration.localCodingSnapshotsEnabled,
                "mlx_executable_path": configuration.mlxExecutableURL?.path ?? NSNull(),
                "mlx_model_path": configuration.mlxModelDirectoryURL?.path ?? NSNull(),
                "mlx_model_id": configuration.mlxModelID ?? NSNull()
            ]
            let startupData = try JSONSerialization.data(
                withJSONObject: startup,
                options: [.sortedKeys]
            )
            guard startupData.count <= maximumAgentStartupBytes else {
                throw AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument
            }
            try input.fileHandleForWriting.write(contentsOf: startupData)
            try input.fileHandleForWriting.close()
            let session = FoundationAssemblywrightMacDeveloperAgentSession(
                process: process,
                runtimeDirectoryURL: runtimeDirectoryURL,
                socketURL: socketURL,
                bearerToken: bearer,
                transport: transport,
                mlxExecutionTransport: mlxExecutionTransport,
                configurationFixtureJobsEnabled: configuration.fixtureJobsEnabled,
                configurationMLXJobsEnabled: configuration.mlxJobsEnabled,
                configurationLocalCodingSnapshotsEnabled:
                    configuration.localCodingSnapshotsEnabled
            )
            try await session.waitUntilHealthy()
            return session
        } catch {
            try? input.fileHandleForWriting.close()
            _ = kill(process.processIdentifier, SIGKILL)
            process.waitUntilExit()
            try? FileManager.default.removeItem(at: runtimeDirectoryURL)
            throw error
        }
    }

    func health() async throws -> AssemblywrightMacDeveloperAgentCursorSnapshot {
        let response = try await send(method: "GET", path: "/health")
        guard response.status == 200,
              let object = try JSONSerialization.jsonObject(with: response.body)
                as? [String: Any],
              Set(object.keys) == Set([
                  "status", "mode", "protocol_version", "schema_version",
                  "cursor", "boundary", "fixture_jobs_enabled", "mlx_jobs_enabled",
                  "local_coding_snapshots_enabled"
              ]),
              object["status"] as? String == "ok",
              object["mode"] as? String == "developer_event_relay",
              (object["protocol_version"] as? NSNumber)?.intValue
                == Int(AssemblywrightMacMTLSBridgeTransport.protocolVersion),
              (object["schema_version"] as? NSNumber)?.intValue == 1,
              object["fixture_jobs_enabled"] as? Bool
                == configurationFixtureJobsEnabled,
              object["mlx_jobs_enabled"] as? Bool == configurationMLXJobsEnabled,
              object["local_coding_snapshots_enabled"] as? Bool
                == configurationLocalCodingSnapshotsEnabled,
              object["boundary"] as? String == expectedBoundary,
              let cursorObject = object["cursor"] as? [String: Any],
              Set(cursorObject.keys) == Set(["cursor", "updated_at_ms"]) else {
            throw AssemblywrightMacDeveloperEventRelayError.invalidAgentResponse
        }
        do {
            return try JSONDecoder().decode(
                AssemblywrightMacDeveloperAgentCursorSnapshot.self,
                from: JSONSerialization.data(withJSONObject: cursorObject)
            )
        } catch {
            throw AssemblywrightMacDeveloperEventRelayError.invalidAgentResponse
        }
    }

    func accept(batch: Data) async throws -> AssemblywrightMacDeveloperAgentCursorSnapshot {
        let response = try await send(
            method: "POST",
            path: "/v1/events/accept",
            body: batch
        )
        guard response.status == 200,
              let object = try JSONSerialization.jsonObject(with: response.body)
                as? [String: Any],
              Set(object.keys) == Set(["status", "cursor"]),
              object["status"] as? String == "accepted",
              let cursorObject = object["cursor"] as? [String: Any],
              Set(cursorObject.keys) == Set(["cursor", "updated_at_ms"]) else {
            throw AssemblywrightMacDeveloperEventRelayError.eventCursorRejected
        }
        do {
            return try JSONDecoder().decode(
                AssemblywrightMacDeveloperAgentCursorSnapshot.self,
                from: JSONSerialization.data(withJSONObject: cursorObject)
            )
        } catch {
            throw AssemblywrightMacDeveloperEventRelayError.invalidAgentResponse
        }
    }

    func executeFixtureJob(_ job: Data) async throws -> Data {
        guard configurationFixtureJobsEnabled else {
            throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
        }
        let response = try await send(
            method: "POST",
            path: "/v1/jobs/execute",
            body: job
        )
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= 16 * 1_024 else {
            throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
        }
        return response.body
    }

    func cancelFixtureJob(_ instruction: Data) async throws -> Data {
        guard configurationFixtureJobsEnabled else {
            throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
        }
        let response = try await send(
            method: "POST",
            path: "/v1/jobs/cancel",
            body: instruction
        )
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= 16 * 1_024 else {
            throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
        }
        return response.body
    }

    func executeMLXJob(_ job: Data) async throws -> Data {
        guard configurationMLXJobsEnabled else {
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
        }
        let response = try await send(
            method: "POST",
            path: "/v1/mlx/jobs/execute",
            body: job,
            using: mlxExecutionTransport
        )
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= 1_024 * 1_024 else {
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
        }
        return response.body
    }

    func cancelMLXJob(_ instruction: Data) async throws -> Data {
        guard configurationMLXJobsEnabled else {
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
        }
        let response = try await send(
            method: "POST",
            path: "/v1/mlx/jobs/cancel",
            body: instruction
        )
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= 16 * 1_024 else {
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
        }
        return response.body
    }

    func admitLocalCodingSnapshot(_ job: Data) async throws {
        guard configurationLocalCodingSnapshotsEnabled else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
        let response = try await send(
            method: "POST",
            path: "/v1/local-coding/snapshots/admit",
            body: job
        )
        guard response.status == 200,
              let object = AssemblywrightMacDeveloperEventRelay.strictJSONObject(
                response.body
              ),
              Set(object.keys) == Set(["status", "next_offset"]),
              object["status"] as? String == "snapshot_admitted",
              AssemblywrightMacDeveloperEventRelay.strictInteger(object["next_offset"]) == 0
        else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
    }

    func acceptLocalCodingSnapshotChunk(
        _ chunk: Data
    ) async throws -> AssemblywrightMacDeveloperAgentSnapshotChunkAcceptance {
        guard configurationLocalCodingSnapshotsEnabled else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
        let response = try await send(
            method: "POST",
            path: "/v1/local-coding/snapshots/accept",
            body: chunk
        )
        if response.status == 202 {
            guard let object = AssemblywrightMacDeveloperEventRelay.strictJSONObject(
                    response.body
                  ),
                  Set(object.keys) == Set(["status", "next_offset"]),
                  object["status"] as? String == "snapshot_chunk_accepted",
                  AssemblywrightMacDeveloperEventRelay.strictInteger(object["next_offset"])
                    != nil else {
                throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
            }
            guard let nextOffset = AssemblywrightMacDeveloperEventRelay.strictInteger(
                object["next_offset"]
            ) else {
                throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
            }
            return .nextOffset(nextOffset)
        }
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= 32 * 1_024 else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
        return .result(response.body)
    }

    func cancelLocalCodingSnapshot(_ instruction: Data) async throws -> Data {
        guard configurationLocalCodingSnapshotsEnabled else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
        let response = try await send(
            method: "POST",
            path: "/v1/local-coding/snapshots/cancel",
            body: instruction
        )
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= 16 * 1_024 else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
        return response.body
    }

    func stop() async throws {
        guard !stopped else { return }
        if process.isRunning { process.terminate() }
        let gracefulDeadline = ContinuousClock.now + .seconds(3)
        while process.isRunning, ContinuousClock.now < gracefulDeadline {
            try? await Task.sleep(for: .milliseconds(20))
        }
        if process.isRunning {
            let result = kill(process.processIdentifier, SIGKILL)
            guard result == 0 || errno == ESRCH else {
                throw AssemblywrightMacDeveloperEventRelayError.teardownFailed
            }
        }
        let killDeadline = ContinuousClock.now + .seconds(1)
        while process.isRunning, ContinuousClock.now < killDeadline {
            try? await Task.sleep(for: .milliseconds(20))
        }
        guard !process.isRunning else {
            throw AssemblywrightMacDeveloperEventRelayError.teardownFailed
        }
        process.waitUntilExit()
        try? FileManager.default.removeItem(at: runtimeDirectoryURL)
        stopped = true
    }

    private func waitUntilHealthy() async throws {
        for _ in 0 ..< 100 {
            if !process.isRunning {
                throw AssemblywrightMacDeveloperEventRelayError.agentUnavailable
            }
            if (try? await health()) != nil { return }
            try await Task.sleep(for: .milliseconds(20))
        }
        throw AssemblywrightMacDeveloperEventRelayError.agentUnavailable
    }

    private var expectedBoundary: String {
        if configurationFixtureJobsEnabled {
            return "metadata_cursor_plus_in_memory_public_fixture_jobs_no_retention"
        }
        if configurationMLXJobsEnabled {
            return "metadata_cursor_plus_bounded_public_mlx_jobs_no_retention"
        }
        if configurationLocalCodingSnapshotsEnabled {
            return "metadata_cursor_plus_fixed_contained_coding_fixture_ephemeral_workspace"
        }
        return "metadata_only_no_authoritative_state"
    }

    private func send(
        method: String,
        path: String,
        body: Data? = nil,
        using selectedTransport: DarwinAssemblywrightUnixSocketTransport? = nil
    ) async throws -> AssemblywrightIPCTransportResponse {
        try await (selectedTransport ?? transport).send(
            AssemblywrightIPCTransportRequest(
                method: method,
                path: path,
                authorization: "Bearer \(bearerToken)",
                accept: "application/json",
                contentType: "application/json",
                body: body
            ),
            to: socketURL
        )
    }

    private static func makeRuntimeDirectory() throws -> URL {
        var template = Array("/tmp/assemblywright-agent.XXXXXX".utf8CString)
        let path = template.withUnsafeMutableBufferPointer { buffer in
            mkdtemp(buffer.baseAddress)
        }
        guard let path else {
            throw AssemblywrightMacDeveloperEventRelayError.unsafeRuntimeDirectory
        }
        let url = URL(
            fileURLWithPath: String(cString: path),
            isDirectory: true
        ).standardizedFileURL
        var metadata = stat()
        guard lstat(url.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFDIR,
              metadata.st_uid == geteuid(),
              chmod(url.path, 0o700) == 0 else {
            try? FileManager.default.removeItem(at: url)
            throw AssemblywrightMacDeveloperEventRelayError.unsafeRuntimeDirectory
        }
        return url
    }

    private static func randomBearer() throws -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
            throw AssemblywrightMacDeveloperEventRelayError.randomUnavailable
        }
        return Data(bytes).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

private extension Data {
    var assemblywrightHexString: String {
        map { String(format: "%02x", $0) }.joined()
    }
}
