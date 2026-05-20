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

@Suite("Jarvis Mac core contracts")
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
        #expect(health.emergencyPaused)
        #expect(health.emergencyPauseReason == "testing")
        #expect(health.schedulerJobs == 2)
        #expect(health.commandRuntime == "fake-local-model")
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
            case "/emergency-pause":
                return (response, Data(#"{"paused":false,"reason":null,"paused_at":null,"resumed_at":"2026-05-20T12:00:00Z","cancelled_scheduler_jobs":0}"#.utf8))
            default:
                return (response, Data("[]".utf8))
            }
        }
        defer { IPCURLProtocol.handler = nil }

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
        _ = try await client.pauseStatus()

        #expect(requests.map(\.method) == ["GET", "POST", "GET", "GET", "POST", "GET"])
        #expect(requests.map(\.path) == ["/memory", "/memory", "/plugins/manifests", "/scheduler/jobs", "/scheduler/jobs", "/emergency-pause"])
        #expect(requests[1].body?["key"] as? String == "release-gate")
        #expect(requests[4].body?["command"] as? String == "status check")
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
