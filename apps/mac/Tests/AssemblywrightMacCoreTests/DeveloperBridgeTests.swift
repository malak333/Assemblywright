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
    var stagedRotation: AssemblywrightMacEnrollmentInvitation?
    var installedRotation: AssemblywrightMacIssuedDeviceCertificate?
    var rotationInstallCount = 0
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

    func stageRotationIdentity(
        for invitation: AssemblywrightMacEnrollmentInvitation
    ) throws -> AssemblywrightMacEnrollmentCSR {
        stagedRotation = invitation
        return AssemblywrightMacEnrollmentCSR(
            schemaVersion: 1,
            status: "enrollment_csr_ready",
            grantID: invitation.grantID,
            deviceID: invitation.deviceID,
            csrPEM: csrPEM
        )
    }

    func loadStagedRotationInvitation() throws -> AssemblywrightMacEnrollmentInvitation? {
        stagedRotation
    }

    func installRotation(
        _ receipt: AssemblywrightMacIssuedDeviceCertificate
    ) throws -> AssemblywrightMacBridgeProfile {
        if stagedRotation == nil {
            guard installedRotation == receipt, let installedProfile else {
                throw AssemblywrightMacDeveloperBridgeError.noStagedEnrollment
            }
            return installedProfile
        }
        guard let invitation = stagedRotation else {
            throw AssemblywrightMacDeveloperBridgeError.noStagedEnrollment
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
        installedRotation = receipt
        installedProfile = profile
        stagedRotation = nil
        rotationInstallCount += 1
        return profile
    }

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
    private let assemblyLineOutcome: FakeSupervisorOutcome?
    private(set) var requests: [AssemblywrightMacBridgeHTTPRequest] = []
    private(set) var cancelled = false

    init(
        connectionEpoch: UInt64,
        outcomes: [FakeSupervisorOutcome],
        featureConveyorOutcome: FakeSupervisorOutcome = .response(
            .init(status: 200, body: validFeatureConveyorData())
        ),
        assemblyLineOutcome: FakeSupervisorOutcome? = nil
    ) {
        self.connectionEpoch = connectionEpoch
        self.outcomes = outcomes
        self.featureConveyorOutcome = featureConveyorOutcome
        self.assemblyLineOutcome = assemblyLineOutcome
    }

    func send(_ request: AssemblywrightMacBridgeHTTPRequest) async throws -> AssemblywrightMacBridgeHTTPResponse {
        requests.append(request)
        if request.path == AssemblywrightMacBridgeSupervisor.featureConveyorPath {
            switch featureConveyorOutcome {
            case let .response(response): return response
            case .failure: throw FakeSupervisorError()
            }
        }
        if request.path == AssemblywrightMacBridgeSupervisor.ownerControlPath {
            switch featureConveyorOutcome {
            case let .response(response):
                let feature = (try? JSONSerialization.jsonObject(with: response.body)) as? [String: Any]
                let guidance = feature?["owner_guidance"] as? [String: Any]
                return .init(status: 200, body: ownerControlData(
                    queueRevision: feature?["queue_revision"] as? UInt64 ?? 0,
                    emergencyPaused: guidance?["reason_code"] as? String == "emergency_paused",
                    emergencyPauseRevision: guidance?["emergency_pause_revision"] as? UInt64 ?? 0
                ))
            case .failure: throw FakeSupervisorError()
            }
        }
        if request.path == AssemblywrightMacBridgeSupervisor.assemblyLinePath {
            if let assemblyLineOutcome {
                switch assemblyLineOutcome {
                case let .response(response): return response
                case .failure: throw FakeSupervisorError()
                }
            }
            switch featureConveyorOutcome {
            case let .response(response):
                let feature = (try? JSONSerialization.jsonObject(with: response.body)) as? [String: Any]
                let guidance = feature?["owner_guidance"] as? [String: Any]
                return .init(
                    status: 200,
                    body: validAssemblyLineProjectionData(
                        emergencyPaused: guidance?["reason_code"] as? String == "emergency_paused",
                        emergencyPauseRevision: guidance?["emergency_pause_revision"] as? UInt64 ?? 0
                    )
                )
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

private actor FakeAssemblyLinePlanningSession: AssemblywrightMacBridgeSession {
    nonisolated let connectionEpoch: UInt64 = 77
    private var responses: [AssemblywrightMacBridgeHTTPResponse]
    private(set) var requests: [AssemblywrightMacBridgeHTTPRequest] = []
    private(set) var cancelled = false

    init(responses: [AssemblywrightMacBridgeHTTPResponse]) {
        self.responses = responses
    }

    func send(_ request: AssemblywrightMacBridgeHTTPRequest) async throws
        -> AssemblywrightMacBridgeHTTPResponse
    {
        requests.append(request)
        guard !responses.isEmpty else { throw FakeSupervisorError() }
        return responses.removeFirst()
    }

    func cancel() async { cancelled = true }
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
    let restartSession: FakeBridgeProcessSession?
    let commandSucceeds: Bool
    private var commandDelays: [Duration]
    private var commandFailuresRemaining: Int
    private var commandErrors: [AssemblywrightDeveloperBridgeProcessError]
    private var commandResponses: [Data]
    private(set) var launchCount = 0
    private(set) var commands: [([String], Data)] = []
    private(set) var commandSawStoppedMonitor = false

    init(
        session: FakeBridgeProcessSession,
        restartSession: FakeBridgeProcessSession? = nil,
        commandSucceeds: Bool = true,
        commandResponse: Data? = nil,
        commandResponses: [Data] = [],
        commandFailures: Int = 0,
        commandErrors: [AssemblywrightDeveloperBridgeProcessError] = [],
        commandDelay: Duration = .zero,
        commandDelays: [Duration] = []
    ) {
        self.session = session
        self.restartSession = restartSession
        self.commandSucceeds = commandSucceeds
        commandFailuresRemaining = commandFailures
        self.commandErrors = commandErrors
        self.commandResponses = commandResponse.map { [$0] } ?? commandResponses
        self.commandDelays = commandDelay > .zero ? [commandDelay] : commandDelays
    }

    func launch(
        executable _: AssemblywrightDeveloperBridgeValidatedExecutable,
        eventRelayConfiguration _: AssemblywrightMacDeveloperEventRelayConfiguration?
    ) async throws -> any AssemblywrightDeveloperBridgeProcessSession {
        launchCount += 1
        return launchCount > 1 ? (restartSession ?? session) : session
    }

    func runCommand(
        executable _: AssemblywrightDeveloperBridgeValidatedExecutable,
        arguments: [String], input: Data
    ) async throws -> Data {
        commandSawStoppedMonitor = await session.stopped
        commands.append((arguments, input))
        if !commandDelays.isEmpty {
            let delay = commandDelays.removeFirst()
            if delay > .zero { try await Task.sleep(for: delay) }
        }
        if commandFailuresRemaining > 0 {
            commandFailuresRemaining -= 1
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        if !commandErrors.isEmpty { throw commandErrors.removeFirst() }
        guard commandSucceeds else { throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot }
        if !commandResponses.isEmpty { return commandResponses.removeFirst() }
        if arguments == AssemblywrightDeveloperBridgeProcessLifecycle.approvedFeatureEnqueueArguments {
            return approvedFeatureAuthoringReceiptData(request: input)
        }
        return activationReceiptData(request: input)
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

    @Test("Certificate rotation preserves the exact standard registration and is retry safe")
    func certificateRotationPreservesRegistrationAndRetriesExactly() throws {
        let store = FakeBridgeIdentityStore()
        store.installedProfile = sampleProfile()
        let coordinator = AssemblywrightMacEnrollmentCoordinator(identityStore: store)

        let reply = try coordinator.prepareRotation(invitationData: validInvitationData())
        let replyText = try #require(String(data: reply, encoding: .utf8))
        #expect(!replyText.contains("grant_secret"))
        #expect(store.stagedRotation?.deviceID == sampleProfile().deviceID)
        #expect(store.stagedRotation?.capabilities == sampleProfile().capabilities)

        let receipt = try rotationIssuedReceiptData()
        let rotated = try coordinator.installRotation(issuedReceiptData: receipt)
        #expect(rotated.deviceID == sampleProfile().deviceID)
        #expect(rotated.deviceName == sampleProfile().deviceName)
        #expect(rotated.registryRevision == sampleProfile().registryRevision)
        #expect(rotated.capabilities == sampleProfile().capabilities)
        #expect(rotated.masterEndpoint == sampleProfile().masterEndpoint)
        #expect(store.stagedRotation == nil)
        #expect(store.rotationInstallCount == 1)

        let exactRetry = try coordinator.installRotation(issuedReceiptData: receipt)
        #expect(exactRetry == rotated)
        #expect(store.rotationInstallCount == 1)
    }

    @Test("Certificate rotation rejects fixture, drift, cross-profile, and non-rotation receipts")
    func certificateRotationRejectsUnsafeBindings() throws {
        let store = FakeBridgeIdentityStore()
        store.installedProfile = sampleProfile()
        let coordinator = AssemblywrightMacEnrollmentCoordinator(identityStore: store)

        var drifted = try #require(
            JSONSerialization.jsonObject(with: validInvitationData()) as? [String: Any]
        )
        drifted["registry_revision"] = 4
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try coordinator.prepareRotation(
                invitationData: try JSONSerialization.data(withJSONObject: drifted)
            )
        }
        #expect(store.stagedRotation == nil)

        var fixture = try #require(
            JSONSerialization.jsonObject(with: validInvitationData()) as? [String: Any]
        )
        fixture["capabilities"] = [[
            "id": "fixture.reasoning", "kind": "local_inference",
            "provider": "assemblywright-fixture", "model": "assemblywright-fixture-v1",
            "max_context_bytes": 8_192, "max_result_bytes": 8_192
        ]]
        store.installedProfile = staleFixtureProfile()
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try coordinator.prepareRotation(
                invitationData: try JSONSerialization.data(withJSONObject: fixture)
            )
        }

        let fixtureCoordinator = AssemblywrightMacEnrollmentCoordinator(
            identityStore: store,
            identityProfile: .fixtureReasoning
        )
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try fixtureCoordinator.prepareRotation(
                invitationData: try JSONSerialization.data(withJSONObject: fixture)
            )
        }

        store.installedProfile = sampleProfile()
        _ = try coordinator.prepareRotation(invitationData: validInvitationData())
        var historical = try #require(
            JSONSerialization.jsonObject(with: rotationIssuedReceiptData()) as? [String: Any]
        )
        historical["grant_id"] = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
        #expect(throws: AssemblywrightMacDeveloperBridgeError.bindingMismatch) {
            _ = try coordinator.installRotation(
                issuedReceiptData: try JSONSerialization.data(withJSONObject: historical)
            )
        }
        #expect(store.installedRotation == nil)
        #expect(throws: AssemblywrightMacDeveloperBridgeError.invalidDocument) {
            _ = try coordinator.installRotation(issuedReceiptData: validIssuedReceiptData())
        }
        #expect(store.installedRotation == nil)
        #expect(store.stagedRotation != nil)
    }

    @Test("Certificate rotation follows only the installed legacy or promoted replacement slot")
    func certificateRotationSelectsCurrentKeyGeneration() throws {
        #expect(try !assemblywrightRotationUsesReplacementSlot(nil))
        #expect(try assemblywrightRotationUsesReplacementSlot("replacement_v1"))
        #expect(throws: AssemblywrightMacDeveloperBridgeError.identityUnavailable) {
            _ = try assemblywrightRotationUsesReplacementSlot("unknown")
        }
        #expect(!assemblywrightRotationStageMayBeReplaced(
            expiresAtMilliseconds: 2_001,
            nowMilliseconds: 2_000
        ))
        #expect(assemblywrightRotationStageMayBeReplaced(
            expiresAtMilliseconds: 2_000,
            nowMilliseconds: 2_000
        ))
    }

    @Test("Every certificate rotation mutation boundary retains exact forward recovery")
    func certificateRotationMutationBoundariesRemainRecoverable() throws {
        for boundary in AssemblywrightRotationMutationBoundary.allCases {
            #expect(throws: AssemblywrightMacDeveloperBridgeError.identityUnavailable) {
                try assemblywrightRotationCheckpoint(boundary) { reached in
                    return reached == boundary
                }
            }

            let state = assemblywrightRotationRecoveryState(after: boundary)
            #expect(
                state.selectedOldPresent || state.selectedNewPresent || state.candidatePresent,
                "rotation boundary \(boundary) lost every certificate copy"
            )
            if !state.selectedOldPresent && !state.installedRecordRotated {
                #expect(state.candidatePresent || state.selectedNewPresent)
                #expect(state.stagedReceiptPresent)
                #expect(state.stagePresent)
            }
            if state.installedRecordRotated {
                #expect(state.selectedNewPresent)
            }
            #expect(
                !(state.candidatePresent && state.selectedNewPresent),
                "rotation boundary \(boundary) modeled duplicate certificate DER items"
            )
            #expect(
                state.candidatePresent || state.selectedNewPresent,
                "rotation boundary \(boundary) lost the new certificate DER item"
            )
        }
        let missingOldWithCandidate = assemblywrightRotationRecoveryState(
            after: .selectedOldDeleted
        )
        #expect(!missingOldWithCandidate.selectedOldPresent)
        #expect(missingOldWithCandidate.candidatePresent)
        #expect(missingOldWithCandidate.stagedReceiptPresent)
        #expect(!assemblywrightRotationMayDeleteSelectedOld(candidatePresent: false))
        #expect(assemblywrightRotationMayDeleteSelectedOld(candidatePresent: true))
        let promoted = assemblywrightRotationRecoveryState(after: .candidatePromoted)
        #expect(!promoted.candidatePresent)
        #expect(promoted.selectedNewPresent)
    }

    @Test("Certificate rotation queries enumerate labels and mutate only one persistent item")
    func certificateRotationQueriesAreExact() {
        let lookup = assemblywrightCertificateLabelLookupQuery(label: "rotation-candidate")
        #expect(lookup[kSecAttrLabel as String] as? String == "rotation-candidate")
        #expect(lookup[kSecReturnRef as String] as? Bool == true)
        #expect(CFEqual(
            lookup[kSecMatchLimit as String] as CFTypeRef?,
            kSecMatchLimitAll
        ))
        #expect(assemblywrightCertificateLabelMultiplicityIsValid(1))
        #expect(!assemblywrightCertificateLabelMultiplicityIsValid(0))
        #expect(!assemblywrightCertificateLabelMultiplicityIsValid(2))

        let persistentReference = Data([0x01, 0x02, 0x03])
        let mutation = assemblywrightCertificateMutationQuery(
            persistentReference: persistentReference
        )
        #expect(mutation[kSecValuePersistentRef as String] as? Data == persistentReference)
        #expect(mutation[kSecAttrLabel as String] == nil)
        #expect(mutation[kSecMatchLimit as String] == nil)
        #expect(Set(mutation.keys) == Set([
            kSecClass as String,
            kSecValuePersistentRef as String,
            kSecUseDataProtectionKeychain as String
        ]))
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
            AssemblywrightMacBridgeSupervisor.ownerControlPath,
            AssemblywrightMacBridgeSupervisor.assemblyLinePath,
            AssemblywrightMacBridgeSupervisor.healthPath,
            AssemblywrightMacBridgeSupervisor.featureConveyorPath,
            AssemblywrightMacBridgeSupervisor.ownerControlPath,
            AssemblywrightMacBridgeSupervisor.assemblyLinePath
        ])
        #expect(await session.cancelled == false)
        await supervisor.stop()
        #expect(await session.cancelled)
    }

    @Test("Signed-helper snapshot with admitted evidence reaches the production app lifecycle")
    func admittedEvidenceSnapshotReachesAppLifecycle() throws {
        let featureConveyor = try JSONDecoder().decode(
            AssemblywrightMacFeatureConveyorStatus.self,
            from: validFeatureConveyorData()
        )
        let ownerControl = try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
            ownerControlDataWithCanonicalAlphabeticEvidence(queueRevision: 0)
        )
        let snapshot = AssemblywrightMacBridgeSupervisorSnapshot(
            phase: .authenticated,
            deviceID: "33333333-3333-4333-8333-333333333333",
            masterEndpoint: "100.64.23.14:7792",
            connectionEpoch: 42,
            consecutiveFailures: 0,
            nextDelayMilliseconds: 5_000,
            masterStatus: "ok",
            maintenanceActive: false,
            emergencyPaused: false,
            protocolVersion: 5,
            schemaVersion: 19,
            featureConveyor: featureConveyor,
            ownerControl: ownerControl,
            assemblyLine: try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
                validAssemblyLineProjectionData()
            ),
            localModelSelection: nil,
            errorCode: nil
        )

        let helperLine = try JSONEncoder().encode(snapshot)
        let appStatus = try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: helperLine)
        let helperText = try #require(String(data: helperLine, encoding: .utf8))

        #expect(appStatus.phase == .connected)
        #expect(appStatus.connectionEpoch == 42)
        #expect(appStatus.featureConveyor?.ownerGuidance.nextOwnerAction == .prepareApprovedFeature)
        #expect(appStatus.ownerControl?.evidence.readyCount == 6)
        #expect(helperText.contains("abcdef01-abcd-4abc-8abc-abcdefabcdef"))
        #expect(!helperText.contains("ABCDEF01-ABCD-4ABC-8ABC-ABCDEFABCDEF"))
    }

    @Test("Signed-helper snapshots canonicalize queued feature UUIDs to lowercase")
    func signedHelperCanonicalizesQueuedFeatureUUIDs() throws {
        let featureConveyor = try JSONDecoder().decode(
            AssemblywrightMacFeatureConveyorStatus.self,
            from: readyFeatureConveyorData()
        )
        let ownerControl = try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
            ownerControlData(queueRevision: 1, completeEvidence: true)
        )
        let snapshot = AssemblywrightMacBridgeSupervisorSnapshot(
            phase: .authenticated,
            deviceID: "33333333-3333-4333-8333-333333333333",
            masterEndpoint: "100.64.23.14:7792",
            connectionEpoch: 43,
            consecutiveFailures: 0,
            nextDelayMilliseconds: 5_000,
            masterStatus: "ok",
            maintenanceActive: false,
            emergencyPaused: false,
            protocolVersion: 5,
            schemaVersion: 19,
            featureConveyor: featureConveyor,
            ownerControl: ownerControl,
            assemblyLine: try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
                validAssemblyLineProjectionData()
            ),
            localModelSelection: nil,
            errorCode: nil
        )

        let helperLine = try JSONEncoder().encode(snapshot)
        let decoded = try AssemblywrightMacBridgeSupervisorSnapshot.decodeStrict(helperLine)
        let helperText = try #require(String(data: helperLine, encoding: .utf8))

        #expect(decoded == snapshot)
        #expect(helperText.contains("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"))
        #expect(!helperText.contains("AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA"))
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
            AssemblywrightMacBridgeSupervisor.featureConveyorPath,
            AssemblywrightMacBridgeSupervisor.ownerControlPath,
            AssemblywrightMacBridgeSupervisor.assemblyLinePath
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

    @Test("Approved-feature authoring emits deterministic canonical typed request bytes")
    func approvedFeatureAuthoringCanonicalRequestFixture() throws {
        let draft = sampleApprovedFeatureDraft()
        let status = try approvedFeatureAuthoringStatus()
        let canonical = try draft.canonicalManifestData()
        let request = try draft.encodeRequest(from: status)
        let requestAgain = try draft.encodeRequest(from: status)

        #expect(request == requestAgain)
        #expect(String(data: canonical, encoding: .utf8) ==
            #"{"acceptance":["acceptance_1"],"allowed_paths":["Sources/App.swift"],"assumptions":[],"decisions":[],"documentation_obligations":[],"e2e_scenarios":[],"knowledge_base_obligations":[],"non_goals":[],"outcome":"Ship bounded owner feature","prohibited_data":[],"publication_checks":[],"required_capabilities":[],"risks":[],"unit_test_obligations":[],"validation_gate":{"command_ids":["requirements_binding","coverage","focused_unit_tests","native_e2e","documentation","knowledge_base","formatting","lint","build","safety","changed_paths","secret_scan","repository_validation"],"schema_version":1}}"#)
        let object = try #require(
            JSONSerialization.jsonObject(with: request) as? [String: Any]
        )
        let specification = try #require(object["specification"] as? [String: Any])
        #expect(object["expected_queue_revision"] as? Int == 0)
        #expect(object["owner_control_designation_revision"] as? Int == 1)
        #expect(object["emergency_pause_revision"] as? Int == 0)
        #expect(specification["feature_id"] as? String
            == "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
        #expect(specification["repository_id"] as? String
            == "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")
        #expect((specification["manifest_sha256"] as? [NSNumber])?.map(\.uint8Value)
            == Array(SHA256.hash(data: canonical)))
        #expect(request.count <= AssemblywrightMacFeatureConveyorApprovedFeatureDraft.maximumRequestBytes)
    }

    @Test("Approved-feature authoring enforces empty and exact field boundaries")
    func approvedFeatureAuthoringRejectsEmptyAndOutOfBoundsFields() throws {
        let status = try approvedFeatureAuthoringStatus()
        let validBoundary = sampleApprovedFeatureDraft(manifest: .init(
            acceptance: [String(repeating: "a", count: 128)],
            outcome: String(repeating: "o", count: 4_096)
        ))
        _ = try validBoundary.encodeRequest(from: status)

        let invalid = [
            sampleApprovedFeatureDraft(manifest: .init(acceptance: [], outcome: "outcome")),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: [String(repeating: "a", count: 129)], outcome: "outcome"
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"], outcome: String(repeating: "o", count: 4_097)
            )),
            sampleApprovedFeatureDraft(designSHA256: Array(repeating: 0, count: 32))
        ]
        for draft in invalid {
            #expect(throws: AssemblywrightMacApprovedFeatureAuthoringError.invalidDraft) {
                try draft.encodeRequest(from: status)
            }
        }
    }

    @Test("Approved-feature authoring rejects secret-shaped disclosure and dependency ambiguity")
    func approvedFeatureAuthoringRejectsSensitiveAndInvalidDependencies() throws {
        let status = try approvedFeatureAuthoringStatus()
        let featureID = UUID(uuidString: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")!
        let dependency = UUID(uuidString: "cccccccc-cccc-4ccc-8ccc-cccccccccccc")!
        let invalid = [
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"], outcome: "Bearer forbidden-owner-token"
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"], outcome: "ghp_12345678901234567890"
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["ghp_12345678901234567890"], outcome: "outcome"
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"],
                outcome: "Never include prefix ghp_12345678901234567890 in evidence"
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"],
                outcome: "redacted value was bearer embedded-secret-value"
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"],
                outcome: "authorization used BaSiC dXNlcjpwYXNzd29yZA=="
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"],
                outcome: "credential is sk-12345678901234567 inside text"
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"],
                outcome: "prefix -----begin private key----- suffix"
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"],
                outcome: "embedded github_pat_11AA22BB33CC44DD55EE66FF token"
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"],
                outcome: "access key AKIA1234567890ABCDEF12 here"
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"],
                outcome: "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.signature1234 here"
            )),
            sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"],
                outcome: "endpoint https://owner:password@example.invalid/path"
            )),
            sampleApprovedFeatureDraft(providerID: "local.review"),
            sampleApprovedFeatureDraft(modelID: "gpt-5.5"),
            sampleApprovedFeatureDraft(dependencies: [featureID]),
            sampleApprovedFeatureDraft(dependencies: [dependency, dependency])
        ]
        for draft in invalid {
            #expect(throws: AssemblywrightMacApprovedFeatureAuthoringError.invalidDraft) {
                try draft.encodeRequest(from: status)
            }
        }
    }

    @Test("Approved-feature authoring permits noncredential security prose")
    func approvedFeatureAuthoringAllowsNoncredentialSecurityProse() throws {
        let status = try approvedFeatureAuthoringStatus()
        for outcome in [
            "Use token-free authorization metadata",
            "The basic-authentication lane remains disabled",
            "The bearer-token header must be redacted",
            "Reject the literal short example sk-example",
            "Reject AWS access-key identifiers without including one"
        ] {
            _ = try sampleApprovedFeatureDraft(manifest: .init(
                acceptance: ["acceptance"], outcome: outcome
            )).encodeRequest(from: status)
        }
    }

    @Test("Approved-feature request binds authenticated revisions and rejects unavailable state")
    func approvedFeatureAuthoringBindsAuthenticatedCurrentState() throws {
        let draft = sampleApprovedFeatureDraft()
        let status = try approvedFeatureAuthoringStatus()
        let request = try draft.encodeRequest(from: status)
        let object = try #require(
            JSONSerialization.jsonObject(with: request) as? [String: Any]
        )
        #expect((object["expected_queue_revision"] as? NSNumber)?.uint64Value
            == status.ownerControl?.queueRevision)
        #expect((object["owner_control_designation_revision"] as? NSNumber)?.uint64Value
            == status.ownerControl?.ownerControlDesignationRevision)
        #expect((object["emergency_pause_revision"] as? NSNumber)?.uint64Value
            == status.ownerControl?.emergencyPauseRevision)

        let paused = try approvedFeatureAuthoringStatus(emergencyPaused: true)
        #expect(throws: AssemblywrightMacApprovedFeatureAuthoringError.invalidAuthenticatedStatus) {
            try draft.encodeRequest(from: paused)
        }
        #expect(throws: AssemblywrightMacApprovedFeatureAuthoringError.invalidAuthenticatedStatus) {
            try draft.encodeRequest(from: .init(phase: .masterOffline))
        }
    }

    @Test("Approved-feature review freezes exact authenticated request and rejects incomplete or stale snapshots")
    func approvedFeaturePreparationFreezesExactAuthenticatedRequest() throws {
        let draft = sampleApprovedFeatureDraft()
        let status = try approvedFeatureAuthoringStatus()
        let prepared = try draft.prepareRequest(from: status)

        #expect(prepared.draft == draft)
        #expect(prepared.deviceID == "22222222-2222-4222-8222-222222222222")
        #expect(prepared.connectionEpoch == 91)
        #expect(prepared.expectedQueueRevision == 0)
        #expect(prepared.ownerControlDesignationRevision == 1)
        #expect(!prepared.emergencyPaused)
        #expect(prepared.emergencyPauseRevision == 0)
        let independentlyEncoded = try draft.encodeRequest(from: status)
        #expect(prepared.requestData == independentlyEncoded)
        #expect(prepared.exactRequestSHA256 == Array(SHA256.hash(data: prepared.requestData)))

        let missingDevice = AssemblywrightDeveloperBridgeAppStatus(
            phase: .connected,
            connectionEpoch: status.connectionEpoch,
            featureConveyor: status.featureConveyor,
            ownerControl: status.ownerControl
        )
        #expect(throws: AssemblywrightMacApprovedFeatureAuthoringError.invalidAuthenticatedStatus) {
            try draft.prepareRequest(from: missingDevice)
        }

        let staleOwnerControl = try AssemblywrightMacFeatureConveyorOwnerControlProjection
            .decodeStrict(ownerControlData(queueRevision: 1, completeEvidence: true, active: true))
        let staleSnapshot = AssemblywrightDeveloperBridgeAppStatus(
            phase: .connected,
            deviceID: status.deviceID,
            masterEndpoint: status.masterEndpoint,
            connectionEpoch: status.connectionEpoch,
            featureConveyor: status.featureConveyor,
            ownerControl: staleOwnerControl
        )
        #expect(throws: AssemblywrightMacApprovedFeatureAuthoringError.invalidAuthenticatedStatus) {
            try draft.prepareRequest(from: staleSnapshot)
        }
    }

    @Test("Approved-feature command receipt rejects every current-revision drift")
    func approvedFeatureAuthoringReceiptRejectsDrift() throws {
        let request = try sampleApprovedFeatureDraft().encodeRequest(
            from: approvedFeatureAuthoringStatus()
        )
        let receipt = approvedFeatureAuthoringReceiptData(request: request)
        _ = try AssemblywrightMacFeatureConveyorApprovedFeatureDraft.validateCommandReceipt(
            receipt, requestData: request
        )
        for key in [
            "specification_revision", "lifecycle_revision", "queue_revision",
            "owner_control_designation_revision", "emergency_pause_revision"
        ] {
            var object = try #require(
                JSONSerialization.jsonObject(with: receipt) as? [String: Any]
            )
            object[key] = (object[key] as! NSNumber).uint64Value + 1
            let drifted = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
            #expect(throws: AssemblywrightMacApprovedFeatureAuthoringError.invalidReceipt) {
                try AssemblywrightMacFeatureConveyorApprovedFeatureDraft.validateCommandReceipt(
                    drifted, requestData: request
                )
            }
        }
        let duplicate = Data(
            String(data: receipt, encoding: .utf8)!.replacingOccurrences(
                of: "\"schema_version\":1",
                with: "\"schema_version\":1,\"schema_version\":1"
            ).utf8
        )
        #expect(throws: AssemblywrightMacApprovedFeatureAuthoringError.invalidReceipt) {
            try AssemblywrightMacFeatureConveyorApprovedFeatureDraft.validateCommandReceipt(
                duplicate, requestData: request
            )
        }
    }

    @Test("Activation projection strictly accepts all-six, partial, and active evidence states")
    func activationProjectionStrictStates() throws {
        let ready = try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
            ownerControlData(queueRevision: 7, completeEvidence: true)
        )
        #expect(ready.activationReady)
        #expect(ready.activationBlocker == .none)
        #expect(ready.evidence.readyCount == 6)

        let partial = try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
            ownerControlData(queueRevision: 7)
        )
        #expect(!partial.activationReady)
        #expect(partial.activationBlocker == .evidenceRequired)
        #expect(partial.evidence.readyCount == 0)

        let active = try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
            ownerControlData(queueRevision: 7, completeEvidence: true, active: true)
        )
        #expect(active.activationStatus == .active)
        #expect(active.activationBlocker == .alreadyActivated)
        #expect(!active.activationReady)
    }

    @Test("Owner-control projection preserves canonical UUID text through signed-helper encoding")
    func ownerControlProjectionCanonicalEncodingRoundTrip() throws {
        for (source, expectedEvidenceID) in [
            (
                ownerControlDataWithCanonicalAlphabeticEvidence(queueRevision: 7),
                "abcdef01-abcd-4abc-8abc-abcdefabcdef"
            ),
            (ownerControlData(queueRevision: 7, completeEvidence: true, active: true), nil),
            (ownerControlDataWithActiveFeature(stage: "paused", ownerPaused: false), nil)
        ] {
            let projection = try AssemblywrightMacFeatureConveyorOwnerControlProjection
                .decodeStrict(source)
            let encoded = try JSONEncoder().encode(projection)
            _ = try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(encoded)
            let text = try #require(String(data: encoded, encoding: .utf8))
            if let expectedEvidenceID { #expect(text.contains(expectedEvidenceID)) }
            #expect(!text.contains("ABCDEF01-ABCD-4ABC-8ABC-ABCDEFABCDEF"))
            #expect(!text.contains("AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA"))
            #expect(!text.contains("BBBBBBBB-BBBB-4BBB-8BBB-BBBBBBBBBBBB"))
        }
    }

    @Test("Owner-control projection rejects noncanonical UUID text")
    func ownerControlProjectionRejectsNoncanonicalUUIDText() throws {
        var evidenceObject = try #require(
            JSONSerialization.jsonObject(
                with: ownerControlDataWithCanonicalAlphabeticEvidence(queueRevision: 7)
            ) as? [String: Any]
        )
        var evidence = try #require(evidenceObject["evidence"] as? [String: Any])
        var repositoryEvidence = try #require(
            evidence["repository_gate_proof"] as? [String: Any]
        )
        repositoryEvidence["evidence_id"] = "ABCDEF01-ABCD-4ABC-8ABC-ABCDEFABCDEF"
        evidence["repository_gate_proof"] = repositoryEvidence
        evidenceObject["evidence"] = evidence

        var activationObject = try #require(
            JSONSerialization.jsonObject(
                with: ownerControlData(queueRevision: 7, completeEvidence: true, active: true)
            ) as? [String: Any]
        )
        activationObject["activation_id"] = "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA"

        var activeFeatureObject = try #require(
            JSONSerialization.jsonObject(
                with: ownerControlDataWithActiveFeature(stage: "paused", ownerPaused: false)
            ) as? [String: Any]
        )
        var activeFeature = try #require(activeFeatureObject["active_feature"] as? [String: Any])
        activeFeature["feature_id"] = "BBBBBBBB-BBBB-4BBB-8BBB-BBBBBBBBBBBB"
        activeFeatureObject["active_feature"] = activeFeature

        for object in [evidenceObject, activationObject, activeFeatureObject] {
            let data = try JSONSerialization.data(withJSONObject: object)
            #expect(throws: ControlError.invalidProjection) {
                try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(data)
            }
        }
    }

    @Test("Owner-control projection distinguishes non-owner pauses from owner pauses")
    func ownerControlProjectionAcceptsNonOwnerPausedCheckpoint() throws {
        let providerPaused = try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
            ownerControlDataWithActiveFeature(stage: "paused", ownerPaused: false)
        )
        #expect(providerPaused.activeFeature?.stage == .paused)
        #expect(providerPaused.activeFeature?.lifecycleStatus == .paused)
        #expect(providerPaused.activeFeature?.ownerPaused == false)
        #expect(throws: ControlError.invalidRequest) {
            try AssemblywrightMacFeatureConveyorActivationControl.orchestrationRequest(
                from: providerPaused,
                action: .pause
            )
        }
        #expect(throws: ControlError.invalidRequest) {
            try AssemblywrightMacFeatureConveyorActivationControl.orchestrationRequest(
                from: providerPaused,
                action: .resume
            )
        }

        let ownerPaused = try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
            ownerControlDataWithActiveFeature(stage: "paused", ownerPaused: true)
        )
        #expect(ownerPaused.activeFeature?.ownerPaused == true)
        _ = try AssemblywrightMacFeatureConveyorActivationControl.orchestrationRequest(
            from: ownerPaused,
            action: .resume
        )

        #expect(throws: ControlError.invalidProjection) {
            try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
                ownerControlDataWithActiveFeature(stage: "reviewing", ownerPaused: true)
            )
        }
    }

    @Test("Activation projection rejects extra, duplicate, contradictory, and oversized data")
    func activationProjectionRejectsMalformedAndOversized() {
        let valid = ownerControlData(completeEvidence: true)
        var extra = try! JSONSerialization.jsonObject(with: valid) as! [String: Any]
        extra["owner_token"] = "forbidden"
        let extraData = try! JSONSerialization.data(withJSONObject: extra)
        let duplicate = Data((String(data: valid, encoding: .utf8)!
            .replacingOccurrences(of: "\"schema_version\":1", with: "\"schema_version\":1,\"schema_version\":1")).utf8)
        var contradictory = try! JSONSerialization.jsonObject(with: valid) as! [String: Any]
        contradictory["activation_ready"] = false
        let contradictoryData = try! JSONSerialization.data(withJSONObject: contradictory)
        let oversized = Data(repeating: 0x61, count: 8 * 1_024 + 1)
        for data in [extraData, duplicate, contradictoryData, oversized] {
            #expect(throws: ControlError.invalidProjection) {
                try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(data)
            }
        }
    }

    @Test("Activation command binds the snapshot revisions and rejects receipt drift")
    func activationCommandBindsRevisionsAndReceipt() async throws {
        let projection = try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
            ownerControlData(queueRevision: 7, completeEvidence: true)
        )
        let request = try AssemblywrightMacFeatureConveyorActivationControl.activationRequest(from: projection)
        let receipt = activationReceiptData(request: request)
        let success = FakeSupervisorSession(
            connectionEpoch: 200,
            outcomes: [.response(.init(status: 200, body: receipt))]
        )
        _ = try await AssemblywrightMacFeatureConveyorActivationControl.perform(
            action: .activation, requestData: request, using: success
        )
        #expect(await success.requests.first?.path == AssemblywrightMacOwnerControlAction.activation.path)
        #expect(await success.cancelled)

        var drifted = try JSONSerialization.jsonObject(with: receipt) as! [String: Any]
        drifted["queue_revision"] = 8
        let driftedData = try JSONSerialization.data(withJSONObject: drifted)
        let failure = FakeSupervisorSession(
            connectionEpoch: 201,
            outcomes: [.response(.init(status: 200, body: driftedData))]
        )
        await #expect(throws: ControlError.invalidReceipt) {
            _ = try await AssemblywrightMacFeatureConveyorActivationControl.perform(
                action: .activation, requestData: request, using: failure
            )
        }
        #expect(await failure.cancelled)
    }

    @Test("Foundation one-shot helper routes one bounded confirmed activation command")
    func foundationOneShotOwnerCommandRoutesExactArguments() async throws {
        let projection = try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
            ownerControlData(queueRevision: 7, completeEvidence: true)
        )
        let request = try AssemblywrightMacFeatureConveyorActivationControl.activationRequest(from: projection)
        let receipt = activationReceiptData(request: request)
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("assemblywright-owner-command-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let executable = directory.appendingPathComponent("bridge-fixture")
        let receiptText = try #require(String(data: receipt, encoding: .utf8))
        let script = "#!/bin/sh\nread ignored\nprintf '%s\\n' '\(receiptText)'\n"
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
        let validated = AssemblywrightDeveloperBridgeValidatedExecutable(
            executableURL: executable, teamIdentifier: "ABCDEFGHIJ",
            codeRequirement: "anchor apple generic", cdHash: Data(repeating: 0x11, count: 20)
        )
        let output = try await FoundationAssemblywrightDeveloperBridgeProcessLauncher(
            runningProcessValidator: FakeBridgeRunningProcessValidator()
        ).runCommand(
            executable: validated,
            arguments: AssemblywrightDeveloperBridgeProcessLifecycle.helperArguments(for: .activation),
            input: request
        )
        #expect(output == receipt)
        try AssemblywrightMacFeatureConveyorActivationControl.validateCommandReceipt(
            output, requestData: request, action: .activation
        )
    }

    @Test("Foundation one-shot helper routes exact approved-feature argv, stdin, and receipt")
    func foundationApprovedFeatureOneShotUsesExactProcessBoundary() async throws {
        let request = try sampleApprovedFeatureDraft().encodeRequest(
            from: approvedFeatureAuthoringStatus()
        )
        let receipt = approvedFeatureAuthoringReceiptData(request: request)
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
            "assemblywright-approved-feature-command-\(UUID().uuidString)", isDirectory: true
        )
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let executable = directory.appendingPathComponent("bridge-fixture")
        let capturedInput = directory.appendingPathComponent("captured-input")
        let receiptText = try #require(String(data: receipt, encoding: .utf8))
        let script = """
        #!/bin/sh
        test "$#" -eq 3 && test "$1" = feature-conveyor \
          && test "$2" = approve-and-enqueue && test "$3" = --confirm || exit 64
        /bin/cat > '\(capturedInput.path)'
        printf '%s\\n' '\(receiptText)'
        """
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700], ofItemAtPath: executable.path
        )
        let validated = AssemblywrightDeveloperBridgeValidatedExecutable(
            executableURL: executable,
            teamIdentifier: "ABCDEFGHIJ",
            codeRequirement: "anchor apple generic",
            cdHash: Data(repeating: 0x11, count: 20)
        )
        let output = try await FoundationAssemblywrightDeveloperBridgeProcessLauncher(
            runningProcessValidator: FakeBridgeRunningProcessValidator()
        ).runCommand(
            executable: validated,
            arguments: AssemblywrightDeveloperBridgeProcessLifecycle
                .approvedFeatureEnqueueArguments,
            input: request
        )

        #expect(try Data(contentsOf: capturedInput) == request)
        #expect(output == receipt)
        _ = try AssemblywrightMacFeatureConveyorApprovedFeatureDraft.validateCommandReceipt(
            output, requestData: request
        )
    }

    @Test("Foundation one-shot helper rejects near-miss approved-feature commands locally")
    func foundationApprovedFeatureOneShotAllowlistIsExact() async throws {
        let request = try sampleApprovedFeatureDraft().encodeRequest(
            from: approvedFeatureAuthoringStatus()
        )
        let validated = AssemblywrightDeveloperBridgeValidatedExecutable(
            executableURL: URL(fileURLWithPath: "/tmp/should-not-launch"),
            teamIdentifier: "ABCDEFGHIJ",
            codeRequirement: "anchor apple generic",
            cdHash: Data(repeating: 0x11, count: 20)
        )
        let launcher = FoundationAssemblywrightDeveloperBridgeProcessLauncher(
            runningProcessValidator: FakeBridgeRunningProcessValidator()
        )
        for arguments in [
            ["feature-conveyor", "approve-and-enqueue"],
            ["feature-conveyor", "approve-and-enqueue", "--confirm", "extra"],
            ["feature-conveyor", "enqueue", "--confirm"]
        ] {
            await #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
                _ = try await launcher.runCommand(
                    executable: validated, arguments: arguments, input: request
                )
            }
        }
    }

    @Test("One-shot helper reaps a child that closes stdout and hangs")
    func oneShotOwnerCommandReapsClosedStdoutHang() async throws {
        try await assertHostileOneShotHelperIsReaped(
            script: "#!/bin/sh\nexec 1>&-\nwhile :; do :; done\n"
        )
    }

    @Test("One-shot helper escalates to KILL when a hung child ignores TERM")
    func oneShotOwnerCommandKillsTermIgnoringHang() async throws {
        try await assertHostileOneShotHelperIsReaped(
            script: "#!/usr/bin/perl\n$SIG{TERM} = 'IGNORE';\nselect undef, undef, undef, 0.1;\nclose STDOUT;\nwhile (1) {}\n",
            minimumDuration: .milliseconds(700),
            commandTimeout: .milliseconds(300)
        )
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

    @Test("Signed-helper buffering reserves the strict Assembly Line projection bound")
    func appHelperSnapshotAssemblyLineBufferBoundary() throws {
        #expect(
            AssemblywrightDeveloperBridgeProcessLifecycle.maximumAssemblyLineSnapshotBytes
                == AssemblywrightMacAssemblyLineOwnerProjection.maximumBytes
        )
        #expect(
            AssemblywrightDeveloperBridgeProcessLifecycle.maximumLineBytes
                == AssemblywrightDeveloperBridgeProcessLifecycle.maximumLegacySnapshotBytes
                    + AssemblywrightMacAssemblyLineOwnerProjection.maximumBytes
        )
        let valid = authenticatedSnapshotData(connectionEpoch: 24)
        let paddingCount =
            AssemblywrightDeveloperBridgeProcessLifecycle.maximumLineBytes - valid.count
        let boundary = Data(repeating: 0x20, count: paddingCount) + valid
        #expect(boundary.count == AssemblywrightDeveloperBridgeProcessLifecycle.maximumLineBytes)
        #expect(
            try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: boundary).phase
                == .connected
        )
        #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
            try AssemblywrightDeveloperBridgeProcessLifecycle.status(
                from: Data([0x20]) + boundary
            )
        }

        let legacy = Data(
            #"{"phase":"backing_off","device_id":"22222222-2222-4222-8222-222222222222","master_endpoint":"100.64.23.14:7792","consecutive_failures":1,"next_delay_ms":1000,"error_code":"connection_failed"}"#.utf8
        )
        let legacyOversized = Data(
            repeating: 0x20,
            count: AssemblywrightDeveloperBridgeProcessLifecycle.maximumLegacySnapshotBytes
                + 1 - legacy.count
        ) + legacy
        #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
            try AssemblywrightDeveloperBridgeProcessLifecycle.status(from: legacyOversized)
        }
    }

    @Test("Persisted helper locator round trips only an exact owner-private safe file")
    func persistedHelperLocatorIsStrictAndPrivate() throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-locator")
        defer { try? FileManager.default.removeItem(at: root) }
        let helper = try executableFixture(in: root)
        let store = AssemblywrightDeveloperBridgeConfigurationStore(
            fileURL: root.appendingPathComponent("private/configuration.json")
        )
        let stored = try AssemblywrightDeveloperBridgeStoredConfiguration(
            helperPath: helper.path,
            teamIdentifier: "ABCDEFGHIJ"
        )

        try store.save(stored)

        #expect(try store.load() == stored)
        let firstDocument = try Data(contentsOf: store.fileURL)
        try store.save(stored)
        #expect(try Data(contentsOf: store.fileURL) == firstDocument)
        #expect(
            try FileManager.default.contentsOfDirectory(
                at: store.fileURL.deletingLastPathComponent(),
                includingPropertiesForKeys: nil
            ).allSatisfy { !$0.lastPathComponent.hasSuffix(".tmp") }
        )
        var metadata = stat()
        #expect(lstat(store.fileURL.path, &metadata) == 0)
        #expect(metadata.st_mode & 0o777 == 0o600)
        #expect(metadata.st_nlink == 1)

        try FileManager.default.setAttributes(
            [.posixPermissions: 0o644],
            ofItemAtPath: store.fileURL.path
        )
        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore) {
            _ = try store.load()
        }
    }

    @Test("Persisted helper locator rejects duplicate shape, symlinks, unsafe modes, and team drift")
    func persistedHelperLocatorRejectsUnsafeInputs() throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-locator-negative")
        defer { try? FileManager.default.removeItem(at: root) }
        let helper = try executableFixture(in: root)
        let symlink = root.appendingPathComponent("helper-link")
        try FileManager.default.createSymbolicLink(at: symlink, withDestinationURL: helper)

        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.invalidConfiguration) {
            _ = try AssemblywrightDeveloperBridgeStoredConfiguration(
                helperPath: symlink.path,
                teamIdentifier: "ABCDEFGHIJ"
            )
        }
        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.invalidConfiguration) {
            _ = try AssemblywrightDeveloperBridgeStoredConfiguration(
                helperPath: helper.path,
                teamIdentifier: "abc"
            )
        }
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: helper.path)
        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.invalidConfiguration) {
            _ = try AssemblywrightDeveloperBridgeStoredConfiguration(
                helperPath: helper.path,
                teamIdentifier: "ABCDEFGHIJ"
            )
        }
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: helper.path)
        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.invalidConfiguration) {
            _ = try AssemblywrightDeveloperBridgeStoredConfiguration(
                helperPath: "assemblywright-mac-bridge",
                teamIdentifier: "ABCDEFGHIJ"
            )
        }
        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.invalidConfiguration) {
            _ = try AssemblywrightDeveloperBridgeStoredConfiguration(
                helperPath: root.appendingPathComponent("nested/../assemblywright-mac-bridge").path,
                teamIdentifier: "ABCDEFGHIJ"
            )
        }
        try FileManager.default.setAttributes([.posixPermissions: 0o722], ofItemAtPath: helper.path)
        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.invalidConfiguration) {
            _ = try AssemblywrightDeveloperBridgeStoredConfiguration(
                helperPath: helper.path,
                teamIdentifier: "ABCDEFGHIJ"
            )
        }

        let store = AssemblywrightDeveloperBridgeConfigurationStore(
            fileURL: root.appendingPathComponent("duplicate.json")
        )
        let duplicate = Data(
            "{\"schema_version\":1,\"schema_version\":1,\"helper_path\":\"\(helper.path)\",\"team_identifier\":\"ABCDEFGHIJ\"}".utf8
        )
        FileManager.default.createFile(atPath: store.fileURL.path, contents: duplicate)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: store.fileURL.path
        )
        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore) {
            _ = try store.load()
        }
    }

    @Test("Persisted helper locator rejects empty, oversized, linked, and unsafe parent state")
    func persistedHelperLocatorRejectsUnsafeStoreShapes() throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-store-shapes")
        defer { try? FileManager.default.removeItem(at: root) }
        let helper = try executableFixture(in: root)
        let stored = try AssemblywrightDeveloperBridgeStoredConfiguration(
            helperPath: helper.path,
            teamIdentifier: "ABCDEFGHIJ"
        )
        let validDocument = try JSONEncoder().encode(stored)

        let missing = AssemblywrightDeveloperBridgeConfigurationStore(
            fileURL: root.appendingPathComponent("missing.json")
        )
        #expect(try missing.load() == nil)

        for (name, data) in [
            ("empty.json", Data()),
            (
                "oversized.json",
                Data(
                    repeating: 0x20,
                    count: AssemblywrightDeveloperBridgeConfigurationStore
                        .maximumDocumentBytes + 1
                )
            )
        ] {
            let url = root.appendingPathComponent(name)
            FileManager.default.createFile(atPath: url.path, contents: data)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600], ofItemAtPath: url.path
            )
            let store = AssemblywrightDeveloperBridgeConfigurationStore(fileURL: url)
            #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore) {
                _ = try store.load()
            }
        }

        let linkedSource = root.appendingPathComponent("linked-source.json")
        FileManager.default.createFile(atPath: linkedSource.path, contents: validDocument)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600], ofItemAtPath: linkedSource.path
        )
        let hardLink = root.appendingPathComponent("hard-link.json")
        try FileManager.default.linkItem(at: linkedSource, to: hardLink)
        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore) {
            _ = try AssemblywrightDeveloperBridgeConfigurationStore(fileURL: hardLink).load()
        }
        let symbolicLink = root.appendingPathComponent("symbolic-link.json")
        try FileManager.default.createSymbolicLink(
            at: symbolicLink, withDestinationURL: linkedSource
        )
        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore) {
            _ = try AssemblywrightDeveloperBridgeConfigurationStore(fileURL: symbolicLink).load()
        }

        let unsafeDirectory = root.appendingPathComponent("unsafe-parent", isDirectory: true)
        try FileManager.default.createDirectory(
            at: unsafeDirectory,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755], ofItemAtPath: unsafeDirectory.path
        )
        let unsafeParentFile = unsafeDirectory.appendingPathComponent("configuration.json")
        FileManager.default.createFile(atPath: unsafeParentFile.path, contents: validDocument)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600], ofItemAtPath: unsafeParentFile.path
        )
        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore) {
            _ = try AssemblywrightDeveloperBridgeConfigurationStore(
                fileURL: unsafeParentFile
            ).load()
        }
        let missingInUnsafeParent = unsafeDirectory.appendingPathComponent("missing.json")
        #expect(throws: AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore) {
            _ = try AssemblywrightDeveloperBridgeConfigurationStore(
                fileURL: missingInUnsafeParent
            ).load()
        }
    }

    @MainActor
    @Test("Invalid persisted helper locator blocks environment fallback")
    func invalidPersistedHelperLocatorTakesPrecedence() async throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-precedence")
        defer { try? FileManager.default.removeItem(at: root) }
        let helper = try executableFixture(in: root)
        let store = AssemblywrightDeveloperBridgeConfigurationStore(
            fileURL: root.appendingPathComponent("configuration.json")
        )
        FileManager.default.createFile(atPath: store.fileURL.path, contents: Data("{}".utf8))
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600], ofItemAtPath: store.fileURL.path
        )
        let launcher = FakeBridgeProcessLauncher(session: FakeBridgeProcessSession(lines: []))
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configurationStore: store,
            environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    helper.path,
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ"
            ],
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher,
            localModelSelectionStore: .init(fileURL: root.appendingPathComponent("model.json")),
            assemblyLinePendingMutationStore: .init(
                fileURL: root.appendingPathComponent("pending.json")
            )
        )

        lifecycle.start()
        await Task.yield()

        #expect(lifecycle.bridgeConfigurationState == .invalidStore)
        #expect(lifecycle.status.errorCode == "developer_bridge_configuration_store_invalid")
        #expect(lifecycle.setupActionErrorCode == "developer_bridge_configuration_store_invalid")
        #expect(await launcher.launchCount == 0)

        let unsafeDirectory = root.appendingPathComponent("unsafe-parent", isDirectory: true)
        try FileManager.default.createDirectory(
            at: unsafeDirectory,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755], ofItemAtPath: unsafeDirectory.path
        )
        let unsafeParentLauncher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: [])
        )
        let unsafeParentLifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configurationStore: .init(
                fileURL: unsafeDirectory.appendingPathComponent("missing.json")
            ),
            environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    helper.path,
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ"
            ],
            validator: FakeBridgeExecutableValidator(),
            launcher: unsafeParentLauncher,
            localModelSelectionStore: .init(
                fileURL: root.appendingPathComponent("unsafe-parent-model.json")
            ),
            assemblyLinePendingMutationStore: .init(
                fileURL: root.appendingPathComponent("unsafe-parent-pending.json")
            )
        )
        unsafeParentLifecycle.start()
        await Task.yield()
        #expect(unsafeParentLifecycle.bridgeConfigurationState == .invalidStore)
        #expect(
            unsafeParentLifecycle.status.errorCode
                == "developer_bridge_configuration_store_invalid"
        )
        #expect(await unsafeParentLauncher.launchCount == 0)
    }

    @MainActor
    @Test("Owner configuration validates, persists, and starts the exact helper")
    func ownerConfigurationPersistsAndStartsHelper() async throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-configure")
        defer { try? FileManager.default.removeItem(at: root) }
        let helper = try executableFixture(in: root)
        let store = AssemblywrightDeveloperBridgeConfigurationStore(
            fileURL: root.appendingPathComponent("private/configuration.json")
        )
        let launcher = FakeBridgeProcessLauncher(session: FakeBridgeProcessSession(lines: []))
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [:]),
            configurationStore: store,
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher,
            localModelSelectionStore: .init(fileURL: root.appendingPathComponent("model.json")),
            assemblyLinePendingMutationStore: .init(
                fileURL: root.appendingPathComponent("pending.json")
            )
        )

        #expect(await lifecycle.configureBridge(
            helperURL: helper,
            expectedTeamIdentifier: "ABCDEFGHIJ"
        ))
        for _ in 0 ..< 100 where await launcher.launchCount == 0 { await Task.yield() }

        #expect(lifecycle.bridgeConfigurationState == .configured)
        #expect(lifecycle.setupActionErrorCode == nil)
        #expect(try store.load()?.helperPath == helper.path)
        #expect(await launcher.launchCount == 1)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Setup lifecycle exposes only strict status, pairing, and rotation documents")
    func setupLifecycleCommandsAreExactAndBounded() async throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-setup")
        defer { try? FileManager.default.removeItem(at: root) }
        let csr = enrollmentCSRData()
        let enrolled = enrollmentStatusData(status: "enrolled")
        let installed = enrollmentStatusData(status: "enrollment_installed")
        let rotated = enrollmentStatusData(status: "certificate_rotation_installed")
        let invitation = try validInvitationData()
        let receipt = try validIssuedReceiptData()
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: []),
            restartSession: FakeBridgeProcessSession(lines: []),
            commandResponses: [enrolled, csr, installed, csr, rotated]
        )
        let lifecycle = setupLifecycle(
            root: root,
            launcher: launcher
        )

        #expect(await lifecycle.enrollmentStatus()?.installed == true)
        #expect(await lifecycle.prepareEnrollment(invitationData: invitation) == csr)
        #expect(
            await lifecycle.installEnrollment(receiptData: receipt)?
                .registryRevision == 3
        )
        #expect(
            await lifecycle.prepareCertificateRotation(invitationData: invitation)
                == csr
        )
        #expect(
            await lifecycle.installCertificateRotation(receiptData: receipt)?
                .installed == true
        )
        #expect(await launcher.commands.map(\.0) == [
            AssemblywrightDeveloperBridgeProcessLifecycle.enrollmentStatusArguments,
            AssemblywrightDeveloperBridgeProcessLifecycle.enrollmentPrepareArguments,
            AssemblywrightDeveloperBridgeProcessLifecycle.enrollmentInstallArguments,
            AssemblywrightDeveloperBridgeProcessLifecycle.rotationPrepareArguments,
            AssemblywrightDeveloperBridgeProcessLifecycle.rotationInstallArguments
        ])
        await lifecycle.stop()
    }

    @MainActor
    @Test("Setup lifecycle rejects overlapping owner actions without misreporting configuration")
    func setupLifecycleRejectsConcurrentOwnerActions() async throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-setup-overlap")
        defer { try? FileManager.default.removeItem(at: root) }
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: []),
            restartSession: FakeBridgeProcessSession(lines: []),
            commandResponse: enrollmentStatusData(status: "enrolled"),
            commandDelay: .milliseconds(100)
        )
        let lifecycle = setupLifecycle(root: root, launcher: launcher)
        let first = Task { await lifecycle.enrollmentStatus() }
        for _ in 0 ..< 100 where await launcher.commands.isEmpty { await Task.yield() }

        #expect(
            await lifecycle.prepareEnrollment(invitationData: try validInvitationData()) == nil
        )
        #expect(lifecycle.setupActionErrorCode == nil)
        #expect(await first.value?.installed == true)
        #expect(lifecycle.setupActionErrorCode == nil)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Setup lifecycle rejects secret, path, duplicate, and binding drift in helper output")
    func setupLifecycleRejectsUnsafeHelperOutput() async throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-output-negative")
        defer { try? FileManager.default.removeItem(at: root) }
        let invitation = try validInvitationData()
        let unsafeStatus = Data(
            #"{"status":"enrolled","device_id":"22222222-2222-4222-8222-222222222222","device_name":"owner-mac-bridge","master_endpoint":"100.64.23.14:7792","registry_revision":3,"certificate_not_after_ms":4102444800000,"helper_path":"/private/helper"}"#.utf8
        )
        let duplicateCSR = Data(
            #"{"schema_version":1,"status":"enrollment_csr_ready","grant_id":"11111111-1111-4111-8111-111111111111","device_id":"22222222-2222-4222-8222-222222222222","device_id":"22222222-2222-4222-8222-222222222222","csr_pem":"-----BEGIN CERTIFICATE REQUEST-----\nZmFrZQ==\n-----END CERTIFICATE REQUEST-----\n"}"#.utf8
        )
        let malformedInstall = Data(
            #"{"status":"enrollment_installed","secret":"must-not-survive"}"#.utf8
        )
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: []),
            restartSession: FakeBridgeProcessSession(lines: []),
            commandResponses: [unsafeStatus, duplicateCSR, malformedInstall]
        )
        let lifecycle = setupLifecycle(root: root, launcher: launcher)
        let receipt = try validIssuedReceiptData()

        #expect(await lifecycle.enrollmentStatus() == nil)
        #expect(lifecycle.setupActionErrorCode == "invalid_helper_setup_response")
        #expect(await lifecycle.prepareEnrollment(invitationData: invitation) == nil)
        #expect(lifecycle.setupActionErrorCode == "invalid_helper_setup_response")
        #expect(
            await lifecycle.installEnrollment(receiptData: receipt) == nil
        )
        #expect(lifecycle.setupActionErrorCode == "enrollment_install_recovery_required")
        #expect(!String(describing: lifecycle.status).contains("/private/helper"))
        await lifecycle.stop()
    }

    @MainActor
    @Test("Setup numeric fields reject JSON booleans before NSNumber conversion")
    func setupNumericFieldsRejectBooleans() async throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-boolean")
        defer { try? FileManager.default.removeItem(at: root) }
        let invitation = try validInvitationData()
        let booleanSchemaCSR = Data(
            #"{"schema_version":true,"status":"enrollment_csr_ready","grant_id":"11111111-1111-4111-8111-111111111111","device_id":"22222222-2222-4222-8222-222222222222","csr_pem":"-----BEGIN CERTIFICATE REQUEST-----\nZmFrZQ==\n-----END CERTIFICATE REQUEST-----\n"}"#.utf8
        )
        let booleanRevision = enrollmentStatusData(
            status: "enrolled",
            registryRevisionJSON: "true"
        )
        let booleanExpiry = enrollmentStatusData(
            status: "enrolled",
            certificateNotAfterJSON: "true"
        )
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: []),
            restartSession: FakeBridgeProcessSession(lines: []),
            commandResponses: [booleanSchemaCSR, booleanRevision, booleanExpiry]
        )
        let lifecycle = setupLifecycle(root: root, launcher: launcher)

        #expect(await lifecycle.prepareEnrollment(invitationData: invitation) == nil)
        #expect(lifecycle.setupActionErrorCode == "invalid_helper_setup_response")
        #expect(await lifecycle.enrollmentStatus() == nil)
        #expect(lifecycle.setupActionErrorCode == "invalid_helper_setup_response")
        #expect(await lifecycle.enrollmentStatus() == nil)
        #expect(lifecycle.setupActionErrorCode == "invalid_helper_setup_response")
        await lifecycle.stop()
    }

    @Test("Native one-shot setup allowlist rejects near misses before launch")
    func nativeSetupCommandAllowlistIsExact() async throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-native-setup")
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("bridge-fixture")
        let script = "#!/bin/sh\ntest \"$#\" -eq 1 && test \"$1\" = status || exit 64\nread unexpected && exit 65\nprintf '%s\\n' '{\"status\":\"not_enrolled\"}'\n"
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700], ofItemAtPath: executable.path
        )
        let validated = AssemblywrightDeveloperBridgeValidatedExecutable(
            executableURL: executable,
            teamIdentifier: "ABCDEFGHIJ",
            codeRequirement: "anchor apple generic",
            cdHash: Data(repeating: 0x11, count: 20)
        )
        let launcher = FoundationAssemblywrightDeveloperBridgeProcessLauncher(
            runningProcessValidator: FakeBridgeRunningProcessValidator()
        )

        #expect(try await launcher.runCommand(
            executable: validated,
            arguments: AssemblywrightDeveloperBridgeProcessLifecycle.enrollmentStatusArguments,
            input: Data()
        ) == Data(#"{"status":"not_enrolled"}"#.utf8))

        for (arguments, input) in [
            (["enrollment", "prepare", "--confirm"], Data("{}".utf8)),
            (["enrollment", "rotate", "prepare"], Data("{}".utf8)),
            (["enrollment", "remove", "--confirm"], Data("{}".utf8)),
            (["status"], Data("{}".utf8)),
            (["enrollment", "prepare"], Data())
        ] {
            await #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
                _ = try await launcher.runCommand(
                    executable: validated,
                    arguments: arguments,
                    input: input
                )
            }
        }
    }

    @MainActor
    @Test("Persisted owner setup drives a real monitor and status process without launch variables")
    func persistedOwnerSetupNativeProcessE2E() async throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-native-persisted")
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("bridge-fixture")
        let monitorLine = String(
            decoding: authenticatedSnapshotData(connectionEpoch: 73),
            as: UTF8.self
        ).replacingOccurrences(of: "'", with: "'\"'\"'")
        let script = """
        #!/bin/sh
        if [ "$#" -eq 1 ] && [ "$1" = monitor ]; then
          printf '%s\n' '\(monitorLine)'
          while true; do /bin/sleep 1; done
        fi
        if [ "$#" -eq 1 ] && [ "$1" = status ]; then
          read unexpected && exit 65
          printf '%s\n' '{"status":"not_enrolled"}'
          exit 0
        fi
        exit 64
        """
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700], ofItemAtPath: executable.path
        )
        let store = AssemblywrightDeveloperBridgeConfigurationStore(
            fileURL: root.appendingPathComponent("private/configuration.json")
        )
        let firstValidator = RecordingBridgeRunningProcessValidator()
        let first = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [:]),
            configurationStore: store,
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationAssemblywrightDeveloperBridgeProcessLauncher(
                runningProcessValidator: firstValidator,
                ownerCommandTimeout: .seconds(2)
            ),
            localModelSelectionStore: .init(fileURL: root.appendingPathComponent("model-1.json")),
            assemblyLinePendingMutationStore: .init(
                fileURL: root.appendingPathComponent("pending-1.json")
            )
        )

        #expect(await first.configureBridge(
            helperURL: executable,
            expectedTeamIdentifier: "ABCDEFGHIJ"
        ))
        for _ in 0 ..< 200 where first.status.phase != .connected {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(first.status.phase == .connected)
        #expect(first.status.connectionEpoch == 73)
        await first.stop()
        if let processIdentifier = firstValidator.processIdentifier {
            #expect(!processExists(processIdentifier))
        }

        let secondValidator = RecordingBridgeRunningProcessValidator()
        let reloaded = AssemblywrightDeveloperBridgeProcessLifecycle(
            configurationStore: store,
            validator: FakeBridgeExecutableValidator(),
            launcher: FoundationAssemblywrightDeveloperBridgeProcessLauncher(
                runningProcessValidator: secondValidator,
                ownerCommandTimeout: .seconds(2)
            ),
            localModelSelectionStore: .init(fileURL: root.appendingPathComponent("model-2.json")),
            assemblyLinePendingMutationStore: .init(
                fileURL: root.appendingPathComponent("pending-2.json")
            )
        )
        #expect(reloaded.bridgeConfigurationState == .configured)
        reloaded.start()
        for _ in 0 ..< 200 where reloaded.status.phase != .connected {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(reloaded.status.phase == .connected)
        #expect(await reloaded.enrollmentStatus()?.installed == false)
        for _ in 0 ..< 200 where reloaded.status.phase != .connected {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(reloaded.status.connectionEpoch == 73)
        await reloaded.stop()
        if let processIdentifier = secondValidator.processIdentifier {
            #expect(!processExists(processIdentifier))
        }
    }

    @Test("Native install command distinguishes pre-input rejection from post-input ambiguity")
    func nativeInstallCommandPreservesEffectBoundary() async throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-install-effect")
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("bridge-fixture")
        let script = "#!/bin/sh\n/bin/cat >/dev/null\nexit 9\n"
        try Data(script.utf8).write(to: executable, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700], ofItemAtPath: executable.path
        )
        let validated = AssemblywrightDeveloperBridgeValidatedExecutable(
            executableURL: executable,
            teamIdentifier: "ABCDEFGHIJ",
            codeRequirement: "anchor apple generic",
            cdHash: Data(repeating: 0x11, count: 20)
        )
        let input = Data("{}".utf8)

        await #expect(throws: AssemblywrightDeveloperBridgeProcessError.commandOutcomeUnknown) {
            _ = try await FoundationAssemblywrightDeveloperBridgeProcessLauncher(
                runningProcessValidator: FakeBridgeRunningProcessValidator()
            ).runCommand(
                executable: validated,
                arguments: AssemblywrightDeveloperBridgeProcessLifecycle
                    .enrollmentInstallArguments,
                input: input
            )
        }
        await #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidExecutableSignature) {
            _ = try await FoundationAssemblywrightDeveloperBridgeProcessLauncher(
                runningProcessValidator: FakeBridgeRunningProcessValidator(
                    error: .invalidExecutableSignature
                )
            ).runCommand(
                executable: validated,
                arguments: AssemblywrightDeveloperBridgeProcessLifecycle
                    .enrollmentInstallArguments,
                input: input
            )
        }
    }

    @MainActor
    @Test("Cancelled setup install reports recovery and restarts observation")
    func cancelledSetupInstallRequiresRecovery() async throws {
        let root = try privateTemporaryDirectory(prefix: "assemblywright-helper-cancel")
        defer { try? FileManager.default.removeItem(at: root) }
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: []),
            restartSession: FakeBridgeProcessSession(lines: []),
            commandDelay: .seconds(30)
        )
        let lifecycle = setupLifecycle(root: root, launcher: launcher)
        let receipt = try validIssuedReceiptData()
        let action = Task {
            await lifecycle.installEnrollment(receiptData: receipt)
        }
        for _ in 0 ..< 100 where await launcher.commands.isEmpty { await Task.yield() }

        action.cancel()
        _ = await action.value
        for _ in 0 ..< 100 where await launcher.launchCount == 0 { await Task.yield() }

        #expect(lifecycle.setupActionErrorCode == "enrollment_install_recovery_required")
        #expect(await launcher.launchCount == 1)
        await lifecycle.stop()
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
    @Test("Owner action reaps monitor before one-shot helper and restarts observation")
    func ownerActionSerializesHelperChildrenAndRecoversMonitor() async {
        let readyLine = authenticatedSnapshotData(
            connectionEpoch: 44,
            ownerControl: ownerControlData(completeEvidence: true)
        )
        let first = FakeBridgeProcessSession(lines: [readyLine])
        let restarted = FakeBridgeProcessSession(lines: [readyLine])
        let launcher = FakeBridgeProcessLauncher(session: first, restartSession: restarted)
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(), launcher: launcher
        )
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.ownerControl?.activationReady != true { await Task.yield() }
        await lifecycle.performOwnerAction(.activation)
        for _ in 0 ..< 100 where await launcher.launchCount < 2 { await Task.yield() }

        #expect(await launcher.commandSawStoppedMonitor)
        #expect(await launcher.commands.count == 1)
        #expect(await launcher.commands.first?.0 == AssemblywrightDeveloperBridgeProcessLifecycle.helperArguments(for: .activation))
        #expect(await launcher.launchCount == 2)
        #expect(lifecycle.ownerActionErrorCode == nil)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Owner action receipt failure fails closed and still relaunches monitor")
    func ownerActionReceiptFailureFailsClosedAndRecovers() async {
        let readyLine = authenticatedSnapshotData(connectionEpoch: 44, ownerControl: ownerControlData(completeEvidence: true))
        let first = FakeBridgeProcessSession(lines: [readyLine])
        let restarted = FakeBridgeProcessSession(lines: [readyLine])
        let launcher = FakeBridgeProcessLauncher(session: first, restartSession: restarted, commandSucceeds: false)
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ"
            ]), validator: FakeBridgeExecutableValidator(), launcher: launcher
        )
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.ownerControl?.activationReady != true { await Task.yield() }
        await lifecycle.performOwnerAction(.activation)
        for _ in 0 ..< 100 where await launcher.launchCount < 2 { await Task.yield() }
        #expect(await launcher.commandSawStoppedMonitor)
        #expect(lifecycle.ownerActionErrorCode == "invalid_helper_snapshot")
        #expect(await launcher.launchCount == 2)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Approved-feature enqueue reaps observation, validates one receipt, and restarts")
    func approvedFeatureLifecycleSerializesOneShotAndRestarts() async {
        let line = authenticatedSnapshotData(
            connectionEpoch: 44,
            ownerControl: ownerControlData(completeEvidence: true, active: true)
        )
        let first = FakeBridgeProcessSession(lines: [line])
        let restarted = FakeBridgeProcessSession(lines: [line])
        let launcher = FakeBridgeProcessLauncher(session: first, restartSession: restarted)
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher
        )
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.ownerControl == nil { await Task.yield() }

        let prepared = try! sampleApprovedFeatureDraft().prepareRequest(from: lifecycle.status)
        await lifecycle.performApprovedFeatureEnqueue(prepared)
        for _ in 0 ..< 100 where await launcher.launchCount < 2 { await Task.yield() }

        #expect(await launcher.commandSawStoppedMonitor)
        #expect(await launcher.commands.count == 1)
        #expect(await launcher.commands.first?.0
            == AssemblywrightDeveloperBridgeProcessLifecycle.approvedFeatureEnqueueArguments)
        #expect(lifecycle.approvedFeatureReceipt?.status == "queued")
        #expect(lifecycle.pendingApprovedFeatureRecovery == nil)
        #expect(lifecycle.ownerActionErrorCode == nil)
        #expect(await launcher.launchCount == 2)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Monitor update between review and confirmation cannot change frozen enqueue bytes")
    func approvedFeatureLifecycleUsesReviewFrozenBytesAfterMonitorUpdate() async throws {
        let draft = sampleApprovedFeatureDraft()
        let reviewStatus = try approvedFeatureAuthoringStatus()
        let prepared = try draft.prepareRequest(from: reviewStatus)
        let updatedLine = authenticatedSnapshotData(
            connectionEpoch: 92,
            featureConveyor: readyFeatureConveyorData(),
            ownerControl: ownerControlData(
                queueRevision: 1, completeEvidence: true, active: true
            )
        )
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: [updatedLine]),
            restartSession: FakeBridgeProcessSession(lines: [updatedLine])
        )
        let lifecycle = approvedFeatureLifecycle(launcher: launcher)
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.ownerControl?.queueRevision != 1 {
            await Task.yield()
        }
        #expect(lifecycle.status.connectionEpoch == 92)

        await lifecycle.performApprovedFeatureEnqueue(prepared)
        for _ in 0 ..< 100 where await launcher.launchCount < 2 { await Task.yield() }
        let commands = await launcher.commands
        let sent = try #require(commands.first?.1)
        let sentObject = try #require(
            JSONSerialization.jsonObject(with: sent) as? [String: Any]
        )

        #expect(commands.count == 1)
        #expect(sent == prepared.requestData)
        #expect(Array(SHA256.hash(data: sent)) == prepared.exactRequestSHA256)
        #expect((sentObject["expected_queue_revision"] as? NSNumber)?.uint64Value == 0)
        #expect(lifecycle.status.ownerControl?.queueRevision == 1)
        #expect(lifecycle.approvedFeatureReceipt?.queueRevision == 1)
        #expect(lifecycle.ownerActionErrorCode == nil)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Approved-feature receipt rejection fails closed and still restarts observation")
    func approvedFeatureLifecycleRejectsReceiptDriftAndRestarts() async {
        let line = authenticatedSnapshotData(
            connectionEpoch: 44,
            ownerControl: ownerControlData(completeEvidence: true, active: true)
        )
        let first = FakeBridgeProcessSession(lines: [line])
        let restarted = FakeBridgeProcessSession(lines: [line])
        let launcher = FakeBridgeProcessLauncher(
            session: first,
            restartSession: restarted,
            commandResponse: approvedFeatureOwnerControlReceiptData(queueRevision: 2)
        )
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher
        )
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.ownerControl == nil { await Task.yield() }

        let prepared = try! sampleApprovedFeatureDraft().prepareRequest(from: lifecycle.status)
        await lifecycle.performApprovedFeatureEnqueue(prepared)
        for _ in 0 ..< 100 where await launcher.launchCount < 2 { await Task.yield() }

        #expect(lifecycle.approvedFeatureReceipt == nil)
        #expect(lifecycle.ownerActionErrorCode == "approved_feature_reconciliation_required")
        #expect(lifecycle.pendingApprovedFeatureRecovery?.draft == sampleApprovedFeatureDraft())
        #expect(await launcher.launchCount == 2)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Approved-feature command cancellation reaps and restores observation")
    func approvedFeatureLifecycleCancellationRestartsObservation() async {
        let line = authenticatedSnapshotData(
            connectionEpoch: 44,
            ownerControl: ownerControlData(completeEvidence: true, active: true)
        )
        let first = FakeBridgeProcessSession(lines: [line])
        let restarted = FakeBridgeProcessSession(lines: [line])
        let launcher = FakeBridgeProcessLauncher(
            session: first,
            restartSession: restarted,
            commandDelay: .seconds(30)
        )
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher
        )
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.ownerControl == nil { await Task.yield() }
        let prepared = try! sampleApprovedFeatureDraft().prepareRequest(from: lifecycle.status)
        let action = Task { await lifecycle.performApprovedFeatureEnqueue(prepared) }
        for _ in 0 ..< 100 where await launcher.commands.isEmpty { await Task.yield() }
        action.cancel()
        await action.value
        for _ in 0 ..< 100 where await launcher.launchCount < 2 { await Task.yield() }

        #expect(lifecycle.approvedFeatureReceipt == nil)
        #expect(lifecycle.ownerActionErrorCode == "approved_feature_reconciliation_required")
        #expect(lifecycle.pendingApprovedFeatureRecovery?.draft == sampleApprovedFeatureDraft())
        #expect(await launcher.launchCount == 2)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Approved-feature enqueue is unavailable to fixture and local-coding identities")
    func approvedFeatureLifecycleRejectsNonstandardIdentity() async {
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: [])
        )
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

        let prepared = try! sampleApprovedFeatureDraft().prepareRequest(
            from: approvedFeatureAuthoringStatus()
        )
        await lifecycle.performApprovedFeatureEnqueue(prepared)

        #expect(lifecycle.ownerActionErrorCode == "approved_feature_enqueue_rejected")
        #expect(await launcher.commands.isEmpty)
        #expect(await launcher.launchCount == 0)
    }

    @MainActor
    @Test("Explicit local-model rejection clears recovery and restarts the old relay")
    func localModelTerminalRejectionCannotResume() async throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent(UUID().uuidString.lowercased(), isDirectory: true)
        try FileManager.default.createDirectory(
            at: root,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("mlx_lm.generate")
        FileManager.default.createFile(
            atPath: executable.path,
            contents: Data("#!/bin/sh\n".utf8)
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: executable.path
        )
        let models = root.appendingPathComponent("models", isDirectory: true)
        try FileManager.default.createDirectory(at: models, withIntermediateDirectories: false)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: models.path
        )
        let projection = Data(
            #"{"schema_version":1,"device_id":"22222222-2222-4222-8222-222222222222","device_name":"owner-bridge","registry_revision":1,"designation_revision":1,"emergency_pause_revision":0,"emergency_paused":false,"model_id":"old-model"}"#.utf8
        )
        let line = authenticatedSnapshotData(
            connectionEpoch: 44,
            localModelSelection: projection
        )
        let terminal = Data(
            #"{"schema_version":1,"device_id":"22222222-2222-4222-8222-222222222222","expected_registry_revision":1,"expected_designation_revision":1,"expected_emergency_pause_revision":0,"model_id":"target-model","status":"rejected","error_code":"local_model_selection_rejected"}"#.utf8
        )
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: [line]),
            restartSession: FakeBridgeProcessSession(lines: [line]),
            commandResponse: terminal
        )
        let store = AssemblywrightMacLocalModelSelectionStore(
            fileURL: root.appendingPathComponent("private/state.json")
        )
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ",
                AssemblywrightDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey:
                    executable.path,
                AssemblywrightDeveloperBridgeProcessConfiguration.agentDataDirectoryEnvironmentKey:
                    root.path,
                AssemblywrightDeveloperBridgeProcessConfiguration.mlxJobsEnabledEnvironmentKey:
                    "true",
                AssemblywrightDeveloperBridgeProcessConfiguration.mlxExecutableEnvironmentKey:
                    executable.path,
                AssemblywrightDeveloperBridgeProcessConfiguration.mlxModelDirectoryEnvironmentKey:
                    models.path,
                AssemblywrightDeveloperBridgeProcessConfiguration.mlxModelIDEnvironmentKey:
                    "old-model"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher,
            localModelSelectionStore: store
        )
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.localModelSelection == nil {
            await Task.yield()
        }

        await lifecycle.selectLocalModel(
            modelID: "target-model",
            executablePath: executable.path,
            modelDirectoryPath: models.path
        )
        for _ in 0 ..< 100 where await launcher.launchCount < 2 { await Task.yield() }
        #expect(lifecycle.localModelSelectionState.pending == nil)
        #expect(lifecycle.localModelSelectionErrorCode == "local_model_selection_rejected")
        #expect(try store.load()?.pending == nil)
        #expect(await launcher.commands.count == 1)
        #expect(await launcher.commands.first?.0
            == AssemblywrightDeveloperBridgeProcessLifecycle.localModelSelectionArguments)

        await lifecycle.resumePendingLocalModelSelection()
        #expect(await launcher.commands.count == 1)
        #expect(await launcher.launchCount == 2)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Exact enqueue reconciliation reuses frozen bytes and clears only on strict success")
    func approvedFeatureLifecycleReconcilesExactFrozenRequest() async {
        let line = authenticatedSnapshotData(
            connectionEpoch: 44,
            ownerControl: ownerControlData(completeEvidence: true, active: true)
        )
        let first = FakeBridgeProcessSession(lines: [line])
        let restarted = FakeBridgeProcessSession(lines: [line])
        let launcher = FakeBridgeProcessLauncher(
            session: first,
            restartSession: restarted,
            commandFailures: 1
        )
        let lifecycle = approvedFeatureLifecycle(launcher: launcher)
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.ownerControl == nil { await Task.yield() }
        let draft = sampleApprovedFeatureDraft()

        let prepared = try! draft.prepareRequest(from: lifecycle.status)
        await lifecycle.performApprovedFeatureEnqueue(prepared)
        let frozen = lifecycle.pendingApprovedFeatureRecovery
        #expect(frozen?.draft == draft)
        #expect(frozen?.exactRequestSHA256.count == 32)

        await lifecycle.reconcilePendingApprovedFeatureEnqueue()
        for _ in 0 ..< 100 where await launcher.launchCount < 3 { await Task.yield() }
        let commands = await launcher.commands

        #expect(commands.count == 2)
        #expect(commands[0].0 == AssemblywrightDeveloperBridgeProcessLifecycle
            .approvedFeatureEnqueueArguments)
        #expect(commands[0].1 == commands[1].1)
        #expect(lifecycle.approvedFeatureReceipt?.featureID == draft.featureID)
        #expect(lifecycle.pendingApprovedFeatureRecovery == nil)
        #expect(lifecycle.ownerActionErrorCode == nil)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Failed exact enqueue reconciliation retains frozen bytes")
    func approvedFeatureLifecycleRetainsRecoveryAfterReconciliationFailure() async {
        let line = authenticatedSnapshotData(
            connectionEpoch: 44,
            ownerControl: ownerControlData(completeEvidence: true, active: true)
        )
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: [line]),
            restartSession: FakeBridgeProcessSession(lines: [line]),
            commandFailures: 2
        )
        let lifecycle = approvedFeatureLifecycle(launcher: launcher)
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.ownerControl == nil { await Task.yield() }

        let prepared = try! sampleApprovedFeatureDraft().prepareRequest(from: lifecycle.status)
        await lifecycle.performApprovedFeatureEnqueue(prepared)
        let original = lifecycle.pendingApprovedFeatureRecovery
        await lifecycle.reconcilePendingApprovedFeatureEnqueue()
        let commands = await launcher.commands

        #expect(commands.count == 2)
        #expect(commands[0].1 == commands[1].1)
        #expect(lifecycle.pendingApprovedFeatureRecovery == original)
        #expect(lifecycle.approvedFeatureReceipt == nil)
        #expect(lifecycle.ownerActionErrorCode == "approved_feature_reconciliation_required")
        await lifecycle.stop()
    }

    @MainActor
    @Test("Cancelled exact enqueue reconciliation retains frozen bytes")
    func approvedFeatureLifecycleRetainsRecoveryAfterReconciliationCancellation() async {
        let line = authenticatedSnapshotData(
            connectionEpoch: 44,
            ownerControl: ownerControlData(completeEvidence: true, active: true)
        )
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: [line]),
            restartSession: FakeBridgeProcessSession(lines: [line]),
            commandFailures: 1,
            commandDelays: [.zero, .seconds(30)]
        )
        let lifecycle = approvedFeatureLifecycle(launcher: launcher)
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.ownerControl == nil { await Task.yield() }
        let prepared = try! sampleApprovedFeatureDraft().prepareRequest(from: lifecycle.status)
        await lifecycle.performApprovedFeatureEnqueue(prepared)
        let original = lifecycle.pendingApprovedFeatureRecovery

        let reconciliation = Task {
            await lifecycle.reconcilePendingApprovedFeatureEnqueue()
        }
        for _ in 0 ..< 100 where await launcher.commands.count < 2 { await Task.yield() }
        reconciliation.cancel()
        await reconciliation.value

        #expect(lifecycle.pendingApprovedFeatureRecovery == original)
        #expect(lifecycle.approvedFeatureReceipt == nil)
        #expect(lifecycle.ownerActionErrorCode == "approved_feature_reconciliation_required")
        await lifecycle.stop()
    }

    @MainActor
    @Test("Pending exact enqueue recovery blocks every new draft")
    func approvedFeatureLifecycleBlocksNewSubmissionDuringRecovery() async {
        let line = authenticatedSnapshotData(
            connectionEpoch: 44,
            ownerControl: ownerControlData(completeEvidence: true, active: true)
        )
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: [line]),
            restartSession: FakeBridgeProcessSession(lines: [line]),
            commandFailures: 1
        )
        let lifecycle = approvedFeatureLifecycle(launcher: launcher)
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.ownerControl == nil { await Task.yield() }
        let original = sampleApprovedFeatureDraft()
        let originalPrepared = try! original.prepareRequest(from: lifecycle.status)
        await lifecycle.performApprovedFeatureEnqueue(originalPrepared)
        let frozen = lifecycle.pendingApprovedFeatureRecovery
        let different = sampleApprovedFeatureDraft(
            featureID: UUID(uuidString: "dddddddd-dddd-4ddd-8ddd-dddddddddddd")!
        )

        let differentPrepared = try! different.prepareRequest(
            from: approvedFeatureAuthoringStatus()
        )
        await lifecycle.performApprovedFeatureEnqueue(differentPrepared)

        #expect(await launcher.commands.count == 1)
        #expect(lifecycle.pendingApprovedFeatureRecovery == frozen)
        #expect(lifecycle.pendingApprovedFeatureRecovery?.draft == original)
        #expect(lifecycle.ownerActionErrorCode == "approved_feature_reconciliation_required")
        await lifecycle.stop()
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

    @Test("Assembly Line projection rejects malformed, duplicate, and oversized documents")
    func assemblyLineProjectionStrictDecoding() throws {
        let valid = validAssemblyLineProjectionData()
        let decoded = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(valid)
        #expect(decoded.assemblyLine.autoRun)
        #expect(decoded.repositories.isEmpty)
        #expect(decoded.queue.isEmpty)

        var extra = try #require(JSONSerialization.jsonObject(with: valid) as? [String: Any])
        extra["legacy_feature_conveyor"] = [:]
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidProjection) {
            try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
                JSONSerialization.data(withJSONObject: extra, options: [.sortedKeys])
            )
        }
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidProjection) {
            try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
                Data(#"{"schema_version":1,"schema_version":1}"#.utf8)
            )
        }
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidProjection) {
            try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
                valid + Data(repeating: 0x20, count: 256 * 1_024)
            )
        }
    }

    @Test("Nonempty Assembly Line helper projections preserve lowercase UUIDs on round trip")
    func assemblyLineProjectionLowercaseUUIDRoundTrip() throws {
        let source = validAssemblyLineProjectionData(
            emergencyPaused: true,
            emergencyPauseRevision: 2
        )
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(source)
        let encoded = try JSONEncoder().encode(projection)
        let text = try #require(String(data: encoded, encoding: .utf8))

        #expect(!text.contains("AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA"))
        #expect(!text.contains("BBBBBBBB-BBBB-4BBB-8BBB-BBBBBBBBBBBB"))
        #expect(!text.contains("CCCCCCCC-CCCC-4CCC-8CCC-CCCCCCCCCCCC"))
        #expect(!text.contains("DDDDDDDD-DDDD-4DDD-8DDD-DDDDDDDDDDDD"))
        #expect(try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(encoded) == projection)
        try AssemblywrightMacAssemblyLineOwnerControl.validateHelperOutput(
            action: .projectDraft,
            requestData: projectDraftRequestData(projection: source),
            responseData: encoded
        )
    }

    @Test("Emergency pause permits an authoritative stopped inert Assembly Line")
    func assemblyLineEmergencyPauseAcceptsStoppedInertState() throws {
        var object = try #require(
            JSONSerialization.jsonObject(
                with: validAssemblyLineProjectionData(
                    emergencyPaused: true,
                    emergencyPauseRevision: 2
                )
            ) as? [String: Any]
        )
        var state = try #require(object["assembly_line"] as? [String: Any])
        state["lifecycle"] = "stopped"
        state["session_id"] = NSNull()
        state["active_child_epoch_id"] = NSNull()
        state["active_feature_id"] = NSNull()
        object["assembly_line"] = state
        var queue = try #require(object["queue"] as? [[String: Any]])
        queue[0]["lifecycle"] = "queued"
        object["queue"] = queue

        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        )
        #expect(projection.emergencyPaused)
        #expect(projection.assemblyLine.lifecycle == .stopped)
        #expect(projection.assemblyLine.activeFeatureID == nil)
        #expect(projection.queue.first?.lifecycle == .queued)
    }

    @Test("Assembly Line accepts Starting only with all execution components available")
    func assemblyLineStartingRequiresAvailableExecutionRuntime() throws {
        var object = try #require(
            JSONSerialization.jsonObject(
                with: validAssemblyLineProjectionData(
                    emergencyPaused: true,
                    emergencyPauseRevision: 2
                )
            ) as? [String: Any]
        )
        object["emergency_paused"] = false
        var state = try #require(object["assembly_line"] as? [String: Any])
        state["lifecycle"] = "starting"
        object["assembly_line"] = state
        var queue = try #require(object["queue"] as? [[String: Any]])
        queue[0]["lifecycle"] = "starting"
        object["queue"] = queue

        let unavailable = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidProjection) {
            try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(unavailable)
        }

        var availability = try #require(object["availability"] as? [String: Any])
        for key in ["windows_executor", "mac_executor", "protected_brokers"] {
            var component = try #require(availability[key] as? [String: Any])
            component["status"] = "available"
            component["unavailable_reason"] = NSNull()
            availability[key] = component
        }
        object["availability"] = availability
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        )
        #expect(projection.assemblyLine.lifecycle == .starting)
        #expect(projection.queue.first?.lifecycle == .starting)
    }

    @Test("Planning mutation preflights, posts, and postflights one authenticated session")
    func assemblyLinePlanningUsesOneSession() async throws {
        let prior = validAssemblyLineProjectionData(ownerControlRevision: 1)
        let post = validAssemblyLineProjectionData(ownerControlRevision: 2)
        let request = projectDraftRequestData(projection: prior)
        let session = FakeAssemblyLinePlanningSession(responses: [
            .init(status: 200, body: prior),
            .init(status: 200, body: post),
            .init(status: 200, body: post)
        ])

        let output = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
            action: .projectDraft,
            requestData: request,
            using: session
        )

        #expect(output == post)
        #expect(await session.requests.map(\.method) == ["GET", "POST", "GET"])
        #expect(await session.requests.map(\.path) == [
            AssemblywrightMacBridgeSupervisor.assemblyLinePath,
            "/v1/distributed/assembly-line/project-drafts",
            AssemblywrightMacBridgeSupervisor.assemblyLinePath
        ])
        #expect(await session.cancelled)
    }

    @Test("Project brainstorming builds a canonical request and binds the frozen response")
    func assemblyLineProjectBrainstormingHappyPath() async throws {
        let prior = validAssemblyLineProjectionData(
            ownerControlRevision: 7,
            brainstormingAvailable: true
        )
        let post = validAssemblyLineProjectionData(
            ownerControlRevision: 9,
            brainstormingAvailable: true
        )
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(prior)
        let draftID = UUID(uuidString: "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee")!
        let repositoryID = UUID(uuidString: "ffffffff-ffff-4fff-8fff-ffffffffffff")!
        let request = try AssemblywrightMacAssemblyLineOwnerControl.projectBrainstormRequest(
            from: projection,
            repositoryID: repositoryID,
            repositoryURL: "https://github.com/owner/new-project",
            visibility: .public,
            idea: "Create a bounded planning-only project",
            informationClassification: .public,
            ownerConfirmedCloudDisclosure: true,
            draftID: draftID
        )
        let frozen = frozenSpecificationData(draftRequest: request, targetKind: "project")
        let session = FakeAssemblyLinePlanningSession(responses: [
            .init(status: 200, body: prior),
            .init(status: 200, body: frozen),
            .init(status: 200, body: post)
        ])

        let output = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
            action: .projectBrainstorm,
            requestData: request,
            using: session
        )
        let decoded = try AssemblywrightMacFrozenBrainstormingSpecification.decodeStrict(
            output,
            matchingDraft: request,
            projection: projection
        )

        #expect(decoded.targetKind == .project)
        #expect(decoded.repository.repositoryID == repositoryID)
        #expect(decoded.specification.title == "Bounded project")
        #expect(await session.requests[1].path == "/v1/distributed/assembly-line/project-brainstorms")
        #expect(await session.requests[1].body == request)
    }

    @Test("Cloud brainstorming wrapper matches the Rust public-disclosure golden digest")
    func assemblyLineProjectBrainstormingCloudGolden() throws {
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            validAssemblyLineProjectionData(brainstormingAvailable: true)
        )
        let request = try AssemblywrightMacAssemblyLineOwnerControl.projectBrainstormRequest(
            from: projection,
            repositoryID: UUID(uuidString: "11111111-1111-4111-8111-111111111111")!,
            repositoryURL: "https://github.com/assemblywright/protocol-test",
            visibility: .public,
            idea: "Create a bounded project with native tests and durable documentation.",
            informationClassification: .public,
            ownerConfirmedCloudDisclosure: true,
            draftID: UUID(uuidString: "22222222-2222-4222-8222-222222222222")!
        )
        let envelope = try #require(
            JSONSerialization.jsonObject(with: request) as? [String: Any]
        )
        let disclosure = try #require(
            (envelope["owner_cloud_disclosure_sha256"] as? [NSNumber])?.map(\.uint8Value)
        )

        #expect(Set(envelope.keys) == Set([
            "schema_version", "draft", "information_classification",
            "owner_cloud_disclosure_sha256"
        ]))
        #expect(envelope["information_classification"] as? String == "public")
        #expect(disclosure.map { String(format: "%02x", $0) }.joined()
            == "295ec0847c198b1463ff9f4b0b5318f1b2276acf9c0e715bef585b3f3c06a914")
        try AssemblywrightMacAssemblyLineOwnerControl.validateStoredRequest(
            action: .projectBrainstorm,
            requestData: request
        )
    }

    @Test("Cloud brainstorming rejects non-Public classification or absent disclosure")
    func assemblyLineBrainstormingRejectsRestrictedClassification() throws {
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            validAssemblyLineProjectionData(brainstormingAvailable: true)
        )

        for classification in [
            AssemblywrightMacPlanningInformationClassification.private,
            .restricted,
            .secret
        ] {
            #expect(throws: AssemblywrightMacAssemblyLineError.invalidRequest) {
                _ = try AssemblywrightMacAssemblyLineOwnerControl.projectBrainstormRequest(
                    from: projection,
                    repositoryURL: "https://github.com/owner/project",
                    visibility: .public,
                    idea: "Public-looking input cannot override an owner classification",
                    informationClassification: classification,
                    ownerConfirmedCloudDisclosure: true
                )
            }
        }
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidRequest) {
            _ = try AssemblywrightMacAssemblyLineOwnerControl.projectBrainstormRequest(
                from: projection,
                repositoryURL: "https://github.com/owner/project",
                visibility: .public,
                idea: "Public information still requires explicit owner disclosure",
                informationClassification: .public,
                ownerConfirmedCloudDisclosure: false
            )
        }

        let valid = try AssemblywrightMacAssemblyLineOwnerControl.projectBrainstormRequest(
            from: projection,
            repositoryURL: "https://github.com/owner/project",
            visibility: .public,
            idea: "Public information with owner disclosure",
            informationClassification: .public,
            ownerConfirmedCloudDisclosure: true
        )
        var tampered = try #require(
            JSONSerialization.jsonObject(with: valid) as? [String: Any]
        )
        tampered["information_classification"] = "restricted"
        let tamperedData = try JSONSerialization.data(
            withJSONObject: tampered,
            options: [.sortedKeys, .withoutEscapingSlashes]
        )
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidRequest) {
            try AssemblywrightMacAssemblyLineOwnerControl.validateStoredRequest(
                action: .projectBrainstorm,
                requestData: tamperedData
            )
        }

        var resignedEnvelope = try #require(
            JSONSerialization.jsonObject(with: valid) as? [String: Any]
        )
        var resignedDraft = try #require(resignedEnvelope["draft"] as? [String: Any])
        resignedDraft["unexpected"] = true
        let catalog = try #require(resignedDraft["orchestrator_catalog"] as? [String: Any])
        let profile = try #require(resignedDraft["orchestrator"] as? [String: Any])
        let canonical: (Any) throws -> Data = {
            try JSONSerialization.data(
                withJSONObject: $0,
                options: [.sortedKeys, .withoutEscapingSlashes]
            )
        }
        let binding: [String: Any] = [
            "schema_version": 1,
            "target_kind": "project",
            "draft_sha256": Array(SHA256.hash(data: try canonical(resignedDraft))),
            "information_classification": "public",
            "provider_id": profile["provider_id"]!,
            "model_id": profile["model_id"]!,
            "orchestrator_catalog_revision": catalog["catalog_revision"]!,
            "orchestrator_catalog_sha256": catalog["catalog_sha256"]!,
            "orchestrator_profile_sha256": Array(
                SHA256.hash(data: try canonical(profile))
            )
        ]
        var preimage = Data("assemblywright.owner-cloud-disclosure.v1\0".utf8)
        preimage.append(try canonical(binding))
        resignedEnvelope["draft"] = resignedDraft
        resignedEnvelope["owner_cloud_disclosure_sha256"] = Array(
            SHA256.hash(data: preimage)
        )
        let resignedData = try canonical(resignedEnvelope)
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidRequest) {
            try AssemblywrightMacAssemblyLineOwnerControl.validateStoredRequest(
                action: .projectBrainstorm,
                requestData: resignedData
            )
        }
    }

    @Test("Feature brainstorming and owner approval bind authoritative revisions")
    func assemblyLineFeatureBrainstormAndApprovalBinding() throws {
        let repositoryID = UUID(uuidString: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")!
        let source = assemblyLineProjectionWithPendingRepository(
            repositoryID: repositoryID,
            created: true,
            brainstormingAvailable: true
        )
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(source)
        let draft = try AssemblywrightMacAssemblyLineOwnerControl.featureBrainstormRequest(
            from: projection,
            repositoryURL: "https://github.com/owner/project",
            idea: "Add a bounded owner-reviewed feature",
            informationClassification: .public,
            ownerConfirmedCloudDisclosure: true,
            draftID: UUID(uuidString: "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee")!
        )
        let frozen = try AssemblywrightMacFrozenBrainstormingSpecification.decodeStrict(
            frozenSpecificationData(draftRequest: draft, targetKind: "feature"),
            matchingDraft: draft,
            projection: projection
        )
        let approval = try AssemblywrightMacAssemblyLineOwnerControl.ownerApprovalRequest(
            for: frozen,
            from: projection,
            approvalID: UUID(uuidString: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")!,
            approvedAtMilliseconds: 1_000
        )
        let object = try #require(
            JSONSerialization.jsonObject(with: approval) as? [String: Any]
        )
        var binding = object
        binding.removeValue(forKey: "owner_approval_sha256")
        let canonical = try JSONSerialization.data(
            withJSONObject: binding,
            options: [.sortedKeys, .withoutEscapingSlashes]
        )

        #expect(object["target_kind"] as? String == "feature")
        #expect(object["visibility"] is NSNull)
        #expect((object["expected_repository_revision"] as? NSNumber)?.uint64Value == 1)
        #expect((object["expected_queue_revision"] as? NSNumber)?.uint64Value == 0)
        let approvalDigest = (object["owner_approval_sha256"] as? [NSNumber])?.map(\.uint8Value)
        #expect(approvalDigest == Array(SHA256.hash(data: canonical)))
    }

    @Test("Brainstorming rejects malformed, oversized, stale, and ambiguous outcomes")
    func assemblyLineBrainstormingNegativePaths() async throws {
        let prior = validAssemblyLineProjectionData(
            ownerControlRevision: 3,
            brainstormingAvailable: true
        )
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(prior)
        let request = try AssemblywrightMacAssemblyLineOwnerControl.projectBrainstormRequest(
            from: projection,
            repositoryID: UUID(uuidString: "ffffffff-ffff-4fff-8fff-ffffffffffff")!,
            repositoryURL: "https://github.com/owner/new-project",
            visibility: .private,
            idea: "Create a bounded planning-only project",
            informationClassification: .public,
            ownerConfirmedCloudDisclosure: true
        )
        let frozen = frozenSpecificationData(draftRequest: request, targetKind: "project")
        var malformed = try #require(JSONSerialization.jsonObject(with: frozen) as? [String: Any])
        malformed["draft_sha256"] = Array(repeating: UInt8(0x99), count: 32)
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidReceipt) {
            try AssemblywrightMacFrozenBrainstormingSpecification.decodeStrict(
                JSONSerialization.data(withJSONObject: malformed, options: [.sortedKeys]),
                matchingDraft: request,
                projection: projection
            )
        }
        #expect(throws: AssemblywrightMacAssemblyLineError.requestTooLarge) {
            _ = try AssemblywrightMacAssemblyLineOwnerControl.projectBrainstormRequest(
                from: projection,
                repositoryID: UUID(),
                repositoryURL: "https://github.com/owner/oversized",
                visibility: .public,
                idea: String(repeating: "a", count: 16 * 1_024 + 1),
                informationClassification: .public,
                ownerConfirmedCloudDisclosure: true
            )
        }

        let stalePost = validAssemblyLineProjectionData(
            ownerControlRevision: 4,
            brainstormingAvailable: true
        )
        let staleSession = FakeAssemblyLinePlanningSession(responses: [
            .init(status: 200, body: prior),
            .init(status: 200, body: frozen),
            .init(status: 200, body: stalePost)
        ])
        await #expect(throws: AssemblywrightMacAssemblyLineError.outcomeUnknown) {
            _ = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
                action: .projectBrainstorm,
                requestData: request,
                using: staleSession
            )
        }

        let ambiguous = FakeAssemblyLinePlanningSession(responses: [
            .init(status: 200, body: prior),
            .init(
                status: 409,
                body: Data(#"{"error":"brainstorming_reconciliation_required"}"#.utf8)
            )
        ])
        await #expect(throws: AssemblywrightMacAssemblyLineError.outcomeUnknown) {
            _ = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
                action: .projectBrainstorm,
                requestData: request,
                using: ambiguous
            )
        }
    }

    @Test("Repository creation derives a lowercase route and sends an empty body")
    func assemblyLineRepositoryCreationExactRoute() async throws {
        let repositoryID = UUID(uuidString: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")!
        let prior = assemblyLineProjectionWithPendingRepository(repositoryID: repositoryID)
        let created = repositoryCreationProjectionData(repositoryID: repositoryID, created: true)
        let post = assemblyLineProjectionWithPendingRepository(
            repositoryID: repositoryID,
            created: true
        )
        let request = try AssemblywrightMacAssemblyLineOwnerControl.repositoryCreationRequest(
            repositoryID: repositoryID
        )
        let session = FakeAssemblyLinePlanningSession(responses: [
            .init(status: 200, body: prior),
            .init(status: 200, body: created),
            .init(status: 200, body: post)
        ])

        _ = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
            action: .repositoryCreation,
            requestData: request,
            using: session
        )

        #expect(await session.requests[1].path ==
            "/v1/distributed/assembly-line/repositories/bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb/create")
        #expect(await session.requests[1].body == Data())
    }

    @Test("Fixed pre-effect planning errors reject while reconciliation remains ambiguous")
    func assemblyLinePlanningFixedErrorDisposition() async throws {
        let prior = validAssemblyLineProjectionData(
            ownerControlRevision: 3,
            brainstormingAvailable: true
        )
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(prior)
        let request = try AssemblywrightMacAssemblyLineOwnerControl.projectBrainstormRequest(
            from: projection,
            repositoryURL: "https://github.com/owner/new-project",
            visibility: .public,
            idea: "Create a bounded project",
            informationClassification: .public,
            ownerConfirmedCloudDisclosure: true
        )
        for (status, code) in [
            (401, "unauthorized"),
            (413, "payload_too_large"),
            (422, "assembly_line_request_rejected"),
            (409, "brainstorming_rejected"),
            (503, "planning_runtime_unavailable")
        ] {
            let body = try JSONSerialization.data(withJSONObject: ["error": code])
            let session = FakeAssemblyLinePlanningSession(responses: [
                .init(status: 200, body: prior), .init(status: status, body: body)
            ])
            await #expect(throws: AssemblywrightMacAssemblyLineError.rejected) {
                _ = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
                    action: .projectBrainstorm,
                    requestData: request,
                    using: session
                )
            }
        }
    }

    @Test("Repository creation distinguishes fixed pre-effect errors from reconciliation")
    func assemblyLineRepositoryCreationErrorDisposition() async throws {
        let repositoryID = UUID(uuidString: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")!
        let prior = assemblyLineProjectionWithPendingRepository(repositoryID: repositoryID)
        let request = try AssemblywrightMacAssemblyLineOwnerControl.repositoryCreationRequest(
            repositoryID: repositoryID
        )
        for (status, code) in [
            (409, "github_creation_conflict"),
            (503, "github_creation_unavailable")
        ] {
            let session = FakeAssemblyLinePlanningSession(responses: [
                .init(status: 200, body: prior),
                .init(
                    status: status,
                    body: try JSONSerialization.data(withJSONObject: ["error": code])
                )
            ])
            await #expect(throws: AssemblywrightMacAssemblyLineError.rejected) {
                _ = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
                    action: .repositoryCreation,
                    requestData: request,
                    using: session
                )
            }
        }

        let reconciliation = FakeAssemblyLinePlanningSession(responses: [
            .init(status: 200, body: prior),
            .init(
                status: 409,
                body: Data(#"{"error":"github_creation_reconciliation_required"}"#.utf8)
            )
        ])
        await #expect(throws: AssemblywrightMacAssemblyLineError.outcomeUnknown) {
            _ = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
                action: .repositoryCreation,
                requestData: request,
                using: reconciliation
            )
        }
    }

    @Test("Planning rejects stale bindings and accepts only an exact replay")
    func assemblyLinePlanningStaleAndReplay() async throws {
        let prior = validAssemblyLineProjectionData(ownerControlRevision: 1)
        var stale = try #require(
            JSONSerialization.jsonObject(with: projectDraftRequestData(projection: prior))
                as? [String: Any]
        )
        var catalog = try #require(stale["orchestrator_catalog"] as? [String: Any])
        catalog["catalog_revision"] = 2
        stale["orchestrator_catalog"] = catalog
        let staleSession = FakeAssemblyLinePlanningSession(responses: [
            .init(status: 200, body: prior)
        ])
        await #expect(throws: AssemblywrightMacAssemblyLineError.invalidRequest) {
            _ = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
                action: .projectDraft,
                requestData: JSONSerialization.data(withJSONObject: stale, options: [.sortedKeys]),
                using: staleSession
            )
        }
        #expect(await staleSession.requests.map(\.path) == [
            AssemblywrightMacBridgeSupervisor.assemblyLinePath
        ])
        #expect(await staleSession.cancelled)

        let replayProjection = validAssemblyLineProjectionData(ownerControlRevision: 2)
        let exactRequest = projectDraftRequestData(projection: prior)
        let replaySession = FakeAssemblyLinePlanningSession(responses: [
            .init(status: 200, body: replayProjection),
            .init(status: 200, body: replayProjection),
            .init(status: 200, body: replayProjection)
        ])
        _ = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
            action: .projectDraft,
            requestData: exactRequest,
            using: replaySession
        )
        #expect(await replaySession.requests.count == 3)

        let driftedResponse = validAssemblyLineProjectionData(ownerControlRevision: 3)
        let driftSession = FakeAssemblyLinePlanningSession(responses: [
            .init(status: 200, body: prior),
            .init(status: 200, body: driftedResponse),
            .init(status: 200, body: replayProjection)
        ])
        await #expect(throws: AssemblywrightMacAssemblyLineError.outcomeUnknown) {
            _ = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
                action: .projectDraft,
                requestData: exactRequest,
                using: driftSession
            )
        }
    }

    @Test("Planning input failures reap the authenticated session before any request")
    func assemblyLinePlanningInputFailureCancelsSession() async {
        let duplicate = Data(
            #"{"schema_version":1,"schema_version":1}"#.utf8
        )
        let oversized = Data(repeating: 0x61, count: 16 * 1_024 + 1)

        for (input, expected) in [
            (duplicate, AssemblywrightMacAssemblyLineError.invalidRequest),
            (oversized, AssemblywrightMacAssemblyLineError.requestTooLarge)
        ] {
            let session = FakeAssemblyLinePlanningSession(responses: [])
            await #expect(throws: expected) {
                _ = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
                    action: .projectDraft,
                    requestData: input,
                    using: session
                )
            }
            #expect(await session.requests.isEmpty)
            #expect(await session.cancelled)
        }
    }

    @Test("Assembly Line helper output binds the fixed request and exact response shape")
    func assemblyLineHelperOutputExactBinding() throws {
        let requestID = "11111111-1111-4111-8111-111111111111"
        let request = try JSONSerialization.data(withJSONObject: [
            "schema_version": 1,
            "request_id": requestID,
            "expected_state_revision": 1,
            "auto_run": false
        ], options: [.sortedKeys])
        let projection = try #require(
            JSONSerialization.jsonObject(with: validAssemblyLineProjectionData())
                as? [String: Any]
        )
        var state = try #require(projection["assembly_line"] as? [String: Any])
        state["state_revision"] = 2
        state["auto_run"] = false
        let response = try JSONSerialization.data(withJSONObject: [
            "schema_version": 1,
            "request_id": requestID,
            "resulting_state": state
        ], options: [.sortedKeys])
        try AssemblywrightMacAssemblyLineOwnerControl.validateHelperOutput(
            action: .autoRun,
            requestData: request,
            responseData: response
        )

        var drifted = try #require(
            JSONSerialization.jsonObject(with: response) as? [String: Any]
        )
        drifted["request_id"] = "22222222-2222-4222-8222-222222222222"
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidReceipt) {
            try AssemblywrightMacAssemblyLineOwnerControl.validateHelperOutput(
                action: .autoRun,
                requestData: request,
                responseData: JSONSerialization.data(
                    withJSONObject: drifted,
                    options: [.sortedKeys]
                )
            )
        }
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidReceipt) {
            try AssemblywrightMacAssemblyLineOwnerControl.validateHelperOutput(
                action: .autoRun,
                requestData: request,
                responseData: Data(
                    String(data: response, encoding: .utf8)!.replacingOccurrences(
                        of: "\"schema_version\":1",
                        with: "\"schema_version\":1,\"schema_version\":1"
                    ).utf8
                )
            )
        }
    }

    @Test("Assembly Line auto-run request is built from the authoritative revision")
    func assemblyLineAutoRunRequestUsesAuthoritativeRevision() throws {
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            validAssemblyLineProjectionData()
        )
        let requestID = UUID(uuidString: "11111111-1111-4111-8111-111111111111")!
        let data = try AssemblywrightMacAssemblyLineOwnerControl.autoRunRequest(
            from: projection,
            enabled: false,
            requestID: requestID
        )
        let object = try #require(
            JSONSerialization.jsonObject(with: data) as? [String: Any]
        )

        #expect(Set(object.keys) == [
            "schema_version", "request_id", "expected_state_revision", "auto_run"
        ])
        #expect(object["schema_version"] as? Int == 1)
        #expect(object["request_id"] as? String == requestID.uuidString.lowercased())
        #expect(object["expected_state_revision"] as? Int == 1)
        #expect(object["auto_run"] as? Bool == false)
    }

    @Test("Pending Assembly Line store is owner-private, strict, hashed, and symlink-safe")
    func assemblyLinePendingStoreRejectsHostileFiles() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString.lowercased(), isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = AssemblywrightMacAssemblyLinePendingMutationStore(
            fileURL: root.appendingPathComponent("pending.json")
        )
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            validAssemblyLineProjectionData()
        )
        let request = try AssemblywrightMacAssemblyLineOwnerControl.autoRunRequest(
            from: projection,
            enabled: false
        )
        let mutation = try AssemblywrightMacPendingAssemblyLinePlanningMutation(
            action: .autoRun,
            requestData: request
        )
        try store.save(mutation)
        #expect(try store.load() == mutation)
        let attributes = try FileManager.default.attributesOfItem(atPath: store.fileURL.path)
        #expect((attributes[.posixPermissions] as? NSNumber)?.intValue == 0o600)

        try FileManager.default.setAttributes(
            [.posixPermissions: 0o644],
            ofItemAtPath: store.fileURL.path
        )
        #expect(throws: AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore) {
            try store.load()
        }
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: store.fileURL.path
        )

        var corrupt = try #require(
            JSONSerialization.jsonObject(with: Data(contentsOf: store.fileURL))
                as? [String: Any]
        )
        corrupt["request_sha256"] = Array(repeating: UInt8(0x44), count: 32)
        try JSONSerialization.data(withJSONObject: corrupt, options: [.sortedKeys])
            .write(to: store.fileURL)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: store.fileURL.path
        )
        #expect(throws: AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore) {
            try store.load()
        }

        try store.save(mutation)
        let saved = try String(contentsOf: store.fileURL, encoding: .utf8)
        let duplicate = saved.replacingOccurrences(
            of: "\"schema_version\":1",
            with: "\"schema_version\":1,\"schema_version\":1"
        )
        try Data(duplicate.utf8).write(to: store.fileURL)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: store.fileURL.path
        )
        #expect(throws: AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore) {
            try store.load()
        }

        let target = root.appendingPathComponent("target.json")
        try store.save(mutation)
        try FileManager.default.moveItem(at: store.fileURL, to: target)
        try FileManager.default.createSymbolicLink(at: store.fileURL, withDestinationURL: target)
        #expect(throws: AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore) {
            try store.load()
        }
        try FileManager.default.removeItem(at: target)
        #expect(throws: AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore) {
            try store.load()
        }
    }

    @Test("Pending Assembly Line validation rejects Rust-denied secret and path shapes")
    func assemblyLinePendingStoreSecretParity() throws {
        let projectionData = validAssemblyLineProjectionData()
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            projectionData
        )
        let original = try #require(
            JSONSerialization.jsonObject(
                with: projectDraftRequestData(projection: projectionData)
            ) as? [String: Any]
        )
        for idea in [
            "token=github_pat_forbidden",
            "embedded GHP_12345678901234567890123456789012",
            "embedded SK-1234567890abcdefg",
            "access AKIA1234567890ABCDEF12",
            "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.signature1234",
            "endpoint https://owner:password@example.invalid/path",
            "read FILE:///tmp/secret",
            "clone SSH://github.com/owner/repo",
            "clone git@github.com:owner/repo",
            "read /private/owner/secret",
            "client secret = forbidden",
            "-----BEGIN PRIVATE KEY----- abc"
        ] {
            var request = original
            request["idea"] = idea
            let data = try JSONSerialization.data(
                withJSONObject: request,
                options: [.sortedKeys]
            )
            #expect(throws: AssemblywrightMacAssemblyLineError.invalidRequest) {
                try AssemblywrightMacAssemblyLineOwnerControl.validateRequest(
                    action: .projectDraft,
                    requestData: data,
                    against: projection
                )
            }
            #expect(throws: AssemblywrightMacAssemblyLineError.invalidRequest) {
                try AssemblywrightMacAssemblyLineOwnerControl.validateStoredRequest(
                    action: .projectDraft,
                    requestData: data
                )
            }
        }
    }

    @MainActor
    @Test("Ambiguous Assembly Line mutations persist and reconcile only exact bytes")
    func assemblyLineLifecycleExactReconciliation() async throws {
        let storeRoot = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString.lowercased(), isDirectory: true)
        defer { try? FileManager.default.removeItem(at: storeRoot) }
        let pendingStore = AssemblywrightMacAssemblyLinePendingMutationStore(
            fileURL: storeRoot.appendingPathComponent("pending.json")
        )
        let line = authenticatedSnapshotData(connectionEpoch: 202)
        let first = FakeBridgeProcessSession(lines: [line])
        let restarted = FakeBridgeProcessSession(lines: [line])
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            validAssemblyLineProjectionData()
        )
        let request = try AssemblywrightMacAssemblyLineOwnerControl.autoRunRequest(
            from: projection,
            enabled: false,
            requestID: UUID(uuidString: "11111111-1111-4111-8111-111111111111")!
        )
        let launcher = FakeBridgeProcessLauncher(
            session: first,
            restartSession: restarted,
            commandResponse: autoRunAssemblyLineReceiptData(request: request),
            commandFailures: 1
        )
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher,
            assemblyLinePendingMutationStore: pendingStore
        )
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.assemblyLine == nil { await Task.yield() }

        await lifecycle.performAssemblyLinePlanningAction(.autoRun, requestData: request)
        for _ in 0 ..< 100 where await launcher.launchCount < 2 { await Task.yield() }
        #expect(lifecycle.pendingAssemblyLinePlanningAction == .autoRun)
        #expect(lifecycle.ownerActionErrorCode == "assembly_line_reconciliation_required")

        let blocked = try AssemblywrightMacAssemblyLineOwnerControl.autoRunRequest(
            from: projection,
            enabled: false,
            requestID: UUID(uuidString: "22222222-2222-4222-8222-222222222222")!
        )
        await lifecycle.performAssemblyLinePlanningAction(.autoRun, requestData: blocked)
        #expect(await launcher.commands.count == 1)
        #expect(lifecycle.ownerActionErrorCode == "assembly_line_reconciliation_required")

        await lifecycle.reconcilePendingAssemblyLinePlanningMutation()
        for _ in 0 ..< 100 where await launcher.launchCount < 3 { await Task.yield() }
        let commands = await launcher.commands
        #expect(commands.count == 2)
        #expect(commands[0].0 == AssemblywrightMacAssemblyLinePlanningAction.autoRun.helperArguments)
        #expect(commands[1].0 == commands[0].0)
        #expect(commands[0].1 == request)
        #expect(commands[1].1 == request)
        #expect(lifecycle.pendingAssemblyLinePlanningAction == nil)
        #expect(lifecycle.ownerActionErrorCode == nil)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Proven pre-effect helper rejection clears pending and permits an edited retry")
    func assemblyLineLifecyclePreEffectRejectionClearsRecovery() async throws {
        let storeRoot = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString.lowercased(), isDirectory: true)
        defer { try? FileManager.default.removeItem(at: storeRoot) }
        let pendingStore = AssemblywrightMacAssemblyLinePendingMutationStore(
            fileURL: storeRoot.appendingPathComponent("pending.json")
        )
        let line = authenticatedSnapshotData(connectionEpoch: 204)
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            validAssemblyLineProjectionData()
        )
        let rejected = try AssemblywrightMacAssemblyLineOwnerControl.autoRunRequest(
            from: projection,
            enabled: false,
            requestID: UUID(uuidString: "11111111-1111-4111-8111-111111111111")!
        )
        let edited = try AssemblywrightMacAssemblyLineOwnerControl.autoRunRequest(
            from: projection,
            enabled: false,
            requestID: UUID(uuidString: "22222222-2222-4222-8222-222222222222")!
        )
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: [line]),
            restartSession: FakeBridgeProcessSession(lines: [line]),
            commandResponse: autoRunAssemblyLineReceiptData(request: edited),
            commandErrors: [.commandRejectedBeforeEffect]
        )
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher,
            assemblyLinePendingMutationStore: pendingStore
        )
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.assemblyLine == nil { await Task.yield() }

        await lifecycle.performAssemblyLinePlanningAction(.autoRun, requestData: rejected)
        for _ in 0 ..< 100 where await launcher.launchCount < 2 { await Task.yield() }
        #expect(lifecycle.pendingAssemblyLinePlanningAction == nil)
        #expect(lifecycle.ownerActionErrorCode == "assembly_line_action_rejected")
        #expect(!FileManager.default.fileExists(atPath: pendingStore.fileURL.path))

        await lifecycle.performAssemblyLinePlanningAction(.autoRun, requestData: edited)
        let commands = await launcher.commands
        #expect(commands.count == 2)
        #expect(commands[0].1 == rejected)
        #expect(commands[1].1 == edited)
        #expect(lifecycle.pendingAssemblyLinePlanningAction == nil)
        #expect(lifecycle.ownerActionErrorCode == nil)
        await lifecycle.stop()
    }

    @MainActor
    @Test("Cancelled post-stdin Assembly Line command retains exact reconciliation action")
    func assemblyLineLifecycleCancellationRetainsRecovery() async throws {
        let storeRoot = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString.lowercased(), isDirectory: true)
        defer { try? FileManager.default.removeItem(at: storeRoot) }
        let pendingStore = AssemblywrightMacAssemblyLinePendingMutationStore(
            fileURL: storeRoot.appendingPathComponent("pending.json")
        )
        let line = authenticatedSnapshotData(connectionEpoch: 203)
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            validAssemblyLineProjectionData()
        )
        let request = try AssemblywrightMacAssemblyLineOwnerControl.autoRunRequest(
            from: projection,
            enabled: false
        )
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: [line]),
            restartSession: FakeBridgeProcessSession(lines: [line]),
            commandResponse: autoRunAssemblyLineReceiptData(request: request),
            commandDelay: .seconds(30)
        )
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher,
            assemblyLinePendingMutationStore: pendingStore
        )
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.assemblyLine == nil { await Task.yield() }

        let operation = Task {
            await lifecycle.performAssemblyLinePlanningAction(.autoRun, requestData: request)
        }
        for _ in 0 ..< 100 where await launcher.commands.isEmpty { await Task.yield() }
        operation.cancel()
        _ = await operation.value
        for _ in 0 ..< 100 where await launcher.launchCount < 2 { await Task.yield() }

        #expect(lifecycle.pendingAssemblyLinePlanningAction == .autoRun)
        #expect(lifecycle.ownerActionErrorCode == "assembly_line_reconciliation_required")
        #expect(await launcher.commands.first?.1 == request)
        await lifecycle.stop()

        let recoveredLauncher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: [line]),
            commandResponse: autoRunAssemblyLineReceiptData(request: request)
        )
        let recovered = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: recoveredLauncher,
            assemblyLinePendingMutationStore: pendingStore
        )
        #expect(recovered.pendingAssemblyLinePlanningAction == .autoRun)
        #expect(recovered.ownerActionErrorCode == "assembly_line_reconciliation_required")
        await recovered.reconcilePendingAssemblyLinePlanningMutation()
        #expect(await recoveredLauncher.commands.first?.1 == request)
        #expect(recovered.pendingAssemblyLinePlanningAction == nil)
        #expect(!FileManager.default.fileExists(atPath: pendingStore.fileURL.path))
        await recovered.stop()
    }

    @MainActor
    @Test("Project approval immediately persists and invokes repository creation")
    func assemblyLineProjectApprovalChainsCreation() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString.lowercased(), isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let pendingStore = AssemblywrightMacAssemblyLinePendingMutationStore(
            fileURL: root.appendingPathComponent("pending.json")
        )
        let projectionData = validAssemblyLineProjectionData(
            ownerControlRevision: 5,
            brainstormingAvailable: true,
            githubCreationAvailable: true
        )
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            projectionData
        )
        let draft = try AssemblywrightMacAssemblyLineOwnerControl.projectBrainstormRequest(
            from: projection,
            repositoryID: UUID(uuidString: "ffffffff-ffff-4fff-8fff-ffffffffffff")!,
            repositoryURL: "https://github.com/owner/new-project",
            visibility: .public,
            idea: "Create a bounded project",
            informationClassification: .public,
            ownerConfirmedCloudDisclosure: true
        )
        let frozen = try AssemblywrightMacFrozenBrainstormingSpecification.decodeStrict(
            frozenSpecificationData(draftRequest: draft, targetKind: "project"),
            matchingDraft: draft,
            projection: projection
        )
        let approval = try AssemblywrightMacAssemblyLineOwnerControl.ownerApprovalRequest(
            for: frozen,
            from: projection,
            approvalID: UUID(uuidString: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")!,
            approvedAtMilliseconds: 1_000
        )
        let creationUnavailable = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            validAssemblyLineProjectionData(
                ownerControlRevision: 5,
                brainstormingAvailable: true,
                githubCreationAvailable: false
            )
        )
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidRequest) {
            try AssemblywrightMacAssemblyLineOwnerControl.validateRequest(
                action: .projectApproval,
                requestData: approval,
                against: creationUnavailable
            )
        }
        let paused = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            validAssemblyLineProjectionData(
                emergencyPaused: true,
                emergencyPauseRevision: 1,
                ownerControlRevision: 5,
                brainstormingAvailable: true,
                githubCreationAvailable: true
            )
        )
        #expect(throws: AssemblywrightMacAssemblyLineError.invalidRequest) {
            try AssemblywrightMacAssemblyLineOwnerControl.validateRequest(
                action: .projectApproval,
                requestData: approval,
                against: paused
            )
        }
        let pendingReceipt = projectApprovalReceiptData(approvalRequest: approval)
        let pendingObject = try #require(
            JSONSerialization.jsonObject(with: pendingReceipt) as? [String: Any]
        )
        var createdObject = pendingObject
        createdObject["lifecycle_revision"] = 2
        createdObject["lifecycle"] = "created"
        createdObject["effect_possible"] = true
        createdObject["creation_evidence_sha256"] = Array(repeating: UInt8(0x77), count: 32)
        let createdReceipt = try JSONSerialization.data(
            withJSONObject: createdObject,
            options: [.sortedKeys]
        )
        let line = authenticatedSnapshotData(
            connectionEpoch: 205,
            assemblyLine: projectionData
        )
        let launcher = FakeBridgeProcessLauncher(
            session: FakeBridgeProcessSession(lines: [line]),
            restartSession: FakeBridgeProcessSession(lines: [line]),
            commandResponses: [pendingReceipt, createdReceipt]
        )
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: .init(environment: [
                AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                    "/tmp/assemblywright-mac-bridge",
                AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                    "ABCDEFGHIJ"
            ]),
            validator: FakeBridgeExecutableValidator(),
            launcher: launcher,
            assemblyLinePendingMutationStore: pendingStore
        )
        lifecycle.start()
        for _ in 0 ..< 100 where lifecycle.status.assemblyLine == nil { await Task.yield() }

        let result = await lifecycle.performAssemblyLinePlanningAction(
            .projectApproval,
            requestData: approval
        )
        let commands = await launcher.commands

        #expect(result == createdReceipt)
        #expect(commands.map(\.0) == [
            AssemblywrightMacAssemblyLinePlanningAction.projectApproval.helperArguments,
            AssemblywrightMacAssemblyLinePlanningAction.repositoryCreation.helperArguments
        ])
        #expect(commands[0].1 == approval)
        let expectedCreation = try AssemblywrightMacAssemblyLineOwnerControl
            .repositoryCreationRequest(repositoryID: frozen.repository.repositoryID)
        #expect(commands[1].1 == expectedCreation)
        #expect(lifecycle.pendingAssemblyLinePlanningAction == nil)
        #expect(!FileManager.default.fileExists(atPath: pendingStore.fileURL.path))
        await lifecycle.stop()
    }

    @Test("Assembly Line helper argv is closed and excludes execution controls")
    func assemblyLineHelperArgumentAllowlist() async {
        let arguments = AssemblywrightMacAssemblyLinePlanningAction.allCases.map(\.helperArguments)
        #expect(Set(arguments.map { $0.joined(separator: "\u{0}") }).count == 9)
        #expect(arguments.allSatisfy { $0.last == "--confirm" && $0.first == "assembly-line" })
        #expect(arguments.flatMap { $0 }.allSatisfy {
            !["start", "stop", "emergency-pause", "url", "path", "body"].contains($0)
        })
        await #expect(throws: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot) {
            _ = try await FoundationAssemblywrightDeveloperBridgeProcessLauncher().runCommand(
                executable: AssemblywrightDeveloperBridgeValidatedExecutable(
                    executableURL: URL(fileURLWithPath: "/bin/false"),
                    teamIdentifier: "ABCDEFGHIJ",
                    codeRequirement: "anchor apple generic",
                    cdHash: Data(repeating: 0x11, count: 20)
                ),
                arguments: ["assembly-line", "start", "--confirm"],
                input: Data("{}".utf8)
            )
        }
    }

    @Test("Assembly Line signed-helper exit dispositions preserve the effect boundary")
    func assemblyLineHelperExitDispositionBoundary() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString.lowercased(), isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let projection = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(
            validAssemblyLineProjectionData()
        )
        let request = try AssemblywrightMacAssemblyLineOwnerControl.autoRunRequest(
            from: projection,
            enabled: false
        )
        let launcher = FoundationAssemblywrightDeveloperBridgeProcessLauncher(
            runningProcessValidator: FakeBridgeRunningProcessValidator()
        )

        for (name, exitStatus, expected) in [
            (
                "rejected",
                AssemblywrightMacAssemblyLineHelperExitStatus.rejectedBeforeEffect,
                AssemblywrightDeveloperBridgeProcessError.commandRejectedBeforeEffect
            ),
            (
                "unknown",
                AssemblywrightMacAssemblyLineHelperExitStatus.outcomeUnknown,
                AssemblywrightDeveloperBridgeProcessError.commandOutcomeUnknown
            )
        ] {
            let executable = root.appendingPathComponent(name)
            let script = "#!/bin/sh\n/bin/cat >/dev/null\nexit \(exitStatus)\n"
            try Data(script.utf8).write(to: executable, options: .atomic)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o700],
                ofItemAtPath: executable.path
            )
            let validated = AssemblywrightDeveloperBridgeValidatedExecutable(
                executableURL: executable,
                teamIdentifier: "ABCDEFGHIJ",
                codeRequirement: "anchor apple generic",
                cdHash: Data(repeating: 0x11, count: 20)
            )
            await #expect(throws: expected) {
                _ = try await launcher.runCommand(
                    executable: validated,
                    arguments: AssemblywrightMacAssemblyLinePlanningAction.autoRun.helperArguments,
                    input: request
                )
            }
        }
    }

    @Test("Assembly Line observation failure never substitutes the legacy conveyor")
    func assemblyLineObservationDoesNotUseLegacyConveyor() async {
        let session = FakeSupervisorSession(
            connectionEpoch: 201,
            outcomes: [.response(.init(status: 200, body: validRemoteHealthData()))],
            assemblyLineOutcome: .response(.init(status: 200, body: Data("{}".utf8)))
        )
        let relay = FakeBridgeEventRelay(routingMode: .fixture)
        let supervisor = AssemblywrightMacBridgeSupervisor(
            profile: staleFixtureProfile(),
            connector: FakeSupervisorConnector(sessions: [session]),
            eventRelay: relay
        )

        let snapshot = await supervisor.sample()

        #expect(snapshot.phase == .backingOff)
        #expect(snapshot.errorCode == "invalid_assembly_line")
        #expect(snapshot.featureConveyor == nil)
        #expect(snapshot.assemblyLine == nil)
        #expect(await relay.epochs.isEmpty)
        #expect(await session.requests.map(\.path) == [
            AssemblywrightMacBridgeSupervisor.healthPath,
            AssemblywrightMacBridgeSupervisor.featureConveyorPath,
            AssemblywrightMacBridgeSupervisor.ownerControlPath,
            AssemblywrightMacBridgeSupervisor.assemblyLinePath
        ])
        #expect(await session.cancelled)
    }

    @Test("Authenticated agent client outlives the bounded Rust peer identity decision")
    func authenticatedAgentClientTimeoutMatchesRustContract() {
        #expect(
            DarwinAssemblywrightUnixSocketTransport.authenticatedPeerRequestTimeoutSeconds == 47
        )
        #expect(
            DarwinAssemblywrightUnixSocketTransport.authenticatedPeerRequestTimeoutSeconds
                < DarwinAssemblywrightUnixSocketTransport.maximumTimeoutSeconds
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

