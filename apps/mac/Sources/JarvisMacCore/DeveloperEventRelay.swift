import Darwin
import Foundation
import Security

public struct JarvisMacDeveloperEventRelayConfiguration: Equatable, Sendable {
    public static let version = 1
    public static let maximumDocumentBytes = 16 * 1_024

    public let agentExecutableURL: URL
    public let agentDataDirectoryURL: URL

    public init(agentExecutableURL: URL, agentDataDirectoryURL: URL) {
        self.agentExecutableURL = agentExecutableURL.standardizedFileURL
        self.agentDataDirectoryURL = agentDataDirectoryURL.standardizedFileURL
    }

    public func encodeStartupDocument() throws -> Data {
        let object: [String: Any] = [
            "version": Self.version,
            "agent_executable_path": agentExecutableURL.path,
            "agent_data_dir": agentDataDirectoryURL.path
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
              Set(object.keys) == Set(["version", "agent_executable_path", "agent_data_dir"]),
              let version = object["version"] as? NSNumber,
              CFGetTypeID(version) != CFBooleanGetTypeID(),
              version.intValue == Self.version,
              let executablePath = object["agent_executable_path"] as? String,
              let dataDirectoryPath = object["agent_data_dir"] as? String else {
            throw JarvisMacDeveloperEventRelayError.invalidStartupDocument
        }
        let configuration = Self(
            agentExecutableURL: URL(fileURLWithPath: executablePath),
            agentDataDirectoryURL: URL(fileURLWithPath: dataDirectoryPath, isDirectory: true)
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
    func stop() async throws
}

public protocol JarvisMacDeveloperAgentLaunching: Sendable {
    func launch(
        configuration: JarvisMacDeveloperEventRelayConfiguration
    ) async throws -> any JarvisMacDeveloperAgentSession
}

public actor JarvisMacDeveloperEventRelay: JarvisMacBridgeEventRelaying {
    public static let remoteEventsPath = "/v1/distributed/events/next"
    public static let maximumEventsPerBatch = 64

    private let configuration: JarvisMacDeveloperEventRelayConfiguration
    private let launcher: any JarvisMacDeveloperAgentLaunching
    private var agent: (any JarvisMacDeveloperAgentSession)?
    private var stopped = false

    public init(
        configuration: JarvisMacDeveloperEventRelayConfiguration,
        launcher: any JarvisMacDeveloperAgentLaunching =
            FoundationJarvisMacDeveloperAgentLauncher()
    ) {
        self.configuration = configuration
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
    private var stopped = false

    private init(
        process: Process,
        runtimeDirectoryURL: URL,
        socketURL: URL,
        bearerToken: String,
        transport: DarwinJarvisUnixSocketTransport
    ) {
        self.process = process
        self.runtimeDirectoryURL = runtimeDirectoryURL
        self.socketURL = socketURL
        self.bearerToken = bearerToken
        self.transport = transport
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
                "bearer_token": bearer
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
                transport: transport
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
                  "cursor", "boundary"
              ]),
              object["status"] as? String == "ok",
              object["mode"] as? String == "developer_event_relay",
              (object["protocol_version"] as? NSNumber)?.intValue
                == Int(JarvisMacMTLSBridgeTransport.protocolVersion),
              (object["schema_version"] as? NSNumber)?.intValue == 1,
              object["boundary"] as? String == "metadata_only_no_authoritative_state",
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
