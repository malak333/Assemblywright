import Darwin
import CryptoKit
import Foundation
import Testing
@testable import JarvisMacCore

private final class FakeBridgeIdentityStore: JarvisMacBridgeIdentityStore, @unchecked Sendable {
    var staged: JarvisMacEnrollmentInvitation?
    var installed: JarvisMacIssuedDeviceCertificate?
    var csrPEM = "-----BEGIN CERTIFICATE REQUEST-----\nZmFrZQ==\n-----END CERTIFICATE REQUEST-----\n"
    var publicKeySHA256 = String(repeating: "a", count: 64)
    var installedProfile: JarvisMacBridgeProfile?

    func stageIdentity(for invitation: JarvisMacEnrollmentInvitation) throws -> JarvisMacEnrollmentCSR {
        staged = invitation
        return JarvisMacEnrollmentCSR(
            schemaVersion: 1,
            status: "enrollment_csr_ready",
            grantID: invitation.grantID,
            deviceID: invitation.deviceID,
            csrPEM: csrPEM
        )
    }

    func loadStagedInvitation() throws -> JarvisMacEnrollmentInvitation? { staged }

    func install(
        _ receipt: JarvisMacIssuedDeviceCertificate,
        for invitation: JarvisMacEnrollmentInvitation
    ) throws -> JarvisMacBridgeProfile {
        installed = receipt
        let profile = JarvisMacBridgeProfile(
            deviceID: receipt.deviceID,
            deviceName: receipt.deviceName,
            role: receipt.role,
            registryRevision: receipt.registryRevision,
            capabilities: invitation.capabilities,
            masterEndpoint: invitation.masterEndpoint,
            certificateNotAfterMilliseconds: receipt.notAfterMilliseconds
        )
        installedProfile = profile
        return profile
    }

    func loadInstalledProfile() throws -> JarvisMacBridgeProfile? { installedProfile }
}

private actor FakeBridgeChannel: JarvisMacAuthenticatedTLSChannel {
    let exporter: Data?
    let response: JarvisMacBridgeHTTPResponse
    private(set) var requests: [JarvisMacBridgeHTTPRequest] = []
    private(set) var cancelled = false

    init(exporter: Data?, response: JarvisMacBridgeHTTPResponse) {
        self.exporter = exporter
        self.response = response
    }

    func tlsExporter(label: String, length: Int) async throws -> Data {
        guard let exporter else { throw JarvisMacDeveloperBridgeError.channelBindingUnavailable }
        #expect(label == "EXPORTER-Jarvis-Developer-Mode-v1")
        #expect(length == 32)
        return exporter
    }

    func send(_ request: JarvisMacBridgeHTTPRequest) async throws -> JarvisMacBridgeHTTPResponse {
        requests.append(request)
        return response
    }

    func cancel() async {
        cancelled = true
    }
}

private struct FakeBridgeChannelFactory: JarvisMacAuthenticatedTLSChannelFactory {
    let channel: FakeBridgeChannel

    func connect(profile _: JarvisMacBridgeProfile) async throws -> any JarvisMacAuthenticatedTLSChannel {
        channel
    }
}

private enum FakeSupervisorOutcome: Sendable {
    case response(JarvisMacBridgeHTTPResponse)
    case failure
}

private struct FakeSupervisorError: Error {}

private actor FakeSupervisorSession: JarvisMacBridgeSession {
    nonisolated let connectionEpoch: UInt64
    private var outcomes: [FakeSupervisorOutcome]
    private(set) var requests: [JarvisMacBridgeHTTPRequest] = []
    private(set) var cancelled = false

    init(connectionEpoch: UInt64, outcomes: [FakeSupervisorOutcome]) {
        self.connectionEpoch = connectionEpoch
        self.outcomes = outcomes
    }

    func send(_ request: JarvisMacBridgeHTTPRequest) async throws -> JarvisMacBridgeHTTPResponse {
        requests.append(request)
        guard !outcomes.isEmpty else { throw FakeSupervisorError() }
        switch outcomes.removeFirst() {
        case let .response(response):
            return response
        case .failure:
            throw FakeSupervisorError()
        }
    }

    func cancel() async { cancelled = true }
}

private actor FakeSupervisorConnector: JarvisMacBridgeConnecting {
    private var sessions: [FakeSupervisorSession]
    private(set) var connectCount = 0

    init(sessions: [FakeSupervisorSession]) {
        self.sessions = sessions
    }

    func connect(profile _: JarvisMacBridgeProfile) async throws -> any JarvisMacBridgeSession {
        connectCount += 1
        guard !sessions.isEmpty else { throw FakeSupervisorError() }
        return sessions.removeFirst()
    }
}

private actor FakeFixtureBridgeSession: JarvisMacBridgeSession {
    nonisolated let connectionEpoch: UInt64
    let eventBatch: Data
    let job: Data
    let cancellation: Data?
    let acceptedResult: Data
    let cancellationPollDelayMilliseconds: UInt64
    private(set) var requests: [JarvisMacBridgeHTTPRequest] = []
    private(set) var cancelled = false
    private var cancellationDelivered = false

    init(
        connectionEpoch: UInt64,
        eventBatch: Data,
        job: Data,
        cancellation: Data? = nil,
        acceptedResult: Data,
        cancellationPollDelayMilliseconds: UInt64 = 0
    ) {
        self.connectionEpoch = connectionEpoch
        self.eventBatch = eventBatch
        self.job = job
        self.cancellation = cancellation
        self.acceptedResult = acceptedResult
        self.cancellationPollDelayMilliseconds = cancellationPollDelayMilliseconds
    }

    func send(_ request: JarvisMacBridgeHTTPRequest) async throws
        -> JarvisMacBridgeHTTPResponse
    {
        requests.append(request)
        switch request.path {
        case JarvisMacDeveloperEventRelay.remoteEventsPath:
            return .init(status: 200, body: eventBatch)
        case JarvisMacDeveloperEventRelay.remoteLeasePath:
            return .init(status: 200, body: job)
        case JarvisMacDeveloperEventRelay.remoteCancellationPath:
            if cancellationPollDelayMilliseconds > 0 {
                do {
                    try await Task.sleep(
                        for: .milliseconds(cancellationPollDelayMilliseconds)
                    )
                } catch {
                    cancelled = true
                    throw error
                }
            }
            if let cancellation, !cancellationDelivered {
                cancellationDelivered = true
                return .init(status: 200, body: cancellation)
            }
            return .init(status: 204, body: Data())
        case JarvisMacDeveloperEventRelay.remoteResultPath:
            return .init(status: 200, body: acceptedResult)
        case JarvisMacDeveloperEventRelay.remoteCancellationAcknowledgementPath:
            return .init(
                status: 200,
                body: Data(#"{"accepted":true,"status":"cancelled"}"#.utf8)
            )
        default:
            throw FakeSupervisorError()
        }
    }

    func cancel() async {
        cancelled = true
    }
}

private actor FakeDeveloperAgentSession: JarvisMacDeveloperAgentSession {
    private var cursor: JarvisMacDeveloperEventCursor?
    private(set) var acceptedBatches: [Data] = []
    private(set) var executedJobs: [Data] = []
    private(set) var cancellations: [Data] = []
    var fixtureResult: Data?
    var cancellationAcknowledgement: Data?
    let fixtureDelayMilliseconds: UInt64
    private(set) var stopped = false

    init(
        cursor: JarvisMacDeveloperEventCursor? = nil,
        fixtureResult: Data? = nil,
        cancellationAcknowledgement: Data? = nil,
        fixtureDelayMilliseconds: UInt64 = 0
    ) {
        self.cursor = cursor
        self.fixtureResult = fixtureResult
        self.cancellationAcknowledgement = cancellationAcknowledgement
        self.fixtureDelayMilliseconds = fixtureDelayMilliseconds
    }

    func health() async throws -> JarvisMacDeveloperAgentCursorSnapshot {
        JarvisMacDeveloperAgentCursorSnapshot(
            cursor: cursor,
            updatedAtMilliseconds: cursor == nil ? nil : 1_000
        )
    }

    func accept(batch: Data) async throws -> JarvisMacDeveloperAgentCursorSnapshot {
        let object = try #require(
            JSONSerialization.jsonObject(with: batch) as? [String: Any]
        )
        let stream = try #require(object["stream_id"] as? String)
        let sequence = try #require(
            (object["next_sequence"] as? NSNumber)?.uint64Value
        )
        cursor = JarvisMacDeveloperEventCursor(
            streamID: try #require(UUID(uuidString: stream)),
            sequence: sequence
        )
        acceptedBatches.append(batch)
        return JarvisMacDeveloperAgentCursorSnapshot(
            cursor: cursor,
            updatedAtMilliseconds: 1_000
        )
    }

    func executeFixtureJob(_ job: Data) async throws -> Data {
        executedJobs.append(job)
        if fixtureDelayMilliseconds > 0 {
            try await Task.sleep(for: .milliseconds(fixtureDelayMilliseconds))
        }
        guard let fixtureResult else {
            throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
        }
        return fixtureResult
    }

    func cancelFixtureJob(_ instruction: Data) async throws -> Data {
        cancellations.append(instruction)
        guard let cancellationAcknowledgement else {
            throw JarvisMacDeveloperEventRelayError.fixtureJobRejected
        }
        return cancellationAcknowledgement
    }

    func stop() async throws {
        stopped = true
    }
}

