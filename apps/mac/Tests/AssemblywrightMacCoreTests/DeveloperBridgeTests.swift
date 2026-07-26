import Darwin
import CryptoKit
import Foundation
import Security
import Testing
@testable import AssemblywrightMacCore

private final class FakeBridgeIdentityStore: AssemblywrightMacBridgeIdentityStore, @unchecked Sendable {
    var staged: AssemblywrightMacEnrollmentInvitation?
    var installed: AssemblywrightMacIssuedDeviceCertificate?
    var csrPEM = "-----BEGIN CERTIFICATE REQUEST-----\nZmFrZQ==\n-----END CERTIFICATE REQUEST-----\n"
    var publicKeySHA256 = String(repeating: "a", count: 64)
    var installedProfile: AssemblywrightMacBridgeProfile?
    var stagedReplacement: AssemblywrightMacEnrollmentInvitation?
    var stagedReplacementReceipt: AssemblywrightMacPendingCapabilityRebindCertificate?
    var replacementCancelled = false
    var replacementPromoted = false

    func stageIdentity(for invitation: AssemblywrightMacEnrollmentInvitation) throws -> AssemblywrightMacEnrollmentCSR {
        staged = invitation
        return AssemblywrightMacEnrollmentCSR(
            schemaVersion: 1,
            status: "enrollment_csr_ready",
            grantID: invitation.grantID,
            deviceID: invitation.deviceID,
            csrPEM: csrPEM
        )
    }

    func loadStagedInvitation() throws -> AssemblywrightMacEnrollmentInvitation? { staged }

    func install(
        _ receipt: AssemblywrightMacIssuedDeviceCertificate,
        for invitation: AssemblywrightMacEnrollmentInvitation
    ) throws -> AssemblywrightMacBridgeProfile {
        installed = receipt
        let profile = AssemblywrightMacBridgeProfile(
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

    func loadInstalledProfile() throws -> AssemblywrightMacBridgeProfile? { installedProfile }

    func stageReplacementIdentity(
        for invitation: AssemblywrightMacEnrollmentInvitation
    ) throws -> AssemblywrightMacEnrollmentCSR {
        stagedReplacement = invitation
        return AssemblywrightMacEnrollmentCSR(
            schemaVersion: 1,
            status: "enrollment_csr_ready",
            grantID: invitation.grantID,
            deviceID: invitation.deviceID,
            csrPEM: csrPEM
        )
    }

    func loadStagedReplacementInvitation() throws -> AssemblywrightMacEnrollmentInvitation? {
        stagedReplacement
    }

    func stageReplacementCertificate(
        _ receipt: AssemblywrightMacPendingCapabilityRebindCertificate,
        for _: AssemblywrightMacEnrollmentInvitation
    ) throws -> AssemblywrightMacCapabilityRebindAcknowledgement {
        stagedReplacementReceipt = receipt
        return AssemblywrightMacCapabilityRebindAcknowledgement(
            status: "capability_rebind_certificate_staged",
            grantID: receipt.grantID,
            deviceID: receipt.deviceID,
            registryRevision: receipt.registryRevision,
            serialHex: receipt.serialHex,
            certificateSHA256: receipt.certificateSHA256,
            signatureAlgorithm: assemblywrightRebindSignatureAlgorithm,
            signatureBase64: "AQEBAQEBAQE="
        )
    }

    func promoteReplacement(
        _ activation: AssemblywrightMacCapabilityRebindActivation
    ) throws -> AssemblywrightMacBridgeProfile {
        guard let invitation = stagedReplacement, let receipt = stagedReplacementReceipt,
              activation.grantID == receipt.grantID,
              activation.deviceID == receipt.deviceID,
              activation.registryRevision == receipt.registryRevision,
              activation.serialHex == receipt.serialHex,
              activation.certificateSHA256 == receipt.certificateSHA256 else {
            throw AssemblywrightMacDeveloperBridgeError.bindingMismatch
        }
        let profile = AssemblywrightMacBridgeProfile(
            deviceID: receipt.deviceID,
            deviceName: receipt.deviceName,
            role: receipt.role,
            registryRevision: receipt.registryRevision,
            capabilities: invitation.capabilities,
            masterEndpoint: invitation.masterEndpoint,
            certificateNotAfterMilliseconds: receipt.notAfterMilliseconds
        )
        installedProfile = profile
        replacementPromoted = true
        stagedReplacement = nil
        stagedReplacementReceipt = nil
        return profile
    }

    func cancelStagedReplacement() throws {
        if stagedReplacementReceipt != nil, !replacementPromoted {
            throw AssemblywrightMacDeveloperBridgeError.bindingMismatch
        }
        replacementCancelled = true
        stagedReplacement = nil
        stagedReplacementReceipt = nil
    }
}

private actor FakeBridgeChannel: AssemblywrightMacAuthenticatedTLSChannel {
    let exporter: Data?
    let response: AssemblywrightMacBridgeHTTPResponse
    private(set) var requests: [AssemblywrightMacBridgeHTTPRequest] = []
    private(set) var cancelled = false

    init(exporter: Data?, response: AssemblywrightMacBridgeHTTPResponse) {
        self.exporter = exporter
        self.response = response
    }

    func tlsExporter(label: String, length: Int) async throws -> Data {
        guard let exporter else { throw AssemblywrightMacDeveloperBridgeError.channelBindingUnavailable }
        #expect(label == "EXPORTER-Assemblywright-Developer-Mode-v1")
        #expect(length == 32)
        return exporter
    }

    func send(_ request: AssemblywrightMacBridgeHTTPRequest) async throws -> AssemblywrightMacBridgeHTTPResponse {
        requests.append(request)
        return response
    }

    func cancel() async {
        cancelled = true
    }
}

private struct FakeBridgeChannelFactory: AssemblywrightMacAuthenticatedTLSChannelFactory {
    let channel: FakeBridgeChannel

    func connect(profile _: AssemblywrightMacBridgeProfile) async throws -> any AssemblywrightMacAuthenticatedTLSChannel {
        channel
    }
}

private enum FakeSupervisorOutcome: Sendable {
    case response(AssemblywrightMacBridgeHTTPResponse)
    case failure
}

private struct FakeSupervisorError: Error {}

private actor FakeSupervisorSession: AssemblywrightMacBridgeSession {
    nonisolated let connectionEpoch: UInt64
    private var outcomes: [FakeSupervisorOutcome]
    private(set) var requests: [AssemblywrightMacBridgeHTTPRequest] = []
    private(set) var cancelled = false

    init(connectionEpoch: UInt64, outcomes: [FakeSupervisorOutcome]) {
        self.connectionEpoch = connectionEpoch
        self.outcomes = outcomes
    }

    func send(_ request: AssemblywrightMacBridgeHTTPRequest) async throws -> AssemblywrightMacBridgeHTTPResponse {
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

private actor FakeSupervisorConnector: AssemblywrightMacBridgeConnecting {
    private var sessions: [FakeSupervisorSession]
    private(set) var connectCount = 0

    init(sessions: [FakeSupervisorSession]) {
        self.sessions = sessions
    }

    func connect(profile _: AssemblywrightMacBridgeProfile) async throws -> any AssemblywrightMacBridgeSession {
        connectCount += 1
        guard !sessions.isEmpty else { throw FakeSupervisorError() }
        return sessions.removeFirst()
    }
}

private actor FakeFixtureBridgeSession: AssemblywrightMacBridgeSession {
    nonisolated let connectionEpoch: UInt64
    let eventBatch: Data
    let job: Data
    let cancellation: Data?
    let acceptedResult: Data
    let cancellationPollDelayMilliseconds: UInt64
    let leaseResponse: AssemblywrightMacBridgeHTTPResponse
    let noCancellationResponse: Data
    private(set) var requests: [AssemblywrightMacBridgeHTTPRequest] = []
    private(set) var cancelled = false
    private var cancellationDelivered = false

    init(
        connectionEpoch: UInt64,
        eventBatch: Data,
        job: Data,
        cancellation: Data? = nil,
        acceptedResult: Data,
        cancellationPollDelayMilliseconds: UInt64 = 0,
        leaseResponse: AssemblywrightMacBridgeHTTPResponse? = nil,
        noCancellationResponse: Data = Data(#"{"status":"no_cancellation"}"#.utf8)
    ) {
        self.connectionEpoch = connectionEpoch
        self.eventBatch = eventBatch
        self.job = job
        self.cancellation = cancellation
        self.acceptedResult = acceptedResult
        self.cancellationPollDelayMilliseconds = cancellationPollDelayMilliseconds
        self.leaseResponse = leaseResponse ?? .init(status: 200, body: job)
        self.noCancellationResponse = noCancellationResponse
    }

    func send(_ request: AssemblywrightMacBridgeHTTPRequest) async throws
        -> AssemblywrightMacBridgeHTTPResponse
    {
        requests.append(request)
        switch request.path {
        case AssemblywrightMacDeveloperEventRelay.remoteEventsPath:
            return .init(status: 200, body: eventBatch)
        case AssemblywrightMacDeveloperEventRelay.remoteLeasePath:
            return leaseResponse
        case AssemblywrightMacDeveloperEventRelay.remoteCancellationPath:
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
            return .init(
                status: 200,
                body: noCancellationResponse
            )
        case AssemblywrightMacDeveloperEventRelay.remoteResultPath:
            return .init(status: 200, body: acceptedResult)
        case AssemblywrightMacDeveloperEventRelay.remoteCancellationAcknowledgementPath:
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

private actor FakeDeveloperAgentSession: AssemblywrightMacDeveloperAgentSession {
    private var cursor: AssemblywrightMacDeveloperEventCursor?
    private(set) var acceptedBatches: [Data] = []
    private(set) var executedJobs: [Data] = []
    private(set) var cancellations: [Data] = []
    private(set) var executedMLXJobs: [Data] = []
    private(set) var mlxCancellations: [Data] = []
    var fixtureResult: Data?
    var cancellationAcknowledgement: Data?
    let fixtureDelayMilliseconds: UInt64
    private(set) var stopped = false

    init(
        cursor: AssemblywrightMacDeveloperEventCursor? = nil,
        fixtureResult: Data? = nil,
        cancellationAcknowledgement: Data? = nil,
        fixtureDelayMilliseconds: UInt64 = 0
    ) {
        self.cursor = cursor
        self.fixtureResult = fixtureResult
        self.cancellationAcknowledgement = cancellationAcknowledgement
        self.fixtureDelayMilliseconds = fixtureDelayMilliseconds
    }

    func health() async throws -> AssemblywrightMacDeveloperAgentCursorSnapshot {
        AssemblywrightMacDeveloperAgentCursorSnapshot(
            cursor: cursor,
            updatedAtMilliseconds: cursor == nil ? nil : 1_000
        )
    }

    func accept(batch: Data) async throws -> AssemblywrightMacDeveloperAgentCursorSnapshot {
        let object = try #require(
            JSONSerialization.jsonObject(with: batch) as? [String: Any]
        )
        let stream = try #require(object["stream_id"] as? String)
        let sequence = try #require(
            (object["next_sequence"] as? NSNumber)?.uint64Value
        )
        cursor = AssemblywrightMacDeveloperEventCursor(
            streamID: try #require(UUID(uuidString: stream)),
            sequence: sequence
        )
        acceptedBatches.append(batch)
        return AssemblywrightMacDeveloperAgentCursorSnapshot(
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
            throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
        }
        return fixtureResult
    }

    func cancelFixtureJob(_ instruction: Data) async throws -> Data {
        cancellations.append(instruction)
        guard let cancellationAcknowledgement else {
            throw AssemblywrightMacDeveloperEventRelayError.fixtureJobRejected
        }
        return cancellationAcknowledgement
    }

    func executeMLXJob(_ job: Data) async throws -> Data {
        executedMLXJobs.append(job)
        if fixtureDelayMilliseconds > 0 {
            try await Task.sleep(for: .milliseconds(fixtureDelayMilliseconds))
        }
        guard let fixtureResult else {
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
        }
        return fixtureResult
    }

    func cancelMLXJob(_ instruction: Data) async throws -> Data {
        mlxCancellations.append(instruction)
        guard let cancellationAcknowledgement else {
            throw AssemblywrightMacDeveloperEventRelayError.mlxJobRejected
        }
        return cancellationAcknowledgement
    }

    func stop() async throws {
        stopped = true
    }
}

private actor FakeDeveloperAgentLauncher: AssemblywrightMacDeveloperAgentLaunching {
    let session: FakeDeveloperAgentSession
    private(set) var configurations: [AssemblywrightMacDeveloperEventRelayConfiguration] = []

    init(session: FakeDeveloperAgentSession) {
        self.session = session
    }

    func launch(
        configuration: AssemblywrightMacDeveloperEventRelayConfiguration
    ) async throws -> any AssemblywrightMacDeveloperAgentSession {
        configurations.append(configuration)
        return session
    }
}

private actor FakeBridgeEventRelay: AssemblywrightMacBridgeEventRelaying {
    let error: AssemblywrightMacDeveloperEventRelayError?
    let requiresFreshConnection: Bool
    private(set) var epochs: [UInt64] = []
    private(set) var stopped = false

    init(
        error: AssemblywrightMacDeveloperEventRelayError? = nil,
        requiresFreshConnection: Bool = false
    ) {
        self.error = error
        self.requiresFreshConnection = requiresFreshConnection
    }

    func relayEvents(
        using session: any AssemblywrightMacBridgeSession
    ) async throws -> AssemblywrightMacDeveloperEventRelayProgress {
        epochs.append(session.connectionEpoch)
        if let error { throw error }
        return AssemblywrightMacDeveloperEventRelayProgress(
            cursor: AssemblywrightMacDeveloperEventCursor(
                streamID: UUID(uuidString: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")!,
                sequence: 1
            ),
            acceptedEventCount: 1,
            hasMore: false,
            requiresFreshConnection: requiresFreshConnection
        )
    }

    func stop() async throws {
        stopped = true
    }
}

private struct FakeBridgeExecutableValidator: AssemblywrightDeveloperBridgeExecutableValidating {
    let error: AssemblywrightDeveloperBridgeProcessError?

    init(error: AssemblywrightDeveloperBridgeProcessError? = nil) {
        self.error = error
    }

    func validate(
        executableURL: URL,
        expectedTeamIdentifier: String
    ) throws -> AssemblywrightDeveloperBridgeValidatedExecutable {
        if let error { throw error }
        return AssemblywrightDeveloperBridgeValidatedExecutable(
            executableURL: executableURL,
            teamIdentifier: expectedTeamIdentifier,
            codeRequirement: "anchor apple generic",
            cdHash: Data(repeating: 0x11, count: 20)
        )
    }
}

private actor FakeBridgeProcessSession: AssemblywrightDeveloperBridgeProcessSession {
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
            throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
        }
        stopped = true
        continuation.finish()
    }
}

private actor FakeBridgeProcessLauncher: AssemblywrightDeveloperBridgeProcessLaunching {
    let session: FakeBridgeProcessSession
    private(set) var launchCount = 0

    init(session: FakeBridgeProcessSession) {
        self.session = session
    }

    func launch(
        executable _: AssemblywrightDeveloperBridgeValidatedExecutable,
        eventRelayConfiguration _: AssemblywrightMacDeveloperEventRelayConfiguration?
    ) async throws -> any AssemblywrightDeveloperBridgeProcessSession {
        launchCount += 1
        return session
    }
}

@MainActor
private func withStartedBridgeLifecycle<T: Sendable>(
    _ lifecycle: AssemblywrightDeveloperBridgeProcessLifecycle,
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
    AssemblywrightDeveloperBridgeRunningProcessValidating
{
    let error: AssemblywrightDeveloperBridgeProcessError?

    init(error: AssemblywrightDeveloperBridgeProcessError? = nil) {
        self.error = error
    }

    func validate(
        processIdentifier _: Int32,
        expected _: AssemblywrightDeveloperBridgeValidatedExecutable
    ) throws {
        if let error { throw error }
    }
}

private final class RecordingBridgeRunningProcessValidator:
    AssemblywrightDeveloperBridgeRunningProcessValidating, @unchecked Sendable
{
    private let lock = NSLock()
    private let error: AssemblywrightDeveloperBridgeProcessError?
    private var recordedProcessIdentifier: Int32?

    init(error: AssemblywrightDeveloperBridgeProcessError? = nil) {
        self.error = error
    }

    func validate(
        processIdentifier: Int32,
        expected _: AssemblywrightDeveloperBridgeValidatedExecutable
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
    @Test("HTTP parser accepts only an exactly bodyless 204 without requiring Content-Length")
    func bodylessNoContentResponseIsAcceptedStrictly() throws {
        var bodyless = Data("HTTP/1.1 204 No Content\r\nDate: now\r\n\r\n".utf8)
        let response = try #require(
            AssemblywrightMacHTTP1ResponseParser.parseResponseIfComplete(
                &bodyless,
                maximumHeaderBytes: 32 * 1_024,
                maximumWireBytes: 1_024 * 1_024
            )
        )
        #expect(response.status == 204)
        #expect(response.body.isEmpty)
        #expect(bodyless.isEmpty)

        var nonzeroLength = Data(
            "HTTP/1.1 204 No Content\r\nContent-Length: 1\r\n\r\n".utf8
        )
        #expect(throws: AssemblywrightMacDeveloperBridgeError.invalidResponse) {
            _ = try AssemblywrightMacHTTP1ResponseParser.parseResponseIfComplete(
                &nonzeroLength,
                maximumHeaderBytes: 32 * 1_024,
                maximumWireBytes: 1_024 * 1_024
            )
        }

        var hiddenBody = Data("HTTP/1.1 204 No Content\r\n\r\nx".utf8)
        #expect(throws: AssemblywrightMacDeveloperBridgeError.invalidResponse) {
            _ = try AssemblywrightMacHTTP1ResponseParser.parseResponseIfComplete(
                &hiddenBody,
                maximumHeaderBytes: 32 * 1_024,
                maximumWireBytes: 1_024 * 1_024
            )
        }

        for malformed in [
            "HTTP/1.1 204 No Content\r\nContent-Length: banana\r\n\r\n",
            "HTTP/1.1 204 No Content\r\nContent-Length : 0\r\n\r\n"
        ] {
            var response = Data(malformed.utf8)
            #expect(throws: AssemblywrightMacDeveloperBridgeError.invalidResponse) {
                _ = try AssemblywrightMacHTTP1ResponseParser.parseResponseIfComplete(
                    &response,
                    maximumHeaderBytes: 32 * 1_024,
                    maximumWireBytes: 1_024 * 1_024
                )
            }
        }
    }

    @Test("Invitation decoding is exact and bounded before Keychain staging")
    func invitationIsExactAndBounded() throws {
        let store = FakeBridgeIdentityStore()
        let enrollment = AssemblywrightMacEnrollmentCoordinator(identityStore: store)

        var unknown = try #require(
            JSONSerialization.jsonObject(with: validInvitationData()) as? [String: Any]
        )
        unknown["grant_secret"] = String(repeating: "0", count: 64)
        let unknownData = try JSONSerialization.data(withJSONObject: unknown)
        #expect(throws: AssemblywrightMacDeveloperBridgeError.invalidDocument) {
            _ = try enrollment.prepare(invitationData: unknownData)
        }
        #expect(store.staged == nil)

        #expect(throws: AssemblywrightMacDeveloperBridgeError.documentTooLarge) {
            _ = try enrollment.prepare(
                invitationData: Data(repeating: 0x20, count: AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes + 1)
            )
        }
        #expect(store.staged == nil)
    }

