import Foundation
import Testing
@testable import JarvisMacCore

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
            phase = .speaking
        case let .failure(error):
            phase = .unavailable(reason: error.description)
        }
        return speakResult
    }

    func stop() async -> Result<Void, JarvisSpeechOutputError> {
        switch stopResult {
        case .success:
            phase = .idle
        case let .failure(error):
            phase = .unavailable(reason: error.description)
        }
        return stopResult
    }

    func interrupt(reason: String) async -> Result<Void, JarvisSpeechOutputError> {
        switch interruptResult {
        case .success:
            phase = .interrupted(reason: reason)
        case let .failure(error):
            phase = .unavailable(reason: error.description)
        }
        return interruptResult
    }
}

@MainActor
private final class FakeSchedulerNotificationAdapter: JarvisSchedulerNotificationAdapter {
    var authorizationResult: Result<Bool, Error>
    var deliveryResult: Result<Void, Error>
    private(set) var authorizationRequestCount: Int
    private(set) var deliveredRequests: [JarvisSchedulerNotificationRequest]

    init(
        authorizationResult: Result<Bool, Error> = .success(true),
        deliveryResult: Result<Void, Error> = .success(())
    ) {
        self.authorizationResult = authorizationResult
        self.deliveryResult = deliveryResult
        self.authorizationRequestCount = 0
        self.deliveredRequests = []
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
                { "method": "GET", "path": "/activity/summary", "repository_required": true, "redacted": false },
                { "method": "GET", "path": "/release/readiness", "repository_required": false, "redacted": true },
                { "method": "GET", "path": "/permissions/grants", "repository_required": true, "redacted": false },
                { "method": "GET", "path": "/permissions/policy-review", "repository_required": true, "redacted": false }
              ],
              "safe_inspection_paths": ["/health", "/release/readiness", "/diagnostics/export"],
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
        #expect(contract.exposesReleaseReadiness)
    }

    @Test("Release readiness payload decodes production blockers")
    func decodesReleaseReadiness() throws {
        let readiness = try JSONDecoder().decode(
            JarvisReleaseReadiness.self,
            from: releaseReadinessJSON()
        )

        #expect(!readiness.productionReady)
        #expect(readiness.verifiedFeatureCount == 16)
        #expect(readiness.pendingFeatureCount == 1)
        #expect(readiness.implementedFeatures.first?.key == "repository_state")
        #expect(readiness.implementedFeatures.map(\.key).contains("operator_release_qa_smoke"))
        #expect(readiness.implementedFeatures.map(\.key).contains("unsigned_distribution_launch"))
        #expect(readiness.implementedFeatures.map(\.key).contains("release_evidence_bundle"))
        #expect(readiness.pendingFeatures.first?.key == "live_voice_loop")
        #expect(readiness.blockingManualGates.contains("Developer ID Application and Installer signing credentials configured and used for a full signed package run"))
        #expect(readiness.blockingManualGates.contains("final release evidence bundle generated and archived after signed distribution, live-device QA, and plugin-trust QA reports exist"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-local.sh"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-operator-qa-smoke.sh"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-live-device-qa.sh --check"))
        #expect(readiness.recommendedVerificationCommands.contains { command in
            command.contains("JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true") &&
                command.contains("JARVIS_QA_OWNER_NAME=") &&
                command.contains("JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE=") &&
                command.contains("./scripts/release-live-device-qa.sh --assert-complete")
        })
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-plugin-trust-qa.sh --check"))
        #expect(readiness.recommendedVerificationCommands.contains { command in
            command.contains("JARVIS_PLUGIN_QA_OWNER_NAME=") &&
                command.contains("JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE=") &&
                command.contains("./scripts/release-plugin-trust-qa.sh --assert-complete")
        })
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-evidence-bundle.sh --check"))
        #expect(readiness.recommendedVerificationCommands.contains("./scripts/release-evidence-doctor.sh --check"))
        #expect(readiness.proofBoundary.contains("does not perform signing"))
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

    @Test("Command request encodes dry_run for Rust IPC")
    func encodesCommandRequest() throws {
        let request = JarvisCommandRequest(input: "status check", dryRun: true)
        let data = try JSONEncoder().encode(request)
        let json = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])

        #expect(json["input"] as? String == "status check")
        #expect(json["dry_run"] as? Bool == true)
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

        IPCURLProtocol.handler = { request in
            let body = decodeRequestBody(request)
            requests.append((request.httpMethod ?? "", request.url?.path(percentEncoded: false) ?? "", body))

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
            case "/memory":
                if request.httpMethod == "GET" {
                    return (response, Data("[]".utf8))
                }
                return (response, memoryItemJSON(id: memoryId))
            case "/memory/classification":
                return (response, memoryClassificationJSON())
            case "/memory/\(memoryId.uuidString)":
                return (response, memoryItemJSON(id: memoryId))
            case "/memory/\(memoryId.uuidString)/review":
                return (response, memoryItemJSON(id: memoryId))
            case "/memory/\(memoryId.uuidString)/restore":
                return (response, memoryItemJSON(id: memoryId))
            case "/plugins/manifests":
                return (response, Data("[]".utf8))
            case "/scheduler/jobs":
                if request.httpMethod == "GET" {
                    return (response, Data("[]".utf8))
                }
                return (response, schedulerJobJSON(id: jobId))
            case "/scheduler/attention":
                return (response, schedulerAttentionJSON(id: jobId))
            case "/scheduler/jobs/\(jobId.uuidString)":
                return (response, schedulerJobJSON(id: jobId))
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
        _ = try await client.listMemoryItems(includeDeleted: true)
        _ = try await client.memoryClassification(includeDeleted: true)
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
        _ = try await client.listSchedulerJobs()
        _ = try await client.schedulerAttention()
        _ = try await client.createSchedulerJob(
            JarvisCreateSchedulerJobRequest(
                name: "one shot",
                command: "status check",
                trigger: .manual
            )
        )
        _ = try await client.schedulerJob(id: jobId)
        _ = try await client.activitySummary()
        _ = try await client.activityEvents(maxEvents: 2, intervalMilliseconds: 500)
        _ = try await client.pauseStatus()

        #expect(requests.map(\.method) == [
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
            "POST",
            "GET",
            "GET",
            "GET",
            "GET"
        ])
        #expect(requests.map(\.path) == [
            "/contract",
            "/release/readiness",
            "/release/evidence-status",
            "/memory",
            "/memory/classification",
            "/memory",
            "/memory/\(memoryId.uuidString)",
            "/memory/\(memoryId.uuidString)",
            "/memory/\(memoryId.uuidString)/review",
            "/memory/\(memoryId.uuidString)",
            "/memory/\(memoryId.uuidString)/restore",
            "/plugins/manifests",
            "/scheduler/jobs",
            "/scheduler/attention",
            "/scheduler/jobs",
            "/scheduler/jobs/\(jobId.uuidString)",
            "/activity/summary",
            "/activity/events",
            "/emergency-pause"
        ])
        #expect(requests[5].body?["key"] as? String == "release-gate")
        #expect(requests[7].body?["value"] as? String == "preview then sync")
        #expect(requests[14].body?["command"] as? String == "status check")
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
        #expect(summary.recentAuditEntries.first?.eventType == "plugin_completed")
    }

    @Test("Activity event stream parses bounded server-sent summaries and errors")
    func parsesActivityEventStream() throws {
        let taskId = UUID()
        let events = try JarvisActivityEvent.parseServerSentEvents(activityEventsSSE(taskId: taskId))

        #expect(events.count == 3)
        #expect(events.first?.event == "activity_summary")
        #expect(events.first?.summary?.recentTasks.first?.id == taskId)
        #expect(events.first?.summary?.activeTaskCount == 1)
        #expect(events[1].event == "activity_progress")
        #expect(events[1].progress?.pluginId == "local_runner_test")
        #expect(events[1].progress?.stage == "prepare")
        #expect(events[1].progress?.message == "validated request")
        #expect(events[1].progress?.stderrRedacted == true)
        #expect(events.last?.event == "activity_error")
        #expect(events.last?.error == "repository unavailable")
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

    @Test("Management payloads decode installed plugin registry records")
    func decodesInstalledPluginRecords() throws {
        let records = try JSONDecoder().decode(
            [JarvisInstalledPluginRecord].self,
            from: installedPluginsJSON()
        )
        let record = try #require(records.first)

        #expect(record.id == "local_runner_test")
        #expect(record.manifest.source == "local_subprocess")
        #expect(record.sourcePath == "/tmp/jarvis-plugin")
        #expect(!record.executionEnabled)
        #expect(record.executionGrant == "metadata_only")
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
              "redaction": "diagnostics export omits command bodies",
              "health": {
                "status": "ok",
                "version": "0.1.0",
                "started_at": "2026-05-20T11:59:00Z",
                "emergency_paused": false,
                "emergency_pause_reason": null,
                "emergency_pause_updated_at": null,
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
        #expect(model.evidenceStatus?.complete == false)
        #expect(model.evidenceStatus?.items.map(\.key).contains("live_device_qa_report") == true)
        #expect(model.readiness?.implementedFeatures.map(\.key).contains("repository_state") == true)
        #expect(model.readiness?.implementedFeatures.map(\.key).contains("release_evidence_bundle") == true)
        #expect(model.readiness?.pendingFeatures.map(\.key).contains("live_voice_loop") == true)
        #expect(model.readiness?.blockingManualGates.contains("Developer ID Application and Installer signing credentials configured and used for a full signed package run") == true)
        #expect(model.readiness?.blockingManualGates.contains("final release evidence bundle generated and archived after signed distribution, live-device QA, and plugin-trust QA reports exist") == true)
        #expect(model.readiness?.proofBoundary.contains("does not perform signing") == true)
        #expect(model.lastError == nil)
        #expect(model.isShowingStaleReadiness == false)
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

        await model.refresh()
        #expect(model.readiness?.generatedAt == "2026-05-22T08:00:00Z")
        #expect(model.lastError != nil)
        #expect(model.isShowingStaleReadiness == true)
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
        let executed = try await client.executeApproval(id: approvalId)

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

        #expect(client.submittedCommands == [
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

        #expect(client.submittedCommands == [
            JarvisCommandRequest(input: "status check", dryRun: false)
        ])
        #expect(console.transcript.map(\.text) == [
            "status check",
            "local response: status check"
        ])
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
        #expect(model.canStartCapture)
        #expect(!model.isCaptureActive)

        await model.requestPermissions()
        #expect(model.phase == .idle)
        #expect(model.lastError == nil)

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

        model.setFinalTranscriptAutoSubmitEnabled(true)
        await model.startCapture()
        adapter.emitFinal("  status check  ")
        try await Task.sleep(nanoseconds: 50_000_000)

        #expect(client.submittedCommands == [
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

        await model.startCapture()

        #expect(model.phase == .unavailable(reason: "Voice capture failed to start: No input device selected."))
        #expect(model.lastError == .captureStartFailed("No input device selected."))
        #expect(voice.transcriptDraft.isEmpty)
        #expect(voice.statusText.contains("Voice unavailable"))
    }

    @MainActor
    @Test("Voice adapter model preserves interruption as an explicit state")
    func voiceAdapterModelInterruptsActiveCaptureExplicitly() async {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        let model = VoiceAdapterStateModel(adapter: adapter, voiceState: voice)

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
    @Test("Voice adapter callback errors mark voice unavailable")
    func voiceAdapterCallbackErrorsMarkVoiceUnavailable() async {
        let adapter = FakeVoiceAdapter()
        let voice = VoiceStateModel()
        let model = VoiceAdapterStateModel(adapter: adapter, voiceState: voice)

        await model.startCapture()
        adapter.emitError(.recognitionFailed("Recognition task cancelled."))

        #expect(model.phase == .unavailable(reason: "Speech recognition failed: Recognition task cancelled."))
        #expect(model.lastError == .recognitionFailed("Recognition task cancelled."))
        #expect(voice.statusText.contains("Voice unavailable"))
        #expect(voice.statusText.contains("Recognition task cancelled"))
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
    @Test("Speech output model rejects empty utterances before adapter playback")
    func speechOutputModelRejectsEmptyUtterances() async {
        let adapter = FakeSpeechOutputAdapter()
        let model = SpeechOutputStateModel(adapter: adapter)

        await model.speak("   ")

        #expect(adapter.spokenTexts.isEmpty)
        #expect(model.phase == .unavailable(reason: JarvisSpeechOutputError.emptyUtterance.description))
        #expect(model.lastError == .emptyUtterance)
        #expect(model.statusText.contains("Speech output unavailable"))
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
            installedPlugins: installed
        )
        let model = PluginManagerModel(client: client)

        await model.refresh()

        #expect(model.manifests.map(\.id) == ["calendar"])
        #expect(model.installedPlugins.map(\.id) == ["local_runner_test"])
        #expect(model.installedPlugins.first?.provenance.needsReview == true)
        #expect(model.installedPlugins.first?.isExecutable == false)
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
        notificationReason: String = "A scheduler job is due and ready for the app to surface."
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
                  "next_due_at": "2026-05-20T12:00:01Z",
                  "notification_kind": "\(notificationKind)",
                  "notification_reason": "\(notificationReason)"
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
                  "user_input": "status check",
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
        {"audit_id":"\(UUID().uuidString)","task_id":"\(taskId.uuidString)","created_at":"2026-05-20T12:00:02Z","plugin_id":"local_runner_test","action":"inspect","session_id":"\(UUID().uuidString)","sequence":1,"stage":"prepare","message":"validated request","stderr_redacted":true}
        """
            .replacingOccurrences(of: "\n", with: "")
        return Data(
            """
            event: activity_summary
            data: \(summary)

            event: activity_progress
            data: \(progress)

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
    status: String = "pending",
    decidedBy: String? = nil,
    decisionReason: String? = nil
) -> JarvisPendingApproval {
    try! JSONDecoder().decode(
        JarvisPendingApproval.self,
        from: pendingApprovalJSON(
            id: id,
            taskId: taskId,
            status: status,
            decidedBy: decidedBy,
            decisionReason: decisionReason
        )
    )
}

private func pendingApprovalJSON(
    id: UUID,
    taskId: UUID,
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
          "action": "fake_echo.approval_echo",
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
          "readiness_scope": "local Rust/CLI foundation and Swift shell evidence only; full production distribution still has external manual gates",
          "verified_feature_count": 16,
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
              "proof": "Local subprocess plugins require full source-tree provenance verification plus explicit grants.",
              "boundary": "Constrained local subprocess execution only; not a WASM, OS-level, or marketplace sandbox."
            },
            {
              "key": "operator_release_qa_smoke",
              "status": "implemented",
              "proof": "`release-operator-qa-smoke.sh` exercises repository-backed operator QA.",
              "boundary": "Local CLI/operator QA evidence only."
            },
            {
              "key": "unsigned_distribution_launch",
              "status": "implemented",
              "proof": "`package-distribution.sh --unsigned-launch-check` builds the release app layout.",
              "boundary": "Unsigned distribution-layout proof only."
            },
            {
              "key": "release_evidence_bundle",
              "status": "implemented",
              "proof": "`release-evidence-bundle.sh --bundle` writes SHA-256-bound evidence manifest entries.",
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
            "./scripts/release-operator-qa-smoke.sh",
            "./scripts/package-distribution.sh --unsigned-launch-check",
            "./scripts/release-live-device-qa.sh --check",
            "JARVIS_QA_CLEAN_PROFILE_VALIDATED=true JARVIS_QA_FINDER_LAUNCH_VALIDATED=true JARVIS_QA_MICROPHONE_VALIDATED=true JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true JARVIS_QA_NOTIFICATION_VALIDATED=true JARVIS_QA_RESTART_VALIDATED=true JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true JARVIS_QA_OWNER_NAME='Release Operator' JARVIS_QA_DEVICE_LABEL='Clean-profile release Mac' JARVIS_QA_PROFILE_LABEL='Clean macOS QA profile' JARVIS_QA_VOICE_CHECK_STARTED_AT='2026-05-22T16:00:00Z' JARVIS_QA_VOICE_CHECK_COMPLETED_AT='2026-05-22T16:05:00Z' JARVIS_QA_MICROPHONE_EVIDENCE_NOTE='Microphone prompt and capture observed' JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE='Speech prompt and recognition observed' JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE='Spoken transcript reached the command path' JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE='Speech output playback observed' ./scripts/release-live-device-qa.sh --assert-complete",
            "./scripts/release-plugin-trust-qa.sh --check",
            "JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED=true JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED=true JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED=true JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED=true JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED=true JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED=true JARVIS_PLUGIN_QA_OWNER_NAME='Release Operator' JARVIS_PLUGIN_QA_REVIEW_STARTED_AT='2026-05-22T16:10:00Z' JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT='2026-05-22T16:20:00Z' JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE='Marketplace review evidence archived' JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE='Malware scan evidence archived' JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE='OS sandbox validation evidence archived' JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE='Host-level egress validation evidence archived' JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE='Signed publisher policy evidence archived' JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE='Manual plugin trust review evidence archived' ./scripts/release-plugin-trust-qa.sh --assert-complete",
            "./scripts/release-evidence-bundle.sh --check",
            "./scripts/release-evidence-doctor.sh --check",
            "JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true JARVIS_EVIDENCE_REPORTS_ARCHIVED=true ./scripts/release-evidence-bundle.sh --bundle"
          ],
          "proof_boundary": "Read-only summary derived from /contract feature metadata and release checklist blockers; it does not perform signing, notarization, installation, Finder/LaunchServices validation, live microphone/Speech validation, spoken transcript handoff, live audio-output validation, App Store review, marketplace plugin review, malware analysis, or OS sandbox enforcement."
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
          "missing_count": 5,
          "invalid_count": 0,
          "items": [
            {
              "key": "signed_app_bundle",
              "label": "Signed app bundle",
              "path": "target/distribution/Jarvis.app",
              "kind": "directory",
              "status": "present",
              "required_for_production": true,
              "manual_gate": true,
              "detail": "directory exists"
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
            }
          ],
          "proof_boundary": "File/report inspection only; this endpoint does not sign, notarize, staple, install, Finder-launch, run live-device QA, run marketplace review, scan malware, or enforce an OS sandbox/egress policy."
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
              "source_path": "/tmp/jarvis-plugin",
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
              ],
              "subprocess": {
                "command": "plugin-runner.py",
                "args": [],
                "stdin": "json",
                "stdout": "json"
              }
            },
            "source_path": "/tmp/jarvis-plugin",
            "provenance": {
              "provenance_schema_version": 1,
              "capture_method": "local_manifest_snapshot",
              "manifest_path": "/tmp/jarvis-plugin/jarvis-plugin.json",
              "manifest_sha256": "abc123",
              "source_path": "/tmp/jarvis-plugin",
              "source_path_canonicalized": true,
              "subprocess_command_path": "/tmp/jarvis-plugin/plugin-runner.py",
              "subprocess_command_sha256": "def456",
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

private func commandResponseJSON(input: String) -> Data {
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
            "status": "completed",
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

private final class FakeProcess: JarvisCoreProcess, @unchecked Sendable {
    private(set) var isRunning = true

    func terminate() {
        isRunning = false
    }
}

private final class FakeProcessLauncher: JarvisCoreProcessLaunching, @unchecked Sendable {
    struct Launch: Equatable {
        var executableURL: URL
        var arguments: [String]
        var environment: [String: String]
    }

    private(set) var launches: [Launch] = []

    func launch(
        executableURL: URL,
        arguments: [String],
        environment: [String: String]
    ) throws -> any JarvisCoreProcess {
        launches.append(Launch(executableURL: executableURL, arguments: arguments, environment: environment))
        return FakeProcess()
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
    private(set) var submittedCommands: [JarvisCommandRequest]
    private var contractResponse: JarvisContractResponse?
    private var releaseReadinessResponse: JarvisReleaseReadiness?
    private var releaseReadinessResults: [Result<JarvisReleaseReadiness, Error>]
    private var releaseEvidenceStatusResponse: JarvisReleaseEvidenceStatus?
    private var approvals: [JarvisPendingApproval]
    private var pluginManifests: [JarvisPluginManifest]
    private var installedPlugins: [JarvisInstalledPluginRecord]
    private var installedPluginsUnavailable: Bool
    private var memoryItems: [JarvisMemoryItem]
    private var permissionGrantSummaryResult: JarvisPermissionGrantSummary?
    private(set) var approvalDecisions: [ApprovalDecision]
    private(set) var approvalExecutions: [UUID]
    private(set) var includeDeletedMemoryRequests: [Bool]
    private(set) var createdMemoryRequests: [JarvisCreateMemoryItemRequest]
    private(set) var updatedMemoryRequests: [(id: UUID, request: JarvisMemoryMutationRequest)]

    init(
        healthResults: [Result<JarvisHealth, Error>] = [.success(sampleHealth())],
        tasks: [JarvisTask] = [],
        auditEntries: [JarvisAuditEntry] = [],
        activityEvents: [JarvisActivityEvent] = [],
        contractResponse: JarvisContractResponse? = nil,
        releaseReadiness: JarvisReleaseReadiness? = nil,
        releaseReadinessResults: [Result<JarvisReleaseReadiness, Error>] = [],
        releaseEvidenceStatus: JarvisReleaseEvidenceStatus? = nil,
        approvals: [JarvisPendingApproval] = [],
        pluginManifests: [JarvisPluginManifest] = [],
        installedPlugins: [JarvisInstalledPluginRecord] = [],
        installedPluginsUnavailable: Bool = false,
        memoryItems: [JarvisMemoryItem] = [],
        permissionGrantSummary: JarvisPermissionGrantSummary? = nil
    ) {
        self.healthResults = healthResults
        self.tasks = tasks
        self.auditEntries = auditEntries
        self.activityEvents = activityEvents
        self.submittedCommands = []
        self.contractResponse = contractResponse
        self.releaseReadinessResponse = releaseReadiness
        self.releaseReadinessResults = releaseReadinessResults
        self.releaseEvidenceStatusResponse = releaseEvidenceStatus
        self.approvals = approvals
        self.pluginManifests = pluginManifests
        self.installedPlugins = installedPlugins
        self.installedPluginsUnavailable = installedPluginsUnavailable
        self.memoryItems = memoryItems
        self.permissionGrantSummaryResult = permissionGrantSummary
        self.approvalDecisions = []
        self.approvalExecutions = []
        self.includeDeletedMemoryRequests = []
        self.createdMemoryRequests = []
        self.updatedMemoryRequests = []
    }

    func health() async throws -> JarvisHealth {
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
        if let releaseEvidenceStatusResponse {
            return releaseEvidenceStatusResponse
        }

        return try JSONDecoder().decode(JarvisReleaseEvidenceStatus.self, from: releaseEvidenceStatusJSON())
    }

    func submit(_ command: JarvisCommandRequest) async throws -> JarvisCommandResponse {
        submittedCommands.append(command)
        return try JSONDecoder().decode(JarvisCommandResponse.self, from: commandResponseJSON(input: command.input))
    }

    func pause(reason: String) async throws -> JarvisPauseResponse {
        throw URLError(.unsupportedURL)
    }

    func resume() async throws -> JarvisPauseResponse {
        throw URLError(.unsupportedURL)
    }

    func pauseStatus() async throws -> JarvisPauseResponse {
        throw URLError(.unsupportedURL)
    }

    func listTasks() async throws -> [JarvisTask] {
        tasks
    }

    func task(id: UUID) async throws -> JarvisTask {
        throw URLError(.unsupportedURL)
    }

    func listAuditEntries(taskId: UUID?) async throws -> [JarvisAuditEntry] {
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
            recentTasks: Array(tasks.suffix(5).reversed()),
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

    func listInstalledPlugins() async throws -> [JarvisInstalledPluginRecord] {
        if installedPluginsUnavailable {
            throw URLError(.cannotConnectToHost)
        }
        return installedPlugins
    }

    func listSchedulerJobs() async throws -> [JarvisSchedulerJob] {
        []
    }

    func schedulerAttention() async throws -> JarvisSchedulerAttentionSummary {
        try JSONDecoder().decode(
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

    func schedulerJob(id: UUID) async throws -> JarvisSchedulerJob {
        throw URLError(.unsupportedURL)
    }

    func createSchedulerJob(_ request: JarvisCreateSchedulerJobRequest) async throws -> JarvisSchedulerJob {
        throw URLError(.unsupportedURL)
    }

    func cancelSchedulerJob(id: UUID) async throws -> JarvisSchedulerJob {
        throw URLError(.unsupportedURL)
    }

    func diagnosticsExport() async throws -> JarvisDiagnosticsExport {
        try JSONDecoder().decode(JarvisDiagnosticsExport.self, from: diagnosticsJSON())
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

    func executeApproval(id: UUID) async throws -> JarvisApprovalExecutionResponse {
        guard let approval = approvals.first(where: { $0.id == id }) else {
            throw URLError(.badServerResponse)
        }
        approvalExecutions.append(id)
        auditEntries.append(
            JarvisAuditEntry(
                id: UUID(),
                taskId: approval.taskId,
                eventType: "approval_executed",
                summary: "approved first-party plugin action execution completed",
                payload: .object(["approval_id": .string(approval.id.uuidString)]),
                createdAt: "2026-05-20T12:15:00Z"
            )
        )
        return try JSONDecoder().decode(
            JarvisApprovalExecutionResponse.self,
            from: approvalExecutionJSON(approvalId: approval.id, taskId: approval.taskId)
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