private actor FakeDeveloperAgentLauncher: JarvisMacDeveloperAgentLaunching {
    let session: FakeDeveloperAgentSession
    private(set) var configurations: [JarvisMacDeveloperEventRelayConfiguration] = []

    init(session: FakeDeveloperAgentSession) {
        self.session = session
    }

    func launch(
        configuration: JarvisMacDeveloperEventRelayConfiguration
    ) async throws -> any JarvisMacDeveloperAgentSession {
        configurations.append(configuration)
        return session
    }
}

private actor FakeBridgeEventRelay: JarvisMacBridgeEventRelaying {
    let error: JarvisMacDeveloperEventRelayError?
    private(set) var epochs: [UInt64] = []
    private(set) var stopped = false

    init(error: JarvisMacDeveloperEventRelayError? = nil) {
        self.error = error
    }

    func relayEvents(
        using session: any JarvisMacBridgeSession
    ) async throws -> JarvisMacDeveloperEventRelayProgress {
        epochs.append(session.connectionEpoch)
        if let error { throw error }
        return JarvisMacDeveloperEventRelayProgress(
            cursor: JarvisMacDeveloperEventCursor(
                streamID: UUID(uuidString: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")!,
                sequence: 1
            ),
            acceptedEventCount: 1,
            hasMore: false
        )
    }

    func stop() async throws {
        stopped = true
    }
}

private struct FakeBridgeExecutableValidator: JarvisDeveloperBridgeExecutableValidating {
    let error: JarvisDeveloperBridgeProcessError?

    init(error: JarvisDeveloperBridgeProcessError? = nil) {
        self.error = error
    }

    func validate(
        executableURL: URL,
        expectedTeamIdentifier: String
    ) throws -> JarvisDeveloperBridgeValidatedExecutable {
        if let error { throw error }
        return JarvisDeveloperBridgeValidatedExecutable(
            executableURL: executableURL,
            teamIdentifier: expectedTeamIdentifier,
            codeRequirement: "anchor apple generic",
            cdHash: Data(repeating: 0x11, count: 20)
        )
    }
}

private actor FakeBridgeProcessSession: JarvisDeveloperBridgeProcessSession {
    nonisolated let outputLines: AsyncThrowingStream<Data, Error>
    private let continuation: AsyncThrowingStream<Data, Error>.Continuation
    private(set) var stopped = false
    private(set) var stopAttempts = 0
    private var stopFailuresRemaining: Int

    init(lines: [Data], finish: Bool = false, stopFailures: Int = 0) {
        var saved: AsyncThrowingStream<Data, Error>.Continuation!
        outputLines = AsyncThrowingStream { saved = $0 }
        continuation = saved
        stopFailuresRemaining = stopFailures
        for line in lines { saved.yield(line) }
        if finish { saved.finish() }
    }

    func stop() async throws {
        stopAttempts += 1
        if stopFailuresRemaining > 0 {
            stopFailuresRemaining -= 1
            throw JarvisDeveloperBridgeProcessError.teardownFailed
        }
        stopped = true
        continuation.finish()
    }
}

private actor FakeBridgeProcessLauncher: JarvisDeveloperBridgeProcessLaunching {
    let session: FakeBridgeProcessSession
    private(set) var launchCount = 0

    init(session: FakeBridgeProcessSession) {
        self.session = session
    }

    func launch(
        executable _: JarvisDeveloperBridgeValidatedExecutable,
        eventRelayConfiguration _: JarvisMacDeveloperEventRelayConfiguration?
    ) async throws -> any JarvisDeveloperBridgeProcessSession {
        launchCount += 1
        return session
    }
}

@MainActor
private func withStartedBridgeLifecycle<T: Sendable>(
    _ lifecycle: JarvisDeveloperBridgeProcessLifecycle,
    operation: @MainActor () async throws -> T
) async throws -> T {
    lifecycle.start()
    do {
        let result = try await operation()
        await lifecycle.stop()
        return result
    } catch {
        await lifecycle.stop()
        throw error
    }
}

private struct FakeBridgeRunningProcessValidator:
    JarvisDeveloperBridgeRunningProcessValidating
{
    let error: JarvisDeveloperBridgeProcessError?

    init(error: JarvisDeveloperBridgeProcessError? = nil) {
        self.error = error
    }

    func validate(
        processIdentifier _: Int32,
        expected _: JarvisDeveloperBridgeValidatedExecutable
    ) throws {
        if let error { throw error }
    }
}

private final class RecordingBridgeRunningProcessValidator:
    JarvisDeveloperBridgeRunningProcessValidating, @unchecked Sendable
{
    private let lock = NSLock()
    private let error: JarvisDeveloperBridgeProcessError?
    private var recordedProcessIdentifier: Int32?

    init(error: JarvisDeveloperBridgeProcessError? = nil) {
        self.error = error
    }

    func validate(
        processIdentifier: Int32,
        expected _: JarvisDeveloperBridgeValidatedExecutable
    ) throws {
        lock.withLock {
            recordedProcessIdentifier = processIdentifier
        }
        if let error { throw error }
    }

    var processIdentifier: Int32? {
        lock.withLock { recordedProcessIdentifier }
    }
}

private func processExists(_ processIdentifier: Int32) -> Bool {
    errno = 0
    if Darwin.kill(processIdentifier, 0) == 0 {
        return true
    }
    return errno != ESRCH
}

@Suite("Mac enrollment and authenticated bridge", .serialized)
struct DeveloperBridgeTests {
    @Test("Invitation decoding is exact and bounded before Keychain staging")
    func invitationIsExactAndBounded() throws {
        let store = FakeBridgeIdentityStore()
        let enrollment = JarvisMacEnrollmentCoordinator(identityStore: store)

        var unknown = try #require(
            JSONSerialization.jsonObject(with: validInvitationData()) as? [String: Any]
        )
        unknown["grant_secret"] = String(repeating: "0", count: 64)
        let unknownData = try JSONSerialization.data(withJSONObject: unknown)
        #expect(throws: JarvisMacDeveloperBridgeError.invalidDocument) {
            _ = try enrollment.prepare(invitationData: unknownData)
        }
        #expect(store.staged == nil)

        #expect(throws: JarvisMacDeveloperBridgeError.documentTooLarge) {
            _ = try enrollment.prepare(
                invitationData: Data(repeating: 0x20, count: JarvisMacEnrollmentCoordinator.maximumDocumentBytes + 1)
            )
        }
        #expect(store.staged == nil)
    }

    @Test("Preparing enrollment stages only public binding and emits exact CSR reply")
    func prepareEmitsExactCSRReply() throws {
        let store = FakeBridgeIdentityStore()
        let enrollment = JarvisMacEnrollmentCoordinator(identityStore: store)
        let reply = try enrollment.prepare(invitationData: validInvitationData())
        let json = try #require(JSONSerialization.jsonObject(with: reply) as? [String: Any])

        #expect(Set(json.keys) == Set(["schema_version", "status", "grant_id", "device_id", "csr_pem"]))
        #expect(json["status"] as? String == "enrollment_csr_ready")
        #expect(json["grant_id"] as? String == "11111111-1111-4111-8111-111111111111")
        #expect(json["device_id"] as? String == "22222222-2222-4222-8222-222222222222")
        #expect(!String(data: reply, encoding: .utf8)!.contains("grant_secret"))
        #expect(store.staged?.masterEndpoint == "100.64.23.14:7792")
    }

    @Test("Expired and non-concrete master invitations fail before identity creation")
    func unsafeInvitationsFailClosed() throws {
        let unsafeEndpoints = ["0.0.0.0:7792", "224.0.0.1:7792", "[::]:7792", "[ff02::1]:7792", "master.local:7792"]
        for endpoint in unsafeEndpoints {
            let store = FakeBridgeIdentityStore()
            let enrollment = JarvisMacEnrollmentCoordinator(identityStore: store)
            var invitation = try #require(
                JSONSerialization.jsonObject(with: validInvitationData()) as? [String: Any]
            )
            invitation["master_endpoint"] = endpoint
            #expect(throws: JarvisMacDeveloperBridgeError.invalidInvitation) {
                _ = try enrollment.prepare(
                    invitationData: try JSONSerialization.data(withJSONObject: invitation)
                )
            }
            #expect(store.staged == nil)
        }

        let store = FakeBridgeIdentityStore()
        let expired = JarvisMacEnrollmentCoordinator(
            identityStore: store,
            nowMilliseconds: { 4_102_444_800_000 }
        )
        #expect(throws: JarvisMacDeveloperBridgeError.invitationExpired) {
            _ = try expired.prepare(invitationData: validInvitationData())
        }
        #expect(store.staged == nil)
    }

    @Test("Issued receipt mismatch fails before identity installation")
    func issuedMismatchFailsClosed() throws {
        let store = FakeBridgeIdentityStore()
        let enrollment = JarvisMacEnrollmentCoordinator(identityStore: store)
        _ = try enrollment.prepare(invitationData: validInvitationData())
        var receipt = try #require(
            JSONSerialization.jsonObject(with: validIssuedReceiptData()) as? [String: Any]
        )
        receipt["device_id"] = "33333333-3333-4333-8333-333333333333"

        #expect(throws: JarvisMacDeveloperBridgeError.bindingMismatch) {
            _ = try enrollment.install(issuedReceiptData: try JSONSerialization.data(withJSONObject: receipt))
        }
        #expect(store.installed == nil)

        var unknown = try #require(
            JSONSerialization.jsonObject(with: validIssuedReceiptData()) as? [String: Any]
        )
        unknown["private_key"] = "must-never-be-accepted"
        #expect(throws: JarvisMacDeveloperBridgeError.invalidDocument) {
            _ = try enrollment.install(
                issuedReceiptData: try JSONSerialization.data(withJSONObject: unknown)
            )
        }
        #expect(store.installed == nil)
    }

    @Test("Windows CRLF certificate receipts install without relaxing PEM framing")
    func windowsCRLFIssuedReceiptInstalls() throws {
        let store = FakeBridgeIdentityStore()
        let enrollment = JarvisMacEnrollmentCoordinator(identityStore: store)
        _ = try enrollment.prepare(invitationData: validInvitationData())
        let receipt = try #require(String(data: validIssuedReceiptData(), encoding: .utf8))
            .replacingOccurrences(of: "\\n", with: "\\r\\n")

        let profile = try enrollment.install(issuedReceiptData: Data(receipt.utf8))

        #expect(profile.deviceID == "22222222-2222-4222-8222-222222222222")
        #expect(store.installed?.certificatePEM.contains("\r\n") == true)

        let mixed = receipt.replacingOccurrences(
            of: "-----END CERTIFICATE-----\\r\\n",
            with: "-----END CERTIFICATE-----\\n",
            options: [],
            range: receipt.range(of: "-----END CERTIFICATE-----\\r\\n")
        )
        #expect(throws: JarvisMacDeveloperBridgeError.invalidDocument) {
            _ = try enrollment.install(issuedReceiptData: Data(mixed.utf8))
        }
    }

    @Test("Security validity numbers use the Apple reference date")
    func securityValidityNumberUsesReferenceDate() throws {
        let date = try #require(jarvisCertificatePropertyDate(NSNumber(value: 809_033_786)))
        #expect(UInt64(date.timeIntervalSince1970 * 1_000) == 1_787_340_986_000)
        #expect(jarvisCertificatePropertyDate(NSNumber(value: Double.nan)) == nil)
    }

    @Test("Handshake is exporter-bound and accepts only the exact registered profile")
    func handshakeIsExporterBound() async throws {
        let profile = sampleProfile()
        let responseData = Data(
            #"{"protocol_version":1,"status":"accepted","connection_epoch":7,"accepted_registry_revision":3,"reason_code":null}"#.utf8
        )
        let channel = FakeBridgeChannel(
            exporter: Data(repeating: 0x42, count: 32),
            response: JarvisMacBridgeHTTPResponse(status: 200, body: responseData)
        )
        let transport = JarvisMacMTLSBridgeTransport(factory: FakeBridgeChannelFactory(channel: channel))
        let session = try await transport.connect(profile: profile)

        #expect(session.connectionEpoch == 7)
        let request = try #require(await channel.requests.first)
        #expect(request.method == "POST")
        #expect(request.path == "/v1/distributed/connections/accept")
        let object = try #require(JSONSerialization.jsonObject(with: request.body) as? [String: Any])
        let digest = try #require(object["tls_exporter_sha256"] as? [Int])
        #expect(digest.count == 32)
        #expect(digest.contains { $0 != 0 })
        let handshake = try #require(object["handshake"] as? [String: Any])
        #expect(handshake["device_id"] as? String == profile.deviceID)
        #expect(handshake["registry_revision"] as? Int == 3)
    }

    @Test("Missing exporter and mismatched acceptance cancel the TLS channel")
    func invalidHandshakeCancelsChannel() async {
        let unavailable = FakeBridgeChannel(
            exporter: nil,
            response: JarvisMacBridgeHTTPResponse(status: 500, body: Data())
        )
        let unavailableTransport = JarvisMacMTLSBridgeTransport(
            factory: FakeBridgeChannelFactory(channel: unavailable)
        )
        await #expect(throws: JarvisMacDeveloperBridgeError.channelBindingUnavailable) {
            _ = try await unavailableTransport.connect(profile: sampleProfile())
        }
        #expect(await unavailable.cancelled)

        let mismatched = FakeBridgeChannel(
            exporter: Data(repeating: 1, count: 32),
            response: JarvisMacBridgeHTTPResponse(
                status: 200,
                body: Data(#"{"protocol_version":1,"status":"accepted","connection_epoch":7,"accepted_registry_revision":4,"reason_code":null}"#.utf8)
            )
        )
        let mismatchedTransport = JarvisMacMTLSBridgeTransport(
            factory: FakeBridgeChannelFactory(channel: mismatched)
        )
        await #expect(throws: JarvisMacDeveloperBridgeError.bindingMismatch) {
            _ = try await mismatchedTransport.connect(profile: sampleProfile())
        }
        #expect(await mismatched.cancelled)
    }

    @Test("Bridge request gate rejects overlap and reopens after completion")
    func requestGateRejectsOverlap() async throws {
        let gate = JarvisMacBridgeRequestGate()
        try await gate.begin()
        await #expect(throws: JarvisMacDeveloperBridgeError.requestInFlight) {
            try await gate.begin()
        }
        await gate.finish()
        try await gate.begin()
        await gate.finish()
    }

    @Test("Supervisor keeps one authenticated session for exact health samples")
    func supervisorKeepsAuthenticatedSession() async throws {
        let response = JarvisMacBridgeHTTPResponse(status: 200, body: validRemoteHealthData())
        let session = FakeSupervisorSession(
            connectionEpoch: 41,
            outcomes: [.response(response), .response(response)]
        )
        let connector = FakeSupervisorConnector(sessions: [session])
        let supervisor = JarvisMacBridgeSupervisor(profile: sampleProfile(), connector: connector)

        let first = await supervisor.sample()
        let second = await supervisor.sample()

        #expect(first.phase == .authenticated)
        #expect(first.connectionEpoch == 41)
        #expect(first.masterStatus == "ok")
        #expect(first.errorCode == nil)
        #expect(second.phase == .authenticated)
        #expect(await connector.connectCount == 1)
        #expect(await session.requests.count == 2)
        #expect(await session.cancelled == false)
        await supervisor.stop()
        #expect(await session.cancelled)
    }

    @Test("Supervisor accepts the exact authoritative paused health projection")
    func supervisorAcceptsPausedHealth() async {
        let session = FakeSupervisorSession(
            connectionEpoch: 42,
            outcomes: [.response(.init(status: 200, body: pausedRemoteHealthData()))]
        )
        let supervisor = JarvisMacBridgeSupervisor(
            profile: sampleProfile(),
            connector: FakeSupervisorConnector(sessions: [session])
        )

        let snapshot = await supervisor.sample()

        #expect(snapshot.phase == .authenticated)
        #expect(snapshot.masterStatus == "paused")
        #expect(snapshot.maintenanceActive == false)
        #expect(snapshot.emergencyPaused == true)
        #expect(!(await session.cancelled))
        await supervisor.stop()
    }

    @Test("Supervisor rejects malformed health, cancels, and reconnects")
    func supervisorReconnectsAfterInvalidHealth() async throws {
        let invalid = FakeSupervisorSession(
            connectionEpoch: 7,
            outcomes: [.response(JarvisMacBridgeHTTPResponse(
                status: 200,
                body: Data(#"{"status":"ok","mode":"developer_remote_master"}"#.utf8)
            ))]
        )
        let recovered = FakeSupervisorSession(
            connectionEpoch: 8,
            outcomes: [.response(JarvisMacBridgeHTTPResponse(status: 200, body: validRemoteHealthData()))]
        )
        let connector = FakeSupervisorConnector(sessions: [invalid, recovered])
        let supervisor = JarvisMacBridgeSupervisor(profile: sampleProfile(), connector: connector)

        let failed = await supervisor.sample()
        let healthy = await supervisor.sample()

        #expect(failed.phase == .backingOff)
        #expect(failed.connectionEpoch == nil)
        #expect(failed.consecutiveFailures == 1)
        #expect(failed.nextDelayMilliseconds == 1_000)
        #expect(failed.errorCode == "invalid_health")
        #expect(await invalid.cancelled)
        #expect(healthy.phase == .authenticated)
        #expect(healthy.connectionEpoch == 8)
        #expect(healthy.consecutiveFailures == 0)
        #expect(await connector.connectCount == 2)
        await supervisor.stop()
    }

    @Test("Event relay resumes from the agent cursor and forwards only an exact bounded batch")
    func eventRelayUsesDurableAgentCursor() async throws {
        let streamID = UUID(uuidString: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")!
        let agent = FakeDeveloperAgentSession()
        let launcher = FakeDeveloperAgentLauncher(session: agent)
        let configuration = JarvisMacDeveloperEventRelayConfiguration(
            agentExecutableURL: URL(fileURLWithPath: "/tmp/jarvis-agent"),
            agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/jarvis-agent-data")
        )
        let relay = JarvisMacDeveloperEventRelay(
            configuration: configuration,
            launcher: launcher
        )
        let batch = Data(
            """
            {"after_sequence":0,"events":[{"connection_epoch":null,"cursor":{"sequence":1,"stream_id":"\(streamID.uuidString.lowercased())"},"device_id":null,"kind":"step_queued","occurred_at_ms":1000,"protocol_version":1,"step_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","task_id":"cccccccc-cccc-4ccc-8ccc-cccccccccccc"}],"has_more":false,"next_sequence":1,"protocol_version":1,"stream_id":"\(streamID.uuidString.lowercased())"}
            """.utf8
        )
        let master = FakeSupervisorSession(
            connectionEpoch: 51,
            outcomes: [.response(JarvisMacBridgeHTTPResponse(status: 200, body: batch))]
        )

        let progress = try await relay.relayEvents(using: master)

        #expect(progress.cursor == JarvisMacDeveloperEventCursor(
            streamID: streamID,
            sequence: 1
        ))
        #expect(progress.acceptedEventCount == 1)
        #expect(progress.hasMore == false)
        #expect(await agent.acceptedBatches == [batch])
        #expect(await launcher.configurations == [configuration])
        let request = try #require(await master.requests.first)
        #expect(request.method == "POST")
        #expect(request.path == JarvisMacDeveloperEventRelay.remoteEventsPath)
        let requestObject = try #require(
            JSONSerialization.jsonObject(with: request.body) as? [String: Any]
        )
        #expect((requestObject["connection_epoch"] as? NSNumber)?.uint64Value == 51)
        #expect(requestObject["after"] is NSNull)
        #expect((requestObject["limit"] as? NSNumber)?.intValue == 64)
        try await relay.stop()
        #expect(await agent.stopped)
    }

    @Test("Malformed master event batch fails before the local agent cursor changes")
    func eventRelayRejectsMalformedMasterBatch() async throws {
        let agent = FakeDeveloperAgentSession()
        let relay = JarvisMacDeveloperEventRelay(
            configuration: JarvisMacDeveloperEventRelayConfiguration(
                agentExecutableURL: URL(fileURLWithPath: "/tmp/jarvis-agent"),
                agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/jarvis-agent-data")
            ),
            launcher: FakeDeveloperAgentLauncher(session: agent)
        )
        let malformed = Data(
            #"{"after_sequence":0,"events":[],"has_more":false,"next_sequence":2,"protocol_version":1,"stream_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"}"#.utf8
        )
        let master = FakeSupervisorSession(
            connectionEpoch: 52,
            outcomes: [.response(JarvisMacBridgeHTTPResponse(status: 200, body: malformed))]
        )

        await #expect(throws: JarvisMacDeveloperEventRelayError.invalidMasterResponse) {
            _ = try await relay.relayEvents(using: master)
        }
        #expect(await agent.acceptedBatches.isEmpty)
        try await relay.stop()
    }

    @Test("Explicit fixture mode relays one exact Public synthetic job and result")
    func fixtureJobRelayCompletesExactSyntheticJob() async throws {
        let fixture = try fixtureJobDocuments(connectionEpoch: 61, delayMilliseconds: 0)
        let agent = FakeDeveloperAgentSession(fixtureResult: fixture.result)
        let relay = JarvisMacDeveloperEventRelay(
            configuration: JarvisMacDeveloperEventRelayConfiguration(
                agentExecutableURL: URL(fileURLWithPath: "/tmp/jarvis-agent"),
                agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/jarvis-agent-data"),
                fixtureJobsEnabled: true
            ),
            deviceID: UUID(uuidString: "22222222-2222-4222-8222-222222222222"),
            launcher: FakeDeveloperAgentLauncher(session: agent)
        )
        let master = FakeFixtureBridgeSession(
            connectionEpoch: 61,
            eventBatch: emptyEventBatch(),
            job: fixture.job,
            acceptedResult: fixture.acceptedResult,
            cancellationPollDelayMilliseconds: 50
        )

        _ = try await relay.relayEvents(using: master)

        #expect(await agent.executedJobs == [fixture.job])
        #expect(await agent.cancellations.isEmpty)
        let paths = await master.requests.map(\.path)
        #expect(paths.contains(JarvisMacDeveloperEventRelay.remoteLeasePath))
        #expect(paths.contains(JarvisMacDeveloperEventRelay.remoteResultPath))
        #expect(!paths.contains(
            JarvisMacDeveloperEventRelay.remoteCancellationAcknowledgementPath
        ))
        #expect(!(await master.cancelled))
        try await relay.stop()
    }

    @Test("Authoritative cancellation wins and suppresses the fixture result")
    func fixtureJobRelayAcknowledgesCancellationWithoutResult() async throws {
        let fixture = try fixtureJobDocuments(connectionEpoch: 62, delayMilliseconds: 5_000)
        let agent = FakeDeveloperAgentSession(
            fixtureResult: fixture.result,
            cancellationAcknowledgement: fixture.cancellationAcknowledgement,
            fixtureDelayMilliseconds: 5_000
        )
        let relay = JarvisMacDeveloperEventRelay(
            configuration: JarvisMacDeveloperEventRelayConfiguration(
                agentExecutableURL: URL(fileURLWithPath: "/tmp/jarvis-agent"),
                agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/jarvis-agent-data"),
                fixtureJobsEnabled: true
            ),
            deviceID: UUID(uuidString: "22222222-2222-4222-8222-222222222222"),
            launcher: FakeDeveloperAgentLauncher(session: agent)
        )
        let master = FakeFixtureBridgeSession(
            connectionEpoch: 62,
            eventBatch: emptyEventBatch(),
            job: fixture.job,
            cancellation: fixture.cancellation,
            acceptedResult: fixture.acceptedResult
        )

        _ = try await relay.relayEvents(using: master)

        #expect(await agent.executedJobs == [fixture.job])
        #expect(await agent.cancellations == [fixture.cancellation])
        let paths = await master.requests.map(\.path)
        #expect(paths.contains(
            JarvisMacDeveloperEventRelay.remoteCancellationAcknowledgementPath
        ))
        #expect(!paths.contains(JarvisMacDeveloperEventRelay.remoteResultPath))
        try await relay.stop()
    }

    @Test("Supervisor cancels the authenticated session when its local relay fails")
    func supervisorFailsClosedOnEventRelayFailure() async {
        let session = FakeSupervisorSession(
            connectionEpoch: 53,
            outcomes: [.response(JarvisMacBridgeHTTPResponse(
                status: 200,
                body: validRemoteHealthData()
            ))]
        )
        let relay = FakeBridgeEventRelay(error: .eventCursorRejected)
        let supervisor = JarvisMacBridgeSupervisor(
            profile: sampleProfile(),
            connector: FakeSupervisorConnector(sessions: [session]),
            eventRelay: relay
        )

        let failed = await supervisor.sample()

        #expect(failed.phase == .backingOff)
        #expect(failed.errorCode == "event_relay_failed")
        #expect(await relay.epochs == [53])
        #expect(await session.cancelled)
        await supervisor.stop()
    }

    @Test("App relay opt-in requires both absolute agent paths and keeps startup secret-free")
    func appRelayConfigurationIsExactAndSecretFree() throws {
        let complete = JarvisDeveloperBridgeProcessConfiguration(environment: [
            JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/jarvis-mac-bridge",
            JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ",
            JarvisDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey:
                "/tmp/jarvis-agent",
            JarvisDeveloperBridgeProcessConfiguration.agentDataDirectoryEnvironmentKey:
                "/tmp/jarvis-agent-data"
        ])
        let relay = try #require(complete.eventRelayConfiguration)
        let document = try relay.encodeStartupDocument()
        let decoded = try JarvisMacDeveloperEventRelayConfiguration
            .decodeStartupDocument(document)
        #expect(decoded == relay)
        #expect(!String(decoding: document, as: UTF8.self).contains("bearer"))
        #expect(!relay.fixtureJobsEnabled)

        let partial = JarvisDeveloperBridgeProcessConfiguration(environment: [
            JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/jarvis-mac-bridge",
            JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ",
            JarvisDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey:
                "/tmp/jarvis-agent"
        ])
        #expect(partial.executableURL == nil)
        #expect(partial.eventRelayConfiguration == nil)

        let fixture = JarvisDeveloperBridgeProcessConfiguration(environment: [
            JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/jarvis-mac-bridge",
            JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ",
            JarvisDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey:
                "/tmp/jarvis-agent",
            JarvisDeveloperBridgeProcessConfiguration.agentDataDirectoryEnvironmentKey:
                "/tmp/jarvis-agent-data",
            JarvisDeveloperBridgeProcessConfiguration.fixtureJobsEnabledEnvironmentKey:
                "true"
        ])
        #expect(fixture.eventRelayConfiguration?.fixtureJobsEnabled == true)

        let unsafeFixture = JarvisDeveloperBridgeProcessConfiguration(environment: [
            JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/jarvis-mac-bridge",
            JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ",
            JarvisDeveloperBridgeProcessConfiguration.fixtureJobsEnabledEnvironmentKey:
                "true"
        ])
        #expect(unsafeFixture.executableURL == nil)

        let extra = try #require(
            JSONSerialization.jsonObject(with: document) as? [String: Any]
        )
        var modified = extra
        modified["bearer_token"] = "must-not-be-accepted"
        #expect(throws: JarvisMacDeveloperEventRelayError.invalidStartupDocument) {
            _ = try JarvisMacDeveloperEventRelayConfiguration.decodeStartupDocument(
                try JSONSerialization.data(withJSONObject: modified)
            )
        }
    }

    @Test("Supervisor backoff is bounded")
    func supervisorBackoffIsBounded() {
        #expect(JarvisMacBridgeSupervisor.backoffMilliseconds(for: 1) == 1_000)
        #expect(JarvisMacBridgeSupervisor.backoffMilliseconds(for: 2) == 2_000)
        #expect(JarvisMacBridgeSupervisor.backoffMilliseconds(for: 5) == 16_000)
        #expect(JarvisMacBridgeSupervisor.backoffMilliseconds(for: 6) == 30_000)
        #expect(JarvisMacBridgeSupervisor.backoffMilliseconds(for: .max) == 30_000)
    }

    @Test("Explicit reconnect cancels the old session and advances to a new epoch")
    func supervisorExplicitReconnectAdvancesEpoch() async throws {
        let response = JarvisMacBridgeHTTPResponse(status: 200, body: validRemoteHealthData())
        let firstSession = FakeSupervisorSession(
            connectionEpoch: 20,
            outcomes: [.response(response)]
        )
        let secondSession = FakeSupervisorSession(
            connectionEpoch: 21,
            outcomes: [.response(response)]
        )
        let connector = FakeSupervisorConnector(sessions: [firstSession, secondSession])
        let supervisor = JarvisMacBridgeSupervisor(profile: sampleProfile(), connector: connector)

        let first = await supervisor.sample()
        await supervisor.reconnectBeforeNextSample()
        let second = await supervisor.sample()

        #expect(first.connectionEpoch == 20)
        #expect(await firstSession.cancelled)
        #expect(second.phase == .authenticated)
        #expect(second.connectionEpoch == 21)
        #expect(await connector.connectCount == 2)
        await supervisor.stop()
    }

    @Test("App helper snapshots decode exact redacted connected and maintenance states")
    func appHelperSnapshotStrictDecoding() throws {
        let connected = Data(
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
        )
        let maintenance = Data(
            #"{"connection_epoch":23,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":true,"master_endpoint":"100.64.23.14:7792","master_status":"maintenance","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
        )
        let paused = Data(
            #"{"connection_epoch":24,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":true,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"paused","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
        )

        let connectedStatus = try JarvisDeveloperBridgeProcessLifecycle.status(from: connected)
        let maintenanceStatus = try JarvisDeveloperBridgeProcessLifecycle.status(from: maintenance)
        let pausedStatus = try JarvisDeveloperBridgeProcessLifecycle.status(from: paused)

        #expect(connectedStatus.phase == .connected)
        #expect(connectedStatus.masterEndpoint == "100.64.23.14:7792")
        #expect(connectedStatus.connectionEpoch == 22)
        #expect(maintenanceStatus.phase == .maintenance)
        #expect(maintenanceStatus.connectionEpoch == 23)
        #expect(pausedStatus.phase == .paused)
        #expect(pausedStatus.connectionEpoch == 24)
    }

    @Test("App helper snapshots reject extra keys, invalid shapes, and oversized lines")
    func appHelperSnapshotRejectsUntrustedOutput() {
        let extra = Data(
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2,"service_identity":"forbidden"}"#.utf8
        )
        let contradictory = Data(
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":true,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
        )
        let duplicate = Data(
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","\u0070hase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
        )
        let oversized = Data(repeating: 0x61, count: JarvisDeveloperBridgeProcessLifecycle.maximumLineBytes + 1)

        #expect(throws: JarvisDeveloperBridgeProcessError.invalidSnapshot) {
            try JarvisDeveloperBridgeProcessLifecycle.status(from: extra)
        }
        #expect(throws: JarvisDeveloperBridgeProcessError.invalidSnapshot) {
            try JarvisDeveloperBridgeProcessLifecycle.status(from: contradictory)
        }
        #expect(throws: JarvisDeveloperBridgeProcessError.invalidSnapshot) {
            try JarvisDeveloperBridgeProcessLifecycle.status(from: duplicate)
        }
        #expect(throws: JarvisDeveloperBridgeProcessError.invalidSnapshot) {
            try JarvisDeveloperBridgeProcessLifecycle.status(from: oversized)
        }
    }

    @MainActor
    @Test("App helper lifecycle is default inert and owns at most one helper")
    func appHelperLifecycleIsDefaultInertAndSingleOwner() async {
        let session = FakeBridgeProcessSession(lines: [])
        let launcher = FakeBridgeProcessLauncher(session: session)
        let disabled = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [:]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher
        )
        let missingTeamPin = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/unpinned-helper"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher
        )

        disabled.start()
        disabled.start()
        missingTeamPin.start()
        await Task.yield()

        #expect(disabled.status.phase == .disabled)
        #expect(missingTeamPin.status.phase == .disabled)
        #expect(await launcher.launchCount == 0)
        await disabled.stop()
        await missingTeamPin.stop()
    }

    @MainActor
    @Test("App helper lifecycle publishes connected state and stops its child")
    func appHelperLifecyclePublishesAndStops() async {
        let line = Data(
            #"{"connection_epoch":44,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
        )
        let session = FakeBridgeProcessSession(lines: [line])
        let launcher = FakeBridgeProcessLauncher(session: session)
        let lifecycle = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/jarvis-mac-bridge",
                JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher
        )

        lifecycle.start()
        for _ in 0 ..< 50 where lifecycle.status.phase != .connected {
            await Task.yield()
        }

        #expect(lifecycle.status.phase == .connected)
        #expect(lifecycle.status.connectionEpoch == 44)
        lifecycle.start()
        #expect(await launcher.launchCount == 1)
        await lifecycle.stop()
        #expect(await session.stopped)
        #expect(lifecycle.status.phase == .stopped)
    }

    @MainActor
    @Test("App helper lifecycle cleanup stops its child after cancellation")
    func appHelperLifecycleCleanupStopsAfterCancellation() async {
        let line = Data(
            #"{"connection_epoch":44,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
        )
        let session = FakeBridgeProcessSession(lines: [line])
        let lifecycle = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/jarvis-mac-bridge",
                JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: FakeBridgeProcessLauncher(session: session)
        )

        do {
            try await withStartedBridgeLifecycle(lifecycle) {
                for _ in 0 ..< 50 where lifecycle.status.phase != .connected {
                    await Task.yield()
                }
                #expect(lifecycle.status.phase == .connected)
                throw CancellationError()
            }
        } catch is CancellationError {
            // Expected: the shared cleanup path must stop the launched helper.
        } catch {
            Issue.record("unexpected lifecycle cleanup error: \(error)")
        }

        #expect(await session.stopped)
        #expect(lifecycle.status.phase == .stopped)
    }

    @MainActor
    @Test("Failed helper teardown retains ownership and blocks relaunch until retry")
    func appHelperTeardownFailureBlocksRelaunchUntilRetry() async {
        let line = Data(
            #"{"connection_epoch":44,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
        )
        let session = FakeBridgeProcessSession(lines: [line], stopFailures: 2)
        let launcher = FakeBridgeProcessLauncher(session: session)
        let lifecycle = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/jarvis-mac-bridge",
                JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher
        )

        lifecycle.start()
        for _ in 0 ..< 50 where lifecycle.status.phase != .connected {
            await Task.yield()
        }
        await lifecycle.stop()
        #expect(lifecycle.status.phase == .masterOffline)
        #expect(lifecycle.status.errorCode == "helper_teardown_failed")
        #expect(await session.stopAttempts == 2)

        lifecycle.start()
        await Task.yield()
        #expect(await launcher.launchCount == 1)

        await lifecycle.stop()
        #expect(await session.stopAttempts == 3)
        #expect(await session.stopped)
        #expect(lifecycle.status.phase == .stopped)

        lifecycle.start()
        for _ in 0 ..< 50 where await launcher.launchCount != 2 {
            await Task.yield()
        }
        #expect(await launcher.launchCount == 2)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Foundation helper session streams one bounded snapshot and is reaped on stop")
    func foundationHelperSessionStreamsAndStops() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("jarvis-bridge-process-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let executable = directory.appendingPathComponent("bridge-fixture")
        let snapshot = #"{"connection_epoch":45,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#
        let script = "#!/bin/sh\nprintf '%s\\n' '\(snapshot)'\nexec /bin/sleep 30\n"
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: executable.path
        )
        let lifecycle = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: executable.path,
                JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationJarvisDeveloperBridgeProcessLauncher(
                runningProcessValidator: FakeBridgeRunningProcessValidator()
            )
        )

        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.phase != .connected {
            try? await Task.sleep(for: .milliseconds(10))
        }

        #expect(lifecycle.status.phase == .connected)
        #expect(lifecycle.status.connectionEpoch == 45)
        await lifecycle.stop()
        #expect(lifecycle.status.phase == .stopped)
    }

    @MainActor
    @Test("Foundation helper output queue is bounded and fails closed on overflow")
    func foundationHelperOutputQueueIsBounded() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("jarvis-bridge-overflow-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let executable = directory.appendingPathComponent("bridge-fixture")
        let snapshot = #"{"connection_epoch":46,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#
        let script = "#!/bin/sh\ni=0\nwhile [ $i -lt 1000 ]; do\n  printf '%s\\n' '\(snapshot)'\n  i=$((i + 1))\ndone\nexec /bin/sleep 30\n"
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: executable.path
        )
        let lifecycle = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: executable.path,
                JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationJarvisDeveloperBridgeProcessLauncher(
                runningProcessValidator: FakeBridgeRunningProcessValidator()
            )
        )

        lifecycle.start()
        for _ in 0 ..< 200 where lifecycle.status.phase != .masterOffline {
            try? await Task.sleep(for: .milliseconds(10))
        }

        #expect(lifecycle.status.phase == .masterOffline)
        #expect(lifecycle.status.errorCode == "helper_output_too_large")
        await lifecycle.stop()
    }

    @MainActor
    @Test("Running-child validation and bounded TERM-to-KILL teardown fail closed")
    func foundationHelperRunningValidationAndKillEscalation() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("jarvis-bridge-kill-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let executable = directory.appendingPathComponent("bridge-fixture")
        let snapshot = #"{"connection_epoch":47,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#
        let script = "#!/bin/sh\ntrap '' TERM\nprintf '%s\\n' '\(snapshot)'\nexec /bin/sleep 30\n"
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: executable.path
        )

        let rejectedRunningValidator = RecordingBridgeRunningProcessValidator(
            error: .invalidExecutableSignature
        )
        let rejected = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: executable.path,
                JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationJarvisDeveloperBridgeProcessLauncher(
                runningProcessValidator: rejectedRunningValidator
            )
        )
        rejected.start()
        for _ in 0 ..< 100 where rejected.status.phase != .masterOffline {
            try? await Task.sleep(for: .milliseconds(10))
        }
        #expect(rejected.status.errorCode == "invalid_helper_signature")
        let rejectedProcessIdentifier = try #require(
            rejectedRunningValidator.processIdentifier
        )
        #expect(!processExists(rejectedProcessIdentifier))
        await rejected.stop()

        let stubbornRunningValidator = RecordingBridgeRunningProcessValidator()
        let stubborn = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: executable.path,
                JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationJarvisDeveloperBridgeProcessLauncher(
                runningProcessValidator: stubbornRunningValidator
            )
        )
        stubborn.start()
        for _ in 0 ..< 100 where stubborn.status.phase != .connected {
            try? await Task.sleep(for: .milliseconds(10))
        }
        #expect(stubborn.status.phase == .connected)
        let start = ContinuousClock.now
        await stubborn.stop()
        #expect(start.duration(to: .now) < .seconds(2))
        #expect(stubborn.status.phase == .stopped)
        let stubbornProcessIdentifier = try #require(
            stubbornRunningValidator.processIdentifier
        )
        #expect(!processExists(stubbornProcessIdentifier))
    }

    @MainActor
    @Test("App helper EOF and signature failure fail closed to Master Offline")
    func appHelperLifecycleFailsClosed() async {
        let finishedSession = FakeBridgeProcessSession(lines: [], finish: true)
        let finishedLauncher = FakeBridgeProcessLauncher(session: finishedSession)
        let finished = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/jarvis-mac-bridge",
                JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: finishedLauncher
        )
        finished.start()
        for _ in 0 ..< 50 where finished.status.phase != .masterOffline {
            await Task.yield()
        }
        #expect(finished.status.phase == .masterOffline)
        #expect(finished.status.errorCode == "helper_exited")

        let rejectedSession = FakeBridgeProcessSession(lines: [])
        let rejected = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/replaced-helper",
                JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(error: .invalidExecutableSignature),
            launcher: FakeBridgeProcessLauncher(session: rejectedSession)
        )
        rejected.start()
        for _ in 0 ..< 50 where rejected.status.phase != .masterOffline {
            await Task.yield()
        }
        #expect(rejected.status.phase == .masterOffline)
        #expect(rejected.status.errorCode == "invalid_helper_signature")
        await finished.stop()
        await rejected.stop()
    }

    @MainActor
    @Test("Live signed helper reaches the Windows master through the production app lifecycle")
    func liveSignedHelperAppLifecycleReachesWindowsMaster() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard environment["JARVIS_MAC_DEVELOPER_BRIDGE_LIVE_E2E"] == "true" else {
            return
        }
        let configuration = JarvisDeveloperBridgeProcessConfiguration(environment: environment)
        try #require(configuration.executableURL != nil)
        try #require(configuration.expectedTeamIdentifier != nil)
        let lifecycle = JarvisDeveloperBridgeProcessLifecycle(configuration: configuration)

        lifecycle.start()
        for _ in 0 ..< 400
            where lifecycle.status.phase != .connected
                && lifecycle.status.phase != .maintenance
        {
            try await Task.sleep(for: .milliseconds(50))
        }
        let liveStatus = lifecycle.status
        await lifecycle.stop()

        #expect(liveStatus.phase == .connected || liveStatus.phase == .maintenance)
        #expect(liveStatus.masterEndpoint?.isEmpty == false)
        #expect(liveStatus.connectionEpoch.map { $0 > 0 } == true)
        #expect(lifecycle.status.phase == .stopped)
        print(
            "jarvis_mac_app_bridge_live_e2e_ok "
                + "endpoint=\(liveStatus.masterEndpoint ?? "missing") "
                + "connection_epoch=\(liveStatus.connectionEpoch ?? 0)"
        )
    }

    @MainActor
    @Test("Live production app lifecycle fails closed and recovers across a Windows outage")
    func liveSignedHelperAppLifecycleRecoversFromWindowsOutage() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard environment["JARVIS_MAC_DEVELOPER_BRIDGE_OUTAGE_LIVE_E2E"] == "true" else {
            return
        }
        let configuration = JarvisDeveloperBridgeProcessConfiguration(environment: environment)
        try #require(configuration.executableURL != nil)
        try #require(configuration.expectedTeamIdentifier != nil)
        let coordinationDirectory = try #require(
            environment["JARVIS_MAC_DEVELOPER_BRIDGE_OUTAGE_COORDINATION_DIR"]
        )
        try #require(coordinationDirectory.hasPrefix("/"))
        var directoryMetadata = stat()
        try #require(
            lstat(coordinationDirectory, &directoryMetadata) == 0
                && directoryMetadata.st_mode & S_IFMT == S_IFDIR
                && directoryMetadata.st_uid == geteuid()
                && directoryMetadata.st_mode & 0o777 == 0o700
        )
        let coordinationURL = URL(
            fileURLWithPath: coordinationDirectory,
            isDirectory: true
        ).standardizedFileURL
        let lifecycle = JarvisDeveloperBridgeProcessLifecycle(configuration: configuration)

        let evidence = try await withStartedBridgeLifecycle(lifecycle) {
            func checkForHarnessCancellation() throws {
                try Task.checkCancellation()
                if FileManager.default.fileExists(
                    atPath: coordinationURL.appendingPathComponent("cancel").path
                ) {
                    throw CancellationError()
                }
            }

            for _ in 0 ..< 1_200 where lifecycle.status.phase != .connected {
                try checkForHarnessCancellation()
                try await Task.sleep(for: .milliseconds(50))
            }
            let connectedBefore = lifecycle.status
            try #require(connectedBefore.phase == .connected)
            let epochBefore = try #require(connectedBefore.connectionEpoch)
            try #require(epochBefore > 0)
            try Data("\(epochBefore)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("connected-before"),
                options: .atomic
            )

            for _ in 0 ..< 2_400 where lifecycle.status.phase != .masterOffline {
                try checkForHarnessCancellation()
                try await Task.sleep(for: .milliseconds(50))
            }
            let offline = lifecycle.status
            try #require(offline.phase == .masterOffline)
            try #require(offline.connectionEpoch == nil)
            let offlineError = try #require(offline.errorCode)
            try #require(!offlineError.isEmpty)
            try Data("\(offlineError)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("master-offline"),
                options: .atomic
            )

            for _ in 0 ..< 3_600
                where lifecycle.status.connectionEpoch.map({ $0 > epochBefore }) != true
                    || lifecycle.status.phase != .connected
            {
                try checkForHarnessCancellation()
                try await Task.sleep(for: .milliseconds(50))
            }
            let connectedAfter = lifecycle.status
            try #require(connectedAfter.phase == .connected)
            let epochAfter = try #require(connectedAfter.connectionEpoch)
            try #require(epochAfter > epochBefore)
            try #require(connectedAfter.masterEndpoint == connectedBefore.masterEndpoint)
            try Data("\(epochAfter)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("connected-after"),
                options: .atomic
            )
            return (
                connectedAfter: connectedAfter,
                epochBefore: epochBefore,
                epochAfter: epochAfter,
                offlineError: offlineError
            )
        }

        #expect(lifecycle.status.phase == .stopped)
        print(
            "jarvis_mac_app_bridge_outage_recovery_live_e2e_ok "
                + "endpoint=\(evidence.connectedAfter.masterEndpoint ?? "missing") "
                + "connection_epoch_before=\(evidence.epochBefore) "
                + "connection_epoch_after=\(evidence.epochAfter) "
                + "offline_error=\(evidence.offlineError)"
        )
    }
}

