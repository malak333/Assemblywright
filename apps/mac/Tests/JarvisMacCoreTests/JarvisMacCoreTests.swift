import Foundation
import Testing
@testable import JarvisMacCore

@Suite("Jarvis Mac core contracts")
struct JarvisMacCoreTests {
    @Test("Endpoint appends paths to the configured core URL")
    func endpointBuildsURL() {
        let endpoint = JarvisEndpoint(baseURL: URL(string: "http://127.0.0.1:7787")!)

        #expect(endpoint.url(path: "/health").absoluteString == "http://127.0.0.1:7787/health")
        #expect(endpoint.url(path: "commands").absoluteString == "http://127.0.0.1:7787/commands")
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
}
