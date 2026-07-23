import Darwin
import CryptoKit
import Foundation
import Security

private actor JarvisMacFixtureRaceResolution {
    private var resolved = false

    func markResolved() {
        resolved = true
    }

    func isResolved() -> Bool {
        resolved
    }
}

public struct JarvisMacDeveloperEventRelayConfiguration: Equatable, Sendable {
    public static let version = 2
    public static let maximumDocumentBytes = 16 * 1_024

    public let agentExecutableURL: URL
    public let agentDataDirectoryURL: URL
    public let fixtureJobsEnabled: Bool

    public init(
        agentExecutableURL: URL,
        agentDataDirectoryURL: URL,
        fixtureJobsEnabled: Bool = false
    ) {
        self.agentExecutableURL = agentExecutableURL.standardizedFileURL
        self.agentDataDirectoryURL = agentDataDirectoryURL.standardizedFileURL
        self.fixtureJobsEnabled = fixtureJobsEnabled
    }

    public func encodeStartupDocument() throws -> Data {
        let object: [String: Any] = [
            "version": Self.version,
            "agent_executable_path": agentExecutableURL.path,
            "agent_data_dir": agentDataDirectoryURL.path,
            "fixture_jobs_enabled": fixtureJobsEnabled
        ]
        let data = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        guard data.count <= Self.maximumDocumentBytes else {
            throw JarvisMacDeveloperEventRelayError.invalidStartupDocument
        }
        return data
    }

    public static func decodeStartupDocument(_ data: Data) throws -> Self {
        guard !data.isEmpty, data.count <= maximumDocumentBytes,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys) == Set([
                  "version", "agent_executable_path", "agent_data_dir",
                  "fixture_jobs_enabled"
              ]),
              let version = object["version"] as? NSNumber,
              CFGetTypeID(version) != CFBooleanGetTypeID(),
              version.intValue == Self.version,
              let executablePath = object["agent_executable_path"] as? String,
              let dataDirectoryPath = object["agent_data_dir"] as? String,
              let fixtureJobsEnabled = object["fixture_jobs_enabled"] as? Bool else {
            throw JarvisMacDeveloperEventRelayError.invalidStartupDocument
        }
        let configuration = Self(
            agentExecutableURL: URL(fileURLWithPath: executablePath),
            agentDataDirectoryURL: URL(fileURLWithPath: dataDirectoryPath, isDirectory: true),
            fixtureJobsEnabled: fixtureJobsEnabled
        )
        try configuration.validatePaths()
        return configuration
    }

    public func validatePaths() throws {
        for url in [agentExecutableURL, agentDataDirectoryURL] {
            guard url.isFileURL,
                  url.path.hasPrefix("/"),
                  !url.path.contains("\0"),
                  !url.path.split(separator: "/").contains(".."),
                  url.path.utf8.count <= 4 * 1_024 else {
                throw JarvisMacDeveloperEventRelayError.invalidStartupDocument
            }
        }
    }
}

public enum JarvisMacDeveloperEventRelayError: Error, Equatable, Sendable {
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
    case teardownFailed
}

public struct JarvisMacDeveloperEventCursor: Codable, Equatable, Sendable {
    public let streamID: UUID
    public let sequence: UInt64

    enum CodingKeys: String, CodingKey {
        case streamID = "stream_id"
        case sequence
    }
}

public struct JarvisMacDeveloperAgentCursorSnapshot: Codable, Equatable, Sendable {
    public let cursor: JarvisMacDeveloperEventCursor?
    public let updatedAtMilliseconds: UInt64?

    enum CodingKeys: String, CodingKey {
        case cursor
        case updatedAtMilliseconds = "updated_at_ms"
    }
}

public struct JarvisMacDeveloperEventRelayProgress: Equatable, Sendable {
    public let cursor: JarvisMacDeveloperEventCursor
    public let acceptedEventCount: Int
    public let hasMore: Bool
}

public protocol JarvisMacBridgeEventRelaying: Sendable {
    func relayEvents(
        using session: any JarvisMacBridgeSession
    ) async throws -> JarvisMacDeveloperEventRelayProgress
    func stop() async throws
}