private struct FixtureJobDocuments {
    let job: Data
    let result: Data
    let cancellation: Data
    let cancellationAcknowledgement: Data
    let acceptedResult: Data
}

private func fixtureJobDocuments(
    connectionEpoch: UInt64,
    delayMilliseconds: UInt64
) throws -> FixtureJobDocuments {
    let taskID = "11111111-1111-4111-8111-111111111111"
    let stepID = "22222222-2222-4222-8222-222222222222"
    let attemptID = "33333333-3333-4333-8333-333333333333"
    let leaseID = "44444444-4444-4444-8444-444444444444"
    let cancellationID = "55555555-5555-4555-8555-555555555555"
    let context: [String: Any] = [
        "operation": "synthetic_echo",
        "input": "fixture-public-input",
        "delay_ms": NSNumber(value: delayMilliseconds)
    ]
    let contextData = try JSONSerialization.data(
        withJSONObject: context,
        options: [.sortedKeys]
    )
    let contextDigest = Array(SHA256.hash(data: contextData))
    let jobObject: [String: Any] = [
        "protocol_version": 1,
        "connection_epoch": NSNumber(value: connectionEpoch),
        "sequence": 10,
        "task_id": taskID,
        "step_id": stepID,
        "attempt_id": attemptID,
        "lease_id": leaseID,
        "cancellation_id": cancellationID,
        "capability_id": "fixture.reasoning",
        "selected_model": "jarvis-fixture-v1",
        "sensitivity": "public",
        "context_handling": "ephemeral_no_retention",
        "lease_duration_ms": 10_000,
        "deadline_after_ms": 10_000,
        "context_sha256": contextDigest,
        "context": context
    ]
    let payload: [String: Any] = [
        "operation": "synthetic_echo",
        "output": "fixture-public-input",
        "synthetic": true
    ]
    let payloadData = try JSONSerialization.data(
        withJSONObject: payload,
        options: [.sortedKeys]
    )
    let payloadDigest = Array(SHA256.hash(data: payloadData))
    let resultObject: [String: Any] = [
        "protocol_version": 1,
        "connection_epoch": NSNumber(value: connectionEpoch),
        "sequence": 11,
        "task_id": taskID,
        "step_id": stepID,
        "attempt_id": attemptID,
        "lease_id": leaseID,
        "cancellation_id": cancellationID,
        "status": "completed",
        "context_sha256": contextDigest,
        "payload_sha256": payloadDigest,
        "payload": payload
    ]
    let cancellationObject: [String: Any] = [
        "protocol_version": 1,
        "connection_epoch": NSNumber(value: connectionEpoch),
        "sequence": 11,
        "task_id": taskID,
        "step_id": stepID,
        "attempt_id": attemptID,
        "lease_id": leaseID,
        "cancellation_id": cancellationID,
        "deadline_after_ms": 2_000
    ]
    let acknowledgementObject: [String: Any] = [
        "protocol_version": 1,
        "connection_epoch": NSNumber(value: connectionEpoch),
        "sequence": 12,
        "task_id": taskID,
        "step_id": stepID,
        "attempt_id": attemptID,
        "lease_id": leaseID,
        "cancellation_id": cancellationID,
        "status": "cancelled"
    ]
    let acceptedResultObject: [String: Any] = [
        "task_id": taskID,
        "step_id": stepID,
        "status": "succeeded",
        "payload_sha256": payloadDigest
    ]
    return FixtureJobDocuments(
        job: try JSONSerialization.data(withJSONObject: jobObject, options: [.sortedKeys]),
        result: try JSONSerialization.data(
            withJSONObject: resultObject,
            options: [.sortedKeys]
        ),
        cancellation: try JSONSerialization.data(
            withJSONObject: cancellationObject,
            options: [.sortedKeys]
        ),
        cancellationAcknowledgement: try JSONSerialization.data(
            withJSONObject: acknowledgementObject,
            options: [.sortedKeys]
        ),
        acceptedResult: try JSONSerialization.data(
            withJSONObject: acceptedResultObject,
            options: [.sortedKeys]
        )
    )
}

