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
              "endpoints": [
                { "method": "GET", "path": "/health", "repository_required": false, "redacted": true },
                { "method": "GET", "path": "/scheduler/jobs/:id", "repository_required": false, "redacted": false }
              ],
              "safe_inspection_paths": ["/health", "/diagnostics/export"]
            }
            """.utf8
        )

        let contract = try JSONDecoder().decode(JarvisContractResponse.self, from: data)

        #expect(contract.contract.name == "jarvis.local-ipc")
        #expect(contract.endpoints.map(\.id).contains("GET /scheduler/jobs/:id"))
        #expect(contract.safeInspectionPaths.contains("/diagnostics/export"))
        #expect(!contract.exposesApprovalActions)
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
            case "/memory":
                if request.httpMethod == "GET" {
                    return (response, Data("[]".utf8))
                }
                return (response, memoryItemJSON(id: memoryId))
            case "/plugins/manifests":
                return (response, Data("[]".utf8))
            case "/scheduler/jobs":
                if request.httpMethod == "GET" {
                    return (response, Data("[]".utf8))
                }
                return (response, schedulerJobJSON(id: jobId))
            case "/scheduler/jobs/\(jobId.uuidString)":
                return (response, schedulerJobJSON(id: jobId))
            case "/emergency-pause":
                return (response, Data(#"{"paused":false,"reason":null,"paused_at":null,"resumed_at":"2026-05-20T12:00:00Z","cancelled_scheduler_jobs":0}"#.utf8))
            default:
                return (response, Data("[]".utf8))
            }
        }
        defer { IPCURLProtocol.handler = nil }

        _ = try await client.contract()
        _ = try await client.listMemoryItems(includeDeleted: true)
        _ = try await client.createMemoryItem(
            JarvisCreateMemoryItemRequest(
                category: "release",
                key: "release-gate",
                value: "preview before write",
                provenance: "manual",
                sensitivity: "workspace"
            )
        )
        _ = try await client.listPluginManifests()
        _ = try await client.listSchedulerJobs()
        _ = try await client.createSchedulerJob(
            JarvisCreateSchedulerJobRequest(
                name: "one shot",
                command: "status check",
                trigger: .manual
            )
        )
        _ = try await client.schedulerJob(id: jobId)
        _ = try await client.pauseStatus()

        #expect(requests.map(\.method) == ["GET", "GET", "POST", "GET", "GET", "POST", "GET", "GET"])
        #expect(requests.map(\.path) == [
            "/contract",
            "/memory",
            "/memory",
            "/plugins/manifests",
            "/scheduler/jobs",
            "/scheduler/jobs",
            "/scheduler/jobs/\(jobId.uuidString)",
            "/emergency-pause"
        ])
        #expect(requests[2].body?["key"] as? String == "release-gate")
        #expect(requests[5].body?["command"] as? String == "status check")
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
              "active_memory_item_count": 3
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
            case "/approvals/\(approvalId.uuidString)":
                return (response, pendingApprovalJSON(id: approvalId, taskId: taskId))
            case "/approvals/\(approvalId.uuidString)/approve":
                return (response, pendingApprovalJSON(id: approvalId, taskId: taskId, status: "approved", decidedBy: "mac-ui", decisionReason: "reviewed"))
            case "/approvals/\(approvalId.uuidString)/deny":
                return (response, pendingApprovalJSON(id: approvalId, taskId: taskId, status: "denied", decidedBy: "mac-ui", decisionReason: "too risky"))
            default:
                return (response, Data("{}".utf8))
            }
        }
        defer { IPCURLProtocol.handler = nil }

        let pending = try await client.listApprovals(status: "pending")
        _ = try await client.approval(id: approvalId)
        let approved = try await client.approveApproval(
            id: approvalId,
            request: JarvisApprovalDecisionRequest(decidedBy: "mac-ui", reason: "reviewed")
        )
        let denied = try await client.denyApproval(
            id: approvalId,
            request: JarvisApprovalDecisionRequest(decidedBy: "mac-ui", reason: "too risky")
        )

        #expect(pending.first?.id == approvalId)
        #expect(approved.status == "approved")
        #expect(denied.status == "denied")
        #expect(requests.map(\.method) == ["GET", "GET", "POST", "POST"])
        #expect(requests.map(\.path) == [
            "/approvals",
            "/approvals/\(approvalId.uuidString)",
            "/approvals/\(approvalId.uuidString)/approve",
            "/approvals/\(approvalId.uuidString)/deny"
        ])
        #expect(requests[0].query == "status=pending")
        #expect(requests[2].body?["decided_by"] as? String == "mac-ui")
        #expect(requests[2].body?["reason"] as? String == "reviewed")
        #expect(requests[3].body?["reason"] as? String == "too risky")
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
        #expect(model.statusText.contains("Speech recognition is not implemented"))
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
        await console.submit(input: handoff.text)

        #expect(client.submittedCommands == [
            JarvisCommandRequest(input: "plugin echo hello", dryRun: true)
        ])
        #expect(console.transcript.map(\.text) == [
            "plugin echo hello",
            "local response: plugin echo hello"
        ])
        #expect(console.activity.contains { $0.title == "Task completed" })
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
    }

    @MainActor
    @Test("Approval management model approves and removes pending approval")
    func approvalManagementModelApprovesPendingApproval() async {
        let approval = samplePendingApproval()
        let client = FakeCoreClient(
            contractResponse: fullApprovalContract(),
            approvals: [approval]
        )
        let model = ApprovalManagementModel(client: client)

        await model.refresh()
        await model.approve(id: approval.id, reason: "reviewed in app")

        #expect(model.supportsApprovalActions)
        #expect(model.pendingItems.isEmpty)
        #expect(model.lastDecision?.status == "approved")
        #expect(model.lastDecision?.decidedBy == "mac-ui")
        #expect(client.approvalDecisions == [
            FakeCoreClient.ApprovalDecision(id: approval.id, approved: true, reason: "reviewed in app")
        ])
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
            processLauncher: launcher
        )

        await supervisor.start()

        #expect(supervisor.mode == .available)
        #expect(launcher.launches.count == 1)
        #expect(launcher.launches.first?.executableURL.path == "/tmp/jarvis-cli")
        #expect(launcher.launches.first?.arguments == ["serve", "--bind", "127.0.0.1:9901"])
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
                { "method": "GET", "path": "/approvals", "repository_required": true, "redacted": false },
                { "method": "GET", "path": "/approvals/:id", "repository_required": true, "redacted": false },
                { "method": "POST", "path": "/approvals/:id/approve", "repository_required": true, "redacted": false },
                { "method": "POST", "path": "/approvals/:id/deny", "repository_required": true, "redacted": false }
              ],
              "safe_inspection_paths": ["/health", "/approvals"]
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
    }

    private(set) var launches: [Launch] = []

    func launch(
        executableURL: URL,
        arguments: [String],
        environment: [String: String]
    ) throws -> any JarvisCoreProcess {
        launches.append(Launch(executableURL: executableURL, arguments: arguments))
        return FakeProcess()
    }
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
    private(set) var submittedCommands: [JarvisCommandRequest]
    private var contractResponse: JarvisContractResponse?
    private var approvals: [JarvisPendingApproval]
    private(set) var approvalDecisions: [ApprovalDecision]

    init(
        healthResults: [Result<JarvisHealth, Error>] = [.success(sampleHealth())],
        tasks: [JarvisTask] = [],
        auditEntries: [JarvisAuditEntry] = [],
        contractResponse: JarvisContractResponse? = nil,
        approvals: [JarvisPendingApproval] = []
    ) {
        self.healthResults = healthResults
        self.tasks = tasks
        self.auditEntries = auditEntries
        self.submittedCommands = []
        self.contractResponse = contractResponse
        self.approvals = approvals
        self.approvalDecisions = []
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

    func listMemoryItems(includeDeleted: Bool) async throws -> [JarvisMemoryItem] {
        []
    }

    func createMemoryItem(_ request: JarvisCreateMemoryItemRequest) async throws -> JarvisMemoryItem {
        throw URLError(.unsupportedURL)
    }

    func memoryItem(id: UUID) async throws -> JarvisMemoryItem {
        throw URLError(.unsupportedURL)
    }

    func updateMemoryItem(id: UUID, request: JarvisMemoryMutationRequest) async throws -> JarvisMemoryItem {
        throw URLError(.unsupportedURL)
    }

    func reviewMemoryItem(id: UUID) async throws -> JarvisMemoryItem {
        throw URLError(.unsupportedURL)
    }

    func deleteMemoryItem(id: UUID) async throws -> JarvisMemoryItem {
        throw URLError(.unsupportedURL)
    }

    func listPluginManifests() async throws -> [JarvisPluginManifest] {
        []
    }

    func listSchedulerJobs() async throws -> [JarvisSchedulerJob] {
        []
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

    private func decideApproval(
        id: UUID,
        approved: Bool,
        request: JarvisApprovalDecisionRequest
    ) throws -> JarvisPendingApproval {
        guard let approval = approvals.first(where: { $0.id == id }) else {
            throw URLError(.badServerResponse)
        }

        approvalDecisions.append(ApprovalDecision(id: id, approved: approved, reason: request.reason))
        return samplePendingApproval(
            id: approval.id,
            taskId: approval.taskId,
            status: approved ? "approved" : "denied",
            decidedBy: request.decidedBy,
            decisionReason: request.reason
        )
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