public protocol JarvisMacDeveloperAgentSession: Sendable {
    func health() async throws -> JarvisMacDeveloperAgentCursorSnapshot
    func accept(batch: Data) async throws -> JarvisMacDeveloperAgentCursorSnapshot
    func executeFixtureJob(_ job: Data) async throws -> Data
    func cancelFixtureJob(_ instruction: Data) async throws -> Data
    func stop() async throws
}

public protocol JarvisMacDeveloperAgentLaunching: Sendable {
    func launch(
        configuration: JarvisMacDeveloperEventRelayConfiguration
    ) async throws -> any JarvisMacDeveloperAgentSession
}

public actor JarvisMacDeveloperEventRelay: JarvisMacBridgeEventRelaying {
    public static let remoteEventsPath = "/v1/distributed/events/next"
    public static let remoteLeasePath = "/v1/distributed/leases/next"
    public static let remoteResultPath = "/v1/distributed/results"
    public static let remoteCancellationPath = "/v1/distributed/cancellations/next"
    public static let remoteCancellationAcknowledgementPath =
        "/v1/distributed/cancellations/ack"
    public static let maximumEventsPerBatch = 64

    private let configuration: JarvisMacDeveloperEventRelayConfiguration
    private let deviceID: UUID?
    private let launcher: any JarvisMacDeveloperAgentLaunching
    private var agent: (any JarvisMacDeveloperAgentSession)?
    private var stopped = false

    public init(
        configuration: JarvisMacDeveloperEventRelayConfiguration,
        deviceID: UUID? = nil,
        launcher: any JarvisMacDeveloperAgentLaunching =
            FoundationJarvisMacDeveloperAgentLauncher()
    ) {
        self.configuration = configuration
        self.deviceID = deviceID
        self.launcher = launcher
    }

    public func relayEvents(
        using session: any JarvisMacBridgeSession
    ) async throws -> JarvisMacDeveloperEventRelayProgress {
        guard !stopped else {
            throw JarvisMacDeveloperEventRelayError.agentUnavailable
        }
        let activeAgent: any JarvisMacDeveloperAgentSession
        if let agent {
            activeAgent = agent
        } else {
            let launched = try await launcher.launch(configuration: configuration)
            agent = launched
            activeAgent = launched
        }
        let before = try await activeAgent.health()
        let request = try Self.eventRequest(
            connectionEpoch: session.connectionEpoch,
            after: before.cursor
        )
        let response = try await session.send(
            JarvisMacBridgeHTTPRequest(
                method: "POST",
                path: Self.remoteEventsPath,
                body: request
            )
        )
        let batch = try Self.validateBatchResponse(response)
        let accepted = try await activeAgent.accept(batch: response.body)
        guard accepted.cursor == batch.cursor else {
            throw JarvisMacDeveloperEventRelayError.eventCursorRejected
        }
        if configuration.fixtureJobsEnabled {
            guard let deviceID else {
                throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
            }
            try await relayOneFixtureJob(
                using: session,
                deviceID: deviceID,
                agent: activeAgent
            )
        }
        return JarvisMacDeveloperEventRelayProgress(
            cursor: batch.cursor,
            acceptedEventCount: batch.eventCount,
            hasMore: batch.hasMore
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

    private func relayOneFixtureJob(
        using session: any JarvisMacBridgeSession,
        deviceID: UUID,
        agent: any JarvisMacDeveloperAgentSession
    ) async throws {
        let leaseRequest = try JSONSerialization.data(
            withJSONObject: [
                "device_id": deviceID.uuidString.lowercased(),
                "connection_epoch": NSNumber(value: session.connectionEpoch)
            ],
            options: [.sortedKeys]
        )
        let leased = try await session.send(
            JarvisMacBridgeHTTPRequest(
                method: "POST",
                path: Self.remoteLeasePath,
                body: leaseRequest
            )
        )
        if leased.status == 204 {
            guard leased.body.isEmpty else {
                throw JarvisMacDeveloperEventRelayError.invalidMasterResponse
            }
            return
        }
        guard leased.status == 200 else {
            throw JarvisMacDeveloperEventRelayError.invalidMasterResponse
        }
        let job = try Self.validateFixtureJob(
            leased.body,
            expectedConnectionEpoch: session.connectionEpoch
        )

        let resolution = JarvisMacFixtureRaceResolution()
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
                throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
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
                JarvisMacBridgeHTTPRequest(
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
                JarvisMacBridgeHTTPRequest(
                    method: "POST",
                    path: Self.remoteCancellationAcknowledgementPath,
                    body: acknowledgement
                )
            )
            try Self.validateAcceptedCancellation(accepted)
        case .timedOut:
            try await agent.stop()
            self.agent = nil
            throw JarvisMacDeveloperEventRelayError.fixtureJobTimedOut
        case .settled:
            throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
        }
    }

    private static func pollFixtureCancellation(
        using session: any JarvisMacBridgeSession,
        job: ValidatedFixtureJob
    ) async throws -> ValidatedCancellation? {
        let request = try JSONSerialization.data(
            withJSONObject: [
                "protocol_version": Int(JarvisMacMTLSBridgeTransport.protocolVersion),
                "connection_epoch": NSNumber(value: session.connectionEpoch)
            ],
            options: [.sortedKeys]
        )
        let response = try await session.send(
            JarvisMacBridgeHTTPRequest(
                method: "POST",
                path: remoteCancellationPath,
                body: request
            )
        )
        if response.status == 204 {
            guard response.body.isEmpty else {
                throw JarvisMacDeveloperEventRelayError.invalidMasterResponse
            }
            return nil
        }
        guard response.status == 200,
              let object = strictJSONObject(response.body),
              Set(object.keys) == Set([
                  "protocol_version", "connection_epoch", "sequence",
                  "task_id", "step_id", "attempt_id", "lease_id",
                  "cancellation_id", "deadline_after_ms"
              ]),
              strictInteger(object["protocol_version"])
                == UInt64(JarvisMacMTLSBridgeTransport.protocolVersion),
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
            throw JarvisMacDeveloperEventRelayError.invalidMasterResponse
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
                == UInt64(JarvisMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == expectedConnectionEpoch,
              let sequence = strictInteger(object["sequence"]),
              sequence > 0,
              let taskID = strictUUID(object["task_id"]),
              let stepID = strictUUID(object["step_id"]),
              let attemptID = strictUUID(object["attempt_id"]),
              let leaseID = strictUUID(object["lease_id"]),
              let cancellationID = strictUUID(object["cancellation_id"]),
              object["capability_id"] as? String == "fixture.reasoning",
              object["selected_model"] as? String == "jarvis-fixture-v1",
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
              let contextData = try? JSONSerialization.data(
                  withJSONObject: context,
                  options: [.sortedKeys]
              ),
              contextData.count <= 8_192,
              Array(SHA256.hash(data: contextData)) == contextDigest else {
            throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
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
                == UInt64(JarvisMacMTLSBridgeTransport.protocolVersion),
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
              let payloadData = try? JSONSerialization.data(
                  withJSONObject: payload,
                  options: [.sortedKeys]
              ),
              payloadData.count <= 8_192,
              Array(SHA256.hash(data: payloadData)) == payloadDigest else {
            throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
        }
        return payloadDigest
    }

    private static func validateAcceptedFixtureResult(
        _ response: JarvisMacBridgeHTTPResponse,
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
            throw JarvisMacDeveloperEventRelayError.invalidMasterResponse
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
                == UInt64(JarvisMacMTLSBridgeTransport.protocolVersion),
              strictInteger(object["connection_epoch"]) == job.connectionEpoch,
              strictInteger(object["sequence"]).map({ $0 > instruction.sequence }) == true,
              strictUUID(object["task_id"]) == job.taskID,
              strictUUID(object["step_id"]) == job.stepID,
              strictUUID(object["attempt_id"]) == job.attemptID,
              strictUUID(object["lease_id"]) == job.leaseID,
              strictUUID(object["cancellation_id"]) == job.cancellationID,
              object["status"] as? String == "cancelled" else {
            throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
        }
    }

    private static func validateAcceptedCancellation(
        _ response: JarvisMacBridgeHTTPResponse
    ) throws {
        guard response.status == 200,
              let object = strictJSONObject(response.body),
              Set(object.keys) == Set(["accepted", "status"]),
              object["accepted"] as? Bool == true,
              object["status"] as? String == "cancelled" else {
            throw JarvisMacDeveloperEventRelayError.invalidMasterResponse
        }
    }

    private static func strictJSONObject(_ data: Data) -> [String: Any]? {
        guard !data.isEmpty,
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

    private static func eventRequest(
        connectionEpoch: UInt64,
        after: JarvisMacDeveloperEventCursor?
    ) throws -> Data {
        guard connectionEpoch > 0 else {
            throw JarvisMacDeveloperEventRelayError.invalidMasterResponse
        }
        var object: [String: Any] = [
            "protocol_version": Int(JarvisMacMTLSBridgeTransport.protocolVersion),
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
        let cursor: JarvisMacDeveloperEventCursor
        let eventCount: Int
        let hasMore: Bool
    }

    private static func validateBatchResponse(
        _ response: JarvisMacBridgeHTTPResponse
    ) throws -> ValidatedBatch {
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= DarwinJarvisUnixSocketTransport.maximumRequestBodyBytes,
              let object = try JSONSerialization.jsonObject(with: response.body)
                as? [String: Any],
              Set(object.keys) == Set([
                  "protocol_version", "stream_id", "after_sequence",
                  "next_sequence", "events", "has_more"
              ]),
              Self.strictInteger(object["protocol_version"])
                == UInt64(JarvisMacMTLSBridgeTransport.protocolVersion),
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
            throw JarvisMacDeveloperEventRelayError.invalidMasterResponse
        }
        var expectedSequence = afterSequence
        for event in events {
            expectedSequence += 1
            guard Set(event.keys) == Set([
                "protocol_version", "cursor", "occurred_at_ms", "kind",
                "task_id", "step_id", "device_id", "connection_epoch"
            ]),
            Self.strictInteger(event["protocol_version"])
                == UInt64(JarvisMacMTLSBridgeTransport.protocolVersion),
            Self.strictInteger(event["occurred_at_ms"]).map({ $0 > 0 }) == true,
            let cursor = event["cursor"] as? [String: Any],
            Set(cursor.keys) == Set(["stream_id", "sequence"]),
            cursor["stream_id"] as? String == streamText,
            Self.strictInteger(cursor["sequence"]) == expectedSequence,
            event["kind"] is String else {
                throw JarvisMacDeveloperEventRelayError.invalidMasterResponse
            }
        }
        return ValidatedBatch(
            cursor: JarvisMacDeveloperEventCursor(
                streamID: streamID,
                sequence: nextSequence
            ),
            eventCount: events.count,
            hasMore: hasMore
        )
    }

    private static func strictInteger(_ value: Any?) -> UInt64? {
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

public struct FoundationJarvisMacDeveloperAgentLauncher:
    JarvisMacDeveloperAgentLaunching, Sendable
{
    public init() {}

    public func launch(
        configuration: JarvisMacDeveloperEventRelayConfiguration
    ) async throws -> any JarvisMacDeveloperAgentSession {
        try configuration.validatePaths()
        let agent = try SecurityJarvisMacRelayCodeIdentity.staticIdentity(
            executableURL: configuration.agentExecutableURL
        )
        let helper = try SecurityJarvisMacRelayCodeIdentity.currentProcessIdentity()
        return try await FoundationJarvisMacDeveloperAgentSession.start(
            configuration: configuration,
            agentIdentity: agent,
            helperIdentity: helper
        )
    }
}

private struct JarvisMacRelayCodeIdentity: Sendable {
    let executableURL: URL
    let identifier: String
    let cdHash: Data
    let requirement: String
}

private enum SecurityJarvisMacRelayCodeIdentity {
    static func staticIdentity(executableURL: URL) throws -> JarvisMacRelayCodeIdentity {
        let standardized = executableURL.standardizedFileURL
        var metadata = stat()
        guard standardized.isFileURL,
              standardized.path.hasPrefix("/"),
              lstat(standardized.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFREG,
              access(standardized.path, X_OK) == 0 else {
            throw JarvisMacDeveloperEventRelayError.invalidAgentExecutable
        }
        var code: SecStaticCode?
        guard SecStaticCodeCreateWithPath(standardized as CFURL, [], &code) == errSecSuccess,
              let code,
              SecStaticCodeCheckValidity(
                  code,
                  SecCSFlags(rawValue: kSecCSStrictValidate),
                  nil
              ) == errSecSuccess else {
            throw JarvisMacDeveloperEventRelayError.invalidAgentSignature
        }
        return try identity(from: code, expectedURL: standardized)
    }

    static func currentProcessIdentity() throws -> JarvisMacRelayCodeIdentity {
        var dynamicCode: SecCode?
        guard SecCodeCopySelf([], &dynamicCode) == errSecSuccess,
              let dynamicCode,
              SecCodeCheckValidity(
                  dynamicCode,
                  SecCSFlags(rawValue: kSecCSStrictValidate),
                  nil
              ) == errSecSuccess else {
            throw JarvisMacDeveloperEventRelayError.invalidHelperIdentity
        }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(dynamicCode, [], &staticCode) == errSecSuccess,
              let staticCode else {
            throw JarvisMacDeveloperEventRelayError.invalidHelperIdentity
        }
        return try identity(from: staticCode, expectedURL: nil)
    }

    static func validateRunning(
        processIdentifier: Int32,
        expected: JarvisMacRelayCodeIdentity
    ) throws {
        let attributes = [
            kSecGuestAttributePid as String: NSNumber(value: processIdentifier)
        ] as CFDictionary
        var dynamicCode: SecCode?
        guard SecCodeCopyGuestWithAttributes(nil, attributes, [], &dynamicCode)
                == errSecSuccess,
              let dynamicCode else {
            throw JarvisMacDeveloperEventRelayError.agentIdentityMismatch
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
            throw JarvisMacDeveloperEventRelayError.agentIdentityMismatch
        }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(dynamicCode, [], &staticCode) == errSecSuccess,
              let staticCode else {
            throw JarvisMacDeveloperEventRelayError.agentIdentityMismatch
        }
        let actual = try identity(from: staticCode, expectedURL: expected.executableURL)
        guard actual.cdHash == expected.cdHash,
              actual.identifier == expected.identifier else {
            throw JarvisMacDeveloperEventRelayError.agentIdentityMismatch
        }
    }

    private static func identity(
        from code: SecStaticCode,
        expectedURL: URL?
    ) throws -> JarvisMacRelayCodeIdentity {
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
            throw JarvisMacDeveloperEventRelayError.invalidAgentSignature
        }
        let standardized = executableURL.standardizedFileURL
        if let expectedURL,
           standardized.path != expectedURL.standardizedFileURL.path {
            throw JarvisMacDeveloperEventRelayError.invalidAgentSignature
        }
        let requirement =
            "identifier \"\(identifier)\" and cdhash H\"\(cdHash.jarvisHexString)\""
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
            throw JarvisMacDeveloperEventRelayError.invalidAgentSignature
        }
        return JarvisMacRelayCodeIdentity(
            executableURL: standardized,
            identifier: identifier,
            cdHash: cdHash,
            requirement: requirement
        )
    }
}

private actor FoundationJarvisMacDeveloperAgentSession:
    JarvisMacDeveloperAgentSession
{
    private static let maximumAgentStartupBytes = 16 * 1_024
    private let process: Process
    private let runtimeDirectoryURL: URL
    private let socketURL: URL
    private let bearerToken: String
    private let transport: DarwinJarvisUnixSocketTransport
    private let configurationFixtureJobsEnabled: Bool
    private var stopped = false

    private init(
        process: Process,
        runtimeDirectoryURL: URL,
        socketURL: URL,
        bearerToken: String,
        transport: DarwinJarvisUnixSocketTransport,
        configurationFixtureJobsEnabled: Bool
    ) {
        self.process = process
        self.runtimeDirectoryURL = runtimeDirectoryURL
        self.socketURL = socketURL
        self.bearerToken = bearerToken
        self.transport = transport
        self.configurationFixtureJobsEnabled = configurationFixtureJobsEnabled
    }

    static func start(
        configuration: JarvisMacDeveloperEventRelayConfiguration,
        agentIdentity: JarvisMacRelayCodeIdentity,
        helperIdentity: JarvisMacRelayCodeIdentity
    ) async throws -> FoundationJarvisMacDeveloperAgentSession {
        let runtimeDirectoryURL = try makeRuntimeDirectory()
        let socketURL = runtimeDirectoryURL.appendingPathComponent("relay.sock")
        guard socketURL.path.utf8.count < 104 else {
            try? FileManager.default.removeItem(at: runtimeDirectoryURL)
            throw JarvisMacDeveloperEventRelayError.unsafeRuntimeDirectory
        }
        let bearer = try randomBearer()
        let peerPolicy = JarvisIPCPeerIdentityPolicy(
            profile: .adhocExact,
            peerCodeRequirement: helperIdentity.requirement,
            coreCodeRequirement: agentIdentity.requirement,
            expectedCoreCDHash: agentIdentity.cdHash,
            expectedCoreExecutableURL: agentIdentity.executableURL
        )
        let transport = DarwinJarvisUnixSocketTransport(
            timeoutSeconds: 5,
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
            throw JarvisMacDeveloperEventRelayError.agentLaunchFailed
        }
        do {
            try SecurityJarvisMacRelayCodeIdentity.validateRunning(
                processIdentifier: process.processIdentifier,
                expected: agentIdentity
            )
            let startup: [String: Any] = [
                "version": 1,
                "supervised_parent_pid": Int(getpid()),
                "socket_path": socketURL.path,
                "peer_code_requirement": helperIdentity.requirement,
                "peer_identity_profile": JarvisIPCPeerIdentityProfile.adhocExact.rawValue,
                "bearer_token": bearer,
                "fixture_jobs_enabled": configuration.fixtureJobsEnabled
            ]
            let startupData = try JSONSerialization.data(
                withJSONObject: startup,
                options: [.sortedKeys]
            )
            guard startupData.count <= maximumAgentStartupBytes else {
                throw JarvisMacDeveloperEventRelayError.invalidStartupDocument
            }
            try input.fileHandleForWriting.write(contentsOf: startupData)
            try input.fileHandleForWriting.close()
            let session = FoundationJarvisMacDeveloperAgentSession(
                process: process,
                runtimeDirectoryURL: runtimeDirectoryURL,
                socketURL: socketURL,
                bearerToken: bearer,
                transport: transport,
                configurationFixtureJobsEnabled: configuration.fixtureJobsEnabled
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

    func health() async throws -> JarvisMacDeveloperAgentCursorSnapshot {
        let response = try await send(method: "GET", path: "/health")
        guard response.status == 200,
              let object = try JSONSerialization.jsonObject(with: response.body)
                as? [String: Any],
              Set(object.keys) == Set([
                  "status", "mode", "protocol_version", "schema_version",
                  "cursor", "boundary", "fixture_jobs_enabled"
              ]),
              object["status"] as? String == "ok",
              object["mode"] as? String == "developer_event_relay",
              (object["protocol_version"] as? NSNumber)?.intValue
                == Int(JarvisMacMTLSBridgeTransport.protocolVersion),
              (object["schema_version"] as? NSNumber)?.intValue == 1,
              object["fixture_jobs_enabled"] as? Bool
                == configurationFixtureJobsEnabled,
              object["boundary"] as? String
                == (configurationFixtureJobsEnabled
                    ? "metadata_cursor_plus_in_memory_public_fixture_jobs_no_retention"
                    : "metadata_only_no_authoritative_state"),
              let cursorObject = object["cursor"] as? [String: Any],
              Set(cursorObject.keys) == Set(["cursor", "updated_at_ms"]) else {
            throw JarvisMacDeveloperEventRelayError.invalidAgentResponse
        }
        do {
            return try JSONDecoder().decode(
                JarvisMacDeveloperAgentCursorSnapshot.self,
                from: JSONSerialization.data(withJSONObject: cursorObject)
            )
        } catch {
            throw JarvisMacDeveloperEventRelayError.invalidAgentResponse
        }
    }

    func accept(batch: Data) async throws -> JarvisMacDeveloperAgentCursorSnapshot {
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
            throw JarvisMacDeveloperEventRelayError.eventCursorRejected
        }
        do {
            return try JSONDecoder().decode(
                JarvisMacDeveloperAgentCursorSnapshot.self,
                from: JSONSerialization.data(withJSONObject: cursorObject)
            )
        } catch {
            throw JarvisMacDeveloperEventRelayError.invalidAgentResponse
        }
    }

    func executeFixtureJob(_ job: Data) async throws -> Data {
        guard configurationFixtureJobsEnabled else {
            throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
        }
        let response = try await send(
            method: "POST",
            path: "/v1/jobs/execute",
            body: job
        )
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= 16 * 1_024 else {
            throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
        }
        return response.body
    }

    func cancelFixtureJob(_ instruction: Data) async throws -> Data {
        guard configurationFixtureJobsEnabled else {
            throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
        }
        let response = try await send(
            method: "POST",
            path: "/v1/jobs/cancel",
            body: instruction
        )
        guard response.status == 200,
              !response.body.isEmpty,
              response.body.count <= 16 * 1_024 else {
            throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
        }
        return response.body
    }

    func stop() async throws {
        guard !stopped else { return }
        if process.isRunning { process.terminate() }
        let gracefulDeadline = ContinuousClock.now + .milliseconds(500)
        while process.isRunning, ContinuousClock.now < gracefulDeadline {
            try? await Task.sleep(for: .milliseconds(20))
        }
        if process.isRunning {
            let result = kill(process.processIdentifier, SIGKILL)
            guard result == 0 || errno == ESRCH else {
                throw JarvisMacDeveloperEventRelayError.teardownFailed
            }
        }
        let killDeadline = ContinuousClock.now + .seconds(1)
        while process.isRunning, ContinuousClock.now < killDeadline {
            try? await Task.sleep(for: .milliseconds(20))
        }
        guard !process.isRunning else {
            throw JarvisMacDeveloperEventRelayError.teardownFailed
        }
        process.waitUntilExit()
        try? FileManager.default.removeItem(at: runtimeDirectoryURL)
        stopped = true
    }

    private func waitUntilHealthy() async throws {
        for _ in 0 ..< 100 {
            if !process.isRunning {
                throw JarvisMacDeveloperEventRelayError.agentUnavailable
            }
            if (try? await health()) != nil { return }
            try await Task.sleep(for: .milliseconds(20))
        }
        throw JarvisMacDeveloperEventRelayError.agentUnavailable
    }

    private func send(
        method: String,
        path: String,
        body: Data? = nil
    ) async throws -> JarvisIPCTransportResponse {
        try await transport.send(
            JarvisIPCTransportRequest(
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
        var template = Array("/tmp/jarvis-agent.XXXXXX".utf8CString)
        let path = template.withUnsafeMutableBufferPointer { buffer in
            mkdtemp(buffer.baseAddress)
        }
        guard let path else {
            throw JarvisMacDeveloperEventRelayError.unsafeRuntimeDirectory
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
            throw JarvisMacDeveloperEventRelayError.unsafeRuntimeDirectory
        }
        return url
    }

    private static func randomBearer() throws -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
            throw JarvisMacDeveloperEventRelayError.randomUnavailable
        }
        return Data(bytes).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

private extension Data {
    var jarvisHexString: String {
        map { String(format: "%02x", $0) }.joined()
    }
}