private func emptyEventBatch() -> Data {
    Data(
        #"{"after_sequence":0,"events":[],"has_more":false,"next_sequence":0,"protocol_version":1,"stream_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"}"#.utf8
    )
}

private func validInvitationData() throws -> Data {
    Data(
        #"{"schema_version":1,"status":"enrollment_invitation_ready","grant_id":"11111111-1111-4111-8111-111111111111","device_id":"22222222-2222-4222-8222-222222222222","device_name":"owner-mac-bridge","role":"mac_bridge","registry_revision":3,"expires_at_ms":4102444800000,"capabilities":[{"id":"mlx.reasoning","kind":"local_inference","provider":"mlx","model":"test-model","max_context_bytes":262144,"max_result_bytes":786432}],"master_endpoint":"100.64.23.14:7792","ca_fingerprint_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#.utf8
    )
}

private func validIssuedReceiptData() throws -> Data {
    Data(
        #"{"status":"device_certificate_issued","operation":"enroll","device_id":"22222222-2222-4222-8222-222222222222","device_name":"owner-mac-bridge","role":"mac_bridge","registry_revision":3,"serial_hex":"01","issued_at_ms":1000,"not_after_ms":4102444800000,"certificate_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","certificate_pem":"-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n","ca_certificate_pem":"-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n"}"#.utf8
    )
}