private func privateTemporaryDirectory(prefix: String) throws -> URL {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("\(prefix)-\(UUID().uuidString.lowercased())", isDirectory: true)
    try FileManager.default.createDirectory(
        at: root,
        withIntermediateDirectories: false,
        attributes: [.posixPermissions: 0o700]
    )
    return root.resolvingSymlinksInPath().standardizedFileURL
}

private func executableFixture(in root: URL) throws -> URL {
    let helper = root.appendingPathComponent("assemblywright-mac-bridge")
    FileManager.default.createFile(
        atPath: helper.path,
        contents: Data("#!/bin/sh\nexit 0\n".utf8)
    )
    try FileManager.default.setAttributes(
        [.posixPermissions: 0o700],
        ofItemAtPath: helper.path
    )
    return helper.standardizedFileURL
}

private func enrollmentCSRData() -> Data {
    Data(
        #"{"schema_version":1,"status":"enrollment_csr_ready","grant_id":"11111111-1111-4111-8111-111111111111","device_id":"22222222-2222-4222-8222-222222222222","csr_pem":"-----BEGIN CERTIFICATE REQUEST-----\nZmFrZQ==\n-----END CERTIFICATE REQUEST-----\n"}"#.utf8
    )
}

private func enrollmentStatusData(
    status: String,
    registryRevisionJSON: String = "3",
    certificateNotAfterJSON: String = "4102444800000"
) -> Data {
    Data(
        "{\"status\":\"\(status)\",\"device_id\":\"22222222-2222-4222-8222-222222222222\",\"device_name\":\"owner-mac-bridge\",\"master_endpoint\":\"100.64.23.14:7792\",\"registry_revision\":\(registryRevisionJSON),\"certificate_not_after_ms\":\(certificateNotAfterJSON)}".utf8
    )
}

