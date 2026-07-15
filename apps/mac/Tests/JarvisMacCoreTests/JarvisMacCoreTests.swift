import Foundation
import Testing
import Darwin
@testable import JarvisMacCore

#if canImport(AVFoundation)
import AVFoundation
#endif

#if canImport(Speech)
import Speech
#endif

private final class IPCURLProtocol: URLProtocol {
    nonisolated(unsafe) static var handler: ((URLRequest) throws -> (HTTPURLResponse, Data))?

    override class func canInit(with request: URLRequest) -> Bool {
        true
    }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest {
        request
    }

    override func startLoading() {
        guard let handler = Self.handler else {
            client?.urlProtocol(self, didFailWithError: URLError(.badServerResponse))
            return
        }

        do {
            let (response, data) = try handler(request)
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override func stopLoading() {}
}

@MainActor
private final class MutableFlag {
    var value: Bool

    init(_ value: Bool) {
        self.value = value
    }
}

@MainActor
private final class FakeVoiceAdapter: JarvisVoiceAdapter {
    var phase: JarvisVoiceAdapterPhase
    var permissionResult: Result<Void, JarvisVoiceAdapterError>
    var startResult: Result<Void, JarvisVoiceAdapterError>
    var stopResult: Result<Void, JarvisVoiceAdapterError>
    var interruptResult: Result<Void, JarvisVoiceAdapterError>
    private(set) var callbacks: JarvisVoiceCaptureCallbacks?

    init(
        phase: JarvisVoiceAdapterPhase = .idle,
        permissionResult: Result<Void, JarvisVoiceAdapterError> = .success(()),
        startResult: Result<Void, JarvisVoiceAdapterError> = .success(()),
        stopResult: Result<Void, JarvisVoiceAdapterError> = .success(()),
        interruptResult: Result<Void, JarvisVoiceAdapterError> = .success(())
    ) {
        self.phase = phase
        self.permissionResult = permissionResult
        self.startResult = startResult
        self.stopResult = stopResult
        self.interruptResult = interruptResult
    }

    func requestPermissions() async -> Result<Void, JarvisVoiceAdapterError> {
        switch permissionResult {
        case .success:
            phase = .idle
        case let .failure(error):
            phase = .unavailable(reason: error.description)
        }
        return permissionResult
    }

    func startCapture(callbacks: JarvisVoiceCaptureCallbacks) async -> Result<Void, JarvisVoiceAdapterError> {
        switch startResult {
        case .success:
            self.callbacks = callbacks
            phase = .listening
        case let .failure(error):
            phase = .unavailable(reason: error.description)
        }
        return startResult
    }

    func stopCapture() async -> Result<Void, JarvisVoiceAdapterError> {
        switch stopResult {
        case .success:
            callbacks = nil
            phase = .idle
        case let .failure(error):
            phase = .unavailable(reason: error.description)
        }
        return stopResult
    }

    func interrupt(reason: String) async -> Result<Void, JarvisVoiceAdapterError> {
        switch interruptResult {
        case .success:
            callbacks = nil
            phase = .interrupted(reason: reason)
        case let .failure(error):
            phase = .unavailable(reason: error.description)
        }
        return interruptResult
    }

    func emitPartial(_ transcript: String) {
        callbacks?.onPartialTranscript(transcript)
    }

    func emitFinal(_ transcript: String) {
        callbacks?.onFinalTranscript(transcript)
    }

    func emitError(_ error: JarvisVoiceAdapterError) {
        callbacks?.onError(error)
    }
}

@MainActor
private final class FakeSpeechOutputAdapter: JarvisSpeechOutputAdapter {
    var phase: JarvisSpeechOutputPhase
    var onPhaseChange: (@MainActor (JarvisSpeechOutputPhase) -> Void)?
    var speakResult: Result<Void, JarvisSpeechOutputError>
    var stopResult: Result<Void, JarvisSpeechOutputError>
    var interruptResult: Result<Void, JarvisSpeechOutputError>
    private(set) var spokenTexts: [String]

    init(
        phase: JarvisSpeechOutputPhase = .idle,
        speakResult: Result<Void, JarvisSpeechOutputError> = .success(()),
        stopResult: Result<Void, JarvisSpeechOutputError> = .success(()),
        interruptResult: Result<Void, JarvisSpeechOutputError> = .success(())
    ) {
        self.phase = phase
        self.speakResult = speakResult
        self.stopResult = stopResult
        self.interruptResult = interruptResult
        self.spokenTexts = []
    }

    func speak(_ text: String) async -> Result<Void, JarvisSpeechOutputError> {
        switch speakResult {
        case .success:
            spokenTexts.append(text)
            setPhase(.speaking)
        case let .failure(error):
            setPhase(.unavailable(reason: error.description))
        }
        return speakResult
    }

    func stop() async -> Result<Void, JarvisSpeechOutputError> {
        switch stopResult {
        case .success:
            setPhase(.idle)
        case let .failure(error):
            setPhase(.unavailable(reason: error.description))
        }
        return stopResult
    }

    func interrupt(reason: String) async -> Result<Void, JarvisSpeechOutputError> {
        switch interruptResult {
        case .success:
            setPhase(.interrupted(reason: reason))
        case let .failure(error):
            setPhase(.unavailable(reason: error.description))
        }
        return interruptResult
    }

    func finishSpeech() {
        setPhase(.idle)
    }

    private func setPhase(_ nextPhase: JarvisSpeechOutputPhase) {
        phase = nextPhase
        onPhaseChange?(nextPhase)
    }
}

private func speechOutputSucceeded(_ result: Result<Void, JarvisSpeechOutputError>) -> Bool {
    if case .success = result {
        return true
    }
    return false
}

private func speechOutputFailed(
    _ result: Result<Void, JarvisSpeechOutputError>,
    with expected: JarvisSpeechOutputError
) -> Bool {
    if case let .failure(error) = result {
        return error == expected
    }
    return false
}

#if canImport(AVFoundation)
@MainActor
private final class CapturingSpeechSynthesizer: JarvisSpeechSynthesizing {
    var isSpeaking: Bool
    private(set) weak var delegate: (any AVSpeechSynthesizerDelegate)?
    private(set) var spokenUtterances: [AVSpeechUtterance]
    private(set) var stopBoundaries: [AVSpeechBoundary]

    init(isSpeaking: Bool = false) {
        self.isSpeaking = isSpeaking
        self.spokenUtterances = []
        self.stopBoundaries = []
    }

    func setDelegate(_ delegate: (any AVSpeechSynthesizerDelegate)?) {
        self.delegate = delegate
    }

    func speak(_ utterance: AVSpeechUtterance) {
        spokenUtterances.append(utterance)
        isSpeaking = true
    }

    func stopSpeaking(at boundary: AVSpeechBoundary) -> Bool {
        stopBoundaries.append(boundary)
        isSpeaking = false
        return true
    }
}
#endif

@MainActor
private final class FakeSchedulerNotificationAdapter: JarvisSchedulerNotificationAdapter {
    var authorizationStatusResult: JarvisSchedulerNotificationAuthorization
    var authorizationResult: Result<Bool, Error>
    var deliveryResult: Result<Void, Error>
    let authorizationStatusDelayNanoseconds: UInt64
    let authorizationStatusWaitsForRelease: Bool
    private var authorizationStatusContinuation: CheckedContinuation<Void, Never>?
    private(set) var authorizationStatusCheckCount: Int
    private(set) var authorizationRequestCount: Int
    private(set) var deliveredRequests: [JarvisSchedulerNotificationRequest]

    init(
        authorizationStatus: JarvisSchedulerNotificationAuthorization = .authorized,
        authorizationResult: Result<Bool, Error> = .success(true),
        deliveryResult: Result<Void, Error> = .success(()),
        authorizationStatusDelayNanoseconds: UInt64 = 0,
        authorizationStatusWaitsForRelease: Bool = false
    ) {
        self.authorizationStatusResult = authorizationStatus
        self.authorizationResult = authorizationResult
        self.deliveryResult = deliveryResult
        self.authorizationStatusDelayNanoseconds = authorizationStatusDelayNanoseconds
        self.authorizationStatusWaitsForRelease = authorizationStatusWaitsForRelease
        self.authorizationStatusContinuation = nil
        self.authorizationStatusCheckCount = 0
        self.authorizationRequestCount = 0
        self.deliveredRequests = []
    }

    func authorizationStatus() async -> JarvisSchedulerNotificationAuthorization {
        authorizationStatusCheckCount += 1
        if authorizationStatusWaitsForRelease {
            await withCheckedContinuation { continuation in
                authorizationStatusContinuation = continuation
            }
        } else if authorizationStatusDelayNanoseconds > 0 {
            try? await Task.sleep(nanoseconds: authorizationStatusDelayNanoseconds)
        }
        return authorizationStatusResult
    }

    func releaseAuthorizationStatus() {
        let continuation = authorizationStatusContinuation
        authorizationStatusContinuation = nil
        continuation?.resume()
    }

    func requestAuthorization() async throws -> Bool {
        authorizationRequestCount += 1
        return try authorizationResult.get()
    }

    func deliver(_ request: JarvisSchedulerNotificationRequest) async throws {
        try deliveryResult.get()
        deliveredRequests.append(request)
    }
}

private final class FakeCredentialStore: JarvisCredentialStore, @unchecked Sendable {
    var values: [JarvisCredentialKey: String]

    init(values: [JarvisCredentialKey: String] = [:]) {
        self.values = values
    }

    func readCredential(_ key: JarvisCredentialKey) throws -> String? {
        values[key]
    }

    func saveCredential(_ value: String, for key: JarvisCredentialKey) throws {
        values[key] = value
    }

    func deleteCredential(_ key: JarvisCredentialKey) throws {
        values.removeValue(forKey: key)
    }
}

@Suite("Jarvis Mac core contracts", .serialized)
struct JarvisMacCoreTests {
    @Test("Endpoint appends paths to the configured core URL")
    func endpointBuildsURL() {
        let endpoint = JarvisEndpoint(baseURL: URL(string: "http://127.0.0.1:7787")!)

        #expect(endpoint.url(path: "/health").absoluteString == "http://127.0.0.1:7787/health")
        #expect(endpoint.url(path: "commands").absoluteString == "http://127.0.0.1:7787/commands")
        #expect(endpoint.url(path: "/memory?include_deleted=true").absoluteString == "http://127.0.0.1:7787/memory?include_deleted=true")
    }

    @Test("Health payload decodes Rust IPC contract names")
    func decodesHealth() throws {
        let data = Data(
            """
            {
              "status": "ok",
              "version": "0.1.0",
              "contract": {
                "name": "jarvis.local-ipc",
                "version": 1,
                "core_version": "0.1.0"
              },
              "started_at": "2026-05-20T12:00:00Z",
              "emergency_paused": true,
              "emergency_pause_reason": "testing",
              "emergency_pause_updated_at": "2026-05-20T12:00:01Z",
              "scheduler_jobs": 2,
              "command_runtime": "fake-local-model"
            }
            """.utf8
        )

        let health = try JSONDecoder().decode(JarvisHealth.self, from: data)

        #expect(health.status == "ok")
        #expect(health.contract?.name == "jarvis.local-ipc")
        #expect(health.emergencyPaused)
        #expect(health.emergencyPauseReason == "testing")
        #expect(health.schedulerJobs == 2)
        #expect(health.commandRuntime == "fake-local-model")
    }

    @Test("Contract payload decodes endpoints and approval action exposure")
    func decodesContract() throws {
        let data = Data(
            """
            {
              "contract": {
                "name": "jarvis.local-ipc",
                "version": 1,
                "core_version": "0.1.0"
              },
              "compatibility": {
                "minimum_supported_version": 1,
                "current_version": 1,
                "additive_changes_allowed": true,
                "breaking_change_policy": "version bump required",
                "deprecation_policy": "list before removal",
                "client_requirements": [
                  "Clients must ignore unknown JSON fields."
                ],
                "removed_endpoints": [],
                "deprecated_endpoints": []
              },
              "endpoints": [
                { "method": "GET", "path": "/health", "repository_required": false, "redacted": true },
                { "method": "GET", "path": "/scheduler/jobs/:id", "repository_required": false, "redacted": false },
                { "method": "GET", "path": "/activity/summary", "repository_required": true, "redacted": true },
                { "method": "GET", "path": "/release/readiness", "repository_required": false, "redacted": true },
                { "method": "GET", "path": "/memory/retention-plan", "repository_required": true, "redacted": true },
                { "method": "GET", "path": "/release/live-device-runbook", "repository_required": false, "redacted": true },
                { "method": "GET", "path": "/release/signed-distribution-runbook", "repository_required": false, "redacted": true },
                { "method": "GET", "path": "/release/plugin-trust-runbook", "repository_required": false, "redacted": true },
                { "method": "GET", "path": "/release/evidence-bundle-runbook", "repository_required": false, "redacted": true },
                { "method": "GET", "path": "/permissions/grants", "repository_required": true, "redacted": false },
                { "method": "GET", "path": "/permissions/policy-review", "repository_required": true, "redacted": false }
              ],
              "safe_inspection_paths": ["/health", "/release/readiness", "/diagnostics/export", "/memory/retention-plan", "/release/live-device-runbook", "/release/signed-distribution-runbook", "/release/plugin-trust-runbook", "/release/evidence-bundle-runbook"],
              "features": [
                {
                  "key": "scheduler_trigger_policy_review",
                  "status": "implemented",
                  "proof": "covered by tests",
                  "boundary": "review visibility only"
                },
                {
                  "key": "scheduler_stale_running_recovery",
                  "status": "implemented",
                  "proof": "explicit plus opt-in startup recovery covered by tests",
                  "boundary": "bounded cleanup only; no default background recovery"
                },
                {
                  "key": "release_evidence_bundle",
                  "status": "implemented",
                  "proof": "final bundle mechanics covered by self-test",
                  "boundary": "owner-recorded external evidence still required"
                },
                {
                  "key": "live_voice_loop",
                  "status": "pending_manual_validation",
                  "proof": "fake-adapter final transcript staging and opt-in auto-submit tests",
                  "boundary": "manual validation pending"
                }
              ]
            }
            """.utf8
        )

        let contract = try JSONDecoder().decode(JarvisContractResponse.self, from: data)

        #expect(contract.contract.name == "jarvis.local-ipc")
        #expect(contract.compatibility?.supportsCurrentClient == true)
        #expect(contract.compatibility?.additiveChangesAllowed == true)
        #expect(contract.compatibility?.clientRequirements.contains("Clients must ignore unknown JSON fields.") == true)
        #expect(contract.endpoints.map(\.id).contains("GET /scheduler/jobs/:id"))
        #expect(contract.safeInspectionPaths.contains("/diagnostics/export"))
        #expect(contract.features.map(\.id).contains("scheduler_trigger_policy_review"))
        #expect(contract.features.first { $0.key == "scheduler_stale_running_recovery" }?.proof.contains("opt-in startup recovery") == true)
        #expect(contract.features.map(\.id).contains("release_evidence_bundle"))
        #expect(contract.features.first { $0.key == "live_voice_loop" }?.status == "pending_manual_validation")
        #expect(!contract.exposesApprovalActions)
        #expect(contract.exposesPermissionGrantSummary)
        #expect(contract.exposesPermissionPolicyReview)
        #expect(contract.exposesMemoryRetentionPlan)
        #expect(contract.exposesReleaseRunbooks)
        #expect(contract.exposesReleaseReadiness)
    }

    @Test("Release readiness payload decodes production blockers")
    func decodesReleaseReadiness() throws {
        let readiness = try JSONDecoder().decode(
            JarvisReleaseReadiness.self,
            from: releaseReadinessJSON()
        )

        #expect(!readiness.productionReady)
        #expect(!readiness.evidenceModeEnabled)
        #expect(readiness.verifiedFeatureCount == readiness.implementedFeatures.count)
        #expect(readiness.pendingFeatureCount == readiness.pendingFeatures.count)
        #expect(readiness.implementedFeatures.first?.key == "repository_state")
        #expect(readiness.implementedFeatures.map(\.key).contains("plugin_network_governance"))
        #expect(readiness.implementedFeatures.map(\.key).contains("operator_release_qa_smoke"))
        #expect(readiness.implementedFeatures.map(\.key).contains("release_ci_gate"))
        #expect(readiness.implementedFeatures.map(\.key).contains("unsigned_distribution_launch"))
        #expect(readiness.implementedFeatures.map(\.key).contains("release_evidence_status"))
        #expect(readiness.implementedFeatures.map(\.key).contains("release_evidence_bundle"))
        #expect(readiness.pendingFeatures.first?.key == "live_voice_loop")
        #expect(readiness.blockingManualGates.contains("Developer ID Application and Installer signing credentials configured and used for a full signed package run"))
        #expect(readiness.blockingManualGates.contains("final release evidence bundle generated and archived after signed distribution, live-device QA, and plugin-trust QA reports exist"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-local.sh"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-ci-workflow-smoke.sh"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-operator-qa-smoke.sh"))
        #expect(!readiness.recommendedVerificationCommands.contains("./scripts/packaged-app-release-smoke.sh"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/package-distribution.sh --check"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/package-distribution.sh --unsigned-launch-check"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-live-device-qa.sh --check"))
        #expect(readiness.recommendedVerificationCommands.contains { command in
            command.contains("JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true") &&
                command.contains("JARVIS_QA_OWNER_NAME=") &&
                command.contains("JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE=") &&
                command.contains("JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE=") &&
                command.contains("JARVIS_QA_NOTIFICATION_OBSERVED_AT=") &&
                command.contains("JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE=") &&
                command.contains("./scripts/release-live-device-qa.sh --assert-complete")
        })
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-plugin-trust-qa.sh --check"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env"))
        #expect(readiness.recommendedVerificationCommands.contains("set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete"))
        #expect(readiness.recommendedVerificationCommands.contains { command in
            command.contains("JARVIS_PLUGIN_QA_OWNER_NAME=") &&
                command.contains("JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE=") &&
                command.contains("JARVIS_PLUGIN_QA_EGRESS_POLICY_LABEL=") &&
                command.contains("JARVIS_PLUGIN_QA_EGRESS_VALIDATION_COMPLETED_AT=") &&
                command.contains("JARVIS_PLUGIN_QA_EGRESS_DENY_FIXTURE_EVIDENCE_NOTE=") &&
                command.contains("JARVIS_PLUGIN_QA_EGRESS_ALLOW_FIXTURE_EVIDENCE_NOTE=") &&
                command.contains("./scripts/release-plugin-trust-qa.sh --assert-complete")
        })
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-evidence-bundle.sh --check"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env"))
        #expect(readiness.recommendedVerificationCommands.contains("set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-evidence-doctor.sh --check"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-evidence-doctor.sh --assert-complete"))
        #expect(readiness.recommendedVerificationCommands.contains("cargo run -p jarvis-cli -- release live-device-runbook"))
        let commands = readiness.recommendedVerificationCommands
        let operatorSmokeIndex = try #require(commands.firstIndex(of: "./scripts/release-operator-qa-smoke.sh"))
        let packagePreflightIndex = try #require(commands.firstIndex(of: "./scripts/package-distribution.sh --check"))
        let unsignedDistributionIndex = try #require(commands.firstIndex(of: "./scripts/package-distribution.sh --unsigned-launch-check"))
        let signedDistributionIndex = try #require(commands.firstIndex(of: "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh"))
        let liveDeviceRunbookIndex = try #require(commands.firstIndex(of: "cargo run -p jarvis-cli -- release live-device-runbook"))
        let liveDeviceCheckIndex = try #require(commands.firstIndex(of: "./scripts/release-live-device-qa.sh --check"))
        let liveDeviceTemplateIndex = try #require(commands.firstIndex(of: "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env"))
        let pluginTrustAssertIndex = try #require(commands.firstIndex(of: "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete"))
        let evidenceBundleSourceIndex = try #require(commands.firstIndex(of: "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle"))
        let evidenceBundleInlineIndex = try #require(commands.firstIndex { command in
            command.contains("JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true")
        })
        let evidenceDoctorAssertIndex = try #require(commands.firstIndex(of: "./scripts/release-evidence-doctor.sh --assert-complete"))
        let externalCoreRestartIndex = try #require(commands.firstIndex(of: "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external"))
        let externalReadinessIndex = try #require(commands.firstIndex(of: "cargo run -p jarvis-cli -- release readiness"))
        #expect(operatorSmokeIndex < packagePreflightIndex)
        #expect(packagePreflightIndex < unsignedDistributionIndex)
        #expect(unsignedDistributionIndex < signedDistributionIndex)
        #expect(signedDistributionIndex < liveDeviceRunbookIndex)
        #expect(liveDeviceRunbookIndex < liveDeviceCheckIndex)
        #expect(liveDeviceCheckIndex < liveDeviceTemplateIndex)
        #expect(pluginTrustAssertIndex < evidenceBundleSourceIndex)
        #expect(evidenceBundleSourceIndex < evidenceDoctorAssertIndex)
        #expect(evidenceBundleInlineIndex < evidenceDoctorAssertIndex)
        #expect(evidenceDoctorAssertIndex < externalCoreRestartIndex)
        #expect(externalCoreRestartIndex < externalReadinessIndex)
        #expect(readiness.proofBoundary.contains("does not perform signing"))
    }

    @Test("Release readiness payload decodes external evidence success")
    func decodesExternalEvidenceReadyReleaseReadiness() throws {
        let readiness = try JSONDecoder().decode(
            JarvisReleaseReadiness.self,
            from: externalProductionReadyReleaseReadinessJSON()
        )

        #expect(readiness.productionReady)
        #expect(readiness.evidenceModeEnabled)
        #expect(readiness.verifiedFeatureCount == readiness.implementedFeatures.count)
        #expect(readiness.pendingFeatureCount == readiness.pendingFeatures.count)
        #expect(readiness.pendingFeatures.isEmpty)
        #expect(readiness.implementedFeatures.map(\.key).contains("live_voice_loop"))
        #expect(readiness.blockingManualGates.isEmpty)
        #expect(readiness.recommendedVerificationCommands.contains("Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external"))
        #expect(readiness.recommendedVerificationCommands.contains("cargo run -p jarvis-cli -- release readiness"))
        #expect(readiness.proofBoundary.contains("does not perform signing"))
    }

    @Test("Release runbook payload decodes operator evidence path")
    func decodesReleaseRunbook() throws {
        let runbook = try JSONDecoder().decode(
            JarvisReleaseRunbook.self,
            from: releaseRunbookJSON(runbook: "live_device")
        )

        #expect(runbook.generatedFrom == "release readiness plus evidence-status")
        #expect(runbook.runbook == "live_device")
        #expect(!runbook.productionReady)
        #expect(runbook.liveVoiceFeature?.key == "live_voice_loop")
        #expect(runbook.evidenceItems.first?.key == "live_device_qa_report")
        #expect(runbook.evidenceItems.first?.status == "missing")
        #expect(runbook.commands.contains("./scripts/release-live-device-qa.sh --check"))
        #expect(runbook.commands.contains("cargo run -p jarvis-cli -- command \"status check\" --endpoint <release-core-endpoint> --json"))
        #expect(runbook.commands.contains { $0.contains("JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid>'") })
        #expect(runbook.commands.contains("JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint <release-core-endpoint>"))
        #expect(runbook.commands.contains("JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint <release-core-endpoint>"))
        #expect(runbook.manualChecks.contains { $0.contains("microphone and Speech") })
        #expect(runbook.proofBoundary.contains("does not perform live-device validation"))
    }

    @Test("Command response decodes task, route, steps, and message")
    func decodesCommandResponse() throws {
        let taskId = UUID()
        let sessionId = UUID()
        let data = Data(
            """
            {
              "accepted": true,
              "task": {
                "id": "\(taskId.uuidString)",
                "session_id": "\(sessionId.uuidString)",
                "user_input": "status check",
                "status": "completed",
                "created_at": "2026-05-20T12:00:00Z",
                "updated_at": "2026-05-20T12:00:01Z"
              },
              "audit_entry": {
                "id": "\(UUID().uuidString)",
                "task_id": "\(taskId.uuidString)",
                "event_type": "task_completed",
                "summary": "command completed",
                "payload": {},
                "created_at": "2026-05-20T12:00:01Z"
              },
              "audit_entries": [],
              "plugin_results": [],
              "route": {
                "provider": "local",
                "model": "fake-local-model",
                "reason": "local model is the default route for v1 commands"
              },
              "steps": [
                { "index": 0, "message": "local response: status check", "complete": true }
              ],
              "message": "local response: status check"
            }
            """.utf8
        )

        let response = try JSONDecoder().decode(JarvisCommandResponse.self, from: data)

        #expect(response.accepted)
        #expect(response.task.id == taskId)
        #expect(response.task.sessionId == sessionId)
        #expect(response.task.status == "completed")
        #expect(response.route?.model == "fake-local-model")
        #expect(response.auditEntry.eventType == "task_completed")
        #expect(response.pluginResults.isEmpty)
        #expect(response.steps == [
            JarvisRuntimeStep(index: 0, message: "local response: status check", complete: true)
        ])
    }

    @Test("Command response decodes audit entries and plugin results")
    func decodesCommandEvidence() throws {
        let taskId = UUID()
        let sessionId = UUID()
        let auditId = UUID()
        let data = Data(
            """
            {
              "accepted": true,
              "task": {
                "id": "\(taskId.uuidString)",
                "session_id": "\(sessionId.uuidString)",
                "user_input": "plugin echo hello",
                "status": "completed",
                "created_at": "2026-05-20T12:00:00Z",
                "updated_at": "2026-05-20T12:00:01Z"
              },
              "audit_entry": {
                "id": "\(auditId.uuidString)",
                "task_id": "\(taskId.uuidString)",
                "event_type": "plugin_completed",
                "summary": "first-party plugin action finished",
                "payload": {},
                "created_at": "2026-05-20T12:00:01Z"
              },
              "audit_entries": [
                {
                  "id": "\(auditId.uuidString)",
                  "task_id": "\(taskId.uuidString)",
                  "event_type": "plugin_completed",
                  "summary": "first-party plugin action finished",
                  "payload": {},
                  "created_at": "2026-05-20T12:00:01Z"
                }
              ],
              "route": {
                "provider": "local",
                "model": "fake-local-model",
                "reason": "local model is the default route for v1 commands"
              },
              "steps": [
                { "index": 0, "message": "local response: plugin echo hello", "complete": true }
              ],
              "plugin_results": [
                {
                  "status": "completed",
                  "output": { "message": "hello" },
                  "metadata": {
                    "plugin_id": "fake_echo",
                    "action": "echo",
                    "permissions": [],
                    "risk_tier": "low",
                    "approval_required": false,
                    "approval_status": "not_required",
                    "proactive": false,
                    "memory_access": "none",
                    "model_access": "none",
                    "timeout_ms": 5000,
                    "cancellation": "cooperative",
                    "audit_fields": ["message"]
                  }
                }
              ],
              "message": "local response: plugin echo hello"
            }
            """.utf8
        )

        let response = try JSONDecoder().decode(JarvisCommandResponse.self, from: data)
        let entries = ActivityEntry.entries(from: response)

        #expect(response.auditEntries.first?.id == auditId)
        #expect(response.pluginResults.first?.metadata.pluginId == "fake_echo")
        #expect(entries.contains { $0.title == "fake_echo.echo" && $0.badge == "completed" })
        #expect(entries.contains { $0.title == "plugin_completed" && $0.badge == "audit" })
    }

    @Test("Command request encodes explicit command opt-ins for Rust IPC")
    func encodesCommandRequest() throws {
        let cancellationID = UUID()
        let request = JarvisCommandRequest(
            input: "status check",
            dryRun: true,
            memoryContext: true,
            installedWasmTools: true,
            cancellationID: cancellationID
        )
        let data = try JSONEncoder().encode(request)
        let json = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])

        #expect(json["input"] as? String == "status check")
        #expect(json["dry_run"] as? Bool == true)
        #expect(json["memory_context"] as? Bool == true)
        #expect(json["installed_wasm_tools"] as? Bool == true)
        #expect(json["cancellation_id"] as? String == cancellationID.uuidString)
    }

    @Test("IPC client targets the authenticated runtime cancellation endpoint")
    func ipcClientCancelsExactCommandHandle() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [IPCURLProtocol.self]
        let session = URLSession(configuration: configuration)
        let client = JarvisIPCClient(
            endpoint: JarvisEndpoint(baseURL: URL(string: "http://127.0.0.1:7787")!),
            session: session
        )
        let cancellationID = UUID()
        var observedPath: String?
        var observedMethod: String?
        IPCURLProtocol.handler = { request in
            observedPath = request.url?.path(percentEncoded: false)
            observedMethod = request.httpMethod
            let response = HTTPURLResponse(
                url: request.url!,
                statusCode: 200,
                httpVersion: nil,
                headerFields: ["Content-Type": "application/json"]
            )!
            return (
                response,
                Data(
                    """
                    {
                      "cancellation_id": "\(cancellationID.uuidString)",
                      "cancellation_requested": true,
                      "active_execution_found": true,
                      "outcome": "cancellation_requested",
                      "audit_entry": {
                        "id": "11111111-1111-4111-8111-111111111111",
                        "task_id": null,
                        "event_type": "runtime_cancellation_requested",
                        "summary": "runtime cancellation was requested through local IPC",
                        "payload": { "request_content_redacted": true },
                        "created_at": "2026-07-14T12:00:00Z"
                      }
                    }
                    """.utf8
                )
            )
        }
        defer { IPCURLProtocol.handler = nil }

        let response = try await client.cancelCommand(cancellationID: cancellationID)
        #expect(observedMethod == "POST")
        #expect(observedPath == "/runtime/cancellations/\(cancellationID.uuidString)")
        #expect(response.activeExecutionFound)
        #expect(response.outcome == "cancellation_requested")
    }

    @Test("Management client methods send supported Rust IPC requests")
    func managementClientMethodsSendSupportedRequests() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [IPCURLProtocol.self]
        let session = URLSession(configuration: configuration)
        let client = JarvisIPCClient(
            endpoint: JarvisEndpoint(baseURL: URL(string: "http://127.0.0.1:7787")!),
            session: session
        )
        var requests: [(method: String, path: String, body: [String: Any]?)] = []
        let memoryId = UUID()
        let jobId = UUID()
        let occurrenceId = UUID()

        IPCURLProtocol.handler = { request in
            let body = decodeRequestBody(request)
            let requestPath = request.url?.path(percentEncoded: false) ?? ""
            let requestPathWithQuery = request.url?.query
                .map { "\(requestPath)?\($0)" }
                ?? requestPath
            requests.append((request.httpMethod ?? "", requestPathWithQuery, body))

            let response = HTTPURLResponse(
                url: request.url!,
                statusCode: 200,
                httpVersion: nil,
                headerFields: ["Content-Type": "application/json"]
            )!

            switch request.url?.path(percentEncoded: false) {
            case "/contract":
                return (response, contractJSON(exposesApprovalEndpoint: false))
            case "/release/readiness":
                return (response, releaseReadinessJSON())
            case "/release/evidence-status":
                return (response, releaseEvidenceStatusJSON())
            case "/release/live-device-runbook":
                return (response, releaseRunbookJSON(runbook: "live_device"))
            case "/release/signed-distribution-runbook":
                return (response, releaseRunbookJSON(runbook: "signed_distribution"))
            case "/release/plugin-trust-runbook":
                return (response, releaseRunbookJSON(runbook: "plugin_trust"))
            case "/release/evidence-bundle-runbook":
                return (response, releaseRunbookJSON(runbook: "evidence_bundle"))
            case "/memory":
                if request.httpMethod == "GET" {
                    return (response, Data("[]".utf8))
                }
                return (response, memoryItemJSON(id: memoryId))
            case "/memory/classification":
                return (response, memoryClassificationJSON())
            case "/memory/retention-plan":
                return (response, memoryRetentionPlanJSON(id: memoryId))
            case "/memory/\(memoryId.uuidString)":
                return (response, memoryItemJSON(id: memoryId))
            case "/memory/\(memoryId.uuidString)/review":
                return (response, memoryItemJSON(id: memoryId))
            case "/memory/\(memoryId.uuidString)/restore":
                return (response, memoryItemJSON(id: memoryId))
            case "/plugins/manifests":
                return (response, Data("[]".utf8))
            case "/tools/model":
                return (response, modelToolCatalogJSON())
            case "/scheduler/jobs":
                if request.httpMethod == "GET" {
                    return (response, Data("[]".utf8))
                }
                return (response, schedulerJobJSON(id: jobId))
            case "/scheduler/attention":
                return (response, schedulerAttentionJSON(id: jobId))
            case "/scheduler/notification-outbox":
                return (
                    response,
                    Data(
                        """
                        [{
                          "id": "\(occurrenceId.uuidString)",
                          "scheduler_job_id": "\(jobId.uuidString)",
                          "name": "one shot",
                          "occurrence_at": "2026-05-20T12:00:00Z",
                          "notification_kind": "due_now",
                          "revision": 1,
                          "created_at": "2026-05-20T12:00:00Z",
                          "updated_at": "2026-05-20T12:00:01Z",
                          "acknowledged_at": null,
                          "acknowledged_disposition": null
                        }]
                        """.utf8
                    )
                )
            case "/scheduler/notification-outbox/\(occurrenceId.uuidString)/ack":
                return (
                    response,
                    Data(
                        """
                        {
                          "occurrence": {
                            "id": "\(occurrenceId.uuidString)",
                            "scheduler_job_id": "\(jobId.uuidString)",
                            "name": "one shot",
                            "occurrence_at": "2026-05-20T12:00:00Z",
                            "notification_kind": "due_now",
                            "revision": 1,
                            "created_at": "2026-05-20T12:00:00Z",
                            "updated_at": "2026-05-20T12:00:02Z",
                            "acknowledged_at": "2026-05-20T12:00:02Z",
                            "acknowledged_disposition": "suppressed_not_authorized"
                          },
                          "proof_boundary": "test acknowledgement"
                        }
                        """.utf8
                    )
                )
            case "/scheduler/jobs/\(jobId.uuidString)":
                return (response, schedulerJobJSON(id: jobId))
            case "/scheduler/run-due":
                return (response, schedulerRunDueJSON(id: jobId))
            case "/scheduler/recover-stale":
                return (response, schedulerRecoverStaleJSON(id: jobId))
            case "/activity/summary":
                return (response, activitySummaryJSON(taskId: jobId))
            case "/activity/events":
                return (response, activityEventsSSE(taskId: jobId))
            case "/emergency-pause":
                return (response, Data(#"{"paused":false,"reason":null,"paused_at":null,"resumed_at":"2026-05-20T12:00:00Z","cancelled_scheduler_jobs":0}"#.utf8))
            default:
                return (response, Data("[]".utf8))
            }
        }
        defer { IPCURLProtocol.handler = nil }

        _ = try await client.contract()
        _ = try await client.releaseReadiness()
        _ = try await client.releaseEvidenceStatus()
        _ = try await client.releaseLiveDeviceRunbook()
        _ = try await client.releaseSignedDistributionRunbook()
        _ = try await client.releasePluginTrustRunbook()
        _ = try await client.releaseEvidenceBundleRunbook()
        _ = try await client.listMemoryItems(includeDeleted: true)
        _ = try await client.memoryClassification(includeDeleted: true)
        _ = try await client.memoryRetentionPlan()
        _ = try await client.createMemoryItem(
            JarvisCreateMemoryItemRequest(
                category: "release",
                key: "release-gate",
                value: "preview before write",
                provenance: "manual",
                sensitivity: "workspace"
            )
        )
        _ = try await client.memoryItem(id: memoryId)
        _ = try await client.updateMemoryItem(
            id: memoryId,
            request: JarvisMemoryMutationRequest(
                value: "preview then sync",
                provenance: "operator correction",
                sensitivity: "private"
            )
        )
        _ = try await client.reviewMemoryItem(id: memoryId)
        _ = try await client.deleteMemoryItem(id: memoryId)
        _ = try await client.restoreMemoryItem(id: memoryId)
        _ = try await client.listPluginManifests()
        _ = try await client.modelToolCatalog()
        _ = try await client.listSchedulerJobs()
        _ = try await client.schedulerAttention()
        _ = try await client.pendingSchedulerNotificationOccurrences(limit: 100)
        _ = try await client.acknowledgeSchedulerNotificationOccurrence(
            id: occurrenceId,
            request: JarvisSchedulerNotificationAcknowledgementRequest(
                revision: 1,
                disposition: .suppressedNotAuthorized
            )
        )
        _ = try await client.createSchedulerJob(
            JarvisCreateSchedulerJobRequest(
                name: "one shot",
                command: "status check",
                trigger: .manual
            )
        )
        _ = try await client.schedulerJob(id: jobId)
        _ = try await client.runDueSchedulerJobs(limit: 4)
        _ = try await client.recoverStaleSchedulerJobs(olderThanSeconds: 120, limit: 2)
        _ = try await client.activitySummary()
        _ = try await client.activityEvents(maxEvents: 2, intervalMilliseconds: 500)
        _ = try await client.pauseStatus()

        #expect(requests.map(\.method) == [
            "GET",
            "GET",
            "GET",
            "GET",
            "GET",
            "GET",
            "GET",
            "GET",
            "GET",
            "GET",
            "POST",
            "GET",
            "PATCH",
            "POST",
            "DELETE",
            "POST",
            "GET",
            "GET",
            "GET",
            "GET",
            "GET",
            "POST",
            "POST",
            "GET",
            "POST",
            "POST",
            "GET",
            "GET",
            "GET"
        ])
        #expect(requests.map(\.path) == [
            "/contract",
            "/release/readiness",
            "/release/evidence-status",
            "/release/live-device-runbook",
            "/release/signed-distribution-runbook",
            "/release/plugin-trust-runbook",
            "/release/evidence-bundle-runbook",
            "/memory?include_deleted=true",
            "/memory/classification?include_deleted=true",
            "/memory/retention-plan",
            "/memory",
            "/memory/\(memoryId.uuidString)",
            "/memory/\(memoryId.uuidString)",
            "/memory/\(memoryId.uuidString)/review",
            "/memory/\(memoryId.uuidString)",
            "/memory/\(memoryId.uuidString)/restore",
            "/plugins/manifests",
            "/tools/model",
            "/scheduler/jobs",
            "/scheduler/attention",
            "/scheduler/notification-outbox?limit=64",
            "/scheduler/notification-outbox/\(occurrenceId.uuidString)/ack",
            "/scheduler/jobs",
            "/scheduler/jobs/\(jobId.uuidString)",
            "/scheduler/run-due?limit=4",
            "/scheduler/recover-stale?older_than_seconds=120&limit=2",
            "/activity/summary",
            "/activity/events?max_events=2&interval_ms=500",
            "/emergency-pause"
        ])
        #expect(requests[10].body?["key"] as? String == "release-gate")
        #expect(requests[12].body?["value"] as? String == "preview then sync")
        #expect(requests[21].body?["revision"] as? Int == 1)
        #expect(requests[21].body?["disposition"] as? String == "suppressed_not_authorized")
        #expect(requests[22].body?["command"] as? String == "status check")
    }

    @Test("Management payloads decode tasks and audit list")
    func decodesTasksAndAuditList() throws {
        let taskId = UUID()
        let sessionId = UUID()
        let auditId = UUID()
        let tasksData = Data(
            """
            [
              {
                "id": "\(taskId.uuidString)",
                "session_id": "\(sessionId.uuidString)",
                "user_input": "status check",
                "status": "completed",
                "created_at": "2026-05-20T12:00:00Z",
                "updated_at": "2026-05-20T12:00:01Z"
              }
            ]
            """.utf8
        )
        let auditData = Data(
            """
            [
              {
                "id": "\(auditId.uuidString)",
                "task_id": "\(taskId.uuidString)",
                "event_type": "task_completed",
                "summary": "command completed",
                "payload": { "accepted": true, "attempt": 1 },
                "created_at": "2026-05-20T12:00:01Z"
              }
            ]
            """.utf8
        )

        let tasks = try JSONDecoder().decode([JarvisTask].self, from: tasksData)
        let auditEntries = try JSONDecoder().decode([JarvisAuditEntry].self, from: auditData)

        #expect(tasks.first?.id == taskId)
        #expect(tasks.first?.createdAt == "2026-05-20T12:00:00Z")
        #expect(auditEntries.first?.id == auditId)
        #expect(auditEntries.first?.payload == .object([
            "accepted": .bool(true),
            "attempt": .number(1)
        ]))
    }

    @Test("Activity summary decodes current progress counts and recent evidence")
    func decodesActivitySummary() throws {
        let taskId = UUID()
        let summary = try JSONDecoder().decode(
            JarvisActivitySummary.self,
            from: activitySummaryJSON(taskId: taskId)
        )

        #expect(summary.repositoryBacked)
        #expect(summary.taskCount == 2)
        #expect(summary.auditEntryCount == 3)
        #expect(summary.activeTaskCount == 1)
        #expect(summary.statusCounts.contains(JarvisActivityStatusCount(status: "running", count: 1)))
        #expect(summary.statusCounts.contains(JarvisActivityStatusCount(status: "completed", count: 1)))
        #expect(summary.recentTasks.first?.id == taskId)
        #expect(summary.recentTasks.first?.status == "running")
        #expect(summary.recentAuditEntries.first?.eventType == "plugin_completed")
    }

    @Test("Activity event stream parses bounded server-sent summaries and errors")
    func parsesActivityEventStream() throws {
        let taskId = UUID()
        let events = try JarvisActivityEvent.parseServerSentEvents(activityEventsSSE(taskId: taskId))

        #expect(events.count == 5)
        #expect(events.first?.event == "activity_summary")
        #expect(events.first?.summary?.recentTasks.first?.id == taskId)
        #expect(events.first?.summary?.activeTaskCount == 1)
        #expect(events[1].event == "activity_progress")
        #expect(events[1].progress?.kind == "installed_plugin")
        #expect(events[1].progress?.pluginId == "local_runner_test")
        #expect(events[1].progress?.stage == "prepare")
        #expect(events[1].progress?.message == "validated request")
        #expect(events[1].progress?.stderrRedacted == true)
        #expect(events[2].event == "activity_progress")
        #expect(events[2].progress?.kind == "model_step")
        #expect(events[2].progress?.provider == "local")
        #expect(events[2].progress?.model == "fake-local-model")
        #expect(events[2].progress?.sequence == 0)
        #expect(events[2].progress?.stage == "completed")
        #expect(events[3].event == "activity_progress")
        #expect(events[3].progress?.kind == "model_output")
        #expect(events[3].progress?.provider == "local")
        #expect(events[3].progress?.model == "fake-local-model")
        #expect(events[3].progress?.sequence == 0)
        #expect(events[3].progress?.byteCount == 42)
        #expect(events[3].progress?.charCount == 42)
        #expect(events[3].progress?.finalChunk == true)
        #expect(events[3].progress?.contentRedacted == true)
        #expect(events[3].progress?.providerNative == true)
        #expect(events.last?.event == "activity_error")
        #expect(events.last?.error == "Activity stream unavailable. Inspect the redacted audit timeline.")
        #expect(events.last?.error?.contains("repository unavailable") == false)
    }

    @Test("Management payloads decode memory items")
    func decodesMemoryItems() throws {
        let id = UUID()
        let data = Data(
            """
            [
              {
                "id": "\(id.uuidString)",
                "category": "release",
                "key": "release-gate",
                "value": "preview before write",
                "provenance": "manual",
                "sensitivity": "workspace",
                "created_at": "2026-05-20T12:00:00Z",
                "updated_at": "2026-05-20T12:00:01Z",
                "reviewed_at": "2026-05-20T12:00:02Z",
                "deleted_at": null
              }
            ]
            """.utf8
        )

        let items = try JSONDecoder().decode([JarvisMemoryItem].self, from: data)

        #expect(items.first?.id == id)
        #expect(items.first?.category == "release")
        #expect(items.first?.sensitivity == "workspace")
        #expect(items.first?.reviewedAt == "2026-05-20T12:00:02Z")
        #expect(items.first?.deletedAt == nil)
    }

    @Test("Memory classification summary decodes sensitivity and category counts")
    func decodesMemoryClassificationSummary() throws {
        let summary = try JSONDecoder().decode(
            JarvisMemoryClassificationSummary.self,
            from: memoryClassificationJSON()
        )

        #expect(summary.includeDeleted)
        #expect(summary.totalCount == 2)
        #expect(summary.activeCount == 1)
        #expect(summary.deletedCount == 1)
        #expect(summary.unreviewedActiveCount == 1)
        #expect(summary.sensitiveActiveCount == 1)
        #expect(summary.bySensitivity.first?.label == "private")
        #expect(summary.byCategory.first?.label == "release")
    }

    @Test("Memory retention plan decodes redacted operator actions")
    func decodesMemoryRetentionPlan() throws {
        let memoryId = UUID()
        let plan = try JSONDecoder().decode(
            JarvisMemoryRetentionPlan.self,
            from: memoryRetentionPlanJSON(id: memoryId)
        )

        #expect(plan.status == "operator_review_required")
        #expect(plan.candidateCount == 1)
        #expect(plan.unreviewedActiveCount == 0)
        #expect(plan.deletedSensitiveRetainedCount == 1)
        #expect(!plan.automationEnabled)
        #expect(plan.valueRedactionRequired)
        let candidate = try #require(plan.candidates.first)
        #expect(candidate.memoryId == memoryId)
        #expect(candidate.category == "retention")
        #expect(candidate.key == "deleted-secret")
        #expect(candidate.sensitivity == "private")
        #expect(candidate.status == "deleted_sensitive_retained")
        #expect(candidate.recommendedAction == "operator_purge_or_restore")
    }

    @Test("Memory mutation requests encode Rust IPC names")
    func encodesMemoryRequests() throws {
        let create = JarvisCreateMemoryItemRequest(
            category: "release",
            key: "release-gate",
            value: "preview before write",
            provenance: "manual",
            sensitivity: "workspace"
        )
        let update = JarvisMemoryMutationRequest(
            value: "preview then sync",
            provenance: "operator correction",
            sensitivity: "private"
        )

        let createJson = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(create)) as? [String: Any])
        let updateJson = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(update)) as? [String: Any])

        #expect(createJson["category"] as? String == "release")
        #expect(createJson["key"] as? String == "release-gate")
        #expect(createJson["sensitivity"] as? String == "workspace")
        #expect(updateJson["value"] as? String == "preview then sync")
        #expect(updateJson["provenance"] as? String == "operator correction")
    }

    @Test("Management payloads decode plugin manifests")
    func decodesPluginManifests() throws {
        let data = Data(
            """
            [
              {
                "id": "fake_echo",
                "name": "Fake Echo",
                "version": "0.1.0",
                "source": "first_party",
                "author": "Jarvis",
                "actions": [
                  {
                    "name": "echo",
                    "description": "Echo a message.",
                    "permissions": ["read_workspace"],
                    "risk_tier": "low",
                    "input_schema": { "type": "object", "properties": { "message": { "type": "string" } } },
                    "output_schema": { "type": "object" },
                    "proactive": false,
                    "memory_access": "none",
                    "model_access": "none",
                    "audit_fields": ["message"],
                    "timeout": { "timeout_ms": 5000, "on_timeout": "cancel" },
                    "cancellation": "cooperative"
                  }
                ]
              }
            ]
            """.utf8
        )

        let manifests = try JSONDecoder().decode([JarvisPluginManifest].self, from: data)
        let action = try #require(manifests.first?.actions.first)

        #expect(manifests.first?.id == "fake_echo")
        #expect(manifests.first?.source == "first_party")
        #expect(action.riskTier == "low")
        #expect(action.timeout.timeoutMilliseconds == 5000)
        #expect(action.inputSchema == .object([
            "type": .string("object"),
            "properties": .object([
                "message": .object(["type": .string("string")])
            ])
        ]))
    }

    @Test("Management payloads decode the redacted model-visible capability catalog")
    func decodesModelToolCatalog() throws {
        let catalog = try JSONDecoder().decode(
            JarvisModelToolCatalog.self,
            from: modelToolCatalogJSON()
        )
        let tool = try #require(catalog.tools.first)

        #expect(catalog.source == "registered_first_party_plugins")
        #expect(tool.id == "workspace_inspect.list")
        #expect(tool.description == "List a bounded allowlisted workspace directory.")
        #expect(tool.riskTier == "low")
        #expect(tool.scopes == ["read_workspace"])
        #expect(tool.proactive == false)
        #expect(tool.constraints == JarvisModelToolConstraints(
            readOnly: true,
            bounded: true,
            noNetwork: true,
            localModelOnly: true
        ))
        #expect(tool.constraints.hasProductionReadOnlyBoundary)
        #expect(tool.constraints.operatorBadge == "bounded read-only • no network • local model only")
    }

    @Test("Management payloads decode installed plugin registry records")
    func decodesInstalledPluginRecords() throws {
        let records = try JSONDecoder().decode(
            [JarvisInstalledPluginRecord].self,
            from: installedPluginsJSON()
        )
        let record = try #require(records.first)

        #expect(record.id == "local_runner_test")
        #expect(record.manifest.source == "local_subprocess")
        #expect(record.sourcePath == nil)
        #expect(!record.executionEnabled)
        #expect(record.executionGrant == "metadata_only")
        #expect(record.provenance.manifestPath == nil)
        #expect(record.provenance.manifestSha256 == nil)
        #expect(record.provenance.sourcePath == nil)
        #expect(record.provenance.integrityStatus == "not_verified")
        #expect(record.provenance.needsReview)
        #expect(!record.isExecutable)
    }

    @Test("Management payloads decode scheduler jobs and encode scheduler requests")
    func decodesSchedulerJobsAndEncodesRequests() throws {
        let id = UUID()
        let data = Data(
            """
            [
              {
                "id": "\(id.uuidString)",
                "name": "daily preview",
                "command": "events preview",
                "trigger": { "interval": { "every_seconds": 86400 } },
                "status": "scheduled",
                "created_at": "2026-05-20T12:00:00Z",
                "updated_at": "2026-05-20T12:00:01Z",
                "cancelled_at": null,
                "cancellation_reason": null
              }
            ]
            """.utf8
        )
        let request = JarvisCreateSchedulerJobRequest(
            name: "one shot",
            command: "status check",
            trigger: .onceAt(runAt: "2026-05-20T13:00:00Z")
        )

        let jobs = try JSONDecoder().decode([JarvisSchedulerJob].self, from: data)
        let requestJson = try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(request)) as? [String: Any])
        let trigger = try #require(requestJson["trigger"] as? [String: Any])
        let onceAt = try #require(trigger["once_at"] as? [String: Any])

        #expect(jobs.first?.id == id)
        #expect(jobs.first?.trigger == .interval(everySeconds: 86400))
        #expect(onceAt["run_at"] as? String == "2026-05-20T13:00:00Z")
    }

    @Test("Scheduler attention summary decodes redacted notification handoff")
    func decodesSchedulerAttentionSummary() throws {
        let id = UUID()
        let summary = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: id)
        )
        let item = try #require(summary.items.first)

        #expect(summary.attentionRequired)
        #expect(summary.dueCount == 1)
        #expect(summary.scheduledCount == 1)
        #expect(summary.runningCount == 0)
        #expect(summary.failedCount == 0)
        #expect(item.id == id)
        #expect(item.notificationKind == "due_now")
        #expect(item.notificationReason.contains("scheduler job is due"))
    }

    @Test("Scheduler action payloads decode run and stale recovery responses")
    func decodesSchedulerActionResponses() throws {
        let id = UUID()
        let runDue = try JSONDecoder().decode(
            JarvisSchedulerRunResponse.self,
            from: schedulerRunDueJSON(id: id)
        )
        let recovered = try JSONDecoder().decode(
            JarvisSchedulerStaleRecoveryResponse.self,
            from: schedulerRecoverStaleJSON(id: id)
        )

        #expect(!runDue.emergencyPaused)
        #expect(runDue.limit == 4)
        #expect(runDue.executions.first?.job.id == id)
        #expect(runDue.executions.first?.accepted == true)
        #expect(runDue.executions.first?.auditEntries.first?.eventType == "scheduler_job_completed")
        #expect(recovered.olderThanSeconds == 120)
        #expect(recovered.limit == 2)
        #expect(recovered.recovered.first?.job.id == id)
        #expect(recovered.recovered.first?.auditEntry.eventType == "scheduler_stale_running_recovered")
    }

    @Test("Scheduler model runs due jobs and recovers stale jobs through IPC client")
    @MainActor
    func schedulerModelRunsDueAndRecoversStaleJobs() async throws {
        let id = UUID()
        let client = FakeCoreClient(
            schedulerJobs: [
                try JSONDecoder().decode(JarvisSchedulerJob.self, from: schedulerJobJSON(id: id))
            ]
        )
        let model = SchedulerModel(client: client)

        await model.runDue(limit: 4)
        await model.recoverStale(olderThanSeconds: 120, limit: 2)

        #expect(client.runDueSchedulerLimits == [4])
        #expect(client.recoverStaleSchedulerRequests.first?.olderThanSeconds == 120)
        #expect(client.recoverStaleSchedulerRequests.first?.limit == 2)
        #expect(model.lastRunDue?.executions.first?.job.id == id)
        #expect(model.lastStaleRecovery?.recovered.first?.job.id == id)
        #expect(model.jobs.first?.id == id)
        #expect(model.attention != nil)
        #expect(model.lastError == nil)
    }

    @Test("Scheduler notification model requests authorization and delivers due attention")
    @MainActor
    func schedulerNotificationsDeliverDueAttention() async throws {
        let id = UUID()
        let attention = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: id)
        )
        let adapter = FakeSchedulerNotificationAdapter()
        let model = SchedulerNotificationModel(adapter: adapter)

        let deliveredCount = await model.notify(attention: attention)
        let delivered = try #require(adapter.deliveredRequests.first)

        #expect(deliveredCount == 1)
        #expect(adapter.authorizationRequestCount == 1)
        #expect(delivered.schedulerJobId == id)
        #expect(delivered.notificationKind == "due_now")
        #expect(delivered.title == "Scheduler job ready: one shot")
        #expect(model.status == .delivered(1))
    }

    @Test("Scheduler notification model delivers emergency-pause blocked attention")
    @MainActor
    func schedulerNotificationsDeliverEmergencyPauseBlockedAttention() async throws {
        let id = UUID()
        let attention = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(
                id: id,
                emergencyPaused: true,
                notificationKind: "blocked_by_emergency_pause",
                notificationReason: "Emergency pause is active; scheduler job execution is blocked."
            )
        )
        let adapter = FakeSchedulerNotificationAdapter()
        let model = SchedulerNotificationModel(adapter: adapter)

        let deliveredCount = await model.notify(attention: attention)
        let delivered = try #require(adapter.deliveredRequests.first)

        #expect(deliveredCount == 1)
        #expect(adapter.authorizationRequestCount == 1)
        #expect(delivered.schedulerJobId == id)
        #expect(delivered.notificationKind == "blocked_by_emergency_pause")
        #expect(delivered.title == "Scheduler job blocked by pause: one shot")
        #expect(delivered.body == "Emergency pause is active; scheduler job execution is blocked.")
        #expect(model.status == .delivered(1))
    }

    @Test("Scheduler notification model avoids duplicate notifications for the same attention item")
    @MainActor
    func schedulerNotificationsAvoidDuplicates() async throws {
        let attention = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: UUID())
        )
        let adapter = FakeSchedulerNotificationAdapter()
        let model = SchedulerNotificationModel(adapter: adapter)

        let firstCount = await model.notify(attention: attention)
        let secondCount = await model.notify(attention: attention)

        #expect(firstCount == 1)
        #expect(secondCount == 0)
        #expect(adapter.deliveredRequests.count == 1)
        #expect(model.status == .delivered(0))
    }

    @Test("Scheduler notification reset allows recapturing the same attention item")
    @MainActor
    func schedulerNotificationsResetAllowsRecapture() async throws {
        let attention = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: UUID())
        )
        let adapter = FakeSchedulerNotificationAdapter()
        let model = SchedulerNotificationModel(adapter: adapter)

        let firstCount = await model.notify(attention: attention)
        let duplicateCount = await model.notify(attention: attention)
        model.resetDeliveredHistory()
        let recapturedCount = await model.notify(attention: attention)

        #expect(firstCount == 1)
        #expect(duplicateCount == 0)
        #expect(recapturedCount == 1)
        #expect(adapter.deliveredRequests.count == 2)
        #expect(model.lastDeliveredRequests.count == 1)
        #expect(model.status == .delivered(1))
    }

    @Test("Scheduler notification model fails closed when notification authorization is denied")
    @MainActor
    func schedulerNotificationsFailClosedWhenDenied() async throws {
        let attention = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: UUID())
        )
        let adapter = FakeSchedulerNotificationAdapter(authorizationResult: .success(false))
        let model = SchedulerNotificationModel(adapter: adapter)

        let deliveredCount = await model.notify(attention: attention)

        #expect(deliveredCount == 0)
        #expect(adapter.authorizationRequestCount == 1)
        #expect(adapter.deliveredRequests.isEmpty)
        #expect(model.status == .denied)
    }

    @Test("Automatic scheduler notifications never prompt and require prior authorization")
    @MainActor
    func automaticSchedulerNotificationsRequirePriorAuthorization() async throws {
        let attention = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: UUID())
        )
        let adapter = FakeSchedulerNotificationAdapter(authorizationStatus: .notDetermined)
        let model = SchedulerNotificationModel(adapter: adapter)

        let beforeAuthorization = await model.notifyIfAuthorized(attention: attention)
        adapter.authorizationStatusResult = .authorized
        let afterAuthorization = await model.notifyIfAuthorized(attention: attention)
        let duplicate = await model.notifyIfAuthorized(attention: attention)

        #expect(beforeAuthorization == 0)
        #expect(afterAuthorization == 1)
        #expect(duplicate == 0)
        #expect(adapter.authorizationRequestCount == 0)
        #expect(adapter.deliveredRequests.count == 1)
    }

    @Test("Automatic scheduler notifications distinguish recurring occurrences")
    @MainActor
    func automaticSchedulerNotificationsDistinguishRecurringOccurrences() async throws {
        let id = UUID()
        let firstOccurrence = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: id, nextDueAt: "2026-05-20T12:00:01Z")
        )
        let secondOccurrence = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: id, nextDueAt: "2026-05-20T12:05:01Z")
        )
        let adapter = FakeSchedulerNotificationAdapter()
        let model = SchedulerNotificationModel(adapter: adapter)

        #expect(await model.notifyIfAuthorized(attention: firstOccurrence) == 1)
        #expect(await model.notifyIfAuthorized(attention: firstOccurrence) == 0)
        #expect(await model.notifyIfAuthorized(attention: secondOccurrence) == 1)
        #expect(adapter.deliveredRequests.count == 2)
        #expect(Set(adapter.deliveredRequests.map(\.id)).count == 2)
    }

    @Test("Durable scheduler occurrences submit or suppress with explicit acknowledgements")
    @MainActor
    func durableSchedulerOccurrencesReturnExplicitAcknowledgements() async throws {
        let submitted = schedulerNotificationOccurrence(jobId: UUID())
        let submittedAdapter = FakeSchedulerNotificationAdapter()
        let submittedModel = SchedulerNotificationModel(adapter: submittedAdapter)

        let submittedAcknowledgements = await submittedModel
            .notifyPendingOccurrencesIfAuthorized([submitted])
        #expect(submittedAcknowledgements == [
            JarvisSchedulerNotificationAcknowledgement(
                id: submitted.id,
                revision: submitted.revision,
                disposition: .submittedToNotificationCenter
            )
        ])
        #expect(submittedAdapter.authorizationRequestCount == 0)
        #expect(submittedAdapter.deliveredRequests.first?.id ==
            "scheduler-occurrence-\(submitted.id.uuidString)-r1")

        let suppressed = schedulerNotificationOccurrence(jobId: UUID())
        let suppressedAdapter = FakeSchedulerNotificationAdapter(
            authorizationStatus: .notDetermined
        )
        let suppressedModel = SchedulerNotificationModel(adapter: suppressedAdapter)
        let suppressedAcknowledgements = await suppressedModel
            .notifyPendingOccurrencesIfAuthorized([suppressed])
        #expect(suppressedAcknowledgements.first?.disposition == .suppressedNotAuthorized)
        #expect(suppressedAdapter.authorizationRequestCount == 0)
        #expect(suppressedAdapter.deliveredRequests.isEmpty)
    }

    @Test("Snapshot notification deduplication history is bounded")
    @MainActor
    func schedulerSnapshotNotificationHistoryIsBounded() async throws {
        let id = UUID()
        let adapter = FakeSchedulerNotificationAdapter()
        let model = SchedulerNotificationModel(adapter: adapter, deliveredHistoryLimit: 2)
        let first = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: id, nextDueAt: "2026-05-20T12:00:01Z")
        )
        let second = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: id, nextDueAt: "2026-05-20T12:01:01Z")
        )
        let third = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: id, nextDueAt: "2026-05-20T12:02:01Z")
        )

        #expect(await model.notifyIfAuthorized(attention: first) == 1)
        #expect(await model.notifyIfAuthorized(attention: second) == 1)
        #expect(await model.notifyIfAuthorized(attention: third) == 1)
        #expect(await model.notifyIfAuthorized(attention: first) == 1)
        #expect(adapter.deliveredRequests.count == 4)
    }

    @Test("Scheduler automation configuration is bounded, explicit, and persisted")
    @MainActor
    func schedulerAutomationConfigurationIsBoundedAndPersisted() throws {
        let suite = "jarvis-scheduler-automation-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let model = SchedulerAutomationSettingsModel(defaults: defaults, environment: [:])

        #expect(!model.isEnabled)
        #expect(model.schedulerAutomationConfiguration.launchArguments.isEmpty)

        model.update(
            isEnabled: true,
            intervalMilliseconds: 1,
            runLimit: 100,
            recoverStaleOnStartup: true,
            staleAgeSeconds: 1,
            staleRecoveryLimit: 0
        )
        let configuration = model.schedulerAutomationConfiguration
        #expect(configuration.intervalMilliseconds == 1_000)
        #expect(configuration.runLimit == 64)
        #expect(configuration.staleAgeSeconds == 60)
        #expect(configuration.staleRecoveryLimit == 1)
        #expect(configuration.launchArguments == [
            "--scheduler-background",
            "--scheduler-interval-ms", "1000",
            "--scheduler-limit", "64",
            "--scheduler-recover-stale-on-startup",
            "--scheduler-stale-older-than-seconds", "60",
            "--scheduler-stale-recovery-limit", "1"
        ])

        let restored = SchedulerAutomationSettingsModel(defaults: defaults, environment: [:])
        #expect(restored.schedulerAutomationConfiguration == configuration)
    }

    @Test("Scheduler automation environment opt-in is ephemeral")
    @MainActor
    func schedulerAutomationEnvironmentOptInIsEphemeral() throws {
        let suite = "jarvis-scheduler-environment-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }

        let launched = SchedulerAutomationSettingsModel(
            defaults: defaults,
            environment: [SchedulerAutomationSettingsModel.enabledEnvironmentKey: "true"]
        )
        launched.update(runLimit: 4)
        let nextLaunch = SchedulerAutomationSettingsModel(defaults: defaults, environment: [:])

        #expect(launched.isEnabled)
        #expect(!nextLaunch.isEnabled)
        #expect(nextLaunch.runLimit == 4)
    }

    @Test("Scheduler attention coordinator starts only for enabled automation and stops cleanly")
    @MainActor
    func schedulerAttentionCoordinatorLifecycleIsBounded() async throws {
        let settings = StaticSchedulerAutomationConfigurationProvider(
            configuration: JarvisSchedulerAutomationConfiguration(isEnabled: true)
        )
        let scheduler = SchedulerModel(client: FakeCoreClient())
        let notifications = SchedulerNotificationModel(adapter: FakeSchedulerNotificationAdapter())
        let coordinator = SchedulerAttentionCoordinator(
            scheduler: scheduler,
            notifications: notifications,
            settings: settings,
            pollInterval: .milliseconds(10),
            isCoreAvailable: { true }
        )

        coordinator.start()
        #expect(coordinator.isRunning)
        try await Task.sleep(for: .milliseconds(30))
        coordinator.stop()
        #expect(!coordinator.isRunning)
        #expect(scheduler.attention != nil)
        #expect(coordinator.lastError == nil)
    }

    @Test("Scheduler coordinator acknowledges later submissions after one acknowledgement fails")
    @MainActor
    func schedulerCoordinatorContinuesAfterPartialAcknowledgementFailure() async throws {
        let first = schedulerNotificationOccurrence(jobId: UUID())
        let second = schedulerNotificationOccurrence(jobId: UUID())
        let client = FakeCoreClient(
            schedulerNotificationOccurrences: [first, second],
            schedulerNotificationAcknowledgementFailureIDs: [first.id]
        )
        let scheduler = SchedulerModel(client: client)
        let adapter = FakeSchedulerNotificationAdapter(authorizationStatus: .authorized)
        let coordinator = SchedulerAttentionCoordinator(
            scheduler: scheduler,
            notifications: SchedulerNotificationModel(adapter: adapter),
            settings: StaticSchedulerAutomationConfigurationProvider(
                configuration: JarvisSchedulerAutomationConfiguration(isEnabled: true)
            ),
            isCoreAvailable: { true }
        )

        await coordinator.pollOnce()

        #expect(adapter.deliveredRequests.map(\.schedulerNotificationOccurrenceId) == [first.id, second.id])
        #expect(client.schedulerNotificationAcknowledgementIDs == [first.id, second.id])
        let remaining = try await client.pendingSchedulerNotificationOccurrences(limit: 64)
        #expect(remaining.map(\.id) == [first.id])
        #expect(coordinator.lastError != nil)
    }

    @Test("Scheduler attention coordinator rejects stale poll generations")
    @MainActor
    func schedulerAttentionCoordinatorRejectsStalePollGenerations() async throws {
        let suite = "jarvis-scheduler-generation-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SchedulerAutomationSettingsModel(defaults: defaults, environment: [:])
        settings.update(isEnabled: true)
        let attention = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: UUID())
        )
        let scheduler = SchedulerModel(
            client: FakeCoreClient(
                schedulerAttention: attention,
                schedulerAttentionDelayNanoseconds: 80_000_000,
                schedulerNotificationOccurrences: [
                    schedulerNotificationOccurrence(jobId: attention.items[0].id)
                ]
            )
        )
        let adapter = FakeSchedulerNotificationAdapter()
        let notifications = SchedulerNotificationModel(adapter: adapter)
        var available = true
        let coordinator = SchedulerAttentionCoordinator(
            scheduler: scheduler,
            notifications: notifications,
            settings: settings,
            pollInterval: .seconds(1),
            isCoreAvailable: { available }
        )

        coordinator.start()
        try await Task.sleep(for: .milliseconds(10))
        settings.update(isEnabled: false)
        coordinator.reconcile()
        try await Task.sleep(for: .milliseconds(100))
        #expect(!coordinator.isRunning)
        #expect(adapter.deliveredRequests.isEmpty)

        settings.update(isEnabled: true)
        coordinator.reconcile()
        try await Task.sleep(for: .milliseconds(10))
        coordinator.stop()
        coordinator.start()
        for _ in 0..<40 where adapter.deliveredRequests.isEmpty {
            try await Task.sleep(for: .milliseconds(25))
        }
        #expect(coordinator.isRunning)
        #expect(adapter.deliveredRequests.count == 1)

        available = false
        coordinator.reconcile()
        #expect(!coordinator.isRunning)
    }

    @Test("Scheduler attention coordinator cancels during delayed authorization")
    @MainActor
    func schedulerAttentionCoordinatorCancelsDuringDelayedAuthorization() async throws {
        let suite = "jarvis-scheduler-authorization-race-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let settings = SchedulerAutomationSettingsModel(defaults: defaults, environment: [:])
        settings.update(isEnabled: true)
        let attention = try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: schedulerAttentionJSON(id: UUID())
        )
        let adapter = FakeSchedulerNotificationAdapter(
            authorizationStatusWaitsForRelease: true
        )
        let notifications = SchedulerNotificationModel(adapter: adapter)
        let coordinator = SchedulerAttentionCoordinator(
            scheduler: SchedulerModel(client: FakeCoreClient(
                schedulerAttention: attention,
                schedulerNotificationOccurrences: [
                    schedulerNotificationOccurrence(jobId: attention.items[0].id)
                ]
            )),
            notifications: notifications,
            settings: settings,
            pollInterval: .seconds(1),
            isCoreAvailable: { true }
        )

        coordinator.start()
        for _ in 0..<40 where adapter.authorizationStatusCheckCount == 0 {
            try await Task.sleep(for: .milliseconds(25))
        }
        #expect(adapter.authorizationStatusCheckCount == 1)
        #expect(notifications.isWorking)
        settings.update(isEnabled: false)
        coordinator.reconcile()
        adapter.releaseAuthorizationStatus()
        for _ in 0..<40 where notifications.isWorking {
            try await Task.sleep(for: .milliseconds(25))
        }

        #expect(!coordinator.isRunning)
        #expect(!notifications.isWorking)
        #expect(adapter.deliveredRequests.isEmpty)
    }

    @Test("Swift scheduler coordinator observes real IPC background execution and authorized attention")
    @MainActor
    func schedulerCoordinatorRealIPCEndToEnd() async throws {
        let port = try unusedLoopbackPort()
        let endpointURL = try #require(URL(string: "http://127.0.0.1:\(port)"))
        let databaseURL = FileManager.default.temporaryDirectory
            .appending(path: "jarvis-swift-scheduler-e2e-\(UUID().uuidString).sqlite")
        defer { try? FileManager.default.removeItem(at: databaseURL) }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = [
            "cargo", "run", "-q", "-p", "jarvis-cli", "--",
            "serve", "--bind", "127.0.0.1:\(port)",
            "--db-path", databaseURL.path,
            "--scheduler-background", "--scheduler-interval-ms", "50", "--scheduler-limit", "4"
        ]
        process.currentDirectoryURL = jarvisRepositoryRoot()
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        defer {
            if process.isRunning { process.terminate() }
            process.waitUntilExit()
        }

        let client = JarvisIPCClient(endpoint: JarvisEndpoint(baseURL: endpointURL))
        var healthy = false
        for _ in 0..<1_200 where !healthy {
            healthy = (try? await client.health().status) == "ok"
            if !healthy { try await Task.sleep(for: .milliseconds(50)) }
        }
        try #require(healthy)

        let dueAt = ISO8601DateFormatter().string(from: Date().addingTimeInterval(-1))
        let completedCandidate = try await client.createSchedulerJob(
            JarvisCreateSchedulerJobRequest(
                name: "Swift IPC scheduler completion",
                command: "Swift IPC scheduler completion check",
                trigger: .onceAt(runAt: dueAt)
            )
        )
        var completed = completedCandidate
        for _ in 0..<120 where completed.status != "completed" {
            try await Task.sleep(for: .milliseconds(50))
            completed = try await client.schedulerJob(id: completedCandidate.id)
        }
        #expect(completed.status == "completed")
        let audit = try await client.listAuditEntries(taskId: nil)
        #expect(audit.contains(where: { $0.eventType == "scheduler_job_completed" }))

        _ = try await client.pause(reason: "Swift scheduler attention E2E")
        let blockedCandidate = try await client.createSchedulerJob(
            JarvisCreateSchedulerJobRequest(
                name: "Swift IPC paused attention",
                command: "redacted paused scheduler command",
                trigger: .onceAt(runAt: dueAt)
            )
        )
        let scheduler = SchedulerModel(client: client)
        let adapter = FakeSchedulerNotificationAdapter(authorizationStatus: .authorized)
        let notifications = SchedulerNotificationModel(adapter: adapter)
        let settings = StaticSchedulerAutomationConfigurationProvider(
            configuration: JarvisSchedulerAutomationConfiguration(isEnabled: true)
        )
        let coordinator = SchedulerAttentionCoordinator(
            scheduler: scheduler,
            notifications: notifications,
            settings: settings,
            pollInterval: .milliseconds(50),
            isCoreAvailable: { true }
        )
        coordinator.start()
        for _ in 0..<120 where adapter.deliveredRequests.count < 2 {
            try await Task.sleep(for: .milliseconds(50))
        }
        coordinator.stop()
        let deliveredByJob = Dictionary(
            uniqueKeysWithValues: adapter.deliveredRequests.map { ($0.schedulerJobId, $0) }
        )
        let completedDelivery = try #require(deliveredByJob[completedCandidate.id])
        let blockedDelivery = try #require(deliveredByJob[blockedCandidate.id])
        #expect(completedDelivery.notificationKind == "due_now")
        #expect(blockedDelivery.notificationKind == "blocked_by_emergency_pause")
        #expect(completedDelivery.schedulerNotificationOccurrenceId != nil)
        #expect(blockedDelivery.schedulerNotificationOccurrenceId != nil)
        #expect(completedDelivery.schedulerNotificationRevision == 1)
        #expect(blockedDelivery.schedulerNotificationRevision == 1)
        #expect(adapter.authorizationRequestCount == 0)
        #expect(scheduler.attention?.items.contains(where: { $0.id == blockedCandidate.id }) == true)
        let pendingAfterAcknowledgement = try await client.pendingSchedulerNotificationOccurrences(limit: 64)
        #expect(pendingAfterAcknowledgement.isEmpty)
        let acknowledgedAudit = try await client.listAuditEntries(taskId: nil)
        #expect(
            acknowledgedAudit.filter { $0.eventType == "scheduler_notification_acknowledged" }.count >= 2
        )

        let deliveredCount = adapter.deliveredRequests.count
        coordinator.start()
        try await Task.sleep(for: .milliseconds(150))
        coordinator.stop()
        #expect(adapter.deliveredRequests.count == deliveredCount)
        _ = try await client.resume()
    }

    @Test("Pause status decodes detailed timestamps")
    func decodesPauseStatusDetail() throws {
        let data = Data(
            """
            {
              "paused": true,
              "reason": "operator hold",
              "paused_at": "2026-05-20T12:00:00Z",
              "resumed_at": null,
              "cancelled_scheduler_jobs": 3
            }
            """.utf8
        )

        let response = try JSONDecoder().decode(JarvisPauseResponse.self, from: data)

        #expect(response.paused)
        #expect(response.reason == "operator hold")
        #expect(response.pausedAt == "2026-05-20T12:00:00Z")
        #expect(response.resumedAt == nil)
        #expect(response.cancelledSchedulerJobs == 3)
    }

    @Test("Diagnostics export decodes redacted core contract")
    func decodesDiagnosticsExport() throws {
        let jobId = UUID()
        let data = Data(
            """
            {
              "generated_at": "2026-05-20T12:00:00Z",
              "redaction": "diagnostics export omits command bodies and emergency-pause reason text",
              "health": {
                "status": "ok",
                "version": "0.1.0",
                "started_at": "2026-05-20T11:59:00Z",
                "emergency_paused": true,
                "emergency_pause_reason": "redacted",
                "emergency_pause_reason_present": true,
                "emergency_pause_updated_at": "2026-05-20T12:00:00Z",
                "scheduler_jobs": 1,
                "command_runtime": "routed-fake-local-model+first-party-plugins"
              },
              "scheduler_jobs": [
                {
                  "id": "\(jobId.uuidString)",
                  "name": "daily preview",
                  "trigger": "manual",
                  "status": "scheduled",
                  "created_at": "2026-05-20T12:00:00Z",
                  "updated_at": "2026-05-20T12:00:01Z",
                  "cancelled_at": null,
                  "cancellation_reason_present": false
                }
              ],
              "repository_backed": true,
              "schema_version": 1,
              "task_count": 2,
              "audit_entry_count": 4,
              "active_memory_item_count": 3,
              "unreviewed_memory_item_count": 2,
              "sensitive_memory_item_count": 1
            }
            """.utf8
        )

        let export = try JSONDecoder().decode(JarvisDiagnosticsExport.self, from: data)

        #expect(export.generatedAt == "2026-05-20T12:00:00Z")
        #expect(export.repositoryBacked)
        #expect(export.schemaVersion == 1)
        #expect(export.taskCount == 2)
        #expect(export.auditEntryCount == 4)
        #expect(export.activeMemoryItemCount == 3)
        #expect(export.unreviewedMemoryItemCount == 2)
        #expect(export.sensitiveMemoryItemCount == 1)
        #expect(export.schedulerJobs.first?.id == jobId)
        #expect(export.schedulerJobs.first?.trigger == .manual)
        #expect(export.health.emergencyPaused)
        #expect(export.health.emergencyPauseReason == .redacted)
        #expect(export.health.emergencyPauseReasonPresent)
        #expect(export.health.emergencyPauseSummary == "paused (reason recorded and redacted)")
    }

    @Test("Diagnostics reject arbitrary emergency-pause reason markers")
    func diagnosticsRejectArbitraryPauseReasonMarker() {
        let data = Data(
            """
            {
              "generated_at": "2026-05-20T12:00:00Z",
              "redaction": "redacted",
              "health": {
                "status": "ok",
                "version": "0.1.0",
                "emergency_paused": true,
                "emergency_pause_reason": "must-not-decode",
                "emergency_pause_reason_present": true,
                "emergency_pause_updated_at": "2026-05-20T12:00:00Z",
                "scheduler_jobs": 0,
                "command_runtime": "test"
              },
              "scheduler_jobs": [],
              "repository_backed": false
            }
            """.utf8
        )

        #expect(throws: DecodingError.self) {
            try JSONDecoder().decode(JarvisDiagnosticsExport.self, from: data)
        }
    }

    @Test("Diagnostics client method requests diagnostics export endpoint")
    func diagnosticsClientMethodSendsSupportedRequest() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [IPCURLProtocol.self]
        let session = URLSession(configuration: configuration)
        let client = JarvisIPCClient(
            endpoint: JarvisEndpoint(baseURL: URL(string: "http://127.0.0.1:7787")!),
            session: session
        )
        var requestPath: String?

        IPCURLProtocol.handler = { request in
            requestPath = request.url?.path(percentEncoded: false)
            return (
                HTTPURLResponse(
                    url: request.url!,
                    statusCode: 200,
                    httpVersion: nil,
                    headerFields: ["Content-Type": "application/json"]
                )!,
                diagnosticsJSON()
            )
        }
        defer { IPCURLProtocol.handler = nil }

        let export = try await client.diagnosticsExport()

        #expect(requestPath == "/diagnostics/export")
        #expect(export.health.status == "ok")
    }

    @MainActor
    @Test("Release readiness model loads conservative blockers")
    func releaseReadinessModelLoadsBlockers() async throws {
        let readiness = try JSONDecoder().decode(JarvisReleaseReadiness.self, from: releaseReadinessJSON())
        let evidence = try JSONDecoder().decode(JarvisReleaseEvidenceStatus.self, from: releaseEvidenceStatusJSON())
        let model = ReleaseReadinessModel(client: FakeCoreClient(releaseReadiness: readiness, releaseEvidenceStatus: evidence))

        await model.refresh()

        #expect(model.readiness?.productionReady == false)
        #expect(model.readiness?.evidenceModeEnabled == false)
        #expect(model.evidenceStatus?.complete == false)
        #expect(model.releaseRunbooks.map(\.runbook) == ["signed_distribution", "live_device", "plugin_trust", "evidence_bundle"])
        #expect(!model.effectiveProductionReady)
        #expect(model.evidenceStatus?.items.map(\.key).contains("live_device_qa_report") == true)
        #expect(model.evidenceStatus?.items.first { $0.key == "signed_app_bundle" }?.detail.contains("Info.plist bundle identifier") == true)
        #expect(model.evidenceStatus?.items.first { $0.key == "app_executable" }?.detail.contains("presence only") == true)
        #expect(model.readiness?.implementedFeatures.map(\.key).contains("repository_state") == true)
        #expect(model.readiness?.implementedFeatures.map(\.key).contains("release_evidence_status") == true)
        #expect(model.readiness?.implementedFeatures.map(\.key).contains("release_evidence_bundle") == true)
        #expect(model.readiness?.pendingFeatures.map(\.key).contains("live_voice_loop") == true)
        #expect(model.readiness?.blockingManualGates.contains("Developer ID Application and Installer signing credentials configured and used for a full signed package run") == true)
        #expect(model.readiness?.blockingManualGates.contains("final release evidence bundle generated and archived after signed distribution, live-device QA, and plugin-trust QA reports exist") == true)
        #expect(model.readiness?.proofBoundary.contains("does not perform signing") == true)
        #expect(model.lastError == nil)
        #expect(model.isShowingStaleReadiness == false)
    }

    @MainActor
    @Test("Release readiness model surfaces runbook load warning")
    func releaseReadinessModelSurfacesRunbookLoadWarning() async throws {
        let readiness = try JSONDecoder().decode(JarvisReleaseReadiness.self, from: releaseReadinessJSON())
        let evidence = try JSONDecoder().decode(JarvisReleaseEvidenceStatus.self, from: releaseEvidenceStatusJSON())
        let model = ReleaseReadinessModel(
            client: FakeCoreClient(
                releaseReadiness: readiness,
                releaseEvidenceStatus: evidence,
                releaseSignedDistributionRunbookResults: [.failure(URLError(.cannotLoadFromNetwork))]
            )
        )

        await model.refresh()

        #expect(model.readiness?.productionReady == false)
        #expect(model.evidenceStatus?.complete == false)
        #expect(model.releaseRunbooks.isEmpty)
        #expect(model.releaseRunbookWarning?.contains("Release runbooks could not be loaded") == true)
        #expect(model.lastError == nil)
        #expect(model.isShowingStaleReadiness == false)
        #expect(!model.effectiveProductionReady)
    }

    @Test("Release decoders accept live CLI fallback JSON")
    func releaseDecodersAcceptLiveCLIFallbackJSON() throws {
        let endpoint = "http://127.0.0.1:9"
        let readinessData = try runJarvisCLIJSON([
            "release",
            "readiness",
            "--json",
            "--endpoint",
            endpoint
        ])
        let evidenceStatusData = try runJarvisCLIJSON([
            "release",
            "evidence-status",
            "--json",
            "--endpoint",
            endpoint
        ])

        let readiness = try JSONDecoder().decode(JarvisReleaseReadiness.self, from: readinessData)
        let evidenceStatus = try JSONDecoder().decode(JarvisReleaseEvidenceStatus.self, from: evidenceStatusData)

        #expect(readiness.productionReady == false)
        #expect(readiness.implementedFeatures.count == readiness.verifiedFeatureCount)
        #expect(readiness.pendingFeatures.count == readiness.pendingFeatureCount)
        #expect(readiness.implementedFeatures.map(\.key).contains("release_evidence_status"))
        #expect(readiness.implementedFeatures.map(\.key).contains("release_evidence_bundle"))
        #expect(readiness.pendingFeatures.map(\.key).contains("live_voice_loop"))
        #expect(readiness.proofBoundary.contains("does not perform signing"))

        let satisfied = evidenceStatus.items.filter { $0.status == "present" }.count
        let missing = evidenceStatus.items.filter { $0.status == "missing" }.count
        let invalid = evidenceStatus.items.filter { $0.status == "invalid" }.count
        #expect(evidenceStatus.satisfiedCount == satisfied)
        #expect(evidenceStatus.missingCount == missing)
        #expect(evidenceStatus.invalidCount == invalid)
        #expect(evidenceStatus.items.map(\.key).contains("live_device_qa_report"))
        #expect(evidenceStatus.items.map(\.key).contains("plugin_trust_qa_report"))
        #expect(evidenceStatus.items.map(\.key).contains("release_evidence_bundle"))
        #expect(evidenceStatus.proofBoundary.contains("does not sign"))
        #expect(evidenceStatus.proofBoundary.contains("repository-backed task/audit command-result evidence resolution"))
        #expect(evidenceStatus.proofBoundary.contains("execute release artifacts"))
    }

    @Test("Release decoders accept live CLI runbook JSON")
    func releaseDecodersAcceptLiveCLIRunbookJSON() throws {
        let endpoint = "http://127.0.0.1:9"
        let signedDistribution = try JSONDecoder().decode(
            JarvisCLISignedDistributionRunbook.self,
            from: runJarvisCLIJSON([
                "release",
                "signed-distribution-runbook",
                "--json",
                "--endpoint",
                endpoint,
            ])
        )
        let liveDevice = try JSONDecoder().decode(
            JarvisCLILiveDeviceRunbook.self,
            from: runJarvisCLIJSON([
                "release",
                "live-device-runbook",
                "--json",
                "--endpoint",
                endpoint,
            ])
        )
        let pluginTrust = try JSONDecoder().decode(
            JarvisCLIPluginTrustRunbook.self,
            from: runJarvisCLIJSON([
                "release",
                "plugin-trust-runbook",
                "--json",
                "--endpoint",
                endpoint,
            ])
        )
        let evidenceBundle = try JSONDecoder().decode(
            JarvisCLIEvidenceBundleRunbook.self,
            from: runJarvisCLIJSON([
                "release",
                "evidence-bundle-runbook",
                "--json",
                "--endpoint",
                endpoint,
            ])
        )

        #expect(signedDistribution.generatedFrom == "release readiness plus evidence-status")
        #expect(signedDistribution.productionReady == false)
        #expect(signedDistribution.distributionEvidence.map(\.key).contains("signed_app_bundle"))
        #expect(signedDistribution.distributionEvidence.map(\.key).contains("signed_distribution_provenance_report"))
        #expect(signedDistribution.commands.contains("./scripts/package-distribution.sh --check"))
        #expect(!signedDistribution.manualChecks.isEmpty)
        #expect(signedDistribution.proofBoundary.contains("Runbook"))
        #expect(signedDistribution.proofBoundary.contains("only"))

        #expect(liveDevice.generatedFrom == "release readiness plus evidence-status")
        #expect(liveDevice.productionReady == false)
        #expect(liveDevice.liveDeviceEvidence.key == "live_device_qa_report")
        #expect(liveDevice.liveVoiceFeature.key == "live_voice_loop")
        #expect(liveDevice.commands.contains("./scripts/release-live-device-qa.sh --check"))
        #expect(!liveDevice.manualChecks.isEmpty)
        #expect(liveDevice.proofBoundary.contains("Runbook"))
        #expect(liveDevice.proofBoundary.contains("only"))

        #expect(pluginTrust.generatedFrom == "release readiness plus evidence-status")
        #expect(pluginTrust.productionReady == false)
        #expect(pluginTrust.pluginTrustEvidence.key == "plugin_trust_qa_report")
        #expect(pluginTrust.commands.contains("./scripts/release-plugin-trust-qa.sh --check"))
        #expect(!pluginTrust.manualChecks.isEmpty)
        #expect(pluginTrust.proofBoundary.contains("Runbook"))
        #expect(pluginTrust.proofBoundary.contains("only"))

        #expect(evidenceBundle.generatedFrom == "release readiness plus evidence-status")
        #expect(evidenceBundle.productionReady == false)
        #expect(evidenceBundle.childEvidence.map(\.key).contains("signed_distribution_provenance_report"))
        #expect(evidenceBundle.childEvidence.map(\.key).contains("live_device_qa_report"))
        #expect(evidenceBundle.childEvidence.map(\.key).contains("plugin_trust_qa_report"))
        #expect(evidenceBundle.finalBundleEvidence.key == "release_evidence_bundle")
        #expect(evidenceBundle.commands.contains("./scripts/release-evidence-bundle.sh --check"))
        #expect(evidenceBundle.commands.contains("./scripts/release-evidence-doctor.sh --assert-complete"))
        #expect(!evidenceBundle.manualChecks.isEmpty)
        #expect(evidenceBundle.proofBoundary.contains("Runbook"))
        #expect(evidenceBundle.proofBoundary.contains("does not generate the final bundle"))
    }

    @MainActor
    @Test("Release readiness model surfaces invalid live-device evidence details")
    func releaseReadinessModelSurfacesInvalidLiveDeviceEvidenceDetails() async throws {
        let readiness = try JSONDecoder().decode(JarvisReleaseReadiness.self, from: releaseReadinessJSON())
        let evidence = try JSONDecoder().decode(JarvisReleaseEvidenceStatus.self, from: invalidLiveDeviceEvidenceStatusJSON())
        let model = ReleaseReadinessModel(client: FakeCoreClient(releaseReadiness: readiness, releaseEvidenceStatus: evidence))

        await model.refresh()

        let liveDeviceItem = try #require(model.evidenceStatus?.items.first { $0.key == "live_device_qa_report" })
        #expect(model.evidenceStatus?.complete == false)
        #expect(model.evidenceStatus?.invalidCount == 1)
        #expect(liveDeviceItem.status == "invalid")
        #expect(liveDeviceItem.detail.contains("app_bundle.bundle_identifier"))
        #expect(model.readiness?.pendingFeatures.map(\.key).contains("live_voice_loop") == true)
        #expect(model.lastError == nil)
    }

    @MainActor
    @Test("Release readiness model loads external production-ready evidence")
    func releaseReadinessModelLoadsExternalProductionReadyEvidence() async throws {
        let readiness = try JSONDecoder().decode(JarvisReleaseReadiness.self, from: externalProductionReadyReleaseReadinessJSON())
        let evidence = try JSONDecoder().decode(JarvisReleaseEvidenceStatus.self, from: completeReleaseEvidenceStatusJSON())
        let model = ReleaseReadinessModel(client: FakeCoreClient(releaseReadiness: readiness, releaseEvidenceStatus: evidence))

        await model.refresh()

        #expect(model.readiness?.productionReady == true)
        #expect(model.readiness?.evidenceModeEnabled == true)
        #expect(model.readiness?.pendingFeatures.isEmpty == true)
        #expect(model.readiness?.blockingManualGates.isEmpty == true)
        #expect(model.evidenceStatus?.complete == true)
        #expect(model.effectiveProductionReady)
        #expect(model.evidenceStatus?.items.first { $0.key == "live_device_qa_report" }?.status == "present")
        #expect(model.evidenceStatus?.items.first { $0.key == "live_device_qa_report" }?.detail.contains("bundled_core.sha256 matches signed-provenance") == true)
        #expect(model.evidenceStatus?.items.first { $0.key == "plugin_trust_qa_report" }?.status == "present")
        #expect(model.evidenceStatus?.items.first { $0.key == "plugin_trust_qa_report" }?.detail.contains("review_source=owner-asserted-manual-review") == true)
        #expect(model.evidenceStatus?.items.first { $0.key == "release_evidence_bundle" }?.status == "present")
        #expect(model.evidenceStatus?.items.first { $0.key == "release_evidence_bundle" }?.detail.contains("child reports are semantically valid") == true)
        #expect(model.evidenceStatus?.items.first { $0.key == "release_evidence_bundle" }?.detail.contains("owner completion is ordered after child report generation") == true)
        #expect(model.lastError == nil)
        #expect(model.isShowingStaleReadiness == false)
    }

    @MainActor
    @Test("Release readiness model blocks effective readiness when evidence is incomplete")
    func releaseReadinessModelBlocksEffectiveReadinessWhenEvidenceIsIncomplete() async throws {
        let readiness = try JSONDecoder().decode(
            JarvisReleaseReadiness.self,
            from: externalProductionReadyReleaseReadinessJSON()
        )
        let incompleteEvidence = try JSONDecoder().decode(
            JarvisReleaseEvidenceStatus.self,
            from: releaseEvidenceStatusJSON()
        )
        let model = ReleaseReadinessModel(
            client: FakeCoreClient(
                releaseReadiness: readiness,
                releaseEvidenceStatus: incompleteEvidence
            )
        )

        await model.refresh()

        #expect(model.readiness?.productionReady == true)
        #expect(model.evidenceStatus?.complete == false)
        #expect(model.evidenceStatus?.missingCount ?? 0 > 0)
        #expect(!model.effectiveProductionReady)
        #expect(model.lastError == nil)
    }

    @MainActor
    @Test("Release readiness model blocks effective readiness when evidence is invalid")
    func releaseReadinessModelBlocksEffectiveReadinessWhenEvidenceIsInvalid() async throws {
        let readiness = try JSONDecoder().decode(
            JarvisReleaseReadiness.self,
            from: externalProductionReadyReleaseReadinessJSON()
        )
        let invalidEvidence = try JSONDecoder().decode(
            JarvisReleaseEvidenceStatus.self,
            from: invalidLiveDeviceEvidenceStatusJSON()
        )
        let model = ReleaseReadinessModel(
            client: FakeCoreClient(
                releaseReadiness: readiness,
                releaseEvidenceStatus: invalidEvidence
            )
        )

        await model.refresh()

        #expect(model.readiness?.productionReady == true)
        #expect(model.evidenceStatus?.invalidCount == 1)
        #expect(!model.effectiveProductionReady)
        #expect(model.lastError == nil)
    }

    @MainActor
    @Test("Release readiness model marks cached readiness stale after refresh failure")
    func releaseReadinessModelMarksCachedReadinessStaleAfterRefreshFailure() async throws {
        let readiness = try JSONDecoder().decode(JarvisReleaseReadiness.self, from: releaseReadinessJSON())
        let evidence = try JSONDecoder().decode(JarvisReleaseEvidenceStatus.self, from: releaseEvidenceStatusJSON())
        let client = FakeCoreClient(
            releaseReadinessResults: [
                .success(readiness),
                .failure(URLError(.cannotConnectToHost))
            ],
            releaseEvidenceStatus: evidence
        )
        let model = ReleaseReadinessModel(client: client)

        await model.refresh()
        #expect(model.readiness?.generatedAt == "2026-05-22T08:00:00Z")
        #expect(model.isShowingStaleReadiness == false)
        #expect(!model.effectiveProductionReady)

        await model.refresh()
        #expect(model.readiness?.generatedAt == "2026-05-22T08:00:00Z")
        #expect(model.lastError != nil)
        #expect(model.isShowingStaleReadiness == true)
        #expect(!model.effectiveProductionReady)
    }

    @MainActor
    @Test("Release readiness model blocks stale external production ready evidence")
    func releaseReadinessModelBlocksStaleExternalProductionReadyEvidence() async throws {
        let readiness = try JSONDecoder().decode(
            JarvisReleaseReadiness.self,
            from: externalProductionReadyReleaseReadinessJSON()
        )
        let evidence = try JSONDecoder().decode(
            JarvisReleaseEvidenceStatus.self,
            from: completeReleaseEvidenceStatusJSON()
        )
        let client = FakeCoreClient(
            releaseReadinessResults: [
                .success(readiness),
                .failure(URLError(.cannotConnectToHost))
            ],
            releaseEvidenceStatus: evidence
        )
        let model = ReleaseReadinessModel(client: client)

        await model.refresh()
        #expect(model.effectiveProductionReady)

        await model.refresh()
        #expect(model.readiness?.productionReady == true)
        #expect(model.evidenceStatus?.complete == true)
        #expect(model.isShowingStaleReadiness)
        #expect(!model.effectiveProductionReady)
    }

    @MainActor
    @Test("Release readiness model blocks production ready when evidence status refresh fails")
    func releaseReadinessModelBlocksProductionReadyWhenEvidenceStatusRefreshFails() async throws {
        let readiness = try JSONDecoder().decode(
            JarvisReleaseReadiness.self,
            from: externalProductionReadyReleaseReadinessJSON()
        )
        let evidence = try JSONDecoder().decode(
            JarvisReleaseEvidenceStatus.self,
            from: completeReleaseEvidenceStatusJSON()
        )
        let client = FakeCoreClient(
            releaseReadinessResults: [
                .success(readiness),
                .success(readiness)
            ],
            releaseEvidenceStatusResults: [
                .success(evidence),
                .failure(URLError(.cannotConnectToHost))
            ]
        )
        let model = ReleaseReadinessModel(client: client)

        await model.refresh()
        #expect(model.readiness?.productionReady == true)
        #expect(model.evidenceStatus?.complete == true)
        #expect(model.lastError == nil)
        #expect(model.isShowingStaleReadiness == false)
        #expect(model.effectiveProductionReady)

        await model.refresh()
        #expect(model.readiness?.productionReady == true)
        #expect(model.evidenceStatus?.complete == true)
        #expect(model.lastError != nil)
        #expect(model.isShowingStaleReadiness)
        #expect(!model.effectiveProductionReady)
    }

    @Test("Approval queue remains inspection-only when core has no approval endpoint")
    func approvalQueueReflectsCoreContract() throws {
        let taskId = UUID()
        let auditId = UUID()
        let task = JarvisTask(
            id: taskId,
            sessionId: UUID(),
            userInput: "private model route",
            status: "waiting_for_approval",
            createdAt: "2026-05-20T12:00:00Z",
            updatedAt: "2026-05-20T12:00:01Z"
        )
        let audit = JarvisAuditEntry(
            id: auditId,
            taskId: taskId,
            eventType: "plugin_approval_required",
            summary: "Tool execution requires approval before continuing.",
            payload: .object(["approval_status": .string("pending")]),
            createdAt: "2026-05-20T12:00:01Z"
        )
        let contract = try JSONDecoder().decode(
            JarvisContractResponse.self,
            from: contractJSON(exposesApprovalEndpoint: false)
        )

        let items = JarvisApprovalQueueItem.pendingItems(
            tasks: [task],
            auditEntries: [audit],
            contract: contract
        )

        #expect(items.count == 2)
        #expect(items.allSatisfy { !$0.actionAvailable })
        #expect(items.contains { $0.id == taskId && $0.source == "task" })
        #expect(items.contains { $0.id == auditId && $0.approvalStatus == "pending" })
    }

    @Test("Pending approval payload decodes Rust IPC contract names")
    func decodesPendingApproval() throws {
        let approvalId = UUID()
        let taskId = UUID()
        let data = pendingApprovalJSON(
            id: approvalId,
            taskId: taskId,
            status: "pending",
            decidedBy: nil,
            decisionReason: nil
        )

        let approval = try JSONDecoder().decode(JarvisPendingApproval.self, from: data)
        let item = JarvisApprovalQueueItem(approval: approval, actionAvailable: true)

        #expect(approval.id == approvalId)
        #expect(approval.taskId == taskId)
        #expect(approval.action == "fake_echo.approval_echo")
        #expect(approval.requestedScopes == ["conversation", "file_write"])
        #expect(approval.riskTier == "confirm")
        #expect(approval.status == "pending")
        #expect(approval.decisionReason == nil)
        #expect(item.id == approvalId)
        #expect(item.actionAvailable)
        #expect(item.title == "fake_echo.approval_echo")
        #expect(item.detail == "Tool execution requires approval before continuing.")
    }

    @Test("Permission grant summary decodes approval history and installed plugin grants")
    func decodesPermissionGrantSummary() throws {
        let approvalId = UUID()
        let taskId = UUID()
        let data = permissionGrantSummaryJSON(approvalId: approvalId, taskId: taskId)

        let summary = try JSONDecoder().decode(JarvisPermissionGrantSummary.self, from: data)

        #expect(summary.count(for: "pending") == 1)
        #expect(summary.count(for: "approved") == 2)
        #expect(summary.latestApprovals.first?.id == approvalId)
        #expect(summary.highRiskPendingCount == 1)
        #expect(summary.unverifiedInstalledPluginCount == 1)
        #expect(summary.installedPluginGrants.first?.pluginId == "local_e2e_plugin")
        #expect(summary.installedPluginGrants.first?.executionGrant == "metadata_only")
        #expect(summary.installedPluginGrants.first?.executionEnabled == false)
        #expect(summary.installedPluginGrants.first?.integrityStatus == "not_verified")
        #expect(summary.installedPluginGrants.first?.captureMethod == "local_manifest_snapshot")
        #expect(summary.installedPluginGrants.first?.originClaim == "Jarvis Test")
        #expect(summary.installedPluginGrants.first?.originClaimVerified == false)
        #expect(summary.installedPluginGrants.first?.needsProvenanceReview == true)
        #expect(summary.sideEffectsRequireApproval)
    }

    @Test("Permission policy review decodes explicit review items")
    func decodesPermissionPolicyReview() throws {
        let approvalId = UUID()
        let review = try JSONDecoder().decode(
            JarvisPermissionPolicyReview.self,
            from: permissionPolicyReviewJSON(approvalId: approvalId)
        )
        let item = try #require(review.items.first)

        #expect(review.status == "review_required")
        #expect(review.reviewItemCount == 4)
        #expect(review.highRiskPendingCount == 1)
        #expect(review.unverifiedInstalledPluginCount == 1)
        #expect(review.unreviewedMemoryItemCount == 1)
        #expect(review.sensitiveMemoryItemCount == 1)
        #expect(item.approvalId == approvalId)
        #expect(item.severity == "high")
        #expect(item.title.contains("approval"))
        let memoryItem = try #require(review.items.first { $0.itemType == "memory_review" })
        #expect(memoryItem.memoryId != nil)
        #expect(memoryItem.action == "preference/voice")
        #expect(!memoryItem.detail.contains("never expose"))
        let retentionItem = try #require(review.items.first { $0.itemType == "memory_retention_review" })
        #expect(retentionItem.memoryId != nil)
        #expect(retentionItem.action == "retention/deleted-secret")
        #expect(!retentionItem.detail.contains("deleted secret value"))
    }

    @Test("Approval client methods send Rust IPC decision requests")
    func approvalClientMethodsSendDecisionRequests() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [IPCURLProtocol.self]
        let session = URLSession(configuration: configuration)
        let client = JarvisIPCClient(
            endpoint: JarvisEndpoint(baseURL: URL(string: "http://127.0.0.1:7787")!),
            session: session
        )
        let approvalId = UUID()
        let taskId = UUID()
        var requests: [(method: String, path: String, query: String?, body: [String: Any]?)] = []

        IPCURLProtocol.handler = { request in
            requests.append((
                request.httpMethod ?? "",
                request.url?.path(percentEncoded: false) ?? "",
                request.url?.query(percentEncoded: false),
                decodeRequestBody(request)
            ))
            let response = HTTPURLResponse(
                url: request.url!,
                statusCode: 200,
                httpVersion: nil,
                headerFields: ["Content-Type": "application/json"]
            )!

            switch request.url?.path(percentEncoded: false) {
            case "/approvals":
                return (response, Data("[\(String(decoding: pendingApprovalJSON(id: approvalId, taskId: taskId), as: UTF8.self))]".utf8))
            case "/permissions/grants":
                return (response, permissionGrantSummaryJSON(approvalId: approvalId, taskId: taskId))
            case "/permissions/policy-review":
                return (response, permissionPolicyReviewJSON(approvalId: approvalId))
            case "/approvals/\(approvalId.uuidString)":
                return (response, pendingApprovalJSON(id: approvalId, taskId: taskId))
            case "/approvals/\(approvalId.uuidString)/approve":
                return (response, pendingApprovalJSON(id: approvalId, taskId: taskId, status: "approved", decidedBy: "mac-ui", decisionReason: "reviewed"))
            case "/approvals/\(approvalId.uuidString)/deny":
                return (response, pendingApprovalJSON(id: approvalId, taskId: taskId, status: "denied", decidedBy: "mac-ui", decisionReason: "too risky"))
            case "/approvals/\(approvalId.uuidString)/execute":
                return (response, approvalExecutionJSON(approvalId: approvalId, taskId: taskId))
            default:
                return (response, Data("{}".utf8))
            }
        }
        defer { IPCURLProtocol.handler = nil }

        let pending = try await client.listApprovals(status: "pending")
        let grants = try await client.permissionGrantSummary()
        let review = try await client.permissionPolicyReview()
        _ = try await client.approval(id: approvalId)
        let approved = try await client.approveApproval(
            id: approvalId,
            request: JarvisApprovalDecisionRequest(decidedBy: "mac-ui", reason: "reviewed")
        )
        let denied = try await client.denyApproval(
            id: approvalId,
            request: JarvisApprovalDecisionRequest(decidedBy: "mac-ui", reason: "too risky")
        )
        let executionCancellationID = UUID()
        let executed = try await client.executeApproval(
            id: approvalId,
            cancellationID: executionCancellationID
        )

        #expect(pending.first?.id == approvalId)
        #expect(grants.highRiskPendingCount == 1)
        #expect(review.reviewItemCount == 4)
        #expect(approved.status == "approved")
        #expect(denied.status == "denied")
        #expect(executed.accepted)
        #expect(executed.auditEntry.eventType == "approval_executed")
        #expect(requests.map(\.method) == ["GET", "GET", "GET", "GET", "POST", "POST", "POST"])
        #expect(requests.map(\.path) == [
            "/approvals",
            "/permissions/grants",
            "/permissions/policy-review",
            "/approvals/\(approvalId.uuidString)",
            "/approvals/\(approvalId.uuidString)/approve",
            "/approvals/\(approvalId.uuidString)/deny",
            "/approvals/\(approvalId.uuidString)/execute"
        ])
        #expect(requests[0].query == "status=pending")
        #expect(requests[4].body?["decided_by"] as? String == "mac-ui")
        #expect(requests[4].body?["reason"] as? String == "reviewed")
        #expect(requests[5].body?["reason"] as? String == "too risky")
        let cancellationID = try #require(requests[6].body?["cancellation_id"] as? String)
        #expect(UUID(uuidString: cancellationID) == executionCancellationID)
        #expect(requests[6].body?.count == 1)
    }

    @MainActor
    @Test("Voice state model is explicit about text-only scaffold")
    func voiceStateModelTracksDegradedMode() {
        let model = VoiceStateModel()

        #expect(model.statusText.contains("Text-only voice scaffold"))
        model.apply(.beginTranscript)
        #expect(model.statusText.contains("Voice transcript staging"))
        model.apply(.updateTranscript(" status check "))
        let handoff = model.apply(.submitTranscript)
        #expect(handoff?.text == "status check")
        #expect(handoff?.source == "voice-transcript-scaffold")
        #expect(handoff?.dryRun == true)
        #expect(model.transcriptDraft.isEmpty)
        model.setUnavailable(reason: "Microphone permission is missing.")
        #expect(model.statusText.contains("Voice unavailable"))
        #expect(!model.isPushToTalkEnabled)
        model.apply(.updateTranscript("ignored"))
        #expect(model.transcriptDraft.isEmpty)
        #expect(model.lastError?.contains("unavailable") == true)
        model.resetTextOnly()
        #expect(model.statusText.contains("Voice capture is idle"))
    }

    @MainActor
    @Test("Voice transcript handoff uses the same console command submit path")
    func voiceTranscriptHandoffUsesTextCommandPath() async throws {
        let client = FakeCoreClient()
        let console = CommandConsoleModel(client: client)
        let voice = VoiceStateModel()

        voice.apply(.beginTranscript)
        voice.apply(.updateTranscript("  plugin echo hello  "))
        let handoff = try #require(voice.apply(.submitTranscript))
        await console.submit(input: handoff.text, dryRun: handoff.dryRun)

        #expect(client.submittedCommandsWithoutCancellationIDs == [
            JarvisCommandRequest(input: "plugin echo hello", dryRun: true)
        ])
        #expect(handoff.dryRun == true)
        #expect(voice.lastHandoff == handoff)
        #expect(console.transcript.map(\.text) == [
            "plugin echo hello",
            "local response: plugin echo hello"
        ])
        #expect(console.activity.contains { $0.title == "Task completed" })
    }

    @MainActor
    @Test("Command console can submit non-dry-run requests when explicitly requested")
    func commandConsoleSubmitsExplicitNonDryRunRequests() async {
        let client = FakeCoreClient()
        let console = CommandConsoleModel(client: client)

        await console.submit(input: "  status check  ", dryRun: false)

        #expect(client.submittedCommandsWithoutCancellationIDs == [
            JarvisCommandRequest(input: "status check", dryRun: false)
        ])
        #expect(console.transcript.map(\.text) == [
            "status check",
            "local response: status check"
        ])
    }

    @MainActor
    @Test("Command console exposes cancellation only while its generated handle is active")
    func commandConsoleCancelsItsActiveSubmission() async throws {
        let client = FakeCoreClient(commandSubmissionDelayNanoseconds: 150_000_000)
        let console = CommandConsoleModel(client: client)

        let submission = Task { @MainActor in
            await console.submit(input: "cancel this active command", dryRun: false)
        }
        try await Task.sleep(nanoseconds: 25_000_000)
        let activeID = try #require(console.activeCancellationID)
        #expect(console.isWorking)
        #expect(client.submittedCommands.first?.cancellationID == activeID)

        await console.cancelActiveCommand()
        #expect(client.commandCancellationRequests == [activeID])
        #expect(console.cancellationStatus == "cancellation_requested")

        await submission.value
        #expect(console.activeCancellationID == nil)
        #expect(!console.isWorking)
    }

    @MainActor
    @Test("Command console rejects overlapping submissions without orphaning the active handle")
    func commandConsoleSerializesConcurrentSubmissions() async throws {
        let client = FakeCoreClient(commandSubmissionDelayNanoseconds: 150_000_000)
        let console = CommandConsoleModel(client: client)

        let first = Task { @MainActor in
            await console.submit(input: "first active command", dryRun: false)
        }
        try await Task.sleep(nanoseconds: 25_000_000)
        let firstID = try #require(console.activeCancellationID)

        await console.submit(input: "keyboard overlap must fail", dryRun: false)

        #expect(console.activeCancellationID == firstID)
        #expect(console.isWorking)
        #expect(client.submittedCommands.count == 1)
        #expect(client.submittedCommands.first?.input == "first active command")
        #expect(!console.transcript.contains { $0.text == "keyboard overlap must fail" })
        #expect(console.lastError?.contains("already active") == true)

        await first.value
        #expect(console.activeCancellationID == nil)
        #expect(!console.isWorking)
    }

    @MainActor
    @Test("Command console memory context remains off until the operator opts in")
    func commandConsoleMemoryContextIsExplicitOptIn() async {
        let client = FakeCoreClient()
        let console = CommandConsoleModel(client: client)

        #expect(!console.memoryContextEnabled)
        await console.submit(input: "status without memory")
        console.memoryContextEnabled = true
        await console.submit(input: "status with memory")

        #expect(client.submittedCommandsWithoutCancellationIDs == [
            JarvisCommandRequest(input: "status without memory", memoryContext: false),
            JarvisCommandRequest(input: "status with memory", memoryContext: true)
        ])
    }

    @MainActor
    @Test("Command console installed WASM tools remain off until the operator opts in")
    func commandConsoleInstalledWasmToolsAreExplicitOptIn() async {
        let client = FakeCoreClient()
        let console = CommandConsoleModel(client: client)

        #expect(!console.installedWasmToolsEnabled)
        #expect(!console.toolExecutionEnabled)
        await console.submit(input: "status without installed tools")
        console.installedWasmToolsEnabled = true
        await console.submit(
            input: "plan with installed tools",
            dryRun: !console.toolExecutionEnabled
        )
        console.toolExecutionEnabled = true
        await console.submit(
            input: "execute with installed tools",
            dryRun: !console.toolExecutionEnabled
        )

        #expect(client.submittedCommandsWithoutCancellationIDs == [
            JarvisCommandRequest(
                input: "status without installed tools",
                installedWasmTools: false
            ),
            JarvisCommandRequest(
                input: "plan with installed tools",
                installedWasmTools: true
            ),
            JarvisCommandRequest(
                input: "execute with installed tools",
                dryRun: false,
                installedWasmTools: true
            )
        ])
    }

    @MainActor
    @Test("A policy-blocked command does not imply emergency pause")
    func blockedCommandDoesNotSetEmergencyPause() async {
        let client = FakeCoreClient(commandStatus: "blocked")
        let console = CommandConsoleModel(client: client)

        await console.submit(input: "blocked installed tool request")

        #expect(!console.isPaused)
        #expect(console.transcript.last?.text == "local response: blocked installed tool request")
    }

    @MainActor
    @Test("Voice transcript handoff rejects empty transcript")
    func voiceTranscriptHandoffRejectsEmptyTranscript() {
        let model = VoiceStateModel()

        model.apply(.beginTranscript)
        model.apply(.updateTranscript(" \n\t "))
        let handoff = model.apply(.submitTranscript)

        #expect(handoff == nil)
        #expect(model.lastError == "Transcript is empty.")
        #expect(model.lastHandoff == nil)
    }

    @MainActor
    @Test("Voice transcript interruption blocks submit until resume or cancel")
    func voiceTranscriptInterruptionBlocksSubmitUntilResumeOrCancel() {
        let model = VoiceStateModel()

        model.apply(.beginTranscript)
        model.apply(.updateTranscript("open diagnostics"))
        model.interruptTranscript(reason: "User started another request.")

        #expect(model.statusText.contains("interrupted"))
        #expect(model.transcriptDraft == "open diagnostics")
        #expect(!model.isPushToTalkEnabled)
        #expect(model.apply(.submitTranscript) == nil)
        #expect(model.lastError == "Voice transcript is interrupted; resume or cancel before submitting.")

        model.apply(.resumeInterruptedTranscript)
        let handoff = model.apply(.submitTranscript)

        #expect(handoff?.text == "open diagnostics")
        #expect(model.lastError == nil)

        model.apply(.beginTranscript)
        model.apply(.updateTranscript("cancel me"))
        model.interruptTranscript(reason: "User cancelled.")
        model.apply(.cancelTranscript)

        #expect(model.transcriptDraft.isEmpty)
        #expect(model.lastHandoff == nil)
        #expect(model.statusText.contains("Text-only voice scaffold"))
    }

    @MainActor
    @Test("Voice degraded mode keeps typed transcript fallback without speech claims")
    func voiceDegradedModeKeepsTypedTranscriptFallbackWithoutSpeechClaims() {
        let model = VoiceStateModel()

        model.markDegraded(reason: "Text-to-speech playback is unavailable.")
        #expect(model.statusText == "Voice degraded to typed transcript fallback: Text-to-speech playback is unavailable.")
        #expect(model.transcriptDraft.isEmpty)
        #expect(model.lastHandoff == nil)
        #expect(!model.isPushToTalkEnabled)

        model.apply(.updateTranscript("status check"))
        #expect(model.statusText.contains("Voice transcript staging"))
        let handoff = model.apply(.submitTranscript)

        #expect(handoff?.text == "status check")
        #expect(handoff?.source == "voice-transcript-scaffold")
        #expect(handoff?.dryRun == true)
        #expect(model.statusText.contains("Text-only voice scaffold"))
        #expect(!model.statusText.contains("speech recognition coverage"))
    }

    @MainActor
    @Test("Voice unavailable mode rejects typed transcript handoff until reset")
    func voiceUnavailableModeRejectsTypedTranscriptHandoffUntilReset() {
        let model = VoiceStateModel()

        model.setUnavailable(reason: "Microphone permission is missing.")
        model.apply(.beginTranscript)
        #expect(model.lastError == "Voice input is unavailable; reset to text-only before staging a transcript.")
        model.apply(.updateTranscript("ignored"))
        #expect(model.transcriptDraft.isEmpty)
        #expect(model.apply(.submitTranscript) == nil)
        #expect(model.lastError == "Voice input is unavailable; transcript cannot be submitted.")

        model.resetTextOnly()
        model.apply(.updateTranscript("typed fallback after reset"))
        let handoff = model.apply(.submitTranscript)

        #expect(handoff?.text == "typed fallback after reset")
    }

    @MainActor
    @Test("Voice adapter model starts capture and stages deterministic transcripts")
    func voiceAdapterModelStagesPartialAndFinalTranscripts() async {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        let model = VoiceAdapterStateModel(adapter: adapter, voiceState: voice)

        #expect(model.statusText == "Voice adapter idle.")
        #expect(model.permissionState == .notRequested)
        #expect(model.permissionStatusText == "Voice permissions not requested.")
        #expect(!model.canStartCapture)
        #expect(!model.isCaptureActive)

        await model.requestPermissions()
        #expect(model.phase == .idle)
        #expect(model.permissionState == .granted)
        #expect(model.permissionStatusText == "Voice permissions granted.")
        #expect(model.lastError == nil)
        #expect(model.canStartCapture)

        await model.startCapture()
        #expect(model.phase == .listening)
        #expect(model.statusText == "Voice adapter listening.")
        #expect(!model.canStartCapture)
        #expect(model.isCaptureActive)
        #expect(voice.statusText.contains("Voice transcript staging"))

        adapter.emitPartial("open")
        #expect(model.phase == .transcribing)
        #expect(model.statusText == "Voice adapter transcribing.")
        #expect(model.isCaptureActive)
        #expect(voice.transcriptDraft == "open")
        adapter.emitFinal("open diagnostics")
        #expect(model.phase == .idle)
        #expect(model.canStartCapture)
        #expect(!model.isCaptureActive)
        #expect(voice.transcriptDraft == "open diagnostics")
        #expect(!model.isFinalTranscriptAutoSubmitEnabled)

        let handoff = voice.apply(.submitTranscript)
        #expect(handoff?.text == "open diagnostics")
        #expect(handoff?.source == "voice-transcript-scaffold")
    }

    @MainActor
    @Test("Voice adapter auto-submits final transcript only when explicitly enabled")
    func voiceAdapterModelAutoSubmitsFinalTranscriptWhenEnabled() async throws {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        var submittedHandoffs: [JarvisVoiceCommandHandoff] = []
        let model = VoiceAdapterStateModel(
            adapter: adapter,
            voiceState: voice,
            submitFinalTranscript: { handoff in
                submittedHandoffs.append(handoff)
            }
        )

        await model.requestPermissions()
        model.setFinalTranscriptAutoSubmitEnabled(true)
        await model.startCapture()
        adapter.emitFinal("  open diagnostics  ")
        try await Task.sleep(nanoseconds: 50_000_000)

        #expect(model.phase == .idle)
        #expect(submittedHandoffs.map(\.text) == ["open diagnostics"])
        #expect(submittedHandoffs.map(\.source) == ["voice-final-transcript"])
        #expect(submittedHandoffs.map(\.dryRun) == [true])
        #expect(voice.transcriptDraft.isEmpty)
        #expect(voice.lastHandoff == submittedHandoffs.first)
    }

    @MainActor
    @Test("Voice adapter auto-submit requires an explicit submit handler")
    func voiceAdapterAutoSubmitRequiresSubmitHandler() async {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        let model = VoiceAdapterStateModel(adapter: adapter, voiceState: voice)

        #expect(!model.isFinalTranscriptAutoSubmitToggleEnabled)
        #expect(model.autoSubmitAvailability.blockedReason == "Auto-submit is unavailable because no command submit handler is configured.")

        model.setFinalTranscriptAutoSubmitEnabled(true)

        #expect(!model.isFinalTranscriptAutoSubmitEnabled)
    }

    @MainActor
    @Test("Voice adapter auto-submit reports busy submitter as unavailable")
    func voiceAdapterAutoSubmitReportsBusySubmitter() async throws {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        var submittedHandoffs: [JarvisVoiceCommandHandoff] = []
        let isSubmitterBusy = MutableFlag(true)
        let model = VoiceAdapterStateModel(
            adapter: adapter,
            voiceState: voice,
            shouldAutoSubmitFinalTranscript: { !isSubmitterBusy.value },
            autoSubmitUnavailableReason: {
                isSubmitterBusy.value ? "Auto-submit is unavailable while a command is already running." : nil
            },
            submitFinalTranscript: { handoff in
                submittedHandoffs.append(handoff)
            }
        )

        #expect(!model.isFinalTranscriptAutoSubmitToggleEnabled)
        #expect(model.autoSubmitAvailability.blockedReason == "Auto-submit is unavailable until microphone and speech permissions are granted.")

        await model.requestPermissions()
        model.setFinalTranscriptAutoSubmitEnabled(true)
        #expect(!model.isFinalTranscriptAutoSubmitEnabled)

        isSubmitterBusy.value = false
        #expect(model.isFinalTranscriptAutoSubmitToggleEnabled)
        model.setFinalTranscriptAutoSubmitEnabled(true)
        await model.startCapture()
        isSubmitterBusy.value = true
        adapter.emitFinal("open diagnostics")
        try await Task.sleep(nanoseconds: 50_000_000)

        #expect(model.phase == .idle)
        #expect(voice.transcriptDraft == "open diagnostics")
        #expect(voice.lastHandoff == nil)
        #expect(submittedHandoffs.isEmpty)
    }

    @MainActor
    @Test("Voice adapter auto-submit uses the console command path")
    func voiceAdapterAutoSubmitFinalTranscriptUsesConsoleCommandPath() async throws {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        let client = FakeCoreClient()
        let console = CommandConsoleModel(client: client)
        let model = VoiceAdapterStateModel(
            adapter: adapter,
            voiceState: voice,
            submitFinalTranscript: { handoff in
                await console.submit(input: handoff.text, dryRun: handoff.dryRun)
            }
        )

        await model.requestPermissions()
        model.setFinalTranscriptAutoSubmitEnabled(true)
        await model.startCapture()
        adapter.emitFinal("  status check  ")
        try await Task.sleep(nanoseconds: 50_000_000)

        #expect(client.submittedCommandsWithoutCancellationIDs == [
            JarvisCommandRequest(input: "status check", dryRun: true)
        ])
        #expect(console.transcript.map(\.text) == [
            "status check",
            "local response: status check"
        ])
        #expect(voice.lastHandoff?.source == "voice-final-transcript")
        #expect(voice.transcriptDraft.isEmpty)
    }

    @MainActor
    @Test("Voice adapter auto-submit encodes the real IPC command request")
    func voiceAdapterAutoSubmitEncodesRealIPCCommandRequest() async throws {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [IPCURLProtocol.self]
        let session = URLSession(configuration: configuration)
        let client = JarvisIPCClient(
            endpoint: JarvisEndpoint(baseURL: URL(string: "http://127.0.0.1:7787")!),
            session: session
        )
        let console = CommandConsoleModel(client: client)
        var requests: [(method: String, path: String, body: [String: Any]?)] = []

        IPCURLProtocol.handler = { request in
            requests.append((
                method: request.httpMethod ?? "",
                path: request.url?.path(percentEncoded: false) ?? "",
                body: decodeRequestBody(request)
            ))
            let response = HTTPURLResponse(
                url: request.url!,
                statusCode: 200,
                httpVersion: nil,
                headerFields: ["Content-Type": "application/json"]
            )!
            return (response, commandResponseJSON(input: "status check"))
        }
        defer { IPCURLProtocol.handler = nil }

        let model = VoiceAdapterStateModel(
            adapter: adapter,
            voiceState: voice,
            submitFinalTranscript: { handoff in
                await console.submit(input: handoff.text, dryRun: handoff.dryRun)
            }
        )

        await model.requestPermissions()
        model.setFinalTranscriptAutoSubmitEnabled(true)
        await model.startCapture()
        adapter.emitFinal("  status check  ")
        try await Task.sleep(nanoseconds: 50_000_000)

        #expect(requests.map(\.method) == ["POST"])
        #expect(requests.map(\.path) == ["/commands"])
        #expect(requests.first?.body?["input"] as? String == "status check")
        #expect(requests.first?.body?["dry_run"] as? Bool == true)
        #expect(console.transcript.map(\.text) == [
            "status check",
            "local response: status check"
        ])
        #expect(console.activity.contains { $0.title == "Task completed" })
        #expect(voice.lastHandoff?.source == "voice-final-transcript")
        #expect(voice.transcriptDraft.isEmpty)
    }

    @MainActor
    @Test("Voice adapter keeps final transcript staged when auto-submit is blocked")
    func voiceAdapterModelKeepsFinalTranscriptStagedWhenAutoSubmitBlocked() async throws {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        var submittedHandoffs: [JarvisVoiceCommandHandoff] = []
        let model = VoiceAdapterStateModel(
            adapter: adapter,
            voiceState: voice,
            shouldAutoSubmitFinalTranscript: { false },
            submitFinalTranscript: { handoff in
                submittedHandoffs.append(handoff)
            }
        )

        await model.requestPermissions()
        model.setFinalTranscriptAutoSubmitEnabled(true)
        await model.startCapture()
        adapter.emitFinal("open diagnostics")
        try await Task.sleep(nanoseconds: 50_000_000)

        #expect(model.phase == .idle)
        #expect(voice.transcriptDraft == "open diagnostics")
        #expect(voice.lastHandoff == nil)
        #expect(submittedHandoffs.isEmpty)
    }

    @MainActor
    @Test("Voice adapter model fails closed on permission errors")
    func voiceAdapterModelFailsClosedOnPermissionErrors() async {
        let adapter = FakeVoiceAdapter(
            permissionResult: .failure(.permissionDenied("Microphone permission was denied."))
        )
        let voice = VoiceStateModel()
        let model = VoiceAdapterStateModel(adapter: adapter, voiceState: voice)

        await model.requestPermissions()

        #expect(model.phase == .unavailable(reason: "Voice permission denied: Microphone permission was denied."))
        #expect(model.lastError == .permissionDenied("Microphone permission was denied."))
        #expect(voice.statusText.contains("Voice unavailable"))
        #expect(voice.statusText.contains("Microphone permission was denied"))
        #expect(voice.apply(.submitTranscript) == nil)
        #expect(voice.lastError == "Voice input is unavailable; transcript cannot be submitted.")
    }

    @MainActor
    @Test("Voice adapter model exposes capture start failures without silent fallback")
    func voiceAdapterModelExposesCaptureStartFailures() async {
        let adapter = FakeVoiceAdapter(
            startResult: .failure(.captureStartFailed("No input device selected."))
        )
        let voice = VoiceStateModel()
        let model = VoiceAdapterStateModel(adapter: adapter, voiceState: voice)

        await model.requestPermissions()
        await model.startCapture()

        #expect(model.phase == .unavailable(reason: "Voice capture failed to start: No input device selected."))
        #expect(model.lastError == .captureStartFailed("No input device selected."))
        #expect(voice.transcriptDraft.isEmpty)
        #expect(voice.statusText.contains("Voice unavailable"))
    }

    @MainActor
    @Test("Voice adapter blocks capture before permissions are granted")
    func voiceAdapterBlocksCaptureBeforePermissionsAreGranted() async {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        let model = VoiceAdapterStateModel(adapter: adapter, voiceState: voice)

        #expect(model.permissionState == .notRequested)
        #expect(!model.canStartCapture)

        await model.startCapture()

        #expect(model.phase == .idle)
        #expect(model.permissionState == .notRequested)
        #expect(model.lastError == .permissionNotRequested("Request microphone and speech permissions before starting capture."))
        #expect(adapter.callbacks == nil)
        #expect(voice.statusText.contains("Text-only voice scaffold"))
    }

    #if canImport(AVFoundation) && canImport(Speech)
    @MainActor
    @available(macOS 14.0, *)
    @Test("Mac speech adapter preflights current permissions before capture")
    func macSpeechAdapterPreflightsCurrentPermissionsBeforeCapture() async {
        let deniedSpeech = MacSpeechVoiceAdapter(
            currentSpeechAuthorization: { .denied },
            currentMicrophoneAuthorization: { .authorized }
        )
        let restrictedSpeech = MacSpeechVoiceAdapter(
            currentSpeechAuthorization: { .restricted },
            currentMicrophoneAuthorization: { .authorized }
        )
        let pendingSpeech = MacSpeechVoiceAdapter(
            currentSpeechAuthorization: { .notDetermined },
            currentMicrophoneAuthorization: { .authorized }
        )
        let deniedMicrophone = MacSpeechVoiceAdapter(
            currentSpeechAuthorization: { .authorized },
            currentMicrophoneAuthorization: { .denied }
        )
        let restrictedMicrophone = MacSpeechVoiceAdapter(
            currentSpeechAuthorization: { .authorized },
            currentMicrophoneAuthorization: { .restricted }
        )
        let pendingMicrophone = MacSpeechVoiceAdapter(
            currentSpeechAuthorization: { .authorized },
            currentMicrophoneAuthorization: { .notDetermined }
        )

        let cases: [(adapter: MacSpeechVoiceAdapter, expected: JarvisVoiceAdapterError)] = [
            (deniedSpeech, .permissionDenied("Speech recognition permission was denied.")),
            (restrictedSpeech, .permissionRestricted("Speech recognition is restricted on this Mac.")),
            (pendingSpeech, .permissionNotRequested("Speech recognition permission has not been requested.")),
            (deniedMicrophone, .permissionDenied("Microphone permission was denied.")),
            (restrictedMicrophone, .permissionRestricted("Microphone access is restricted on this Mac.")),
            (pendingMicrophone, .permissionNotRequested("Microphone permission has not been requested.")),
        ]

        for testCase in cases {
            let result = await testCase.adapter.startCapture(callbacks: noopVoiceCaptureCallbacks())

            if case let .failure(error) = result {
                #expect(error == testCase.expected)
            } else {
                Issue.record("Expected startCapture to fail before installing the audio tap")
            }
            #expect(testCase.adapter.phase == .unavailable(reason: testCase.expected.description))
        }
    }
    #endif

    @MainActor
    @Test("Voice adapter model preserves interruption as an explicit state")
    func voiceAdapterModelInterruptsActiveCaptureExplicitly() async {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        let model = VoiceAdapterStateModel(adapter: adapter, voiceState: voice)

        await model.requestPermissions()
        await model.startCapture()
        adapter.emitPartial("status check")
        await model.interrupt(reason: "User stopped push-to-talk.")

        #expect(model.phase == .interrupted(reason: "User stopped push-to-talk."))
        #expect(model.lastError == nil)
        #expect(voice.statusText.contains("interrupted"))
        #expect(voice.transcriptDraft == "status check")
        #expect(voice.apply(.submitTranscript) == nil)
        #expect(voice.lastError == "Voice transcript is interrupted; resume or cancel before submitting.")
    }

    @MainActor
    @Test("Voice adapter ignores late final transcript after interruption")
    func voiceAdapterIgnoresLateFinalTranscriptAfterInterruption() async throws {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        var submittedHandoffs: [JarvisVoiceCommandHandoff] = []
        let model = VoiceAdapterStateModel(
            adapter: adapter,
            voiceState: voice,
            submitFinalTranscript: { handoff in
                submittedHandoffs.append(handoff)
            }
        )

        await model.requestPermissions()
        model.setFinalTranscriptAutoSubmitEnabled(true)
        await model.startCapture()
        adapter.emitPartial("status")
        let callbacks = try #require(adapter.callbacks)

        await model.interrupt(reason: "User stopped push-to-talk.")
        callbacks.onFinalTranscript("status check")
        try await Task.sleep(nanoseconds: 50_000_000)

        #expect(model.phase == .interrupted(reason: "User stopped push-to-talk."))
        #expect(voice.transcriptDraft == "status")
        #expect(voice.lastHandoff == nil)
        #expect(submittedHandoffs.isEmpty)
    }

    @MainActor
    @Test("Voice adapter callback errors mark voice unavailable")
    func voiceAdapterCallbackErrorsMarkVoiceUnavailable() async {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        let model = VoiceAdapterStateModel(adapter: adapter, voiceState: voice)

        await model.requestPermissions()
        await model.startCapture()
        adapter.emitError(.recognitionFailed("Recognition task cancelled."))

        #expect(model.phase == .unavailable(reason: "Speech recognition failed: Recognition task cancelled."))
        #expect(model.lastError == .recognitionFailed("Recognition task cancelled."))
        #expect(voice.statusText.contains("Voice unavailable"))
        #expect(voice.statusText.contains("Recognition task cancelled"))
    }

    @MainActor
    @Test("Voice adapter disables auto-submit when capture becomes unavailable")
    func voiceAdapterDisablesAutoSubmitWhenUnavailable() async {
        let adapter = FakeVoiceAdapter(
            permissionResult: .failure(.permissionDenied("Microphone permission was denied."))
        )
        let voice = VoiceStateModel()
        let model = VoiceAdapterStateModel(
            adapter: adapter,
            voiceState: voice,
            submitFinalTranscript: { _ in }
        )

        model.setFinalTranscriptAutoSubmitEnabled(true)
        #expect(!model.isFinalTranscriptAutoSubmitEnabled)
        #expect(model.autoSubmitAvailability.blockedReason == "Auto-submit is unavailable until microphone and speech permissions are granted.")

        await model.requestPermissions()

        #expect(!model.isFinalTranscriptAutoSubmitEnabled)
        #expect(!model.isFinalTranscriptAutoSubmitToggleEnabled)
        #expect(model.autoSubmitAvailability.blockedReason == "Auto-submit is unavailable while voice capture is unavailable: Voice permission denied: Microphone permission was denied.")
    }

    @MainActor
    @Test("Voice adapter command-state errors are recoverable")
    func voiceAdapterCommandStateErrorsAreRecoverable() async {
        let activeAdapter = FakeVoiceAdapter(
            phase: .listening,
            startResult: .failure(.alreadyCapturing)
        )
        let activeVoice = VoiceStateModel()
        let activeModel = VoiceAdapterStateModel(adapter: activeAdapter, voiceState: activeVoice)

        await activeModel.startCapture()

        #expect(activeModel.phase == .listening)
        #expect(activeModel.lastError == .alreadyCapturing)
        #expect(activeVoice.statusText.contains("Text-only voice scaffold"))

        let idleAdapter = FakeVoiceAdapter(stopResult: .failure(.noActiveCapture))
        let idleVoice = VoiceStateModel()
        let idleModel = VoiceAdapterStateModel(adapter: idleAdapter, voiceState: idleVoice)

        await idleModel.stopCapture()

        #expect(idleModel.phase == .idle)
        #expect(idleModel.lastError == .noActiveCapture)
        #expect(idleModel.permissionState == .notRequested)
        #expect(!idleModel.canStartCapture)
        #expect(idleVoice.statusText.contains("Text-only voice scaffold"))
    }

    @MainActor
    @Test("Speech output model speaks trimmed preview text")
    func speechOutputModelSpeaksTrimmedPreviewText() async {
        let adapter = FakeSpeechOutputAdapter()
        let model = SpeechOutputStateModel(adapter: adapter)

        await model.speak("  Jarvis is ready.  ")

        #expect(adapter.spokenTexts == ["Jarvis is ready."])
        #expect(model.lastSpokenText == "Jarvis is ready.")
        #expect(model.phase == .speaking)
        #expect(model.isSpeaking)
        #expect(!model.canSpeak)
        #expect(model.statusText == "Speech output speaking.")
    }

    @MainActor
    @Test("Speech output model observes natural adapter completion")
    func speechOutputModelObservesNaturalAdapterCompletion() async {
        let adapter = FakeSpeechOutputAdapter()
        let model = SpeechOutputStateModel(adapter: adapter)

        await model.speak("Jarvis is ready.")
        adapter.finishSpeech()

        #expect(model.lastSpokenText == "Jarvis is ready.")
        #expect(model.phase == .idle)
        #expect(!model.isSpeaking)
        #expect(model.canSpeak)
        #expect(model.statusText == "Speech output idle.")
    }

    @MainActor
    @Test("Speech output model rejects empty utterances before adapter playback")
    func speechOutputModelRejectsEmptyUtterances() async {
        let adapter = FakeSpeechOutputAdapter()
        let model = SpeechOutputStateModel(adapter: adapter)

        await model.speak("   ")

        #expect(adapter.spokenTexts.isEmpty)
        #expect(model.phase == .idle)
        #expect(model.lastError == .emptyUtterance)
        #expect(model.canSpeak)
        #expect(model.statusText == "Speech output idle.")
    }

    @MainActor
    @Test("Speech output model keeps command-state errors recoverable")
    func speechOutputModelKeepsCommandStateErrorsRecoverable() async {
        let adapter = FakeSpeechOutputAdapter(stopResult: .failure(.noActiveSpeech))
        let model = SpeechOutputStateModel(adapter: adapter)

        await model.stop()

        #expect(model.phase == .idle)
        #expect(model.lastError == .noActiveSpeech)
        #expect(model.canSpeak)
    }

    @MainActor
    @Test("Speech output model exposes playback failures")
    func speechOutputModelExposesPlaybackFailures() async {
        let adapter = FakeSpeechOutputAdapter(
            speakResult: .failure(.playbackUnavailable("No output device was available."))
        )
        let model = SpeechOutputStateModel(adapter: adapter)

        await model.speak("read this aloud")

        #expect(adapter.spokenTexts.isEmpty)
        #expect(model.lastError == .playbackUnavailable("No output device was available."))
        #expect(model.phase == .unavailable(reason: JarvisSpeechOutputError.playbackUnavailable("No output device was available.").description))
    }

    @MainActor
    @Test("Speech output model stops and interrupts active playback")
    func speechOutputModelStopsAndInterruptsActivePlayback() async {
        let adapter = FakeSpeechOutputAdapter()
        let model = SpeechOutputStateModel(adapter: adapter)

        await model.speak("status update")
        await model.stop()

        #expect(model.phase == .idle)
        #expect(!model.isSpeaking)
        #expect(model.canSpeak)

        await model.speak("status update")
        await model.interrupt(reason: "User interrupted speech output.")

        #expect(model.phase == .interrupted(reason: "User interrupted speech output."))
        #expect(!model.isSpeaking)
        #expect(model.canSpeak)
        #expect(model.statusText.contains("interrupted"))
    }

    #if canImport(AVFoundation)
    @MainActor
    @Test("Speech output adapter trims text and delegates synthesized utterance")
    func speechOutputAdapterTrimsTextAndDelegatesSynthesizedUtterance() async {
        let synthesizer = CapturingSpeechSynthesizer()
        let adapter = MacSpeechOutputAdapter(synthesizer: synthesizer)
        var phases: [JarvisSpeechOutputPhase] = []
        adapter.onPhaseChange = { phase in
            phases.append(phase)
        }

        let result = await adapter.speak("  Jarvis status ready.  ")

        #expect(speechOutputSucceeded(result))
        #expect(synthesizer.delegate === adapter)
        #expect(synthesizer.spokenUtterances.map(\.speechString) == ["Jarvis status ready."])
        #expect(synthesizer.stopBoundaries.isEmpty)
        #expect(adapter.phase == .speaking)
        #expect(phases == [.speaking])
    }

    @MainActor
    @Test("Speech output adapter stops existing playback before replacement utterance")
    func speechOutputAdapterStopsExistingPlaybackBeforeReplacementUtterance() async {
        let synthesizer = CapturingSpeechSynthesizer(isSpeaking: true)
        let adapter = MacSpeechOutputAdapter(synthesizer: synthesizer)

        let result = await adapter.speak("replacement status")

        #expect(speechOutputSucceeded(result))
        #expect(synthesizer.stopBoundaries == [.immediate])
        #expect(synthesizer.spokenUtterances.map(\.speechString) == ["replacement status"])
        #expect(adapter.phase == .speaking)
    }

    @MainActor
    @Test("Speech output adapter stop and interrupt use distinct synthesizer boundaries")
    func speechOutputAdapterStopAndInterruptUseDistinctSynthesizerBoundaries() async {
        let synthesizer = CapturingSpeechSynthesizer()
        let adapter = MacSpeechOutputAdapter(synthesizer: synthesizer)

        _ = await adapter.speak("first status")
        let stopResult = await adapter.stop()
        _ = await adapter.speak("second status")
        let interruptResult = await adapter.interrupt(reason: "Operator interrupted speech output.")

        #expect(speechOutputSucceeded(stopResult))
        #expect(speechOutputSucceeded(interruptResult))
        #expect(synthesizer.stopBoundaries == [.word, .immediate])
        #expect(synthesizer.spokenUtterances.map(\.speechString) == ["first status", "second status"])
        #expect(adapter.phase == .interrupted(reason: "Operator interrupted speech output."))
    }

    @MainActor
    @Test("Speech output adapter rejects empty utterances before synthesizer playback")
    func speechOutputAdapterRejectsEmptyUtterancesBeforeSynthesizerPlayback() async {
        let synthesizer = CapturingSpeechSynthesizer()
        let adapter = MacSpeechOutputAdapter(synthesizer: synthesizer)

        let result = await adapter.speak("   ")

        #expect(speechOutputFailed(result, with: .emptyUtterance))
        #expect(synthesizer.spokenUtterances.isEmpty)
        #expect(synthesizer.stopBoundaries.isEmpty)
        #expect(adapter.phase == .idle)
    }

    @MainActor
    @Test("Speech output adapter ignores stale utterance completion")
    func speechOutputAdapterIgnoresStaleUtteranceCompletion() async {
        let adapter = MacSpeechOutputAdapter()
        let staleUtterance = AVSpeechUtterance(string: "old status")
        let activeUtterance = AVSpeechUtterance(string: "new status")

        adapter.beginSpeechTracking(for: activeUtterance)
        adapter.markSpeechCompleted(for: staleUtterance)

        #expect(adapter.phase == .speaking)
        adapter.markSpeechCompleted(for: activeUtterance)
        #expect(adapter.phase == .idle)
    }
    #endif

    @MainActor
    @Test("Memory manager model supports create, update, review, and soft delete")
    func memoryManagerModelSupportsCrudAndIncludeDeleted() async {
        let active = sampleMemoryItem(category: "workflow", key: "release-gate")
        let deleted = sampleMemoryItem(category: "archive", key: "old-note", deletedAt: "2026-05-20T12:05:00Z")
        let client = FakeCoreClient(memoryItems: [active, deleted])
        let model = MemoryManagerModel(client: client)

        await model.refresh()
        #expect(model.items.map(\.id) == [active.id])
        #expect(client.includeDeletedMemoryRequests == [false])
        #expect(model.classification?.activeCount == 1)
        #expect(model.classification?.sensitiveActiveCount == 1)
        #expect(model.retentionPlan?.candidateCount == 1)
        #expect(model.retentionPlan?.automationEnabled == false)
        #expect(model.retentionPlan?.candidates.first?.recommendedAction == "operator_purge_or_restore")
        #expect(model.indexStatus?.state == "stale")
        #expect(model.indexStatus?.retrievalEnabled == false)
        #expect(model.indexStatus?.staleEntryCount == 1)

        await model.rebuildIndex()
        #expect(model.indexStatus?.state == "current")
        #expect(model.indexStatus?.currentEntryCount == 1)

        await model.refresh(includeDeleted: true)
        #expect(model.includeDeleted)
        #expect(model.items.map(\.id) == [active.id, deleted.id])
        #expect(client.includeDeletedMemoryRequests == [false, true])

        await model.create(
            category: "release",
            key: "gate",
            value: "Run local release verification before opening a PR.",
            provenance: "manual",
            sensitivity: "workspace"
        )
        let created = try! #require(model.selectedItem)
        #expect(created.category == "release")
        #expect(client.createdMemoryRequests.last?.key == "gate")

        await model.update(
            id: created.id,
            value: "Run release verification, then open the PR.",
            provenance: "operator correction",
            sensitivity: "private"
        )
        #expect(client.updatedMemoryRequests.last?.id == created.id)
        #expect(client.updatedMemoryRequests.last?.request.value == "Run release verification, then open the PR.")
        #expect(model.selectedItem?.sensitivity == "private")

        await model.review(id: created.id)
        #expect(model.selectedItem?.reviewedAt != nil)

        await model.refresh()
        await model.delete(id: created.id)
        #expect(!model.items.contains { $0.id == created.id })
        #expect(model.selectedItem == nil)

        await model.refresh(includeDeleted: true)
        await model.restore(id: created.id)
        #expect(model.selectedItem?.id == created.id)
        #expect(model.selectedItem?.deletedAt == nil)
        #expect(model.items.contains { $0.id == created.id && $0.deletedAt == nil })
    }

    @MainActor
    @Test("Approval management model loads contract, tasks, and audit evidence")
    func approvalManagementModelLoadsQueue() async {
        let task = JarvisTask(
            id: UUID(),
            sessionId: UUID(),
            userInput: "private tool",
            status: "waiting_for_approval",
            createdAt: nil,
            updatedAt: nil
        )
        let client = FakeCoreClient(
            tasks: [task],
            auditEntries: [
                JarvisAuditEntry(
                    id: UUID(),
                    taskId: task.id,
                    eventType: "plugin_approval_required",
                    summary: "approval required",
                    payload: .object(["approval_status": .string("pending")]),
                    createdAt: "2026-05-20T12:00:00Z"
                )
            ]
        )
        let model = ApprovalManagementModel(client: client)

        await model.refresh()

        #expect(model.pendingItems.count == 2)
        #expect(!model.supportsApprovalActions)
        #expect(model.limitationText?.contains("no approval decision endpoint") == true)
        #expect(model.permissionSurface.status == .inspectionOnly)
        #expect(model.permissionSurface.pendingApprovalCount == 2)
        #expect(model.permissionSurface.inspectionOnlyApprovalCount == 2)
    }

    @MainActor
    @Test("Approval management model approves and stages executable approval")
    func approvalManagementModelApprovesPendingApproval() async {
        let approval = samplePendingApproval()
        let client = FakeCoreClient(
            contractResponse: fullApprovalContract(),
            approvals: [approval],
            permissionGrantSummary: samplePermissionGrantSummary(approval: approval)
        )
        let model = ApprovalManagementModel(client: client)

        await model.refresh()
        await model.approve(id: approval.id, reason: "reviewed in app")

        #expect(model.supportsApprovalActions)
        #expect(model.supportsApprovalExecution)
        #expect(model.pendingItems.count == 1)
        #expect(model.pendingItems.first?.approvalStatus == "approved")
        #expect(model.pendingItems.first?.executionAvailable == true)
        #expect(model.lastDecision?.status == "approved")
        #expect(model.lastDecision?.decidedBy == "mac-ui")
        #expect(client.approvalDecisions == [
            FakeCoreClient.ApprovalDecision(id: approval.id, approved: true, reason: "reviewed in app")
        ])
        #expect(model.permissionSurface.status == .clear)
        #expect(model.permissionSurface.pendingApprovalCount == 0)
        #expect(model.permissionSurface.approvedGrantCount == 2)
        #expect(model.permissionSurface.sideEffectsRequireApproval)
        #expect(model.permissionSurface.unverifiedInstalledPluginGrantCount == 1)
        #expect(model.policyReview?.reviewItemCount == 4)
        #expect(model.policyReview?.status == "review_required")
    }

    @MainActor
    @Test("Approval management model executes approved approval and hides completed replay")
    func approvalManagementModelExecutesApprovedApproval() async {
        let approval = samplePendingApproval(status: "approved", decidedBy: "mac-ui", decisionReason: "reviewed")
        let client = FakeCoreClient(
            contractResponse: fullApprovalContract(),
            approvals: [approval],
            permissionGrantSummary: samplePermissionGrantSummary(approval: approval)
        )
        let model = ApprovalManagementModel(client: client)

        await model.refresh()

        #expect(model.pendingItems.count == 1)
        #expect(model.pendingItems.first?.executionAvailable == true)

        await model.execute(id: approval.id)

        #expect(client.approvalExecutions == [approval.id])
        #expect(model.pendingItems.isEmpty)
        #expect(model.lastExecution?.accepted == true)
        #expect(model.lastExecution?.auditEntry.eventType == "approval_executed")

        await model.refresh()

        #expect(model.pendingItems.isEmpty)
    }

    @MainActor
    @Test("Approval management model suppresses duplicate approved execution submits")
    func approvalManagementModelSuppressesDuplicateExecution() async {
        let approval = samplePendingApproval(status: "approved", decidedBy: "mac-ui", decisionReason: "reviewed")
        let client = FakeCoreClient(
            contractResponse: fullApprovalContract(),
            approvals: [approval],
            permissionGrantSummary: samplePermissionGrantSummary(approval: approval),
            approvalExecutionDelayNanoseconds: 50_000_000
        )
        let model = ApprovalManagementModel(client: client)
        await model.refresh()

        async let first: Void = model.execute(id: approval.id)
        async let second: Void = model.execute(id: approval.id)
        _ = await (first, second)

        #expect(client.approvalExecutions == [approval.id])
        #expect(model.pendingItems.isEmpty)
    }

    @MainActor
    @Test("Approval execution and cancellation share one handle and clear late state")
    func approvalManagementModelCancelsExecutionWithMatchingHandle() async throws {
        let approval = samplePendingApproval(
            status: "approved",
            decidedBy: "mac-ui",
            decisionReason: "reviewed"
        )
        let client = FakeCoreClient(
            contractResponse: fullApprovalContract(),
            approvals: [approval],
            permissionGrantSummary: samplePermissionGrantSummary(approval: approval),
            approvalExecutionDelayNanoseconds: 100_000_000,
            commandCancellationDelayNanoseconds: 50_000_000
        )
        let model = ApprovalManagementModel(client: client)
        await model.refresh()

        async let execution: Void = model.execute(id: approval.id)
        while client.approvalExecutionCancellationIDs.isEmpty {
            await Task.yield()
        }
        let activeCancellationID = try #require(model.executionCancellationID(for: approval.id))
        #expect(model.isExecuting(id: approval.id))
        #expect(client.approvalExecutionCancellationIDs == [activeCancellationID])

        async let duplicate: Void = model.execute(id: approval.id)
        async let cancellation: Void = model.cancelExecution(id: approval.id)
        while client.commandCancellationRequests.isEmpty {
            await Task.yield()
        }

        #expect(client.commandCancellationRequests == [activeCancellationID])
        #expect(model.isCancelling(id: approval.id))
        await cancellation
        #expect(model.isExecuting(id: approval.id))
        _ = await (execution, duplicate)

        #expect(client.approvalExecutions == [approval.id])
        #expect(client.approvalExecutionCancellationIDs == [activeCancellationID])
        #expect(!model.isExecuting(id: approval.id))
        #expect(!model.isCancelling(id: approval.id))
        #expect(model.executionCancellationID(for: approval.id) == nil)
        #expect(model.pendingItems.isEmpty)
    }

    @MainActor
    @Test("Approval management model approves and executes an installed action exactly once")
    func approvalManagementModelExecutesInstalledApprovalOnce() async {
        let approval = samplePendingApproval(action: "local_installed.confirm_action")
        let client = FakeCoreClient(
            contractResponse: fullApprovalContract(),
            approvals: [approval],
            permissionGrantSummary: samplePermissionGrantSummary(approval: approval),
            approvalExecutionDelayNanoseconds: 50_000_000
        )
        let model = ApprovalManagementModel(client: client)

        await model.refresh()
        await model.approve(id: approval.id, reason: "reviewed installed action")
        #expect(model.pendingItems.first?.executionAvailable == true)

        async let first: Void = model.execute(id: approval.id)
        async let duplicate: Void = model.execute(id: approval.id)
        _ = await (first, duplicate)

        #expect(client.approvalDecisions == [
            FakeCoreClient.ApprovalDecision(
                id: approval.id,
                approved: true,
                reason: "reviewed installed action"
            )
        ])
        #expect(client.approvalExecutions == [approval.id])
        #expect(model.pendingItems.isEmpty)
        #expect(model.lastExecution?.installedPluginResult?.pluginId == "local_installed")
        #expect(model.lastExecution?.installedPluginResult?.action == "confirm_action")
        #expect(model.lastExecution?.installedPluginResult?.sideEffectExecuted == true)
        #expect(model.lastExecution?.pluginResults.isEmpty == true)
        let decodedDescription = String(describing: model.lastExecution)
        #expect(!decodedDescription.contains("do-not-expose-bound-input"))
        #expect(!decodedDescription.contains("binding-digest-must-stay-core-only"))
    }

    @MainActor
    @Test("Approval management model hides durably claimed approval after restart")
    func approvalManagementModelHidesClaimedApproval() async {
        let approval = samplePendingApproval(status: "approved", decidedBy: "mac-ui", decisionReason: "reviewed")
        let client = FakeCoreClient(
            auditEntries: [
                JarvisAuditEntry(
                    id: UUID(),
                    taskId: approval.taskId,
                    eventType: "approval_execution_claimed",
                    summary: "approved action execution was atomically claimed",
                    payload: .object(["approval_id": .string(approval.id.uuidString)]),
                    createdAt: "2026-07-14T12:15:00Z"
                )
            ],
            contractResponse: fullApprovalContract(),
            approvals: [approval]
        )
        let model = ApprovalManagementModel(client: client)

        await model.refresh()

        #expect(model.pendingItems.isEmpty)
        #expect(client.approvalExecutions.isEmpty)
    }

    @MainActor
    @Test("Approval management model denies pending approval")
    func approvalManagementModelDeniesPendingApproval() async {
        let approval = samplePendingApproval()
        let client = FakeCoreClient(
            contractResponse: fullApprovalContract(),
            approvals: [approval]
        )
        let model = ApprovalManagementModel(client: client)

        await model.refresh()
        await model.deny(id: approval.id, reason: "not safe enough")

        #expect(model.pendingItems.isEmpty)
        #expect(model.lastDecision?.status == "denied")
        #expect(model.lastDecision?.decisionReason == "not safe enough")
        #expect(client.approvalDecisions == [
            FakeCoreClient.ApprovalDecision(id: approval.id, approved: false, reason: "not safe enough")
        ])
    }

    @MainActor
    @Test("Permission surface summarizes plugin scopes and risk tiers")
    func permissionSurfaceSummarizesPluginScopeState() async {
        let approval = samplePendingApproval()
        let client = FakeCoreClient(
            contractResponse: fullApprovalContract(),
            approvals: [approval],
            pluginManifests: [samplePluginManifest()],
            permissionGrantSummary: samplePermissionGrantSummary(approval: approval)
        )
        let model = ApprovalManagementModel(client: client)

        await model.refresh()

        #expect(model.permissionSurface.status == .reviewRequired)
        #expect(model.permissionSurface.approvalActionsAvailable)
        #expect(model.permissionSurface.pendingApprovalCount == 1)
        #expect(model.permissionSurface.actionableApprovalCount == 1)
        #expect(model.permissionSurface.declaredScopes == ["calendar_write", "conversation", "file_read"])
        #expect(model.permissionSurface.riskTierCounts == [
            JarvisPermissionRiskCount(riskTier: "allow", count: 1),
            JarvisPermissionRiskCount(riskTier: "confirm", count: 1)
        ])
        #expect(model.permissionSurface.proactiveActionCount == 1)
        #expect(model.permissionSurface.approvedGrantCount == 2)
        #expect(model.permissionSurface.installedPluginGrantCount == 1)
        #expect(model.permissionSurface.executableInstalledPluginGrantCount == 0)
        #expect(model.permissionSurface.unverifiedInstalledPluginGrantCount == 1)
        #expect(model.permissionSurface.summaryText.contains("need a decision"))
        #expect(model.grantSummary?.installedPluginGrants.first?.needsProvenanceReview == true)
        #expect(model.grantSummary?.installedPluginGrants.first?.integrityStatus == "not_verified")
    }

    @MainActor
    @Test("Plugin manager model loads first-party manifests and installed registry")
    func pluginManagerModelLoadsInstalledRegistry() async {
        let installed = try! JSONDecoder().decode(
            [JarvisInstalledPluginRecord].self,
            from: installedPluginsJSON()
        )
        let client = FakeCoreClient(
            pluginManifests: [samplePluginManifest()],
            modelToolCatalog: sampleModelToolCatalog(),
            installedPlugins: installed
        )
        let model = PluginManagerModel(client: client)

        await model.refresh()

        #expect(model.manifests.map(\.id) == ["calendar"])
        #expect(model.modelTools.map(\.id) == ["workspace_inspect.list"])
        #expect(model.modelToolCatalogWarning == nil)
        #expect(model.installedPlugins.map(\.id) == ["local_runner_test"])
        #expect(model.installedPlugins.first?.provenance.needsReview == true)
        #expect(model.installedPlugins.first?.isExecutable == false)
        #expect(model.installedPlugins.first?.resolvedRuntimeKind == "subprocess")
        #expect(model.installedPlugins.first?.confinementSummary == "local subprocess • not OS sandboxed")
    }

    @MainActor
    @Test("Plugin manager decodes redacted WASM confinement metadata")
    func pluginManagerModelDecodesWasmConfinement() async throws {
        let installed = try JSONDecoder().decode(
            [JarvisInstalledPluginRecord].self,
            from: installedWasmPluginJSON()
        )
        let client = FakeCoreClient(installedPlugins: installed)
        let model = PluginManagerModel(client: client)

        await model.refresh()

        let plugin = try #require(model.installedPlugins.first)
        #expect(plugin.manifest.source == "local_wasm")
        #expect(plugin.executionGrant == "wasm_compute")
        #expect(plugin.resolvedRuntimeKind == "wasm")
        #expect(plugin.wasmConfinementEnforced == true)
        #expect(plugin.osSandboxEnforced == false)
        #expect(plugin.hasEnforcedLanguageConfinement)
        #expect(plugin.confinementSummary == "WASM confined • no imports • no filesystem • no network")
        #expect(plugin.sourcePath == nil)
    }

    @MainActor
    @Test("Plugin manager treats model tool catalog failure as an independent degraded surface")
    func pluginManagerModelDegradesWhenModelToolCatalogFails() async {
        let client = FakeCoreClient(
            pluginManifests: [samplePluginManifest()],
            modelToolCatalogUnavailable: true
        )
        let model = PluginManagerModel(client: client)

        await model.refresh()

        #expect(model.manifests.map(\.id) == ["calendar"])
        #expect(model.modelTools.isEmpty)
        #expect(model.modelToolCatalogWarning?.contains("Production capability catalog unavailable") == true)
        #expect(model.lastError == nil)
    }

    @MainActor
    @Test("Plugin manager keeps first-party manifests when installed registry is unavailable")
    func pluginManagerModelKeepsManifestsWhenInstalledRegistryFails() async {
        let client = FakeCoreClient(
            pluginManifests: [samplePluginManifest()],
            installedPluginsUnavailable: true
        )
        let model = PluginManagerModel(client: client)

        await model.refresh()

        #expect(model.manifests.map(\.id) == ["calendar"])
        #expect(model.installedPlugins.isEmpty)
        #expect(model.installedRegistryWarning?.contains("Installed plugin registry unavailable") == true)
        #expect(model.lastError == nil)
    }

    @MainActor
    @Test("Run management model loads tasks and audit entries")
    func runManagementModelLoadsRuns() async {
        let task = JarvisTask(
            id: UUID(),
            sessionId: UUID(),
            userInput: "status check",
            status: "completed",
            createdAt: nil,
            updatedAt: nil
        )
        let audit = JarvisAuditEntry(
            id: UUID(),
            taskId: task.id,
            eventType: "task_completed",
            summary: "command completed",
            payload: nil,
            createdAt: "2026-05-20T12:00:00Z"
        )
        let model = RunManagementModel(client: FakeCoreClient(tasks: [task], auditEntries: [audit]))

        await model.refresh()

        #expect(model.tasks == [task])
        #expect(model.auditEntries == [audit])
        #expect(model.activitySummary?.taskCount == 1)
        #expect(model.activitySummary?.auditEntryCount == 1)
        #expect(model.activitySummary?.statusCounts == [
            JarvisActivityStatusCount(status: "completed", count: 1)
        ])
    }

    @MainActor
    @Test("Run management model watches bounded activity events")
    func runManagementModelWatchesActivityEvents() async {
        let taskId = UUID()
        let summary = try! JSONDecoder().decode(
            JarvisActivitySummary.self,
            from: activitySummaryJSON(taskId: taskId)
        )
        let event = JarvisActivityEvent(sequence: 0, event: "activity_summary", summary: summary, progress: nil, error: nil)
        let model = RunManagementModel(client: FakeCoreClient(activityEvents: [event]))

        await model.watchActivity()

        #expect(model.activityEvents == [event])
        #expect(model.activitySummary?.recentTasks.first?.id == taskId)
        #expect(model.lastError == nil)
    }

    @MainActor
    @Test("Run management model deduplicates redacted provider progress by audit id")
    func runManagementModelDeduplicatesProviderProgress() async throws {
        let taskId = UUID()
        let auditId = UUID()
        let payload = """
        {"audit_id":"\(auditId.uuidString)","task_id":"\(taskId.uuidString)","created_at":"2026-05-20T12:00:04Z","kind":"model_output","provider":"local","model":"ollama-test","sequence":0,"stage":"step_0","message":"model step 0 output chunk 0","byte_count":12,"char_count":10,"final_chunk":true,"content_redacted":true,"provider_native":true,"stderr_redacted":true}
        """
        let data = Data(
            """
            event: activity_progress
            data: \(payload)

            event: activity_progress
            data: \(payload)

            """.utf8
        )
        let events = try JarvisActivityEvent.parseServerSentEvents(data)
        let model = RunManagementModel(client: FakeCoreClient(activityEvents: events))

        await model.watchActivity()

        #expect(model.activityEvents.count == 1)
        #expect(model.activityEvents.first?.progress?.providerNative == true)
        #expect(model.activityEvents.first?.progress?.contentRedacted == true)
    }

    @Test("Supervisor configuration builds jarvis-cli serve arguments")
    func supervisorConfigurationBuildsLaunchArguments() {
        let databaseURL = URL(fileURLWithPath: "/tmp/jarvis-test.sqlite")
        let configuration = JarvisCoreSupervisorConfiguration(
            bindAddress: "127.0.0.1:8899",
            executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
            databaseURL: databaseURL
        )

        #expect(configuration.launchArguments == [
            "serve",
            "--bind",
            "127.0.0.1:8899",
            "--db-path",
            "/tmp/jarvis-test.sqlite"
        ])
    }

    @Test("Supervisor launch arguments opt in to bounded scheduler automation")
    func supervisorConfigurationBuildsSchedulerAutomationArguments() {
        let configuration = JarvisCoreSupervisorConfiguration(
            bindAddress: "127.0.0.1:8899",
            executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
            databaseURL: nil
        )
        let automation = JarvisSchedulerAutomationConfiguration(
            isEnabled: true,
            intervalMilliseconds: 5_000,
            runLimit: 4,
            recoverStaleOnStartup: true,
            staleAgeSeconds: 600,
            staleRecoveryLimit: 3
        )

        #expect(configuration.launchArguments(
            includeLoopbackBind: false,
            schedulerAutomation: automation
        ) == [
            "serve",
            "--scheduler-background",
            "--scheduler-interval-ms", "5000",
            "--scheduler-limit", "4",
            "--scheduler-recover-stale-on-startup",
            "--scheduler-stale-older-than-seconds", "600",
            "--scheduler-stale-recovery-limit", "3"
        ])
    }

    @Test("Supervisor configuration accepts packaged smoke endpoint environment")
    func supervisorConfigurationAcceptsPackagedSmokeEnvironment() {
        let configuration = JarvisCoreSupervisorConfiguration(
            executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
            databaseURL: nil,
            environment: [
                "JARVIS_MAC_CORE_BIND_ADDRESS": "127.0.0.1:18999",
                "JARVIS_MAC_CORE_ENDPOINT": "http://127.0.0.1:18999"
            ]
        )

        #expect(configuration.bindAddress == "127.0.0.1:18999")
        #expect(configuration.endpoint.baseURL.absoluteString == "http://127.0.0.1:18999")
        #expect(configuration.launchArguments == ["serve", "--bind", "127.0.0.1:18999"])
    }

    @Test("Credential provider injects missing provider secrets without overriding explicit environment")
    func credentialProviderInjectsMissingProviderSecrets() {
        let provider = JarvisCoreCredentialProvider(
            store: FakeCredentialStore(values: [.openAIAPIKey: "keychain-token"])
        )

        let injected = provider.launchEnvironment(base: ["JARVIS_CHATGPT_ENABLED": "true"])
        #expect(injected["JARVIS_OPENAI_API_KEY"] == "keychain-token")
        #expect(injected["JARVIS_CHATGPT_ENABLED"] == "true")

        let explicit = provider.launchEnvironment(base: ["JARVIS_OPENAI_API_KEY": "env-token"])
        #expect(explicit["JARVIS_OPENAI_API_KEY"] == "env-token")
    }

    @Test("Supervisor resolves configured executable before packaged candidates")
    func supervisorResolvesConfiguredExecutableFirst() {
        let configuredURL = JarvisCoreSupervisorConfiguration.configuredExecutableURL(
            environment: [
                "JARVIS_MAC_CORE_EXECUTABLE": "/opt/jarvis/bin/jarvis-cli",
                "JARVIS_CORE_EXECUTABLE": "/other/jarvis-core"
            ]
        )

        #expect(configuredURL?.path == "/opt/jarvis/bin/jarvis-cli")
    }

    @Test("Supervisor packaged discovery prefers Resources bin before loose resources")
    func supervisorPackagedDiscoveryPrefersResourcesBin() throws {
        let root = try temporaryDirectory(name: "jarvis-packaged-discovery")
        defer { try? FileManager.default.removeItem(at: root) }

        let resourcesBin = root
            .appending(path: "Contents", directoryHint: .isDirectory)
            .appending(path: "Resources", directoryHint: .isDirectory)
            .appending(path: "bin", directoryHint: .isDirectory)
        let resources = root
            .appending(path: "Contents", directoryHint: .isDirectory)
            .appending(path: "Resources", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: resourcesBin, withIntermediateDirectories: true)
        let binExecutable = resourcesBin.appending(path: "jarvis-cli")
        let looseExecutable = resources.appending(path: "jarvis-core")
        try writeExecutableStub(at: binExecutable)
        try writeExecutableStub(at: looseExecutable)

        let resolvedURL = JarvisCoreSupervisorConfiguration.firstExecutableURL(
            named: ["jarvis-cli", "jarvis-core"],
            in: [resourcesBin, resources]
        )

        #expect(resolvedURL?.path == binExecutable.path)
    }

    @Test("Supervisor packaged discovery ignores non executable files")
    func supervisorPackagedDiscoveryRequiresExecutableBit() throws {
        let root = try temporaryDirectory(name: "jarvis-non-executable-discovery")
        defer { try? FileManager.default.removeItem(at: root) }

        let resourcesBin = root.appending(path: "Resources/bin", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: resourcesBin, withIntermediateDirectories: true)
        let nonExecutable = resourcesBin.appending(path: "jarvis-cli")
        try Data("#!/bin/sh\nexit 0\n".utf8).write(to: nonExecutable)

        let resolvedURL = JarvisCoreSupervisorConfiguration.firstExecutableURL(
            named: ["jarvis-cli"],
            in: [resourcesBin]
        )

        #expect(resolvedURL == nil)
    }

    @Test("Supervisor packaged discovery accepts cargo binary name")
    func supervisorPackagedDiscoveryAcceptsCargoBinaryName() throws {
        let root = try temporaryDirectory(name: "jarvis-cargo-binary-discovery")
        defer { try? FileManager.default.removeItem(at: root) }

        let resourcesBin = root.appending(path: "Resources/bin", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: resourcesBin, withIntermediateDirectories: true)
        let cargoExecutable = resourcesBin.appending(path: "jarvis")
        try writeExecutableStub(at: cargoExecutable)

        let resolvedURL = JarvisCoreSupervisorConfiguration.firstExecutableURL(
            named: ["jarvis-cli", "jarvis", "jarvis-core"],
            in: [resourcesBin]
        )

        #expect(resolvedURL?.path == cargoExecutable.path)
    }

    @Test("Health decoding includes active local model metadata")
    func healthDecodingIncludesModelMetadata() throws {
        let health = try JSONDecoder().decode(
            JarvisHealth.self,
            from: Data(
                """
                {
                  "status": "ok",
                  "version": "0.1.4",
                  "contract": { "name": "jarvis.local-ipc", "version": 1, "core_version": "0.1.4" },
                  "started_at": "2026-06-25T17:00:00Z",
                  "emergency_paused": false,
                  "emergency_pause_reason": null,
                  "emergency_pause_updated_at": null,
                  "scheduler_jobs": 0,
                  "command_runtime": "routed-ollama-local-model+first-party-plugins",
                  "local_model_provider": "ollama",
                  "local_model": "qwen2.5:7b",
                  "local_endpoint_configured": true,
                  "chatgpt_enabled": true,
                  "chatgpt_auth_mode": "codex_account",
                  "chatgpt_model": "gpt-test",
                  "chatgpt_requires_approval": true
                }
                """.utf8
            )
        )

        #expect(health.localModelProvider == "ollama")
        #expect(health.localModel == "qwen2.5:7b")
        #expect(health.localEndpointConfigured)
        #expect(health.chatgptEnabled)
        #expect(health.chatgptAuthMode == "codex_account")
        #expect(health.chatgptModel == "gpt-test")
        #expect(health.chatgptRequiresApproval)
    }

    @Test("Model configuration maps selected Ollama model to launch environment")
    func modelConfigurationMapsOllamaSelectionToEnvironment() {
        let configuration = JarvisModelConfiguration(
            provider: .ollama,
            localModel: " qwen2.5:7b ",
            ollamaBaseURL: " http://127.0.0.1:11434/ ",
            timeoutMilliseconds: "45000"
        )

        let environment = configuration.launchEnvironmentOverrides

        #expect(environment["JARVIS_LOCAL_MODEL_PROVIDER"] == "ollama")
        #expect(environment["JARVIS_LOCAL_MODEL"] == "qwen2.5:7b")
        #expect(environment["JARVIS_OLLAMA_BASE_URL"] == "http://127.0.0.1:11434")
        #expect(environment["JARVIS_LOCAL_MODEL_TIMEOUT_MS"] == "45000")
        #expect(environment["JARVIS_CHATGPT_ENABLED"] == "false")
    }

    @Test("Model configuration defaults to fake local provider for packaged smoke")
    func modelConfigurationDefaultsToFakeLocalProviderForPackagedSmoke() {
        let configuration = JarvisModelConfiguration.fromEnvironment([:])

        #expect(configuration.provider == .fake)
        #expect(configuration.launchEnvironmentOverrides["JARVIS_LOCAL_MODEL_PROVIDER"] == "fake")
        #expect(configuration.launchEnvironmentOverrides["JARVIS_LOCAL_MODEL"] == "fake-local-model")
        #expect(configuration.launchEnvironmentOverrides["JARVIS_CHATGPT_ENABLED"] == "false")
    }

    @Test("Model configuration maps selected Codex model to guarded cloud launch environment")
    func modelConfigurationMapsCodexSelectionToEnvironment() {
        let configuration = JarvisModelConfiguration(
            provider: .codex,
            codexModel: " gpt-codex-test ",
            codexBaseURL: " https://api.openai.test/v1/ ",
            timeoutMilliseconds: "45000"
        )

        let environment = configuration.launchEnvironmentOverrides

        #expect(environment["JARVIS_LOCAL_MODEL_ENABLED"] == "false")
        #expect(environment["JARVIS_CHATGPT_ENABLED"] == "true")
        #expect(environment["JARVIS_CHATGPT_AUTH"] == "api_key")
        #expect(environment["JARVIS_CHATGPT_MODEL"] == "gpt-codex-test")
        #expect(environment["JARVIS_OPENAI_BASE_URL"] == "https://api.openai.test/v1")
        #expect(environment["JARVIS_CHATGPT_TIMEOUT_MS"] == "45000")
        #expect(environment["JARVIS_CHATGPT_REQUIRES_APPROVAL"] == "true")
        #expect(environment["JARVIS_OPENAI_API_KEY"] == nil)
    }

    @Test("Model configuration maps Codex account auth to CLI-backed launch environment")
    func modelConfigurationMapsCodexAccountSelectionToEnvironment() {
        let configuration = JarvisModelConfiguration(
            provider: .codexAccount,
            codexModel: " gpt-5.5 ",
            codexExecutable: " /Applications/Codex.app/Contents/Resources/codex ",
            timeoutMilliseconds: "45000"
        )

        let environment = configuration.launchEnvironmentOverrides

        #expect(environment["JARVIS_LOCAL_MODEL_ENABLED"] == "false")
        #expect(environment["JARVIS_CHATGPT_ENABLED"] == "true")
        #expect(environment["JARVIS_CHATGPT_AUTH"] == "codex_account")
        #expect(environment["JARVIS_CHATGPT_MODEL"] == "gpt-5.5")
        #expect(environment["JARVIS_CODEX_EXECUTABLE"] == "/Applications/Codex.app/Contents/Resources/codex")
        #expect(environment["JARVIS_CHATGPT_TIMEOUT_MS"] == "45000")
        #expect(environment["JARVIS_CHATGPT_REQUIRES_APPROVAL"] == "true")
        #expect(environment["JARVIS_OPENAI_API_KEY"] == nil)
        #expect(environment["JARVIS_OPENAI_BASE_URL"] == nil)
    }

    @MainActor
    @Test("Model configuration manages Codex application credential through credential store")
    func modelConfigurationManagesCodexCredential() {
        let store = FakeCredentialStore()
        let model = ModelConfigurationModel(credentialStore: store)

        #expect(!model.hasStoredCodexCredential)

        model.codexAPIKeyEntry = " test-key "
        model.saveCodexCredential()

        #expect(store.values[.openAIAPIKey] == "test-key")
        #expect(model.hasStoredCodexCredential)
        #expect(model.codexAPIKeyEntry.isEmpty)

        model.deleteCodexCredential()

        #expect(store.values[.openAIAPIKey] == nil)
        #expect(!model.hasStoredCodexCredential)
    }

    @MainActor
    @Test("Model configuration health prefers active Codex provider when cloud routing is enabled")
    func modelConfigurationHealthPrefersCodexProviderWhenEnabled() {
        let model = ModelConfigurationModel(credentialStore: FakeCredentialStore())

        model.applyHealth(JarvisHealth(
            status: "ok",
            version: "0.1.4",
            emergencyPaused: false,
            emergencyPauseReason: nil,
            schedulerJobs: 0,
            commandRuntime: "routed-codex-cloud-model+first-party-plugins",
            localModelProvider: "fake",
            localModel: "fake-local-model",
            chatgptEnabled: true,
            chatgptAuthMode: "codex_account",
            chatgptModel: "gpt-codex-test"
        ))

        #expect(model.activeProvider == "codex account")
        #expect(model.activeModel == "gpt-codex-test")
    }

    @MainActor
    @Test("Supervisor enters degraded mode when no core executable is configured")
    func supervisorDegradesWithoutExecutable() async {
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: nil,
                databaseURL: nil,
                startupTimeoutSeconds: 0.01,
                healthPollIntervalNanoseconds: 1
            ),
            client: FakeCoreClient(healthResults: [.failure(URLError(.cannotConnectToHost))]),
            processLauncher: FakeProcessLauncher()
        )

        await supervisor.start()

        if case let .degraded(reason) = supervisor.mode {
            #expect(reason.contains("executable"))
        } else {
            Issue.record("expected degraded mode, got \(supervisor.mode)")
        }
    }

    @MainActor
    @Test("Supervisor launches configured core and waits for health")
    func supervisorLaunchesConfiguredCore() async {
        let launcher = FakeProcessLauncher()
        let credentialProvider = JarvisCoreCredentialProvider(
            store: FakeCredentialStore(values: [.openAIAPIKey: "keychain-token"])
        )
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                bindAddress: "127.0.0.1:9901",
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil,
                startupTimeoutSeconds: 0.1,
                healthPollIntervalNanoseconds: 1
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth())
            ]),
            processLauncher: launcher,
            credentialProvider: credentialProvider
        )

        await supervisor.start()

        #expect(supervisor.mode == .available)
        #expect(launcher.launches.count == 1)
        #expect(launcher.launches.first?.executableURL.path == "/tmp/jarvis-cli")
        #expect(launcher.launches.first?.arguments == ["serve", "--bind", "127.0.0.1:9901"])
        #expect(launcher.launches.first?.environment["JARVIS_OPENAI_API_KEY"] == "keychain-token")
        #expect(supervisor.lastHealth?.status == "ok")
        #expect(supervisor.smokeSnapshot.canAttemptPackagedCoreSmoke)
        #expect(supervisor.smokeSnapshot.summary.contains("jarvis-cli"))
    }

    @MainActor
    @Test("Supervisor launch applies selected model environment overrides")
    func supervisorLaunchAppliesModelEnvironmentOverrides() async {
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                bindAddress: "127.0.0.1:9902",
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil,
                startupTimeoutSeconds: 0.1,
                healthPollIntervalNanoseconds: 1
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth())
            ]),
            processLauncher: launcher
        )

        await supervisor.start(environmentOverrides: [
            "JARVIS_LOCAL_MODEL_PROVIDER": "ollama",
            "JARVIS_LOCAL_MODEL": "qwen2.5:7b",
            "JARVIS_OLLAMA_BASE_URL": "http://127.0.0.1:11434"
        ])

        #expect(launcher.launches.first?.environment["JARVIS_LOCAL_MODEL_PROVIDER"] == "ollama")
        #expect(launcher.launches.first?.environment["JARVIS_LOCAL_MODEL"] == "qwen2.5:7b")
        #expect(launcher.launches.first?.environment["JARVIS_OLLAMA_BASE_URL"] == "http://127.0.0.1:11434")
    }

    @MainActor
    @Test("Supervisor waits for launched core health to match selected model configuration")
    func supervisorWaitsForMatchingLaunchedCoreHealth() async {
        let launcher = FakeProcessLauncher()
        let mismatched = JarvisHealth(
            status: "ok",
            version: "0.1.0",
            emergencyPaused: false,
            emergencyPauseReason: nil,
            schedulerJobs: 0,
            commandRuntime: "routed-codex-cloud-model+first-party-plugins",
            chatgptEnabled: true,
            chatgptAuthMode: "api_key",
            chatgptModel: "gpt-old",
            chatgptRequiresApproval: true
        )
        let matching = JarvisHealth(
            status: "ok",
            version: "0.1.0",
            emergencyPaused: false,
            emergencyPauseReason: nil,
            schedulerJobs: 0,
            commandRuntime: "routed-codex-cloud-model+first-party-plugins",
            chatgptEnabled: true,
            chatgptAuthMode: "codex_account",
            chatgptModel: "gpt-codex-test",
            chatgptRequiresApproval: true
        )
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                bindAddress: "127.0.0.1:9906",
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil,
                startupTimeoutSeconds: 0.1,
                healthPollIntervalNanoseconds: 1
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(mismatched),
                .success(matching)
            ]),
            processLauncher: launcher
        )

        await supervisor.start(
            environmentOverrides: [
                "JARVIS_CHATGPT_ENABLED": "true",
                "JARVIS_CHATGPT_AUTH": "codex_account",
                "JARVIS_CHATGPT_MODEL": "gpt-codex-test",
                "JARVIS_CHATGPT_REQUIRES_APPROVAL": "true"
            ],
            requireMatchingConfiguration: true
        )

        #expect(supervisor.mode == .available)
        #expect(supervisor.lastHealth?.chatgptAuthMode == "codex_account")
        #expect(launcher.launches.count == 1)
    }

    @MainActor
    @Test("Supervisor stop waits for the supervised process to exit")
    func supervisorStopWaitsForProcessExit() async {
        let launcher = DelayedStopProcessLauncher(runningChecksAfterTerminate: 2)
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                bindAddress: "127.0.0.1:9907",
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil,
                startupTimeoutSeconds: 0.1,
                healthPollIntervalNanoseconds: 1
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth())
            ]),
            processLauncher: launcher
        )

        await supervisor.start()
        let stopped = await supervisor.stop()

        #expect(stopped)
        #expect(launcher.process.terminateCalled)
        #expect(launcher.process.runningChecksAfterTerminate == 0)
        #expect(supervisor.mode == .stopped)
        #expect(supervisor.lastHealth == nil)
    }

    @MainActor
    @Test("Supervisor does not replace a process that misses the shutdown timeout")
    func supervisorDoesNotReplaceProcessAfterShutdownTimeout() async {
        let launcher = DelayedStopProcessLauncher(runningChecksAfterTerminate: .max)
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                bindAddress: "127.0.0.1:9908",
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil,
                startupTimeoutSeconds: 0.001,
                healthPollIntervalNanoseconds: 100_000
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth()),
                .failure(URLError(.cannotConnectToHost))
            ]),
            processLauncher: launcher
        )

        await supervisor.start()
        let stopped = await supervisor.stop()
        await supervisor.start()

        #expect(!stopped)
        #expect(launcher.launchCount == 1)
        if case let .degraded(reason) = supervisor.mode {
            #expect(reason.contains("still running"))
        } else {
            Issue.record("expected degraded mode, got \(supervisor.mode)")
        }
    }

    @MainActor
    @Test("Supervisor rejects mismatched existing core when selected model configuration is required")
    func supervisorRejectsMismatchedExistingCoreForRequiredConfiguration() async {
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                bindAddress: "127.0.0.1:9903",
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil,
                startupTimeoutSeconds: 0.1,
                healthPollIntervalNanoseconds: 1
            ),
            client: FakeCoreClient(healthResults: [
                .success(JarvisHealth(
                    status: "ok",
                    version: "0.1.0",
                    emergencyPaused: false,
                    emergencyPauseReason: nil,
                    schedulerJobs: 0,
                    commandRuntime: "routed-ollama-local-model+first-party-plugins",
                    localModelProvider: "ollama",
                    localModel: "llama3.2",
                    localEndpointConfigured: true
                ))
            ]),
            processLauncher: launcher
        )

        await supervisor.start(
            environmentOverrides: [
                "JARVIS_CHATGPT_ENABLED": "true",
                "JARVIS_CHATGPT_AUTH": "codex_account",
                "JARVIS_CHATGPT_MODEL": "gpt-codex-test",
                "JARVIS_CHATGPT_REQUIRES_APPROVAL": "true"
            ],
            requireMatchingConfiguration: true
        )

        #expect(launcher.launches.isEmpty)
        if case let .degraded(reason) = supervisor.mode {
            #expect(reason.contains("different Jarvis core"))
            #expect(reason.contains("selected model"))
        } else {
            Issue.record("expected degraded mode, got \(supervisor.mode)")
        }
    }

    @MainActor
    @Test("Supervisor accepts matching existing core when selected model configuration is required")
    func supervisorAcceptsMatchingExistingCoreForRequiredConfiguration() async {
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                bindAddress: "127.0.0.1:9904",
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil,
                startupTimeoutSeconds: 0.1,
                healthPollIntervalNanoseconds: 1
            ),
            client: FakeCoreClient(healthResults: [
                .success(JarvisHealth(
                    status: "ok",
                    version: "0.1.0",
                    emergencyPaused: false,
                    emergencyPauseReason: nil,
                    schedulerJobs: 0,
                    commandRuntime: "routed-codex-cloud-model+first-party-plugins",
                    chatgptEnabled: true,
                    chatgptAuthMode: "codex_account",
                    chatgptModel: "gpt-codex-test",
                    chatgptRequiresApproval: true
                ))
            ]),
            processLauncher: launcher
        )

        await supervisor.start(
            environmentOverrides: [
                "JARVIS_CHATGPT_ENABLED": "true",
                "JARVIS_CHATGPT_AUTH": "codex_account",
                "JARVIS_CHATGPT_MODEL": "gpt-codex-test",
                "JARVIS_CHATGPT_REQUIRES_APPROVAL": "true"
            ],
            requireMatchingConfiguration: true
        )

        #expect(supervisor.mode == .available)
        #expect(launcher.launches.isEmpty)
    }

    @MainActor
    @Test("Supervisor rejects existing cloud core with different ChatGPT auth mode")
    func supervisorRejectsExistingCloudCoreWithDifferentChatGPTAuthMode() async {
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                bindAddress: "127.0.0.1:9905",
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil,
                startupTimeoutSeconds: 0.1,
                healthPollIntervalNanoseconds: 1
            ),
            client: FakeCoreClient(healthResults: [
                .success(JarvisHealth(
                    status: "ok",
                    version: "0.1.0",
                    emergencyPaused: false,
                    emergencyPauseReason: nil,
                    schedulerJobs: 0,
                    commandRuntime: "routed-codex-cloud-model+first-party-plugins",
                    chatgptEnabled: true,
                    chatgptAuthMode: "api_key",
                    chatgptModel: "gpt-codex-test",
                    chatgptRequiresApproval: true
                ))
            ]),
            processLauncher: launcher
        )

        await supervisor.start(
            environmentOverrides: [
                "JARVIS_CHATGPT_ENABLED": "true",
                "JARVIS_CHATGPT_AUTH": "codex_account",
                "JARVIS_CHATGPT_MODEL": "gpt-codex-test",
                "JARVIS_CHATGPT_REQUIRES_APPROVAL": "true"
            ],
            requireMatchingConfiguration: true
        )

        #expect(launcher.launches.isEmpty)
        if case let .degraded(reason) = supervisor.mode {
            #expect(reason.contains("different Jarvis core"))
        } else {
            Issue.record("expected degraded mode, got \(supervisor.mode)")
        }
    }

    @MainActor
    @Test("Model configuration model starts and stops selected Ollama model")
    func modelConfigurationModelControlsSelectedOllamaModel() async {
        let controller = CapturingModelRuntimeController()
        let model = ModelConfigurationModel(
            configuration: JarvisModelConfiguration(
                provider: .ollama,
                localModel: "llama3.2",
                ollamaBaseURL: "http://127.0.0.1:11434",
                timeoutMilliseconds: "60000"
            ),
            controller: controller
        )

        await model.loadSelectedModel()
        await model.unloadSelectedModel()

        #expect(controller.loadRequests == [
            CapturingModelRuntimeController.Request(model: "llama3.2", baseURL: "http://127.0.0.1:11434")
        ])
        #expect(controller.unloadRequests == [
            CapturingModelRuntimeController.Request(model: "llama3.2", baseURL: "http://127.0.0.1:11434")
        ])
    }

    @MainActor
    @Test("Model configuration model lists installed models with RAM estimates")
    func modelConfigurationModelListsInstalledModelsWithMemoryEstimates() async {
        let controller = CapturingModelRuntimeController(installedModels: [
            JarvisOllamaModelInfo(
                name: "custom-local:latest",
                installed: true,
                diskSizeBytes: 4_294_967_296,
                estimatedRamBytes: 4_294_967_296,
                details: "llama / 7B / Q4"
            ),
            JarvisOllamaModelInfo(
                name: "llama3.2:latest",
                installed: true,
                diskSizeBytes: 2_019_393_189,
                estimatedRamBytes: 2_019_393_189,
                details: "llama / 3.2B / Q4"
            )
        ])
        let model = ModelConfigurationModel(
            configuration: JarvisModelConfiguration(provider: .ollama, localModel: "llama3.2"),
            controller: controller
        )

        await model.refreshAvailableModels()

        let customModel = try! #require(model.availableModels.first { $0.name == "custom-local:latest" })
        #expect(customModel.installed)
        #expect(customModel.memoryLine.contains("4"))
        #expect(model.availableModels.contains { $0.name == "llama3.2:latest" })
        #expect(!model.availableModels.contains { $0.name == "llama3.2" })
        #expect(model.selectedModelIsInstalled)
    }

    @MainActor
    @Test("Selecting missing model downloads it automatically")
    func selectingMissingModelDownloadsAutomatically() async {
        let controller = CapturingModelRuntimeController()
        let model = ModelConfigurationModel(
            configuration: JarvisModelConfiguration(provider: .ollama, localModel: "llama3.2"),
            controller: controller
        )
        let missingModel = JarvisOllamaModelInfo(name: "qwen2.5:7b", installed: false)

        await model.selectModel(missingModel)

        #expect(model.configuration.localModel == "qwen2.5:7b")
        #expect(controller.pullRequests == [
            CapturingModelRuntimeController.Request(model: "qwen2.5:7b", baseURL: "http://127.0.0.1:11434")
        ])
        #expect(model.selectedModelIsInstalled)
        #expect(model.downloadProgress == nil)
        #expect(model.statusMessage == "qwen2.5:7b downloaded and reloaded through Ollama.")
    }

    @MainActor
    @Test("Download selected model tracks progress and reloads inventory")
    func downloadSelectedModelTracksProgressAndReloadsInventory() async {
        let controller = CapturingModelRuntimeController()
        let model = ModelConfigurationModel(
            configuration: JarvisModelConfiguration(provider: .ollama, localModel: "gemma3:270m"),
            controller: controller
        )

        await model.downloadSelectedModel()

        #expect(controller.pullRequests == [
            CapturingModelRuntimeController.Request(model: "gemma3:270m", baseURL: "http://127.0.0.1:11434")
        ])
        #expect(controller.listRequests.count == 1)
        #expect(controller.progressSnapshots == [
            JarvisOllamaPullProgress(status: "pulling manifest"),
            JarvisOllamaPullProgress(status: "downloading", completedBytes: 50, totalBytes: 100),
            JarvisOllamaPullProgress(status: "success")
        ])
        #expect(model.selectedModelIsInstalled)
        #expect(model.availableModels.first { $0.name == "gemma3:270m" }?.installed == true)
        #expect(model.downloadProgress == nil)
    }

    @Test("Ollama runtime controller sends inventory, pull, load, and unload HTTP requests")
    func ollamaRuntimeControllerSendsExpectedHTTPRequests() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [CapturingURLProtocol.self]
        let session = URLSession(configuration: configuration)
        let controller = OllamaModelRuntimeController(urlSession: session)
        let baseURL = URL(string: "http://ollama.test")!
        await CapturingURLProtocol.reset()
        await CapturingURLProtocol.setResponder { request in
            let path = request.url?.path ?? ""
            if path == "/api/tags" {
                return (
                    200,
                    """
                    {"models":[{"name":"qwen2.5:7b","size":4700000000,"details":{"family":"qwen2","parameter_size":"7B","quantization_level":"Q4_K_M"}}]}
                    """.data(using: .utf8)!
                )
            }
            if path == "/api/pull" {
                return (
                    200,
                    """
                    {"status":"pulling manifest"}
                    {"status":"downloading","completed":25,"total":100}
                    {"status":"success"}
                    """.data(using: .utf8)!
                )
            }
            return (200, #"{"status":"ok"}"#.data(using: .utf8)!)
        }

        let progressRecorder = CapturingProgressRecorder()
        let models = try await controller.listOllamaModels(baseURL: baseURL)
        try await controller.pullOllamaModel(model: "qwen2.5:7b", baseURL: baseURL) { progress in
            await progressRecorder.append(progress)
        }
        try await controller.loadOllamaModel(model: "qwen2.5:7b", baseURL: baseURL)
        try await controller.unloadOllamaModel(model: "qwen2.5:7b", baseURL: baseURL)

        let requests = await CapturingURLProtocol.requests
        let bodies = await CapturingURLProtocol.bodyStrings
        let progressEvents = await progressRecorder.events
        #expect(models.first?.name == "qwen2.5:7b")
        #expect(models.first?.estimatedRamBytes == 4_700_000_000)
        #expect(requests.map { $0.url?.path } == ["/api/tags", "/api/pull", "/api/generate", "/api/generate"])
        #expect(bodies[1]?.contains("\"model\":\"qwen2.5:7b\"") == true)
        #expect(bodies[1]?.contains("\"stream\":true") == true)
        #expect(bodies[2]?.contains("\"keep_alive\":\"5m\"") == true)
        #expect(bodies[3]?.contains("\"keep_alive\":\"0\"") == true)
        #expect(progressEvents == [
            JarvisOllamaPullProgress(status: "pulling manifest"),
            JarvisOllamaPullProgress(status: "downloading", completedBytes: 25, totalBytes: 100),
            JarvisOllamaPullProgress(status: "success")
        ])
    }

    @Test("Ollama runtime controller normalizes update-required pull errors")
    func ollamaRuntimeControllerNormalizesUpdateRequiredPullErrors() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [CapturingURLProtocol.self]
        let session = URLSession(configuration: configuration)
        let controller = OllamaModelRuntimeController(urlSession: session)
        let baseURL = URL(string: "http://ollama.test")!
        await CapturingURLProtocol.reset()
        await CapturingURLProtocol.setResponder { request in
            let path = request.url?.path ?? ""
            if path == "/api/pull" {
                return (
                    200,
                    """
                    {"error":"pull model manifest: 412: \\nThe model you are attempting to pull requires a newer version of Ollama.\\n\\nPlease download the latest version at:\\n\\nhttps://ollama.com/download"}
                    """.data(using: .utf8)!
                )
            }
            return (200, Data())
        }

        do {
            try await controller.pullOllamaModel(model: "gemma4:12b", baseURL: baseURL) { _ in }
            Issue.record("expected update-required pull error")
        } catch {
            let message = error.localizedDescription
            #expect(message.hasPrefix("Update Ollama before retrying."))
            #expect(message.contains("requires a newer version of Ollama"))
            #expect(message.contains("https://ollama.com/download"))
            #expect(!message.contains("\\n"))
        }
    }

    private func memoryItemJSON(id: UUID) -> Data {
        Data(
            """
            {
              "id": "\(id.uuidString)",
              "category": "release",
              "key": "release-gate",
              "value": "preview before write",
              "provenance": "manual",
              "sensitivity": "workspace",
              "created_at": "2026-05-20T12:00:00Z",
              "updated_at": "2026-05-20T12:00:01Z",
              "reviewed_at": null,
              "deleted_at": null
            }
            """.utf8
        )
    }

    private func memoryClassificationJSON() -> Data {
        Data(
            """
            {
              "generated_at": "2026-05-20T12:00:02Z",
              "include_deleted": true,
              "total_count": 2,
              "active_count": 1,
              "deleted_count": 1,
              "reviewed_count": 0,
              "unreviewed_active_count": 1,
              "sensitive_active_count": 1,
              "by_sensitivity": [
                {
                  "label": "private",
                  "count": 1,
                  "active_count": 1,
                  "deleted_count": 0,
                  "unreviewed_active_count": 1
                }
              ],
              "by_category": [
                {
                  "label": "release",
                  "count": 2,
                  "active_count": 1,
                  "deleted_count": 1,
                  "unreviewed_active_count": 1
                }
              ]
            }
            """.utf8
        )
    }

    private func memoryRetentionPlanJSON(id: UUID) -> Data {
        Data(
            """
            {
              "generated_at": "2026-05-20T12:00:03Z",
              "status": "operator_review_required",
              "candidate_count": 1,
              "unreviewed_active_count": 0,
              "deleted_sensitive_retained_count": 1,
              "next_required_action": "review candidates, then mark reviewed, restore, or purge outside Jarvis storage with operator approval",
              "automation_enabled": false,
              "value_redaction_required": true,
              "candidates": [
                {
                  "memory_id": "\(id.uuidString)",
                  "category": "retention",
                  "key": "deleted-secret",
                  "sensitivity": "private",
                  "status": "deleted_sensitive_retained",
                  "severity": "high",
                  "reason": "Deleted sensitive memory is still retained in local storage",
                  "recommended_action": "operator_purge_or_restore",
                  "reviewed_at": null,
                  "deleted_at": "2026-05-20T12:00:02Z"
                }
              ]
            }
            """.utf8
        )
    }

    private func schedulerJobJSON(id: UUID) -> Data {
        Data(
            """
            {
              "id": "\(id.uuidString)",
              "name": "one shot",
              "command": "status check",
              "trigger": "manual",
              "status": "scheduled",
              "created_at": "2026-05-20T12:00:00Z",
              "updated_at": "2026-05-20T12:00:01Z",
              "cancelled_at": null,
              "cancellation_reason": null
            }
            """.utf8
        )
    }

    private func schedulerAttentionJSON(
        id: UUID,
        emergencyPaused: Bool = false,
        notificationKind: String = "due_now",
        notificationReason: String = "A scheduler job is due and ready for the app to surface.",
        nextDueAt: String = "2026-05-20T12:00:01Z"
    ) -> Data {
        Data(
            """
            {
              "generated_at": "2026-05-20T12:00:02Z",
              "emergency_paused": \(emergencyPaused),
              "attention_required": true,
              "due_count": 1,
              "scheduled_count": 1,
              "running_count": 0,
              "failed_count": 0,
              "next_due_at": null,
              "items": [
                {
                  "id": "\(id.uuidString)",
                  "name": "one shot",
                  "trigger": "manual",
                  "status": "scheduled",
                  "due": true,
                  "next_due_at": "\(nextDueAt)",
                  "notification_kind": "\(notificationKind)",
                  "notification_reason": "\(notificationReason)"
                }
              ]
            }
            """.utf8
        )
    }

    private func schedulerNotificationOccurrence(
        id: UUID = UUID(),
        jobId: UUID,
        notificationKind: String = "due_now",
        revision: UInt64 = 1
    ) -> JarvisSchedulerNotificationOccurrence {
        JarvisSchedulerNotificationOccurrence(
            id: id,
            schedulerJobId: jobId,
            name: "one shot",
            occurrenceAt: "2026-05-20T12:00:01Z",
            notificationKind: notificationKind,
            revision: revision,
            createdAt: "2026-05-20T12:00:01Z",
            updatedAt: "2026-05-20T12:00:01Z",
            acknowledgedAt: nil,
            acknowledgedDisposition: nil
        )
    }

    private func schedulerRunDueJSON(id: UUID) -> Data {
        Data(
            """
            {
              "checked_at": "2026-05-20T12:00:04Z",
              "limit": 4,
              "emergency_paused": false,
              "executions": [
                {
                  "job": {
                    "id": "\(id.uuidString)",
                    "name": "one shot",
                    "command": "status check",
                    "trigger": "manual",
                    "status": "completed",
                    "created_at": "2026-05-20T12:00:00Z",
                    "updated_at": "2026-05-20T12:00:04Z",
                    "cancelled_at": null,
                    "cancellation_reason": null
                  },
                  "task": {
                    "id": "\(id.uuidString)",
                    "session_id": "\(UUID().uuidString)",
                    "user_input": "status check",
                    "status": "completed",
                    "created_at": "2026-05-20T12:00:03Z",
                    "updated_at": "2026-05-20T12:00:04Z"
                  },
                  "accepted": true,
                  "message": "scheduled command completed",
                  "audit_entries": [
                    {
                      "id": "\(UUID().uuidString)",
                      "task_id": "\(id.uuidString)",
                      "event_type": "scheduler_job_completed",
                      "summary": "scheduler finished due job command",
                      "payload": { "command_redacted": true },
                      "created_at": "2026-05-20T12:00:04Z"
                    }
                  ]
                }
              ]
            }
            """.utf8
        )
    }

    private func schedulerRecoverStaleJSON(id: UUID) -> Data {
        Data(
            """
            {
              "checked_at": "2026-05-20T12:00:05Z",
              "older_than_seconds": 120,
              "limit": 2,
              "recovered": [
                {
                  "job": {
                    "id": "\(id.uuidString)",
                    "name": "stale running job",
                    "trigger": "manual",
                    "status": "failed",
                    "created_at": "2026-05-20T11:00:00Z",
                    "updated_at": "2026-05-20T12:00:05Z",
                    "cancelled_at": null,
                    "cancellation_reason_present": false
                  },
                  "stale_since": "2026-05-20T11:55:00Z",
                  "stale_for_seconds": 300,
                  "audit_entry": {
                    "id": "\(UUID().uuidString)",
                    "task_id": null,
                    "event_type": "scheduler_stale_running_recovered",
                    "summary": "scheduler marked a stale running job failed for explicit operator recovery",
                    "payload": { "command_redacted": true },
                    "created_at": "2026-05-20T12:00:05Z"
                  }
                }
              ]
            }
            """.utf8
        )
    }

    private func activitySummaryJSON(taskId: UUID) -> Data {
        Data(
            """
            {
              "generated_at": "2026-05-20T12:00:03Z",
              "repository_backed": true,
              "task_count": 2,
              "audit_entry_count": 3,
              "active_task_count": 1,
              "status_counts": [
                { "status": "running", "count": 1 },
                { "status": "completed", "count": 1 }
              ],
              "recent_tasks": [
                {
                  "id": "\(taskId.uuidString)",
                  "session_id": "\(UUID().uuidString)",
                  "status": "running",
                  "created_at": "2026-05-20T12:00:00Z",
                  "updated_at": "2026-05-20T12:00:01Z"
                }
              ],
              "recent_audit_entries": [
                {
                  "id": "\(UUID().uuidString)",
                  "task_id": "\(taskId.uuidString)",
                  "event_type": "plugin_completed",
                  "summary": "plugin completed",
                  "payload": { "side_effect_executed": true },
                  "created_at": "2026-05-20T12:00:02Z"
                }
              ]
            }
            """.utf8
        )
    }

    private func activityEventsSSE(taskId: UUID) -> Data {
        let summary = String(decoding: activitySummaryJSON(taskId: taskId), as: UTF8.self)
            .replacingOccurrences(of: "\n", with: "")
        let progress = """
        {"audit_id":"\(UUID().uuidString)","task_id":"\(taskId.uuidString)","created_at":"2026-05-20T12:00:02Z","kind":"installed_plugin","plugin_id":"local_runner_test","action":"inspect","session_id":"\(UUID().uuidString)","sequence":1,"stage":"prepare","message":"validated request","stderr_redacted":true}
        """
            .replacingOccurrences(of: "\n", with: "")
        let modelProgress = """
        {"audit_id":"\(UUID().uuidString)","task_id":"\(taskId.uuidString)","created_at":"2026-05-20T12:00:03Z","kind":"model_step","provider":"local","model":"fake-local-model","sequence":0,"stage":"completed","message":"model step 0 completed","stderr_redacted":true}
        """
            .replacingOccurrences(of: "\n", with: "")
        let modelOutput = """
        {"audit_id":"\(UUID().uuidString)","task_id":"\(taskId.uuidString)","created_at":"2026-05-20T12:00:04Z","kind":"model_output","provider":"local","model":"fake-local-model","sequence":0,"stage":"step_0","message":"model step 0 output chunk 0","byte_count":42,"char_count":42,"final_chunk":true,"content_redacted":true,"provider_native":true,"stderr_redacted":true}
        """
            .replacingOccurrences(of: "\n", with: "")
        return Data(
            """
            event: activity_summary
            data: \(summary)

            event: activity_progress
            data: \(progress)

            event: activity_progress
            data: \(modelProgress)

            event: activity_progress
            data: \(modelOutput)

            event: activity_error
            data: {"error":"repository unavailable"}

            """.utf8
        )
    }

    private func contractJSON(exposesApprovalEndpoint: Bool) -> Data {
        let extra = exposesApprovalEndpoint
            ? """
            ,{ "method": "GET", "path": "/approvals", "repository_required": true, "redacted": false },
                { "method": "GET", "path": "/approvals/:id", "repository_required": true, "redacted": false },
                { "method": "POST", "path": "/approvals/:id/approve", "repository_required": true, "redacted": false },
                { "method": "POST", "path": "/approvals/:id/deny", "repository_required": true, "redacted": false }
            """
            : ""
        return Data(
            """
            {
              "contract": {
                "name": "jarvis.local-ipc",
                "version": 1,
                "core_version": "0.1.0"
              },
              "endpoints": [
                { "method": "GET", "path": "/health", "repository_required": false, "redacted": true },
                { "method": "GET", "path": "/scheduler/jobs/:id", "repository_required": false, "redacted": false }
                \(extra)
              ],
              "safe_inspection_paths": ["/health", "/diagnostics/export"]
            }
            """.utf8
        )
    }
}

private func fakeMemoryRetentionPlanJSON(id: UUID) -> Data {
    Data(
        """
        {
          "generated_at": "2026-05-20T12:00:03Z",
          "status": "operator_review_required",
          "candidate_count": 1,
          "unreviewed_active_count": 0,
          "deleted_sensitive_retained_count": 1,
          "next_required_action": "review candidates, then mark reviewed, restore, or purge outside Jarvis storage with operator approval",
          "automation_enabled": false,
          "value_redaction_required": true,
          "candidates": [
            {
              "memory_id": "\(id.uuidString)",
              "category": "retention",
              "key": "deleted-secret",
              "sensitivity": "private",
              "status": "deleted_sensitive_retained",
              "severity": "high",
              "reason": "Deleted sensitive memory is still retained in local storage",
              "recommended_action": "operator_purge_or_restore",
              "reviewed_at": null,
              "deleted_at": "2026-05-20T12:00:02Z"
            }
          ]
        }
        """.utf8
    )
}

private func temporaryDirectory(name: String) throws -> URL {
    let directory = FileManager.default.temporaryDirectory
        .appending(path: "\(name)-\(UUID().uuidString)", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory
}

private func writeExecutableStub(at url: URL) throws {
    try Data("#!/bin/sh\nexit 0\n".utf8).write(to: url)
    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
}

private func sampleHealth() -> JarvisHealth {
    JarvisHealth(
        status: "ok",
        version: "0.1.0",
        emergencyPaused: false,
        emergencyPauseReason: nil,
        schedulerJobs: 0,
        commandRuntime: "routed-fake-local-model+first-party-plugins"
    )
}

private func fullApprovalContract() -> JarvisContractResponse {
    try! JSONDecoder().decode(
        JarvisContractResponse.self,
        from: Data(
            """
            {
              "contract": { "name": "jarvis.local-ipc", "version": 1, "core_version": "0.1.0" },
              "endpoints": [
                { "method": "GET", "path": "/health", "repository_required": false, "redacted": true },
                { "method": "GET", "path": "/permissions/grants", "repository_required": true, "redacted": false },
                { "method": "GET", "path": "/permissions/policy-review", "repository_required": true, "redacted": false },
                { "method": "GET", "path": "/approvals", "repository_required": true, "redacted": false },
                { "method": "GET", "path": "/approvals/:id", "repository_required": true, "redacted": false },
                { "method": "POST", "path": "/approvals/:id/approve", "repository_required": true, "redacted": false },
                { "method": "POST", "path": "/approvals/:id/deny", "repository_required": true, "redacted": false },
                { "method": "POST", "path": "/approvals/:id/execute", "repository_required": true, "redacted": false }
              ],
              "safe_inspection_paths": ["/health", "/permissions/grants", "/permissions/policy-review", "/approvals"]
            }
            """.utf8
        )
    )
}

private func samplePendingApproval(
    id: UUID = UUID(),
    taskId: UUID = UUID(),
    action: String = "fake_echo.approval_echo",
    status: String = "pending",
    decidedBy: String? = nil,
    decisionReason: String? = nil
) -> JarvisPendingApproval {
    try! JSONDecoder().decode(
        JarvisPendingApproval.self,
        from: pendingApprovalJSON(
            id: id,
            taskId: taskId,
            action: action,
            status: status,
            decidedBy: decidedBy,
            decisionReason: decisionReason
        )
    )
}

private func pendingApprovalJSON(
    id: UUID,
    taskId: UUID,
    action: String = "fake_echo.approval_echo",
    status: String = "pending",
    decidedBy: String? = nil,
    decisionReason: String? = nil
) -> Data {
    let decidedAt = decidedBy == nil ? "null" : #""2026-05-20T12:01:00Z""#
    let encodedDecidedBy = decidedBy.map { #""\#($0)""# } ?? "null"
    let encodedDecisionReason = decisionReason.map { #""\#($0)""# } ?? "null"

    return Data(
        """
        {
          "id": "\(id.uuidString)",
          "task_id": "\(taskId.uuidString)",
          "action": "\(action)",
          "requested_scopes": ["conversation", "file_write"],
          "risk_tier": "confirm",
          "sensitivity": "workspace",
          "status": "\(status)",
          "reason": "Tool execution requires approval before continuing.",
          "requested_at": "2026-05-20T12:00:00Z",
          "decided_at": \(decidedAt),
          "decided_by": \(encodedDecidedBy),
          "decision_reason": \(encodedDecisionReason)
        }
        """.utf8
    )
}

private func approvalExecutionJSON(approvalId: UUID, taskId: UUID) -> Data {
    let sessionId = UUID()
    let auditId = UUID()
    return Data(
        """
        {
          "accepted": true,
          "approval": \(String(decoding: pendingApprovalJSON(id: approvalId, taskId: taskId, status: "approved", decidedBy: "mac-ui", decisionReason: "reviewed"), as: UTF8.self)),
          "task": {
            "id": "\(taskId.uuidString)",
            "session_id": "\(sessionId.uuidString)",
            "user_input": "plugin approval echo needs user approval",
            "status": "completed",
            "created_at": "2026-05-20T12:00:00Z",
            "updated_at": "2026-05-20T12:02:00Z"
          },
          "audit_entry": {
            "id": "\(auditId.uuidString)",
            "task_id": "\(taskId.uuidString)",
            "event_type": "approval_executed",
            "summary": "approved first-party plugin action execution completed",
            "payload": { "approval_id": "\(approvalId.uuidString)", "side_effect_executed": true },
            "created_at": "2026-05-20T12:02:00Z"
          },
          "audit_entries": [],
          "plugin_results": [
            {
              "status": "completed",
              "output": { "message": "needs user approval" },
              "metadata": {
                "plugin_id": "fake_echo",
                "action": "approval_echo",
                "permissions": ["file_write"],
                "risk_tier": "confirm",
                "approval_required": true,
                "approval_status": "approved",
                "proactive": false,
                "memory_access": "none",
                "model_access": "none",
                "timeout_ms": 5000,
                "cancellation": "cooperative",
                "audit_fields": ["message"]
              }
            }
          ],
          "message": "approved first-party plugin action executed"
        }
        """.utf8
    )
}

private func installedApprovalExecutionJSON(approval: JarvisPendingApproval) -> Data {
    let sessionId = UUID()
    let auditId = UUID()
    return Data(
        """
        {
          "accepted": true,
          "approval": \(String(decoding: pendingApprovalJSON(id: approval.id, taskId: approval.taskId, action: approval.action, status: "approved", decidedBy: "mac-ui", decisionReason: "reviewed installed action"), as: UTF8.self)),
          "task": {
            "id": "\(approval.taskId.uuidString)",
            "session_id": "\(sessionId.uuidString)",
            "user_input": "installed plugin action awaiting approval: local_installed.confirm_action",
            "status": "completed",
            "created_at": "2026-07-14T12:00:00Z",
            "updated_at": "2026-07-14T12:02:00Z"
          },
          "audit_entry": {
            "id": "\(auditId.uuidString)",
            "task_id": "\(approval.taskId.uuidString)",
            "event_type": "approval_executed",
            "summary": "approved action reached a non-retryable terminal state",
            "payload": { "approval_id": "\(approval.id.uuidString)", "side_effect_executed": true },
            "created_at": "2026-07-14T12:02:00Z"
          },
          "audit_entries": [],
          "plugin_results": [],
          "installed_plugin_result": {
            "plugin_id": "local_installed",
            "action": "confirm_action",
            "status": "completed",
            "reason": "installed plugin completed after explicit approval",
            "execution_enabled": true,
            "execution_grant": "subprocess_stdio",
            "contract_validated": true,
            "side_effect_executed": true,
            "runtime_kind": "subprocess",
            "input": { "secret": "do-not-expose-bound-input" },
            "input_sha256": "binding-digest-must-stay-core-only",
            "contract_sha256": "binding-digest-must-stay-core-only"
          },
          "message": "approved installed plugin action executed"
        }
        """.utf8
    )
}

private func permissionGrantSummaryJSON(approvalId: UUID = UUID(), taskId: UUID = UUID()) -> Data {
    Data(
        """
        {
          "generated_at": "2026-05-20T12:02:00Z",
          "approval_counts": [
            { "status": "pending", "count": 1 },
            { "status": "approved", "count": 2 },
            { "status": "denied", "count": 1 }
          ],
          "latest_approvals": [
            \(String(decoding: pendingApprovalJSON(id: approvalId, taskId: taskId), as: UTF8.self))
          ],
          "installed_plugin_grants": [
            {
              "plugin_id": "local_e2e_plugin",
              "name": "Local E2E Plugin",
              "execution_enabled": false,
              "execution_grant": "metadata_only",
              "integrity_status": "not_verified",
              "capture_method": "local_manifest_snapshot",
              "last_verified_at": null,
              "origin_claim": "Jarvis Test",
              "origin_claim_verified": false,
              "installed_at": "2026-05-20T12:00:00Z",
              "action_count": 1,
              "high_risk_action_count": 0
            }
          ],
          "high_risk_pending_count": 1,
          "executable_installed_plugin_count": 0,
          "unverified_installed_plugin_count": 1,
          "side_effects_require_approval": true
        }
        """.utf8
    )
}

private func releaseReadinessJSON() -> Data {
    Data(
        """
        {
          "generated_at": "2026-05-22T08:00:00Z",
          "production_ready": false,
          "evidence_mode_enabled": false,
          "readiness_scope": "local Rust/CLI foundation and Swift shell evidence only; full production distribution still has external manual gates",
          "verified_feature_count": 8,
          "pending_feature_count": 1,
          "implemented_features": [
            {
              "key": "repository_state",
              "status": "implemented",
              "proof": "SQLite-backed task, audit, model-route, memory, scheduler, approval, and installed-plugin state is covered by Rust unit tests and local IPC E2E.",
              "boundary": "Local repository evidence only; no hosted sync or multi-device state claim."
            },
            {
              "key": "installed_plugin_execution",
              "status": "implemented",
              "proof": "Local subprocess plugins require full source-tree provenance verification plus explicit grants and emit audit evidence that reports os_sandbox_enforced:false.",
              "boundary": "Constrained local subprocess execution only; audit evidence reports os_sandbox_enforced:false, so this is not a WASM, OS-level, host-egress, or marketplace sandbox."
            },
            {
              "key": "plugin_network_governance",
              "status": "implemented",
              "proof": "Network-capable plugin actions must declare exact allowed hosts and require the explicit subprocess_stdio_network execution grant.",
              "boundary": "Runtime grant gate plus manifest governance only; not OS-level network sandbox enforcement or host-level egress filtering."
            },
            {
              "key": "operator_release_qa_smoke",
              "status": "implemented",
              "proof": "`release-operator-qa-smoke.sh` exercises repository-backed operator QA.",
              "boundary": "Local CLI/operator QA evidence only."
            },
            {
              "key": "release_ci_gate",
              "status": "implemented",
              "proof": "`.github/workflows/release-local.yml` runs `./scripts/release-local.sh` on macOS for pull requests, pushes to main, and manual dispatch.",
              "boundary": "Public CI evidence for the repo-owned local release gate only."
            },
            {
              "key": "unsigned_distribution_launch",
              "status": "implemented",
              "proof": "`package-distribution.sh --unsigned-launch-check` builds the release app layout.",
              "boundary": "Unsigned distribution-layout proof only."
            },
            {
              "key": "release_evidence_status",
              "status": "implemented",
              "proof": "`/release/evidence-status` and `jarvis release evidence-status` validate release evidence inventory, repository-backed command-result evidence, host-egress policy fields, archive-URI validation, and child-report semantic revalidation.",
              "boundary": "Read-only file/report inventory plus report semantic validation only."
            },
            {
              "key": "release_evidence_bundle",
              "status": "implemented",
              "proof": "`release-evidence-bundle.sh --bundle` validates durable reports archive URI evidence, writes SHA-256-bound evidence manifest entries, and child reports are revalidated by doctor/status checks.",
              "boundary": "Evidence-bundle mechanics and local artifact/report validation only."
            }
          ],
          "pending_features": [
            {
              "key": "live_voice_loop",
              "status": "pending_manual_validation",
              "proof": "Swift voice input and speech-output adapters have deterministic fake-adapter tests, including final transcript staging and opt-in final-transcript auto-submit into the text command path.",
              "boundary": "Live microphone, Speech permission, spoken transcript handoff, live audio output, and device validation are not proven by automated tests."
            }
          ],
          "blocking_manual_gates": [
            "Developer ID Application and Installer signing credentials configured and used for a full signed package run",
            "live microphone and Speech permission prompt validation plus spoken transcript handoff on a real Mac",
            "final release evidence bundle generated and archived after signed distribution, live-device QA, and plugin-trust QA reports exist"
          ],
          "recommended_verification_commands": [
            "./scripts/release-local.sh",
            "./scripts/release-ci-workflow-smoke.sh",
            "./scripts/release-operator-qa-smoke.sh",
            "./scripts/package-distribution.sh --check",
            "./scripts/package-distribution.sh --unsigned-launch-check",
            "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh",
            "cargo run -p jarvis-cli -- release live-device-runbook",
            "./scripts/release-live-device-qa.sh --check",
            "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env",
            "set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete",
            "JARVIS_QA_CLEAN_PROFILE_VALIDATED=true JARVIS_QA_FINDER_LAUNCH_VALIDATED=true JARVIS_QA_MICROPHONE_VALIDATED=true JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true JARVIS_QA_NOTIFICATION_VALIDATED=true JARVIS_QA_RESTART_VALIDATED=true JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true JARVIS_QA_OWNER_NAME='Release Operator' JARVIS_QA_DEVICE_LABEL='Clean-profile release Mac' JARVIS_QA_PROFILE_LABEL='Clean macOS QA profile' JARVIS_QA_VOICE_CHECK_STARTED_AT='2026-05-22T16:00:00Z' JARVIS_QA_VOICE_CHECK_COMPLETED_AT='2026-05-22T16:05:00Z' JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE='Clean profile install observed' JARVIS_QA_FINDER_LAUNCH_EVIDENCE_NOTE='Finder launch observed' JARVIS_QA_MICROPHONE_EVIDENCE_NOTE='Microphone prompt and capture observed' JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE='Speech prompt and recognition observed' JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE='Spoken transcript reached the command path' JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE='Speech output playback observed' JARVIS_QA_NOTIFICATION_EVIDENCE_NOTE='Scheduler notification observed' JARVIS_QA_NOTIFICATION_OBSERVED_AT='2026-05-22T16:04:00Z' JARVIS_QA_RESTART_EVIDENCE_NOTE='Restart recovery observed' JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE='Manual release QA surfaces observed' JARVIS_QA_VOICE_TEST_PHRASE='Jarvis status check' JARVIS_QA_OBSERVED_TRANSCRIPT='Jarvis status check' JARVIS_QA_EXPECTED_COMMAND_TEXT='status check' JARVIS_QA_OBSERVED_COMMAND_TEXT='status check' JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid-from-live-command>' JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL='Built-in speakers' ./scripts/release-live-device-qa.sh --assert-complete",
            "./scripts/release-plugin-trust-qa.sh --check",
            "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env",
            "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete",
            "JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED=true JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED=true JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED=true JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED=true JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED=true JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED=true JARVIS_PLUGIN_QA_OWNER_NAME='Release Operator' JARVIS_PLUGIN_QA_REVIEW_STARTED_AT='2026-05-22T16:10:00Z' JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT='2026-05-22T16:20:00Z' JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE='Marketplace review evidence archived' JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE='Malware scan evidence archived' JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE='OS sandbox validation evidence archived' JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE='Host-level egress validation evidence archived' JARVIS_PLUGIN_QA_EGRESS_POLICY_LABEL='Host egress policy/profile reviewed' JARVIS_PLUGIN_QA_EGRESS_VALIDATION_COMPLETED_AT='2026-05-22T16:18:00Z' JARVIS_PLUGIN_QA_EGRESS_DENY_FIXTURE_EVIDENCE_NOTE='Undeclared-host deny fixture evidence archived' JARVIS_PLUGIN_QA_EGRESS_ALLOW_FIXTURE_EVIDENCE_NOTE='Declared-host allow fixture evidence archived' JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE='Signed publisher policy evidence archived' JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE='Manual plugin trust review evidence archived' ./scripts/release-plugin-trust-qa.sh --assert-complete",
            "./scripts/release-evidence-bundle.sh --check",
            "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env",
            "./scripts/release-evidence-doctor.sh --check",
            "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle",
            "JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true JARVIS_EVIDENCE_REPORTS_ARCHIVED=true ./scripts/release-evidence-bundle.sh --bundle",
            "./scripts/release-evidence-doctor.sh --assert-complete",
            "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external",
            "cargo run -p jarvis-cli -- release readiness"
          ],
          "proof_boundary": "Read-only summary derived from /contract feature metadata and release checklist blockers; it does not perform signing, notarization, stapling, installation, Finder/LaunchServices validation, live microphone/Speech validation, spoken transcript handoff, live audio-output validation, App Store review, marketplace plugin review, malware analysis, or OS sandbox enforcement."
        }
        """.utf8
    )
}

private func externalProductionReadyReleaseReadinessJSON() -> Data {
    Data(
        """
        {
          "generated_at": "2026-05-22T17:05:00Z",
          "production_ready": true,
          "evidence_mode_enabled": true,
          "readiness_scope": "local Rust/CLI foundation and Swift shell evidence plus explicitly enabled external release evidence status",
          "verified_feature_count": 4,
          "pending_feature_count": 0,
          "implemented_features": [
            {
              "key": "repository_state",
              "status": "implemented",
              "proof": "SQLite-backed task, audit, model-route, memory, scheduler, approval, and installed-plugin state is covered by Rust unit tests and local IPC E2E.",
              "boundary": "Local repository evidence only; no hosted sync or multi-device state claim."
            },
            {
              "key": "release_ci_gate",
              "status": "implemented",
              "proof": "The public release-local CI gate is present.",
              "boundary": "Public CI evidence for the repo-owned local release gate only."
            },
            {
              "key": "live_voice_loop",
              "status": "implemented",
              "proof": "A valid owner-recorded live-device QA report is present through explicitly enabled release evidence status.",
              "boundary": "Owner-recorded live-device QA evidence for the referenced release candidate only."
            },
            {
              "key": "release_evidence_bundle",
              "status": "implemented",
              "proof": "A valid final evidence bundle is present in explicitly enabled external evidence mode.",
              "boundary": "Evidence-bundle mechanics and owner-recorded external evidence only."
            }
          ],
          "pending_features": [],
          "blocking_manual_gates": [],
          "recommended_verification_commands": [
            "./scripts/release-local.sh",
            "./scripts/release-ci-workflow-smoke.sh",
            "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external",
            "cargo run -p jarvis-cli -- release readiness"
          ],
          "proof_boundary": "Read-only summary derived from /contract feature metadata, release checklist blockers, and explicitly enabled external release evidence status; it does not perform signing, notarization, stapling, installation, Finder/LaunchServices validation, live microphone/Speech validation, spoken transcript handoff, live audio-output validation, App Store review, marketplace plugin review, malware analysis, or OS sandbox enforcement."
        }
        """.utf8
    )
}

private func releaseEvidenceStatusJSON() -> Data {
    Data(
        """
        {
          "generated_at": "2026-05-22T08:05:00Z",
          "complete": false,
          "satisfied_count": 3,
          "missing_count": 6,
          "invalid_count": 0,
          "items": [
            {
              "key": "signed_app_bundle",
              "label": "App bundle path",
              "path": "target/distribution/Jarvis.app",
              "kind": "directory",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "directory exists; Info.plist bundle identifier, short version, and build version match expected release metadata; signing, notarization, and stapling are not validated by evidence-status"
            },
            {
              "key": "app_executable",
              "label": "App executable",
              "path": "target/distribution/Jarvis.app/Contents/MacOS/JarvisMacApp",
              "kind": "executable",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "executable file exists; presence only; signing, notarization, and stapling are not validated by evidence-status"
            },
            {
              "key": "bundled_core_executable",
              "label": "Bundled core executable",
              "path": "target/distribution/Jarvis.app/Contents/Resources/bin/jarvis-cli",
              "kind": "executable",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "executable file exists; bundled core version marker matches expected release version; signing, notarization, and stapling are not validated by evidence-status"
            },
            {
              "key": "signed_app_zip",
              "label": "App zip path",
              "path": "target/distribution/Jarvis-0.1.4.zip",
              "kind": "file",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected evidence path is missing"
            },
            {
              "key": "signed_installer_package",
              "label": "Installer package path",
              "path": "target/distribution/Jarvis-0.1.4.pkg",
              "kind": "file",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected evidence path is missing"
            },
            {
              "key": "live_device_qa_report",
              "label": "Live-device QA report",
              "path": "target/release-live-device-qa-report.json",
              "kind": "json_report",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected JSON report is missing"
            },
            {
              "key": "plugin_trust_qa_report",
              "label": "Plugin-trust QA report",
              "path": "target/release-plugin-trust-qa-report.json",
              "kind": "json_report",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected JSON report is missing"
            },
            {
              "key": "signed_distribution_provenance_report",
              "label": "Signed-distribution provenance report",
              "path": "target/distribution/Jarvis-0.1.4-signed-provenance.json",
              "kind": "json_report",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected JSON report is missing"
            },
            {
              "key": "release_evidence_bundle",
              "label": "Release evidence bundle",
              "path": "target/release-evidence-bundle.json",
              "kind": "json_report",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected JSON report is missing"
            }
          ],
          "proof_boundary": "File/report inventory only; complete means expected paths are present, app bundle metadata matches the expected bundle identifier/version/build, bundled core version-marker metadata matches the expected release version, and JSON reports pass required field checks plus signed-provenance artifact digest matching, live-device QA release-metadata/non-future timestamp semantics, required repository-backed task/audit command-result evidence resolution, plugin-trust non-future timestamp and owner-asserted review-source semantics, and final evidence-bundle path/digest/archive-URI/signature-validation/non-future timestamp semantics. This endpoint does not sign, notarize, staple, install, Finder-launch, execute release artifacts, run live-device QA, run marketplace review, scan malware, or enforce an OS sandbox/egress policy."
        }
        """.utf8
    )
}

private func completeReleaseEvidenceStatusJSON() -> Data {
    Data(
        """
        {
          "generated_at": "2026-05-22T17:06:00Z",
          "complete": true,
          "satisfied_count": 9,
          "missing_count": 0,
          "invalid_count": 0,
          "items": [
            {
              "key": "signed_app_bundle",
              "label": "App bundle path",
              "path": "target/distribution/Jarvis.app",
              "kind": "directory",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "directory exists; presence only; signing, notarization, and stapling are not validated by evidence-status"
            },
            {
              "key": "app_executable",
              "label": "App executable",
              "path": "target/distribution/Jarvis.app/Contents/MacOS/JarvisMacApp",
              "kind": "executable",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "executable file exists; presence only; signing, notarization, and stapling are not validated by evidence-status"
            },
            {
              "key": "bundled_core_executable",
              "label": "Bundled core executable",
              "path": "target/distribution/Jarvis.app/Contents/Resources/bin/jarvis-cli",
              "kind": "executable",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "executable file exists; bundled core version marker matches expected release version; signing, notarization, and stapling are not validated by evidence-status"
            },
            {
              "key": "signed_app_zip",
              "label": "App zip path",
              "path": "target/distribution/Jarvis-0.1.4.zip",
              "kind": "file",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "file exists; presence only; signing, notarization, and stapling are not validated by evidence-status"
            },
            {
              "key": "signed_installer_package",
              "label": "Installer package path",
              "path": "target/distribution/Jarvis-0.1.4.pkg",
              "kind": "file",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "file exists; presence only; signing, notarization, and stapling are not validated by evidence-status"
            },
            {
              "key": "signed_distribution_provenance_report",
              "label": "Signed-distribution provenance report",
              "path": "target/distribution/Jarvis-0.1.4-signed-provenance.json",
              "kind": "json_report",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "JSON report exists, expected release version and bundle identifier match, signing/notarization/stapling/Gatekeeper evidence fields are present, required flags are true, and artifact SHA-256 digests match the current zip/pkg files; clean-profile install and live-device QA remain separate manual gates"
            },
            {
              "key": "live_device_qa_report",
              "label": "Live-device QA report",
              "path": "target/release-live-device-qa-report.json",
              "kind": "json_report",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "JSON report exists, required owner-recorded fields are present, release metadata plus timestamps match expected values, repository-backed task/audit command-result evidence resolves, and bundled_core.sha256 matches signed-provenance artifacts.bundled_core_sha256; live-device claims are still owner-recorded external evidence"
            },
            {
              "key": "plugin_trust_qa_report",
              "label": "Plugin-trust QA report",
              "path": "target/release-plugin-trust-qa-report.json",
              "kind": "json_report",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "JSON report exists, self_test_fixture=false, review_source=owner-asserted-manual-review, required owner-recorded fields are present, review and egress validation timestamps are valid and ordered, and deny/allow egress fixture notes are present; marketplace, malware, sandbox, and host-level egress claims remain owner-recorded external evidence"
            },
            {
              "key": "release_evidence_bundle",
              "label": "Release evidence bundle",
              "path": "target/release-evidence-bundle.json",
              "kind": "json_report",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "JSON report exists, expected release version matches, artifact/report paths and SHA-256 digests match current artifacts and reports, referenced child reports are semantically valid, owner completion is ordered after child report generation and before final bundle generation, reports archive URI is durable and non-placeholder, and local signature validation is true; signed_distribution and notarization remain owner-recorded external evidence"
            }
          ],
          "proof_boundary": "File/report inventory only; complete means expected paths are present, app bundle metadata matches the expected bundle identifier/version/build, bundled core version-marker metadata matches the expected release version, and JSON reports pass required field checks plus signed-provenance artifact digest matching, live-device QA release-metadata/non-future timestamp semantics, required repository-backed task/audit command-result evidence resolution, plugin-trust non-future timestamp and owner-asserted review-source semantics, and final evidence-bundle path/digest/archive-URI/signature-validation/non-future timestamp semantics. This endpoint does not sign, notarize, staple, install, Finder-launch, execute release artifacts, run live-device QA, run marketplace review, scan malware, or enforce an OS sandbox/egress policy."
        }
        """.utf8
    )
}

private func releaseRunbookJSON(runbook: String) -> Data {
    let evidenceItems: String
    let boundary: String
    let commands: String
    let firstManualCheck: String
    let liveVoiceFeature: String

    switch runbook {
    case "signed_distribution":
        evidenceItems = """
            {
              "key": "signed_app_bundle",
              "label": "Signed app bundle",
              "path": "target/distribution/Jarvis.app",
              "kind": "file",
              "status": "present",
              "required_for_production": true,
              "manual_gate": false,
              "detail": "app bundle present"
            },
            {
              "key": "app_executable",
              "label": "App executable",
              "path": "target/distribution/Jarvis.app/Contents/MacOS/Jarvis",
              "kind": "file",
              "status": "present",
              "required_for_production": true,
              "manual_gate": false,
              "detail": "presence only"
            },
            {
              "key": "bundled_core_executable",
              "label": "Bundled core executable",
              "path": "target/distribution/Jarvis.app/Contents/Resources/bin/jarvis",
              "kind": "file",
              "status": "present",
              "required_for_production": true,
              "manual_gate": false,
              "detail": "presence only"
            },
            {
              "key": "signed_app_zip",
              "label": "Signed app zip",
              "path": "target/distribution/Jarvis-0.1.4-signed.zip",
              "kind": "file",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "signed zip is missing"
            },
            {
              "key": "signed_installer_package",
              "label": "Signed installer package",
              "path": "target/distribution/Jarvis-0.1.4.pkg",
              "kind": "file",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "signed installer package is missing"
            },
            {
              "key": "signed_distribution_provenance_report",
              "label": "Signed-distribution provenance report",
              "path": "target/distribution/Jarvis-0.1.4-signed-provenance.json",
              "kind": "json_report",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected JSON report is missing"
            }
        """
        boundary = "Runbook and local evidence inspection only; this endpoint does not perform signing."
        commands = """
            "./scripts/package-distribution.sh --check",
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \\"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\\""
        """
        firstManualCheck = "Configure Developer ID Application and Installer identities plus the notarytool profile on the release Mac."
        liveVoiceFeature = "null"
    case "plugin_trust":
        evidenceItems = """
            {
              "key": "plugin_trust_qa_report",
              "label": "Plugin-trust QA report",
              "path": "target/release-plugin-trust-qa-report.json",
              "kind": "json_report",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected JSON report is missing"
            }
        """
        boundary = "Runbook and local evidence inspection only; this endpoint does not perform marketplace review."
        commands = """
            "./scripts/release-plugin-trust-qa.sh --check",
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \\"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\\""
        """
        firstManualCheck = "Preserve malware scan evidence for distributed plugin archives and updates."
        liveVoiceFeature = "null"
    case "evidence_bundle":
        evidenceItems = """
            {
              "key": "signed_distribution_provenance_report",
              "label": "Signed-distribution provenance report",
              "path": "target/distribution/Jarvis-0.1.4-signed-provenance.json",
              "kind": "json_report",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected JSON report is missing"
            },
            {
              "key": "live_device_qa_report",
              "label": "Live-device QA report",
              "path": "target/release-live-device-qa-report.json",
              "kind": "json_report",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected JSON report is missing"
            },
            {
              "key": "plugin_trust_qa_report",
              "label": "Plugin-trust QA report",
              "path": "target/release-plugin-trust-qa-report.json",
              "kind": "json_report",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected JSON report is missing"
            },
            {
              "key": "release_evidence_bundle",
              "label": "Release evidence bundle",
              "path": "target/release-evidence-bundle.json",
              "kind": "json_report",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected JSON report is missing"
            }
        """
        boundary = "Runbook and local evidence inspection only; this endpoint does not generate the final bundle."
        commands = """
            "./scripts/release-evidence-bundle.sh --check",
            "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env",
            "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle",
            "./scripts/release-evidence-doctor.sh --assert-complete",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint \\"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\\""
        """
        firstManualCheck = "Generate the final evidence bundle only after signed-distribution, live-device QA, and plugin-trust QA reports exist and have been archived."
        liveVoiceFeature = "null"
    default:
        evidenceItems = """
            {
              "key": "live_device_qa_report",
              "label": "Live-device QA report",
              "path": "target/release-live-device-qa-report.json",
              "kind": "json_report",
              "status": "missing",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "expected JSON report is missing"
            }
        """
        boundary = "Runbook and local evidence inspection only; this endpoint does not perform live-device validation."
        commands = """
            "./scripts/release-live-device-qa.sh --check",
            "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env",
            "cargo run -p jarvis-cli -- command \\"status check\\" --endpoint <release-core-endpoint> --json",
            "Record the returned task ID as JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid>' or a task-associated audit ID as 'audit:<uuid>' in target/release-live-device-qa.env",
            "set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint <release-core-endpoint>",
            "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint <release-core-endpoint>"
        """
        firstManualCheck = "Verify microphone and Speech permission prompts during live voice capture."
        liveVoiceFeature = """
        {
          "key": "live_voice_loop",
          "status": "pending_manual_validation",
          "proof": "Swift adapters have deterministic fake-adapter tests.",
          "boundary": "Live-device validation is not proven by automated tests."
        }
        """
    }

    return Data(
        """
        {
          "generated_at": "2026-05-22T08:10:00Z",
          "generated_from": "release readiness plus evidence-status",
          "runbook": "\(runbook)",
          "production_ready": false,
          "live_voice_feature": \(liveVoiceFeature),
          "evidence_items": [
            \(evidenceItems)
          ],
          "commands": [
            \(commands)
          ],
          "manual_checks": [
            "\(firstManualCheck)"
          ],
          "proof_boundary": "\(boundary)"
        }
        """.utf8
    )
}

private func invalidLiveDeviceEvidenceStatusJSON() -> Data {
    Data(
        """
        {
          "generated_at": "2026-05-22T08:05:00Z",
          "complete": false,
          "satisfied_count": 1,
          "missing_count": 0,
          "invalid_count": 1,
          "items": [
            {
              "key": "signed_app_bundle",
              "label": "App bundle path",
              "path": "target/distribution/Jarvis.app",
              "kind": "directory",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "directory exists; presence only; signing, notarization, and stapling are not validated by evidence-status"
            },
            {
              "key": "live_device_qa_report",
              "label": "Live-device QA report",
              "path": "target/release-live-device-qa-report.json",
              "kind": "json_report",
              "status": "invalid",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "JSON report app_bundle.bundle_identifier mismatch: expected com.nobiletechnology.jarvis, got com.example.StaleJarvis"
            }
          ],
          "proof_boundary": "File/report inventory only; complete means expected paths are present, app bundle metadata matches the expected bundle identifier/version/build, bundled core version-marker metadata matches the expected release version, and JSON reports pass required field checks plus signed-provenance artifact digest matching, live-device QA release-metadata/non-future timestamp semantics, required repository-backed task/audit command-result evidence resolution, plugin-trust non-future timestamp and owner-asserted review-source semantics, and final evidence-bundle path/digest/archive-URI/signature-validation/non-future timestamp semantics. This endpoint does not sign, notarize, staple, install, Finder-launch, execute release artifacts, run live-device QA, run marketplace review, scan malware, or enforce an OS sandbox/egress policy."
        }
        """.utf8
    )
}

private func permissionPolicyReviewJSON(approvalId: UUID = UUID()) -> Data {
    Data(
        """
        {
          "generated_at": "2026-05-20T12:03:00Z",
          "status": "review_required",
          "review_item_count": 4,
          "high_risk_pending_count": 1,
          "executable_installed_plugin_count": 0,
          "unverified_installed_plugin_count": 1,
          "unreviewed_memory_item_count": 1,
          "sensitive_memory_item_count": 1,
          "side_effects_require_approval": true,
          "items": [
            {
              "item_type": "pending_approval",
              "severity": "high",
              "title": "Pending approval requires review",
              "detail": "plugin approval echo requests Confirm access for Workspace data",
              "approval_id": "\(approvalId.uuidString)",
              "action": "plugin approval echo"
            },
            {
              "item_type": "installed_plugin_provenance",
              "severity": "medium",
              "title": "Installed plugin provenance is not verified",
              "detail": "Local E2E Plugin integrity status is not_verified",
              "plugin_id": "local_e2e_plugin"
            },
            {
              "item_type": "memory_review",
              "severity": "high",
              "title": "Memory item needs review",
              "detail": "Memory item preference/voice is Private and unreviewed; value text is redacted from policy review",
              "memory_id": "\(UUID().uuidString)",
              "action": "preference/voice"
            },
            {
              "item_type": "memory_retention_review",
              "severity": "high",
              "title": "Deleted sensitive memory is retained locally",
              "detail": "Deleted memory item retention/deleted-secret is Private and still retained in local storage; value text is redacted from policy review",
              "memory_id": "\(UUID().uuidString)",
              "action": "retention/deleted-secret"
            }
          ]
        }
        """.utf8
    )
}

private func samplePermissionGrantSummary(approval: JarvisPendingApproval) -> JarvisPermissionGrantSummary {
    try! JSONDecoder().decode(
        JarvisPermissionGrantSummary.self,
        from: permissionGrantSummaryJSON(approvalId: approval.id, taskId: approval.taskId)
    )
}

private func diagnosticsJSON() -> Data {
    Data(
        """
        {
          "generated_at": "2026-05-20T12:00:00Z",
          "redaction": "redacted",
          "health": {
            "status": "ok",
            "version": "0.1.0",
            "started_at": "2026-05-20T11:59:00Z",
            "emergency_paused": false,
            "emergency_pause_reason": null,
            "emergency_pause_reason_present": false,
            "emergency_pause_updated_at": null,
            "scheduler_jobs": 0,
            "command_runtime": "routed-fake-local-model+first-party-plugins"
          },
          "scheduler_jobs": [],
          "repository_backed": false,
          "schema_version": null,
          "task_count": null,
          "audit_entry_count": null,
          "active_memory_item_count": null
        }
        """.utf8
    )
}

private func samplePluginManifest() -> JarvisPluginManifest {
    JarvisPluginManifest(
        id: "calendar",
        name: "Calendar",
        version: "0.1.0",
        source: "first-party",
        author: "Jarvis",
        actions: [
            JarvisPluginActionManifest(
                name: "inspect_events",
                description: "Inspect calendar events",
                permissions: ["conversation", "file_read"],
                riskTier: "allow",
                inputSchema: .object([:]),
                outputSchema: .object([:]),
                proactive: false,
                memoryAccess: "none",
                modelAccess: "local",
                auditFields: ["calendar_id"],
                timeout: JarvisPluginTimeout(timeoutMilliseconds: 1_000),
                cancellation: "supported"
            ),
            JarvisPluginActionManifest(
                name: "create_event",
                description: "Create a calendar event",
                permissions: ["calendar_write", "conversation"],
                riskTier: "confirm",
                inputSchema: .object([:]),
                outputSchema: .object([:]),
                proactive: true,
                memoryAccess: "read",
                modelAccess: "local",
                auditFields: ["calendar_id", "event_id"],
                timeout: JarvisPluginTimeout(timeoutMilliseconds: 2_000),
                cancellation: "supported"
            )
        ]
    )
}

private func sampleModelToolCatalog() -> JarvisModelToolCatalog {
    try! JSONDecoder().decode(JarvisModelToolCatalog.self, from: modelToolCatalogJSON())
}

private func modelToolCatalogJSON() -> Data {
    Data(
        """
        {
          "generated_at": "2026-07-13T12:00:00Z",
          "source": "registered_first_party_plugins",
          "tools": [
            {
              "plugin_id": "workspace_inspect",
              "action": "list",
              "description": "List a bounded allowlisted workspace directory.",
              "risk_tier": "low",
              "scopes": ["read_workspace"],
              "proactive": false,
              "constraints": {
                "read_only": true,
                "bounded": true,
                "no_network": true,
                "local_model_only": true
              }
            }
          ],
          "proof_boundary": "Redacted registered first-party capability metadata only."
        }
        """.utf8
    )
}

private func installedPluginsJSON() -> Data {
    Data(
        """
        [
          {
            "id": "local_runner_test",
            "manifest": {
              "id": "local_runner_test",
              "name": "Local Runner Test",
              "version": "0.1.0",
              "source": "local_subprocess",
              "author": "Jarvis Test",
              "actions": [
                {
                  "name": "inspect",
                  "description": "Inspect a workspace path.",
                  "permissions": ["read_workspace"],
                  "risk_tier": "low",
                  "input_schema": { "type": "object", "properties": { "path": { "type": "string" } } },
                  "output_schema": { "type": "object" },
                  "proactive": false,
                  "memory_access": "none",
                  "model_access": "none",
                  "network_access": { "mode": "none" },
                  "audit_fields": ["path"],
                  "timeout": { "timeout_ms": 5000, "on_timeout": "cancel" },
                  "cancellation": "cooperative"
                }
              ]
            },
            "provenance": {
              "provenance_schema_version": 1,
              "capture_method": "local_manifest_snapshot",
              "source_path_canonicalized": true,
              "captured_at": "2026-05-20T12:00:00Z",
              "last_verified_at": null,
              "integrity_status": "not_verified",
              "origin_claim": "Jarvis Test",
              "origin_claim_verified": false
            },
            "execution_enabled": false,
            "execution_grant": "metadata_only",
            "installed_at": "2026-05-20T12:00:00Z"
          }
        ]
        """.utf8
    )
}

private func installedWasmPluginJSON() -> Data {
    Data(
        """
        [
          {
            "id": "local_wasm_compute",
            "manifest": {
              "id": "local_wasm_compute",
              "name": "Local WASM Compute",
              "version": "0.1.0",
              "source": "local_wasm",
              "author": "Jarvis Test",
              "actions": [
                {
                  "name": "compute",
                  "description": "Run bounded deterministic computation.",
                  "permissions": ["compute"],
                  "risk_tier": "low",
                  "input_schema": { "type": "object" },
                  "output_schema": { "type": "object" },
                  "proactive": false,
                  "memory_access": "none",
                  "model_access": "none",
                  "network_access": { "mode": "none" },
                  "audit_fields": [],
                  "timeout": { "timeout_ms": 5000, "on_timeout": "cancel" },
                  "cancellation": "cooperative"
                }
              ]
            },
            "provenance": {
              "provenance_schema_version": 1,
              "capture_method": "local_manifest_snapshot",
              "source_path_canonicalized": true,
              "captured_at": "2026-07-13T12:00:00Z",
              "last_verified_at": "2026-07-13T12:01:00Z",
              "integrity_status": "matches_install_snapshot",
              "origin_claim": "Jarvis Test",
              "origin_claim_verified": false
            },
            "execution_enabled": true,
            "execution_grant": "wasm_compute",
            "runtime_kind": "wasm",
            "wasm_confinement_enforced": true,
            "os_sandbox_enforced": false,
            "installed_at": "2026-07-13T12:00:00Z"
          }
        ]
        """.utf8
    )
}

private func commandResponseJSON(input: String, status: String = "completed") -> Data {
    let taskId = UUID()
    let sessionId = UUID()
    let auditId = UUID()
    return Data(
        """
        {
          "accepted": true,
          "task": {
            "id": "\(taskId.uuidString)",
            "session_id": "\(sessionId.uuidString)",
            "user_input": "\(input)",
            "status": "\(status)",
            "created_at": "2026-05-20T12:00:00Z",
            "updated_at": "2026-05-20T12:00:01Z"
          },
          "audit_entry": {
            "id": "\(auditId.uuidString)",
            "task_id": "\(taskId.uuidString)",
            "event_type": "task_completed",
            "summary": "command completed",
            "payload": {},
            "created_at": "2026-05-20T12:00:01Z"
          },
          "audit_entries": [],
          "route": {
            "provider": "local",
            "model": "fake-local-model",
            "reason": "local model is the default route for v1 commands"
          },
          "steps": [
            { "index": 0, "message": "local response: \(input)", "complete": true }
          ],
          "plugin_results": [],
          "message": "local response: \(input)"
        }
        """.utf8
    )
}

private func releaseSmokeBlockedCommandResponseJSON(input: String) -> Data {
    let taskId = UUID()
    let sessionId = UUID()
    let auditId = UUID()
    return Data(
        """
        {
          "accepted": false,
          "task": {
            "id": "\(taskId.uuidString)",
            "session_id": "\(sessionId.uuidString)",
            "user_input": "\(input)",
            "status": "blocked",
            "created_at": "2026-07-14T12:00:00Z",
            "updated_at": "2026-07-14T12:00:01Z"
          },
          "audit_entry": {
            "id": "\(auditId.uuidString)",
            "task_id": "\(taskId.uuidString)",
            "event_type": "emergency_pause_blocked",
            "summary": "emergency pause blocked command execution",
            "payload": { "emergency_paused": true },
            "created_at": "2026-07-14T12:00:01Z"
          },
          "audit_entries": [],
          "route": null,
          "steps": [],
          "plugin_results": [],
          "message": "Emergency pause is active; command execution is blocked."
        }
        """.utf8
    )
}

private func releaseSmokePauseResponse(paused: Bool, reason: String?) -> JarvisPauseResponse {
    JarvisPauseResponse(
        paused: paused,
        reason: reason,
        pausedAt: paused ? "2026-07-14T12:00:02Z" : nil,
        resumedAt: paused ? nil : "2026-07-14T12:00:03Z",
        cancelledSchedulerJobs: 0
    )
}

@Suite("Release smoke probe", .serialized)
struct ReleaseSmokeProbeTests {
    @Test("Probe verifies the complete route sequence before returning its fixed marker")
    func verifiesCompleteSequence() async throws {
        let client = FakeCoreClient(releaseSmokeMode: true)

        let result = try await JarvisReleaseSmokeProbe(
            client: client,
            timeout: .seconds(2)
        ).run()

        #expect(result == JarvisReleaseSmokeProbe.successLine)
        #expect(result == "Jarvis release smoke: default supervised Unix IPC route sequence verified")
        #expect(client.releaseSmokeCalls == [
            "health",
            "submitInitial",
            "task",
            "listTasks",
            "taskAudit",
            "diagnostics",
            "schedulerCreate",
            "schedulerJob",
            "schedulerAudit",
            "schedulerNotifications",
            "schedulerNotificationAck",
            "schedulerNotifications",
            "pause",
            "pauseStatusPaused",
            "submitPaused",
            "resume",
            "pauseStatusResumed"
        ])
        #expect(client.submittedCommandsWithoutCancellationIDs == [
            JarvisCommandRequest(
                input: "Jarvis release smoke deterministic dry-run check.",
                dryRun: true
            ),
            JarvisCommandRequest(
                input: "Jarvis release smoke deterministic dry-run check.",
                dryRun: true
            )
        ])
    }

    @Test("Probe returns no success marker when a representative read step fails")
    func failsClosedWithoutMarker() async {
        let client = FakeCoreClient(
            releaseSmokeMode: true,
            releaseSmokeFailureCall: "diagnostics"
        )
        var success: String?

        do {
            success = try await JarvisReleaseSmokeProbe(
                client: client,
                timeout: .seconds(2)
            ).run()
        } catch {
            // Expected: errors are deliberately not converted into output text.
        }

        #expect(success == nil)
        #expect(client.releaseSmokeCalls == [
            "health",
            "submitInitial",
            "task",
            "listTasks",
            "taskAudit",
            "diagnostics"
        ])
    }

    @Test("Probe best-effort resumes when a post-pause step fails")
    func resumesAfterPostPauseFailure() async {
        let client = FakeCoreClient(
            releaseSmokeMode: true,
            releaseSmokeFailureCall: "submitPaused"
        )
        var success: String?

        do {
            success = try await JarvisReleaseSmokeProbe(
                client: client,
                timeout: .seconds(2)
            ).run()
        } catch {
            // Expected: cleanup must run before the error escapes.
        }

        #expect(success == nil)
        #expect(client.releaseSmokeCalls == [
            "health",
            "submitInitial",
            "task",
            "listTasks",
            "taskAudit",
            "diagnostics",
            "schedulerCreate",
            "schedulerJob",
            "schedulerAudit",
            "schedulerNotifications",
            "schedulerNotificationAck",
            "schedulerNotifications",
            "pause",
            "pauseStatusPaused",
            "submitPaused",
            "resume"
        ])
    }
}

@MainActor
@Suite("Workspace root bookmarks", .serialized)
struct WorkspaceRootBookmarkTests {
    @Test("Application Support bookmark store is atomic and owner-only")
    func bookmarkStoreRoundTripsWithRestrictivePermissions() throws {
        let directory = FileManager.default.temporaryDirectory
            .appending(path: "jarvis-workspace-store-\(UUID().uuidString)", directoryHint: .isDirectory)
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appending(path: "bookmarks.json")
        let store = ApplicationSupportWorkspaceRootBookmarkStore(fileURL: file)
        let expected = [JarvisStoredWorkspaceRootBookmark(id: "root_test", bookmarkData: Data([1, 2, 3]))]

        try store.save(expected)

        #expect(try store.load() == expected)
        let fileMode = try #require(FileManager.default.attributesOfItem(atPath: file.path)[.posixPermissions] as? NSNumber)
        let directoryMode = try #require(FileManager.default.attributesOfItem(atPath: directory.path)[.posixPermissions] as? NSNumber)
        #expect(fileMode.intValue & 0o777 == 0o600)
        #expect(directoryMode.intValue & 0o777 == 0o700)
    }

    @Test("Coordinator refreshes stale bookmarks and balances access")
    func coordinatorRefreshesStaleBookmarkAndBalancesAccess() throws {
        let url = URL(fileURLWithPath: "/private/tmp/secret-workspace", isDirectory: true)
        let store = InMemoryWorkspaceRootBookmarkStore(records: [
            JarvisStoredWorkspaceRootBookmark(id: "root_test", bookmarkData: Data("old".utf8))
        ])
        let accessor = FakeWorkspaceRootBookmarkAccessor(
            resolutions: [Data("old".utf8): JarvisResolvedWorkspaceRootBookmark(url: url, isStale: true)],
            createdBookmark: Data("fresh".utf8)
        )
        let coordinator = JarvisWorkspaceRootBookmarkCoordinator(store: store, accessor: accessor)

        let lease = try coordinator.acquireForCoreLaunch()

        #expect(lease.roots.map(\.id) == ["root_test"])
        #expect(store.records.first?.bookmarkData == Data("fresh".utf8))
        #expect(accessor.startedURLs == [url])
        #expect(accessor.stoppedURLs.isEmpty)
        lease.release()
        lease.release()
        #expect(accessor.stoppedURLs == [url])
    }

    @Test("Coordinator fails closed and redacts an unavailable bookmark path")
    func coordinatorFailsClosedWithRedactedError() {
        let sensitivePath = "/Users/operator/SecretProject"
        let store = InMemoryWorkspaceRootBookmarkStore(records: [
            JarvisStoredWorkspaceRootBookmark(id: "root_safe", bookmarkData: Data(sensitivePath.utf8))
        ])
        let coordinator = JarvisWorkspaceRootBookmarkCoordinator(
            store: store,
            accessor: FakeWorkspaceRootBookmarkAccessor(resolutionError: TestWorkspaceRootError.failed)
        )

        do {
            _ = try coordinator.acquireForCoreLaunch()
            Issue.record("Expected bookmark resolution to fail")
        } catch {
            #expect(String(describing: error).contains("root_safe"))
            #expect(!String(describing: error).contains(sensitivePath))
        }
    }

    @Test("Supervisor sends one bounded redacted startup envelope and releases access")
    func supervisorUsesUnifiedWorkspaceRootEnvelope() async throws {
        let sensitivePath = "/Users/operator/SecretProject"
        let provider = FakeWorkspaceRootGrantProvider(roots: [
            JarvisWorkspaceRootLaunchRoot(id: "root_safe", path: sensitivePath)
        ])
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth())
            ]),
            processLauncher: launcher,
            workspaceRootProvider: provider
        )

        await supervisor.start(environmentOverrides: ["JARVIS_LOCAL_MODEL": "safe-model"])

        let launch = try #require(launcher.launches.first)
        #expect(launch.arguments.contains("--startup-config-stdin"))
        #expect(!launch.arguments.joined().contains(sensitivePath))
        #expect(!launch.environment.description.contains(sensitivePath))
        #expect(!supervisor.smokeSnapshot.summary.contains(sensitivePath))
        let input = try #require(launch.standardInput)
        #expect(input.count <= jarvisWorkspaceRootStartupEnvelopeMaximumBytes)
        let json = try #require(JSONSerialization.jsonObject(with: input) as? [String: Any])
        #expect(json["version"] as? Int == 1)
        let roots = try #require(json["workspace_roots"] as? [[String: String]])
        #expect(roots == [["id": "root_safe", "path": sensitivePath]])
        #expect(!provider.released)

        #expect(await supervisor.stop())
        #expect(provider.released)
    }

    @Test("Supervisor preserves automation for its healthy child and clears it for external core")
    func supervisorPreservesOwnedSchedulerAutomationAcrossRepeatedStart() async throws {
        let automation = JarvisSchedulerAutomationConfiguration(
            isEnabled: true,
            intervalMilliseconds: 5_000
        )
        let provider = StaticSchedulerAutomationConfigurationProvider(
            configuration: automation
        )
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth()),
                .success(sampleHealth())
            ]),
            processLauncher: launcher,
            workspaceRootProvider: FakeWorkspaceRootGrantProvider(roots: []),
            schedulerAutomationProvider: provider
        )

        await supervisor.start()
        #expect(supervisor.activeSchedulerAutomationConfiguration == automation)
        await supervisor.start()
        #expect(launcher.launches.count == 1)
        #expect(supervisor.activeSchedulerAutomationConfiguration == automation)
        #expect(await supervisor.stop())

        let externalLauncher = FakeProcessLauncher()
        let externalSupervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil
            ),
            client: FakeCoreClient(healthResults: [.success(sampleHealth())]),
            processLauncher: externalLauncher,
            workspaceRootProvider: FakeWorkspaceRootGrantProvider(roots: []),
            schedulerAutomationProvider: provider
        )

        await externalSupervisor.start()
        #expect(externalLauncher.launches.isEmpty)
        #expect(externalSupervisor.activeSchedulerAutomationConfiguration == nil)
    }

    @Test("Supervisor omits startup stdin when no authority is configured")
    func supervisorOmitsEmptyStartupEnvelope() async throws {
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth())
            ]),
            processLauncher: launcher,
            workspaceRootProvider: FakeWorkspaceRootGrantProvider(roots: [])
        )

        await supervisor.start()

        let launch = try #require(launcher.launches.first)
        #expect(!launch.arguments.contains("--startup-config-stdin"))
        #expect(launch.standardInput == nil)
    }

    @Test("Workspace roots coexist with one-shot trusted-wake input")
    func workspaceRootsCoexistWithTrustedWakeRestart() async throws {
        let sensitivePath = "/Users/operator/SecretProject"
        let provider = FakeWorkspaceRootGrantProvider(roots: [
            JarvisWorkspaceRootLaunchRoot(id: "root_safe", path: sensitivePath)
        ])
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth()),
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth()),
            ]),
            processLauncher: launcher,
            workspaceRootProvider: provider
        )
        await supervisor.start()

        try await supervisor.provisionTrustedWake(
            using: FakeBootstrapProvider(
                result: .success(Data("{\"public_key_x963_b64\":\"public-only\"}".utf8))
            )
        )

        let launch = try #require(launcher.launches.last)
        #expect(launch.arguments.contains("--startup-config-stdin"))
        #expect(!launch.arguments.joined().contains(sensitivePath))
        let input = try #require(launch.standardInput)
        let json = try #require(JSONSerialization.jsonObject(with: input) as? [String: Any])
        let roots = try #require(json["workspace_roots"] as? [[String: String]])
        #expect(roots == [["id": "root_safe", "path": sensitivePath]])
        let trustedWake = try #require(json["trusted_wake"] as? [String: Any])
        #expect(trustedWake["kind"] as? String == "bootstrap")
    }

    @Test("Launch failure releases acquired workspace access")
    func launchFailureReleasesWorkspaceAccess() async {
        let provider = FakeWorkspaceRootGrantProvider(roots: [
            JarvisWorkspaceRootLaunchRoot(id: "root_safe", path: "/private/tmp/project")
        ])
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil
            ),
            client: FakeCoreClient(healthResults: [.failure(URLError(.cannotConnectToHost))]),
            processLauncher: FailingProcessLauncher(),
            workspaceRootProvider: provider
        )

        await supervisor.start()

        #expect(provider.released)
        if case .degraded = supervisor.mode {
            // Expected.
        } else {
            Issue.record("Expected a launch failure to degrade the supervisor")
        }
    }

    @Test("Unexpected child exit releases workspace access and degrades supervision")
    func unexpectedChildExitReleasesWorkspaceAccess() async throws {
        let provider = FakeWorkspaceRootGrantProvider(roots: [
            JarvisWorkspaceRootLaunchRoot(id: "root_safe", path: "/private/tmp/project")
        ])
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil,
                healthPollIntervalNanoseconds: 100_000
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth())
            ]),
            processLauncher: launcher,
            workspaceRootProvider: provider
        )
        await supervisor.start()
        #expect(supervisor.isSupervisingCoreProcess)

        let process = try #require(launcher.processes.first)
        process.simulateUnexpectedExit()
        for _ in 0 ..< 100 where !provider.released {
            try await Task.sleep(nanoseconds: 100_000)
        }

        #expect(provider.released)
        #expect(!supervisor.isSupervisingCoreProcess)
        if case let .degraded(reason) = supervisor.mode {
            #expect(reason.contains("exited unexpectedly"))
        } else {
            Issue.record("Expected unexpected exit to degrade the supervisor")
        }
    }

    @Test("Foundation launcher bounds a child that does not consume startup stdin")
    func foundationLauncherTimesOutBlockedStartupInput() async {
        let launcher = FoundationJarvisCoreProcessLauncher(
            // Swift Testing runs suites concurrently in CI. Keep enough separation
            // that scheduler jitter cannot let the synthetic writer beat the timer.
            standardInputWriteTimeoutNanoseconds: 50_000_000,
            standardInputWriter: { _, _ in
                Thread.sleep(forTimeInterval: 2)
                return true
            }
        )
        do {
            _ = try await launcher.launch(
                executableURL: URL(fileURLWithPath: "/bin/sleep"),
                arguments: ["30"],
                environment: ProcessInfo.processInfo.environment,
                standardInput: Data(repeating: 0x5a, count: jarvisWorkspaceRootStartupEnvelopeMaximumBytes)
            )
            Issue.record("Expected startup input timeout")
        } catch JarvisCoreSupervisorError.startupConfigurationWriteTimedOut {
            // Expected.
        } catch {
            Issue.record("Unexpected launcher error: \(error)")
        }
    }

    @Test("Foundation launcher force-reaps a child after startup input failure")
    func foundationLauncherReapsFailedStartupInput() async {
        let launcher = FoundationJarvisCoreProcessLauncher(
            standardInputWriteTimeoutNanoseconds: 2_000_000_000,
            standardInputWriter: { _, _ in false }
        )
        let startedAt = Date()
        do {
            _ = try await launcher.launch(
                executableURL: URL(fileURLWithPath: "/bin/sleep"),
                arguments: ["30"],
                environment: ProcessInfo.processInfo.environment,
                standardInput: Data("bounded".utf8)
            )
            Issue.record("Expected startup input failure")
        } catch JarvisCoreSupervisorError.startupConfigurationWriteFailed {
            #expect(Date().timeIntervalSince(startedAt) < 1)
        } catch {
            Issue.record("Unexpected launcher error: \(error)")
        }
    }
}

@MainActor
@Suite("IPC bearer authorization", .serialized)
struct IPCBearerAuthorizationTests {
    @Test("CLI handoff environment requires an exact opt-in and an absolute override")
    func cliHandoffEnvironmentContract() {
        let enableKey = JarvisIPCCLIHandoffConfiguration.enableEnvironmentKey
        let fileKey = JarvisIPCCLIHandoffConfiguration.fileEnvironmentKey
        let file = URL(fileURLWithPath: "/tmp/jarvis-explicit-ipc-handoff.json")

        #expect(JarvisIPCCLIHandoffConfiguration.fromEnvironment([:]) == .disabled)
        let disabledWithAbsoluteOverride = JarvisIPCCLIHandoffConfiguration.fromEnvironment([
            enableKey: "false",
            fileKey: file.path
        ])
        #expect(!disabledWithAbsoluteOverride.isEnabled)
        #expect(disabledWithAbsoluteOverride.fileURL == nil)
        #expect(JarvisIPCCLIHandoffConfiguration.fromEnvironment([
            enableKey: "true",
            fileKey: "relative.json"
        ]) == .disabled)
        #expect(JarvisIPCCLIHandoffConfiguration.fromEnvironment([
            enableKey: "true",
            fileKey: file.path
        ]) == .enabled(fileURL: file))
    }

    @Test("Disabled handoff removes an absolute stale environment override without recreating it")
    func disabledHandoffCleansAbsoluteStaleOverride() throws {
        let directory = try temporaryDirectory(name: "jarvis-disabled-ipc-handoff")
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appending(path: "stale-ipc-session-auth.json")
        try Data("stale legacy bearer".utf8).write(to: file)
        let configuration = JarvisIPCCLIHandoffConfiguration.fromEnvironment([
            JarvisIPCCLIHandoffConfiguration.fileEnvironmentKey: file.path
        ])

        #expect(!configuration.isEnabled)
        let authorization = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            cliHandoffConfiguration: configuration,
            randomBytes: DeterministicAuthRandom().bytes
        )
        #expect(!FileManager.default.fileExists(atPath: file.path))
        _ = try authorization.rotateForLaunch()
        #expect(!FileManager.default.fileExists(atPath: file.path))
    }

    @Test("Disabled handoff stale cleanup never recursively removes a directory override")
    func disabledHandoffPreservesDirectoryOverride() throws {
        let directory = try temporaryDirectory(name: "jarvis-disabled-ipc-directory")
        defer { try? FileManager.default.removeItem(at: directory) }
        let override = directory.appending(path: "must-remain", directoryHint: .isDirectory)
        let sentinel = override.appending(path: "sentinel.txt")
        try FileManager.default.createDirectory(at: override, withIntermediateDirectories: false)
        try Data("preserve directory contents".utf8).write(to: sentinel)
        let configuration = JarvisIPCCLIHandoffConfiguration.fromEnvironment([
            JarvisIPCCLIHandoffConfiguration.fileEnvironmentKey: override.path
        ])

        let authorization = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            cliHandoffConfiguration: configuration,
            randomBytes: DeterministicAuthRandom().bytes
        )
        #expect(FileManager.default.fileExists(atPath: sentinel.path))
        let launchValue = try authorization.rotateForLaunch()
        let launch = try #require(launchValue)
        #expect(FileManager.default.fileExists(atPath: sentinel.path))
        authorization.clear(generation: launch.generation)
        #expect(FileManager.default.fileExists(atPath: sentinel.path))
    }

    @Test("Disabled handoff stale cleanup never removes a FIFO override")
    func disabledHandoffPreservesFIFOOverride() throws {
        let directory = try temporaryDirectory(name: "jarvis-disabled-ipc-fifo")
        defer { try? FileManager.default.removeItem(at: directory) }
        let fifo = directory.appending(path: "must-remain.fifo")
        #expect(Darwin.mkfifo(fifo.path, mode_t(0o600)) == 0)
        let configuration = JarvisIPCCLIHandoffConfiguration.fromEnvironment([
            JarvisIPCCLIHandoffConfiguration.fileEnvironmentKey: fifo.path
        ])

        let authorization = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            cliHandoffConfiguration: configuration,
            randomBytes: DeterministicAuthRandom().bytes
        )
        var metadata = stat()
        #expect(Darwin.lstat(fifo.path, &metadata) == 0)
        #expect(metadata.st_mode & S_IFMT == S_IFIFO)
        _ = try authorization.rotateForLaunch()
        #expect(Darwin.lstat(fifo.path, &metadata) == 0)
        #expect(metadata.st_mode & S_IFMT == S_IFIFO)
    }

    @Test("Disabled handoff may unlink a symlink leaf but never deletes its target")
    func disabledHandoffSymlinkCleanupPreservesTarget() throws {
        let directory = try temporaryDirectory(name: "jarvis-disabled-ipc-symlink")
        defer { try? FileManager.default.removeItem(at: directory) }
        let target = directory.appending(path: "target.txt")
        let symlink = directory.appending(path: "stale-ipc-session-auth.json")
        try Data("target must remain".utf8).write(to: target)
        try FileManager.default.createSymbolicLink(at: symlink, withDestinationURL: target)
        let configuration = JarvisIPCCLIHandoffConfiguration.fromEnvironment([
            JarvisIPCCLIHandoffConfiguration.fileEnvironmentKey: symlink.path
        ])

        _ = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            cliHandoffConfiguration: configuration,
            randomBytes: DeterministicAuthRandom().bytes
        )
        #expect(!FileManager.default.fileExists(atPath: symlink.path))
        #expect(try Data(contentsOf: target) == Data("target must remain".utf8))
    }

    @Test("Default supervised launch tokens rotate in memory and remove stale handoff files")
    func inMemoryTokenLifecycleRemovesStaleHandoff() throws {
        let directory = try temporaryDirectory(name: "jarvis-ipc-auth")
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appending(path: "ipc-session-auth.json")
        try Data("stale legacy bearer".utf8).write(to: file)
        let random = DeterministicAuthRandom()
        let authorization = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            tokenFileURL: file,
            randomBytes: random.bytes
        )

        let firstValue = try authorization.rotateForLaunch()
        let first = try #require(firstValue)
        let secondValue = try authorization.rotateForLaunch()
        let second = try #require(secondValue)

        #expect(first.token.count == 43)
        #expect(first.token.range(of: #"^[A-Za-z0-9_-]{43}$"#, options: .regularExpression) != nil)
        #expect(first.token != second.token)
        #expect(second.generation == first.generation + 1)
        #expect(authorization.cliHandoffConfiguration == .disabled)
        #expect(!FileManager.default.fileExists(atPath: file.path))

        authorization.clear(generation: first.generation)
        #expect(try authorization.authorizationHeader() == second.headerValue)
        authorization.clear(generation: second.generation)
        #expect(authorization.activeGeneration == nil)
        #expect(!FileManager.default.fileExists(atPath: file.path))
        #expect(throws: JarvisIPCAuthorizationError.credentialUnavailable) {
            _ = try authorization.authorizationHeader()
        }
    }

    @Test("Explicit CLI handoff remains versioned, owner-only, and generation-bound")
    func explicitCLIHandoffLifecycleAndPermissions() throws {
        let directory = try temporaryDirectory(name: "jarvis-ipc-cli-handoff")
        defer { try? FileManager.default.removeItem(at: directory) }
        let file = directory.appending(path: "ipc-session-auth.json")
        let authorization = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            cliHandoffConfiguration: .enabled(fileURL: file),
            randomBytes: DeterministicAuthRandom().bytes
        )

        let firstValue = try authorization.rotateForLaunch()
        let first = try #require(firstValue)
        let secondValue = try authorization.rotateForLaunch()
        let second = try #require(secondValue)

        #expect(authorization.cliHandoffConfiguration == .enabled(fileURL: file))
        #expect(first.token != second.token)
        #expect(second.generation == first.generation + 1)
        let json = try #require(JSONSerialization.jsonObject(with: Data(contentsOf: file)) as? [String: Any])
        #expect(json["version"] as? Int == 1)
        #expect(json["scheme"] as? String == "bearer")
        #expect(json["token"] as? String == second.token)
        #expect((json["generation"] as? NSNumber)?.uint64Value == second.generation)
        let fileMode = try #require(FileManager.default.attributesOfItem(atPath: file.path)[.posixPermissions] as? NSNumber)
        let directoryMode = try #require(FileManager.default.attributesOfItem(atPath: directory.path)[.posixPermissions] as? NSNumber)
        #expect(fileMode.intValue & 0o777 == 0o600)
        #expect(directoryMode.intValue & 0o777 == 0o700)

        authorization.clear(generation: first.generation)
        #expect(try authorization.authorizationHeader() == second.headerValue)
        #expect(FileManager.default.fileExists(atPath: file.path))
        authorization.clear(generation: second.generation)
        #expect(!FileManager.default.fileExists(atPath: file.path))
        #expect(throws: JarvisIPCAuthorizationError.credentialUnavailable) {
            _ = try authorization.authorizationHeader()
        }
    }

    @Test("Token-file write failure blocks only an explicitly enabled CLI handoff")
    func handoffWriteFailureIsOptInOnly() throws {
        let directory = try temporaryDirectory(name: "jarvis-ipc-handoff-failure")
        defer { try? FileManager.default.removeItem(at: directory) }
        let blockedParent = directory.appending(path: "not-a-directory")
        try Data("regular file".utf8).write(to: blockedParent)
        let unavailableFile = blockedParent.appending(path: "ipc-session-auth.json")

        let inMemory = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            tokenFileURL: unavailableFile,
            randomBytes: DeterministicAuthRandom().bytes
        )
        let launchValue = try inMemory.rotateForLaunch()
        let launch = try #require(launchValue)
        #expect(try inMemory.authorizationHeader() == launch.headerValue)
        #expect(!FileManager.default.fileExists(atPath: unavailableFile.path))

        let handoff = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            cliHandoffConfiguration: .enabled(fileURL: unavailableFile),
            randomBytes: DeterministicAuthRandom().bytes
        )
        #expect(throws: JarvisIPCAuthorizationError.tokenFileUnavailable) {
            _ = try handoff.rotateForLaunch()
        }
        #expect(handoff.activeGeneration == nil)
        #expect(throws: JarvisIPCAuthorizationError.credentialUnavailable) {
            _ = try handoff.authorizationHeader()
        }
    }

    @Test("Client requires a supervised token and sends it on JSON and SSE requests")
    func clientHeadersAndMissingToken() async throws {
        let directory = try temporaryDirectory(name: "jarvis-ipc-client-auth")
        defer { try? FileManager.default.removeItem(at: directory) }
        let authorization = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            tokenFileURL: directory.appending(path: "auth.json"),
            randomBytes: DeterministicAuthRandom().bytes
        )
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [IPCURLProtocol.self]
        let client = JarvisIPCClient(
            endpoint: JarvisEndpoint(baseURL: URL(string: "http://127.0.0.1:7787")!),
            session: URLSession(configuration: configuration),
            authorization: authorization
        )
        var headers: [String?] = []
        IPCURLProtocol.handler = { request in
            headers.append(request.value(forHTTPHeaderField: "Authorization"))
            let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
            if request.url?.path == "/health" {
                return (response, Data(#"{"status":"ok","version":"0.1.0","emergency_paused":false,"scheduler_jobs":0,"command_runtime":"test"}"#.utf8))
            }
            return (response, Data())
        }
        defer { IPCURLProtocol.handler = nil }

        do {
            _ = try await client.health()
            Issue.record("Expected a missing-token failure")
        } catch JarvisIPCAuthorizationError.credentialUnavailable {
            // Expected before any URL loading.
        }
        #expect(headers.isEmpty)

        let grantValue = try authorization.rotateForLaunch()
        let grant = try #require(grantValue)
        _ = try await client.health()
        _ = try await client.activityEvents(maxEvents: 1, intervalMilliseconds: 100)
        #expect(headers == [grant.headerValue, grant.headerValue])

        let externalClient = JarvisIPCClient(
            endpoint: JarvisEndpoint(baseURL: URL(string: "http://192.0.2.10:7787")!),
            session: URLSession(configuration: configuration),
            authorization: authorization
        )
        do {
            _ = try await externalClient.health()
            Issue.record("Expected managed authorization to reject a non-loopback endpoint")
        } catch JarvisIPCAuthorizationError.nonLoopbackEndpoint {
            // Expected before URL loading or bearer exposure.
        }
        #expect(headers == [grant.headerValue, grant.headerValue])
    }

    @Test("Explicit unauthenticated compatibility client omits the header")
    func explicitUnauthenticatedClientOmitsHeader() async throws {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [IPCURLProtocol.self]
        let client = JarvisIPCClient(session: URLSession(configuration: configuration))
        var header: String?
        IPCURLProtocol.handler = { request in
            header = request.value(forHTTPHeaderField: "Authorization")
            return (
                HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!,
                Data(#"{"status":"ok","version":"0.1.0","emergency_paused":false,"scheduler_jobs":0,"command_runtime":"test"}"#.utf8)
            )
        }
        defer { IPCURLProtocol.handler = nil }

        _ = try await client.health()
        #expect(header == nil)
    }

    @Test("Supervisor rotates bearer auth through trusted-wake restart without leaking it")
    func supervisorEnvelopeRotationAndTrustedWake() async throws {
        let directory = try temporaryDirectory(name: "jarvis-supervisor-auth")
        defer { try? FileManager.default.removeItem(at: directory) }
        let authorization = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            tokenFileURL: directory.appending(path: "auth.json"),
            randomBytes: DeterministicAuthRandom().bytes
        )
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"), databaseURL: nil
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth()),
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth())
            ]),
            processLauncher: launcher,
            workspaceRootProvider: FakeWorkspaceRootGrantProvider(roots: []),
            ipcAuthorization: authorization
        )
        await supervisor.start(environmentOverrides: [
            "JARVIS_IPC_TOKEN_FILE": "/tmp/must-not-reach-server",
            "JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF": "true",
            "JARVIS_MAC_IPC_AUTH_FILE": "/tmp/must-not-reach-server"
        ])
        let first = try ipcAuth(from: #require(launcher.launches.first?.standardInput))
        #expect(!launcher.launches[0].arguments.joined().contains(first.token))
        #expect(!launcher.launches[0].environment.description.contains(first.token))
        #expect(launcher.launches[0].environment["JARVIS_IPC_TOKEN_FILE"] == nil)
        #expect(launcher.launches[0].environment["JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF"] == nil)
        #expect(launcher.launches[0].environment["JARVIS_MAC_IPC_AUTH_FILE"] == nil)
        #expect(authorization.activeGeneration == first.generation)
        #expect(!FileManager.default.fileExists(atPath: authorization.tokenFileURL.path))
        #expect(!supervisor.smokeSnapshot.summary.contains(first.token))

        try await supervisor.provisionTrustedWake(
            using: FakeBootstrapProvider(result: .success(Data(#"{"public_key_x963_b64":"public-only"}"#.utf8)))
        )
        let lastInput = try #require(launcher.launches.last?.standardInput)
        let second = try ipcAuth(from: lastInput)
        let envelope = try #require(JSONSerialization.jsonObject(with: lastInput) as? [String: Any])
        #expect(second.token != first.token)
        #expect(second.generation == first.generation + 1)
        #expect(authorization.activeGeneration == second.generation)
        #expect(!FileManager.default.fileExists(atPath: authorization.tokenFileURL.path))
        #expect(launcher.launches.last?.environment["JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF"] == nil)
        #expect(launcher.launches.last?.environment["JARVIS_MAC_IPC_AUTH_FILE"] == nil)
        #expect((envelope["trusted_wake"] as? [String: Any])?["kind"] as? String == "bootstrap")
        #expect(await supervisor.stop())
        #expect(authorization.activeGeneration == nil)
        #expect(!FileManager.default.fileExists(atPath: authorization.tokenFileURL.path))
    }

    @Test("Launch failure clears bearer state and an unauthenticated legacy core cannot pass preflight")
    func launchFailureAndLegacyCoreFailClosed() async throws {
        let directory = try temporaryDirectory(name: "jarvis-auth-failure")
        defer { try? FileManager.default.removeItem(at: directory) }
        let authorization = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            tokenFileURL: directory.appending(path: "auth.json"),
            randomBytes: DeterministicAuthRandom().bytes
        )
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [IPCURLProtocol.self]
        var networkRequests = 0
        IPCURLProtocol.handler = { request in
            networkRequests += 1
            return (
                HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!,
                Data(#"{"status":"ok","version":"0.1.0","emergency_paused":false,"scheduler_jobs":0,"command_runtime":"legacy"}"#.utf8)
            )
        }
        defer { IPCURLProtocol.handler = nil }
        let client = JarvisIPCClient(session: URLSession(configuration: configuration), authorization: authorization)
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"), databaseURL: nil
            ),
            client: client,
            processLauncher: FailingProcessLauncher(),
            workspaceRootProvider: FakeWorkspaceRootGrantProvider(roots: []),
            ipcAuthorization: authorization
        )

        await supervisor.start()

        #expect(networkRequests == 0)
        #expect(authorization.activeGeneration == nil)
        #expect(!FileManager.default.fileExists(atPath: authorization.tokenFileURL.path))
        #expect(!supervisor.isAvailable)

        let externalAuthorization = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            tokenFileURL: directory.appending(path: "external-auth.json"),
            randomBytes: DeterministicAuthRandom().bytes
        )
        let externalLauncher = FakeProcessLauncher()
        let externalSupervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                endpoint: JarvisEndpoint(baseURL: URL(string: "http://192.0.2.10:7787")!),
                bindAddress: "0.0.0.0:7787",
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: nil
            ),
            client: FakeCoreClient(healthResults: [.failure(URLError(.cannotConnectToHost))]),
            processLauncher: externalLauncher,
            workspaceRootProvider: FakeWorkspaceRootGrantProvider(roots: []),
            ipcAuthorization: externalAuthorization
        )
        await externalSupervisor.start()
        #expect(externalLauncher.launches.isEmpty)
        #expect(externalAuthorization.activeGeneration == nil)
        #expect(!FileManager.default.fileExists(atPath: externalAuthorization.tokenFileURL.path))
    }

    @Test("App-style transport selection defaults to UDS and exact handoff selects TCP")
    func appStyleTransportSelectionAndSupervisorEnvelope() async throws {
        let directory = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let disabled = JarvisIPCCLIHandoffConfiguration.fromEnvironment([:])
        let enabled = JarvisIPCCLIHandoffConfiguration.fromEnvironment([
            JarvisIPCCLIHandoffConfiguration.enableEnvironmentKey: "true",
            JarvisIPCCLIHandoffConfiguration.fileEnvironmentKey:
                directory.appending(path: "ipc-session-auth.json").path
        ])
        #expect(!disabled.isEnabled)
        #expect(enabled.isEnabled)

        let udsAuthorization = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            tokenFileURL: directory.appending(path: "unused-uds-auth.json"),
            cliHandoffConfiguration: disabled,
            transportMode: disabled.isEnabled ? .loopbackTCP : .unixSocket,
            socketDirectoryPath: directory.appending(path: "run").path,
            randomBytes: DeterministicAuthRandom().bytes,
            socketIdentifier: { "udsselection" }
        )
        let udsLauncher = FakeProcessLauncher()
        let udsSupervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"), databaseURL: nil
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth())
            ]),
            processLauncher: udsLauncher,
            workspaceRootProvider: FakeWorkspaceRootGrantProvider(roots: []),
            ipcAuthorization: udsAuthorization,
            peerIdentityPolicyProvider: FakePeerIdentityPolicyProvider()
        )

        await udsSupervisor.start(environmentOverrides: [
            JarvisCoreSupervisor.releaseSmokeEnvironmentKey: "true"
        ])

        let udsLaunch = try #require(udsLauncher.launches.first)
        #expect(!udsLaunch.arguments.contains("--bind"))
        let udsInput = try #require(udsLaunch.standardInput)
        let udsEnvelope = try #require(
            JSONSerialization.jsonObject(with: udsInput) as? [String: Any]
        )
        let udsTransport = try #require(udsEnvelope["ipc_transport"] as? [String: String])
        #expect(udsTransport["kind"] == JarvisIPCPeerIdentityPolicy.kind)
        #expect(udsTransport["socket_path"]?.hasSuffix("/run/core-udsselection.sock") == true)
        #expect(udsTransport["peer_identity_profile"] == JarvisIPCPeerIdentityProfile.adhocExact.rawValue)
        #expect(udsTransport["peer_code_requirement"] == samplePeerIdentityPolicy().peerCodeRequirement)
        #expect(udsEnvelope["ipc_auth"] != nil)
        #expect(udsLaunch.environment["JARVIS_IPC_TOKEN_FILE"] == nil)
        #expect(udsLaunch.environment[JarvisCoreSupervisor.releaseSmokeEnvironmentKey] == nil)

        let tcpAuthorization = JarvisIPCSessionAuthorization(
            mode: .appSupervised,
            cliHandoffConfiguration: enabled,
            transportMode: enabled.isEnabled ? .loopbackTCP : .unixSocket,
            randomBytes: DeterministicAuthRandom().bytes
        )
        let tcpLauncher = FakeProcessLauncher()
        let tcpSupervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"), databaseURL: nil
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth())
            ]),
            processLauncher: tcpLauncher,
            workspaceRootProvider: FakeWorkspaceRootGrantProvider(roots: []),
            ipcAuthorization: tcpAuthorization
        )

        await tcpSupervisor.start()

        let tcpLaunch = try #require(tcpLauncher.launches.first)
        #expect(tcpLaunch.arguments.contains("--bind"))
        let tcpInput = try #require(tcpLaunch.standardInput)
        let tcpEnvelope = try #require(
            JSONSerialization.jsonObject(with: tcpInput) as? [String: Any]
        )
        #expect(tcpEnvelope["ipc_auth"] != nil)
        #expect(tcpEnvelope["ipc_transport"] == nil)
        #expect(FileManager.default.fileExists(atPath: try #require(enabled.fileURL).path))
        #expect(await tcpSupervisor.stop())
        #expect(!FileManager.default.fileExists(atPath: try #require(enabled.fileURL).path))
    }

    @Test("UDS identity policy is generation-bound and missing policy fails closed")
    func unixSocketPeerIdentityPolicyLifecycle() throws {
        let directory = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let authorization = makeUnixSocketAuthorization(
            socketDirectoryURL: directory.appending(path: "run"),
            tokenFileRoot: directory,
            socketIdentifier: { "policy" }
        )

        #expect(throws: JarvisIPCAuthorizationError.peerIdentityUnavailable) {
            _ = try authorization.rotateForLaunch()
        }
        let policy = samplePeerIdentityPolicy()
        let launch = try #require(
            try authorization.rotateForLaunch(peerIdentityPolicy: policy)
        )
        #expect(try authorization.activeUnixPeerIdentityPolicy() == policy)
        authorization.clear(generation: launch.generation)
        #expect(throws: JarvisIPCAuthorizationError.peerIdentityUnavailable) {
            _ = try authorization.activeUnixPeerIdentityPolicy()
        }
    }

    @Test("Supervisor degrades before launch when UDS identity policy is unavailable")
    func supervisorPeerIdentityPolicyFailure() async throws {
        let directory = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let authorization = makeUnixSocketAuthorization(
            socketDirectoryURL: directory.appending(path: "run"),
            tokenFileRoot: directory,
            socketIdentifier: { "policy-failure" }
        )
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"), databaseURL: nil
            ),
            client: FakeCoreClient(healthResults: [.failure(URLError(.cannotConnectToHost))]),
            processLauncher: launcher,
            workspaceRootProvider: FakeWorkspaceRootGrantProvider(roots: []),
            ipcAuthorization: authorization,
            peerIdentityPolicyProvider: FakePeerIdentityPolicyProvider(shouldFail: true)
        )

        await supervisor.start()

        #expect(launcher.launches.isEmpty)
        #expect(authorization.activeGeneration == nil)
        #expect(!supervisor.isAvailable)
    }

    @Test("UDS run directory rejects symlink, file, and permissive preexisting state")
    func unixSocketDirectorySafety() throws {
        let root = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let target = root.appending(path: "target", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: target, withIntermediateDirectories: false)

        let symlink = root.appending(path: "symlink-run")
        try FileManager.default.createSymbolicLink(at: symlink, withDestinationURL: target)
        try assertUnsafeUnixSocketDirectory(symlink, tokenFileRoot: root)

        let file = root.appending(path: "file-run")
        try Data("not a directory".utf8).write(to: file)
        try assertUnsafeUnixSocketDirectory(file, tokenFileRoot: root)

        let permissive = root.appending(path: "permissive-run", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: permissive, withIntermediateDirectories: false)
        #expect(Darwin.chmod(permissive.path, mode_t(0o755)) == 0)
        try assertUnsafeUnixSocketDirectory(permissive, tokenFileRoot: root)
    }

    @Test("UDS path bounds and repeated leaf collisions fail closed")
    func unixSocketPathBoundsAndCollisions() throws {
        let root = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let run = root.appending(path: "run", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: run, withIntermediateDirectories: false)
        #expect(Darwin.chmod(run.path, mode_t(0o700)) == 0)
        let collision = run.appending(path: "core-collision.sock")
        try Data("must remain".utf8).write(to: collision)
        let collisionAuthorization = makeUnixSocketAuthorization(
            socketDirectoryURL: run,
            tokenFileRoot: root,
            socketIdentifier: { "collision" }
        )
        let collisionLaunchValue = try collisionAuthorization.rotateForLaunch(
            peerIdentityPolicy: samplePeerIdentityPolicy()
        )
        let collisionLaunch = try #require(collisionLaunchValue)
        #expect(throws: JarvisIPCAuthorizationError.unixSocketPathInvalid) {
            _ = try collisionAuthorization.prepareUnixSocketForLaunch(
                generation: collisionLaunch.generation
            )
        }
        #expect(try Data(contentsOf: collision) == Data("must remain".utf8))

        let longParent = root.appending(
            path: String(repeating: "p", count: 80), directoryHint: .isDirectory
        )
        let longRun = longParent.appending(path: "run", directoryHint: .isDirectory)
        let longAuthorization = makeUnixSocketAuthorization(
            socketDirectoryURL: longRun,
            tokenFileRoot: root,
            socketIdentifier: { "bounded" }
        )
        let longLaunchValue = try longAuthorization.rotateForLaunch(
            peerIdentityPolicy: samplePeerIdentityPolicy()
        )
        let longLaunch = try #require(longLaunchValue)
        #expect(throws: JarvisIPCAuthorizationError.unixSocketPathInvalid) {
            _ = try longAuthorization.prepareUnixSocketForLaunch(
                generation: longLaunch.generation
            )
        }
    }

    @Test("UDS cleanup removes only the captured socket identity")
    func unixSocketIdentityBoundCleanup() throws {
        let root = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let authorization = makeUnixSocketAuthorization(
            socketDirectoryURL: root.appending(path: "run"),
            tokenFileRoot: root,
            socketIdentifier: { "identity" }
        )
        let launchValue = try authorization.rotateForLaunch(
            peerIdentityPolicy: samplePeerIdentityPolicy()
        )
        let launch = try #require(launchValue)
        let descriptorValue = try authorization.prepareUnixSocketForLaunch(
            generation: launch.generation
        )
        let descriptor = try #require(descriptorValue)
        let listener = try bindUnixSocketForTest(at: descriptor.socketURL)
        defer { Darwin.close(listener) }
        #expect(Darwin.chmod(descriptor.socketURL.path, mode_t(0o600)) == 0)
        try authorization.captureActiveUnixSocketIdentity(generation: launch.generation)

        #expect(Darwin.unlink(descriptor.socketURL.path) == 0)
        try Data("replacement must remain".utf8).write(to: descriptor.socketURL)
        authorization.clear(generation: launch.generation)

        #expect(try Data(contentsOf: descriptor.socketURL) == Data("replacement must remain".utf8))
    }
}

@Suite("Unix IPC transport", .serialized)
struct UnixIPCTransportTests {
    @Test("Peer identity validation completes before the first frame byte")
    func peerIdentityPrecedesFraming() async throws {
        let root = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let verifier = ControlledUnixPeerIdentityVerifier()
        let prematureByte = LockedTestValue(false)
        let server = try UnixSocketTestServer(
            socketURL: root.appending(path: "identity-order.sock"),
            handler: { descriptor in
                _ = verifier.started.wait(timeout: .now() + 2)
                var byte: UInt8 = 0
                let received = Darwin.recv(descriptor, &byte, 1, MSG_DONTWAIT)
                prematureByte.set(received > 0)
                verifier.proceed.signal()
                guard (try? readUnixTestFrame(descriptor)) != nil else { return }
                try? writeUnixTestFrame(
                    descriptor,
                    Data(#"{"version":1,"status":200,"content_type":null,"body_base64":""}"#.utf8)
                )
            }
        )
        defer { withExtendedLifetime(server) {} }
        let transport = DarwinJarvisUnixSocketTransport(
            timeoutSeconds: 2,
            peerIdentityPolicy: { samplePeerIdentityPolicy() },
            peerIdentityVerifier: verifier
        )

        _ = try await transport.send(
            JarvisIPCTransportRequest(
                method: "GET", path: "/health", authorization: "Bearer token"
            ),
            to: server.socketURL
        )

        #expect(!prematureByte.value)
    }

    @Test("Missing or rejected peer identity closes without sending a frame")
    func peerIdentityFailureSendsNoFrame() async throws {
        let root = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let receivedBytes = LockedTestValue<Int?>(nil)
        let server = try UnixSocketTestServer(
            socketURL: root.appending(path: "identity-reject.sock"),
            handler: { descriptor in
                var byte: UInt8 = 0
                receivedBytes.set(Darwin.read(descriptor, &byte, 1))
            }
        )
        defer { withExtendedLifetime(server) {} }
        let request = JarvisIPCTransportRequest(
            method: "GET", path: "/health", authorization: "Bearer token"
        )
        let rejected = DarwinJarvisUnixSocketTransport(
            timeoutSeconds: 2,
            peerIdentityPolicy: { samplePeerIdentityPolicy() },
            peerIdentityVerifier: RejectingUnixPeerIdentityVerifier()
        )

        await #expect(throws: JarvisUnixSocketTransportError.peerIdentityUnavailable) {
            _ = try await rejected.send(request, to: server.socketURL)
        }
        try await waitForUnixTestServerAccept(server)
        let deadline = ContinuousClock.now.advanced(by: .seconds(2))
        while receivedBytes.value == nil, ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(receivedBytes.value == 0)
    }

    @Test("Wire request is strict and padded while a valid response decodes")
    func strictWireRoundTrip() async throws {
        let root = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let captured = LockedTestValue<Data?>(nil)
        let server = try UnixSocketTestServer(
            socketURL: root.appending(path: "wire.sock"),
            handler: { descriptor in
                captured.set(try? readUnixTestFrame(descriptor))
                let response = Data(
                    #"{"version":1,"status":201,"content_type":"application/json","body_base64":"b2s="}"#.utf8
                )
                try? writeUnixTestFrame(descriptor, response)
            }
        )
        defer { withExtendedLifetime(server) {} }
        let response = try await testUnixSocketTransport(timeoutSeconds: 2).send(
            JarvisIPCTransportRequest(
                method: "POST",
                path: "/commands?dry_run=true",
                authorization: "Bearer test-token",
                accept: "application/json",
                contentType: "application/json",
                body: Data("x".utf8)
            ),
            to: server.socketURL
        )

        #expect(response.status == 201)
        #expect(response.contentType == "application/json")
        #expect(response.body == Data("ok".utf8))
        let requestData = try #require(captured.value)
        let request = try #require(
            JSONSerialization.jsonObject(with: requestData) as? [String: Any]
        )
        #expect(Set(request.keys) == Set([
            "version", "method", "path", "authorization", "accept", "content_type", "body_base64"
        ]))
        #expect(request["version"] as? Int == 1)
        #expect(request["method"] as? String == "POST")
        #expect(request["body_base64"] as? String == "eA==")
    }

    @Test("Malformed, oversized, and early-EOF responses fail closed")
    func invalidResponsesFailClosed() async throws {
        let root = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let request = JarvisIPCTransportRequest(
            method: "GET", path: "/health", authorization: "Bearer test-token"
        )

        let malformed = try UnixSocketTestServer(
            socketURL: root.appending(path: "malformed.sock"),
            handler: { descriptor in
                _ = try? readUnixTestFrame(descriptor)
                try? writeUnixTestFrame(
                    descriptor,
                    Data(#"{"version":1,"status":200,"content_type":null,"body_base64":"","extra":true}"#.utf8)
                )
            }
        )
        defer { withExtendedLifetime(malformed) {} }
        await #expect(throws: JarvisUnixSocketTransportError.invalidResponse) {
            _ = try await testUnixSocketTransport(timeoutSeconds: 2)
                .send(request, to: malformed.socketURL)
        }

        let oversized = try UnixSocketTestServer(
            socketURL: root.appending(path: "oversized.sock"),
            handler: { descriptor in
                _ = try? readUnixTestFrame(descriptor)
                var length = UInt32(
                    DarwinJarvisUnixSocketTransport.maximumResponseFrameBytes + 1
                ).bigEndian
                withUnsafeBytes(of: &length) { bytes in
                    _ = Darwin.write(descriptor, bytes.baseAddress, bytes.count)
                }
            }
        )
        defer { withExtendedLifetime(oversized) {} }
        await #expect(throws: JarvisUnixSocketTransportError.frameTooLarge) {
            _ = try await testUnixSocketTransport(timeoutSeconds: 2)
                .send(request, to: oversized.socketURL)
        }

        let eof = try UnixSocketTestServer(
            socketURL: root.appending(path: "eof.sock"),
            handler: { descriptor in _ = try? readUnixTestFrame(descriptor) }
        )
        defer { withExtendedLifetime(eof) {} }
        await #expect(throws: JarvisUnixSocketTransportError.readFailed) {
            _ = try await testUnixSocketTransport(timeoutSeconds: 2)
                .send(request, to: eof.socketURL)
        }
    }

    @Test("Request caps reject oversized bodies before connecting")
    func requestCapsFailBeforeConnect() async {
        #expect(DarwinJarvisUnixSocketTransport.maximumRequestFrameBytes == 2 * 1024 * 1024)
        #expect(DarwinJarvisUnixSocketTransport.maximumRequestBodyBytes == 1024 * 1024)
        #expect(DarwinJarvisUnixSocketTransport.maximumResponseFrameBytes == 12 * 1024 * 1024)
        #expect(DarwinJarvisUnixSocketTransport.maximumResponseBodyBytes == 8 * 1024 * 1024)
        let body = Data(
            repeating: 0,
            count: DarwinJarvisUnixSocketTransport.maximumRequestBodyBytes + 1
        )
        await #expect(throws: JarvisUnixSocketTransportError.invalidRequest) {
            _ = try await testUnixSocketTransport(timeoutSeconds: 2).send(
                JarvisIPCTransportRequest(
                    method: "POST", path: "/commands", authorization: "Bearer token", body: body
                ),
                to: URL(fileURLWithPath: "/tmp/does-not-connect.sock")
            )
        }
    }

    @Test("Peer EUID comparison rejects a mismatched local user")
    func peerUIDMismatchFailsClosed() throws {
        try DarwinJarvisUnixSocketTransport.validatePeerUID(42, currentEUID: 42)
        #expect(throws: JarvisUnixSocketTransportError.peerUIDMismatch) {
            try DarwinJarvisUnixSocketTransport.validatePeerUID(41, currentEUID: 42)
        }
    }

    @Test("Trickle responses cannot extend the hard end-to-end deadline")
    func trickleResponseHonorsHardDeadline() async throws {
        let root = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let response = Data(
            #"{"version":1,"status":200,"content_type":null,"body_base64":""}"#.utf8
        )
        let framedResponse: Data = {
            var responseLength = UInt32(response.count).bigEndian
            var frame = withUnsafeBytes(of: &responseLength) { Data($0) }
            frame.append(response)
            return frame
        }()
        let server = try UnixSocketTestServer(
            socketURL: root.appending(path: "trickle.sock"),
            handler: { descriptor in
                guard (try? readUnixTestFrame(descriptor)) != nil else { return }
                for byte in framedResponse {
                    var byte = byte
                    guard Darwin.write(descriptor, &byte, 1) == 1 else { return }
                    Darwin.usleep(150_000)
                }
            }
        )
        defer { withExtendedLifetime(server) {} }
        let clock = ContinuousClock()
        let started = clock.now
        await #expect(throws: JarvisUnixSocketTransportError.timedOut) {
            _ = try await testUnixSocketTransport(timeoutSeconds: 1).send(
                JarvisIPCTransportRequest(
                    method: "GET", path: "/health", authorization: "Bearer token"
                ),
                to: server.socketURL
            )
        }
        let elapsed = started.duration(to: clock.now)
        #expect(elapsed >= .milliseconds(700))
        #expect(elapsed < .seconds(2))
    }

    @Test("Cancellation closes a connected request without waiting for timeout")
    func cancellationClosesConnectedRequest() async throws {
        let root = try shortUnixTestDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let server = try UnixSocketTestServer(
            socketURL: root.appending(path: "cancel.sock"),
            handler: { descriptor in
                _ = try? readUnixTestFrame(descriptor)
                _ = DispatchSemaphore(value: 0).wait(timeout: .now() + 5)
            }
        )
        defer { withExtendedLifetime(server) {} }
        let task = Task {
            try await testUnixSocketTransport(timeoutSeconds: 30).send(
                JarvisIPCTransportRequest(
                    method: "GET", path: "/health", authorization: "Bearer token"
                ),
                to: server.socketURL
            )
        }
        try await waitForUnixTestServerAccept(server)
        task.cancel()
        await #expect(throws: JarvisUnixSocketTransportError.cancelled) {
            _ = try await task.value
        }
    }
}

@Suite("Trusted wake contracts")
struct TrustedWakeContractsTests {
    @Test("Counter recovers after Keychain loss from Rust durable high-water")
    func counterRecoversAfterKeychainLoss() throws {
        let counter = try nextTrustedWakeCounter(
            persisted: nil,
            epochMilliseconds: 12,
            durableHighWater: 41
        )
        #expect(counter == 42)
    }

    @Test("Counter remains monotonic when wall clock moves backward")
    func counterRecoversFromBackwardClock() throws {
        let counter = try nextTrustedWakeCounter(
            persisted: 90,
            epochMilliseconds: 12,
            durableHighWater: 80
        )
        #expect(counter == 91)
    }

    @Test("Counter exhaustion fails closed")
    func counterExhaustionFailsClosed() {
        do {
            _ = try nextTrustedWakeCounter(
                persisted: UInt64.max,
                epochMilliseconds: 12,
                durableHighWater: 80
            )
            Issue.record("Expected persisted counter exhaustion to fail closed")
        } catch TrustedWakeError.counterExhausted {
            // Expected.
        } catch {
            Issue.record("Unexpected persisted counter error: \(error)")
        }

        do {
            _ = try nextTrustedWakeCounter(
                persisted: 90,
                epochMilliseconds: 12,
                durableHighWater: UInt64.max
            )
            Issue.record("Expected durable high-water exhaustion to fail closed")
        } catch TrustedWakeError.counterExhausted {
            // Expected.
        } catch {
            Issue.record("Unexpected durable high-water error: \(error)")
        }
    }

    @Test("Key-control reconciliation converges after install, cancel, and expiry")
    func keyControlReconciliationDecisions() throws {
        let installed = try fakeTrustedWakeStatus(
            generation: 2,
            fingerprint: "new-fingerprint",
            pendingJSON: "null"
        )
        #expect(trustedWakeKeyReconcileDisposition(
            rule: installed.rule,
            pending: installed.pendingKeyControl,
            targetGeneration: 2,
            oldFingerprint: "old-fingerprint",
            newFingerprint: "new-fingerprint"
        ) == .promoteCandidate)
        let cancelled = try fakeTrustedWakeStatus(
            generation: 2,
            fingerprint: "old-fingerprint",
            pendingJSON: "null"
        )
        #expect(trustedWakeKeyReconcileDisposition(
            rule: cancelled.rule,
            pending: cancelled.pendingKeyControl,
            targetGeneration: 2,
            oldFingerprint: "old-fingerprint",
            newFingerprint: "new-fingerprint"
        ) == .clearConfirmedCancel)
        let pending = try fakeTrustedWakeStatus(
            generation: 2,
            fingerprint: "old-fingerprint",
            pendingJSON: fakePendingKeyControlJSON(expiresAt: "2026-07-13T12:05:00Z")
        )
        #expect(trustedWakeKeyReconcileDisposition(
            rule: pending.rule,
            pending: pending.pendingKeyControl,
            targetGeneration: 2,
            oldFingerprint: "old-fingerprint",
            newFingerprint: "new-fingerprint"
        ) == .wait)
        let parser = ISO8601DateFormatter()
        let boundary = parser.date(from: "2026-07-13T12:00:00Z")!
        #expect(trustedWakeGrantIsExpired("2026-07-13T12:00:00Z", now: boundary))
        #expect(!trustedWakeGrantHasMinimumValidity(
            "2026-07-13T08:00:00-04:00",
            minimumValiditySeconds: 0,
            now: boundary
        ))
        #expect(trustedWakeGrantHasMinimumValidity(
            "2026-07-13T12:00:00.500Z",
            minimumValiditySeconds: 0,
            now: boundary
        ))
        #expect(!trustedWakeGrantHasMinimumValidity(
            "2026-07-13T12:00:05Z",
            minimumValiditySeconds: 7,
            now: boundary
        ))
        #expect(!trustedWakeGrantHasMinimumValidity(
            "not-a-date",
            minimumValiditySeconds: 0,
            now: boundary
        ))
        #expect(trustedWakeGrantIsExpired("not-a-date"))
    }

    @Test("Expired resume preserves the healthy core")
    @MainActor
    func expiredResumeDoesNotRestartCore() async throws {
        let expired = try fakeTrustedWakeStatus(
            generation: 2,
            fingerprint: "old-fingerprint",
            pendingJSON: fakePendingKeyControlJSON(expiresAt: "2020-01-01T00:00:00Z")
        )
        let restarted = MutableFlag(false)
        let model = TrustedWakeModel(
            client: FakeCoreClient(trustedWakeStatusResults: [.success(expired)]),
            signer: FakeTrustedWakeSigner(),
            keyRing: RecordingTrustedWakeKeyRing(),
            installKeyControl: { restarted.value = true }
        )
        await model.resumeKeyControl()
        #expect(!restarted.value)
        #expect(model.errorMessage?.contains("expired") == true)
        #expect(model.errorMessage?.contains("did not stop the healthy core") == true)
    }

    @Test("Resume reconciles a completed install before requiring a pending grant")
    @MainActor
    func resumeReconcilesCompletedInstallFirst() async throws {
        let installed = try fakeTrustedWakeStatus(
            generation: 2,
            fingerprint: "new-fingerprint",
            pendingJSON: "null"
        )
        let restarted = MutableFlag(false)
        let model = TrustedWakeModel(
            client: FakeCoreClient(trustedWakeStatusResults: [.success(installed)]),
            signer: FakeTrustedWakeSigner(),
            keyRing: ReconciledTrustedWakeKeyRing(),
            installKeyControl: { restarted.value = true }
        )

        await model.resumeKeyControl()

        #expect(!restarted.value)
        #expect(model.status?.rule?.generation == 2)
        #expect(model.errorMessage == nil)
        #expect(model.keyControlMessage?.contains("journal reconciled") == true)
    }

    @Test("Durable prepare failure refreshes pending actions without retrying install")
    @MainActor
    func durablePrepareFailureExposesPendingActions() async throws {
        let active = try fakeTrustedWakeStatus(
            generation: 1,
            fingerprint: "old-fingerprint",
            pendingJSON: "null"
        )
        let pending = try fakeTrustedWakeStatus(
            generation: 2,
            fingerprint: "old-fingerprint",
            pendingJSON: fakePendingKeyControlJSON(expiresAt: "2099-07-13T12:05:00Z")
        )
        let client = FakeCoreClient(
            trustedWakeStatusResults: [.success(active), .success(pending)],
            trustedWakePrepareResult: .success(try fakeTrustedWakePrepareResponse())
        )
        let model = TrustedWakeModel(
            client: client,
            signer: FakeTrustedWakeSigner(),
            keyRing: RecordingTrustedWakeKeyRing(),
            installKeyControl: { throw JarvisCoreSupervisorError.trustedWakeRestartFailed }
        )

        await model.beginKeyControl(
            operation: .recover,
            confirmation: jarvisTrustedWakeRecoverConfirmation
        )

        #expect(model.status?.pendingKeyControl != nil)
        #expect(model.errorMessage?.contains("durable journal") == true)
        #expect(model.errorMessage?.contains("Resume or Cancel/reset") == true)
    }

    @Test("Refresh cannot delete a candidate between stage and journal persist")
    @MainActor
    func refreshDoesNotRaceKeyControlStage() async {
        let keyRing = ControlledTrustedWakeKeyRing()
        let model = TrustedWakeModel(
            client: FakeCoreClient(),
            signer: FakeTrustedWakeSigner(),
            keyRing: keyRing,
            installKeyControl: {}
        )
        let control = Task { @MainActor in
            await model.beginKeyControl(
                operation: .recover,
                confirmation: jarvisTrustedWakeRecoverConfirmation
            )
        }
        for _ in 0 ..< 100 where !keyRing.didStageStart {
            try? await Task.sleep(nanoseconds: 1_000_000)
        }
        #expect(keyRing.didStageStart)
        await model.refresh()
        #expect(keyRing.reconcileCount == 0)
        #expect(keyRing.discardCount == 0)
        keyRing.resumeStage()
        await control.value
        #expect(keyRing.discardCount == 1)
    }

    @Test("Journal persist and server cancel failure retains staged state for cancel-reset")
    @MainActor
    func persistFailureWithoutConfirmedCancelRetainsCandidate() async throws {
        let active = try fakeTrustedWakeStatus(
            generation: 1,
            fingerprint: "old-fingerprint",
            pendingJSON: "null"
        )
        let pending = try fakeTrustedWakeStatus(
            generation: 2,
            fingerprint: "old-fingerprint",
            pendingJSON: fakePendingKeyControlJSON(expiresAt: "2099-07-13T12:05:00Z")
        )
        let keyRing = FailingPersistTrustedWakeKeyRing()
        let restarted = MutableFlag(false)
        let client = FakeCoreClient(
            trustedWakeStatusResults: [.success(active), .success(pending)],
            trustedWakePrepareResult: .success(try fakeTrustedWakePrepareResponse()),
            trustedWakeCancelResults: [.failure(URLError(.cannotConnectToHost))]
        )
        let model = TrustedWakeModel(
            client: client,
            signer: FakeTrustedWakeSigner(),
            keyRing: keyRing,
            installKeyControl: { restarted.value = true }
        )
        await model.beginKeyControl(
            operation: .recover,
            confirmation: jarvisTrustedWakeRecoverConfirmation
        )
        #expect(!restarted.value)
        #expect(keyRing.cancelLocalCount == 0)
        #expect(model.status?.pendingKeyControl != nil)
        #expect(model.errorMessage?.contains("cancel could not be confirmed") == true)
        #expect(model.errorMessage?.contains("Resume is unavailable") == true)
    }

    @Test("Rust status timestamps decode and model uses injected signer once")
    @MainActor
    func modelUsesInjectedSigner() async throws {
        let client = FakeCoreClient()
        let model = TrustedWakeModel(client: client, signer: FakeTrustedWakeSigner())
        await model.refresh()
        #expect(model.status?.rule?.createdAt == "2026-07-13T10:00:00.123456Z")
        await model.handleSystemWake()
        #expect(client.trustedWakeSubmitCount == 1)
        #expect(model.lastEvent?.state == "completed")
    }

    @Test("Model runs explicit provision action and reports restart degradation truthfully")
    @MainActor
    func modelProvisionActionAndFailureCopy() async {
        let called = MutableFlag(false)
        let successful = TrustedWakeModel(
            client: FakeCoreClient(),
            signer: FakeTrustedWakeSigner(),
            provision: { called.value = true }
        )
        await successful.provision()
        #expect(called.value)
        #expect(successful.status?.rule != nil)
        #expect(successful.errorMessage == nil)

        let failed = TrustedWakeModel(
            client: FakeCoreClient(),
            signer: FakeTrustedWakeSigner(),
            provision: { throw JarvisCoreSupervisorError.trustedWakeRestartFailed }
        )
        await failed.provision()
        #expect(failed.errorMessage?.contains("supervisor may be degraded") == true)
        #expect(failed.errorMessage?.contains("existing core was preserved") != true)
    }

    @Test("Model exposes and explicitly resolves ambiguous dispatch without retry")
    @MainActor
    func modelResolvesAmbiguousDispatchWithoutRetry() async {
        let item = JarvisTrustedWakeAttentionItem(
            eventId: UUID(uuidString: "00000000-0000-4000-8000-000000000331")!,
            schedulerJobId: UUID(uuidString: "00000000-0000-4000-8000-000000000332")!,
            ruleGeneration: 7,
            state: "dispatch_started",
            receivedAt: "2026-07-13T10:00:00Z",
            updatedAt: "2026-07-13T10:00:01Z"
        )
        let client = FakeCoreClient(trustedWakeAttentionItems: [item])
        let model = TrustedWakeModel(client: client, signer: FakeTrustedWakeSigner())

        await model.refresh()
        #expect(model.attentionItems == [item])
        await model.resolve(item)

        #expect(client.trustedWakeResolutionRequests.count == 1)
        #expect(client.trustedWakeResolutionRequests[0].id == item.eventId)
        #expect(client.trustedWakeResolutionRequests[0].request.expectedGeneration == 7)
        #expect(client.trustedWakeResolutionRequests[0].request.expectedState == "dispatch_started")
        #expect(model.attentionItems.isEmpty)
        #expect(client.trustedWakeSubmitCount == 0)
    }

    @Test("Normal startup omits Keychain and explicit provision restarts once with stdin")
    @MainActor
    func supervisorUsesOneShotBootstrapStdin() async throws {
        let bootstrap = Data("{\"public_key_x963_b64\":\"public-only\"}".utf8)
        let launcher = FakeProcessLauncher()
        let client = FakeCoreClient(healthResults: [
            .failure(URLError(.cannotConnectToHost)),
            .success(sampleHealth()),
            .failure(URLError(.cannotConnectToHost)),
            .success(sampleHealth()),
            .failure(URLError(.cannotConnectToHost)),
            .success(sampleHealth()),
        ])
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite")
            ),
            client: client,
            processLauncher: launcher
        )
        await supervisor.start(environmentOverrides: ["JARVIS_LOCAL_MODEL": "fake-local-model"])
        #expect(launcher.launches.count == 1)
        #expect(!launcher.launches[0].arguments.contains("--startup-config-stdin"))
        #expect(launcher.launches[0].standardInput == nil)

        try await supervisor.provisionTrustedWake(using: FakeBootstrapProvider(result: .success(bootstrap)))
        #expect(launcher.launches.count == 2)
        #expect(launcher.launches[1].arguments.contains("--startup-config-stdin"))
        let startup = try #require(launcher.launches[1].standardInput)
        let startupJSON = try #require(JSONSerialization.jsonObject(with: startup) as? [String: Any])
        let trustedWake = try #require(startupJSON["trusted_wake"] as? [String: Any])
        #expect(trustedWake["kind"] as? String == "bootstrap")
        let trustedWakeDocument = try #require(trustedWake["document"] as? [String: String])
        #expect(trustedWakeDocument["public_key_x963_b64"] == "public-only")
        #expect(!launcher.launches[1].arguments.joined().contains("public-only"))
        #expect(launcher.launches[1].environment["JARVIS_LOCAL_MODEL"] == "fake-local-model")

        let stopped = await supervisor.stop()
        #expect(stopped)
        await supervisor.start()
        #expect(launcher.launches.count == 3)
        #expect(!launcher.launches[2].arguments.contains("--startup-config-stdin"))
        #expect(launcher.launches[2].standardInput == nil)
    }

    @Test("Bootstrap preparation failure preserves the running supervised core")
    @MainActor
    func failedProvisionPreservesRunningCore() async {
        let launcher = FakeProcessLauncher()
        let client = FakeCoreClient(healthResults: [
            .failure(URLError(.cannotConnectToHost)),
            .success(sampleHealth()),
        ])
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite")
            ),
            client: client,
            processLauncher: launcher
        )
        await supervisor.start()

        do {
            try await supervisor.provisionTrustedWake(
                using: FakeBootstrapProvider(result: .failure(.unavailable))
            )
            Issue.record("Expected bootstrap error")
        } catch JarvisCoreSupervisorError.trustedWakeBootstrapPreparationFailed {
            // Expected.
        } catch {
            Issue.record("Unexpected bootstrap error: \(error)")
        }
        #expect(supervisor.mode == .available)
        #expect(launcher.launches.count == 1)
        #expect(launcher.processes[0].isRunning)

        do {
            try await supervisor.provisionTrustedWake(
                using: FakeBootstrapProvider(result: .success(nil))
            )
            Issue.record("Expected nil bootstrap to fail closed")
        } catch JarvisCoreSupervisorError.trustedWakeBootstrapUnavailable {
            // Expected.
        } catch {
            Issue.record("Unexpected nil bootstrap error: \(error)")
        }
        #expect(supervisor.mode == .available)
        #expect(launcher.launches.count == 1)
        #expect(launcher.processes[0].isRunning)

        do {
            try await supervisor.provisionTrustedWake(
                using: FakeBootstrapProvider(result: .success(Data(repeating: 1, count: 8 * 1024 + 1)))
            )
            Issue.record("Expected oversized bootstrap to fail closed")
        } catch JarvisCoreSupervisorError.trustedWakeBootstrapTooLarge {
            // Expected.
        } catch {
            Issue.record("Unexpected oversized bootstrap error: \(error)")
        }
        #expect(supervisor.mode == .available)
        #expect(launcher.launches.count == 1)
        #expect(launcher.processes[0].isRunning)
    }

    @Test("Stop failure prevents provisioning relaunch")
    @MainActor
    func stopFailurePreventsProvisionRelaunch() async {
        let launcher = DelayedStopProcessLauncher(runningChecksAfterTerminate: 10_000)
        let client = FakeCoreClient(healthResults: [
            .failure(URLError(.cannotConnectToHost)),
            .success(sampleHealth()),
        ])
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite"),
                startupTimeoutSeconds: 0.002,
                healthPollIntervalNanoseconds: 100_000
            ),
            client: client,
            processLauncher: launcher
        )
        await supervisor.start()

        do {
            try await supervisor.provisionTrustedWake(
                using: FakeBootstrapProvider(result: .success(Data("{}".utf8)))
            )
            Issue.record("Expected stop timeout")
        } catch JarvisCoreSupervisorError.trustedWakeStopFailed {
            // Expected.
        } catch {
            Issue.record("Unexpected stop failure: \(error)")
        }
        #expect(launcher.launchCount == 1)
        #expect(launcher.process.terminateCalled)
        if case .degraded = supervisor.mode {
            // Expected.
        } else {
            Issue.record("Expected degraded supervisor after stop failure")
        }
    }

    @Test("Provision restart failure is visible and does not loop")
    @MainActor
    func restartFailureIsVisibleAndDoesNotLoop() async {
        var healthResults: [Result<JarvisHealth, Error>] = [
            .failure(URLError(.cannotConnectToHost)),
            .success(sampleHealth()),
        ]
        for _ in 0 ..< 50 {
            healthResults.append(.failure(URLError(.cannotConnectToHost)))
        }
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite"),
                startupTimeoutSeconds: 0.003,
                healthPollIntervalNanoseconds: 100_000
            ),
            client: FakeCoreClient(healthResults: healthResults),
            processLauncher: launcher
        )
        await supervisor.start()

        do {
            try await supervisor.provisionTrustedWake(
                using: FakeBootstrapProvider(result: .success(Data("{}".utf8)))
            )
            Issue.record("Expected restart failure")
        } catch JarvisCoreSupervisorError.trustedWakeRestartFailed {
            // Expected.
        } catch {
            Issue.record("Unexpected restart error: \(error)")
        }
        #expect(launcher.launches.count == 2)
        #expect(!launcher.processes[0].isRunning)
        #expect(!launcher.processes[1].isRunning)
        if case .degraded = supervisor.mode {
            // Expected.
        } else {
            Issue.record("Expected degraded supervisor after restart failure")
        }
    }

    @Test("Concurrent provision is single-flight")
    @MainActor
    func concurrentProvisionIsRejected() async {
        let provider = ControlledBootstrapProvider(data: Data("{}".utf8))
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite")
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth()),
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth()),
            ]),
            processLauncher: launcher
        )
        await supervisor.start()
        let first = Task { @MainActor in
            try await supervisor.provisionTrustedWake(using: provider)
        }
        let providerStarted = await provider.waitUntilStarted()
        #expect(providerStarted)

        do {
            try await supervisor.provisionTrustedWake(
                using: FakeBootstrapProvider(result: .success(Data("{}".utf8)))
            )
            Issue.record("Expected concurrent provision rejection")
        } catch JarvisCoreSupervisorError.trustedWakeProvisionInProgress {
            // Expected.
        } catch {
            Issue.record("Unexpected concurrent provision error: \(error)")
        }
        #expect(launcher.launches.count == 1)
        #expect(launcher.processes[0].isRunning)

        provider.resume()
        do {
            try await first.value
        } catch {
            Issue.record("First provision unexpectedly failed: \(error)")
        }
        #expect(launcher.launches.count == 2)
    }

    @Test("Provision preparation never stops a replacement core")
    @MainActor
    func provisionRevalidatesOriginalProcessIdentity() async {
        let provider = ControlledBootstrapProvider(data: Data("{}".utf8))
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite")
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth()),
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth()),
            ]),
            processLauncher: launcher
        )
        await supervisor.start()
        let provision = Task { @MainActor in
            try await supervisor.provisionTrustedWake(using: provider)
        }
        let providerStarted = await provider.waitUntilStarted()
        #expect(providerStarted)

        let stopped = await supervisor.stop()
        #expect(stopped)
        await supervisor.start()
        #expect(launcher.launches.count == 2)
        let replacement = launcher.processes[1]
        #expect(replacement.isRunning)

        provider.resume()
        do {
            try await provision.value
            Issue.record("Expected process identity revalidation failure")
        } catch JarvisCoreSupervisorError.trustedWakeCoreChangedDuringPreparation {
            // Expected.
        } catch {
            Issue.record("Unexpected identity revalidation error: \(error)")
        }
        #expect(launcher.launches.count == 2)
        #expect(replacement.isRunning)
        #expect(supervisor.mode == .available)
    }

    @Test("Provision stop serializes concurrent start and stop")
    @MainActor
    func provisionStopSerializesLifecycleMutations() async {
        let launcher = ControlledLifecycleProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite"),
                startupTimeoutSeconds: 1,
                healthPollIntervalNanoseconds: 1_000_000
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth()),
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth()),
            ]),
            processLauncher: launcher
        )
        await supervisor.start()
        let provision = Task { @MainActor in
            try await supervisor.provisionTrustedWake(
                using: FakeBootstrapProvider(result: .success(Data("{}".utf8)))
            )
        }

        for _ in 0 ..< 100 where !launcher.firstProcess.terminateCalled {
            try? await Task.sleep(nanoseconds: 1_000_000)
        }
        #expect(launcher.firstProcess.terminateCalled)

        await supervisor.start(environmentOverrides: ["JARVIS_LOCAL_MODEL": "must-not-launch"])
        let concurrentStop = await supervisor.stop()
        #expect(!concurrentStop)
        #expect(launcher.launchCount == 1)

        launcher.firstProcess.allowExit()
        do {
            try await provision.value
        } catch {
            Issue.record("Provision unexpectedly failed: \(error)")
        }

        #expect(launcher.launchCount == 2)
        #expect(launcher.secondProcess?.isRunning == true)
        #expect(supervisor.mode == .available)
    }

    @Test("Foundation launcher delivers exact stdin bytes and closes EOF")
    func foundationLauncherClosesBootstrapStdin() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appending(path: "jarvis-bootstrap-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let received = directory.appending(path: "received.bin")
        let eofMarker = directory.appending(path: "eof.txt")
        let bootstrap = Data([0, 1, 2, 10, 13, 127, 255])
        let process = try await FoundationJarvisCoreProcessLauncher().launch(
            executableURL: URL(fileURLWithPath: "/bin/sh"),
            arguments: [
                "-c",
                "cat > \"$1\"; printf eof > \"$2\"",
                "jarvis-bootstrap-test",
                received.path,
                eofMarker.path,
            ],
            environment: ProcessInfo.processInfo.environment,
            standardInput: bootstrap
        )
        defer { process.terminate() }

        for _ in 0 ..< 100 where !FileManager.default.fileExists(atPath: eofMarker.path) {
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        #expect(try Data(contentsOf: received) == bootstrap)
        #expect(try String(contentsOf: eofMarker, encoding: .utf8) == "eof")
    }

    @Test("Key-control install uses one supervised stdin restart and preserves environment")
    @MainActor
    func keyControlInstallIsOneShotAndSerialized() async throws {
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite")
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth()),
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth()),
            ]),
            processLauncher: launcher
        )
        await supervisor.start(environmentOverrides: ["JARVIS_LOCAL_MODEL": "selected-model"])
        let document = Data("{\"operation\":\"test\"}".utf8)
        try await supervisor.installTrustedWakeKeyControl(
            using: FakeKeyControlInstallProvider(result: .success(document))
        )
        #expect(launcher.launches.count == 2)
        #expect(launcher.launches[1].arguments.contains("--startup-config-stdin"))
        let startup = try #require(launcher.launches[1].standardInput)
        let startupJSON = try #require(JSONSerialization.jsonObject(with: startup) as? [String: Any])
        let trustedWake = try #require(startupJSON["trusted_wake"] as? [String: Any])
        #expect(trustedWake["kind"] as? String == "key_control")
        let trustedWakeDocument = try #require(trustedWake["document"] as? [String: String])
        #expect(trustedWakeDocument["operation"] == "test")
        #expect(launcher.launches[1].environment["JARVIS_LOCAL_MODEL"] == "selected-model")
    }

    @Test("Near-expiry or Keychain preparation failure preserves the healthy core")
    @MainActor
    func keyControlPreparationFailurePreservesCore() async {
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite"),
                startupTimeoutSeconds: 2
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)),
                .success(sampleHealth()),
            ]),
            processLauncher: launcher
        )
        await supervisor.start()
        let provider = RejectingKeyControlInstallProvider()
        do {
            try await supervisor.installTrustedWakeKeyControl(using: provider)
            Issue.record("Expected key-control preparation failure")
        } catch JarvisCoreSupervisorError.trustedWakeKeyControlPreparationFailed {
            // Expected.
        } catch {
            Issue.record("Unexpected key-control preparation error: \(error)")
        }
        #expect(provider.minimumValiditySeconds == 7)
        #expect(launcher.launches.count == 1)
        #expect(launcher.processes[0].isRunning)
        #expect(!launcher.processes[0].terminateCalled)
    }

    @Test("Key-control preparation never stops a replacement core")
    @MainActor
    func keyControlRevalidatesProcessIdentity() async {
        let provider = ControlledKeyControlInstallProvider(data: Data("install".utf8))
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite")
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth()),
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth()),
            ]),
            processLauncher: launcher
        )
        await supervisor.start()
        let install = Task { @MainActor in
            try await supervisor.installTrustedWakeKeyControl(using: provider)
        }
        for _ in 0 ..< 100 where !provider.didStart {
            try? await Task.sleep(nanoseconds: 1_000_000)
        }
        #expect(provider.didStart)
        #expect(await supervisor.stop())
        await supervisor.start()
        let replacement = launcher.processes[1]
        provider.resume()
        do {
            try await install.value
            Issue.record("Expected process identity failure")
        } catch JarvisCoreSupervisorError.trustedWakeCoreChangedDuringPreparation {
            // Expected.
        } catch {
            Issue.record("Unexpected identity error: \(error)")
        }
        #expect(launcher.launches.count == 2)
        #expect(replacement.isRunning)
    }

    @Test("Key-control stop failure prevents relaunch")
    @MainActor
    func keyControlStopFailurePreventsRelaunch() async {
        let launcher = DelayedStopProcessLauncher(runningChecksAfterTerminate: 10_000)
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite"),
                startupTimeoutSeconds: 0.002,
                healthPollIntervalNanoseconds: 100_000
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth()),
            ]),
            processLauncher: launcher
        )
        await supervisor.start()
        do {
            try await supervisor.installTrustedWakeKeyControl(
                using: FakeKeyControlInstallProvider(result: .success(Data("{}".utf8)))
            )
            Issue.record("Expected stop failure")
        } catch JarvisCoreSupervisorError.trustedWakeStopFailed {
            // Expected.
        } catch {
            Issue.record("Unexpected stop error: \(error)")
        }
        #expect(launcher.launchCount == 1)
    }

    @Test("Key-control restart failure is visible and never retries")
    @MainActor
    func keyControlRestartFailureDoesNotRetry() async {
        var results: [Result<JarvisHealth, Error>] = [
            .failure(URLError(.cannotConnectToHost)), .success(sampleHealth()),
        ]
        for _ in 0 ..< 50 { results.append(.failure(URLError(.cannotConnectToHost))) }
        let launcher = FakeProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite"),
                startupTimeoutSeconds: 0.003,
                healthPollIntervalNanoseconds: 100_000
            ),
            client: FakeCoreClient(healthResults: results),
            processLauncher: launcher
        )
        await supervisor.start()
        do {
            try await supervisor.installTrustedWakeKeyControl(
                using: FakeKeyControlInstallProvider(result: .success(Data("{}".utf8)))
            )
            Issue.record("Expected restart failure")
        } catch JarvisCoreSupervisorError.trustedWakeRestartFailed {
            // Expected.
        } catch {
            Issue.record("Unexpected restart error: \(error)")
        }
        #expect(launcher.launches.count == 2)
        #expect(!launcher.processes[1].isRunning)
    }

    @Test("Key-control stop serializes concurrent lifecycle mutations")
    @MainActor
    func keyControlSerializesLifecycleMutations() async {
        let launcher = ControlledLifecycleProcessLauncher()
        let supervisor = JarvisCoreSupervisor(
            configuration: JarvisCoreSupervisorConfiguration(
                executableURL: URL(fileURLWithPath: "/tmp/jarvis-cli"),
                databaseURL: URL(fileURLWithPath: "/tmp/jarvis.sqlite"),
                startupTimeoutSeconds: 1,
                healthPollIntervalNanoseconds: 1_000_000
            ),
            client: FakeCoreClient(healthResults: [
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth()),
                .failure(URLError(.cannotConnectToHost)), .success(sampleHealth()),
            ]),
            processLauncher: launcher
        )
        await supervisor.start()
        let install = Task { @MainActor in
            try await supervisor.installTrustedWakeKeyControl(
                using: FakeKeyControlInstallProvider(result: .success(Data("{}".utf8)))
            )
        }
        for _ in 0 ..< 100 where !launcher.firstProcess.terminateCalled {
            try? await Task.sleep(nanoseconds: 1_000_000)
        }
        await supervisor.start(environmentOverrides: ["JARVIS_LOCAL_MODEL": "must-not-launch"])
        #expect(!(await supervisor.stop()))
        #expect(launcher.launchCount == 1)
        launcher.firstProcess.allowExit()
        do { try await install.value } catch { Issue.record("Install failed: \(error)") }
        #expect(launcher.launchCount == 2)
        #expect(launcher.secondProcess?.isRunning == true)
    }
}

private struct FakeTrustedWakeSigner: TrustedWakeEnvelopeSigning {
    func envelope(
        status _: JarvisTrustedWakeStatus,
        occurredAt _: Date
    ) throws -> JarvisTrustedWakeEnvelope {
        JarvisTrustedWakeEnvelope(payloadBase64: "bounded-payload", signatureDERBase64: "bounded-signature")
    }
}

private func fakePendingKeyControlJSON(expiresAt: String) -> String {
    """
    {
      "operation": "recover",
      "source_generation": 1,
      "target_generation": 2,
      "old_fingerprint": "old-fingerprint",
      "new_fingerprint": "new-fingerprint",
      "expires_at": "\(expiresAt)",
      "created_at": "2026-07-13T12:00:00Z"
    }
    """
}

private func fakeTrustedWakeStatus(
    generation: UInt64,
    fingerprint: String,
    pendingJSON: String
) throws -> JarvisTrustedWakeStatus {
    try JSONDecoder().decode(
        JarvisTrustedWakeStatus.self,
        from: Data(
            """
            {
              "schema_version": 1,
              "session_id": "00000000-0000-4000-8000-000000000111",
              "challenge": "challenge",
              "rule": {
                "id": "4a617276-6973-4000-8000-000000000010",
                "enabled": false,
                "key_fingerprint": "\(fingerprint)",
                "generation": \(generation),
                "highest_counter": 0,
                "created_at": "2026-07-13T10:00:00Z",
                "updated_at": "2026-07-13T10:00:01Z"
              },
              "attention_required": false,
              "ambiguous_dispatch_count": 0,
              "pending_key_control": \(pendingJSON),
              "proof_boundary": "local explicit control only"
            }
            """.utf8
        )
    )
}

private func fakeTrustedWakePrepareResponse() throws -> JarvisTrustedWakeKeyControlPrepareResponse {
    try JSONDecoder().decode(
        JarvisTrustedWakeKeyControlPrepareResponse.self,
        from: Data(
            """
            {
              "pending": \(fakePendingKeyControlJSON(expiresAt: "2099-07-13T12:05:00Z")),
              "grant_token": "bounded-one-shot-token",
              "blocked_accepted_count": 0,
              "proof_boundary": "local explicit control only"
            }
            """.utf8
        )
    )
}

private class RecordingTrustedWakeKeyRing: TrustedWakeKeyRingManaging, @unchecked Sendable {
    private let lock = NSLock()
    private(set) var reconcileCount = 0
    private(set) var discardCount = 0
    private(set) var cancelLocalCount = 0

    func stage(
        operation _: JarvisTrustedWakeKeyControlOperation,
        status _: JarvisTrustedWakeStatus,
        confirmation _: String
    ) throws -> JarvisTrustedWakeKeyControlPrepareRequest {
        return try JSONDecoder().decode(
            JarvisTrustedWakeKeyControlPrepareRequest.self,
            from: Data(
                """
                {
                  "operation": "recover",
                  "expected_generation": 1,
                  "expected_fingerprint": "old-fingerprint",
                  "new_public_key_x963_b64": "candidate-public",
                  "confirmation": "RECOVER LOST TRUSTED WAKE KEY AND BLOCK PENDING WORK",
                  "proof": null
                }
                """.utf8
            )
        )
    }

    func persist(response _: JarvisTrustedWakeKeyControlPrepareResponse) throws {}
    func installData(minimumValiditySeconds _: TimeInterval) throws -> Data? { Data("install".utf8) }

    func reconcile(status _: JarvisTrustedWakeStatus) throws -> Bool {
        lock.lock()
        reconcileCount += 1
        lock.unlock()
        return false
    }

    func discardUnjournaledCandidate() throws {
        lock.lock()
        discardCount += 1
        lock.unlock()
    }

    func cancelLocalPending() throws {
        lock.lock()
        cancelLocalCount += 1
        lock.unlock()
    }
}

private final class FailingPersistTrustedWakeKeyRing: RecordingTrustedWakeKeyRing, @unchecked Sendable {
    override func persist(response _: JarvisTrustedWakeKeyControlPrepareResponse) throws {
        throw BootstrapTestError.unavailable
    }
}

private final class ReconciledTrustedWakeKeyRing: RecordingTrustedWakeKeyRing, @unchecked Sendable {
    override func reconcile(status _: JarvisTrustedWakeStatus) throws -> Bool { true }
}

private final class ControlledTrustedWakeKeyRing: RecordingTrustedWakeKeyRing, @unchecked Sendable {
    private let stateLock = NSLock()
    private var stageStarted = false
    private let release = DispatchSemaphore(value: 0)

    override func stage(
        operation: JarvisTrustedWakeKeyControlOperation,
        status: JarvisTrustedWakeStatus,
        confirmation: String
    ) throws -> JarvisTrustedWakeKeyControlPrepareRequest {
        stateLock.lock()
        stageStarted = true
        stateLock.unlock()
        release.wait()
        return try super.stage(
            operation: operation,
            status: status,
            confirmation: confirmation
        )
    }

    var didStageStart: Bool {
        stateLock.lock()
        defer { stateLock.unlock() }
        return stageStarted
    }

    func resumeStage() {
        release.signal()
    }
}

private enum BootstrapTestError: Error {
    case unavailable
}

private final class DeterministicAuthRandom: @unchecked Sendable {
    private let lock = NSLock()
    private var seed: UInt8 = 0

    func bytes(count: Int) throws -> Data {
        lock.lock()
        seed &+= 1
        let value = seed
        lock.unlock()
        return Data(repeating: value, count: count)
    }
}

private func ipcAuth(from envelopeData: Data) throws -> (token: String, generation: UInt64) {
    let envelope = try #require(JSONSerialization.jsonObject(with: envelopeData) as? [String: Any])
    let auth = try #require(envelope["ipc_auth"] as? [String: Any])
    let token = try #require(auth["token"] as? String)
    let generation = try #require((auth["generation"] as? NSNumber)?.uint64Value)
    #expect(auth["scheme"] as? String == "bearer")
    return (token, generation)
}

private enum TestWorkspaceRootError: Error {
    case failed
}

private final class InMemoryWorkspaceRootBookmarkStore: JarvisWorkspaceRootBookmarkStoring, @unchecked Sendable {
    private let lock = NSLock()
    private var storedRecords: [JarvisStoredWorkspaceRootBookmark]

    init(records: [JarvisStoredWorkspaceRootBookmark] = []) {
        storedRecords = records
    }

    var records: [JarvisStoredWorkspaceRootBookmark] {
        lock.lock()
        defer { lock.unlock() }
        return storedRecords
    }

    func load() throws -> [JarvisStoredWorkspaceRootBookmark] { records }

    func save(_ records: [JarvisStoredWorkspaceRootBookmark]) throws {
        lock.lock()
        storedRecords = records
        lock.unlock()
    }
}

private final class FakeWorkspaceRootBookmarkAccessor: JarvisSecurityScopedBookmarkAccessing, @unchecked Sendable {
    private let lock = NSLock()
    private let resolutions: [Data: JarvisResolvedWorkspaceRootBookmark]
    private let createdBookmark: Data
    private let resolutionError: Error?
    private(set) var startedURLs: [URL] = []
    private(set) var stoppedURLs: [URL] = []

    init(
        resolutions: [Data: JarvisResolvedWorkspaceRootBookmark] = [:],
        createdBookmark: Data = Data("bookmark".utf8),
        resolutionError: Error? = nil
    ) {
        self.resolutions = resolutions
        self.createdBookmark = createdBookmark
        self.resolutionError = resolutionError
    }

    func createBookmark(for _: URL) throws -> Data { createdBookmark }

    func resolveBookmark(_ data: Data) throws -> JarvisResolvedWorkspaceRootBookmark {
        if let resolutionError { throw resolutionError }
        guard let resolution = resolutions[data] else { throw TestWorkspaceRootError.failed }
        return resolution
    }

    func isDirectory(_: URL) throws -> Bool { true }

    func startAccessing(_ url: URL) -> Bool {
        lock.lock()
        startedURLs.append(url)
        lock.unlock()
        return true
    }

    func stopAccessing(_ url: URL) {
        lock.lock()
        stoppedURLs.append(url)
        lock.unlock()
    }
}

@MainActor
private final class FakeWorkspaceRootGrantProvider: JarvisWorkspaceRootGrantProviding {
    private let roots: [JarvisWorkspaceRootLaunchRoot]
    private let releaseFlag = WorkspaceRootReleaseFlag()

    var released: Bool { releaseFlag.value }

    init(roots: [JarvisWorkspaceRootLaunchRoot]) {
        self.roots = roots
    }

    func acquireForCoreLaunch() throws -> JarvisWorkspaceRootAccessLease {
        releaseFlag.clear()
        return JarvisWorkspaceRootAccessLease(roots: roots) { [releaseFlag] in releaseFlag.mark() }
    }
}

private final class WorkspaceRootReleaseFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var released = false

    var value: Bool {
        lock.lock()
        defer { lock.unlock() }
        return released
    }

    func clear() {
        lock.lock()
        released = false
        lock.unlock()
    }

    func mark() {
        lock.lock()
        released = true
        lock.unlock()
    }
}

private final class ControlledBootstrapProvider: TrustedWakeBootstrapProviding, @unchecked Sendable {
    private let data: Data
    private let lock = NSLock()
    private var started = false
    private let release = DispatchSemaphore(value: 0)

    init(data: Data) {
        self.data = data
    }

    func bootstrapData() throws -> Data? {
        lock.withLock { started = true }
        release.wait()
        return data
    }

    func waitUntilStarted() async -> Bool {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: .seconds(5))
        while clock.now < deadline {
            let didStart = lock.withLock { started }
            if didStart {
                return true
            }
            try? await Task.sleep(for: .milliseconds(5))
        }
        return lock.withLock { started }
    }

    func resume() {
        release.signal()
    }
}

private struct FakeBootstrapProvider: TrustedWakeBootstrapProviding {
    var result: Result<Data?, BootstrapTestError>
    func bootstrapData() throws -> Data? { try result.get() }
}

private struct FakeKeyControlInstallProvider: TrustedWakeKeyControlInstallProviding {
    var result: Result<Data?, BootstrapTestError>
    func installData(minimumValiditySeconds _: TimeInterval) throws -> Data? { try result.get() }
}

private final class RejectingKeyControlInstallProvider: TrustedWakeKeyControlInstallProviding, @unchecked Sendable {
    private(set) var minimumValiditySeconds: TimeInterval?

    func installData(minimumValiditySeconds: TimeInterval) throws -> Data? {
        self.minimumValiditySeconds = minimumValiditySeconds
        throw BootstrapTestError.unavailable
    }
}

private final class ControlledKeyControlInstallProvider: TrustedWakeKeyControlInstallProviding, @unchecked Sendable {
    private let data: Data
    private let lock = NSLock()
    private var started = false
    private let release = DispatchSemaphore(value: 0)

    init(data: Data) { self.data = data }

    func installData(minimumValiditySeconds _: TimeInterval) throws -> Data? {
        lock.lock()
        started = true
        lock.unlock()
        release.wait()
        return data
    }

    var didStart: Bool {
        lock.lock()
        defer { lock.unlock() }
        return started
    }

    func resume() { release.signal() }
}

private final class FakeProcess: JarvisCoreProcess, @unchecked Sendable {
    private(set) var isRunning = true
    private(set) var terminateCalled = false

    func terminate() {
        terminateCalled = true
        isRunning = false
    }

    func simulateUnexpectedExit() {
        isRunning = false
    }
}

private final class FakeProcessLauncher: JarvisCoreProcessLaunching, @unchecked Sendable {
    struct Launch: Equatable {
        var executableURL: URL
        var arguments: [String]
        var environment: [String: String]
        var standardInput: Data?
    }

    private(set) var launches: [Launch] = []
    private(set) var processes: [FakeProcess] = []

    func launch(
        executableURL: URL,
        arguments: [String],
        environment: [String: String],
        standardInput: Data?
    ) async throws -> any JarvisCoreProcess {
        launches.append(Launch(
            executableURL: executableURL,
            arguments: arguments,
            environment: environment,
            standardInput: standardInput
        ))
        let process = FakeProcess()
        processes.append(process)
        return process
    }
}

private struct FailingProcessLauncher: JarvisCoreProcessLaunching {
    func launch(
        executableURL _: URL,
        arguments _: [String],
        environment _: [String: String],
        standardInput _: Data?
    ) async throws -> any JarvisCoreProcess {
        throw URLError(.cannotCreateFile)
    }
}

private final class DelayedStopProcess: JarvisCoreProcess, @unchecked Sendable {
    private(set) var terminateCalled = false
    private(set) var runningChecksAfterTerminate: Int

    init(runningChecksAfterTerminate: Int) {
        self.runningChecksAfterTerminate = runningChecksAfterTerminate
    }

    var isRunning: Bool {
        guard terminateCalled else { return true }
        guard runningChecksAfterTerminate > 0 else { return false }
        runningChecksAfterTerminate -= 1
        return true
    }

    func terminate() {
        terminateCalled = true
    }
}

private final class DelayedStopProcessLauncher: JarvisCoreProcessLaunching, @unchecked Sendable {
    let process: DelayedStopProcess
    private(set) var launchCount = 0

    init(runningChecksAfterTerminate: Int) {
        process = DelayedStopProcess(runningChecksAfterTerminate: runningChecksAfterTerminate)
    }

    func launch(
        executableURL: URL,
        arguments: [String],
        environment: [String: String],
        standardInput _: Data?
    ) async throws -> any JarvisCoreProcess {
        launchCount += 1
        return process
    }
}

private final class ControlledLifecycleProcess: JarvisCoreProcess, @unchecked Sendable {
    private(set) var terminateCalled = false
    private var exitAllowed = false

    var isRunning: Bool {
        !terminateCalled || !exitAllowed
    }

    func terminate() {
        terminateCalled = true
    }

    func allowExit() {
        exitAllowed = true
    }
}

private final class ControlledLifecycleProcessLauncher: JarvisCoreProcessLaunching, @unchecked Sendable {
    let firstProcess = ControlledLifecycleProcess()
    private(set) var secondProcess: FakeProcess?
    private(set) var launchCount = 0

    func launch(
        executableURL _: URL,
        arguments _: [String],
        environment _: [String: String],
        standardInput _: Data?
    ) async throws -> any JarvisCoreProcess {
        launchCount += 1
        if launchCount == 1 {
            return firstProcess
        }
        let process = FakeProcess()
        secondProcess = process
        return process
    }
}

private final class CapturingModelRuntimeController: JarvisLocalModelRuntimeControlling, @unchecked Sendable {
    struct Request: Equatable {
        var model: String
        var baseURL: String
    }

    private(set) var loadRequests: [Request] = []
    private(set) var unloadRequests: [Request] = []
    private(set) var pullRequests: [Request] = []
    private(set) var listRequests: [String] = []
    private(set) var progressSnapshots: [JarvisOllamaPullProgress] = []
    private var installedModels: [JarvisOllamaModelInfo]

    init(installedModels: [JarvisOllamaModelInfo] = []) {
        self.installedModels = installedModels
    }

    func listOllamaModels(baseURL: URL) async throws -> [JarvisOllamaModelInfo] {
        listRequests.append(baseURL.absoluteString)
        return installedModels
    }

    func pullOllamaModel(
        model: String,
        baseURL: URL,
        progress: @escaping @Sendable (JarvisOllamaPullProgress) async -> Void
    ) async throws {
        pullRequests.append(Request(model: model, baseURL: baseURL.absoluteString))
        let snapshots = [
            JarvisOllamaPullProgress(status: "pulling manifest"),
            JarvisOllamaPullProgress(status: "downloading", completedBytes: 50, totalBytes: 100),
            JarvisOllamaPullProgress(status: "success")
        ]
        for snapshot in snapshots {
            progressSnapshots.append(snapshot)
            await progress(snapshot)
        }
        if !installedModels.contains(where: { $0.name == model }) {
            installedModels.append(JarvisOllamaModelInfo(
                name: model,
                installed: true,
                diskSizeBytes: 1_073_741_824,
                estimatedRamBytes: 1_073_741_824,
                details: "downloaded test model"
            ))
        }
    }

    func loadOllamaModel(model: String, baseURL: URL) async throws {
        loadRequests.append(Request(model: model, baseURL: baseURL.absoluteString))
    }

    func unloadOllamaModel(model: String, baseURL: URL) async throws {
        unloadRequests.append(Request(model: model, baseURL: baseURL.absoluteString))
    }
}

private actor CapturingProgressRecorder {
    private var capturedEvents: [JarvisOllamaPullProgress] = []

    var events: [JarvisOllamaPullProgress] {
        capturedEvents
    }

    func append(_ progress: JarvisOllamaPullProgress) {
        capturedEvents.append(progress)
    }
}

private final class CapturingURLProtocol: URLProtocol {
    private nonisolated(unsafe) static var capturedRequests: [URLRequest] = []
    private nonisolated(unsafe) static var capturedBodyStrings: [String?] = []
    private nonisolated(unsafe) static var responseHandler: ((URLRequest) -> (Int, Data))?

    static var requests: [URLRequest] {
        get async { capturedRequests }
    }

    static var bodyStrings: [String?] {
        get async { capturedBodyStrings }
    }

    static func reset() async {
        capturedRequests = []
        capturedBodyStrings = []
        responseHandler = nil
    }

    static func setResponder(_ responder: @escaping (URLRequest) -> (Int, Data)) async {
        responseHandler = responder
    }

    override class func canInit(with request: URLRequest) -> Bool {
        true
    }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest {
        request
    }

    override func startLoading() {
        Self.capturedRequests.append(request)
        Self.capturedBodyStrings.append(Self.bodyString(from: request))
        let (status, data) = Self.responseHandler?(request) ?? (200, Data())
        let response = HTTPURLResponse(
            url: request.url!,
            statusCode: status,
            httpVersion: nil,
            headerFields: ["Content-Type": "application/json"]
        )!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}

    private static func bodyString(from request: URLRequest) -> String? {
        if let httpBody = request.httpBody {
            return String(data: httpBody, encoding: .utf8)
        }
        guard let stream = request.httpBodyStream else {
            return nil
        }

        stream.open()
        defer { stream.close() }

        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 1024)
        while stream.hasBytesAvailable {
            let count = stream.read(&buffer, maxLength: buffer.count)
            if count <= 0 { break }
            data.append(buffer, count: count)
        }
        return String(data: data, encoding: .utf8)
    }
}

private func sampleMemoryItem(
    id: UUID = UUID(),
    category: String,
    key: String,
    value: String = "preview before write",
    provenance: String = "manual",
    sensitivity: String = "workspace",
    reviewedAt: String? = nil,
    deletedAt: String? = nil
) -> JarvisMemoryItem {
    JarvisMemoryItem(
        id: id,
        category: category,
        key: key,
        value: value,
        provenance: provenance,
        sensitivity: sensitivity,
        createdAt: "2026-05-20T12:00:00Z",
        updatedAt: "2026-05-20T12:00:01Z",
        reviewedAt: reviewedAt,
        deletedAt: deletedAt
    )
}

private final class FakeCoreClient: JarvisCoreClient, @unchecked Sendable {
    struct ApprovalDecision: Equatable {
        var id: UUID
        var approved: Bool
        var reason: String?
    }

    private var healthResults: [Result<JarvisHealth, Error>]
    private var tasks: [JarvisTask]
    private var auditEntries: [JarvisAuditEntry]
    private var activityEvents: [JarvisActivityEvent]
    private var commandStatus: String
    private(set) var submittedCommands: [JarvisCommandRequest]
    var submittedCommandsWithoutCancellationIDs: [JarvisCommandRequest] {
        submittedCommands.map { command in
            var command = command
            command.cancellationID = nil
            return command
        }
    }
    private(set) var commandCancellationRequests: [UUID]
    private let commandCancellationDelayNanoseconds: UInt64
    private let commandSubmissionDelayNanoseconds: UInt64
    private var contractResponse: JarvisContractResponse?
    private var releaseReadinessResponse: JarvisReleaseReadiness?
    private var releaseReadinessResults: [Result<JarvisReleaseReadiness, Error>]
    private var releaseEvidenceStatusResponse: JarvisReleaseEvidenceStatus?
    private var releaseEvidenceStatusResults: [Result<JarvisReleaseEvidenceStatus, Error>]
    private var releaseLiveDeviceRunbookResults: [Result<JarvisReleaseRunbook, Error>]
    private var releaseSignedDistributionRunbookResults: [Result<JarvisReleaseRunbook, Error>]
    private var releasePluginTrustRunbookResults: [Result<JarvisReleaseRunbook, Error>]
    private var releaseEvidenceBundleRunbookResults: [Result<JarvisReleaseRunbook, Error>]
    private var approvals: [JarvisPendingApproval]
    private var pluginManifests: [JarvisPluginManifest]
    private var modelToolCatalogResult: JarvisModelToolCatalog
    private var modelToolCatalogUnavailable: Bool
    private var installedPlugins: [JarvisInstalledPluginRecord]
    private var installedPluginsUnavailable: Bool
    private var schedulerJobs: [JarvisSchedulerJob]
    private var schedulerAttentionResponse: JarvisSchedulerAttentionSummary?
    private let schedulerAttentionDelayNanoseconds: UInt64
    private var schedulerNotificationOccurrences: [JarvisSchedulerNotificationOccurrence]
    private(set) var schedulerNotificationAcknowledgements: [JarvisSchedulerNotificationAcknowledgementRequest]
    private(set) var schedulerNotificationAcknowledgementIDs: [UUID]
    private let schedulerNotificationAcknowledgementFailureIDs: Set<UUID>
    private var memoryItems: [JarvisMemoryItem]
    private var memoryRetentionPlanResult: JarvisMemoryRetentionPlan?
    private var permissionGrantSummaryResult: JarvisPermissionGrantSummary?
    private(set) var approvalDecisions: [ApprovalDecision]
    private(set) var approvalExecutions: [UUID]
    private(set) var approvalExecutionCancellationIDs: [UUID]
    private let approvalExecutionDelayNanoseconds: UInt64
    private(set) var includeDeletedMemoryRequests: [Bool]
    private(set) var createdMemoryRequests: [JarvisCreateMemoryItemRequest]
    private(set) var updatedMemoryRequests: [(id: UUID, request: JarvisMemoryMutationRequest)]
    private(set) var runDueSchedulerLimits: [Int]
    private(set) var recoverStaleSchedulerRequests: [(olderThanSeconds: UInt64, limit: Int)]
    private(set) var trustedWakeSubmitCount = 0
    private(set) var trustedWakeResolutionRequests: [(id: UUID, request: JarvisTrustedWakeResolutionRequest)] = []
    private var trustedWakeAttentionItems: [JarvisTrustedWakeAttentionItem]
    private var trustedWakeStatusResults: [Result<JarvisTrustedWakeStatus, Error>]
    private var trustedWakePrepareResult: Result<JarvisTrustedWakeKeyControlPrepareResponse, Error>?
    private var trustedWakeCancelResults: [Result<JarvisTrustedWakeRule, Error>]
    private let releaseSmokeMode: Bool
    private let releaseSmokeFailureCall: String?
    private var releaseSmokePaused: Bool
    private(set) var releaseSmokeCalls: [String]

    init(
        healthResults: [Result<JarvisHealth, Error>] = [.success(sampleHealth())],
        tasks: [JarvisTask] = [],
        auditEntries: [JarvisAuditEntry] = [],
        activityEvents: [JarvisActivityEvent] = [],
        commandStatus: String = "completed",
        contractResponse: JarvisContractResponse? = nil,
        releaseReadiness: JarvisReleaseReadiness? = nil,
        releaseReadinessResults: [Result<JarvisReleaseReadiness, Error>] = [],
        releaseEvidenceStatus: JarvisReleaseEvidenceStatus? = nil,
        releaseEvidenceStatusResults: [Result<JarvisReleaseEvidenceStatus, Error>] = [],
        releaseLiveDeviceRunbookResults: [Result<JarvisReleaseRunbook, Error>] = [],
        releaseSignedDistributionRunbookResults: [Result<JarvisReleaseRunbook, Error>] = [],
        releasePluginTrustRunbookResults: [Result<JarvisReleaseRunbook, Error>] = [],
        releaseEvidenceBundleRunbookResults: [Result<JarvisReleaseRunbook, Error>] = [],
        approvals: [JarvisPendingApproval] = [],
        pluginManifests: [JarvisPluginManifest] = [],
        modelToolCatalog: JarvisModelToolCatalog = sampleModelToolCatalog(),
        modelToolCatalogUnavailable: Bool = false,
        installedPlugins: [JarvisInstalledPluginRecord] = [],
        installedPluginsUnavailable: Bool = false,
        schedulerJobs: [JarvisSchedulerJob] = [],
        schedulerAttention: JarvisSchedulerAttentionSummary? = nil,
        schedulerAttentionDelayNanoseconds: UInt64 = 0,
        schedulerNotificationOccurrences: [JarvisSchedulerNotificationOccurrence] = [],
        schedulerNotificationAcknowledgementFailureIDs: Set<UUID> = [],
        memoryItems: [JarvisMemoryItem] = [],
        memoryRetentionPlan: JarvisMemoryRetentionPlan? = nil,
        permissionGrantSummary: JarvisPermissionGrantSummary? = nil,
        trustedWakeAttentionItems: [JarvisTrustedWakeAttentionItem] = [],
        trustedWakeStatusResults: [Result<JarvisTrustedWakeStatus, Error>] = [],
        trustedWakePrepareResult: Result<JarvisTrustedWakeKeyControlPrepareResponse, Error>? = nil,
        trustedWakeCancelResults: [Result<JarvisTrustedWakeRule, Error>] = [],
        releaseSmokeMode: Bool = false,
        releaseSmokeFailureCall: String? = nil,
        approvalExecutionDelayNanoseconds: UInt64 = 0,
        commandCancellationDelayNanoseconds: UInt64 = 0,
        commandSubmissionDelayNanoseconds: UInt64 = 0
    ) {
        self.healthResults = healthResults
        self.tasks = tasks
        self.auditEntries = auditEntries
        self.activityEvents = activityEvents
        self.commandStatus = commandStatus
        self.submittedCommands = []
        self.commandCancellationRequests = []
        self.commandCancellationDelayNanoseconds = commandCancellationDelayNanoseconds
        self.commandSubmissionDelayNanoseconds = commandSubmissionDelayNanoseconds
        self.contractResponse = contractResponse
        self.releaseReadinessResponse = releaseReadiness
        self.releaseReadinessResults = releaseReadinessResults
        self.releaseEvidenceStatusResponse = releaseEvidenceStatus
        self.releaseEvidenceStatusResults = releaseEvidenceStatusResults
        self.releaseLiveDeviceRunbookResults = releaseLiveDeviceRunbookResults
        self.releaseSignedDistributionRunbookResults = releaseSignedDistributionRunbookResults
        self.releasePluginTrustRunbookResults = releasePluginTrustRunbookResults
        self.releaseEvidenceBundleRunbookResults = releaseEvidenceBundleRunbookResults
        self.approvals = approvals
        self.pluginManifests = pluginManifests
        self.modelToolCatalogResult = modelToolCatalog
        self.modelToolCatalogUnavailable = modelToolCatalogUnavailable
        self.installedPlugins = installedPlugins
        self.installedPluginsUnavailable = installedPluginsUnavailable
        self.schedulerJobs = schedulerJobs
        self.schedulerAttentionResponse = schedulerAttention
        self.schedulerAttentionDelayNanoseconds = schedulerAttentionDelayNanoseconds
        self.schedulerNotificationOccurrences = schedulerNotificationOccurrences
        self.schedulerNotificationAcknowledgements = []
        self.schedulerNotificationAcknowledgementIDs = []
        self.schedulerNotificationAcknowledgementFailureIDs = schedulerNotificationAcknowledgementFailureIDs
        self.memoryItems = memoryItems
        self.memoryRetentionPlanResult = memoryRetentionPlan
        self.permissionGrantSummaryResult = permissionGrantSummary
        self.trustedWakeAttentionItems = trustedWakeAttentionItems
        self.trustedWakeStatusResults = trustedWakeStatusResults
        self.trustedWakePrepareResult = trustedWakePrepareResult
        self.trustedWakeCancelResults = trustedWakeCancelResults
        self.releaseSmokeMode = releaseSmokeMode
        self.releaseSmokeFailureCall = releaseSmokeFailureCall
        self.releaseSmokePaused = false
        self.releaseSmokeCalls = []
        self.approvalDecisions = []
        self.approvalExecutions = []
        self.approvalExecutionCancellationIDs = []
        self.approvalExecutionDelayNanoseconds = approvalExecutionDelayNanoseconds
        self.includeDeletedMemoryRequests = []
        self.createdMemoryRequests = []
        self.updatedMemoryRequests = []
        self.runDueSchedulerLimits = []
        self.recoverStaleSchedulerRequests = []
    }

    func health() async throws -> JarvisHealth {
        try recordReleaseSmokeCall("health")
        guard !healthResults.isEmpty else {
            return sampleHealth()
        }

        return try healthResults.removeFirst().get()
    }

    func contract() async throws -> JarvisContractResponse {
        if let contractResponse {
            return contractResponse
        }

        return try JSONDecoder().decode(
            JarvisContractResponse.self,
            from: Data(
                """
                {
                  "contract": { "name": "jarvis.local-ipc", "version": 1, "core_version": "0.1.0" },
                  "endpoints": [
                    { "method": "GET", "path": "/health", "repository_required": false, "redacted": true }
                  ],
                  "safe_inspection_paths": ["/health"]
                }
                """.utf8
            )
        )
    }

    func releaseReadiness() async throws -> JarvisReleaseReadiness {
        if !releaseReadinessResults.isEmpty {
            return try releaseReadinessResults.removeFirst().get()
        }
        if let releaseReadinessResponse {
            return releaseReadinessResponse
        }

        return try JSONDecoder().decode(JarvisReleaseReadiness.self, from: releaseReadinessJSON())
    }

    func releaseEvidenceStatus() async throws -> JarvisReleaseEvidenceStatus {
        if !releaseEvidenceStatusResults.isEmpty {
            return try releaseEvidenceStatusResults.removeFirst().get()
        }
        if let releaseEvidenceStatusResponse {
            return releaseEvidenceStatusResponse
        }

        return try JSONDecoder().decode(JarvisReleaseEvidenceStatus.self, from: releaseEvidenceStatusJSON())
    }

    func releaseLiveDeviceRunbook() async throws -> JarvisReleaseRunbook {
        if !releaseLiveDeviceRunbookResults.isEmpty {
            return try releaseLiveDeviceRunbookResults.removeFirst().get()
        }

        return try JSONDecoder().decode(JarvisReleaseRunbook.self, from: releaseRunbookJSON(runbook: "live_device"))
    }

    func releaseSignedDistributionRunbook() async throws -> JarvisReleaseRunbook {
        if !releaseSignedDistributionRunbookResults.isEmpty {
            return try releaseSignedDistributionRunbookResults.removeFirst().get()
        }

        return try JSONDecoder().decode(JarvisReleaseRunbook.self, from: releaseRunbookJSON(runbook: "signed_distribution"))
    }

    func releasePluginTrustRunbook() async throws -> JarvisReleaseRunbook {
        if !releasePluginTrustRunbookResults.isEmpty {
            return try releasePluginTrustRunbookResults.removeFirst().get()
        }

        return try JSONDecoder().decode(JarvisReleaseRunbook.self, from: releaseRunbookJSON(runbook: "plugin_trust"))
    }

    func releaseEvidenceBundleRunbook() async throws -> JarvisReleaseRunbook {
        if !releaseEvidenceBundleRunbookResults.isEmpty {
            return try releaseEvidenceBundleRunbookResults.removeFirst().get()
        }

        return try JSONDecoder().decode(JarvisReleaseRunbook.self, from: releaseRunbookJSON(runbook: "evidence_bundle"))
    }

    func submit(_ command: JarvisCommandRequest) async throws -> JarvisCommandResponse {
        submittedCommands.append(command)
        if commandSubmissionDelayNanoseconds > 0 {
            try await Task.sleep(nanoseconds: commandSubmissionDelayNanoseconds)
        }
        let call = releaseSmokePaused ? "submitPaused" : "submitInitial"
        try recordReleaseSmokeCall(call)
        let response = try JSONDecoder().decode(
            JarvisCommandResponse.self,
            from: releaseSmokePaused
                ? releaseSmokeBlockedCommandResponseJSON(input: command.input)
                : commandResponseJSON(input: command.input, status: commandStatus)
        )
        if releaseSmokeMode {
            tasks.append(response.task)
            auditEntries.append(response.auditEntry)
            auditEntries.append(contentsOf: response.auditEntries)
        }
        return response
    }

    func cancelCommand(cancellationID: UUID) async throws -> JarvisRuntimeCancellationResponse {
        commandCancellationRequests.append(cancellationID)
        if commandCancellationDelayNanoseconds > 0 {
            try await Task.sleep(nanoseconds: commandCancellationDelayNanoseconds)
        }
        return try JSONDecoder().decode(
            JarvisRuntimeCancellationResponse.self,
            from: Data(
                """
                {
                  "cancellation_id": "\(cancellationID.uuidString)",
                  "cancellation_requested": true,
                  "active_execution_found": true,
                  "outcome": "cancellation_requested",
                  "audit_entry": {
                    "id": "11111111-1111-4111-8111-111111111111",
                    "task_id": null,
                    "event_type": "runtime_cancellation_requested",
                    "summary": "runtime cancellation was requested through local IPC",
                    "payload": { "request_content_redacted": true },
                    "created_at": "2026-07-14T12:00:00Z"
                  }
                }
                """.utf8
            )
        )
    }

    func pause(reason: String) async throws -> JarvisPauseResponse {
        guard releaseSmokeMode else { throw URLError(.unsupportedURL) }
        try recordReleaseSmokeCall("pause")
        releaseSmokePaused = true
        return releaseSmokePauseResponse(paused: true, reason: reason)
    }

    func resume() async throws -> JarvisPauseResponse {
        guard releaseSmokeMode else { throw URLError(.unsupportedURL) }
        try recordReleaseSmokeCall("resume")
        releaseSmokePaused = false
        return releaseSmokePauseResponse(paused: false, reason: nil)
    }

    func pauseStatus() async throws -> JarvisPauseResponse {
        guard releaseSmokeMode else { throw URLError(.unsupportedURL) }
        try recordReleaseSmokeCall(releaseSmokePaused ? "pauseStatusPaused" : "pauseStatusResumed")
        return releaseSmokePauseResponse(
            paused: releaseSmokePaused,
            reason: releaseSmokePaused ? "release smoke" : nil
        )
    }

    func listTasks() async throws -> [JarvisTask] {
        try recordReleaseSmokeCall("listTasks")
        return tasks
    }

    func task(id: UUID) async throws -> JarvisTask {
        guard releaseSmokeMode else { throw URLError(.unsupportedURL) }
        try recordReleaseSmokeCall("task")
        guard let task = tasks.first(where: { $0.id == id }) else {
            throw URLError(.fileDoesNotExist)
        }
        return task
    }

    func listAuditEntries(taskId: UUID?) async throws -> [JarvisAuditEntry] {
        try recordReleaseSmokeCall(taskId == nil ? "schedulerAudit" : "taskAudit")
        if let taskId {
            return auditEntries.filter { $0.taskId == taskId }
        }
        return auditEntries
    }

    func activitySummary() async throws -> JarvisActivitySummary {
        let activeStatuses = Set(["created", "running", "waiting_for_approval"])
        let grouped = Dictionary(grouping: tasks, by: \.status)
        let statusCounts = grouped
            .map { status, tasks in
                JarvisActivityStatusCount(status: status, count: tasks.count)
            }
            .sorted { $0.status < $1.status }
        return JarvisActivitySummary(
            generatedAt: "2026-05-20T12:00:03Z",
            repositoryBacked: true,
            taskCount: tasks.count,
            auditEntryCount: auditEntries.count,
            activeTaskCount: tasks.filter { activeStatuses.contains($0.status) }.count,
            statusCounts: statusCounts,
            recentTasks: tasks.suffix(5).reversed().map {
                JarvisActivityTaskSummary(
                    id: $0.id,
                    sessionId: $0.sessionId,
                    status: $0.status,
                    createdAt: $0.createdAt ?? "2026-05-20T12:00:00Z",
                    updatedAt: $0.updatedAt ?? "2026-05-20T12:00:00Z"
                )
            },
            recentAuditEntries: Array(auditEntries.suffix(10).reversed())
        )
    }

    func activityEvents(maxEvents: Int, intervalMilliseconds: Int) async throws -> [JarvisActivityEvent] {
        if !activityEvents.isEmpty {
            return activityEvents
        }

        let summary = try await activitySummary()
        return [
            JarvisActivityEvent(sequence: 0, event: "activity_summary", summary: summary, progress: nil, error: nil)
        ]
    }

    func listMemoryItems(includeDeleted: Bool) async throws -> [JarvisMemoryItem] {
        includeDeletedMemoryRequests.append(includeDeleted)
        if includeDeleted {
            return memoryItems
        }
        return memoryItems.filter { $0.deletedAt == nil }
    }

    func memoryClassification(includeDeleted: Bool) async throws -> JarvisMemoryClassificationSummary {
        try JSONDecoder().decode(
            JarvisMemoryClassificationSummary.self,
            from: Data(
                """
                {
                  "generated_at": "2026-05-20T12:00:02Z",
                  "include_deleted": true,
                  "total_count": 2,
                  "active_count": 1,
                  "deleted_count": 1,
                  "reviewed_count": 0,
                  "unreviewed_active_count": 1,
                  "sensitive_active_count": 1,
                  "by_sensitivity": [
                    {
                      "label": "private",
                      "count": 1,
                      "active_count": 1,
                      "deleted_count": 0,
                      "unreviewed_active_count": 1
                    }
                  ],
                  "by_category": [
                    {
                      "label": "release",
                      "count": 2,
                      "active_count": 1,
                      "deleted_count": 1,
                      "unreviewed_active_count": 1
                    }
                  ]
                }
                """.utf8
            )
        )
    }

    func memoryIndexStatus() async throws -> JarvisMemoryIndexStatus {
        try fakeMemoryIndexStatus(state: "stale")
    }

    func rebuildMemoryIndex() async throws -> JarvisMemoryIndexStatus {
        try fakeMemoryIndexStatus(state: "current")
    }

    private func fakeMemoryIndexStatus(state: String) throws -> JarvisMemoryIndexStatus {
        try JSONDecoder().decode(
            JarvisMemoryIndexStatus.self,
            from: Data(
                """
                {
                  "generated_at": "2026-05-20T12:00:00Z",
                  "state": "\(state)",
                  "index_version": 1,
                  "rebuilt_at": "2026-05-20T11:59:00Z",
                  "active_record_count": 1,
                  "indexed_entry_count": 1,
                  "current_entry_count": \(state == "current" ? 1 : 0),
                  "missing_entry_count": 0,
                  "stale_entry_count": \(state == "stale" ? 1 : 0),
                  "orphaned_entry_count": 0,
                  "deleted_projection_count": 0,
                  "canonical_source": "sqlite_memory_items",
                  "retrieval_enabled": false,
                  "redaction": "count-only",
                  "detail": "safe status"
                }
                """.utf8
            )
        )
    }

    func memoryRetentionPlan() async throws -> JarvisMemoryRetentionPlan {
        if let memoryRetentionPlanResult {
            return memoryRetentionPlanResult
        }

        return try JSONDecoder().decode(
            JarvisMemoryRetentionPlan.self,
            from: fakeMemoryRetentionPlanJSON(id: memoryItems.last?.id ?? UUID())
        )
    }

    func createMemoryItem(_ request: JarvisCreateMemoryItemRequest) async throws -> JarvisMemoryItem {
        createdMemoryRequests.append(request)
        let item = sampleMemoryItem(
            category: request.category,
            key: request.key,
            value: request.value,
            provenance: request.provenance,
            sensitivity: request.sensitivity
        )
        memoryItems.insert(item, at: 0)
        return item
    }

    func memoryItem(id: UUID) async throws -> JarvisMemoryItem {
        guard let item = memoryItems.first(where: { $0.id == id }) else {
            throw URLError(.fileDoesNotExist)
        }
        return item
    }

    func updateMemoryItem(id: UUID, request: JarvisMemoryMutationRequest) async throws -> JarvisMemoryItem {
        updatedMemoryRequests.append((id: id, request: request))
        guard let index = memoryItems.firstIndex(where: { $0.id == id }) else {
            throw URLError(.fileDoesNotExist)
        }
        var item = memoryItems[index]
        item.value = request.value
        item.provenance = request.provenance
        item.sensitivity = request.sensitivity
        item.updatedAt = "2026-05-20T12:10:00Z"
        memoryItems[index] = item
        return item
    }

    func reviewMemoryItem(id: UUID) async throws -> JarvisMemoryItem {
        guard let index = memoryItems.firstIndex(where: { $0.id == id }) else {
            throw URLError(.fileDoesNotExist)
        }
        var item = memoryItems[index]
        item.reviewedAt = "2026-05-20T12:11:00Z"
        item.updatedAt = "2026-05-20T12:11:00Z"
        memoryItems[index] = item
        return item
    }

    func deleteMemoryItem(id: UUID) async throws -> JarvisMemoryItem {
        guard let index = memoryItems.firstIndex(where: { $0.id == id }) else {
            throw URLError(.fileDoesNotExist)
        }
        var item = memoryItems[index]
        item.deletedAt = "2026-05-20T12:12:00Z"
        item.updatedAt = "2026-05-20T12:12:00Z"
        memoryItems[index] = item
        return item
    }

    func restoreMemoryItem(id: UUID) async throws -> JarvisMemoryItem {
        guard let index = memoryItems.firstIndex(where: { $0.id == id }) else {
            throw URLError(.fileDoesNotExist)
        }
        var item = memoryItems[index]
        item.deletedAt = nil
        item.updatedAt = "2026-05-20T12:13:00Z"
        memoryItems[index] = item
        return item
    }

    func listPluginManifests() async throws -> [JarvisPluginManifest] {
        pluginManifests
    }

    func modelToolCatalog() async throws -> JarvisModelToolCatalog {
        if modelToolCatalogUnavailable {
            throw URLError(.cannotConnectToHost)
        }
        return modelToolCatalogResult
    }

    func listInstalledPlugins() async throws -> [JarvisInstalledPluginRecord] {
        if installedPluginsUnavailable {
            throw URLError(.cannotConnectToHost)
        }
        return installedPlugins
    }

    func listSchedulerJobs() async throws -> [JarvisSchedulerJob] {
        schedulerJobs
    }

    func schedulerAttention() async throws -> JarvisSchedulerAttentionSummary {
        if schedulerAttentionDelayNanoseconds > 0 {
            try await Task.sleep(nanoseconds: schedulerAttentionDelayNanoseconds)
        }
        if let schedulerAttentionResponse {
            return schedulerAttentionResponse
        }
        return try JSONDecoder().decode(
            JarvisSchedulerAttentionSummary.self,
            from: Data(
                """
                {
                  "generated_at": "2026-05-20T12:00:02Z",
                  "emergency_paused": false,
                  "attention_required": false,
                  "due_count": 0,
                  "scheduled_count": 0,
                  "running_count": 0,
                  "failed_count": 0,
                  "next_due_at": null,
                  "items": []
                }
                """.utf8
            )
        )
    }

    func pendingSchedulerNotificationOccurrences(
        limit: Int
    ) async throws -> [JarvisSchedulerNotificationOccurrence] {
        try recordReleaseSmokeCall("schedulerNotifications")
        return Array(schedulerNotificationOccurrences.prefix(max(1, min(64, limit))))
    }

    func acknowledgeSchedulerNotificationOccurrence(
        id: UUID,
        request: JarvisSchedulerNotificationAcknowledgementRequest
    ) async throws -> JarvisSchedulerNotificationAcknowledgementResponse {
        try recordReleaseSmokeCall("schedulerNotificationAck")
        schedulerNotificationAcknowledgementIDs.append(id)
        schedulerNotificationAcknowledgements.append(request)
        if schedulerNotificationAcknowledgementFailureIDs.contains(id) {
            throw URLError(.cannotWriteToFile)
        }
        guard let index = schedulerNotificationOccurrences.firstIndex(where: { $0.id == id }),
              schedulerNotificationOccurrences[index].revision == request.revision else {
            throw URLError(.resourceUnavailable)
        }
        var occurrence = schedulerNotificationOccurrences.remove(at: index)
        occurrence.acknowledgedAt = "2026-07-14T12:00:03Z"
        occurrence.acknowledgedDisposition = request.disposition.rawValue
        return JarvisSchedulerNotificationAcknowledgementResponse(
            occurrence: occurrence,
            proofBoundary: "test acknowledgement"
        )
    }

    func schedulerJob(id: UUID) async throws -> JarvisSchedulerJob {
        guard releaseSmokeMode,
              let index = schedulerJobs.firstIndex(where: { $0.id == id })
        else {
            throw URLError(.unsupportedURL)
        }
        try recordReleaseSmokeCall("schedulerJob")
        schedulerJobs[index].status = "completed"
        schedulerJobs[index].updatedAt = "2026-07-14T12:00:02Z"
        if !auditEntries.contains(where: { $0.eventType == "scheduler_job_completed" }) {
            auditEntries.append(
                JarvisAuditEntry(
                    id: UUID(),
                    taskId: UUID(),
                    eventType: "scheduler_job_completed",
                    summary: "scheduler finished due job command",
                    payload: .object(["scheduler_job_id": .string(id.uuidString)]),
                    createdAt: "2026-07-14T12:00:02Z"
                )
            )
        }
        if !schedulerNotificationOccurrences.contains(where: { $0.schedulerJobId == id }) {
            schedulerNotificationOccurrences.append(
                JarvisSchedulerNotificationOccurrence(
                    id: UUID(),
                    schedulerJobId: id,
                    name: schedulerJobs[index].name,
                    occurrenceAt: "2026-07-14T12:00:01Z",
                    notificationKind: "due_now",
                    revision: 1,
                    createdAt: "2026-07-14T12:00:01Z",
                    updatedAt: "2026-07-14T12:00:01Z",
                    acknowledgedAt: nil,
                    acknowledgedDisposition: nil
                )
            )
        }
        return schedulerJobs[index]
    }

    func createSchedulerJob(_ request: JarvisCreateSchedulerJobRequest) async throws -> JarvisSchedulerJob {
        guard releaseSmokeMode else { throw URLError(.unsupportedURL) }
        try recordReleaseSmokeCall("schedulerCreate")
        let job = JarvisSchedulerJob(
            id: UUID(),
            name: request.name,
            command: request.command,
            trigger: request.trigger,
            status: "scheduled",
            createdAt: "2026-07-14T12:00:01Z",
            updatedAt: "2026-07-14T12:00:01Z",
            cancelledAt: nil,
            cancellationReason: nil
        )
        schedulerJobs.append(job)
        return job
    }

    func cancelSchedulerJob(id: UUID) async throws -> JarvisSchedulerJob {
        throw URLError(.unsupportedURL)
    }

    func runDueSchedulerJobs(limit: Int) async throws -> JarvisSchedulerRunResponse {
        runDueSchedulerLimits.append(limit)
        let id = schedulerJobs.first?.id ?? UUID()
        return try JSONDecoder().decode(JarvisSchedulerRunResponse.self, from: fakeSchedulerRunDueJSON(id: id))
    }

    func recoverStaleSchedulerJobs(
        olderThanSeconds: UInt64,
        limit: Int
    ) async throws -> JarvisSchedulerStaleRecoveryResponse {
        recoverStaleSchedulerRequests.append((olderThanSeconds: olderThanSeconds, limit: limit))
        let id = schedulerJobs.first?.id ?? UUID()
        return try JSONDecoder().decode(
            JarvisSchedulerStaleRecoveryResponse.self,
            from: fakeSchedulerRecoverStaleJSON(id: id)
        )
    }

    func trustedWakeStatus() async throws -> JarvisTrustedWakeStatus {
        if !trustedWakeStatusResults.isEmpty {
            return try trustedWakeStatusResults.removeFirst().get()
        }
        return try JSONDecoder().decode(
            JarvisTrustedWakeStatus.self,
            from: Data(
                """
                {
                  "schema_version": 1,
                  "session_id": "00000000-0000-4000-8000-000000000111",
                  "challenge": "challenge",
                  "rule": {
                    "id": "4a617276-6973-4000-8000-000000000010",
                    "enabled": true,
                    "key_fingerprint": "redacted-fingerprint",
                    "generation": 1,
                    "highest_counter": 4,
                    "created_at": "2026-07-13T10:00:00.123456Z",
                    "updated_at": "2026-07-13T10:00:01Z"
                  },
                  "attention_required": false,
                  "ambiguous_dispatch_count": 0,
                  "proof_boundary": "local contract only"
                }
                """.utf8
            )
        )
    }

    func trustedWakeAttention() async throws -> [JarvisTrustedWakeAttentionItem] {
        trustedWakeAttentionItems
    }

    func resolveTrustedWakeAttention(
        id: UUID,
        request: JarvisTrustedWakeResolutionRequest
    ) async throws -> JarvisTrustedWakeAttentionItem {
        trustedWakeResolutionRequests.append((id: id, request: request))
        guard let index = trustedWakeAttentionItems.firstIndex(where: { $0.eventId == id }) else {
            throw URLError(.fileDoesNotExist)
        }
        var item = trustedWakeAttentionItems.remove(at: index)
        item.state = "blocked"
        return item
    }

    func setTrustedWakeEnabled(
        _ request: JarvisTrustedWakeRuleEnablement
    ) async throws -> JarvisTrustedWakeRule {
        throw URLError(.unsupportedURL)
    }

    func prepareTrustedWakeKeyControl(
        _ request: JarvisTrustedWakeKeyControlPrepareRequest
    ) async throws -> JarvisTrustedWakeKeyControlPrepareResponse {
        if let trustedWakePrepareResult { return try trustedWakePrepareResult.get() }
        throw URLError(.unsupportedURL)
    }

    func cancelTrustedWakeKeyControl(
        _ request: JarvisTrustedWakeKeyControlCancelRequest
    ) async throws -> JarvisTrustedWakeRule {
        if !trustedWakeCancelResults.isEmpty {
            return try trustedWakeCancelResults.removeFirst().get()
        }
        throw URLError(.unsupportedURL)
    }

    func submitTrustedWake(
        _ envelope: JarvisTrustedWakeEnvelope
    ) async throws -> JarvisTrustedWakeEventResponse {
        trustedWakeSubmitCount += 1
        return try JSONDecoder().decode(
            JarvisTrustedWakeEventResponse.self,
            from: Data(
                """
                {
                  "event": {
                    "id": "00000000-0000-4000-8000-000000000222",
                    "rule_id": "4a617276-6973-4000-8000-000000000010",
                    "counter": 5,
                    "state": "completed",
                    "task_id": "00000000-0000-4000-8000-000000000223",
                    "scheduler_job_id": "00000000-0000-4000-8000-000000000224"
                  },
                  "idempotent_retry": false,
                  "execution": null,
                  "proof_boundary": "local contract only"
                }
                """.utf8
            )
        )
    }

    private func fakeSchedulerRunDueJSON(id: UUID) -> Data {
        Data(
            """
            {
              "checked_at": "2026-05-20T12:00:04Z",
              "limit": 4,
              "emergency_paused": false,
              "executions": [
                {
                  "job": {
                    "id": "\(id.uuidString)",
                    "name": "one shot",
                    "command": "status check",
                    "trigger": "manual",
                    "status": "completed",
                    "created_at": "2026-05-20T12:00:00Z",
                    "updated_at": "2026-05-20T12:00:04Z",
                    "cancelled_at": null,
                    "cancellation_reason": null
                  },
                  "task": {
                    "id": "\(id.uuidString)",
                    "session_id": "\(UUID().uuidString)",
                    "user_input": "status check",
                    "status": "completed",
                    "created_at": "2026-05-20T12:00:03Z",
                    "updated_at": "2026-05-20T12:00:04Z"
                  },
                  "accepted": true,
                  "message": "scheduled command completed",
                  "audit_entries": []
                }
              ]
            }
            """.utf8
        )
    }

    private func fakeSchedulerRecoverStaleJSON(id: UUID) -> Data {
        Data(
            """
            {
              "checked_at": "2026-05-20T12:00:05Z",
              "older_than_seconds": 120,
              "limit": 2,
              "recovered": [
                {
                  "job": {
                    "id": "\(id.uuidString)",
                    "name": "stale running job",
                    "trigger": "manual",
                    "status": "failed",
                    "created_at": "2026-05-20T11:00:00Z",
                    "updated_at": "2026-05-20T12:00:05Z",
                    "cancelled_at": null,
                    "cancellation_reason_present": false
                  },
                  "stale_since": "2026-05-20T11:55:00Z",
                  "stale_for_seconds": 300,
                  "audit_entry": {
                    "id": "\(UUID().uuidString)",
                    "task_id": null,
                    "event_type": "scheduler_stale_running_recovered",
                    "summary": "scheduler marked a stale running job failed for explicit operator recovery",
                    "payload": { "command_redacted": true },
                    "created_at": "2026-05-20T12:00:05Z"
                  }
                }
              ]
            }
            """.utf8
        )
    }

    func diagnosticsExport() async throws -> JarvisDiagnosticsExport {
        try recordReleaseSmokeCall("diagnostics")
        return try JSONDecoder().decode(JarvisDiagnosticsExport.self, from: diagnosticsJSON())
    }

    private func recordReleaseSmokeCall(_ call: String) throws {
        guard releaseSmokeMode else { return }
        releaseSmokeCalls.append(call)
        if releaseSmokeFailureCall == call {
            throw URLError(.cannotConnectToHost)
        }
    }

    func permissionGrantSummary() async throws -> JarvisPermissionGrantSummary {
        if let permissionGrantSummaryResult {
            return permissionGrantSummaryResult
        }

        return try JSONDecoder().decode(
            JarvisPermissionGrantSummary.self,
            from: permissionGrantSummaryJSON()
        )
    }

    func permissionPolicyReview() async throws -> JarvisPermissionPolicyReview {
        try JSONDecoder().decode(
            JarvisPermissionPolicyReview.self,
            from: permissionPolicyReviewJSON()
        )
    }

    func listApprovals(status: String?) async throws -> [JarvisPendingApproval] {
        guard let status else {
            return approvals
        }
        return approvals.filter { $0.status == status }
    }

    func approval(id: UUID) async throws -> JarvisPendingApproval {
        guard let approval = approvals.first(where: { $0.id == id }) else {
            throw URLError(.badServerResponse)
        }
        return approval
    }

    func approveApproval(
        id: UUID,
        request: JarvisApprovalDecisionRequest
    ) async throws -> JarvisPendingApproval {
        try decideApproval(id: id, approved: true, request: request)
    }

    func denyApproval(
        id: UUID,
        request: JarvisApprovalDecisionRequest
    ) async throws -> JarvisPendingApproval {
        try decideApproval(id: id, approved: false, request: request)
    }

    func executeApproval(
        id: UUID,
        cancellationID: UUID
    ) async throws -> JarvisApprovalExecutionResponse {
        guard let approval = approvals.first(where: { $0.id == id }) else {
            throw URLError(.badServerResponse)
        }
        approvalExecutionCancellationIDs.append(cancellationID)
        if approvalExecutionDelayNanoseconds > 0 {
            try await Task.sleep(nanoseconds: approvalExecutionDelayNanoseconds)
        }
        approvalExecutions.append(id)
        auditEntries.append(
            JarvisAuditEntry(
                id: UUID(),
                taskId: approval.taskId,
                eventType: "approval_executed",
                summary: "approved action execution completed",
                payload: .object(["approval_id": .string(approval.id.uuidString)]),
                createdAt: "2026-05-20T12:15:00Z"
            )
        )
        return try JSONDecoder().decode(
            JarvisApprovalExecutionResponse.self,
            from: approval.action == "local_installed.confirm_action"
                ? installedApprovalExecutionJSON(approval: approval)
                : approvalExecutionJSON(approvalId: approval.id, taskId: approval.taskId)
        )
    }

    private func decideApproval(
        id: UUID,
        approved: Bool,
        request: JarvisApprovalDecisionRequest
    ) throws -> JarvisPendingApproval {
        guard let index = approvals.firstIndex(where: { $0.id == id }) else {
            throw URLError(.badServerResponse)
        }
        let approval = approvals[index]

        approvalDecisions.append(ApprovalDecision(id: id, approved: approved, reason: request.reason))
        let decidedApproval = samplePendingApproval(
            id: approval.id,
            taskId: approval.taskId,
            action: approval.action,
            status: approved ? "approved" : "denied",
            decidedBy: request.decidedBy,
            decisionReason: request.reason
        )
        approvals[index] = decidedApproval
        return decidedApproval
    }
}

private func decodeRequestBody(_ request: URLRequest) -> [String: Any]? {
    if let body = request.httpBody {
        return try? JSONSerialization.jsonObject(with: body) as? [String: Any]
    }

    guard let stream = request.httpBodyStream else {
        return nil
    }

    stream.open()
    defer { stream.close() }

    var data = Data()
    var buffer = [UInt8](repeating: 0, count: 1024)
    while stream.hasBytesAvailable {
        let count = stream.read(&buffer, maxLength: buffer.count)
        if count <= 0 {
            break
        }
        data.append(buffer, count: count)
    }

    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
}

private func runJarvisCLIJSON(_ args: [String]) throws -> Data {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = ["cargo", "run", "-p", "jarvis-cli", "--"] + args
    process.currentDirectoryURL = jarvisRepositoryRoot()

    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr

    try process.run()
    process.waitUntilExit()

    let output = stdout.fileHandleForReading.readDataToEndOfFile()
    let errorOutput = stderr.fileHandleForReading.readDataToEndOfFile()
    if process.terminationStatus != 0 {
        let stderrText = String(decoding: errorOutput, as: UTF8.self)
        let stdoutText = String(decoding: output, as: UTF8.self)
        throw JarvisCLISmokeError(
            status: process.terminationStatus,
            stdout: stdoutText,
            stderr: stderrText
        )
    }
    return output
}

private func unusedLoopbackPort() throws -> UInt16 {
    let descriptor = Darwin.socket(AF_INET, SOCK_STREAM, 0)
    guard descriptor >= 0 else { throw POSIXError(.ENOTSOCK) }
    defer { Darwin.close(descriptor) }

    var address = sockaddr_in()
    address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
    address.sin_family = sa_family_t(AF_INET)
    address.sin_port = 0
    address.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))
    let bindResult = withUnsafePointer(to: &address) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
            Darwin.bind(descriptor, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
        }
    }
    guard bindResult == 0 else { throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EADDRINUSE) }

    var length = socklen_t(MemoryLayout<sockaddr_in>.size)
    let nameResult = withUnsafeMutablePointer(to: &address) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
            Darwin.getsockname(descriptor, $0, &length)
        }
    }
    guard nameResult == 0 else { throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EINVAL) }
    return UInt16(bigEndian: address.sin_port)
}

private func noopVoiceCaptureCallbacks() -> JarvisVoiceCaptureCallbacks {
    JarvisVoiceCaptureCallbacks(
        onPartialTranscript: { _ in },
        onFinalTranscript: { _ in },
        onError: { _ in }
    )
}

private func jarvisRepositoryRoot() -> URL {
    var url = URL(fileURLWithPath: #filePath)
    for _ in 0..<5 {
        url.deleteLastPathComponent()
    }
    return url
}

private struct JarvisCLIRunbookEvidenceItem: Decodable {
    var key: String
    var status: String
    var kind: String
    var path: String
    var manualGate: Bool
    var requiredForProduction: Bool

    enum CodingKeys: String, CodingKey {
        case key
        case status
        case kind
        case path
        case manualGate = "manual_gate"
        case requiredForProduction = "required_for_production"
    }
}

private struct JarvisCLIRunbookFeature: Decodable {
    var key: String
    var status: String
}

private struct JarvisCLILiveDeviceRunbook: Decodable {
    var generatedFrom: String
    var productionReady: Bool
    var liveDeviceEvidence: JarvisCLIRunbookEvidenceItem
    var liveVoiceFeature: JarvisCLIRunbookFeature
    var commands: [String]
    var manualChecks: [String]
    var proofBoundary: String

    enum CodingKeys: String, CodingKey {
        case generatedFrom = "generated_from"
        case productionReady = "production_ready"
        case liveDeviceEvidence = "live_device_evidence"
        case liveVoiceFeature = "live_voice_feature"
        case commands
        case manualChecks = "manual_checks"
        case proofBoundary = "proof_boundary"
    }
}

private struct JarvisCLISignedDistributionRunbook: Decodable {
    var generatedFrom: String
    var productionReady: Bool
    var distributionEvidence: [JarvisCLIRunbookEvidenceItem]
    var commands: [String]
    var manualChecks: [String]
    var proofBoundary: String

    enum CodingKeys: String, CodingKey {
        case generatedFrom = "generated_from"
        case productionReady = "production_ready"
        case distributionEvidence = "distribution_evidence"
        case commands
        case manualChecks = "manual_checks"
        case proofBoundary = "proof_boundary"
    }
}

private struct JarvisCLIPluginTrustRunbook: Decodable {
    var generatedFrom: String
    var productionReady: Bool
    var pluginTrustEvidence: JarvisCLIRunbookEvidenceItem
    var commands: [String]
    var manualChecks: [String]
    var proofBoundary: String

    enum CodingKeys: String, CodingKey {
        case generatedFrom = "generated_from"
        case productionReady = "production_ready"
        case pluginTrustEvidence = "plugin_trust_evidence"
        case commands
        case manualChecks = "manual_checks"
        case proofBoundary = "proof_boundary"
    }
}

private struct JarvisCLIEvidenceBundleRunbook: Decodable {
    var generatedFrom: String
    var productionReady: Bool
    var childEvidence: [JarvisCLIRunbookEvidenceItem]
    var finalBundleEvidence: JarvisCLIRunbookEvidenceItem
    var commands: [String]
    var manualChecks: [String]
    var proofBoundary: String

    enum CodingKeys: String, CodingKey {
        case generatedFrom = "generated_from"
        case productionReady = "production_ready"
        case childEvidence = "child_evidence"
        case finalBundleEvidence = "final_bundle_evidence"
        case commands
        case manualChecks = "manual_checks"
        case proofBoundary = "proof_boundary"
    }
}

private struct JarvisCLISmokeError: Error, CustomStringConvertible {
    var status: Int32
    var stdout: String
    var stderr: String

    var description: String {
        "jarvis CLI smoke failed with status \(status)\nstdout:\n\(stdout)\nstderr:\n\(stderr)"
    }
}

private func makeUnixSocketAuthorization(
    socketDirectoryURL: URL,
    tokenFileRoot: URL,
    socketIdentifier: @escaping @Sendable () throws -> String
) -> JarvisIPCSessionAuthorization {
    JarvisIPCSessionAuthorization(
        mode: .appSupervised,
        tokenFileURL: tokenFileRoot.appending(path: "unused-auth.json"),
        transportMode: .unixSocket,
        socketDirectoryPath: socketDirectoryURL.path,
        randomBytes: DeterministicAuthRandom().bytes,
        socketIdentifier: socketIdentifier
    )
}

private func samplePeerIdentityPolicy() -> JarvisIPCPeerIdentityPolicy {
    JarvisIPCPeerIdentityPolicy(
        profile: .adhocExact,
        peerCodeRequirement: "identifier \"com.nobiletechnology.jarvis\"",
        coreCodeRequirement: "identifier \"com.nobiletechnology.jarvis.core\"",
        expectedCoreCDHash: Data(repeating: 0x42, count: 20),
        expectedCoreExecutableURL: URL(fileURLWithPath: "/tmp/jarvis-cli")
    )
}

private struct FakePeerIdentityPolicyProvider: JarvisIPCPeerIdentityPolicyProviding {
    var shouldFail = false

    func policy(forCoreExecutable executableURL: URL) throws -> JarvisIPCPeerIdentityPolicy {
        if shouldFail {
            throw JarvisIPCPeerIdentityError.unavailable
        }
        return samplePeerIdentityPolicy()
    }
}

private struct AcceptingUnixPeerIdentityVerifier: JarvisUnixPeerIdentityVerifying {
    func verifyPeer(
        on socketDescriptor: Int32,
        policy: JarvisIPCPeerIdentityPolicy
    ) throws {}
}

private struct RejectingUnixPeerIdentityVerifier: JarvisUnixPeerIdentityVerifying {
    func verifyPeer(
        on socketDescriptor: Int32,
        policy: JarvisIPCPeerIdentityPolicy
    ) throws {
        throw JarvisIPCPeerIdentityError.peerCodeInvalid
    }
}

private final class ControlledUnixPeerIdentityVerifier: JarvisUnixPeerIdentityVerifying,
    @unchecked Sendable
{
    let started = DispatchSemaphore(value: 0)
    let proceed = DispatchSemaphore(value: 0)

    func verifyPeer(
        on socketDescriptor: Int32,
        policy: JarvisIPCPeerIdentityPolicy
    ) throws {
        started.signal()
        guard proceed.wait(timeout: .now() + 2) == .success else {
            throw JarvisIPCPeerIdentityError.peerCodeUnavailable
        }
    }
}

private func testUnixSocketTransport(timeoutSeconds: Int) -> DarwinJarvisUnixSocketTransport {
    DarwinJarvisUnixSocketTransport(
        timeoutSeconds: timeoutSeconds,
        peerIdentityPolicy: { samplePeerIdentityPolicy() },
        peerIdentityVerifier: AcceptingUnixPeerIdentityVerifier()
    )
}

private func shortUnixTestDirectory() throws -> URL {
    let suffix = UUID().uuidString.prefix(8).lowercased()
    let directory = URL(fileURLWithPath: "/tmp/juds-\(suffix)", isDirectory: true)
    try FileManager.default.createDirectory(
        at: directory,
        withIntermediateDirectories: false,
        attributes: [.posixPermissions: NSNumber(value: Int16(0o700))]
    )
    return directory
}

private func assertUnsafeUnixSocketDirectory(
    _ socketDirectoryURL: URL,
    tokenFileRoot: URL
) throws {
    let authorization = makeUnixSocketAuthorization(
        socketDirectoryURL: socketDirectoryURL,
        tokenFileRoot: tokenFileRoot,
        socketIdentifier: { "unsafe" }
    )
    let launchValue = try authorization.rotateForLaunch(
        peerIdentityPolicy: samplePeerIdentityPolicy()
    )
    let launch = try #require(launchValue)
    #expect(throws: JarvisIPCAuthorizationError.unixSocketParentUnsafe) {
        _ = try authorization.prepareUnixSocketForLaunch(generation: launch.generation)
    }
}

private final class LockedTestValue<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var storage: Value

    init(_ value: Value) {
        storage = value
    }

    var value: Value {
        lock.lock()
        defer { lock.unlock() }
        return storage
    }

    func set(_ value: Value) {
        lock.lock()
        storage = value
        lock.unlock()
    }
}

private final class UnixSocketTestServer: @unchecked Sendable {
    let socketURL: URL
    private let listener: Int32
    private let accepted = LockedTestValue(false)
    private let handler: @Sendable (Int32) -> Void

    init(socketURL: URL, handler: @escaping @Sendable (Int32) -> Void) throws {
        self.socketURL = socketURL
        self.handler = handler
        listener = try bindUnixSocketForTest(at: socketURL)
        guard Darwin.listen(listener, 1) == 0 else {
            Darwin.close(listener)
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .ENOTSUP)
        }
        DispatchQueue.global(qos: .userInitiated).async { [self] in
            var client: Int32
            repeat {
                client = Darwin.accept(listener, nil, nil)
            } while client < 0 && errno == EINTR
            guard client >= 0 else { return }
            var enabled: Int32 = 1
            _ = Darwin.setsockopt(
                client,
                SOL_SOCKET,
                SO_NOSIGPIPE,
                &enabled,
                socklen_t(MemoryLayout.size(ofValue: enabled))
            )
            var receiveTimeout = timeval(tv_sec: 2, tv_usec: 0)
            _ = Darwin.setsockopt(
                client,
                SOL_SOCKET,
                SO_RCVTIMEO,
                &receiveTimeout,
                socklen_t(MemoryLayout.size(ofValue: receiveTimeout))
            )
            accepted.set(true)
            handler(client)
            Darwin.close(client)
        }
    }

    var didAccept: Bool { accepted.value }

    deinit {
        Darwin.close(listener)
        _ = Darwin.unlink(socketURL.path)
    }
}

private func waitForUnixTestServerAccept(_ server: UnixSocketTestServer) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(2))
    while !server.didAccept {
        guard ContinuousClock.now < deadline else {
            throw POSIXError(.ETIMEDOUT)
        }
        try await Task.sleep(for: .milliseconds(5))
    }
}

private func bindUnixSocketForTest(at socketURL: URL) throws -> Int32 {
    guard socketURL.isFileURL, socketURL.path.hasPrefix("/") else {
        throw POSIXError(.EINVAL)
    }
    let pathBytes = Array(socketURL.path.utf8)
    guard !pathBytes.isEmpty, pathBytes.count < 104 else {
        throw POSIXError(.ENAMETOOLONG)
    }
    let descriptor = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
    guard descriptor >= 0 else {
        throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .ENOTSUP)
    }
    var address = sockaddr_un()
    let addressLength = MemoryLayout.offset(of: \sockaddr_un.sun_path)! + pathBytes.count + 1
    address.sun_len = UInt8(addressLength)
    address.sun_family = sa_family_t(AF_UNIX)
    withUnsafeMutableBytes(of: &address.sun_path) { destination in
        destination.copyBytes(from: pathBytes)
        destination[pathBytes.count] = 0
    }
    let result = withUnsafePointer(to: &address) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
            Darwin.bind(descriptor, $0, socklen_t(addressLength))
        }
    }
    guard result == 0 else {
        let code = errno
        Darwin.close(descriptor)
        throw POSIXError(POSIXErrorCode(rawValue: code) ?? .ENOTSUP)
    }
    return descriptor
}

private func readUnixTestFrame(_ descriptor: Int32) throws -> Data {
    let prefix = try readUnixTestBytes(descriptor, count: 4)
    var encodedLength: UInt32 = 0
    _ = withUnsafeMutableBytes(of: &encodedLength) { prefix.copyBytes(to: $0) }
    let frame = try readUnixTestBytes(
        descriptor,
        count: Int(UInt32(bigEndian: encodedLength))
    )
    var trailingByte: UInt8 = 0
    while true {
        let result = Darwin.read(descriptor, &trailingByte, 1)
        if result < 0, errno == EINTR { continue }
        guard result == 0 else {
            throw POSIXError(result < 0 ? .ETIMEDOUT : .EPROTO)
        }
        return frame
    }
}

private func readUnixTestBytes(_ descriptor: Int32, count: Int) throws -> Data {
    var data = Data(count: count)
    var offset = 0
    try data.withUnsafeMutableBytes { bytes in
        while offset < count {
            let result = Darwin.read(
                descriptor,
                bytes.baseAddress?.advanced(by: offset),
                count - offset
            )
            if result < 0, errno == EINTR { continue }
            guard result > 0 else { throw POSIXError(.ECONNRESET) }
            offset += result
        }
    }
    return data
}

private func writeUnixTestFrame(_ descriptor: Int32, _ frame: Data) throws {
    var length = UInt32(frame.count).bigEndian
    try withUnsafeBytes(of: &length) { bytes in
        try writeUnixTestBytes(descriptor, bytes)
    }
    try frame.withUnsafeBytes { bytes in
        try writeUnixTestBytes(descriptor, bytes)
    }
}

private func writeUnixTestBytes(
    _ descriptor: Int32,
    _ bytes: UnsafeRawBufferPointer
) throws {
    var offset = 0
    while offset < bytes.count {
        let result = Darwin.write(
            descriptor,
            bytes.baseAddress?.advanced(by: offset),
            bytes.count - offset
        )
        if result < 0, errno == EINTR { continue }
        guard result > 0 else { throw POSIXError(.EPIPE) }
        offset += result
    }
}
