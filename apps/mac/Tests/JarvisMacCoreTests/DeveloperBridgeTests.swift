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

    init(lines: [Data], finish: Bool = false) {
        var saved: AsyncThrowingStream<Data, Error>.Continuation!
        outputLines = AsyncThrowingStream { saved = $0 }
        continuation = saved
        for line in lines { saved.yield(line) }
        if finish { saved.finish() }
    }

    func stop() async {
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
        executable _: JarvisDeveloperBridgeValidatedExecutable
    ) async throws -> any JarvisDeveloperBridgeProcessSession {
        launchCount += 1
        return session
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
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
        )
        let maintenance = Data(
            #"{"connection_epoch":23,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","maintenance_active":true,"master_endpoint":"100.64.23.14:7792","master_status":"maintenance","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
        )

        let connectedStatus = try JarvisDeveloperBridgeProcessLifecycle.status(from: connected)
        let maintenanceStatus = try JarvisDeveloperBridgeProcessLifecycle.status(from: maintenance)

        #expect(connectedStatus.phase == .connected)
        #expect(connectedStatus.masterEndpoint == "100.64.23.14:7792")
        #expect(connectedStatus.connectionEpoch == 22)
        #expect(maintenanceStatus.phase == .maintenance)
        #expect(maintenanceStatus.connectionEpoch == 23)
    }

    @Test("App helper snapshots reject extra keys, invalid shapes, and oversized lines")
    func appHelperSnapshotRejectsUntrustedOutput() {
        let extra = Data(
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2,"service_identity":"forbidden"}"#.utf8
        )
        let contradictory = Data(
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","maintenance_active":true,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
        )
        let duplicate = Data(
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","\u0070hase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
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
            #"{"connection_epoch":44,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#.utf8
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
    @Test("Foundation helper session streams one bounded snapshot and is reaped on stop")
    func foundationHelperSessionStreamsAndStops() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("jarvis-bridge-process-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let executable = directory.appendingPathComponent("bridge-fixture")
        let snapshot = #"{"connection_epoch":45,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#
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
        let snapshot = #"{"connection_epoch":46,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#
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
        let snapshot = #"{"connection_epoch":47,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":1,"schema_version":2}"#
        let script = "#!/bin/sh\ntrap '' TERM\nprintf '%s\\n' '\(snapshot)'\nwhile :; do :; done\n"
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: executable.path
        )

        let rejected = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: executable.path,
                JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationJarvisDeveloperBridgeProcessLauncher(
                runningProcessValidator: FakeBridgeRunningProcessValidator(
                    error: .invalidExecutableSignature
                )
            )
        )
        rejected.start()
        for _ in 0 ..< 100 where rejected.status.phase != .masterOffline {
            try? await Task.sleep(for: .milliseconds(10))
        }
        #expect(rejected.status.errorCode == "invalid_helper_signature")
        await rejected.stop()

        let stubborn = JarvisDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                JarvisDeveloperBridgeProcessConfiguration.executableEnvironmentKey: executable.path,
                JarvisDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationJarvisDeveloperBridgeProcessLauncher(
                runningProcessValidator: FakeBridgeRunningProcessValidator()
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
        #"{"status":"ok","mode":"developer_remote_master","host_mode":"windows_service","service_identity":"MIKE-PC\\mike","maintenance_active":false,"maintenance_reason":null,"protocol_version":1,"schema_version":2,"process_id":43752,"started_at_ms":1784749559000,"startup_reconciliation":{"disconnected_connections":0,"abandoned_attempts":0,"requeued_steps":0},"state":{"registered_devices":1,"active_device_certificates":1,"unconsumed_enrollment_grants":2,"active_connections":1,"queued_steps":0,"leased_steps":0,"terminal_steps":0,"active_attempts":0},"boundary":"TLS 1.3 mutual authentication with enrolled-device certificate and durable revocation checks"}"#.utf8
    )
}