@MainActor
private func setupLifecycle(
    root: URL,
    launcher: FakeBridgeProcessLauncher
) -> AssemblywrightDeveloperBridgeProcessLifecycle {
    AssemblywrightDeveloperBridgeProcessLifecycle(
        configuration: .init(
            executableURL: root.appendingPathComponent("assemblywright-mac-bridge"),
            expectedTeamIdentifier: "ABCDEFGHIJ"
        ),
        configurationStore: .init(fileURL: root.appendingPathComponent("configuration.json")),
        validator: FakeBridgeExecutableValidator(),
        launcher: launcher,
        localModelSelectionStore: .init(fileURL: root.appendingPathComponent("model.json")),
        assemblyLinePendingMutationStore: .init(
            fileURL: root.appendingPathComponent("pending.json")
        )
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

private func rotationIssuedReceiptData() throws -> Data {
    var receipt = try #require(
        JSONSerialization.jsonObject(with: validIssuedReceiptData()) as? [String: Any]
    )
    receipt["operation"] = "rotate"
    receipt["grant_id"] = "11111111-1111-4111-8111-111111111111"
    receipt["serial_hex"] = "02"
    receipt["certificate_sha256"] = String(repeating: "c", count: 64)
    return try JSONSerialization.data(withJSONObject: receipt, options: [.sortedKeys])
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

private func validAssemblyLineProjectionData(
    emergencyPaused: Bool = false,
    emergencyPauseRevision: UInt64 = 0,
    ownerControlRevision: UInt64 = 1,
    autoRun: Bool = true,
    brainstormingAvailable: Bool = false,
    githubCreationAvailable: Bool = false
) -> Data {
    let profile: [String: Any] = [
        "configuration_revision": 1,
        "provider_id": "openai.codex",
        "model_id": "gpt-5.6-sol"
    ]
    let profileData = try! JSONSerialization.data(
        withJSONObject: profile,
        options: [.sortedKeys, .withoutEscapingSlashes]
    )
    let profileDigest = Array(SHA256.hash(data: profileData))
    var catalog: [String: Any] = [
        "schema_version": 1,
        "catalog_revision": 1,
        "profiles": [profile],
        "default_profile_sha256": profileDigest
    ]
    let catalogData = try! JSONSerialization.data(
        withJSONObject: catalog,
        options: [.sortedKeys, .withoutEscapingSlashes]
    )
    catalog["catalog_sha256"] = Array(SHA256.hash(data: catalogData))
    let component: [String: Any] = [
        "binding_revision": 1,
        "binding_sha256": Array(repeating: UInt8(0x44), count: 32),
        "status": "unavailable",
        "unavailable_reason": "not_configured"
    ]
    let availableComponent: [String: Any] = [
        "binding_revision": 1,
        "binding_sha256": Array(repeating: UInt8(0x44), count: 32),
        "status": "available",
        "unavailable_reason": NSNull()
    ]
    var repositories: [[String: Any]] = []
    var queue: [[String: Any]] = []
    var state: [String: Any] = [
        "schema_version": 1,
        "state_revision": 1,
        "queue_revision": 0,
        "queue_count": 0,
        "auto_run": autoRun,
        "lifecycle": "stopped",
        "session_id": NSNull(),
        "active_child_epoch_id": NSNull(),
        "active_feature_id": NSNull()
    ]
    if emergencyPaused {
        let repositoryID = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
        let featureID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
        repositories = [[
            "schema_version": 1,
            "repository": [
                "repository_id": repositoryID,
                "git_url": ["url": "https://github.com/owner/project"]
            ],
            "repository_revision": 1,
            "lifecycle_revision": 1,
            "visibility": "public",
            "approved_specification_id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            "approved_specification_revision": 1,
            "approved_specification_sha256": Array(repeating: UInt8(0x11), count: 32),
            "owner_approval_sha256": Array(repeating: UInt8(0x22), count: 32),
            "lifecycle": "created",
            "effect_possible": true,
            "creation_evidence_sha256": Array(repeating: UInt8(0x33), count: 32)
        ]]
        queue = [[
            "schema_version": 1,
            "feature_id": featureID,
            "repository_id": repositoryID,
            "specification_id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            "specification_revision": 1,
            "specification_sha256": Array(repeating: UInt8(0x55), count: 32),
            "owner_approval_sha256": Array(repeating: UInt8(0x66), count: 32),
            "position": 1,
            "lifecycle_revision": 1,
            "lifecycle": "emergency_paused"
        ]]
        state["queue_revision"] = 1
        state["queue_count"] = 1
        state["lifecycle"] = "emergency_paused"
        state["session_id"] = "11111111-1111-4111-8111-111111111111"
        state["active_child_epoch_id"] = "22222222-2222-4222-8222-222222222222"
        state["active_feature_id"] = featureID
    }
    let object: [String: Any] = [
        "schema_version": 1,
        "owner_control_revision": ownerControlRevision,
        "emergency_pause_revision": emergencyPauseRevision,
        "emergency_paused": emergencyPaused,
        "orchestrator_catalog": catalog,
        "repositories": repositories,
        "queue": queue,
        "assembly_line": state,
        "availability": [
            "schema_version": 1,
            "availability_revision": 1,
            "observed_at_ms": 1,
            "brainstorming_provider": brainstormingAvailable ? availableComponent : component,
            "github_creation": githubCreationAvailable ? availableComponent : component,
            "windows_executor": component,
            "mac_executor": component,
            "protected_brokers": component
        ]
    ]
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func projectDraftRequestData(projection: Data) -> Data {
    let owner = try! JSONSerialization.jsonObject(with: projection) as! [String: Any]
    let catalog = owner["orchestrator_catalog"] as! [String: Any]
    let profiles = catalog["profiles"] as! [[String: Any]]
    let request: [String: Any] = [
        "schema_version": 1,
        "draft_id": "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
        "draft_revision": 1,
        "repository": [
            "repository_id": "ffffffff-ffff-4fff-8fff-ffffffffffff",
            "git_url": ["url": "https://github.com/owner/new-project"]
        ],
        "visibility": "public",
        "orchestrator_catalog": catalog,
        "orchestrator": profiles[0],
        "idea": "Create a bounded planning-only project"
    ]
    return try! JSONSerialization.data(
        withJSONObject: request,
        options: [.sortedKeys, .withoutEscapingSlashes]
    )
}

private func frozenSpecificationData(draftRequest: Data, targetKind: String) -> Data {
    let envelope = try! JSONSerialization.jsonObject(with: draftRequest) as! [String: Any]
    let draft = (envelope["draft"] as? [String: Any]) ?? envelope
    let specification: [String: Any] = [
        "title": targetKind == "project" ? "Bounded project" : "Bounded feature",
        "outcome": "Deliver the owner-reviewed result",
        "acceptance_criteria": [[
            "id": "owner_review",
            "requirement": "The owner reviews the frozen specification"
        ]],
        "obligations": ["Keep planning separate from effects"]
    ]
    let canonical: (Any) -> Data = { object in
        try! JSONSerialization.data(
            withJSONObject: object,
            options: [.sortedKeys, .withoutEscapingSlashes]
        )
    }
    let object: [String: Any] = [
        "schema_version": 1,
        "specification_id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        "specification_revision": 1,
        "target_kind": targetKind,
        "draft_id": draft["draft_id"]!,
        "draft_revision": draft["draft_revision"]!,
        "draft_sha256": Array(SHA256.hash(data: canonical(draft))),
        "repository": draft["repository"]!,
        "visibility": targetKind == "project" ? draft["visibility"]! : NSNull(),
        "orchestrator_catalog_revision":
            (draft["orchestrator_catalog"] as! [String: Any])["catalog_revision"]!,
        "orchestrator_catalog_sha256":
            (draft["orchestrator_catalog"] as! [String: Any])["catalog_sha256"]!,
        "orchestrator_profile_sha256": Array(
            SHA256.hash(data: canonical(draft["orchestrator"]!))
        ),
        "specification": specification,
        "specification_sha256": Array(SHA256.hash(data: canonical(specification)))
    ]
    return try! JSONSerialization.data(
        withJSONObject: object,
        options: [.sortedKeys, .withoutEscapingSlashes]
    )
}

private func repositoryCreationProjectionData(
    repositoryID: UUID,
    created: Bool
) -> Data {
    var object: [String: Any] = [
        "schema_version": 1,
        "repository": [
            "repository_id": repositoryID.uuidString.lowercased(),
            "git_url": ["url": "https://github.com/owner/project"]
        ],
        "repository_revision": 1,
        "lifecycle_revision": created ? 2 : 1,
        "visibility": "public",
        "approved_specification_id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
        "approved_specification_revision": 1,
        "approved_specification_sha256": Array(repeating: UInt8(0x11), count: 32),
        "owner_approval_sha256": Array(repeating: UInt8(0x22), count: 32),
        "lifecycle": created ? "created" : "creation_pending",
        "effect_possible": created,
        "creation_evidence_sha256": NSNull()
    ]
    if created {
        object["creation_evidence_sha256"] = Array(repeating: UInt8(0x33), count: 32)
    }
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func projectApprovalReceiptData(approvalRequest: Data) -> Data {
    let approval = try! JSONSerialization.jsonObject(with: approvalRequest) as! [String: Any]
    return try! JSONSerialization.data(withJSONObject: [
        "schema_version": 1,
        "repository": approval["repository"]!,
        "repository_revision": 1,
        "lifecycle_revision": 1,
        "visibility": approval["visibility"]!,
        "approved_specification_id": approval["specification_id"]!,
        "approved_specification_revision": approval["specification_revision"]!,
        "approved_specification_sha256": approval["specification_sha256"]!,
        "owner_approval_sha256": approval["owner_approval_sha256"]!,
        "lifecycle": "creation_pending",
        "effect_possible": false,
        "creation_evidence_sha256": NSNull()
    ], options: [.sortedKeys])
}

private func assemblyLineProjectionWithPendingRepository(
    repositoryID: UUID,
    created: Bool = false,
    brainstormingAvailable: Bool = false
) -> Data {
    var projection = try! JSONSerialization.jsonObject(
        with: validAssemblyLineProjectionData(
            ownerControlRevision: created ? 3 : 2,
            brainstormingAvailable: brainstormingAvailable,
            githubCreationAvailable: true
        )
    ) as! [String: Any]
    projection["repositories"] = [
        try! JSONSerialization.jsonObject(
            with: repositoryCreationProjectionData(repositoryID: repositoryID, created: created)
        )
    ]
    return try! JSONSerialization.data(withJSONObject: projection, options: [.sortedKeys])
}

private func autoRunAssemblyLineReceiptData(request: Data) -> Data {
    let requestObject = try! JSONSerialization.jsonObject(with: request) as! [String: Any]
    let projection = try! JSONSerialization.jsonObject(
        with: validAssemblyLineProjectionData()
    ) as! [String: Any]
    var state = projection["assembly_line"] as! [String: Any]
    state["state_revision"] =
        (requestObject["expected_state_revision"] as! NSNumber).uint64Value + 1
    state["auto_run"] = requestObject["auto_run"] as! Bool
    return try! JSONSerialization.data(withJSONObject: [
        "schema_version": 1,
        "request_id": requestObject["request_id"] as! String,
        "resulting_state": state
    ], options: [.sortedKeys])
}

private func authenticatedSnapshotData(
    connectionEpoch: UInt64,
    maintenanceActive: Bool = false,
    emergencyPaused: Bool = false,
    masterStatus: String? = nil,
    featureConveyor: Data = validFeatureConveyorData(),
    ownerControl: Data? = nil,
    localModelSelection: Data? = nil,
    assemblyLine: Data? = nil
) -> Data {
    let featureObject = try! JSONSerialization.jsonObject(with: featureConveyor)
    let guidance = (featureObject as? [String: Any])?["owner_guidance"] as? [String: Any]
    let ownerObject = try! JSONSerialization.jsonObject(with: ownerControl ?? ownerControlData(
        queueRevision: (featureObject as? [String: Any])?["queue_revision"] as? UInt64 ?? 0,
        emergencyPaused: emergencyPaused,
        emergencyPauseRevision: guidance?["emergency_pause_revision"] as? UInt64 ?? 0
    ))
    var object: [String: Any] = [
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
        "feature_conveyor": featureObject,
        "owner_control": ownerObject,
        "assembly_line": try! JSONSerialization.jsonObject(
            with: assemblyLine ?? validAssemblyLineProjectionData(
                emergencyPaused: emergencyPaused,
                emergencyPauseRevision: guidance?["emergency_pause_revision"] as? UInt64 ?? 0
            )
        )
    ]
    if let localModelSelection {
        object["local_model_selection"] = try! JSONSerialization.jsonObject(
            with: localModelSelection
        )
    }
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
    object.removeValue(forKey: "owner_control")
    object.removeValue(forKey: "assembly_line")
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func ownerControlData(
    queueRevision: UInt64 = 0,
    emergencyPaused: Bool = false,
    emergencyPauseRevision: UInt64 = 0,
    completeEvidence: Bool = false,
    active: Bool = false
) -> Data {
    let names = ["repository_gate_proof", "restricted_worker_live", "review_provider_live",
                 "github_publication_live", "restart_recovery_live", "mac_windows_control_event_streaming_live"]
    var evidence: [String: Any] = [:]
    for (index, name) in names.enumerated() {
        evidence[name] = completeEvidence ? [
            "evidence_id": String(format: "%08x-0000-4000-8000-%012x", index + 1, index + 1),
            "revision": 1,
            "receipt_sha256": Array(repeating: index + 1, count: 32)
        ] : NSNull()
    }
    let ready = !active && !emergencyPaused && completeEvidence
    let object: [String: Any] = [
        "schema_version": 1,
        "queue_revision": queueRevision,
        "emergency_paused": emergencyPaused,
        "emergency_pause_revision": emergencyPauseRevision,
        "owner_control_designation_revision": 1,
        "activation_status": active ? "active" : "inactive",
        "activation_id": active ? "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa" : NSNull(),
        "activation_ready": ready,
        "activation_blocker": active ? "already_activated" : emergencyPaused ? "emergency_paused" : completeEvidence ? "none" : "evidence_required",
        "active_feature": NSNull(),
        "evidence": evidence
    ]
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func ownerControlDataWithCanonicalAlphabeticEvidence(
    queueRevision: UInt64
) -> Data {
    var object = try! JSONSerialization.jsonObject(
        with: ownerControlData(queueRevision: queueRevision, completeEvidence: true)
    ) as! [String: Any]
    var evidence = object["evidence"] as! [String: Any]
    var repositoryEvidence = evidence["repository_gate_proof"] as! [String: Any]
    repositoryEvidence["evidence_id"] = "abcdef01-abcd-4abc-8abc-abcdefabcdef"
    evidence["repository_gate_proof"] = repositoryEvidence
    object["evidence"] = evidence
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func ownerControlDataWithActiveFeature(
    stage: String,
    ownerPaused: Bool
) -> Data {
    var object = try! JSONSerialization.jsonObject(
        with: ownerControlData()
    ) as! [String: Any]
    object["active_feature"] = [
        "feature_id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        "specification_revision": 1,
        "lifecycle_revision": 3,
        "lifecycle_status": "paused",
        "orchestration_revision": 4,
        "stage": stage,
        "owner_paused": ownerPaused,
    ]
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func assertHostileOneShotHelperIsReaped(
    script: String,
    minimumDuration: Duration = .zero,
    commandTimeout: Duration = .milliseconds(100)
) async throws {
    let projection = try AssemblywrightMacFeatureConveyorOwnerControlProjection.decodeStrict(
        ownerControlData(completeEvidence: true)
    )
    let request = try AssemblywrightMacFeatureConveyorActivationControl.activationRequest(
        from: projection
    )
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
        "assemblywright-hostile-owner-command-\(UUID().uuidString)",
        isDirectory: true
    )
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: directory) }
    let executable = directory.appendingPathComponent("bridge-fixture")
    try Data(script.utf8).write(to: executable, options: .atomic)
    try FileManager.default.setAttributes(
        [.posixPermissions: 0o700],
        ofItemAtPath: executable.path
    )
    let validated = AssemblywrightDeveloperBridgeValidatedExecutable(
        executableURL: executable,
        teamIdentifier: "ABCDEFGHIJ",
        codeRequirement: "anchor apple generic",
        cdHash: Data(repeating: 0x11, count: 20)
    )
    let validator = RecordingBridgeRunningProcessValidator()
    let launcher = FoundationAssemblywrightDeveloperBridgeProcessLauncher(
        runningProcessValidator: validator,
        ownerCommandTimeout: commandTimeout
    )
    let started = ContinuousClock.now
    do {
        _ = try await launcher.runCommand(
            executable: validated,
            arguments: AssemblywrightDeveloperBridgeProcessLifecycle.helperArguments(
                for: .activation
            ),
            input: request
        )
        Issue.record("hostile one-shot helper unexpectedly succeeded")
    } catch {
        #expect(error is AssemblywrightDeveloperBridgeProcessError)
    }
    let duration = started.duration(to: .now)
    #expect(duration >= minimumDuration)
    #expect(duration < .seconds(2))
    let processIdentifier = try #require(validator.processIdentifier)
    #expect(!processExists(processIdentifier))
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

private func approvedFeatureAuthoringStatus(
    queueRevision: UInt64 = 0,
    emergencyPaused: Bool = false,
    active: Bool = true
) throws -> AssemblywrightDeveloperBridgeAppStatus {
    let featureConveyor = emergencyPaused ? pausedFeatureConveyorData() : validFeatureConveyorData()
    return try AssemblywrightDeveloperBridgeProcessLifecycle.status(
        from: authenticatedSnapshotData(
            connectionEpoch: 91,
            emergencyPaused: emergencyPaused,
            featureConveyor: featureConveyor,
            ownerControl: ownerControlData(
                queueRevision: queueRevision,
                emergencyPaused: emergencyPaused,
                emergencyPauseRevision: emergencyPaused ? 1 : 0,
                completeEvidence: true,
                active: active
            )
        )
    )
}

@MainActor
private func approvedFeatureLifecycle(
    launcher: FakeBridgeProcessLauncher
) -> AssemblywrightDeveloperBridgeProcessLifecycle {
    AssemblywrightDeveloperBridgeProcessLifecycle(
        configuration: .init(environment: [
            AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey:
                "/tmp/assemblywright-mac-bridge",
            AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey:
                "ABCDEFGHIJ"
        ]),
        validator: FakeBridgeExecutableValidator(),
        launcher: launcher
    )
}

private func sampleApprovedFeatureDraft(
    featureID: UUID = UUID(uuidString: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")!,
    manifest: AssemblywrightMacApprovedFeatureManifest = .init(
        acceptance: ["acceptance_1"],
        outcome: "Ship bounded owner feature",
        allowedPaths: ["Sources/App.swift"]
    ),
    designSHA256: [UInt8] = Array(repeating: 0x22, count: 32),
    providerID: String = "openai.codex",
    modelID: String = "gpt-5.6-sol",
    dependencies: [UUID] = []
) -> AssemblywrightMacFeatureConveyorApprovedFeatureDraft {
    AssemblywrightMacFeatureConveyorApprovedFeatureDraft(
        featureID: featureID,
        repositoryID: UUID(uuidString: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")!,
        specificationRevision: 1,
        manifest: manifest,
        designSHA256: designSHA256,
        brainstormingSHA256: Array(repeating: 0x33, count: 32),
        ownerApprovalSHA256: Array(repeating: 0x44, count: 32),
        grants: .init(registration: 1, cloudDisclosure: 2, autonomousPublication: 3),
        providerID: providerID,
        modelID: modelID,
        dependencies: dependencies
    )
}

private func approvedFeatureAuthoringReceiptData(
    request: Data,
    queueRevisionOffset: UInt64 = 1
) -> Data {
    let object = try! JSONSerialization.jsonObject(with: request) as! [String: Any]
    let specification = object["specification"] as! [String: Any]
    let queueRevision = (object["expected_queue_revision"] as! NSNumber).uint64Value
    let receipt: [String: Any] = [
        "schema_version": 1,
        "feature_id": specification["feature_id"]!,
        "specification_revision": specification["revision"]!,
        "lifecycle_revision": 1,
        "queue_revision": queueRevision + queueRevisionOffset,
        "owner_control_designation_revision": object["owner_control_designation_revision"]!,
        "emergency_pause_revision": object["emergency_pause_revision"]!,
        "status": "queued"
    ]
    return try! JSONSerialization.data(withJSONObject: receipt, options: [.sortedKeys])
}

private func activationReceiptData(request: Data) -> Data {
    let requestObject = try! JSONSerialization.jsonObject(with: request) as! [String: Any]
    let object: [String: Any] = [
        "schema_version": 1,
        "activation_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "queue_revision": requestObject["expected_queue_revision"]!,
        "owner_control_designation_revision": requestObject["expected_owner_control_designation_revision"]!,
        "emergency_pause_revision": requestObject["expected_emergency_pause_revision"]!,
        "evidence": requestObject["evidence"]!,
        "activated_at_ms": 1_000,
        "status": "active"
    ]
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}
