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
    private let featureConveyorOutcome: FakeSupervisorOutcome
    private(set) var requests: [AssemblywrightMacBridgeHTTPRequest] = []
    private(set) var cancelled = false

    init(
        connectionEpoch: UInt64,
        outcomes: [FakeSupervisorOutcome],
        featureConveyorOutcome: FakeSupervisorOutcome = .response(
            .init(status: 200, body: validFeatureConveyorData())
        )
    ) {
        self.connectionEpoch = connectionEpoch
        self.outcomes = outcomes
        self.featureConveyorOutcome = featureConveyorOutcome
    }

    func send(_ request: AssemblywrightMacBridgeHTTPRequest) async throws -> AssemblywrightMacBridgeHTTPResponse {
        requests.append(request)
        if request.path == AssemblywrightMacBridgeSupervisor.featureConveyorPath {
            switch featureConveyorOutcome {
            case let .response(response): return response
            case .failure: throw FakeSupervisorError()
            }
        }
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

private actor FakeLocalCodingBridgeSession: AssemblywrightMacBridgeSession {
    nonisolated let connectionEpoch: UInt64
    let eventBatch: Data
    let job: Data
    private var chunks: [Data]
    let cancellation: Data?
    let acceptedResult: Data?
    let cancellationRequiresFinalChunk: Bool
    let cancellationDelayAfterFinalChunkMilliseconds: UInt64
    let rejectConcurrentRequests: Bool
    let responseDelayMilliseconds: UInt64
    let rejectResultArtifactAdmission: Bool
    let cleanupDirectoryURL: URL?
    private(set) var requests: [AssemblywrightMacBridgeHTTPRequest] = []
    private(set) var cancelled = false
    private var cancellationDelivered = false
    private var finalChunkServedAtNanoseconds: UInt64?
    private(set) var cancellationDeliveredAtNanoseconds: UInt64?
    private(set) var cancellationAcknowledgedAtNanoseconds: UInt64?
    private(set) var cleanupWasCompleteAtAcknowledgement: Bool?
    private var requestActive = false

    init(
        connectionEpoch: UInt64,
        eventBatch: Data,
        job: Data,
        chunks: [Data],
        cancellation: Data? = nil,
        acceptedResult: Data?,
        cancellationRequiresFinalChunk: Bool = false,
        cancellationDelayAfterFinalChunkMilliseconds: UInt64 = 0,
        cleanupDirectoryURL: URL? = nil,
        rejectConcurrentRequests: Bool = false,
        responseDelayMilliseconds: UInt64 = 0,
        rejectResultArtifactAdmission: Bool = false
    ) {
        self.connectionEpoch = connectionEpoch
        self.eventBatch = eventBatch
        self.job = job
        self.chunks = chunks
        self.cancellation = cancellation
        self.acceptedResult = acceptedResult
        self.cancellationRequiresFinalChunk = cancellationRequiresFinalChunk
        self.cancellationDelayAfterFinalChunkMilliseconds =
            cancellationDelayAfterFinalChunkMilliseconds
        self.cleanupDirectoryURL = cleanupDirectoryURL
        self.rejectConcurrentRequests = rejectConcurrentRequests
        self.responseDelayMilliseconds = responseDelayMilliseconds
        self.rejectResultArtifactAdmission = rejectResultArtifactAdmission
    }

    func send(_ request: AssemblywrightMacBridgeHTTPRequest) async throws
        -> AssemblywrightMacBridgeHTTPResponse
    {
        if rejectConcurrentRequests, requestActive {
            throw AssemblywrightMacDeveloperBridgeError.requestInFlight
        }
        requestActive = true
        defer { requestActive = false }
        if responseDelayMilliseconds > 0 {
            try await Task.sleep(for: .milliseconds(responseDelayMilliseconds))
        }
        requests.append(request)
        switch request.path {
        case AssemblywrightMacDeveloperEventRelay.remoteEventsPath:
            return .init(status: 200, body: eventBatch)
        case AssemblywrightMacDeveloperEventRelay.remoteLeasePath:
            return .init(status: 200, body: job)
        case AssemblywrightMacDeveloperEventRelay.remoteSnapshotChunksPath:
            guard !chunks.isEmpty else { throw FakeSupervisorError() }
            let chunk = chunks.removeFirst()
            if chunks.isEmpty {
                finalChunkServedAtNanoseconds = DispatchTime.now().uptimeNanoseconds
            }
            return .init(status: 200, body: chunk)
        case AssemblywrightMacDeveloperEventRelay.remoteCancellationPath:
            if cancellationRequiresFinalChunk {
                guard let finalChunkServedAtNanoseconds else {
                    return .init(
                        status: 200,
                        body: Data(#"{"status":"no_cancellation"}"#.utf8)
                    )
                }
                let elapsed = DispatchTime.now().uptimeNanoseconds
                    .subtractingReportingOverflow(finalChunkServedAtNanoseconds)
                let requiredNanoseconds = cancellationDelayAfterFinalChunkMilliseconds
                    .multipliedReportingOverflow(by: 1_000_000)
                guard !elapsed.overflow,
                      !requiredNanoseconds.overflow,
                      elapsed.partialValue >= requiredNanoseconds.partialValue else {
                    return .init(
                        status: 200,
                        body: Data(#"{"status":"no_cancellation"}"#.utf8)
                    )
                }
            }
            if let cancellation, !cancellationDelivered {
                cancellationDelivered = true
                cancellationDeliveredAtNanoseconds = DispatchTime.now().uptimeNanoseconds
                return .init(status: 200, body: cancellation)
            }
            return .init(
                status: 200,
                body: Data(#"{"status":"no_cancellation"}"#.utf8)
            )
        case AssemblywrightMacDeveloperEventRelay.remoteResultPath:
            if let acceptedResult {
                return .init(status: 200, body: acceptedResult)
            }
            let result = try #require(
                JSONSerialization.jsonObject(with: request.body) as? [String: Any]
            )
            return .init(
                status: 200,
                body: try JSONSerialization.data(
                    withJSONObject: [
                        "task_id": try #require(result["task_id"] as? String),
                        "step_id": try #require(result["step_id"] as? String),
                        "status": "succeeded",
                        "payload_sha256": try #require(result["payload_sha256"] as? [Any])
                    ],
                    options: [.sortedKeys]
                )
            )
        case AssemblywrightMacDeveloperEventRelay.remoteResultArtifactsPath:
            if rejectResultArtifactAdmission {
                return .init(
                    status: 409,
                    body: Data(#"{"error":"result_artifact_admission_rejected"}"#.utf8)
                )
            }
            let admission = try #require(
                JSONSerialization.jsonObject(with: request.body) as? [String: Any]
            )
            let artifact = try #require(admission["artifact"] as? [String: Any])
            return .init(
                status: 200,
                body: try JSONSerialization.data(
                    withJSONObject: [
                        "protocol_version": admission["protocol_version"]!,
                        "connection_epoch": admission["connection_epoch"]!,
                        "sequence": admission["sequence"]!,
                        "task_id": admission["task_id"]!,
                        "step_id": admission["step_id"]!,
                        "attempt_id": admission["attempt_id"]!,
                        "lease_id": admission["lease_id"]!,
                        "cancellation_id": admission["cancellation_id"]!,
                        "artifact_id": artifact["artifact_id"]!,
                        "artifact_sha256": artifact["artifact_sha256"]!,
                        "artifact_size_bytes": artifact["artifact_size_bytes"]!,
                        "workspace_retained": admission["workspace_retained"]!,
                        "workspace_expires_at_ms": admission["workspace_expires_at_ms"]!,
                        "status": "result_artifact_admitted"
                    ],
                    options: [.sortedKeys]
                )
            )
        case AssemblywrightMacDeveloperEventRelay.remoteCancellationAcknowledgementPath:
            cancellationAcknowledgedAtNanoseconds = DispatchTime.now().uptimeNanoseconds
            if let cleanupDirectoryURL {
                let fileManager = FileManager.default
                if !fileManager.fileExists(atPath: cleanupDirectoryURL.path) {
                    cleanupWasCompleteAtAcknowledgement = true
                } else {
                    cleanupWasCompleteAtAcknowledgement =
                        (try? fileManager.contentsOfDirectory(
                            atPath: cleanupDirectoryURL.path
                        ).isEmpty) == true
                }
            }
            return .init(
                status: 200,
                body: Data(#"{"accepted":true,"status":"cancelled"}"#.utf8)
            )
        default:
            throw FakeSupervisorError()
        }
    }

    func cancel() async { cancelled = true }
}

private actor FakeDeveloperAgentSession: AssemblywrightMacDeveloperAgentSession {
    private var cursor: AssemblywrightMacDeveloperEventCursor?
    private(set) var acceptedBatches: [Data] = []
    private(set) var executedJobs: [Data] = []
    private(set) var cancellations: [Data] = []
    private(set) var executedMLXJobs: [Data] = []
    private(set) var mlxCancellations: [Data] = []
    private(set) var admittedLocalCodingJobs: [Data] = []
    private(set) var acceptedLocalCodingChunks: [Data] = []
    private(set) var localCodingCancellations: [Data] = []
    var fixtureResult: Data?
    var cancellationAcknowledgement: Data?
    private var localCodingChunkResults: [Data?]
    let fixtureDelayMilliseconds: UInt64
    let localCodingFinalChunkRejectsDuringCancellation: Bool
    let localCodingCancellationAcknowledgementDelayMilliseconds: UInt64
    private var inFlightLocalCodingFinalChunk: CheckedContinuation<Void, Never>?
    private(set) var stopped = false

    init(
        cursor: AssemblywrightMacDeveloperEventCursor? = nil,
        fixtureResult: Data? = nil,
        cancellationAcknowledgement: Data? = nil,
        fixtureDelayMilliseconds: UInt64 = 0,
        localCodingChunkResults: [Data?] = [],
        localCodingFinalChunkRejectsDuringCancellation: Bool = false,
        localCodingCancellationAcknowledgementDelayMilliseconds: UInt64 = 0
    ) {
        self.cursor = cursor
        self.fixtureResult = fixtureResult
        self.cancellationAcknowledgement = cancellationAcknowledgement
        self.fixtureDelayMilliseconds = fixtureDelayMilliseconds
        self.localCodingChunkResults = localCodingChunkResults
        self.localCodingFinalChunkRejectsDuringCancellation =
            localCodingFinalChunkRejectsDuringCancellation
        self.localCodingCancellationAcknowledgementDelayMilliseconds =
            localCodingCancellationAcknowledgementDelayMilliseconds
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

    func admitLocalCodingSnapshot(_ job: Data) async throws {
        admittedLocalCodingJobs.append(job)
    }

    func acceptLocalCodingSnapshotChunk(
        _ chunk: Data
    ) async throws -> AssemblywrightMacDeveloperAgentSnapshotChunkAcceptance {
        acceptedLocalCodingChunks.append(chunk)
        if fixtureDelayMilliseconds > 0 {
            try await Task.sleep(for: .milliseconds(fixtureDelayMilliseconds))
        }
        guard !localCodingChunkResults.isEmpty else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
        if localCodingFinalChunkRejectsDuringCancellation,
           localCodingChunkResults.count == 1 {
            await withCheckedContinuation { continuation in
                inFlightLocalCodingFinalChunk = continuation
            }
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        }
        if let result = localCodingChunkResults.removeFirst() {
            return .result(result)
        }
        let object = try #require(
            JSONSerialization.jsonObject(with: chunk) as? [String: Any]
        )
        let offset = try #require((object["offset"] as? NSNumber)?.uint64Value)
        let content = try #require(object["content_hex"] as? String)
        return .nextOffset(offset + UInt64(content.utf8.count / 2))
    }

    func cancelLocalCodingSnapshot(_ instruction: Data) async throws -> Data {
        localCodingCancellations.append(instruction)
        if localCodingFinalChunkRejectsDuringCancellation {
            guard let continuation = inFlightLocalCodingFinalChunk else {
                throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
            }
            inFlightLocalCodingFinalChunk = nil
            continuation.resume()
        }
        if localCodingCancellationAcknowledgementDelayMilliseconds > 0 {
            try await Task.sleep(
                for: .milliseconds(
                    localCodingCancellationAcknowledgementDelayMilliseconds
                )
            )
        }
        guard let cancellationAcknowledgement else {
            throw AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
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
    nonisolated let routingMode: AssemblywrightMacBridgeEventRelayRoutingMode
    let error: AssemblywrightMacDeveloperEventRelayError?
    let requiresFreshConnection: Bool
    private(set) var epochs: [UInt64] = []
    private(set) var stopped = false

    init(
        routingMode: AssemblywrightMacBridgeEventRelayRoutingMode = .metadataOnly,
        error: AssemblywrightMacDeveloperEventRelayError? = nil,
        requiresFreshConnection: Bool = false
    ) {
        self.routingMode = routingMode
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
        var wrongFixtureRole = try #require(
            JSONSerialization.jsonObject(with: fixtureInvitationData()) as? [String: Any]
        )
        wrongFixtureRole["role"] = "inference_worker"
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try exactFixtureEnrollment.prepare(
                invitationData: try JSONSerialization.data(withJSONObject: wrongFixtureRole)
            )
        }
        acceptedStore.installedProfile = sampleProfile()
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try exactFixtureEnrollment.status()
        }

        let standard = AssemblywrightMacBridgeKeychainNamespace.identityProfile(.standard)
        let fixture = AssemblywrightMacBridgeKeychainNamespace.identityProfile(.fixtureReasoning)
        let localCoding = AssemblywrightMacBridgeKeychainNamespace.identityProfile(.localCoding)
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
        #expect(
            localCoding.service
                == "com.nobiletechnology.assemblywright.developer-bridge.local-coding"
        )
        #expect(
            localCoding.certificateLabel
                == "com.nobiletechnology.assemblywright.developer-bridge.local-coding.identity-v1"
        )
        #expect(
            localCoding.keyTag
                == Data(
                    "com.nobiletechnology.assemblywright.developer-bridge.local-coding.p256-v1".utf8
                )
        )
        #expect(localCoding.service != standard.service)
        #expect(localCoding.service != fixture.service)
        #expect(localCoding.certificateLabel != standard.certificateLabel)
        #expect(localCoding.certificateLabel != fixture.certificateLabel)
        #expect(localCoding.keyTag != standard.keyTag)
        #expect(localCoding.keyTag != fixture.keyTag)
        #expect(standard.replacementKeyTag != standard.keyTag)
        #expect(standard.replacementCertificateLabel != standard.certificateLabel)
        #expect(standard.replacementStagedAccount != standard.stagedAccount)
        #expect(AssemblywrightMacBridgeIdentityProfile(selector: "fixture") == .fixtureReasoning)
        #expect(AssemblywrightMacBridgeIdentityProfile(selector: "local-coding") == .localCoding)
        #expect(AssemblywrightMacBridgeIdentityProfile(selector: "mlx") == nil)
    }

    @Test("Local-coding enrollment is an exact inference-worker profile and round trips")
    func localCodingEnrollmentIsExactAndProfileIsolated() throws {
        let standardStore = FakeBridgeIdentityStore()
        let standardEnrollment = AssemblywrightMacEnrollmentCoordinator(
            identityStore: standardStore
        )
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try standardEnrollment.prepare(invitationData: localCodingInvitationData())
        }
        #expect(standardStore.staged == nil)
        var macBridgeLocalCoding = try #require(
            JSONSerialization.jsonObject(with: localCodingInvitationData()) as? [String: Any]
        )
        macBridgeLocalCoding["role"] = "mac_bridge"
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try standardEnrollment.prepare(
                invitationData: try JSONSerialization.data(
                    withJSONObject: macBridgeLocalCoding,
                    options: [.sortedKeys]
                )
            )
        }

        let store = FakeBridgeIdentityStore()
        let enrollment = AssemblywrightMacEnrollmentCoordinator(
            identityStore: store,
            identityProfile: .localCoding
        )
        let csr = try enrollment.prepare(invitationData: localCodingInvitationData())
        let csrObject = try #require(
            JSONSerialization.jsonObject(with: csr) as? [String: Any]
        )
        #expect(csrObject["status"] as? String == "enrollment_csr_ready")
        #expect(store.staged?.role == "inference_worker")
        #expect(store.staged?.capabilities == [localCodingCapability()])

        let installed = try enrollment.install(
            issuedReceiptData: localCodingIssuedReceiptData()
        )
        #expect(installed.role == "inference_worker")
        #expect(installed.capabilities == [localCodingCapability()])
        #expect(try enrollment.status() == installed)

        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try enrollment.prepareCapabilityRebind(
                invitationData: localCodingInvitationData()
            )
        }
    }

    @Test("Local-coding profile rejects role, singleton, and descriptor drift")
    func localCodingEnrollmentRejectsDrift() throws {
        let base = try #require(
            JSONSerialization.jsonObject(with: localCodingInvitationData()) as? [String: Any]
        )
        var driftedInvitations: [[String: Any]] = []

        var wrongRole = base
        wrongRole["role"] = "mac_bridge"
        driftedInvitations.append(wrongRole)

        for (field, value) in [
            ("id", "local.coding.v2"),
            ("kind", "local_inference"),
            ("provider", "mlx"),
            ("model", "assemblywright-local-coding-v2"),
            ("max_context_bytes", 8_193),
            ("max_result_bytes", 32_769)
        ] as [(String, Any)] {
            var invitation = base
            var capabilities = try #require(invitation["capabilities"] as? [[String: Any]])
            capabilities[0][field] = value
            invitation["capabilities"] = capabilities
            driftedInvitations.append(invitation)
        }

        var mixed = base
        var mixedCapabilities = try #require(mixed["capabilities"] as? [[String: Any]])
        mixedCapabilities.append([
            "id": "fixture.reasoning",
            "kind": "local_inference",
            "provider": "assemblywright-fixture",
            "model": "assemblywright-fixture-v1",
            "max_context_bytes": 8_192,
            "max_result_bytes": 8_192
        ])
        mixed["capabilities"] = mixedCapabilities
        driftedInvitations.append(mixed)

        for invitation in driftedInvitations {
            let store = FakeBridgeIdentityStore()
            let enrollment = AssemblywrightMacEnrollmentCoordinator(
                identityStore: store,
                identityProfile: .localCoding
            )
            #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
                _ = try enrollment.prepare(
                    invitationData: try JSONSerialization.data(
                        withJSONObject: invitation,
                        options: [.sortedKeys]
                    )
                )
            }
            #expect(store.staged == nil)
        }

        let statusStore = FakeBridgeIdentityStore()
        statusStore.installedProfile = sampleProfile()
        let status = AssemblywrightMacEnrollmentCoordinator(
            identityStore: statusStore,
            identityProfile: .localCoding
        )
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try status.status()
        }
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
            #"{"protocol_version":5,"status":"accepted","connection_epoch":7,"accepted_registry_revision":3,"reason_code":null}"#.utf8
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
        #expect(handshake["protocol_version"] as? Int == 5)
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

        let staleProtocol = FakeBridgeChannel(
            exporter: Data(repeating: 1, count: 32),
            response: AssemblywrightMacBridgeHTTPResponse(
                status: 200,
                body: Data(#"{"protocol_version":2,"status":"accepted","connection_epoch":7,"accepted_registry_revision":3,"reason_code":null}"#.utf8)
            )
        )
        let staleProtocolTransport = AssemblywrightMacMTLSBridgeTransport(
            factory: FakeBridgeChannelFactory(channel: staleProtocol)
        )
        await #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try await staleProtocolTransport.connect(profile: sampleProfile())
        }
        #expect(await staleProtocol.cancelled)

        let mismatched = FakeBridgeChannel(
            exporter: Data(repeating: 1, count: 32),
            response: AssemblywrightMacBridgeHTTPResponse(
                status: 200,
                body: Data(#"{"protocol_version":5,"status":"accepted","connection_epoch":7,"accepted_registry_revision":4,"reason_code":null}"#.utf8)
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

    @Test("Local-coding FIFO skips a cancelled waiter and releases the next request")
    func localCodingRequestFIFOHandlesCancelledWaiter() async throws {
        let session = FakeLocalCodingBridgeSession(
            connectionEpoch: 1,
            eventBatch: emptyEventBatch(),
            job: Data(),
            chunks: [],
            acceptedResult: nil,
            rejectConcurrentRequests: true,
            responseDelayMilliseconds: 50
        )
        let requests = AssemblywrightMacLocalCodingSessionRequests(session: session)
        let firstRequest = AssemblywrightMacBridgeHTTPRequest(
            method: "POST",
            path: AssemblywrightMacDeveloperEventRelay.remoteCancellationPath,
            body: Data("first".utf8)
        )
        let cancelledRequest = AssemblywrightMacBridgeHTTPRequest(
            method: "POST",
            path: AssemblywrightMacDeveloperEventRelay.remoteCancellationPath,
            body: Data("cancelled".utf8)
        )
        let thirdRequest = AssemblywrightMacBridgeHTTPRequest(
            method: "POST",
            path: AssemblywrightMacDeveloperEventRelay.remoteCancellationPath,
            body: Data("third".utf8)
        )

        let first = Task { try await requests.send(firstRequest) }
        try await Task.sleep(for: .milliseconds(5))
        let cancelled = Task { try await requests.send(cancelledRequest) }
        try await Task.sleep(for: .milliseconds(5))
        cancelled.cancel()
        let third = Task { try await requests.send(thirdRequest) }

        _ = try await first.value
        do {
            _ = try await cancelled.value
            Issue.record("cancelled FIFO waiter unexpectedly reached the session")
        } catch is CancellationError {
            // Expected: cancellation is observed when the waiter reaches the FIFO head.
        }
        _ = try await third.value

        #expect(await session.requests == [firstRequest, thirdRequest])
    }

    @Test("Supervisor keeps one authenticated session across distinct master and projection schemas")
    func supervisorKeepsAuthenticatedSession() async throws {
        let response = AssemblywrightMacBridgeHTTPResponse(
            status: 200,
            body: validRemoteHealthData(schemaVersion: 11)
        )
        let session = FakeSupervisorSession(
            connectionEpoch: 41,
            outcomes: [.response(response), .response(response)]
        )
        let connector = FakeSupervisorConnector(sessions: [session])
        let supervisor = AssemblywrightMacBridgeSupervisor(profile: sampleProfile(), connector: connector)

        let first = await supervisor.sample()
        let second = await supervisor.sample()

        let encodedSnapshot = try JSONEncoder().encode(first)
        let strictlyDecodedSnapshot = try AssemblywrightMacBridgeSupervisorSnapshot.decodeStrict(
            encodedSnapshot
        )
        let encodedObject = try #require(
            JSONSerialization.jsonObject(with: encodedSnapshot) as? [String: Any]
        )
        let encodedFeatureConveyor = try #require(
            encodedObject["feature_conveyor"] as? [String: Any]
        )
        let encodedGuidance = try #require(
            encodedFeatureConveyor["owner_guidance"] as? [String: Any]
        )

        #expect(first.phase == .authenticated)
        #expect(first.connectionEpoch == 41)
        #expect(first.masterStatus == "ok")
        #expect(first.schemaVersion == 11)
        #expect(first.featureConveyor?.schemaVersion == 9)
        #expect(first.featureConveyor?.ownerGuidance.state == .idle)
        #expect(first.errorCode == nil)
        #expect(strictlyDecodedSnapshot == first)
        #expect(encodedGuidance["feature_id"] is NSNull)
        #expect(encodedGuidance["specification_revision"] is NSNull)
        #expect(encodedGuidance["lifecycle_revision"] is NSNull)
        #expect(second.phase == .authenticated)
        #expect(await connector.connectCount == 1)
        #expect(await session.requests.map(\.path) == [
            AssemblywrightMacBridgeSupervisor.healthPath,
            AssemblywrightMacBridgeSupervisor.featureConveyorPath,
            AssemblywrightMacBridgeSupervisor.healthPath,
            AssemblywrightMacBridgeSupervisor.featureConveyorPath
        ])
        #expect(await session.cancelled == false)
        await supervisor.stop()
        #expect(await session.cancelled)
    }

    @Test("Fixture MacBridge keeps strict Feature Conveyor observation")
    func fixtureSupervisorKeepsFeatureConveyorObservation() async {
        let session = FakeSupervisorSession(
            connectionEpoch: 141,
            outcomes: [.response(.init(status: 200, body: validRemoteHealthData()))]
        )
        let relay = FakeBridgeEventRelay(routingMode: .fixture)
        let supervisor = AssemblywrightMacBridgeSupervisor(
            profile: staleFixtureProfile(),
            connector: FakeSupervisorConnector(sessions: [session]),
            eventRelay: relay
        )

        let snapshot = await supervisor.sample()

        #expect(snapshot.phase == .authenticated)
        #expect(snapshot.featureConveyor?.ownerGuidance.state == .idle)
        #expect(await session.requests.map(\.path) == [
            AssemblywrightMacBridgeSupervisor.healthPath,
            AssemblywrightMacBridgeSupervisor.featureConveyorPath
        ])
        #expect(await relay.epochs == [141])
        await supervisor.stop()
    }

    @Test("Exact local-coding supervisor relays after health without MacBridge observation")
    func localCodingSupervisorSkipsFeatureConveyorObservation() async throws {
        let session = FakeSupervisorSession(
            connectionEpoch: 142,
            outcomes: [.response(.init(status: 200, body: validRemoteHealthData()))],
            featureConveyorOutcome: .failure
        )
        let connector = FakeSupervisorConnector(sessions: [session])
        let relay = FakeBridgeEventRelay(routingMode: .localCoding)
        let supervisor = AssemblywrightMacBridgeSupervisor(
            profile: localCodingProfile(),
            connector: connector,
            eventRelay: relay
        )

        let snapshot = await supervisor.sample()
        let encoded = try JSONEncoder().encode(snapshot)
        let object = try #require(JSONSerialization.jsonObject(with: encoded) as? [String: Any])

        #expect(snapshot.phase == .authenticated)
        #expect(snapshot.connectionEpoch == 142)
        #expect(snapshot.featureConveyor == nil)
        #expect(snapshot.errorCode == nil)
        #expect(object["feature_conveyor"] == nil)
        #expect(await session.requests.map(\.path) == [
            AssemblywrightMacBridgeSupervisor.healthPath
        ])
        #expect(await relay.epochs == [142])
        #expect(await connector.connectCount == 1)
        #expect(!(await session.cancelled))
        #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
            try AssemblywrightMacBridgeSupervisorSnapshot.decodeStrict(encoded)
        }
        let decoded = try AssemblywrightMacBridgeSupervisorSnapshot.decodeStrict(
            encoded,
            localCodingSnapshotsEnabled: true
        )
        #expect(decoded == snapshot)
        #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
            try AssemblywrightMacBridgeSupervisorSnapshot.decodeStrict(
                authenticatedSnapshotData(connectionEpoch: 142),
                localCodingSnapshotsEnabled: true
            )
        }
        let appStatus = try AssemblywrightDeveloperBridgeProcessLifecycle.status(
            from: encoded,
            localCodingSnapshotsEnabled: true
        )
        #expect(appStatus.phase == .connected)
        #expect(appStatus.featureConveyor == nil)

        await supervisor.stop()
        #expect(await session.cancelled)
    }

    @Test("Supervisor rejects partial, mixed, and relayless local-coding profiles before connect")
    func supervisorRejectsAmbiguousLocalCodingProfiles() async {
        let localCoding = localCodingProfile()
        let exactCapability = localCodingCapability()
        let driftedCapability = AssemblywrightMacBridgeCapability(
            id: exactCapability.id,
            kind: exactCapability.kind,
            provider: exactCapability.provider,
            model: "assemblywright-local-coding-v2",
            maxContextBytes: exactCapability.maxContextBytes,
            maxResultBytes: exactCapability.maxResultBytes
        )
        var profiles = [
            AssemblywrightMacBridgeProfile(
                deviceID: localCoding.deviceID,
                deviceName: localCoding.deviceName,
                role: "inference_worker",
                registryRevision: localCoding.registryRevision,
                capabilities: [],
                masterEndpoint: localCoding.masterEndpoint,
                certificateNotAfterMilliseconds: localCoding.certificateNotAfterMilliseconds
            ),
            AssemblywrightMacBridgeProfile(
                deviceID: localCoding.deviceID,
                deviceName: localCoding.deviceName,
                role: "mac_bridge",
                registryRevision: localCoding.registryRevision,
                capabilities: [exactCapability],
                masterEndpoint: localCoding.masterEndpoint,
                certificateNotAfterMilliseconds: localCoding.certificateNotAfterMilliseconds
            )
        ]
        profiles.append(AssemblywrightMacBridgeProfile(
            deviceID: localCoding.deviceID,
            deviceName: localCoding.deviceName,
            role: localCoding.role,
            registryRevision: localCoding.registryRevision,
            capabilities: [driftedCapability],
            masterEndpoint: localCoding.masterEndpoint,
            certificateNotAfterMilliseconds: localCoding.certificateNotAfterMilliseconds
        ))

        for profile in profiles {
            let session = FakeSupervisorSession(
                connectionEpoch: 143,
                outcomes: [.response(.init(status: 200, body: validRemoteHealthData()))]
            )
            let connector = FakeSupervisorConnector(sessions: [session])
            let relay = FakeBridgeEventRelay(routingMode: .localCoding)
            let supervisor = AssemblywrightMacBridgeSupervisor(
                profile: profile,
                connector: connector,
                eventRelay: relay
            )

            let snapshot = await supervisor.sample()

            #expect(snapshot.phase == .backingOff)
            #expect(snapshot.errorCode == "bridge_unavailable")
            #expect(await connector.connectCount == 0)
            #expect(await session.requests.isEmpty)
            #expect(await relay.epochs.isEmpty)
        }

        let connector = FakeSupervisorConnector(sessions: [])
        let relayless = AssemblywrightMacBridgeSupervisor(
            profile: localCodingProfile(),
            connector: connector
        )
        let relaylessSnapshot = await relayless.sample()
        #expect(relaylessSnapshot.phase == .backingOff)
        #expect(relaylessSnapshot.errorCode == "bridge_unavailable")
        #expect(await connector.connectCount == 0)

        for routingMode in [
            AssemblywrightMacBridgeEventRelayRoutingMode.metadataOnly,
            .fixture,
            .mlx,
            .invalid
        ] {
            let connector = FakeSupervisorConnector(sessions: [])
            let mismatchedRelay = FakeBridgeEventRelay(routingMode: routingMode)
            let mismatched = AssemblywrightMacBridgeSupervisor(
                profile: localCodingProfile(),
                connector: connector,
                eventRelay: mismatchedRelay
            )

            let snapshot = await mismatched.sample()

            #expect(snapshot.phase == .backingOff)
            #expect(snapshot.errorCode == "bridge_unavailable")
            #expect(await connector.connectCount == 0)
            #expect(await mismatchedRelay.epochs.isEmpty)
        }

        let reverseConnector = FakeSupervisorConnector(sessions: [])
        let reverseMismatchedRelay = FakeBridgeEventRelay(routingMode: .localCoding)
        let reverseMismatched = AssemblywrightMacBridgeSupervisor(
            profile: sampleProfile(),
            connector: reverseConnector,
            eventRelay: reverseMismatchedRelay
        )
        let reverseSnapshot = await reverseMismatched.sample()
        #expect(reverseSnapshot.phase == .backingOff)
        #expect(reverseSnapshot.errorCode == "bridge_unavailable")
        #expect(await reverseConnector.connectCount == 0)
        #expect(await reverseMismatchedRelay.epochs.isEmpty)
    }

    @Test("Local-coding malformed health cancels before relay and never requests status")
    func localCodingSupervisorRejectsMalformedHealth() async {
        let session = FakeSupervisorSession(
            connectionEpoch: 144,
            outcomes: [.response(.init(status: 200, body: Data(#"{"status":"ok"}"#.utf8)))],
            featureConveyorOutcome: .failure
        )
        let relay = FakeBridgeEventRelay(routingMode: .localCoding)
        let supervisor = AssemblywrightMacBridgeSupervisor(
            profile: localCodingProfile(),
            connector: FakeSupervisorConnector(sessions: [session]),
            eventRelay: relay
        )

        let snapshot = await supervisor.sample()

        #expect(snapshot.phase == .backingOff)
        #expect(snapshot.errorCode == "invalid_health")
        #expect(snapshot.featureConveyor == nil)
        #expect(await session.requests.map(\.path) == [
            AssemblywrightMacBridgeSupervisor.healthPath
        ])
        #expect(await relay.epochs.isEmpty)
        #expect(await session.cancelled)
    }

    @Test("Supervisor accepts the exact authoritative paused health projection")
    func supervisorAcceptsPausedHealth() async {
        let session = FakeSupervisorSession(
            connectionEpoch: 42,
            outcomes: [.response(.init(status: 200, body: pausedRemoteHealthData()))],
            featureConveyorOutcome: .response(
                .init(status: 200, body: pausedFeatureConveyorData())
            )
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
        #expect(snapshot.featureConveyor?.ownerGuidance.reasonCode == .emergencyPaused)
        #expect(!(await session.cancelled))
        await supervisor.stop()
    }

    @Test("Supervisor strictly accepts bounded nonempty and reconciliation Conveyor shapes")
    func supervisorAcceptsBoundedFeatureConveyorShapes() async {
        for (data, expectedState, expectedReason, expectedFeatureCount) in [
            (
                readyFeatureConveyorData(),
                AssemblywrightMacFeatureConveyorGuidanceState.ready,
                AssemblywrightMacFeatureConveyorGuidanceReason.headDependencySatisfied,
                1
            ),
            (
                reconciliationFeatureConveyorData(),
                AssemblywrightMacFeatureConveyorGuidanceState.blocked,
                AssemblywrightMacFeatureConveyorGuidanceReason.activeRequiresReconciliation,
                1
            ),
            (
                maximumFeatureConveyorData(),
                AssemblywrightMacFeatureConveyorGuidanceState.ready,
                AssemblywrightMacFeatureConveyorGuidanceReason.headDependencySatisfied,
                100
            ),
            (
                truncatedFeatureConveyorData(),
                AssemblywrightMacFeatureConveyorGuidanceState.blocked,
                AssemblywrightMacFeatureConveyorGuidanceReason.activeRequiresReconciliation,
                100
            )
        ] {
            let session = FakeSupervisorSession(
                connectionEpoch: 43,
                outcomes: [.response(.init(status: 200, body: validRemoteHealthData()))],
                featureConveyorOutcome: .response(.init(status: 200, body: data))
            )
            let supervisor = AssemblywrightMacBridgeSupervisor(
                profile: sampleProfile(),
                connector: FakeSupervisorConnector(sessions: [session])
            )

            let snapshot = await supervisor.sample()

            #expect(snapshot.phase == .authenticated)
            #expect(snapshot.featureConveyor?.ownerGuidance.state == expectedState)
            #expect(snapshot.featureConveyor?.ownerGuidance.reasonCode == expectedReason)
            #expect(snapshot.featureConveyor?.features.count == expectedFeatureCount)
            #expect(!(await session.cancelled))
            await supervisor.stop()
        }
    }

    @Test("Supervisor rejects drifted, inconsistent, duplicate, and oversized Conveyor data")
    func supervisorRejectsInvalidFeatureConveyorData() async {
        let idle = String(data: validFeatureConveyorData(), encoding: .utf8)!
        let ready = String(data: readyFeatureConveyorData(), encoding: .utf8)!
        let invalidBodies = [
            Data((String(idle.dropLast()) + #", "repository_id":"forbidden"}"#).utf8),
            Data(idle.replacingOccurrences(
                of: #""queued":0"#,
                with: #""queued":0,"\u0071ueued":0"#
            ).utf8),
            Data(idle.replacingOccurrences(of: #""state":"idle""#, with: #""state":"drifted""#).utf8),
            Data(idle.replacingOccurrences(of: #""visible_feature_count":0"#, with: #""visible_feature_count":102"#).utf8),
            Data(idle.replacingOccurrences(of: #""queued":0"#, with: #""queued":1"#).utf8),
            Data(ready.replacingOccurrences(of: #""status":"queued""#, with: #""status":"unknown""#).utf8),
            Data(ready.replacingOccurrences(
                of: #""specification_revision":1,"lifecycle_revision":1,"queue_revision":1"#,
                with: #""specification_revision":null,"lifecycle_revision":1,"queue_revision":1"#
            ).utf8),
            Data(ready.replacingOccurrences(
                of: #""queue_revision":1,"emergency_pause_revision":0"#,
                with: #""queue_revision":2,"emergency_pause_revision":0"#
            ).utf8),
            Data(repeating: 0x61, count: AssemblywrightMacBridgeSupervisor.featureConveyorMaximumBytes + 1)
        ]

        for body in invalidBodies {
            let session = FakeSupervisorSession(
                connectionEpoch: 44,
                outcomes: [.response(.init(status: 200, body: validRemoteHealthData()))],
                featureConveyorOutcome: .response(.init(status: 200, body: body))
            )
            let supervisor = AssemblywrightMacBridgeSupervisor(
                profile: sampleProfile(),
                connector: FakeSupervisorConnector(sessions: [session])
            )

            let snapshot = await supervisor.sample()

            #expect(snapshot.phase == .backingOff)
            #expect(snapshot.featureConveyor == nil)
            #expect(snapshot.errorCode == "invalid_feature_conveyor_status")
            #expect(await session.cancelled)
        }
    }

    @Test("Feature status failure stops request ordering before event relay")
    func supervisorFeatureStatusFailureCancelsBeforeRelay() async {
        let session = FakeSupervisorSession(
            connectionEpoch: 45,
            outcomes: [.response(.init(status: 200, body: validRemoteHealthData()))],
            featureConveyorOutcome: .response(.init(status: 500, body: Data()))
        )
        let relay = FakeBridgeEventRelay()
        let supervisor = AssemblywrightMacBridgeSupervisor(
            profile: sampleProfile(),
            connector: FakeSupervisorConnector(sessions: [session]),
            eventRelay: relay
        )

        let snapshot = await supervisor.sample()

        #expect(snapshot.phase == .backingOff)
        #expect(snapshot.errorCode == "invalid_feature_conveyor_status")
        #expect(await session.requests.map(\.path) == [
            AssemblywrightMacBridgeSupervisor.healthPath,
            AssemblywrightMacBridgeSupervisor.featureConveyorPath
        ])
        #expect(await relay.epochs.isEmpty)
        #expect(await session.cancelled)
    }

    @Test("Owner control submits one exact approved feature and binds the redacted receipt")
    func ownerControlApprovesAndEnqueuesExactFeature() async throws {
        let request = numericManifestApprovedFeatureOwnerControlRequestData()
        let session = FakeSupervisorSession(
            connectionEpoch: 44,
            outcomes: [
                .response(.init(status: 200, body: approvedFeatureOwnerControlReceiptData()))
            ]
        )

        let receipt = try await AssemblywrightMacFeatureConveyorOwnerControl
            .approveAndEnqueue(requestData: request, using: session)

        #expect(receipt.status == "queued")
        #expect(receipt.queueRevision == 1)
        #expect(receipt.ownerControlDesignationRevision == 3)
        let requests = await session.requests
        #expect(requests == [
            AssemblywrightMacBridgeHTTPRequest(
                method: "POST",
                path: AssemblywrightMacFeatureConveyorOwnerControl.approvedFeaturesPath,
                body: request
            )
        ])
        #expect(await session.cancelled)
    }

    @Test("Owner control rejects malformed shape and self-dependencies before the request")
    func ownerControlRejectsMalformedInputLocally() async {
        let valid = approvedFeatureOwnerControlRequestData()
        var duplicate = String(data: valid, encoding: .utf8)!
        duplicate = duplicate.replacingOccurrences(
            of: "\"schema_version\":1",
            with: "\"schema_version\":1,\"schema_version\":1"
        )
        let invalidInputs = [
            Data(duplicate.utf8),
            selfDependentApprovedFeatureOwnerControlRequestData(),
            Data(
                repeating: 0x61,
                count: AssemblywrightMacFeatureConveyorOwnerControl.maximumRequestBytes + 1
            )
        ]

        for input in invalidInputs {
            let session = FakeSupervisorSession(connectionEpoch: 45, outcomes: [])
            do {
                _ = try await AssemblywrightMacFeatureConveyorOwnerControl.approveAndEnqueue(
                    requestData: input,
                    using: session
                )
                Issue.record("malformed owner-control input was accepted")
            } catch {
                #expect(error is AssemblywrightMacFeatureConveyorOwnerControlError)
            }
            #expect(await session.requests.isEmpty)
            #expect(await session.cancelled)
        }
    }

    @Test("Owner control fails closed on denial and drifted or oversized receipts")
    func ownerControlRejectsDenialAndReceiptDrift() async {
        let requestsAndResponses = [
            (
                tamperedApprovedFeatureOwnerControlRequestData(),
                AssemblywrightMacBridgeHTTPResponse(
                status: 409,
                body: Data("{\"error\":\"approved_feature_enqueue_rejected\"}".utf8)
                )
            ),
            (
                approvedFeatureOwnerControlRequestData(),
                AssemblywrightMacBridgeHTTPResponse(
                status: 200,
                body: approvedFeatureOwnerControlReceiptData(queueRevision: 2)
                )
            ),
            (
                approvedFeatureOwnerControlRequestData(),
                AssemblywrightMacBridgeHTTPResponse(
                status: 200,
                body: Data(
                    repeating: 0x61,
                    count: AssemblywrightMacFeatureConveyorOwnerControl.maximumReceiptBytes + 1
                )
                )
            )
        ]

        for (request, response) in requestsAndResponses {
            let session = FakeSupervisorSession(
                connectionEpoch: 46,
                outcomes: [.response(response)]
            )
            do {
                _ = try await AssemblywrightMacFeatureConveyorOwnerControl.approveAndEnqueue(
                    requestData: request,
                    using: session
                )
                Issue.record("denied or drifted owner-control receipt was accepted")
            } catch {
                #expect(error is AssemblywrightMacFeatureConveyorOwnerControlError)
            }
            #expect(await session.requests.count == 1)
            #expect(await session.cancelled)
        }
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
            {"after_sequence":0,"events":[{"connection_epoch":null,"cursor":{"sequence":1,"stream_id":"\(streamID.uuidString.lowercased())"},"device_id":null,"kind":"step_queued","occurred_at_ms":1000,"protocol_version":5,"step_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","task_id":"cccccccc-cccc-4ccc-8ccc-cccccccccccc"}],"has_more":false,"next_sequence":1,"protocol_version":5,"stream_id":"\(streamID.uuidString.lowercased())"}
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
            #"{"after_sequence":0,"events":[],"has_more":false,"next_sequence":2,"protocol_version":5,"stream_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"}"#.utf8
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
        #expect(!relay.localCodingSnapshotsEnabled)
        let relayDeviceID = UUID(uuidString: "22222222-2222-4222-8222-222222222222")!
        #expect(
            AssemblywrightMacDeveloperEventRelay(
                configuration: relay,
                deviceID: relayDeviceID
            ).routingMode == .metadataOnly
        )

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
        let fixtureRelay = try #require(fixture.eventRelayConfiguration)
        #expect(
            AssemblywrightMacDeveloperEventRelay(
                configuration: fixtureRelay,
                deviceID: relayDeviceID
            ).routingMode == .fixture
        )
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

        let localCoding = AssemblywrightDeveloperBridgeProcessConfiguration(environment: [
            AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/assemblywright-mac-bridge",
            AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ",
            AssemblywrightDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey:
                "/tmp/assemblywright-agent",
            AssemblywrightDeveloperBridgeProcessConfiguration.agentDataDirectoryEnvironmentKey:
                "/tmp/assemblywright-agent-data",
            AssemblywrightDeveloperBridgeProcessConfiguration
                .localCodingSnapshotsEnabledEnvironmentKey: "true"
        ])
        #expect(localCoding.eventRelayConfiguration?.localCodingSnapshotsEnabled == true)
        #expect(localCoding.eventRelayConfiguration?.fixtureJobsEnabled == false)
        #expect(localCoding.eventRelayConfiguration?.mlxJobsEnabled == false)
        #expect(
            FoundationAssemblywrightDeveloperBridgeProcessLauncher.helperArguments(
                eventRelayConfiguration: localCoding.eventRelayConfiguration
            ) == ["relay", "--identity-profile", "local-coding"]
        )
        let localCodingRelay = try #require(localCoding.eventRelayConfiguration)
        #expect(
            AssemblywrightMacDeveloperEventRelay(
                configuration: localCodingRelay,
                deviceID: relayDeviceID
            ).routingMode == .localCoding
        )
        #expect(
            AssemblywrightMacDeveloperEventRelay(
                configuration: localCodingRelay
            ).routingMode == .invalid
        )
        #expect(
            AssemblywrightMacDeveloperEventRelay(
                configuration: localCodingRelay,
                deviceID: UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))
            ).routingMode == .invalid
        )
        let mixedRelay = AssemblywrightMacDeveloperEventRelayConfiguration(
            agentExecutableURL: URL(fileURLWithPath: "/tmp/assemblywright-agent"),
            agentDataDirectoryURL: URL(
                fileURLWithPath: "/tmp/assemblywright-agent-data",
                isDirectory: true
            ),
            fixtureJobsEnabled: true,
            localCodingSnapshotsEnabled: true
        )
        #expect(
            AssemblywrightMacDeveloperEventRelay(
                configuration: mixedRelay,
                deviceID: relayDeviceID
            ).routingMode == .invalid
        )

        let unsafeLocalCoding = AssemblywrightDeveloperBridgeProcessConfiguration(environment: [
            AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/assemblywright-mac-bridge",
            AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ",
            AssemblywrightDeveloperBridgeProcessConfiguration
                .localCodingSnapshotsEnabledEnvironmentKey: "true"
        ])
        #expect(unsafeLocalCoding.executableURL == nil)

        var mixedLocalCoding = [
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
        ]
        mixedLocalCoding[
            AssemblywrightDeveloperBridgeProcessConfiguration
                .localCodingSnapshotsEnabledEnvironmentKey
        ] = "true"
        #expect(
            AssemblywrightDeveloperBridgeProcessConfiguration(
                environment: mixedLocalCoding
            ).executableURL == nil
        )

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
            AssemblywrightMacDeveloperEventRelay(
                configuration: relay,
                deviceID: UUID(uuidString: "22222222-2222-4222-8222-222222222222")!
            ).routingMode == .mlx
        )
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
        #expect((object["version"] as? NSNumber)?.intValue == 4)
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

    @Test("Local-coding snapshot mode is default-off, strict, and mutually exclusive")
    func localCodingSnapshotConfigurationIsStrict() throws {
        let base = AssemblywrightMacDeveloperEventRelayConfiguration(
            agentExecutableURL: URL(fileURLWithPath: "/tmp/assemblywright-agent"),
            agentDataDirectoryURL: URL(
                fileURLWithPath: "/tmp/assemblywright-agent-data",
                isDirectory: true
            )
        )
        let baseDocument = try base.encodeStartupDocument()
        let baseObject = try #require(
            JSONSerialization.jsonObject(with: baseDocument) as? [String: Any]
        )
        #expect(baseObject["local_coding_snapshots_enabled"] as? Bool == false)
        #expect((baseObject["version"] as? NSNumber)?.intValue == 4)
        #expect(
            try AssemblywrightMacDeveloperEventRelayConfiguration
                .decodeStartupDocument(baseDocument) == base
        )

        let enabled = AssemblywrightMacDeveloperEventRelayConfiguration(
            agentExecutableURL: base.agentExecutableURL,
            agentDataDirectoryURL: base.agentDataDirectoryURL,
            localCodingSnapshotsEnabled: true
        )
        #expect(
            try AssemblywrightMacDeveloperEventRelayConfiguration.decodeStartupDocument(
                enabled.encodeStartupDocument()
            ) == enabled
        )
        for mixed in [
            AssemblywrightMacDeveloperEventRelayConfiguration(
                agentExecutableURL: base.agentExecutableURL,
                agentDataDirectoryURL: base.agentDataDirectoryURL,
                fixtureJobsEnabled: true,
                localCodingSnapshotsEnabled: true
            ),
            AssemblywrightMacDeveloperEventRelayConfiguration(
                agentExecutableURL: base.agentExecutableURL,
                agentDataDirectoryURL: base.agentDataDirectoryURL,
                mlxJobsEnabled: true,
                localCodingSnapshotsEnabled: true,
                mlxExecutableURL: URL(fileURLWithPath: "/opt/assemblywright/bin/mlx-runner"),
                mlxModelDirectoryURL: URL(fileURLWithPath: "/opt/assemblywright/models/mlx"),
                mlxModelID: "mlx-local"
            )
        ] {
            #expect(throws: AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument) {
                _ = try mixed.encodeStartupDocument()
            }
        }

        var extra = baseObject
        extra.removeValue(forKey: "local_coding_snapshots_enabled")
        #expect(throws: AssemblywrightMacDeveloperEventRelayError.invalidStartupDocument) {
            _ = try AssemblywrightMacDeveloperEventRelayConfiguration.decodeStartupDocument(
                try JSONSerialization.data(withJSONObject: extra)
            )
        }
    }

    @Test("Local-coding relay accepts exact contained-coding evidence")
    func localCodingSnapshotRelayCompletesExactTransfer() async throws {
        let documents = try localCodingSnapshotDocuments(connectionEpoch: 66)
        let agent = FakeDeveloperAgentSession(
            localCodingChunkResults: [nil, documents.result]
        )
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: localCodingRelayConfiguration(),
            deviceID: documents.deviceID,
            launcher: FakeDeveloperAgentLauncher(session: agent)
        )
        let master = FakeLocalCodingBridgeSession(
            connectionEpoch: 66,
            eventBatch: emptyEventBatch(),
            job: documents.job,
            chunks: documents.chunks,
            acceptedResult: documents.acceptedResult,
            rejectConcurrentRequests: true,
            responseDelayMilliseconds: 5
        )

        let progress = try await relay.relayEvents(using: master)

        #expect(progress.cursor == nil)
        #expect(progress.acceptedEventCount == 0)
        #expect(progress.hasMore == false)
        #expect(await agent.admittedLocalCodingJobs == [documents.job])
        #expect(await agent.acceptedLocalCodingChunks == documents.chunks)
        #expect(await agent.acceptedBatches.isEmpty)
        #expect(await agent.executedJobs.isEmpty)
        #expect(await agent.executedMLXJobs.isEmpty)
        let requests = await master.requests
        #expect(!requests.map(\.path).contains(
            AssemblywrightMacDeveloperEventRelay.remoteEventsPath
        ))
        let chunkRequests = requests.filter {
            $0.path == AssemblywrightMacDeveloperEventRelay.remoteSnapshotChunksPath
        }
        #expect(chunkRequests.count == 2)
        let offsets = try chunkRequests.map { request in
            let object = try #require(
                JSONSerialization.jsonObject(with: request.body) as? [String: Any]
            )
            #expect(object["snapshot_id"] as? String == documents.snapshotID)
            return try #require((object["offset"] as? NSNumber)?.uint64Value)
        }
        #expect(offsets == [0, 2])
        #expect(requests.map(\.path).contains(
            AssemblywrightMacDeveloperEventRelay.remoteResultPath
        ))
        try await relay.stop()
    }

    @Test("Production local-coding decoder accepts 4 KiB replacement and rejects one byte over")
    func localCodingReplacementBoundaryMatchesRustProtocol() async throws {
        for (count, accepted) in [(4 * 1_024, true), (4 * 1_024 + 1, false)] {
            let documents = try localCodingSnapshotDocuments(
                connectionEpoch: UInt64(6_000 + count),
                replacement: Data(repeating: 0x61, count: count)
            )
            #expect(documents.job.count <= 16 * 1_024)
            let agent = FakeDeveloperAgentSession(
                localCodingChunkResults: [nil, documents.result]
            )
            let relay = AssemblywrightMacDeveloperEventRelay(
                configuration: localCodingRelayConfiguration(),
                deviceID: documents.deviceID,
                launcher: FakeDeveloperAgentLauncher(session: agent)
            )
            let master = FakeLocalCodingBridgeSession(
                connectionEpoch: UInt64(6_000 + count),
                eventBatch: emptyEventBatch(),
                job: documents.job,
                chunks: documents.chunks,
                acceptedResult: documents.acceptedResult
            )
            if accepted {
                _ = try await relay.relayEvents(using: master)
                #expect(await agent.admittedLocalCodingJobs == [documents.job])
            } else {
                await #expect(throws: AssemblywrightMacDeveloperEventRelayError.self) {
                    _ = try await relay.relayEvents(using: master)
                }
                #expect(await agent.admittedLocalCodingJobs.isEmpty)
            }
            try await relay.stop()
        }
    }

    @Test("Local-coding admission digest matches the protocol golden transcript")
    func localCodingAdmissionDigestMatchesProtocolGoldenTranscript() {
        let digest = localCodingAdmissionDigest(
            protocolVersion: 5,
            contextDigest: [UInt8](repeating: 0x11, count: 32),
            taskID: UUID(uuidString: "00010203-0405-0607-0809-0a0b0c0d0e0f")!,
            stepID: UUID(uuidString: "10111213-1415-1617-1819-1a1b1c1d1e1f")!,
            attemptID: UUID(uuidString: "20212223-2425-2627-2829-2a2b2c2d2e2f")!,
            leaseID: UUID(uuidString: "30313233-3435-3637-3839-3a3b3c3d3e3f")!,
            cancellationID: UUID(uuidString: "40414243-4445-4647-4849-4a4b4c4d4e4f")!,
            connectionEpoch: 0x0102_0304_0506_0708,
            sequence: 0x1112_1314_1516_1718,
            leaseDurationMilliseconds: 0x2122_2324_2526_2728,
            deadlineAfterMilliseconds: 0x3132_3334_3536_3738
        )

        #expect(
            digest.map { String(format: "%02x", $0) }.joined()
                == "fb69cef80f0f2a37a886898c25121446a54308b52cb83fd70175c772936874cc"
        )
    }

    @Test("Local-coding relay rejects old or drifted contained-coding evidence")
    func localCodingSnapshotRelayRejectsResultDrift() async throws {
        let documents = try localCodingSnapshotDocuments(connectionEpoch: 70)
        let completionObject = try #require(
            JSONSerialization.jsonObject(with: documents.result) as? [String: Any]
        )
        let resultObject = try #require(completionObject["result"] as? [String: Any])
        let basePayload = try #require(resultObject["payload"] as? [String: Any])

        let oldShape: [String: Any] = [
            "status": "snapshot_materialized",
            "work_packet_sha256": basePayload["work_packet_sha256"]!,
            "admission_sha256": basePayload["admission_sha256"]!,
            "snapshot_sha256": basePayload["snapshot_sha256"]!,
            "mutation_performed": false
        ]
        var wrongDigest = basePayload
        wrongDigest["changed_paths_sha256"] = [UInt8](repeating: 0x77, count: 32)
        var wrongAdmission = basePayload
        wrongAdmission["admission_sha256"] = [UInt8](repeating: 0x44, count: 32)
        var wrongCount = basePayload
        wrongCount["changed_file_count"] = 2
        var wrongStatus = basePayload
        wrongStatus["test_status"] = "passed"
        var notRetained = basePayload
        notRetained["workspace_retained"] = false
        var ambiguous = basePayload
        ambiguous["ambiguous"] = true
        var unknown = basePayload
        unknown["repository_path"] = "/private/forbidden"

        for payload in [
            oldShape, wrongDigest, wrongAdmission, wrongCount, wrongStatus, notRetained, ambiguous,
            unknown
        ] {
            let payloadData = try JSONSerialization.data(
                withJSONObject: payload,
                options: [.sortedKeys, .withoutEscapingSlashes]
            )
            var invalidObject = resultObject
            invalidObject["payload"] = payload
            invalidObject["payload_sha256"] = Array(SHA256.hash(data: payloadData))
            var invalidCompletion = completionObject
            invalidCompletion["result"] = invalidObject
            let invalidResult = try JSONSerialization.data(
                withJSONObject: invalidCompletion,
                options: [.sortedKeys]
            )
            let agent = FakeDeveloperAgentSession(
                localCodingChunkResults: [nil, invalidResult]
            )
            let relay = AssemblywrightMacDeveloperEventRelay(
                configuration: localCodingRelayConfiguration(),
                deviceID: documents.deviceID,
                launcher: FakeDeveloperAgentLauncher(session: agent)
            )
            let master = FakeLocalCodingBridgeSession(
                connectionEpoch: 70,
                eventBatch: emptyEventBatch(),
                job: documents.job,
                chunks: documents.chunks,
                acceptedResult: documents.acceptedResult
            )

            await #expect(
                throws: AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
            ) {
                _ = try await relay.relayEvents(using: master)
            }
            #expect(!(await master.requests.map(\.path).contains(
                AssemblywrightMacDeveloperEventRelay.remoteResultPath
            )))
        }
    }

    @Test("Local-coding cancellation acknowledgement dominates transfer rejection")
    func localCodingCancellationDominatesInFlightTransferRejection() async throws {
        let documents = try localCodingSnapshotDocuments(connectionEpoch: 72)
        let agent = FakeDeveloperAgentSession(
            cancellationAcknowledgement: documents.cancellationAcknowledgement,
            localCodingChunkResults: [nil, documents.result],
            localCodingFinalChunkRejectsDuringCancellation: true,
            localCodingCancellationAcknowledgementDelayMilliseconds: 100
        )
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: localCodingRelayConfiguration(),
            deviceID: documents.deviceID,
            launcher: FakeDeveloperAgentLauncher(session: agent)
        )
        let master = FakeLocalCodingBridgeSession(
            connectionEpoch: 72,
            eventBatch: emptyEventBatch(),
            job: documents.job,
            chunks: documents.chunks,
            cancellation: documents.cancellation,
            acceptedResult: nil,
            cancellationRequiresFinalChunk: true
        )

        _ = try await relay.relayEvents(using: master)

        #expect(await agent.localCodingCancellations == [documents.cancellation])
        let requests = await master.requests.map(\.path)
        #expect(!requests.contains(
            AssemblywrightMacDeveloperEventRelay.remoteResultPath
        ))
        #expect(requests.contains(
            AssemblywrightMacDeveloperEventRelay.remoteCancellationAcknowledgementPath
        ))
        try await relay.stop()
    }

    @Test("Local-coding cancellation acknowledgement dominates artifact admission rejection")
    func localCodingCancellationDominatesArtifactAdmissionRejection() async throws {
        let documents = try localCodingSnapshotDocuments(connectionEpoch: 73)
        let agent = FakeDeveloperAgentSession(
            cancellationAcknowledgement: documents.cancellationAcknowledgement,
            localCodingChunkResults: [nil, documents.result],
            localCodingCancellationAcknowledgementDelayMilliseconds: 125
        )
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: localCodingRelayConfiguration(),
            deviceID: documents.deviceID,
            launcher: FakeDeveloperAgentLauncher(session: agent)
        )
        let master = FakeLocalCodingBridgeSession(
            connectionEpoch: 73,
            eventBatch: emptyEventBatch(),
            job: documents.job,
            chunks: documents.chunks,
            cancellation: documents.cancellation,
            acceptedResult: nil,
            cancellationRequiresFinalChunk: true,
            responseDelayMilliseconds: 50,
            rejectResultArtifactAdmission: true
        )

        _ = try await relay.relayEvents(using: master)

        #expect(await agent.localCodingCancellations == [documents.cancellation])
        let requests = await master.requests.map(\.path)
        #expect(requests.contains(
            AssemblywrightMacDeveloperEventRelay.remoteResultArtifactsPath
        ))
        #expect(!requests.contains(
            AssemblywrightMacDeveloperEventRelay.remoteResultPath
        ))
        #expect(requests.contains(
            AssemblywrightMacDeveloperEventRelay.remoteCancellationAcknowledgementPath
        ))
        try await relay.stop()
    }

    @Test("Production Swift relay transfers a snapshot through a real supervised Rust agent")
    func localCodingSnapshotRelayUsesRealSupervisedAgent() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard environment["ASSEMBLYWRIGHT_MAC_LOCAL_CODING_NATIVE_E2E"] == "true" else {
            return
        }
        let executablePath = try #require(
            environment["ASSEMBLYWRIGHT_MAC_LOCAL_CODING_AGENT_EXECUTABLE"]
        )
        let dataDirectoryPath = try #require(
            environment["ASSEMBLYWRIGHT_MAC_LOCAL_CODING_AGENT_DATA_DIR"]
        )
        let documents = try localCodingSnapshotDocuments(
            connectionEpoch: 69,
            snapshotBundle: nativeLocalCodingSnapshotBundle(),
            leaseDurationMilliseconds: 120_000,
            deadlineAfterMilliseconds: 120_000
        )
        let dataDirectoryURL = URL(
            fileURLWithPath: dataDirectoryPath,
            isDirectory: true
        )
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: AssemblywrightMacDeveloperEventRelayConfiguration(
                agentExecutableURL: URL(fileURLWithPath: executablePath),
                agentDataDirectoryURL: dataDirectoryURL,
                localCodingSnapshotsEnabled: true
            ),
            deviceID: documents.deviceID
        )
        let master = FakeLocalCodingBridgeSession(
            connectionEpoch: 69,
            eventBatch: emptyEventBatch(),
            job: documents.job,
            chunks: documents.chunks,
            acceptedResult: nil
        )

        do {
            let progress = try await relay.relayEvents(using: master)
            #expect(progress.acceptedEventCount == 0)
            #expect(progress.requiresFreshConnection == false)
            try await relay.stop()
        } catch {
            try? await relay.stop()
            throw error
        }

        let attemptRoot = dataDirectoryURL.appendingPathComponent(
            "local-coding-snapshots",
            isDirectory: true
        )
        let retainedAttemptID = "33333333-3333-4333-8333-333333333333"
        #expect(
            try FileManager.default.contentsOfDirectory(atPath: attemptRoot.path).sorted()
                == [
                    "\(retainedAttemptID).retention.json",
                    "\(retainedAttemptID).sealed",
                ]
        )
        let requests = await master.requests
        #expect(
            requests.filter {
                $0.path == AssemblywrightMacDeveloperEventRelay.remoteSnapshotChunksPath
            }.count == documents.chunks.count
        )
        #expect(requests.map(\.path).contains(
            AssemblywrightMacDeveloperEventRelay.remoteResultPath
        ))

        let cancellationDocuments = try localCodingSnapshotDocuments(
            connectionEpoch: 71,
            snapshotBundle: nativeLocalCodingSnapshotBundle(
                paddingByteCount: 16 * 1_024 * 1_024
            ),
            leaseDurationMilliseconds: 120_000,
            deadlineAfterMilliseconds: 120_000
        )
        let cancellationDataDirectoryURL = dataDirectoryURL.appendingPathComponent(
            "cancellation-agent",
            isDirectory: true
        )
        try FileManager.default.createDirectory(
            at: cancellationDataDirectoryURL,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        let cancellationAttemptRoot = cancellationDataDirectoryURL.appendingPathComponent(
            "local-coding-snapshots",
            isDirectory: true
        )
        let cancellationRelay = AssemblywrightMacDeveloperEventRelay(
            configuration: AssemblywrightMacDeveloperEventRelayConfiguration(
                agentExecutableURL: URL(fileURLWithPath: executablePath),
                agentDataDirectoryURL: cancellationDataDirectoryURL,
                localCodingSnapshotsEnabled: true
            ),
            deviceID: cancellationDocuments.deviceID
        )
        let cancellationMaster = FakeLocalCodingBridgeSession(
            connectionEpoch: 71,
            eventBatch: emptyEventBatch(),
            job: cancellationDocuments.job,
            chunks: cancellationDocuments.chunks,
            cancellation: cancellationDocuments.cancellation,
            acceptedResult: nil,
            cancellationRequiresFinalChunk: true,
            cancellationDelayAfterFinalChunkMilliseconds: 25,
            cleanupDirectoryURL: cancellationAttemptRoot
        )

        do {
            let progress = try await cancellationRelay.relayEvents(using: cancellationMaster)
            #expect(progress.acceptedEventCount == 0)
            #expect(progress.requiresFreshConnection == false)
            try await cancellationRelay.stop()
        } catch {
            try? await cancellationRelay.stop()
            throw error
        }

        let cancellationDelivered = try #require(
            await cancellationMaster.cancellationDeliveredAtNanoseconds
        )
        let cancellationAcknowledged = try #require(
            await cancellationMaster.cancellationAcknowledgedAtNanoseconds
        )
        #expect(cancellationAcknowledged > cancellationDelivered)
        let acknowledgementLatencyNanoseconds = cancellationAcknowledged - cancellationDelivered
        let cancellationAcknowledgementDeadlineNanoseconds = UInt64(2_000) * 1_000_000
        #expect(
            acknowledgementLatencyNanoseconds
                < cancellationAcknowledgementDeadlineNanoseconds
        )
        #expect(await cancellationMaster.cleanupWasCompleteAtAcknowledgement == true)
        #expect(
            try FileManager.default.contentsOfDirectory(atPath: cancellationAttemptRoot.path)
                .isEmpty
        )
        let cancellationRequests = await cancellationMaster.requests
        #expect(
            cancellationRequests.filter {
                $0.path == AssemblywrightMacDeveloperEventRelay.remoteSnapshotChunksPath
            }.count == cancellationDocuments.chunks.count
        )
        #expect(!cancellationRequests.map(\.path).contains(
            AssemblywrightMacDeveloperEventRelay.remoteResultPath
        ))
        #expect(cancellationRequests.map(\.path).contains(
            AssemblywrightMacDeveloperEventRelay.remoteCancellationAcknowledgementPath
        ))
        print(
            "assemblywright_mac_local_coding_native_e2e_ok "
                + "agent_supervision=verified sequential_transfer=verified "
                + "git_materialization=verified general_coding=verified "
                + "retained_attempt_pair=verified"
        )
        print(
            "assemblywright_mac_local_coding_native_cancellation_e2e_ok "
                + "final_verification_cancellation=verified transport_unblock=verified "
                + "local_cancel=verified cleanup_before_ack=verified no_result=verified "
                + "ack_latency_ms=\(acknowledgementLatencyNanoseconds / 1_000_000)"
        )
    }

    @Test("Local-coding relay rejects drift before forwarding and cleans partial state")
    func localCodingSnapshotRelayRejectsChunkDrift() async throws {
        let documents = try localCodingSnapshotDocuments(connectionEpoch: 67)
        var wrongIdentity = try #require(
            JSONSerialization.jsonObject(with: documents.chunks[0]) as? [String: Any]
        )
        wrongIdentity["snapshot_id"] = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
        let invalidChunk = try JSONSerialization.data(
            withJSONObject: wrongIdentity,
            options: [.sortedKeys]
        )
        let agent = FakeDeveloperAgentSession(
            localCodingChunkResults: [documents.result]
        )
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: localCodingRelayConfiguration(),
            deviceID: documents.deviceID,
            launcher: FakeDeveloperAgentLauncher(session: agent)
        )
        let master = FakeLocalCodingBridgeSession(
            connectionEpoch: 67,
            eventBatch: emptyEventBatch(),
            job: documents.job,
            chunks: [invalidChunk],
            acceptedResult: documents.acceptedResult
        )

        await #expect(
            throws: AssemblywrightMacDeveloperEventRelayError.localCodingSnapshotRejected
        ) {
            _ = try await relay.relayEvents(using: master)
        }
        #expect(await agent.acceptedLocalCodingChunks.isEmpty)
        #expect(await agent.stopped)
        #expect(!(await master.requests.map(\.path).contains(
            AssemblywrightMacDeveloperEventRelay.remoteResultPath
        )))
    }

    @Test("Authoritative cancellation removes partial local-coding materialization")
    func localCodingSnapshotCancellationDominatesTransfer() async throws {
        let documents = try localCodingSnapshotDocuments(connectionEpoch: 68)
        let agent = FakeDeveloperAgentSession(
            cancellationAcknowledgement: documents.cancellationAcknowledgement,
            fixtureDelayMilliseconds: 5_000,
            localCodingChunkResults: [documents.result]
        )
        let relay = AssemblywrightMacDeveloperEventRelay(
            configuration: localCodingRelayConfiguration(),
            deviceID: documents.deviceID,
            launcher: FakeDeveloperAgentLauncher(session: agent)
        )
        let master = FakeLocalCodingBridgeSession(
            connectionEpoch: 68,
            eventBatch: emptyEventBatch(),
            job: documents.job,
            chunks: [documents.chunks[0]],
            cancellation: documents.cancellation,
            acceptedResult: documents.acceptedResult
        )

        _ = try await relay.relayEvents(using: master)

        #expect(await agent.localCodingCancellations == [documents.cancellation])
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
        let connected = authenticatedSnapshotData(connectionEpoch: 22)
        let maintenance = authenticatedSnapshotData(
            connectionEpoch: 23,
            maintenanceActive: true
        )
        let paused = authenticatedSnapshotData(
            connectionEpoch: 24,
            emergencyPaused: true,
            featureConveyor: pausedFeatureConveyorData()
        )

        let connectedStatus = try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: connected)
        let maintenanceStatus = try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: maintenance)
        let pausedStatus = try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: paused)

        #expect(connectedStatus.phase == .connected)
        #expect(connectedStatus.masterEndpoint == "100.64.23.14:7792")
        #expect(connectedStatus.connectionEpoch == 22)
        #expect(connectedStatus.featureConveyor?.ownerGuidance.state == .idle)
        #expect(maintenanceStatus.phase == .maintenance)
        #expect(maintenanceStatus.connectionEpoch == 23)
        #expect(pausedStatus.phase == .paused)
        #expect(pausedStatus.connectionEpoch == 24)
    }

    @Test("App helper snapshots reject extra keys, invalid shapes, and oversized lines")
    func appHelperSnapshotRejectsUntrustedOutput() {
        let valid = authenticatedSnapshotJSON(connectionEpoch: 22)
        let extra = Data((String(valid.dropLast()) + #", "service_identity":"forbidden"}"#).utf8)
        let contradictory = authenticatedSnapshotData(
            connectionEpoch: 22,
            maintenanceActive: true,
            masterStatus: "ok"
        )
        let duplicate = Data((String(valid.dropLast()) + #", "\u0070hase":"authenticated"}"#).utf8)
        let wrongPhase = Data(
            (#"{"phase":"backing_off","device_id":"22222222-2222-4222-8222-222222222222","master_endpoint":"100.64.23.14:7792","consecutive_failures":1,"next_delay_ms":1000,"error_code":"connection_failed","feature_conveyor":"#
                + String(data: validFeatureConveyorData(), encoding: .utf8)! + "}").utf8
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
            try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: wrongPhase)
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
        let line = authenticatedSnapshotData(connectionEpoch: 44)
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
        #expect(lifecycle.status.featureConveyor?.ownerGuidance.state == .idle)
        lifecycle.start()
        #expect(await launcher.launchCount == 1)
        await lifecycle.stop()
        #expect(await session.stopped)
        #expect(lifecycle.status.phase == .stopped)
    }

    @MainActor
    @Test("App helper lifecycle accepts projection-free output only for local-coding opt-in")
    func appHelperLifecycleBindsProjectionFreeOutputToLocalCodingOptIn() async {
        let line = localCodingAuthenticatedSnapshotData(connectionEpoch: 145)
        let session = FakeBridgeProcessSession(lines: [line])
        let launcher = FakeBridgeProcessLauncher(session: session)
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ",
                AssemblywrightDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey:
                    "/tmp/assemblywright-agent",
                AssemblywrightDeveloperBridgeProcessConfiguration.agentDataDirectoryEnvironmentKey:
                    "/tmp/assemblywright-agent-data",
                AssemblywrightDeveloperBridgeProcessConfiguration
                    .localCodingSnapshotsEnabledEnvironmentKey: "true"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher
        )

        lifecycle.start()
        for _ in 0 ..< 50 where lifecycle.status.phase != .connected {
            await Task.yield()
        }

        #expect(lifecycle.status.phase == .connected)
        #expect(lifecycle.status.connectionEpoch == 145)
        #expect(lifecycle.status.featureConveyor == nil)
        #expect(await launcher.launchCount == 1)
        await lifecycle.stop()
        #expect(await session.stopped)
    }

    @MainActor
    @Test("App helper lifecycle cleanup stops its child after cancellation")
    func appHelperLifecycleCleanupStopsAfterCancellation() async {
        let line = authenticatedSnapshotData(connectionEpoch: 44)
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
        let line = authenticatedSnapshotData(connectionEpoch: 44)
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
        let snapshot = authenticatedSnapshotJSON(connectionEpoch: 45)
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
        let snapshot = authenticatedSnapshotJSON(connectionEpoch: 46)
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
        let snapshot = authenticatedSnapshotJSON(connectionEpoch: 47)
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
        let featureConveyor = try #require(liveStatus.featureConveyor)
        #expect(
            featureConveyor.schemaVersion
                == AssemblywrightMacFeatureConveyorStatus.expectedSchemaVersion
        )
        #expect(
            featureConveyor.ownerGuidance.queueRevision
                == featureConveyor.queueRevision
        )
        #expect(lifecycle.status.phase == .stopped)
        print(
            "assemblywright_mac_app_bridge_live_e2e_ok "
                + "endpoint=\(liveStatus.masterEndpoint ?? "missing") "
                + "connection_epoch=\(liveStatus.connectionEpoch ?? 0) "
                + "feature_conveyor_schema=\(featureConveyor.schemaVersion) "
                + "feature_conveyor_queue_revision=\(featureConveyor.queueRevision) "
                + "feature_conveyor_guidance=\(featureConveyor.ownerGuidance.state.rawValue)"
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
        "protocol_version": 5,
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
        "protocol_version": 5,
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
        "protocol_version": 5,
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
        "protocol_version": 5,
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

private struct LocalCodingSnapshotDocuments {
    let deviceID: UUID
    let snapshotID: String
    let job: Data
    let chunks: [Data]
    let result: Data
    let acceptedResult: Data
    let cancellation: Data
    let cancellationAcknowledgement: Data
}

private func localCodingRelayConfiguration()
    -> AssemblywrightMacDeveloperEventRelayConfiguration
{
    AssemblywrightMacDeveloperEventRelayConfiguration(
        agentExecutableURL: URL(fileURLWithPath: "/tmp/assemblywright-agent"),
        agentDataDirectoryURL: URL(fileURLWithPath: "/tmp/assemblywright-agent-data"),
        localCodingSnapshotsEnabled: true
    )
}

private func localCodingSnapshotDocuments(
    connectionEpoch: UInt64,
    snapshotBundle: (data: Data, digest: [UInt8])? = nil,
    leaseDurationMilliseconds: UInt64 = 10_000,
    deadlineAfterMilliseconds: UInt64 = 10_000,
    replacement: Data = Data("assemblywright contained coding fixture\n".utf8)
) throws -> LocalCodingSnapshotDocuments {
    let taskID = "11111111-1111-4111-8111-111111111111"
    let stepID = "22222222-2222-4222-8222-222222222222"
    let attemptID = "33333333-3333-4333-8333-333333333333"
    let leaseID = "44444444-4444-4444-8444-444444444444"
    let cancellationID = "55555555-5555-4555-8555-555555555555"
    let deviceIDText = "66666666-6666-4666-8666-666666666666"
    let snapshotID = "77777777-7777-4777-8777-777777777777"
    let snapshotDigest = snapshotBundle?.digest ?? [UInt8](repeating: 0x22, count: 32)
    let artifactID = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
    let beforeDigest = snapshotBundle == nil
        ? [UInt8](repeating: 0x42, count: 32)
        : Array(SHA256.hash(data: Data("before contained coding fixture\n".utf8)))
    let replacementDigest = Array(SHA256.hash(data: replacement))
    let replacementHex = replacement.map { String(format: "%02x", $0) }.joined()
    let operations: [[String: Any]] = [[
        "tool_id": "file.write.v1",
        "arguments": [
            "path": "README.md", "expected_before_sha256": beforeDigest,
            "replacement_sha256": replacementDigest, "replacement_hex": replacementHex,
            "executable": false
        ]
    ]]
    let workPacket: [String: Any] = [
        "packet_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "ordinal": 1, "acceptance_criteria_count": 2,
        "allowed_paths": ["README.md"], "operations": operations
    ]
    let workPacketData = try JSONSerialization.data(withJSONObject: workPacket, options: [.sortedKeys, .withoutEscapingSlashes])
    let workPacketDigest = Array(SHA256.hash(data: workPacketData))
    let canonicalArtifact = try JSONSerialization.data(withJSONObject: [
        "format": "assemblywright.multi-file-patch.v1",
        "work_packet_sha256": workPacketDigest,
        "changes": operations
    ], options: [.sortedKeys, .withoutEscapingSlashes])
    let artifactDigest = Array(SHA256.hash(data: canonicalArtifact))
    let artifact: [String: Any] = [
        "artifact_id": artifactID,
        "artifact_sha256": artifactDigest,
        "artifact_size_bytes": canonicalArtifact.count,
        "artifact_hex": canonicalArtifact.map { String(format: "%02x", $0) }.joined()
    ]
    let context: [String: Any] = [
        "feature_id": "88888888-8888-4888-8888-888888888888",
        "specification_revision": 1,
        "lifecycle_revision": 2,
        "feature_lease_id": "99999999-9999-4999-8999-999999999999",
        "snapshot_id": snapshotID,
        "snapshot_sha256": snapshotDigest,
        "work_packet_sha256": workPacketDigest,
        "work_packet": workPacket,
        "device_id": deviceIDText,
        "device_registry_revision": 3,
        "queue_revision": 4,
        "emergency_pause_revision": 0
    ]
    let contextData = try JSONSerialization.data(
        withJSONObject: context,
        options: [.sortedKeys, .withoutEscapingSlashes]
    )
    let contextDigest = Array(SHA256.hash(data: contextData))
    let jobObject: [String: Any] = [
        "protocol_version": 5,
        "connection_epoch": connectionEpoch,
        "sequence": 10,
        "task_id": taskID,
        "step_id": stepID,
        "attempt_id": attemptID,
        "lease_id": leaseID,
        "cancellation_id": cancellationID,
        "capability_id": "local.coding.v1",
        "selected_model": "assemblywright-local-coding-v1",
        "sensitivity": "workspace",
        "context_handling": "sealed_until_resolved_or_expired",
        "lease_duration_ms": leaseDurationMilliseconds,
        "deadline_after_ms": deadlineAfterMilliseconds,
        "context_sha256": contextDigest,
        "context": context
    ]
    let job = try JSONSerialization.data(withJSONObject: jobObject, options: [.sortedKeys])

    let totalBytes = snapshotBundle.map { UInt64($0.data.count) } ?? 3
    func chunk(offset: UInt64, content: Data, complete: Bool) throws -> Data {
        try JSONSerialization.data(
            withJSONObject: [
                "protocol_version": 5,
                "connection_epoch": connectionEpoch,
                "task_id": taskID,
                "step_id": stepID,
                "attempt_id": attemptID,
                "lease_id": leaseID,
                "cancellation_id": cancellationID,
                "snapshot_id": snapshotID,
                "snapshot_sha256": snapshotDigest,
                "offset": offset,
                "total_bytes": totalBytes,
                "content_sha256": Array(SHA256.hash(data: content)),
                "content_hex": content.map { String(format: "%02x", $0) }.joined(),
                "complete": complete
            ],
            options: [.sortedKeys]
        )
    }
    let chunks: [Data]
    if let snapshotBundle {
        let maximumChunkBytes = 128 * 1_024
        var encodedChunks = [Data]()
        var offset = 0
        while offset < snapshotBundle.data.count {
            let end = min(offset + maximumChunkBytes, snapshotBundle.data.count)
            encodedChunks.append(
                try chunk(
                    offset: UInt64(offset),
                    content: snapshotBundle.data.subdata(in: offset ..< end),
                    complete: end == snapshotBundle.data.count
                )
            )
            offset = end
        }
        chunks = encodedChunks
    } else {
        chunks = [
            try chunk(offset: 0, content: Data([0xaa, 0xbb]), complete: false),
            try chunk(offset: 2, content: Data([0xcc]), complete: true)
        ]
    }

    let payload: [String: Any] = [
        "status": "contained_coding_completed",
        "work_packet_sha256": workPacketDigest,
        "admission_sha256": localCodingAdmissionDigest(
            protocolVersion: 5,
            contextDigest: contextDigest,
            taskID: UUID(uuidString: taskID)!,
            stepID: UUID(uuidString: stepID)!,
            attemptID: UUID(uuidString: attemptID)!,
            leaseID: UUID(uuidString: leaseID)!,
            cancellationID: UUID(uuidString: cancellationID)!,
            connectionEpoch: connectionEpoch,
            sequence: 10,
            leaseDurationMilliseconds: leaseDurationMilliseconds,
            deadlineAfterMilliseconds: deadlineAfterMilliseconds
        ),
        "snapshot_sha256": snapshotDigest,
        "allowed_paths_sha256": localCodingAllowedPathsDigest(),
        "changed_paths_sha256": localCodingAllowedPathsDigest(),
        "patch_sha256": artifactDigest,
        "artifact_id": artifactID,
        "artifact_sha256": artifactDigest,
        "artifact_size_bytes": canonicalArtifact.count,
        "changed_file_count": 1,
        "test_status": "not_run",
        "mutation_performed": true,
        "workspace_retained": true,
        "workspace_expires_at_ms": 4_102_444_800_000,
        "ambiguous": false
    ]
    let payloadData = try JSONSerialization.data(
        withJSONObject: payload,
        options: [.sortedKeys, .withoutEscapingSlashes]
    )
    let payloadDigest = Array(SHA256.hash(data: payloadData))
    let resultObject: [String: Any] = [
            "protocol_version": 5,
            "connection_epoch": connectionEpoch,
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
    let result = try JSONSerialization.data(
        withJSONObject: ["result": resultObject, "artifact": artifact],
        options: [.sortedKeys]
    )
    let acceptedResult = try JSONSerialization.data(
        withJSONObject: [
            "task_id": taskID,
            "step_id": stepID,
            "status": "succeeded",
            "payload_sha256": payloadDigest
        ],
        options: [.sortedKeys]
    )
    let cancellation = try JSONSerialization.data(
        withJSONObject: [
            "protocol_version": 5,
            "connection_epoch": connectionEpoch,
            "sequence": 11,
            "task_id": taskID,
            "step_id": stepID,
            "attempt_id": attemptID,
            "lease_id": leaseID,
            "cancellation_id": cancellationID,
            "deadline_after_ms": 2_000
        ],
        options: [.sortedKeys]
    )
    let cancellationAcknowledgement = try JSONSerialization.data(
        withJSONObject: [
            "protocol_version": 5,
            "connection_epoch": connectionEpoch,
            "sequence": 12,
            "task_id": taskID,
            "step_id": stepID,
            "attempt_id": attemptID,
            "lease_id": leaseID,
            "cancellation_id": cancellationID,
            "status": "cancelled"
        ],
        options: [.sortedKeys]
    )
    return LocalCodingSnapshotDocuments(
        deviceID: UUID(uuidString: deviceIDText)!,
        snapshotID: snapshotID,
        job: job,
        chunks: chunks,
        result: result,
        acceptedResult: acceptedResult,
        cancellation: cancellation,
        cancellationAcknowledgement: cancellationAcknowledgement
    )
}

private func nativeLocalCodingSnapshotBundle(
    paddingByteCount: Int = 0
) -> (data: Data, digest: [UInt8]) {
    func gitObjectID(kind: String, data: Data) -> Data {
        var framed = Data("\(kind) \(data.count)\0".utf8)
        framed.append(data)
        return Data(Insecure.SHA1.hash(data: framed))
    }

    precondition((0 ... 32 * 1_024 * 1_024).contains(paddingByteCount))
    var files = [
        (path: "README.md", content: Data("before contained coding fixture\n".utf8))
    ]
    if paddingByteCount > 0 {
        files.append(
            (path: "padding.bin", content: Data(repeating: 0x5a, count: paddingByteCount))
        )
    }
    let blobs = files.map { file in
        (path: file.path, content: file.content, id: gitObjectID(kind: "blob", data: file.content))
    }
    var tree = Data()
    for blob in blobs {
        tree.append(Data("100644 \(blob.path)\0".utf8))
        tree.append(blob.id)
    }
    let treeID = gitObjectID(kind: "tree", data: tree)
    let treeHex = treeID.map { String(format: "%02x", $0) }.joined()
    let commit = Data(
        (
            "tree \(treeHex)\n"
                + "author Assemblywright E2E <e2e@example.invalid> 0 +0000\n"
                + "committer Assemblywright E2E <e2e@example.invalid> 0 +0000\n\n"
                + "bounded native relay fixture\n"
        ).utf8
    )
    let commitID = gitObjectID(kind: "commit", data: commit)
    let commitHex = commitID.map { String(format: "%02x", $0) }.joined()

    var bundle = Data("AW-SNAPSHOT-BUNDLE-V1\n\(commitHex)".utf8)
    var objects: [(kind: UInt8, id: Data, data: Data)] = [
        (UInt8(1), commitID, commit),
        (UInt8(2), treeID, tree)
    ]
    objects.append(contentsOf: blobs.map { (UInt8(3), $0.id, $0.content) })
    for (kind, objectID, data) in objects {
        bundle.append(contentsOf: [1, kind])
        bundle.append(objectID)
        bundle.appendBigEndian(UInt64(data.count))
        bundle.append(data)
    }
    bundle.append(0)
    for blob in blobs {
        bundle.append(1)
        bundle.appendBigEndian(UInt16(blob.path.utf8.count))
        bundle.append(Data(blob.path.utf8))
        bundle.appendBigEndian(UInt32(0o100644))
        bundle.append(blob.id)
        bundle.appendBigEndian(UInt64(blob.content.count))
    }
    bundle.append(0)

    var digest = SHA256()
    digest.update(data: Data("assemblywright.repository-snapshot.v1\0".utf8))
    digest.update(data: commitID)
    for blob in blobs {
        var pathLength = Data()
        pathLength.appendBigEndian(UInt64(blob.path.utf8.count))
        digest.update(data: pathLength)
        digest.update(data: Data(blob.path.utf8))
        var mode = Data()
        mode.appendBigEndian(UInt32(0o100644))
        digest.update(data: mode)
        digest.update(data: blob.id)
        var contentLength = Data()
        contentLength.appendBigEndian(UInt64(blob.content.count))
        digest.update(data: contentLength)
        digest.update(data: blob.content)
    }
    let snapshotDigest = Array(digest.finalize())
    bundle.append(Data("AW-SNAPSHOT-END-V1\n".utf8))
    bundle.append(Data(snapshotDigest))
    return (bundle, snapshotDigest)
}

private func localCodingAllowedPathsDigest() -> [UInt8] {
    let path = Data("README.md".utf8)
    var input = Data("assemblywright.local-coding-allowed-paths.v2\0".utf8)
    input.appendBigEndian(UInt16(1))
    input.appendBigEndian(UInt64(path.count))
    input.append(path)
    return Array(SHA256.hash(data: input))
}

private func localCodingAdmissionDigest(
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
    transcript.appendBigEndian(protocolVersion)
    transcript.append(contentsOf: contextDigest)
    for identifier in [taskID, stepID, attemptID, leaseID, cancellationID] {
        var bytes = identifier.uuid
        Swift.withUnsafeBytes(of: &bytes) { transcript.append(contentsOf: $0) }
    }
    transcript.appendBigEndian(connectionEpoch)
    transcript.appendBigEndian(sequence)
    transcript.appendBigEndian(leaseDurationMilliseconds)
    transcript.appendBigEndian(deadlineAfterMilliseconds)
    return Array(SHA256.hash(data: transcript))
}

private extension Data {
    mutating func appendBigEndian<T: FixedWidthInteger>(_ value: T) {
        var encoded = value.bigEndian
        Swift.withUnsafeBytes(of: &encoded) { append(contentsOf: $0) }
    }
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
        "protocol_version": 5,
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
        "protocol_version": 5,
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
        "protocol_version": 5,
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
        "protocol_version": 5,
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
        #"{"after_sequence":0,"events":[],"has_more":false,"next_sequence":0,"protocol_version":5,"stream_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"}"#.utf8
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

private func localCodingCapability() -> AssemblywrightMacBridgeCapability {
    AssemblywrightMacBridgeCapability(
        id: "local.coding.v1",
        kind: "local_coding",
        provider: "assemblywright-agent",
        model: "assemblywright-local-coding-v1",
        maxContextBytes: 12_288,
        maxResultBytes: 32_768
    )
}

private func localCodingInvitationData() throws -> Data {
    var invitation = try #require(
        JSONSerialization.jsonObject(with: validInvitationData()) as? [String: Any]
    )
    invitation["device_name"] = "owner-mac-local-coding"
    invitation["role"] = "inference_worker"
    invitation["capabilities"] = [[
        "id": "local.coding.v1",
        "kind": "local_coding",
        "provider": "assemblywright-agent",
        "model": "assemblywright-local-coding-v1",
        "max_context_bytes": 12_288,
        "max_result_bytes": 32_768
    ]]
    return try JSONSerialization.data(withJSONObject: invitation, options: [.sortedKeys])
}

private func validIssuedReceiptData() throws -> Data {
    Data(
        #"{"status":"device_certificate_issued","operation":"enroll","device_id":"22222222-2222-4222-8222-222222222222","device_name":"owner-mac-bridge","role":"mac_bridge","registry_revision":3,"serial_hex":"01","issued_at_ms":1000,"not_after_ms":4102444800000,"certificate_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","certificate_pem":"-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n","ca_certificate_pem":"-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n"}"#.utf8
    )
}

private func localCodingIssuedReceiptData() throws -> Data {
    var receipt = try #require(
        JSONSerialization.jsonObject(with: validIssuedReceiptData()) as? [String: Any]
    )
    receipt["device_name"] = "owner-mac-local-coding"
    receipt["role"] = "inference_worker"
    return try JSONSerialization.data(withJSONObject: receipt, options: [.sortedKeys])
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

private func localCodingProfile() -> AssemblywrightMacBridgeProfile {
    AssemblywrightMacBridgeProfile(
        deviceID: "33333333-3333-4333-8333-333333333333",
        deviceName: "owner-mac-local-coding",
        role: "inference_worker",
        registryRevision: 4,
        capabilities: [localCodingCapability()],
        masterEndpoint: "100.64.23.14:7792",
        certificateNotAfterMilliseconds: 4_102_444_800_000
    )
}

private func validRemoteHealthData(schemaVersion: UInt64 = 8) -> Data {
    Data(
        #"{"status":"ok","mode":"developer_remote_master","host_mode":"windows_service","service_identity":"MIKE-PC\\mike","maintenance_active":false,"maintenance_reason":null,"emergency_paused":false,"protocol_version":5,"schema_version":\#(schemaVersion),"process_id":43752,"started_at_ms":1784749559000,"startup_reconciliation":{"disconnected_connections":0,"abandoned_attempts":0,"requeued_steps":0},"state":{"registered_devices":1,"active_device_certificates":1,"unconsumed_enrollment_grants":2,"active_connections":1,"queued_steps":0,"leased_steps":0,"terminal_steps":0,"active_attempts":0},"boundary":"TLS 1.3 mutual authentication with enrolled-device certificate and durable revocation checks"}"#.utf8
    )
}

private func pausedRemoteHealthData() -> Data {
    Data(
        #"{"status":"paused","mode":"developer_remote_master","host_mode":"windows_service","service_identity":"MIKE-PC\\mike","maintenance_active":false,"maintenance_reason":null,"emergency_paused":true,"protocol_version":5,"schema_version":18,"process_id":43752,"started_at_ms":1784749559000,"startup_reconciliation":{"disconnected_connections":0,"abandoned_attempts":0,"requeued_steps":0},"state":{"registered_devices":1,"active_device_certificates":1,"unconsumed_enrollment_grants":2,"active_connections":1,"queued_steps":0,"leased_steps":1,"terminal_steps":0,"active_attempts":1},"boundary":"TLS 1.3 mutual authentication with enrolled-device certificate and durable revocation checks"}"#.utf8
    )
}

private func validFeatureConveyorData() -> Data {
    Data(
        #"{"schema_version":9,"queue_revision":0,"startup_quarantine_count":0,"counts_by_status":{"queued":0,"implementing":0,"validating":0,"reviewing":0,"publishing":0,"verifying_main":0,"repairing":0,"paused":0,"attention_required":0,"failed":0,"succeeded":0,"cancelled":0,"abandoned":0,"quarantined":0},"visible_feature_count":0,"features_truncated":false,"features":[],"owner_guidance":{"state":"idle","reason_code":"queue_empty","next_owner_action":"prepare_approved_feature","feature_id":null,"specification_revision":null,"lifecycle_revision":null,"queue_revision":0,"emergency_pause_revision":0}}"#.utf8
    )
}

private func readyFeatureConveyorData() -> Data {
    Data(
        #"{"schema_version":9,"queue_revision":1,"startup_quarantine_count":0,"counts_by_status":{"queued":1,"implementing":0,"validating":0,"reviewing":0,"publishing":0,"verifying_main":0,"repairing":0,"paused":0,"attention_required":0,"failed":0,"succeeded":0,"cancelled":0,"abandoned":0,"quarantined":0},"visible_feature_count":1,"features_truncated":false,"features":[{"feature_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","specification_revision":1,"lifecycle_revision":1,"queue_position":1,"status":"queued","lease_present":false,"effect_possible":false}],"owner_guidance":{"state":"ready","reason_code":"head_dependency_satisfied","next_owner_action":"await_owner_control_surface","feature_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","specification_revision":1,"lifecycle_revision":1,"queue_revision":1,"emergency_pause_revision":0}}"#.utf8
    )
}

private func reconciliationFeatureConveyorData() -> Data {
    Data(
        #"{"schema_version":9,"queue_revision":2,"startup_quarantine_count":0,"counts_by_status":{"queued":0,"implementing":0,"validating":0,"reviewing":0,"publishing":0,"verifying_main":0,"repairing":0,"paused":0,"attention_required":0,"failed":0,"succeeded":0,"cancelled":1,"abandoned":0,"quarantined":0},"visible_feature_count":1,"features_truncated":false,"features":[{"feature_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","specification_revision":1,"lifecycle_revision":3,"queue_position":1,"status":"cancelled","lease_present":true,"effect_possible":true}],"owner_guidance":{"state":"blocked","reason_code":"active_requires_reconciliation","next_owner_action":"reconcile_active_feature","feature_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","specification_revision":1,"lifecycle_revision":3,"queue_revision":2,"emergency_pause_revision":0}}"#.utf8
    )
}

private func maximumFeatureConveyorData() -> Data {
    let features: [[String: Any]] = (1 ... 100).map { index in
        [
            "feature_id": String(
                format: "%08x-0000-4000-8000-%012x",
                index,
                index
            ),
            "specification_revision": 1,
            "lifecycle_revision": 1,
            "queue_position": index,
            "status": "queued",
            "lease_present": false,
            "effect_possible": false
        ]
    }
    let firstID = features[0]["feature_id"]!
    let object: [String: Any] = [
        "schema_version": 9,
        "queue_revision": 100,
        "startup_quarantine_count": 0,
        "counts_by_status": [
            "queued": 100,
            "implementing": 0,
            "validating": 0,
            "reviewing": 0,
            "publishing": 0,
            "verifying_main": 0,
            "repairing": 0,
            "paused": 0,
            "attention_required": 0,
            "failed": 0,
            "succeeded": 0,
            "cancelled": 0,
            "abandoned": 0,
            "quarantined": 0
        ],
        "visible_feature_count": 100,
        "features_truncated": false,
        "features": features,
        "owner_guidance": [
            "state": "ready",
            "reason_code": "head_dependency_satisfied",
            "next_owner_action": "await_owner_control_surface",
            "feature_id": firstID,
            "specification_revision": 1,
            "lifecycle_revision": 1,
            "queue_revision": 100,
            "emergency_pause_revision": 0
        ]
    ]
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func truncatedFeatureConveyorData() -> Data {
    let activeID = "ffffffff-ffff-4fff-8fff-ffffffffffff"
    var features: [[String: Any]] = [[
        "feature_id": activeID,
        "specification_revision": 1,
        "lifecycle_revision": 3,
        "queue_position": 1,
        "status": "cancelled",
        "lease_present": true,
        "effect_possible": true
    ]]
    features.append(contentsOf: (2 ... 100).map { index in
        [
            "feature_id": String(
                format: "%08x-0000-4000-8000-%012x",
                index,
                index
            ),
            "specification_revision": 1,
            "lifecycle_revision": 1,
            "queue_position": index,
            "status": "queued",
            "lease_present": false,
            "effect_possible": false
        ]
    })
    let object: [String: Any] = [
        "schema_version": 9,
        "queue_revision": 101,
        "startup_quarantine_count": 0,
        "counts_by_status": [
            "queued": 100,
            "implementing": 0,
            "validating": 0,
            "reviewing": 0,
            "publishing": 0,
            "verifying_main": 0,
            "repairing": 0,
            "paused": 0,
            "attention_required": 0,
            "failed": 0,
            "succeeded": 0,
            "cancelled": 1,
            "abandoned": 0,
            "quarantined": 0
        ],
        "visible_feature_count": 101,
        "features_truncated": true,
        "features": features,
        "owner_guidance": [
            "state": "blocked",
            "reason_code": "active_requires_reconciliation",
            "next_owner_action": "reconcile_active_feature",
            "feature_id": activeID,
            "specification_revision": 1,
            "lifecycle_revision": 3,
            "queue_revision": 101,
            "emergency_pause_revision": 0
        ]
    ]
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func pausedFeatureConveyorData() -> Data {
    Data(
        #"{"schema_version":9,"queue_revision":0,"startup_quarantine_count":0,"counts_by_status":{"queued":0,"implementing":0,"validating":0,"reviewing":0,"publishing":0,"verifying_main":0,"repairing":0,"paused":0,"attention_required":0,"failed":0,"succeeded":0,"cancelled":0,"abandoned":0,"quarantined":0},"visible_feature_count":0,"features_truncated":false,"features":[],"owner_guidance":{"state":"blocked","reason_code":"emergency_paused","next_owner_action":"resume_emergency_pause","feature_id":null,"specification_revision":null,"lifecycle_revision":null,"queue_revision":0,"emergency_pause_revision":1}}"#.utf8
    )
}

private func authenticatedSnapshotData(
    connectionEpoch: UInt64,
    maintenanceActive: Bool = false,
    emergencyPaused: Bool = false,
    masterStatus: String? = nil,
    featureConveyor: Data = validFeatureConveyorData()
) -> Data {
    let featureObject = try! JSONSerialization.jsonObject(with: featureConveyor)
    let object: [String: Any] = [
        "phase": "authenticated",
        "device_id": "22222222-2222-4222-8222-222222222222",
        "master_endpoint": "100.64.23.14:7792",
        "connection_epoch": connectionEpoch,
        "consecutive_failures": 0,
        "next_delay_ms": 5_000,
        "master_status": masterStatus
            ?? (maintenanceActive ? "maintenance" : emergencyPaused ? "paused" : "ok"),
        "maintenance_active": maintenanceActive,
        "emergency_paused": emergencyPaused,
        "protocol_version": 5,
        "schema_version": 18,
        "feature_conveyor": featureObject
    ]
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func authenticatedSnapshotJSON(connectionEpoch: UInt64) -> String {
    String(data: authenticatedSnapshotData(connectionEpoch: connectionEpoch), encoding: .utf8)!
}

private func localCodingAuthenticatedSnapshotData(connectionEpoch: UInt64) -> Data {
    var object = try! JSONSerialization.jsonObject(
        with: authenticatedSnapshotData(connectionEpoch: connectionEpoch)
    ) as! [String: Any]
    object.removeValue(forKey: "feature_conveyor")
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func approvedFeatureOwnerControlRequestData() -> Data {
    let manifest: [String: Any] = [
        "acceptance": ["owner_control_transport_only"],
        "title": "Bounded owner control"
    ]
    let canonicalManifest = try! JSONSerialization.data(
        withJSONObject: manifest,
        options: [.sortedKeys, .withoutEscapingSlashes]
    )
    let digest = Array(SHA256.hash(data: canonicalManifest))
    let object: [String: Any] = [
        "schema_version": 1,
        "expected_queue_revision": 0,
        "owner_control_designation_revision": 3,
        "emergency_pause_revision": 0,
        "specification": [
            "feature_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "revision": 1,
            "repository_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            "manifest": manifest,
            "manifest_sha256": digest,
            "design_sha256": Array(repeating: UInt8(0x22), count: 32),
            "brainstorming_sha256": Array(repeating: UInt8(0x33), count: 32),
            "owner_approval_sha256": Array(repeating: UInt8(0x44), count: 32),
            "grants": [
                "registration": 1,
                "cloud_disclosure": 2,
                "autonomous_publication": 3
            ],
            "provider_id": "local-owner-planner",
            "model_id": "owner-approved-v1",
            "dependencies": []
        ]
    ]
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func numericManifestApprovedFeatureOwnerControlRequestData() -> Data {
    let canonicalManifest = Data(#"{"ratio":1.0}"#.utf8)
    let digest = Array(SHA256.hash(data: canonicalManifest))
        .map(String.init)
        .joined(separator: ",")
    return Data(
        """
        {"schema_version":1,"expected_queue_revision":0,"owner_control_designation_revision":3,"emergency_pause_revision":0,"specification":{"feature_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","revision":1,"repository_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","manifest":{"ratio":1.0},"manifest_sha256":[\(digest)],"design_sha256":[\(Array(repeating: "34", count: 32).joined(separator: ","))],"brainstorming_sha256":[\(Array(repeating: "51", count: 32).joined(separator: ","))],"owner_approval_sha256":[\(Array(repeating: "68", count: 32).joined(separator: ","))],"grants":{"registration":1,"cloud_disclosure":2,"autonomous_publication":3},"provider_id":"local-owner-planner","model_id":"owner-approved-v1","dependencies":[]}}
        """.utf8
    )
}

private func tamperedApprovedFeatureOwnerControlRequestData() -> Data {
    var object = try! JSONSerialization.jsonObject(
        with: approvedFeatureOwnerControlRequestData()
    ) as! [String: Any]
    var specification = object["specification"] as! [String: Any]
    var manifest = specification["manifest"] as! [String: Any]
    manifest["title"] = "tampered after approval"
    specification["manifest"] = manifest
    object["specification"] = specification
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func selfDependentApprovedFeatureOwnerControlRequestData() -> Data {
    var object = try! JSONSerialization.jsonObject(
        with: approvedFeatureOwnerControlRequestData()
    ) as! [String: Any]
    var specification = object["specification"] as! [String: Any]
    specification["dependencies"] = [specification["feature_id"]!]
    object["specification"] = specification
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func approvedFeatureOwnerControlReceiptData(queueRevision: UInt64 = 1) -> Data {
    let object: [String: Any] = [
        "schema_version": 1,
        "feature_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "specification_revision": 1,
        "lifecycle_revision": 1,
        "queue_revision": queueRevision,
        "owner_control_designation_revision": 3,
        "emergency_pause_revision": 0,
        "status": "queued"
    ]
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}