    @Test("Preparing enrollment stages only public binding and emits exact CSR reply")
    func prepareEmitsExactCSRReply() throws {
        let store = FakeBridgeIdentityStore()
        let enrollment = AssemblywrightMacEnrollmentCoordinator(identityStore: store)
        let reply = try enrollment.prepare(invitationData: validInvitationData())
        let json = try #require(JSONSerialization.jsonObject(with: reply) as? [String: Any])

        #expect(Set(json.keys) == Set(["schema_version", "status", "grant_id", "device_id", "csr_pem"]))
        #expect(json["status"] as? String == "enrollment_csr_ready")
        #expect(json["grant_id"] as? String == "11111111-1111-4111-8111-111111111111")
        #expect(json["device_id"] as? String == "22222222-2222-4222-8222-222222222222")
        #expect(!String(data: reply, encoding: .utf8)!.contains("grant_secret"))
        #expect(store.staged?.masterEndpoint == "100.64.23.14:7792")
    }

    @Test("Capability rebind stages and promotes a separate standard replacement only after activation")
    func capabilityRebindPreservesWorkingIdentityUntilActivation() throws {
        let store = FakeBridgeIdentityStore()
        store.installedProfile = staleFixtureProfile()
        let coordinator = AssemblywrightMacEnrollmentCoordinator(identityStore: store)
        let invitation = try rebindInvitationData()

        _ = try coordinator.prepareCapabilityRebind(invitationData: invitation)
        #expect(store.installedProfile == staleFixtureProfile())
        #expect(store.stagedReplacement?.registryRevision == 4)

        let acknowledgementData = try coordinator.stageCapabilityRebind(
            issuedReceiptData: try pendingRebindReceiptData()
        )
        let acknowledgement = try #require(
            JSONSerialization.jsonObject(with: acknowledgementData) as? [String: Any]
        )
        #expect(acknowledgement["status"] as? String == "capability_rebind_certificate_staged")
        #expect(acknowledgement["signature_algorithm"] as? String == "ecdsa_p256_sha256_der")
        #expect(store.installedProfile == staleFixtureProfile())