private func sampleProfile() -> JarvisMacBridgeProfile {
    JarvisMacBridgeProfile(
        deviceID: "22222222-2222-4222-8222-222222222222",
        deviceName: "owner-mac-bridge",
        role: "mac_bridge",
        registryRevision: 3,
        capabilities: [
            JarvisMacBridgeCapability(
                id: "mlx.reasoning",
                kind: "local_inference",
                provider: "mlx",
                model: "test-model",
                maxContextBytes: 262_144,
                maxResultBytes: 786_432
            )
        ],
        masterEndpoint: "100.64.23.14:7792",
        certificateNotAfterMilliseconds: 4_102_444_800_000
    )
}

private func validRemoteHealthData() -> Data {
    Data(
        #"{"status":"ok","mode":"developer_remote_master","host_mode":"windows_service","service_identity":"MIKE-PC\\mike","maintenance_active":false,"maintenance_reason":null,"emergency_paused":false,"protocol_version":1,"schema_version":2,"process_id":43752,"started_at_ms":1784749559000,"startup_reconciliation":{"disconnected_connections":0,"abandoned_attempts":0,"requeued_steps":0},"state":{"registered_devices":1,"active_device_certificates":1,"unconsumed_enrollment_grants":2,"active_connections":1,"queued_steps":0,"leased_steps":0,"terminal_steps":0,"active_attempts":0},"boundary":"TLS 1.3 mutual authentication with enrolled-device certificate and durable revocation checks"}"#.utf8
    )
}

private func pausedRemoteHealthData() -> Data {
    Data(
        #"{"status":"paused","mode":"developer_remote_master","host_mode":"windows_service","service_identity":"MIKE-PC\\mike","maintenance_active":false,"maintenance_reason":null,"emergency_paused":true,"protocol_version":1,"schema_version":2,"process_id":43752,"started_at_ms":1784749559000,"startup_reconciliation":{"disconnected_connections":0,"abandoned_attempts":0,"requeued_steps":0},"state":{"registered_devices":1,"active_device_certificates":1,"unconsumed_enrollment_grants":2,"active_connections":1,"queued_steps":0,"leased_steps":1,"terminal_steps":0,"active_attempts":1},"boundary":"TLS 1.3 mutual authentication with enrolled-device certificate and durable revocation checks"}"#.utf8
    )
}