        var wrongActivation = try #require(
            JSONSerialization.jsonObject(with: rebindActivationData()) as? [String: Any]
        )
        wrongActivation["certificate_sha256"] = String(repeating: "0", count: 64)
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try coordinator.promoteCapabilityRebind(
                activationData: try JSONSerialization.data(withJSONObject: wrongActivation)
            )
        }
        #expect(store.installedProfile == staleFixtureProfile())

        let promoted = try coordinator.promoteCapabilityRebind(
            activationData: rebindActivationData()
        )
        #expect(promoted.deviceID == staleFixtureProfile().deviceID)
        #expect(promoted.registryRevision == 4)
        #expect(promoted.capabilities.first?.id == "mlx.reasoning")
        try coordinator.cancelCapabilityRebind()
        #expect(store.installedProfile == promoted)
    }

    @Test("Capability rebind prepare-only cancellation destructively removes only unacknowledged staging")
    func capabilityRebindPrepareOnlyCancellationIsDestructive() throws {
        let store = FakeBridgeIdentityStore()
        store.installedProfile = staleFixtureProfile()
        let coordinator = AssemblywrightMacEnrollmentCoordinator(identityStore: store)
        _ = try coordinator.prepareCapabilityRebind(invitationData: rebindInvitationData())
        try coordinator.cancelCapabilityRebind()
        #expect(store.replacementCancelled)
        #expect(store.stagedReplacement == nil)
        #expect(store.stagedReplacementReceipt == nil)
        #expect(try !assemblywrightReplacementCancellationDeletesMaterial(
            installedRecordPresent: true,
            installedKeyGeneration: "replacement_v1",
            stagedRecordPresent: false,
            stagedReceiptPresent: false
        ))
        #expect(try assemblywrightReplacementCancellationDeletesMaterial(
            installedRecordPresent: true,
            installedKeyGeneration: nil,
            stagedRecordPresent: true,
            stagedReceiptPresent: false
        ))
    }

    @Test("Capability rebind cancel refuses after staging and preserves acknowledgement recovery")
    func capabilityRebindStagedCancellationPreservesRecovery() throws {
        let store = FakeBridgeIdentityStore()
        store.installedProfile = staleFixtureProfile()
        let coordinator = AssemblywrightMacEnrollmentCoordinator(identityStore: store)
        _ = try coordinator.prepareCapabilityRebind(invitationData: rebindInvitationData())
        _ = try coordinator.stageCapabilityRebind(
            issuedReceiptData: pendingRebindReceiptData()
        )
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            try coordinator.cancelCapabilityRebind()
        }
        #expect(!store.replacementCancelled)
        #expect(store.stagedReplacement != nil)
        #expect(store.stagedReplacementReceipt != nil)
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try assemblywrightReplacementCancellationDeletesMaterial(
                installedRecordPresent: true,
                installedKeyGeneration: nil,
                stagedRecordPresent: true,
                stagedReceiptPresent: true
            )
        }
    }

    @Test("Capability rebind cancel preserves post-activation pre-promotion ambiguity")
    func capabilityRebindAmbiguousActivationCancellationPreservesRecovery() throws {
        let store = FakeBridgeIdentityStore()
        store.installedProfile = staleFixtureProfile()
        let coordinator = AssemblywrightMacEnrollmentCoordinator(identityStore: store)
        _ = try coordinator.prepareCapabilityRebind(invitationData: rebindInvitationData())
        _ = try coordinator.stageCapabilityRebind(
            issuedReceiptData: pendingRebindReceiptData()
        )
        // Windows may already have activated while its receipt was lost; local state is identical.
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            try coordinator.cancelCapabilityRebind()
        }
        #expect(store.stagedReplacementReceipt?.grantID
            == "11111111-1111-4111-8111-111111111111")
        #expect(store.installedProfile == staleFixtureProfile())
        #expect(throws: AssemblywrightMacDeveloperBridgeError.identityUnavailable) {
            _ = try assemblywrightReplacementCancellationDeletesMaterial(
                installedRecordPresent: true,
                installedKeyGeneration: "unknown",
                stagedRecordPresent: true,
                stagedReceiptPresent: false
            )
        }
    }

    @Test("Capability rebind activation proof rejects transcript tampering and a different CA")
    func capabilityRebindActivationProofIsPinnedAndTamperClosed() throws {
        let attributes: [String: Any] = [
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrKeySizeInBits as String: 256
        ]
        let caPrivateKey = try #require(
            SecKeyCreateRandomKey(attributes as CFDictionary, nil)
        )
        let wrongCAPrivateKey = try #require(
            SecKeyCreateRandomKey(attributes as CFDictionary, nil)
        )
        let caPublicKey = try #require(SecKeyCopyPublicKey(caPrivateKey))
        let wrongCAPublicKey = try #require(SecKeyCopyPublicKey(wrongCAPrivateKey))
        let unsigned = AssemblywrightMacCapabilityRebindActivation(
            status: "capability_rebind_activated",
            grantID: "11111111-1111-4111-8111-111111111111",
            deviceID: "22222222-2222-4222-8222-222222222222",
            registryRevision: 4,
            serialHex: "02",
            certificateSHA256: String(repeating: "c", count: 64),
            activatedAtMilliseconds: 2_000,
            signatureAlgorithm: assemblywrightRebindSignatureAlgorithm,
            signatureBase64: ""
        )
        let signature = try #require(
            SecKeyCreateSignature(
                caPrivateKey,
                .ecdsaSignatureMessageX962SHA256,
                assemblywrightCapabilityRebindActivationTranscript(unsigned) as CFData,
                nil
            ) as Data?
        )
        #expect(assemblywrightVerifyCapabilityRebindActivationSignature(
            unsigned,
            signature: signature,
            caPublicKey: caPublicKey
        ))
        #expect(!assemblywrightVerifyCapabilityRebindActivationSignature(
            unsigned,
            signature: signature,
            caPublicKey: wrongCAPublicKey
        ))
        let tampered = AssemblywrightMacCapabilityRebindActivation(
            status: unsigned.status,
            grantID: unsigned.grantID,
            deviceID: unsigned.deviceID,
            registryRevision: unsigned.registryRevision + 1,
            serialHex: unsigned.serialHex,
            certificateSHA256: unsigned.certificateSHA256,
            activatedAtMilliseconds: unsigned.activatedAtMilliseconds,
            signatureAlgorithm: unsigned.signatureAlgorithm,
            signatureBase64: unsigned.signatureBase64
        )
        #expect(!assemblywrightVerifyCapabilityRebindActivationSignature(
            tampered,
            signature: signature,
            caPublicKey: caPublicKey
        ))
    }

    @Test("Capability rebind rejects mixed, cross-profile, stale, and failed staging without deleting installed identity")
    func capabilityRebindRejectsUnsafeMutationAndRollsBackStage() throws {
        let store = FakeBridgeIdentityStore()
        store.installedProfile = staleFixtureProfile()
        let coordinator = AssemblywrightMacEnrollmentCoordinator(identityStore: store)
        var mixed = try #require(
            JSONSerialization.jsonObject(with: rebindInvitationData()) as? [String: Any]
        )
        var capabilities = try #require(mixed["capabilities"] as? [[String: Any]])
        capabilities.append([
            "id": "fixture.reasoning", "kind": "local_inference",
            "provider": "assemblywright-fixture", "model": "assemblywright-fixture-v1",
            "max_context_bytes": 8_192, "max_result_bytes": 8_192
        ])
        mixed["capabilities"] = capabilities
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try coordinator.prepareCapabilityRebind(
                invitationData: try JSONSerialization.data(withJSONObject: mixed)
            )
        }
        #expect(store.installedProfile == staleFixtureProfile())

        let fixtureCoordinator = AssemblywrightMacEnrollmentCoordinator(
            identityStore: store,
            identityProfile: .fixtureReasoning
        )
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try fixtureCoordinator.prepareCapabilityRebind(
                invitationData: rebindInvitationData()
            )
        }

        var stale = try #require(
            JSONSerialization.jsonObject(with: rebindInvitationData()) as? [String: Any]
        )
        stale["registry_revision"] = 3
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try coordinator.prepareCapabilityRebind(
                invitationData: try JSONSerialization.data(withJSONObject: stale)
            )
        }

        _ = try coordinator.prepareCapabilityRebind(invitationData: rebindInvitationData())
        var wrongReceipt = try #require(
            JSONSerialization.jsonObject(with: pendingRebindReceiptData()) as? [String: Any]
        )
        wrongReceipt["device_name"] = "other-device"
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try coordinator.stageCapabilityRebind(
                issuedReceiptData: try JSONSerialization.data(withJSONObject: wrongReceipt)
            )
        }
        #expect(store.replacementCancelled)
        #expect(store.installedProfile == staleFixtureProfile())
    }

    @Test("Fixture enrollment is exact and isolated from the standard profile")
    func fixtureEnrollmentIsExactAndProfileIsolated() throws {
        let rejectedStore = FakeBridgeIdentityStore()
        let fixtureEnrollment = AssemblywrightMacEnrollmentCoordinator(
            identityStore: rejectedStore,
            identityProfile: .fixtureReasoning
        )
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try fixtureEnrollment.prepare(invitationData: validInvitationData())
        }
        #expect(rejectedStore.staged == nil)

        let acceptedStore = FakeBridgeIdentityStore()
        let exactFixtureEnrollment = AssemblywrightMacEnrollmentCoordinator(
            identityStore: acceptedStore,
            identityProfile: .fixtureReasoning
        )
        _ = try exactFixtureEnrollment.prepare(invitationData: fixtureInvitationData())
        #expect(acceptedStore.staged?.capabilities == [
            AssemblywrightMacBridgeCapability(
                id: "fixture.reasoning",
                kind: "local_inference",
                provider: "assemblywright-fixture",
                model: "assemblywright-fixture-v1",
                maxContextBytes: 8_192,
                maxResultBytes: 8_192
            )
        ])
        acceptedStore.installedProfile = sampleProfile()
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try exactFixtureEnrollment.status()
        }

        let standard = AssemblywrightMacBridgeKeychainNamespace.identityProfile(.standard)
        let fixture = AssemblywrightMacBridgeKeychainNamespace.identityProfile(.fixtureReasoning)
        #expect(
            standard.service == "com.nobiletechnology.assemblywright.developer-bridge"
        )
        #expect(standard.stagedAccount == "enrollment-staged-v1")
        #expect(standard.installedAccount == "identity-installed-v1")
        #expect(
            standard.certificateLabel
                == "com.nobiletechnology.assemblywright.developer-bridge.identity-v1"
        )
        #expect(
            standard.keyTag
                == Data("com.nobiletechnology.assemblywright.developer-bridge.p256-v1".utf8)
        )
        // These Keychain service names scope items already stored on an installed
        // Mac. Renaming them orphans the enrolled identity rather than migrating
        // it, so the Assemblywright rename left them unchanged on purpose. See
        // docs/brand.md "Compatibility Names".
        #expect(
            fixture.service == "com.nobiletechnology.assemblywright.developer-bridge.fixture"
        )
        #expect(
            fixture.certificateLabel
                == "com.nobiletechnology.assemblywright.developer-bridge.fixture.identity-v1"
        )
        #expect(
            fixture.keyTag
                == Data("com.nobiletechnology.assemblywright.developer-bridge.fixture.p256-v1".utf8)
        )
        #expect(fixture.service != standard.service)
        #expect(fixture.certificateLabel != standard.certificateLabel)
        #expect(fixture.keyTag != standard.keyTag)
        #expect(standard.replacementKeyTag != standard.keyTag)
        #expect(standard.replacementCertificateLabel != standard.certificateLabel)
        #expect(standard.replacementStagedAccount != standard.stagedAccount)
        #expect(AssemblywrightMacBridgeIdentityProfile(selector: "fixture") == .fixtureReasoning)
        #expect(AssemblywrightMacBridgeIdentityProfile(selector: "mlx") == nil)
    }

    @Test("Expired and non-concrete master invitations fail before identity creation")
    func unsafeInvitationsFailClosed() throws {
        let unsafeEndpoints = ["0.0.0.0:7792", "224.0.0.1:7792", "[::]:7792", "[ff02::1]:7792", "master.local:7792"]
        for endpoint in unsafeEndpoints {
            let store = FakeBridgeIdentityStore()
            let enrollment = AssemblywrightMacEnrollmentCoordinator(identityStore: store)
            var invitation = try #require(
                JSONSerialization.jsonObject(with: validInvitationData()) as? [String: Any]
            )
            invitation["master_endpoint"] = endpoint
            #expect(throws: AssemblywrightMacDeveloperBridgeError.invalidInvitation) {
                _ = try enrollment.prepare(
                    invitationData: try JSONSerialization.data(withJSONObject: invitation)
                )
            }
            #expect(store.staged == nil)
        }

        let store = FakeBridgeIdentityStore()
        let expired = AssemblywrightMacEnrollmentCoordinator(
            identityStore: store,
            nowMilliseconds: { 4_102_444_800_000 }
        )
        #expect(throws: AssemblywrightMacDeveloperBridgeError.invitationExpired) {
            _ = try expired.prepare(invitationData: validInvitationData())
        }
        #expect(store.staged == nil)
    }

    @Test("Issued receipt mismatch fails before identity installation")
    func issuedMismatchFailsClosed() throws {
        let store = FakeBridgeIdentityStore()
        let enrollment = AssemblywrightMacEnrollmentCoordinator(identityStore: store)
        _ = try enrollment.prepare(invitationData: validInvitationData())
        var receipt = try #require(
            JSONSerialization.jsonObject(with: validIssuedReceiptData()) as? [String: Any]
        )
        receipt["device_id"] = "33333333-3333-4333-8333-333333333333"

        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try enrollment.install(issuedReceiptData: try JSONSerialization.data(withJSONObject: receipt))
        }
        #expect(store.installed == nil)

        var unknown = try #require(
            JSONSerialization.jsonObject(with: validIssuedReceiptData()) as? [String: Any]
        )
        unknown["private_key"] = "must-never-be-accepted"
        #expect(throws: AssemblywrightMacDeveloperBridgeError.invalidDocument) {
            _ = try enrollment.install(
                issuedReceiptData: try JSONSerialization.data(withJSONObject: unknown)
            )
        }
        #expect(store.installed == nil)
    }

    @Test("Windows CRLF certificate receipts install without relaxing PEM framing")
    func windowsCRLFIssuedReceiptInstalls() throws {
        let store = FakeBridgeIdentityStore()
        let enrollment = AssemblywrightMacEnrollmentCoordinator(identityStore: store)
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
        #expect(throws: AssemblywrightMacDeveloperBridgeError.invalidDocument) {
            _ = try enrollment.install(issuedReceiptData: Data(mixed.utf8))
        }
    }

    @Test("Security validity numbers use the Apple reference date")
    func securityValidityNumberUsesReferenceDate() throws {
        let date = try #require(assemblywrightCertificatePropertyDate(NSNumber(value: 809_033_786)))
        #expect(UInt64(date.timeIntervalSince1970 * 1_000) == 1_787_340_986_000)
        #expect(assemblywrightCertificatePropertyDate(NSNumber(value: Double.nan)) == nil)
    }

    @Test("Handshake is exporter-bound and accepts only the exact registered profile")
    func handshakeIsExporterBound() async throws {
        let profile = sampleProfile()
        let responseData = Data(
            #"{"protocol_version":2,"status":"accepted","connection_epoch":7,"accepted_registry_revision":3,"reason_code":null}"#.utf8
        )
        let channel = FakeBridgeChannel(
            exporter: Data(repeating: 0x42, count: 32),
            response: AssemblywrightMacBridgeHTTPResponse(status: 200, body: responseData)
        )
        let transport = AssemblywrightMacMTLSBridgeTransport(factory: FakeBridgeChannelFactory(channel: channel))
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
            response: AssemblywrightMacBridgeHTTPResponse(status: 500, body: Data())
        )
        let unavailableTransport = AssemblywrightMacMTLSBridgeTransport(
            factory: FakeBridgeChannelFactory(channel: unavailable)
        )
        await #expect(throws: AssemblywrightMacDeveloperBridgeError.channelBindingUnavailable) {
            _ = try await unavailableTransport.connect(profile: sampleProfile())
        }
        #expect(await unavailable.cancelled)

        let mismatched = FakeBridgeChannel(
            exporter: Data(repeating: 1, count: 32),
            response: AssemblywrightMacBridgeHTTPResponse(
                status: 200,
                body: Data(#"{"protocol_version":2,"status":"accepted","connection_epoch":7,"accepted_registry_revision":4,"reason_code":null}"#.utf8)
            )
        )
        let mismatchedTransport = AssemblywrightMacMTLSBridgeTransport(
            factory: FakeBridgeChannelFactory(channel: mismatched)
        )
        await #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try await mismatchedTransport.connect(profile: sampleProfile())
        }
        #expect(await mismatched.cancelled)
    }

    @Test("Bridge request gate rejects overlap and reopens after completion")
    func requestGateRejectsOverlap() async throws {
        let gate = AssemblywrightMacBridgeRequestGate()
        try await gate.begin()
        await #expect(throws: AssemblywrightMacDeveloperBridgeError.requestInFlight) {
            try await gate.begin()
        }
        await gate.finish()
        try await gate.begin()
        await gate.finish()
    }

    @Test("Supervisor keeps one authenticated session for exact health samples")
    func supervisorKeepsAuthenticatedSession() async throws {
        let response = AssemblywrightMacBridgeHTTPResponse(status: 200, body: validRemoteHealthData())
        let session = FakeSupervisorSession(
            connectionEpoch: 41,
            outcomes: [.response(response), .response(response)]
        )
        let connector = FakeSupervisorConnector(sessions: [session])
        let supervisor = AssemblywrightMacBridgeSupervisor(profile: sampleProfile(), connector: connector)

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
        let supervisor = AssemblywrightMacBridgeSupervisor(
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
            outcomes: [.response(AssemblywrightMacBridgeHTTPResponse(
                status: 200,
                body: Data(#"{"status":"ok","mode":"developer_remote_master"}"#.utf8)
            ))]
        )
        let recovered = FakeSupervisorSession(
            connectionEpoch: 8,
            outcomes: [.response(AssemblywrightMacBridgeHTTPResponse(status: 200, body: validRemoteHealthData()))]
        )
        let connector = FakeSupervisorConnector(sessions: [invalid, recovered])
        let supervisor = AssemblywrightMacBridgeSupervisor(profile: sampleProfile(), connector: connector)

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
        let configuration = AssemblywrightMacDeveloperEventRelayConfiguration(
            agentExecutableURL: URL(fileURLWithPath: "/tmp/assemblywright-agent"),
            agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/assemblywright-agent-data")
        )
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: configuration,
            launcher: launcher
        )
        let batch = Data(
            """
            {"after_sequence":0,"events":[{"connection_epoch":null,"cursor":{"sequence":1,"stream_id":"\(streamID.uuidString.lowercased())"},"device_id":null,"kind":"step_queued","occurred_at_ms":1000,"protocol_version":2,"step_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","task_id":"cccccccc-cccc-4ccc-8ccc-cccccccccccc"}],"has_more":false,"next_sequence":1,"protocol_version":2,"stream_id":"\(streamID.uuidString.lowercased())"}
            """.utf8
        )
        let master = FakeSupervisorSession(
            connectionEpoch: 51,
            outcomes: [.response(AssemblywrightMacBridgeHTTPResponse(status: 200, body: batch))]
        )

        let progress = try await relay.relayEvents(using: master)

        #expect(progress.cursor == AssemblywrightMacDeveloperEventCursor(
            streamID: streamID,
            sequence: 1
        ))
        #expect(progress.acceptedEventCount == 1)
        #expect(progress.hasMore == false)
        #expect(progress.requiresFreshConnection == false)
        #expect(await agent.acceptedBatches == [batch])
        #expect(await launcher.configurations == [configuration])
        let request = try #require(await master.requests.first)
        #expect(request.method == "POST")
        #expect(request.path == AssemblywrightMacDeveloperEventRelay.remoteEventsPath)
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
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: AssemblywrightMacDeveloperEventRelayConfiguration(
                agentExecutableURL: URL(fileURLWithPath: "/tmp/assemblywright-agent"),
                agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/assemblywright-agent-data")
            ),
            launcher: FakeDeveloperAgentLauncher(session: agent)
        )
        let malformed = Data(
            #"{"after_sequence":0,"events":[],"has_more":false,"next_sequence":2,"protocol_version":2,"stream_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"}"#.utf8
        )
        let master = FakeSupervisorSession(
            connectionEpoch: 52,
            outcomes: [.response(AssemblywrightMacBridgeHTTPResponse(status: 200, body: malformed))]
        )

        await #expect(throws: AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse) {
            _ = try await relay.relayEvents(using: master)
        }
        #expect(await agent.acceptedBatches.isEmpty)
        try await relay.stop()
    }

    @Test("Explicit fixture mode relays one exact Public synthetic job and result")
    func fixtureJobRelayCompletesExactSyntheticJob() async throws {
        let fixture = try fixtureJobDocuments(connectionEpoch: 61, delayMilliseconds: 0)
        let agent = FakeDeveloperAgentSession(fixtureResult: fixture.result)
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: AssemblywrightMacDeveloperEventRelayConfiguration(
                agentExecutableURL: URL(fileURLWithPath: "/tmp/assemblywright-agent"),
                agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/assemblywright-agent-data"),
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
        #expect(paths.contains(AssemblywrightMacDeveloperEventRelay.remoteLeasePath))
        #expect(paths.contains(AssemblywrightMacDeveloperEventRelay.remoteResultPath))
        #expect(!paths.contains(
            AssemblywrightMacDeveloperEventRelay.remoteCancellationAcknowledgementPath
        ))
        #expect(!(await master.cancelled))
        try await relay.stop()
    }

    @Test("Fixture lease no-work requires a fresh authenticated connection")
    func fixtureLeaseNoWorkRequiresFreshConnection() async throws {
        let fixture = try fixtureJobDocuments(connectionEpoch: 61, delayMilliseconds: 0)
        let agent = FakeDeveloperAgentSession(fixtureResult: fixture.result)
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: AssemblywrightMacDeveloperEventRelayConfiguration(
                agentExecutableURL: URL(fileURLWithPath: "/tmp/assemblywright-agent"),
                agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/assemblywright-agent-data"),
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
            leaseResponse: .init(status: 204, body: Data())
        )

        let progress = try await relay.relayEvents(using: master)

        #expect(progress.requiresFreshConnection)
        #expect(await agent.executedJobs.isEmpty)
        try await relay.stop()
    }

    @Test("Cancellation no-work rejects duplicate and escaped-equivalent keys")
    func fixtureCancellationNoWorkRejectsDuplicateKeys() async throws {
        let fixture = try fixtureJobDocuments(connectionEpoch: 61, delayMilliseconds: 100)
        let agent = FakeDeveloperAgentSession(
            fixtureResult: fixture.result,
            fixtureDelayMilliseconds: 100
        )
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: AssemblywrightMacDeveloperEventRelayConfiguration(
                agentExecutableURL: URL(fileURLWithPath: "/tmp/assemblywright-agent"),
                agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/assemblywright-agent-data"),
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
            noCancellationResponse: Data(
                #"{"status":"no_cancellation","\u0073tatus":"cancel_me"}"#.utf8
            )
        )

        await #expect(throws: AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse) {
            _ = try await relay.relayEvents(using: master)
        }
        #expect(!(await master.requests.map(\.path).contains(
            AssemblywrightMacDeveloperEventRelay.remoteResultPath
        )))
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
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: AssemblywrightMacDeveloperEventRelayConfiguration(
                agentExecutableURL: URL(fileURLWithPath: "/tmp/assemblywright-agent"),
                agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/assemblywright-agent-data"),
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
            AssemblywrightMacDeveloperEventRelay.remoteCancellationAcknowledgementPath
        ))
        #expect(!paths.contains(AssemblywrightMacDeveloperEventRelay.remoteResultPath))
        try await relay.stop()
    }

    @Test("Paused fixture admission is exact no-work while other failures close the session")
    func fixturePauseIsExactNoWork() async throws {
        let fixture = try fixtureJobDocuments(connectionEpoch: 63, delayMilliseconds: 0)
        let agent = FakeDeveloperAgentSession(fixtureResult: fixture.result)
        let configuration = AssemblywrightMacDeveloperEventRelayConfiguration(
            agentExecutableURL: URL(fileURLWithPath: "/tmp/assemblywright-agent"),
            agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/assemblywright-agent-data"),
            fixtureJobsEnabled: true
        )
        let deviceID = UUID(uuidString: "22222222-2222-4222-8222-222222222222")
        let pausedRelay = AssemblywrightMacDeveloperEventRelay(
            configuration: configuration,
            deviceID: deviceID,
            launcher: FakeDeveloperAgentLauncher(session: agent)
        )
        let pausedMaster = FakeFixtureBridgeSession(
            connectionEpoch: 63,
            eventBatch: emptyEventBatch(),
            job: fixture.job,
            acceptedResult: fixture.acceptedResult,
            leaseResponse: .init(
                status: 503,
                body: Data(#"{"error":"emergency_pause_blocks_work"}"#.utf8)
            )
        )

        _ = try await pausedRelay.relayEvents(using: pausedMaster)
        #expect(await agent.executedJobs.isEmpty)
        try await pausedRelay.stop()

        let malformedRelay = AssemblywrightMacDeveloperEventRelay(
            configuration: configuration,
            deviceID: deviceID,
            launcher: FakeDeveloperAgentLauncher(session: FakeDeveloperAgentSession())
        )
        let malformedMaster = FakeFixtureBridgeSession(
            connectionEpoch: 63,
            eventBatch: emptyEventBatch(),
            job: fixture.job,
            acceptedResult: fixture.acceptedResult,
            leaseResponse: .init(
                status: 503,
                body: Data(#"{"error":"maintenance_mode_blocks_work"}"#.utf8)
            )
        )
        await #expect(throws: AssemblywrightMacDeveloperEventRelayError.invalidMasterResponse) {
            _ = try await malformedRelay.relayEvents(using: malformedMaster)
        }
        try await malformedRelay.stop()
    }

    @Test("Supervisor cancels the authenticated session when its local relay fails")
    func supervisorFailsClosedOnEventRelayFailure() async {
        let session = FakeSupervisorSession(
            connectionEpoch: 53,
            outcomes: [.response(AssemblywrightMacBridgeHTTPResponse(
                status: 200,
                body: validRemoteHealthData()
            ))]
        )
        let relay = FakeBridgeEventRelay(error: .eventCursorRejected)
        let supervisor = AssemblywrightMacBridgeSupervisor(
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

    @Test("Fixture no-work closes the 204 connection before the next sample")
    func supervisorReconnectsAfterFixtureNoWork() async {
        let response = AssemblywrightMacBridgeHTTPResponse(
            status: 200,
            body: validRemoteHealthData()
        )
        let firstSession = FakeSupervisorSession(
            connectionEpoch: 54,
            outcomes: [.response(response)]
        )
        let secondSession = FakeSupervisorSession(
            connectionEpoch: 55,
            outcomes: [.response(response)]
        )
        let connector = FakeSupervisorConnector(sessions: [firstSession, secondSession])
        let relay = FakeBridgeEventRelay(requiresFreshConnection: true)
        let supervisor = AssemblywrightMacBridgeSupervisor(
            profile: sampleProfile(),
            connector: connector,
            eventRelay: relay
        )

        let first = await supervisor.sample()
        let second = await supervisor.sample()

        #expect(first.phase == .authenticated)
        #expect(first.connectionEpoch == 54)
        #expect(await firstSession.cancelled)
        #expect(second.phase == .authenticated)
        #expect(second.connectionEpoch == 55)
        #expect(await connector.connectCount == 2)
        await supervisor.stop()
    }

    @Test("App relay opt-in requires both absolute agent paths and keeps startup secret-free")
    func appRelayConfigurationIsExactAndSecretFree() throws {
        let complete = AssemblywrightDeveloperBridgeProcessConfiguration(environment: [
            AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/assemblywright-mac-bridge",
            AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ",
            AssemblywrightDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey:
                "/tmp/assemblywright-agent",
            AssemblywrightDeveloperBridgeProcessConfiguration.agentDataDirectoryEnvironmentKey:
                "/tmp/assemblywright-agent-data"
        ])
        let relay = try #require(complete.eventRelayConfiguration)
        let document = try relay.encodeStartupDocument()
        let decoded = try AssemblywrightMacDeveloperEventRelayConfiguration
            .decodeStartupDocument(document)
        #expect(decoded == relay)
        #expect(!String(decoding: document, as: UTF8.self).contains("bearer"))
        #expect(!relay.fixtureJobsEnabled)

        let partial = AssemblywrightDeveloperBridgeProcessConfiguration(environment: [
            AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/assemblywright-mac-bridge",
            AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ",
            AssemblywrightDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey:
                "/tmp/assemblywright-agent"
        ])
        #expect(partial.executableURL == nil)
        #expect(partial.eventRelayConfiguration == nil)

        let fixture = AssemblywrightDeveloperBridgeProcessConfiguration(environment: [
            AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/assemblywright-mac-bridge",
            AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ",
            AssemblywrightDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey:
                "/tmp/assemblywright-agent",
            AssemblywrightDeveloperBridgeProcessConfiguration.agentDataDirectoryEnvironmentKey:
                "/tmp/assemblywright-agent-data",
            AssemblywrightDeveloperBridgeProcessConfiguration.fixtureJobsEnabledEnvironmentKey:
                "true"
        ])
        #expect(fixture.eventRelayConfiguration?.fixtureJobsEnabled == true)
        #expect(
            FoundationAssemblywrightDeveloperBridgeProcessLauncher.helperArguments(
                eventRelayConfiguration: fixture.eventRelayConfiguration
            ) == ["relay", "--identity-profile", "fixture"]
        )
        #expect(
            FoundationAssemblywrightDeveloperBridgeProcessLauncher.helperArguments(
                eventRelayConfiguration: relay
            ) == ["relay"]
        )
        #expect(
            FoundationAssemblywrightDeveloperBridgeProcessLauncher.helperArguments(
                eventRelayConfiguration: nil
            ) == ["monitor"]
        )

        let unsafeFixture = AssemblywrightDeveloperBridgeProcessConfiguration(environment: [
            AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/assemblywright-mac-bridge",
            AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ",
            AssemblywrightDeveloperBridgeProcessConfiguration.fixtureJobsEnabledEnvironmentKey:
                "true"
        ])
        #expect(unsafeFixture.executableURL == nil)

        let extra = try #require(
            JSONSerialization.jsonObject(with: document) as? [String: Any]
        )
        var modified = extra
        modified["bearer_token"] = "must-not-be-accepted"
        #expect(throws: AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument) {
            _ = try AssemblywrightMacDeveloperEventRelayConfiguration.decodeStartupDocument(
                try JSONSerialization.data(withJSONObject: modified)
            )
        }
    }

    @Test("MLX relay opt-in is complete, mutually exclusive, and keeps standard identity")
    func mlxRelayConfigurationIsStrictAndUsesStandardIdentity() throws {
        let base = [
            AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/assemblywright-mac-bridge",
            AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ",
            AssemblywrightDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey:
                "/tmp/assemblywright-agent",
            AssemblywrightDeveloperBridgeProcessConfiguration.agentDataDirectoryEnvironmentKey:
                "/tmp/assemblywright-agent-data"
        ]
        var enabled = base
        enabled[AssemblywrightDeveloperBridgeProcessConfiguration.mlxJobsEnabledEnvironmentKey] =
            "true"
        enabled[AssemblywrightDeveloperBridgeProcessConfiguration.mlxExecutableEnvironmentKey] =
            "/opt/assemblywright/bin/mlx-runner"
        enabled[
            AssemblywrightDeveloperBridgeProcessConfiguration.mlxModelDirectoryEnvironmentKey
        ] = "/opt/assemblywright/models/mlx-local"
        enabled[AssemblywrightDeveloperBridgeProcessConfiguration.mlxModelIDEnvironmentKey] =
            "mlx-local"

        let configuration = AssemblywrightDeveloperBridgeProcessConfiguration(environment: enabled)
        let relay = try #require(configuration.eventRelayConfiguration)
        #expect(relay.mlxJobsEnabled)
        #expect(!relay.fixtureJobsEnabled)
        #expect(relay.mlxModelID == "mlx-local")
        #expect(
            FoundationAssemblywrightDeveloperBridgeProcessLauncher.helperArguments(
                eventRelayConfiguration: relay
            ) == ["relay"]
        )
        let document = try relay.encodeStartupDocument()
        let decoded = try AssemblywrightMacDeveloperEventRelayConfiguration
            .decodeStartupDocument(document)
        #expect(decoded == relay)
        let object = try #require(
            JSONSerialization.jsonObject(with: document) as? [String: Any]
        )
        #expect((object["version"] as? NSNumber)?.intValue == 3)
        #expect(object["mlx_executable_path"] as? String == "/opt/assemblywright/bin/mlx-runner")
        #expect(object["mlx_model_dir"] as? String == "/opt/assemblywright/models/mlx-local")
        #expect(object["mlx_model_id"] as? String == "mlx-local")

        for missing in [
            AssemblywrightDeveloperBridgeProcessConfiguration.mlxExecutableEnvironmentKey,
            AssemblywrightDeveloperBridgeProcessConfiguration.mlxModelDirectoryEnvironmentKey,
            AssemblywrightDeveloperBridgeProcessConfiguration.mlxModelIDEnvironmentKey
        ] {
            var partial = enabled
            partial.removeValue(forKey: missing)
            #expect(
                AssemblywrightDeveloperBridgeProcessConfiguration(environment: partial)
                    .executableURL == nil
            )
        }
        var mixed = enabled
        mixed[
            AssemblywrightDeveloperBridgeProcessConfiguration.fixtureJobsEnabledEnvironmentKey
        ] = "true"
        #expect(
            AssemblywrightDeveloperBridgeProcessConfiguration(environment: mixed).executableURL
                == nil
        )
        var unexpectedFields = base
        unexpectedFields[
            AssemblywrightDeveloperBridgeProcessConfiguration.mlxModelIDEnvironmentKey
        ] = "mlx-local"
        #expect(
            AssemblywrightDeveloperBridgeProcessConfiguration(environment: unexpectedFields)
                .executableURL == nil
        )

        var malformed = object
        malformed["mlx_model_id"] = "\n"
        #expect(throws: AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument) {
            _ = try AssemblywrightMacDeveloperEventRelayConfiguration.decodeStartupDocument(
                try JSONSerialization.data(withJSONObject: malformed)
            )
        }
        malformed = object
        malformed["mlx_model_dir"] = "relative/model"
        #expect(throws: AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument) {
            _ = try AssemblywrightMacDeveloperEventRelayConfiguration.decodeStartupDocument(
                try JSONSerialization.data(withJSONObject: malformed)
            )
        }
        malformed = object
        malformed["fixture_jobs_enabled"] = true
        #expect(throws: AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument) {
            _ = try AssemblywrightMacDeveloperEventRelayConfiguration.decodeStartupDocument(
                try JSONSerialization.data(withJSONObject: malformed)
            )
        }
    }

    @Test("MLX relay validates bounded work and returns only a bound result")
    func mlxJobRelayAcceptsValidWorkAndRejectsDrift() async throws {
        let mlx = try mlxJobDocuments(connectionEpoch: 64)
        let configuration = mlxRelayConfiguration()
        let deviceID = UUID(uuidString: "22222222-2222-4222-8222-222222222222")

        let successAgent = FakeDeveloperAgentSession(fixtureResult: mlx.result)
        let successRelay = AssemblywrightMacDeveloperEventRelay(
            configuration: configuration,
            deviceID: deviceID,
            launcher: FakeDeveloperAgentLauncher(session: successAgent)
        )
        let successMaster = FakeFixtureBridgeSession(
            connectionEpoch: 64,
            eventBatch: emptyEventBatch(),
            job: mlx.job,
            acceptedResult: mlx.acceptedResult
        )
        _ = try await successRelay.relayEvents(using: successMaster)
        #expect(await successAgent.executedMLXJobs == [mlx.job])
        #expect(await successAgent.executedJobs.isEmpty)
        #expect(
            await successMaster.requests.map(\.path).contains(
                AssemblywrightMacDeveloperEventRelay.remoteResultPath
            )
        )
        try await successRelay.stop()

        var badJob = try #require(
            JSONSerialization.jsonObject(with: mlx.job) as? [String: Any]
        )
        badJob["capability_id"] = "fixture.reasoning"
        let rejectedAgent = FakeDeveloperAgentSession(fixtureResult: mlx.result)
        let rejectedRelay = AssemblywrightMacDeveloperEventRelay(
            configuration: configuration,
            deviceID: deviceID,
            launcher: FakeDeveloperAgentLauncher(session: rejectedAgent)
        )
        let rejectedMaster = FakeFixtureBridgeSession(
            connectionEpoch: 64,
            eventBatch: emptyEventBatch(),
            job: try JSONSerialization.data(withJSONObject: badJob, options: [.sortedKeys]),
            acceptedResult: mlx.acceptedResult
        )
        await #expect(throws: AssemblywrightMacDeveloperEventRelayError.mlxJobRejected) {
            _ = try await rejectedRelay.relayEvents(using: rejectedMaster)
        }
        #expect(await rejectedAgent.executedMLXJobs.isEmpty)
        try await rejectedRelay.stop()

        var badResult = try #require(
            JSONSerialization.jsonObject(with: mlx.result) as? [String: Any]
        )
        var badPayload = try #require(badResult["payload"] as? [String: Any])
        badPayload["model"] = "other-model"
        let badPayloadData = try JSONSerialization.data(
            withJSONObject: badPayload,
            options: [.sortedKeys]
        )
        badResult["payload"] = badPayload
        badResult["payload_sha256"] = Array(SHA256.hash(data: badPayloadData))
        let resultAgent = FakeDeveloperAgentSession(
            fixtureResult: try JSONSerialization.data(
                withJSONObject: badResult,
                options: [.sortedKeys]
            )
        )
        let resultRelay = AssemblywrightMacDeveloperEventRelay(
            configuration: configuration,
            deviceID: deviceID,
            launcher: FakeDeveloperAgentLauncher(session: resultAgent)
        )
        let resultMaster = FakeFixtureBridgeSession(
            connectionEpoch: 64,
            eventBatch: emptyEventBatch(),
            job: mlx.job,
            acceptedResult: mlx.acceptedResult
        )
        await #expect(throws: AssemblywrightMacDeveloperEventRelayError.mlxJobRejected) {
            _ = try await resultRelay.relayEvents(using: resultMaster)
        }
        #expect(
            !(await resultMaster.requests.map(\.path).contains(
                AssemblywrightMacDeveloperEventRelay.remoteResultPath
            ))
        )
        try await resultRelay.stop()
    }

    @Test("Authoritative cancellation suppresses an in-flight MLX result")
    func mlxJobRelayAcknowledgesCancellationWithoutResult() async throws {
        let mlx = try mlxJobDocuments(connectionEpoch: 65)
        let agent = FakeDeveloperAgentSession(
            fixtureResult: mlx.result,
            cancellationAcknowledgement: mlx.cancellationAcknowledgement,
            fixtureDelayMilliseconds: 5_000
        )
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: mlxRelayConfiguration(),
            deviceID: UUID(uuidString: "22222222-2222-4222-8222-222222222222"),
            launcher: FakeDeveloperAgentLauncher(session: agent)
        )
        let master = FakeFixtureBridgeSession(
            connectionEpoch: 65,
            eventBatch: emptyEventBatch(),
            job: mlx.job,
            cancellation: mlx.cancellation,
            acceptedResult: mlx.acceptedResult
        )

        _ = try await relay.relayEvents(using: master)

        #expect(await agent.executedMLXJobs == [mlx.job])
        #expect(await agent.mlxCancellations == [mlx.cancellation])
        let paths = await master.requests.map(\.path)
        #expect(paths.contains(
            AssemblywrightMacDeveloperEventRelay.remoteCancellationAcknowledgementPath
        ))
        #expect(!paths.contains(AssemblywrightMacDeveloperEventRelay.remoteResultPath))
        try await relay.stop()
    }

    @Test("Supervisor backoff is bounded")
    func supervisorBackoffIsBounded() {
        #expect(AssemblywrightMacBridgeSupervisor.backoffMilliseconds(for: 1) == 1_000)
        #expect(AssemblywrightMacBridgeSupervisor.backoffMilliseconds(for: 2) == 2_000)
        #expect(AssemblywrightMacBridgeSupervisor.backoffMilliseconds(for: 5) == 16_000)
        #expect(AssemblywrightMacBridgeSupervisor.backoffMilliseconds(for: 6) == 30_000)
        #expect(AssemblywrightMacBridgeSupervisor.backoffMilliseconds(for: .max) == 30_000)
    }

    @Test("Explicit reconnect cancels the old session and advances to a new epoch")
    func supervisorExplicitReconnectAdvancesEpoch() async throws {
        let response = AssemblywrightMacBridgeHTTPResponse(status: 200, body: validRemoteHealthData())
        let firstSession = FakeSupervisorSession(
            connectionEpoch: 20,
            outcomes: [.response(response)]
        )
        let secondSession = FakeSupervisorSession(
            connectionEpoch: 21,
            outcomes: [.response(response)]
        )
        let connector = FakeSupervisorConnector(sessions: [firstSession, secondSession])
        let supervisor = AssemblywrightMacBridgeSupervisor(profile: sampleProfile(), connector: connector)

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
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":2,"schema_version":2}"#.utf8
        )
        let maintenance = Data(
            #"{"connection_epoch":23,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":true,"master_endpoint":"100.64.23.14:7792","master_status":"maintenance","next_delay_ms":5000,"phase":"authenticated","protocol_version":2,"schema_version":2}"#.utf8
        )
        let paused = Data(
            #"{"connection_epoch":24,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":true,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"paused","next_delay_ms":5000,"phase":"authenticated","protocol_version":2,"schema_version":2}"#.utf8
        )

        let connectedStatus = try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: connected)
        let maintenanceStatus = try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: maintenance)
        let pausedStatus = try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: paused)

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
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":2,"schema_version":2,"service_identity":"forbidden"}"#.utf8
        )
        let contradictory = Data(
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":true,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":2,"schema_version":2}"#.utf8
        )
        let duplicate = Data(
            #"{"connection_epoch":22,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","\u0070hase":"authenticated","protocol_version":2,"schema_version":2}"#.utf8
        )
        let oversized = Data(repeating: 0x61, count: AssemblywrightDeveloperBridgeProcessLifecycle.maximumLineBytes + 1)

        #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
            try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: extra)
        }
        #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
            try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: contradictory)
        }
        #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
            try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: duplicate)
        }
        #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
            try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: oversized)
        }
    }

    @MainActor
    @Test("App helper lifecycle is default inert and owns at most one helper")
    func appHelperLifecycleIsDefaultInertAndSingleOwner() async {
        let session = FakeBridgeProcessSession(lines: [])
        let launcher = FakeBridgeProcessLauncher(session: session)
        let disabled = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [:]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher
        )
        let missingTeamPin = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/unpinned-helper"
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
            #"{"connection_epoch":44,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":2,"schema_version":2}"#.utf8
        )
        let session = FakeBridgeProcessSession(lines: [line])
        let launcher = FakeBridgeProcessLauncher(session: session)
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
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
            #"{"connection_epoch":44,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":2,"schema_version":2}"#.utf8
        )
        let session = FakeBridgeProcessSession(lines: [line])
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
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
            #"{"connection_epoch":44,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":2,"schema_version":2}"#.utf8
        )
        let session = FakeBridgeProcessSession(lines: [line], stopFailures: 2)
        let launcher = FakeBridgeProcessLauncher(session: session)
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
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
            .appendingPathComponent("assemblywright-bridge-process-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let executable = directory.appendingPathComponent("bridge-fixture")
        let snapshot = #"{"connection_epoch":45,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":2,"schema_version":2}"#
        let script = "#!/bin/sh\nprintf '%s\\n' '\(snapshot)'\nexec /bin/sleep 30\n"
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: executable.path
        )
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: executable.path,
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationAssemblywrightDeveloperBridgeProcessLauncher(
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
            .appendingPathComponent("assemblywright-bridge-overflow-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let executable = directory.appendingPathComponent("bridge-fixture")
        let snapshot = #"{"connection_epoch":46,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":2,"schema_version":2}"#
        let script = "#!/bin/sh\ni=0\nwhile [ $i -lt 1000 ]; do\n  printf '%s\\n' '\(snapshot)'\n  i=$((i + 1))\ndone\nexec /bin/sleep 30\n"
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: executable.path
        )
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: executable.path,
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationAssemblywrightDeveloperBridgeProcessLauncher(
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
            .appendingPathComponent("assemblywright-bridge-kill-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let executable = directory.appendingPathComponent("bridge-fixture")
        let snapshot = #"{"connection_epoch":47,"consecutive_failures":0,"device_id":"22222222-2222-4222-8222-222222222222","emergency_paused":false,"maintenance_active":false,"master_endpoint":"100.64.23.14:7792","master_status":"ok","next_delay_ms":5000,"phase":"authenticated","protocol_version":2,"schema_version":2}"#
        let script = "#!/bin/sh\ntrap '' TERM\nprintf '%s\\n' '\(snapshot)'\nexec /bin/sleep 30\n"
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: executable.path
        )

        let rejectedRunningValidator = RecordingBridgeRunningProcessValidator(
            error: .invalidExecutableSignature
        )
        let rejected = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: executable.path,
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationAssemblywrightDeveloperBridgeProcessLauncher(
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
        let stubborn = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: executable.path,
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationAssemblywrightDeveloperBridgeProcessLauncher(
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
        let finished = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
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
        let rejected = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/replaced-helper",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
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
        guard environment["ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_LIVE_E2E"] == "true" else {
            return
        }
        let configuration = AssemblywrightDeveloperBridgeProcessConfiguration(environment: environment)
        try #require(configuration.executableURL != nil)
        try #require(configuration.expectedTeamIdentifier != nil)
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(configuration: configuration)

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
            "assemblywright_mac_app_bridge_live_e2e_ok "
                + "endpoint=\(liveStatus.masterEndpoint ?? "missing") "
                + "connection_epoch=\(liveStatus.connectionEpoch ?? 0)"
        )
    }

    @Test("Fixture control receipts bind exact identities and ordered event sequences")
    func fixtureControlReceiptsAreStrict() throws {
        let success = try AssemblywrightMacFixtureControlReceipt.decodeStrict(Data(
            #"{"schema_version":1,"status":"fixture_success_observed","task_id":"11111111-1111-4111-8111-111111111111","step_id":"22222222-2222-4222-8222-222222222222","stream_id":"33333333-3333-4333-8333-333333333333","queued_sequence":10,"leased_sequence":11,"succeeded_sequence":12}"#.utf8
        ))
        #expect(success.status == .successObserved)
        #expect(success.queuedSequence == 10)
        #expect(success.succeededSequence == 12)

        let leased = try AssemblywrightMacFixtureControlReceipt.decodeStrict(Data(
            #"{"schema_version":1,"status":"fixture_cancellation_leased","task_id":"44444444-4444-4444-8444-444444444444","step_id":"55555555-5555-4555-8555-555555555555","stream_id":"33333333-3333-4333-8333-333333333333","queued_sequence":18,"leased_sequence":19}"#.utf8
        ))
        #expect(leased.status == .cancellationLeased)
        #expect(leased.leasedSequence == 19)

        let cancellation = try AssemblywrightMacFixtureControlReceipt.decodeStrict(Data(
            #"{"schema_version":1,"status":"fixture_cancellation_observed","task_id":"44444444-4444-4444-8444-444444444444","step_id":"55555555-5555-4555-8555-555555555555","stream_id":"33333333-3333-4333-8333-333333333333","requested_sequence":20,"acknowledged_sequence":21,"cancelled_sequence":22,"late_output_window_ms":7000}"#.utf8
        ))
        #expect(cancellation.status == .cancellationObserved)
        #expect(cancellation.cancelledSequence == 22)
        let resumed = try AssemblywrightMacFixtureControlReceipt.decodeStrict(Data(
            #"{"schema_version":1,"status":"fixture_emergency_resumed"}"#.utf8
        ))
        #expect(resumed.status == .emergencyResumed)

        for invalid in [
            #"{"schema_version":1,"status":"fixture_success_observed","task_id":"11111111-1111-4111-8111-111111111111","step_id":"22222222-2222-4222-8222-222222222222","stream_id":"33333333-3333-4333-8333-333333333333","queued_sequence":10,"leased_sequence":11,"succeeded_sequence":12,"payload":"forbidden"}"#,
            #"{"schema_version":1,"status":"fixture_success_observed","task_id":"11111111-1111-4111-8111-111111111111","step_id":"22222222-2222-4222-8222-222222222222","stream_id":"33333333-3333-4333-8333-333333333333","queued_sequence":10,"leased_sequence":11}"#,
            #"{"schema_version":1,"status":"fixture_success_observed","task_id":"11111111-1111-4111-8111-111111111111","step_id":"22222222-2222-4222-8222-222222222222","stream_id":"33333333-3333-4333-8333-333333333333","queued_sequence":10,"leased_sequence":11,"succeeded_sequence":11}"#,
            #"{"schema_version":1,"status":"fixture_cancellation_observed","task_id":"44444444-4444-4444-8444-444444444444","step_id":"55555555-5555-4555-8555-555555555555","stream_id":"33333333-3333-4333-8333-333333333333","requested_sequence":20,"acknowledged_sequence":21,"cancelled_sequence":22,"late_output_window_ms":5000}"#,
            #"{"schema_version":1,"\u0073tatus":"fixture_success_observed","status":"fixture_success_observed","task_id":"11111111-1111-4111-8111-111111111111","step_id":"22222222-2222-4222-8222-222222222222","stream_id":"33333333-3333-4333-8333-333333333333","queued_sequence":10,"leased_sequence":11,"succeeded_sequence":12}"#
        ] {
            #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
                _ = try AssemblywrightMacFixtureControlReceipt.decodeStrict(Data(invalid.utf8))
            }
        }
    }

    @Test("MLX control receipts bind exact identities and ordered event sequences")
    func mlxControlReceiptsAreStrict() throws {
        let success = try AssemblywrightMacMLXControlReceipt.decodeStrict(Data(
            #"{"schema_version":1,"status":"mlx_success_observed","task_id":"11111111-1111-4111-8111-111111111111","step_id":"22222222-2222-4222-8222-222222222222","stream_id":"33333333-3333-4333-8333-333333333333","device_id":"66666666-6666-4666-8666-666666666666","connection_epoch":7,"queued_sequence":10,"leased_sequence":11,"succeeded_sequence":12}"#.utf8
        ))
        #expect(success.status == .successObserved)
        #expect(success.succeededSequence == 12)
        #expect(success.connectionEpoch == 7)

        let leased = try AssemblywrightMacMLXControlReceipt.decodeStrict(Data(
            #"{"schema_version":1,"status":"mlx_cancellation_leased","task_id":"44444444-4444-4444-8444-444444444444","step_id":"55555555-5555-4555-8555-555555555555","stream_id":"33333333-3333-4333-8333-333333333333","device_id":"66666666-6666-4666-8666-666666666666","connection_epoch":8,"queued_sequence":18,"leased_sequence":19}"#.utf8
        ))
        #expect(leased.status == .cancellationLeased)
        #expect(leased.leasedSequence == 19)

        let cancellation = try AssemblywrightMacMLXControlReceipt.decodeStrict(Data(
            #"{"schema_version":1,"status":"mlx_cancellation_observed","task_id":"44444444-4444-4444-8444-444444444444","step_id":"55555555-5555-4555-8555-555555555555","stream_id":"33333333-3333-4333-8333-333333333333","device_id":"66666666-6666-4666-8666-666666666666","connection_epoch":8,"requested_sequence":20,"acknowledged_sequence":21,"cancelled_sequence":22,"late_output_window_ms":7000}"#.utf8
        ))
        #expect(cancellation.status == .cancellationObserved)
        #expect(cancellation.cancelledSequence == 22)

        let resumed = try AssemblywrightMacMLXControlReceipt.decodeStrict(Data(
            #"{"schema_version":1,"status":"mlx_emergency_resumed"}"#.utf8
        ))
        #expect(resumed.status == .emergencyResumed)

        for invalid in [
            #"{"status":"mlx_emergency_resumed"}"#,
            #"{"schema_version":1,"status":"mlx_success_observed","task_id":"11111111-1111-4111-8111-111111111111","step_id":"22222222-2222-4222-8222-222222222222","stream_id":"33333333-3333-4333-8333-333333333333","queued_sequence":10,"leased_sequence":11,"succeeded_sequence":12,"output":"forbidden"}"#,
            #"{"schema_version":1,"status":"mlx_cancellation_observed","task_id":"44444444-4444-4444-8444-444444444444","step_id":"55555555-5555-4555-8555-555555555555","stream_id":"33333333-3333-4333-8333-333333333333","requested_sequence":20,"acknowledged_sequence":21,"cancelled_sequence":22,"late_output_window_ms":6000}"#,
            #"{"schema_version":1,"\u0073tatus":"mlx_emergency_resumed","status":"mlx_emergency_resumed"}"#
        ] {
            #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
                _ = try AssemblywrightMacMLXControlReceipt.decodeStrict(Data(invalid.utf8))
            }
        }
    }

    @MainActor
    @Test("Live standard profile executes bounded local MLX under Windows control")
    func liveSignedHelperAppLifecycleRunsMLXJob() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard environment["ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_LIVE_E2E"] == "true" else {
            return
        }
        let configuration = AssemblywrightDeveloperBridgeProcessConfiguration(environment: environment)
        try #require(configuration.executableURL != nil)
        try #require(configuration.expectedTeamIdentifier != nil)
        try #require(configuration.eventRelayConfiguration?.mlxJobsEnabled == true)
        try #require(configuration.eventRelayConfiguration?.fixtureJobsEnabled == false)
        let coordinationDirectory = try #require(
            environment["ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_COORDINATION_DIR"]
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
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(configuration: configuration)

        let evidence = try await withStartedBridgeLifecycle(lifecycle) {
            func cancelledByHarness() throws {
                try Task.checkCancellation()
                if FileManager.default.fileExists(
                    atPath: coordinationURL.appendingPathComponent("cancel").path
                ) {
                    throw CancellationError()
                }
            }
            func controlReceipt(
                named name: String
            ) async throws -> AssemblywrightMacMLXControlReceipt {
                let url = coordinationURL.appendingPathComponent(name)
                for _ in 0 ..< 4_800 where !FileManager.default.fileExists(atPath: url.path) {
                    try cancelledByHarness()
                    try await Task.sleep(for: .milliseconds(50))
                }
                var metadata = stat()
                try #require(
                    lstat(url.path, &metadata) == 0
                        && metadata.st_mode & S_IFMT == S_IFREG
                        && metadata.st_uid == geteuid()
                        && metadata.st_mode & 0o777 == 0o600
                )
                let data = try Data(contentsOf: url, options: [.mappedIfSafe])
                return try AssemblywrightMacMLXControlReceipt.decodeStrict(data)
            }

            for _ in 0 ..< 2_400 where lifecycle.status.phase != .connected {
                try cancelledByHarness()
                try await Task.sleep(for: .milliseconds(50))
            }
            let connectedBefore = lifecycle.status
            try #require(connectedBefore.phase == .connected)
            let epochBefore = try #require(connectedBefore.connectionEpoch)
            try #require(epochBefore > 0)
            try Data("\(epochBefore)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("mlx-ready"),
                options: .atomic
            )

            let success = try await controlReceipt(named: "success-control.json")
            try #require(success.status == .successObserved)
            let successStreamID = try #require(success.streamID)
            let successTaskID = try #require(success.taskID)
            let successStepID = try #require(success.stepID)
            let succeededSequence = try #require(success.succeededSequence)
            let successDeviceID = try #require(success.deviceID)
            let successEpoch = try #require(success.connectionEpoch)
            try #require(successEpoch >= epochBefore)
            try Data("\(succeededSequence)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("success-observed"),
                options: .atomic
            )

            let leased = try await controlReceipt(named: "cancellation-control.json")
            let leasedSequence = try #require(leased.leasedSequence)
            try #require(
                leased.status == .cancellationLeased
                    && leased.streamID == successStreamID
                    && leased.deviceID == successDeviceID
                    && leased.connectionEpoch.map({ $0 >= successEpoch }) == true
                    && leased.taskID != successTaskID
                    && leased.stepID != successStepID
                    && leased.queuedSequence.map({ $0 > succeededSequence }) == true
            )
            try Data("\(leasedSequence)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("cancellation-leased-observed"),
                options: .atomic
            )

            for _ in 0 ..< 4_800 where lifecycle.status.phase != .paused {
                try cancelledByHarness()
                try await Task.sleep(for: .milliseconds(50))
            }
            let paused = lifecycle.status
            try #require(paused.phase == .paused)
            let pausedEpoch = try #require(paused.connectionEpoch)
            try #require(pausedEpoch >= epochBefore)
            let cancelled = try await controlReceipt(named: "pause-control.json")
            let cancelledSequence = try #require(cancelled.cancelledSequence)
            try #require(
                cancelled.status == .cancellationObserved
                    && cancelled.taskID == leased.taskID
                    && cancelled.stepID == leased.stepID
                    && cancelled.streamID == successStreamID
                    && cancelled.deviceID == successDeviceID
                    && cancelled.connectionEpoch == leased.connectionEpoch
                    && cancelled.requestedSequence.map({ $0 > leasedSequence }) == true
            )
            try Data("\(cancelledSequence)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("cancellation-observed"),
                options: .atomic
            )

            let resumed = try await controlReceipt(named: "resume-control.json")
            try #require(resumed.status == .emergencyResumed)
            for _ in 0 ..< 4_800 where lifecycle.status.phase != .connected {
                try cancelledByHarness()
                try await Task.sleep(for: .milliseconds(50))
            }
            let connectedAfter = lifecycle.status
            try #require(connectedAfter.phase == .connected)
            let epochAfter = try #require(connectedAfter.connectionEpoch)
            try #require(epochAfter >= pausedEpoch)
            try #require(connectedAfter.masterEndpoint == connectedBefore.masterEndpoint)
            try Data("\(epochAfter)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("mlx-complete"),
                options: .atomic
            )
            return (
                connectedBefore: connectedBefore,
                epochBefore: epochBefore,
                epochAfter: epochAfter
            )
        }

        #expect(lifecycle.status.phase == .stopped)
        print(
            "assemblywright_mac_app_mlx_live_e2e_ok "
                + "endpoint=\(evidence.connectedBefore.masterEndpoint ?? "missing") "
                + "connection_epoch_before=\(evidence.epochBefore) "
                + "connection_epoch_after=\(evidence.epochAfter)"
        )
    }

    @MainActor
    @Test("Live fixture profile executes only through owner-coordinated Windows control")
    func liveSignedHelperAppLifecycleRunsFixtureJob() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard environment["ASSEMBLYWRIGHT_MAC_DEVELOPER_FIXTURE_LIVE_E2E"] == "true" else {
            return
        }
        let configuration = AssemblywrightDeveloperBridgeProcessConfiguration(environment: environment)
        try #require(configuration.executableURL != nil)
        try #require(configuration.expectedTeamIdentifier != nil)
        try #require(configuration.eventRelayConfiguration?.fixtureJobsEnabled == true)
        let coordinationDirectory = try #require(
            environment["ASSEMBLYWRIGHT_MAC_DEVELOPER_FIXTURE_COORDINATION_DIR"]
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
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(configuration: configuration)

        let evidence = try await withStartedBridgeLifecycle(lifecycle) {
            func cancelledByHarness() throws {
                try Task.checkCancellation()
                if FileManager.default.fileExists(
                    atPath: coordinationURL.appendingPathComponent("cancel").path
                ) {
                    throw CancellationError()
                }
            }
            func controlReceipt(
                named name: String
            ) async throws -> AssemblywrightMacFixtureControlReceipt {
                let url = coordinationURL.appendingPathComponent(name)
                for _ in 0 ..< 4_800 where !FileManager.default.fileExists(atPath: url.path) {
                    try cancelledByHarness()
                    try await Task.sleep(for: .milliseconds(50))
                }
                var metadata = stat()
                try #require(
                    lstat(url.path, &metadata) == 0
                        && metadata.st_mode & S_IFMT == S_IFREG
                        && metadata.st_uid == geteuid()
                        && metadata.st_mode & 0o777 == 0o600
                )
                let data = try Data(contentsOf: url, options: [.mappedIfSafe])
                return try AssemblywrightMacFixtureControlReceipt.decodeStrict(data)
            }

            for _ in 0 ..< 2_400 where lifecycle.status.phase != .connected {
                try cancelledByHarness()
                try await Task.sleep(for: .milliseconds(50))
            }
            let connectedBefore = lifecycle.status
            try #require(connectedBefore.phase == .connected)
            let epochBefore = try #require(connectedBefore.connectionEpoch)
            try #require(epochBefore > 0)
            try Data("\(epochBefore)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("fixture-ready"),
                options: .atomic
            )

            let success = try await controlReceipt(named: "success-control.json")
            try #require(success.status == .successObserved)
            let successStreamID = try #require(success.streamID)
            let successTaskID = try #require(success.taskID)
            let successStepID = try #require(success.stepID)
            let succeededSequence = try #require(success.succeededSequence)
            try Data("\(succeededSequence)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("success-observed"),
                options: .atomic
            )

            let leased = try await controlReceipt(named: "cancellation-control.json")
            let leasedSequence = try #require(leased.leasedSequence)
            try #require(
                leased.status == .cancellationLeased
                    && leased.streamID == successStreamID
                    && leased.taskID != successTaskID
                    && leased.stepID != successStepID
                    && leased.queuedSequence.map({ $0 > succeededSequence }) == true
            )
            try Data("\(leasedSequence)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("cancellation-leased-observed"),
                options: .atomic
            )

            for _ in 0 ..< 4_800 where lifecycle.status.phase != .paused {
                try cancelledByHarness()
                try await Task.sleep(for: .milliseconds(50))
            }
            let paused = lifecycle.status
            try #require(paused.phase == .paused)
            let pausedEpoch = try #require(paused.connectionEpoch)
            try #require(pausedEpoch >= epochBefore)
            let cancelled = try await controlReceipt(named: "pause-control.json")
            let cancelledSequence = try #require(cancelled.cancelledSequence)
            try #require(
                cancelled.status == .cancellationObserved
                    && cancelled.taskID == leased.taskID
                    && cancelled.stepID == leased.stepID
                    && cancelled.streamID == successStreamID
                    && cancelled.requestedSequence.map({ $0 > leasedSequence }) == true
            )
            try Data("\(cancelledSequence)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("cancellation-observed"),
                options: .atomic
            )

            let resumed = try await controlReceipt(named: "resume-control.json")
            try #require(resumed.status == .emergencyResumed)
            for _ in 0 ..< 4_800 where lifecycle.status.phase != .connected {
                try cancelledByHarness()
                try await Task.sleep(for: .milliseconds(50))
            }
            let connectedAfter = lifecycle.status
            try #require(connectedAfter.phase == .connected)
            let epochAfter = try #require(connectedAfter.connectionEpoch)
            try #require(epochAfter >= pausedEpoch)
            try #require(connectedAfter.masterEndpoint == connectedBefore.masterEndpoint)
            try Data("\(epochAfter)\n".utf8).write(
                to: coordinationURL.appendingPathComponent("fixture-complete"),
                options: .atomic
            )
            return (
                connectedBefore: connectedBefore,
                epochBefore: epochBefore,
                epochAfter: epochAfter
            )
        }

        #expect(lifecycle.status.phase == .stopped)
        print(
            "assemblywright_mac_app_fixture_live_e2e_ok "
                + "endpoint=\(evidence.connectedBefore.masterEndpoint ?? "missing") "
                + "connection_epoch_before=\(evidence.epochBefore) "
                + "connection_epoch_after=\(evidence.epochAfter)"
        )
    }

    @MainActor
    @Test("Live production app lifecycle fails closed and recovers across a Windows outage")
    func liveSignedHelperAppLifecycleRecoversFromWindowsOutage() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard environment["ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_OUTAGE_LIVE_E2E"] == "true" else {
            return
        }
        let configuration = AssemblywrightDeveloperBridgeProcessConfiguration(environment: environment)
        try #require(configuration.executableURL != nil)
        try #require(configuration.expectedTeamIdentifier != nil)
        let coordinationDirectory = try #require(
            environment["ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_OUTAGE_COORDINATION_DIR"]
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
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(configuration: configuration)

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
            "assemblywright_mac_app_bridge_outage_recovery_live_e2e_ok "
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
        "input": "fixture/public-input",
        "delay_ms": NSNumber(value: delayMilliseconds)
    ]
    let contextData = try JSONSerialization.data(
        withJSONObject: context,
        options: [.sortedKeys, .withoutEscapingSlashes]
    )
    let contextDigest = Array(SHA256.hash(data: contextData))
    let jobObject: [String: Any] = [
        "protocol_version": 2,
        "connection_epoch": NSNumber(value: connectionEpoch),
        "sequence": 10,
        "task_id": taskID,
        "step_id": stepID,
        "attempt_id": attemptID,
        "lease_id": leaseID,
        "cancellation_id": cancellationID,
        "capability_id": "fixture.reasoning",
        "selected_model": "assemblywright-fixture-v1",
        "sensitivity": "public",
        "context_handling": "ephemeral_no_retention",
        "lease_duration_ms": 10_000,
        "deadline_after_ms": 10_000,
        "context_sha256": contextDigest,
        "context": context
    ]
    let payload: [String: Any] = [
        "operation": "synthetic_echo",
        "output": "fixture/public-input",
        "synthetic": true
    ]
    let payloadData = try JSONSerialization.data(
        withJSONObject: payload,
        options: [.sortedKeys, .withoutEscapingSlashes]
    )
    let payloadDigest = Array(SHA256.hash(data: payloadData))
    let resultObject: [String: Any] = [
        "protocol_version": 2,
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
        "protocol_version": 2,
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
        "protocol_version": 2,
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

private func mlxRelayConfiguration() -> AssemblywrightMacDeveloperEventRelayConfiguration {
    AssemblywrightMacDeveloperEventRelayConfiguration(
        agentExecutableURL: URL(fileURLWithPath: "/tmp/assemblywright-agent"),
        agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/assemblywright-agent-data"),
        mlxJobsEnabled: true,
        mlxExecutableURL: URL(fileURLWithPath: "/opt/assemblywright/bin/mlx-runner"),
        mlxModelDirectoryURL: URL(fileURLWithPath: "/opt/assemblywright/models/mlx-local"),
        mlxModelID: "mlx-community/mlx-local"
    )
}

private func mlxJobDocuments(connectionEpoch: UInt64) throws -> FixtureJobDocuments {
    let taskID = "11111111-1111-4111-8111-111111111111"
    let stepID = "22222222-2222-4222-8222-222222222222"
    let attemptID = "33333333-3333-4333-8333-333333333333"
    let leaseID = "44444444-4444-4444-8444-444444444444"
    let cancellationID = "55555555-5555-4555-8555-555555555555"
    let context: [String: Any] = [
        "operation": "generate_text",
        "prompt": "Explain the local/first boundary.",
        "max_tokens": 128,
        "temperature_milli": 700
    ]
    let contextData = try JSONSerialization.data(
        withJSONObject: context,
        options: [.sortedKeys, .withoutEscapingSlashes]
    )
    let contextDigest = Array(SHA256.hash(data: contextData))
    let jobObject: [String: Any] = [
        "protocol_version": 2,
        "connection_epoch": NSNumber(value: connectionEpoch),
        "sequence": 10,
        "task_id": taskID,
        "step_id": stepID,
        "attempt_id": attemptID,
        "lease_id": leaseID,
        "cancellation_id": cancellationID,
        "capability_id": "mlx.reasoning",
        "selected_model": "mlx-community/mlx-local",
        "sensitivity": "public",
        "context_handling": "ephemeral_no_retention",
        "lease_duration_ms": 10_000,
        "deadline_after_ms": 10_000,
        "context_sha256": contextDigest,
        "context": context
    ]
    let payload: [String: Any] = [
        "operation": "generate_text",
        "output": "Local inference stays on the enrolled Mac worker.",
        "model": "mlx-community/mlx-local"
    ]
    let payloadData = try JSONSerialization.data(
        withJSONObject: payload,
        options: [.sortedKeys, .withoutEscapingSlashes]
    )
    let payloadDigest = Array(SHA256.hash(data: payloadData))
    let resultObject: [String: Any] = [
        "protocol_version": 2,
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
        "protocol_version": 2,
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
        "protocol_version": 2,
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
        #"{"after_sequence":0,"events":[],"has_more":false,"next_sequence":0,"protocol_version":2,"stream_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"}"#.utf8
    )
}

private func validInvitationData() throws -> Data {
    Data(
        #"{"schema_version":1,"status":"enrollment_invitation_ready","grant_id":"11111111-1111-4111-8111-111111111111","device_id":"22222222-2222-4222-8222-222222222222","device_name":"owner-mac-bridge","role":"mac_bridge","registry_revision":3,"expires_at_ms":4102444800000,"capabilities":[{"id":"mlx.reasoning","kind":"local_inference","provider":"mlx","model":"test-model","max_context_bytes":262144,"max_result_bytes":786432}],"master_endpoint":"100.64.23.14:7792","ca_fingerprint_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#.utf8
    )
}

private func fixtureInvitationData() throws -> Data {
    var invitation = try #require(
        JSONSerialization.jsonObject(with: validInvitationData()) as? [String: Any]
    )
    invitation["device_name"] = "owner-mac-fixture"
    invitation["capabilities"] = [[
        "id": "fixture.reasoning",
        "kind": "local_inference",
        "provider": "assemblywright-fixture",
        "model": "assemblywright-fixture-v1",
        "max_context_bytes": 8_192,
        "max_result_bytes": 8_192
    ]]
    return try JSONSerialization.data(withJSONObject: invitation, options: [.sortedKeys])
}

private func validIssuedReceiptData() throws -> Data {
    Data(
        #"{"status":"device_certificate_issued","operation":"enroll","device_id":"22222222-2222-4222-8222-222222222222","device_name":"owner-mac-bridge","role":"mac_bridge","registry_revision":3,"serial_hex":"01","issued_at_ms":1000,"not_after_ms":4102444800000,"certificate_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","certificate_pem":"-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n","ca_certificate_pem":"-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n"}"#.utf8
    )
}

private func rebindInvitationData() throws -> Data {
    var invitation = try #require(
        JSONSerialization.jsonObject(with: validInvitationData()) as? [String: Any]
    )
    invitation["registry_revision"] = 4
    return try JSONSerialization.data(withJSONObject: invitation, options: [.sortedKeys])
}

private func pendingRebindReceiptData() throws -> Data {
    Data(
        #"{"status":"capability_rebind_certificate_pending","operation":"capability_rebind","grant_id":"11111111-1111-4111-8111-111111111111","device_id":"22222222-2222-4222-8222-222222222222","device_name":"owner-mac-bridge","role":"mac_bridge","registry_revision":4,"serial_hex":"02","issued_at_ms":1000,"not_after_ms":4102444800000,"certificate_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","certificate_pem":"-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n","ca_certificate_pem":"-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n"}"#.utf8
    )
}

private func rebindActivationData() -> Data {
    Data(
        #"{"status":"capability_rebind_activated","grant_id":"11111111-1111-4111-8111-111111111111","device_id":"22222222-2222-4222-8222-222222222222","registry_revision":4,"serial_hex":"02","certificate_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","activated_at_ms":2000,"signature_algorithm":"ecdsa_p256_sha256_der","signature_base64":"AQEBAQEBAQE="}"#.utf8
    )
}

private func staleFixtureProfile() -> AssemblywrightMacBridgeProfile {
    AssemblywrightMacBridgeProfile(
        deviceID: "22222222-2222-4222-8222-222222222222",
        deviceName: "owner-mac-bridge",
        role: "mac_bridge",
        registryRevision: 3,
        capabilities: [
            AssemblywrightMacBridgeCapability(
                id: "fixture.reasoning",
                kind: "local_inference",
                provider: "assemblywright-fixture",
                model: "assemblywright-fixture-v1",
                maxContextBytes: 8_192,
                maxResultBytes: 8_192
            )
        ],
        masterEndpoint: "100.64.23.14:7792",
        certificateNotAfterMilliseconds: 4_102_444_800_000
    )
}

private func sampleProfile() -> AssemblywrightMacBridgeProfile {
    AssemblywrightMacBridgeProfile(
        deviceID: "22222222-2222-4222-8222-222222222222",
        deviceName: "owner-mac-bridge",
        role: "mac_bridge",
        registryRevision: 3,
        capabilities: [
            AssemblywrightMacBridgeCapability(
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
        #"{"status":"ok","mode":"developer_remote_master","host_mode":"windows_service","service_identity":"MIKE-PC\\mike","maintenance_active":false,"maintenance_reason":null,"emergency_paused":false,"protocol_version":2,"schema_version":2,"process_id":43752,"started_at_ms":1784749559000,"startup_reconciliation":{"disconnected_connections":0,"abandoned_attempts":0,"requeued_steps":0},"state":{"registered_devices":1,"active_device_certificates":1,"unconsumed_enrollment_grants":2,"active_connections":1,"queued_steps":0,"leased_steps":0,"terminal_steps":0,"active_attempts":0},"boundary":"TLS 1.3 mutual authentication with enrolled-device certificate and durable revocation checks"}"#.utf8
    )
}

private func pausedRemoteHealthData() -> Data {
    Data(
        #"{"status":"paused","mode":"developer_remote_master","host_mode":"windows_service","service_identity":"MIKE-PC\\mike","maintenance_active":false,"maintenance_reason":null,"emergency_paused":true,"protocol_version":2,"schema_version":2,"process_id":43752,"started_at_ms":1784749559000,"startup_reconciliation":{"disconnected_connections":0,"abandoned_attempts":0,"requeued_steps":0},"state":{"registered_devices":1,"active_device_certificates":1,"unconsumed_enrollment_grants":2,"active_connections":1,"queued_steps":0,"leased_steps":1,"terminal_steps":0,"active_attempts":1},"boundary":"TLS 1.3 mutual authentication with enrolled-device certificate and durable revocation checks"}"#.utf8
    )
}
