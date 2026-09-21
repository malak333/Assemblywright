import Foundation
import Testing
@testable import AssemblywrightMacApp

private final class DeveloperHTTPFixture: URLProtocol, @unchecked Sendable {
  private static let lock = NSLock()
  nonisolated(unsafe) private static var handler: ((URLRequest) throws -> (Int, Data))?

  static func respond(_ value: @escaping (URLRequest) throws -> (Int, Data)) {
    lock.withLock { handler = value }
  }
  override class func canInit(with request: URLRequest) -> Bool { true }
  override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
  override func startLoading() {
    do {
      let callback = Self.lock.withLock { Self.handler }
      guard let callback else { throw URLError(.unknown) }
      let (status, data) = try callback(request)
      let response = HTTPURLResponse(url: request.url!, statusCode: status,
        httpVersion: nil, headerFields: ["Content-Type": "application/json"])!
      client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
      client?.urlProtocol(self, didLoad: data)
      client?.urlProtocolDidFinishLoading(self)
    } catch { client?.urlProtocol(self, didFailWithError: error) }
  }
  override func stopLoading() {}
}

@Suite("Developer runner client", .serialized)
@MainActor
final class DeveloperRunnerClientTests {
  private var configurationFiles: [URL] = []

  deinit {
    for file in configurationFiles { try? FileManager.default.removeItem(at: file) }
  }

  private func snapshot(_ revision: Int, mode: String = "supervised_developer",
    autoRepairEnabled: Bool = false, maxEscalations: Int = 100,
    policyRevision: Int? = nil, queue: String = "[]") -> Data {
    let policyRevision = policyRevision ?? revision
    return Data("""
      {"mode":"\(mode)","host":"fixture-windows","workspace_root":"fixture",
       "revision":\(revision),"auto_run":true,"emergency_paused":false,
       "running":false,"auto_ai_repair_enabled":\(autoRepairEnabled),
       "auto_ai_repair_max_escalations":\(maxEscalations),
       "auto_ai_repair_policy_revision":\(policyRevision),"queue":\(queue),
       "review_required":true,"review_provider":"openai.codex",
       "review_model":"gpt-5.6-sol","planning_required":true,"planning_provider":"openai.codex",
       "planning_model":"gpt-5.6-sol","planning_running":false,"planning_sessions":[]}
      """.utf8)
  }

  private func configuredSnapshot(_ revision: Int, settingsRevision: Int,
    featureReviewer: DeveloperAISelection? = nil) throws -> Data {
    let orchestrator = ["model": "gpt-6-astra", "reasoning_effort": "high"]
    let reviewer = ["model": "gpt-5.3-codex-spark", "reasoning_effort": "high"]
    var wire: [String: Any] = [
      "mode": "supervised_developer", "host": "fixture-windows", "workspace_root": "fixture",
      "revision": revision, "auto_run": true, "emergency_paused": false, "running": false,
      "chat_running": false, "escalation_running": false, "queue": [],
      "review_required": true, "review_provider": "openai.codex",
      "review_model": "gpt-5.3-codex-spark", "review_reasoning_effort": "high",
      "planning_required": true, "planning_provider": "openai.codex",
      "planning_model": "gpt-6-astra", "planning_reasoning_effort": "high",
      "planning_running": false, "planning_sessions": [], "feature_reviewer_selection": true,
      "ai_settings": ["revision": settingsRevision, "orchestrator": orchestrator,
        "reviewer": reviewer],
      "ai_models": [
        ["id": "gpt-6-astra", "name": "Astra", "reasoning_efforts": ["high"],
          "default_reasoning_effort": "high"],
        ["id": "gpt-5.3-codex-spark", "name": "Spark", "reasoning_efforts": ["high"],
          "default_reasoning_effort": "high"],
        ["id": "gpt-5.6-sol", "name": "Sol", "reasoning_efforts": ["high"],
          "default_reasoning_effort": "high"],
      ],
    ]
    if let featureReviewer {
      wire["queue"] = [[
        "id": "6b7c48c1-1e3c-4e98-9791-e54a67f0786a", "project": "fixture",
        "instruction": "Restore the feature", "validation": "true", "status": "failed",
        "checkpoint": "review_3_unavailable", "message": "Review unavailable",
        "changed_files": ["app.py"], "repair_attempts": 3, "review_status": "unavailable",
        "review_model": featureReviewer.model,
        "review_reasoning_effort": featureReviewer.reasoningEffort,
        "can_change_reviewer": true,
      ]]
    }
    return try JSONSerialization.data(withJSONObject: wire)
  }

  private func model(endpoint: String = "http://127.0.0.1:17796") throws -> DeveloperRunnerModel {
    let path = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try JSONSerialization.data(withJSONObject: ["endpoint": endpoint, "token": "fixture-token"])
      .write(to: path)
    configurationFiles.append(path)
    let config = URLSessionConfiguration.ephemeral
    config.protocolClasses = [DeveloperHTTPFixture.self]
    return DeveloperRunnerModel(configurationPath: path.path, session: URLSession(configuration: config))
  }

  private func body(_ request: URLRequest) throws -> Data {
    if let body = request.httpBody { return body }
    let stream = try #require(request.httpBodyStream)
    stream.open()
    defer { stream.close() }
    var result = Data()
    var buffer = [UInt8](repeating: 0, count: 1_024)
    while true {
      let count = stream.read(&buffer, maxLength: buffer.count)
      if count < 0 { throw stream.streamError ?? URLError(.cannotDecodeContentData) }
      if count == 0 { return result }
      result.append(contentsOf: buffer.prefix(count))
    }
  }

  @Test func authenticatesAndDecodesStatus() async throws {
    let body = snapshot(7)
    DeveloperHTTPFixture.respond { request in
      #expect(request.url?.path == "/status")
      #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer fixture-token")
      return (200, body)
    }
    let client = try model()
    await client.refresh()
    #expect(client.snapshot?.host == "fixture-windows")
    #expect(client.snapshot?.revision == 7)
    #expect(client.error == nil)
  }

  @Test func staleStatusCannotReplaceNewerCommandResult() async throws {
    let newer = snapshot(9)
    DeveloperHTTPFixture.respond { request in
      #expect(request.httpMethod == "POST")
      #expect(request.url?.path == "/control")
      return (200, newer)
    }
    let client = try model()
    await client.send("stop")
    let older = snapshot(8)
    DeveloperHTTPFixture.respond { _ in (200, older) }
    await client.refresh()
    #expect(client.snapshot?.revision == 9)
    #expect(!client.sending)
  }

  @Test func autoAIRepairMutationSendsAtomicBodyAndRequiresExactAcknowledgement() async throws {
    let client = try model()
    DeveloperHTTPFixture.respond { _ in (200, self.snapshot(30)) }
    await client.refresh()
    DeveloperHTTPFixture.respond { request in
      if request.url?.path == "/status" { return (200, self.snapshot(30)) }
      #expect(request.httpMethod == "POST")
      #expect(request.url?.path == "/auto-ai-repair")
      #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer fixture-token")
      let body = try self.body(request)
      let value = try #require(JSONSerialization.jsonObject(with: body) as? [String: Any])
      #expect(value.count == 3)
      #expect(value["enabled"] as? Bool == true)
      #expect(value["max_escalations"] as? Int == 7)
      #expect(value["expected_revision"] as? Int == 30)
      return (200, self.snapshot(31, autoRepairEnabled: true, maxEscalations: 7,
        policyRevision: 31))
    }
    await client.updateAutoAIRepair(enabled: true, maxEscalations: 7)
    #expect(client.snapshot?.revision == 31)
    #expect(client.snapshot?.autoAiRepairEnabled == true)
    #expect(client.snapshot?.autoAiRepairMaxEscalations == 7)
    #expect(client.autoAIRepairMutationMessage == nil)
    #expect(!client.autoAIRepairMutationPending)
  }

  @Test func newerPollingSnapshotWinsOverExactMutationReceiptWhenPolicyStillMatches() async throws {
    let client = try model()
    DeveloperHTTPFixture.respond { _ in (200, self.snapshot(80)) }
    await client.refresh()

    let releaseReceipt = DispatchSemaphore(value: 0)
    DeveloperHTTPFixture.respond { request in
      #expect(request.url?.path == "/auto-ai-repair")
      releaseReceipt.wait()
      return (200, self.snapshot(81, autoRepairEnabled: true, maxEscalations: 12,
        policyRevision: 81))
    }
    let mutation = Task { await client.updateAutoAIRepair(enabled: true, maxEscalations: 12) }
    while !client.autoAIRepairMutationPending { await Task.yield() }

    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    client.snapshot = try decoder.decode(DeveloperRunnerSnapshot.self, from:
      snapshot(82, autoRepairEnabled: true, maxEscalations: 12, policyRevision: 81))
    releaseReceipt.signal()
    await mutation.value

    #expect(client.snapshot?.revision == 82)
    #expect(client.snapshot?.autoAiRepairEnabled == true)
    #expect(client.snapshot?.autoAiRepairMaxEscalations == 12)
    #expect(client.autoAIRepairMutationMessage == nil)
    #expect(!client.autoAIRepairMutationPending)
  }

  @Test func newerPollingSnapshotMustStillConfirmTheRequestedPolicy() async throws {
    let client = try model()
    DeveloperHTTPFixture.respond { _ in (200, self.snapshot(90)) }
    await client.refresh()

    let releaseReceipt = DispatchSemaphore(value: 0)
    DeveloperHTTPFixture.respond { request in
      if request.url?.path == "/status" {
        return (200, self.snapshot(92, autoRepairEnabled: false, maxEscalations: 9,
          policyRevision: 92))
      }
      #expect(request.url?.path == "/auto-ai-repair")
      releaseReceipt.wait()
      return (200, self.snapshot(91, autoRepairEnabled: true, maxEscalations: 12,
        policyRevision: 91))
    }
    let mutation = Task { await client.updateAutoAIRepair(enabled: true, maxEscalations: 12) }
    while !client.autoAIRepairMutationPending { await Task.yield() }

    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    client.snapshot = try decoder.decode(DeveloperRunnerSnapshot.self, from:
      snapshot(92, autoRepairEnabled: false, maxEscalations: 9, policyRevision: 92))
    releaseReceipt.signal()
    await mutation.value

    #expect(client.snapshot?.revision == 92)
    #expect(client.snapshot?.autoAiRepairEnabled == false)
    #expect(client.snapshot?.autoAiRepairMaxEscalations == 9)
    #expect(client.autoAIRepairMutationMessage?.contains("no longer confirms") == true)
    #expect(client.autoAIRepairDraftResetToken == 1)
  }

  @Test func autoAIRepairRejectsPartialStaleAndReplayedAcknowledgements() async throws {
    let invalidReplies = [
      snapshot(40, autoRepairEnabled: true, maxEscalations: 8, policyRevision: 40),
      snapshot(41, autoRepairEnabled: false, maxEscalations: 8, policyRevision: 41),
      snapshot(41, autoRepairEnabled: true, maxEscalations: 9, policyRevision: 41),
      snapshot(41, autoRepairEnabled: true, maxEscalations: 8, policyRevision: 40),
    ]
    for reply in invalidReplies {
      let client = try model()
      DeveloperHTTPFixture.respond { _ in (200, self.snapshot(40)) }
      await client.refresh()
      DeveloperHTTPFixture.respond { request in
        if request.url?.path == "/auto-ai-repair" { return (200, reply) }
        #expect(request.url?.path == "/status")
        return (200, self.snapshot(40))
      }
      await client.updateAutoAIRepair(enabled: true, maxEscalations: 8)
      #expect(client.snapshot?.revision == 40)
      #expect(client.snapshot?.autoAiRepairEnabled == false)
      #expect(client.autoAIRepairMutationMessage?.contains("exact Auto AI repair") == true)
      #expect(!client.autoAIRepairMutationPending)
    }
  }

  @Test func autoAIRepairConflictRefreshesAuthoritativeState() async throws {
    let client = try model()
    DeveloperHTTPFixture.respond { _ in (200, self.snapshot(50)) }
    await client.refresh()
    DeveloperHTTPFixture.respond { request in
      if request.url?.path == "/auto-ai-repair" {
        return (409, Data(#"{"error":"Runner revision changed"}"#.utf8))
      }
      #expect(request.url?.path == "/status")
      return (200, self.snapshot(53, autoRepairEnabled: true, maxEscalations: 9,
        policyRevision: 53))
    }
    await client.updateAutoAIRepair(enabled: false, maxEscalations: 100)
    #expect(client.snapshot?.revision == 53)
    #expect(client.snapshot?.autoAiRepairEnabled == true)
    #expect(client.snapshot?.autoAiRepairMaxEscalations == 9)
    #expect(client.autoAIRepairDraftResetToken == 1)
    #expect(client.autoAIRepairMutationMessage?.contains("Authoritative settings were refreshed") == true)
  }

  @Test func invalidFocusedDraftCannotBlockOrAlterDisableRequest() async throws {
    let client = try model()
    DeveloperHTTPFixture.respond { _ in
      (200, self.snapshot(70, autoRepairEnabled: true, maxEscalations: 37,
        policyRevision: 70))
    }
    await client.refresh()
    let maximum = try #require(DeveloperAutoAIRepairPresentation.mutationMaximum(
      draft: "", authoritative: client.snapshot?.autoAiRepairMaxEscalations,
      disabling: true))
    DeveloperHTTPFixture.respond { request in
      let value = try #require(JSONSerialization.jsonObject(
        with: try self.body(request)) as? [String: Any])
      #expect(request.url?.path == "/auto-ai-repair")
      #expect(value["enabled"] as? Bool == false)
      #expect(value["max_escalations"] as? Int == 37)
      #expect(value["expected_revision"] as? Int == 70)
      return (200, self.snapshot(71, autoRepairEnabled: false, maxEscalations: 37,
        policyRevision: 71))
    }
    await client.updateAutoAIRepair(enabled: false, maxEscalations: maximum)
    #expect(client.snapshot?.revision == 71)
    #expect(client.snapshot?.autoAiRepairEnabled == false)
    #expect(client.snapshot?.autoAiRepairMaxEscalations == 37)
  }

  @Test func autoAIRepairRejectsInvalidInputBeforeNetworkAndNamesCancellation() async throws {
    let client = try model()
    DeveloperHTTPFixture.respond { _ in (200, self.snapshot(60)) }
    await client.refresh()
    DeveloperHTTPFixture.respond { _ in
      Issue.record("Invalid Auto AI repair input reached the network")
      return (500, Data())
    }
    await client.updateAutoAIRepair(enabled: true, maxEscalations: 0)
    #expect(client.snapshot?.revision == 60)
    #expect(client.error?.contains("1 through 100") == true)
    #expect(DeveloperAutoAIRepairPresentation.pendingMutation(
      enabled: true, automaticRepairActive: false) == "Enabling Auto AI repair…")
    #expect(DeveloperAutoAIRepairPresentation.pendingMutation(
      enabled: false, automaticRepairActive: true) == "Cancelling active Auto AI repair…")
  }

  @Test func rejectedCommandPreservesObservedStateAndClearsSending() async throws {
    let initial = snapshot(3)
    DeveloperHTTPFixture.respond { _ in (200, initial) }
    let client = try model()
    await client.refresh()
    DeveloperHTTPFixture.respond { _ in (409, Data(#"{"error":"Clear Emergency Pause"}"#.utf8)) }
    await client.send("resume")
    #expect(client.error == "Clear Emergency Pause")
    #expect(client.snapshot?.revision == 3)
    #expect(!client.sending)
  }

  @Test func settingsMutationRequiresExactRunnerAndSettingsRevisionAdvance() async throws {
    let client = try model()
    DeveloperHTTPFixture.respond { _ in (200, try self.configuredSnapshot(12, settingsRevision: 4)) }
    await client.refresh()
    let draft = try #require(client.snapshot?.aiSettings)

    DeveloperHTTPFixture.respond { request in
      #expect(request.httpMethod == "POST")
      #expect(request.url?.path == "/settings")
      return (200, try self.configuredSnapshot(12, settingsRevision: 5))
    }
    await #expect(throws: (any Error).self) { try await client.saveAISettings(draft) }
    #expect(client.snapshot?.revision == 12)
    #expect(client.snapshot?.aiSettings?.revision == 4)

    DeveloperHTTPFixture.respond { _ in
      (200, try self.configuredSnapshot(13, settingsRevision: 4))
    }
    await #expect(throws: (any Error).self) { try await client.saveAISettings(draft) }
    #expect(client.snapshot?.revision == 12)
    #expect(client.snapshot?.aiSettings?.revision == 4)

    DeveloperHTTPFixture.respond { _ in
      (200, try self.configuredSnapshot(13, settingsRevision: 5))
    }
    try await client.saveAISettings(draft)
    #expect(client.snapshot?.revision == 13)
    #expect(client.snapshot?.aiSettings?.revision == 5)
  }

  @Test func reviewerMutationRejectsReplayedSuccessWithoutRevisionAdvance() async throws {
    let original = DeveloperAISelection(model: "gpt-5.6-sol", reasoningEffort: "high")
    let requested = DeveloperAISelection(model: "gpt-6-astra", reasoningEffort: "high")
    let client = try model()
    DeveloperHTTPFixture.respond { _ in
      (200, try self.configuredSnapshot(20, settingsRevision: 4, featureReviewer: original))
    }
    await client.refresh()
    let feature = try #require(client.snapshot?.queue.first)

    DeveloperHTTPFixture.respond { request in
      #expect(request.httpMethod == "POST")
      #expect(request.url?.path == "/feature-reviewer")
      return (200, try self.configuredSnapshot(20, settingsRevision: 4,
        featureReviewer: requested))
    }
    await #expect(throws: (any Error).self) {
      try await client.changeReviewer(feature, revision: 20, selection: requested)
    }
    #expect(client.snapshot?.revision == 20)
    #expect(client.snapshot?.queue.first?.reviewerSelection == original)

    DeveloperHTTPFixture.respond { _ in
      (200, try self.configuredSnapshot(21, settingsRevision: 4, featureReviewer: requested))
    }
    try await client.changeReviewer(feature, revision: 20, selection: requested)
    #expect(client.snapshot?.revision == 21)
    #expect(client.snapshot?.queue.first?.reviewerSelection == requested)
  }

  @Test(arguments: ["malformed", "wrong-mode", "unauthorized", "transport"])
  func failedObservationDoesNotInventState(_ failure: String) async throws {
    let wrongMode = snapshot(1, mode: "production")
    DeveloperHTTPFixture.respond { _ in
      switch failure {
      case "wrong-mode": return (200, wrongMode)
      case "unauthorized": return (401, Data(#"{"error":"Unauthorized"}"#.utf8))
      case "transport": throw URLError(.notConnectedToInternet)
      default: return (200, Data("not-json".utf8))
      }
    }
    let client = try model()
    await client.refresh()
    #expect(client.snapshot == nil)
    #expect(client.error != nil)
  }

  @Test func invalidConfigurationDoesNotSendRequests() async throws {
    DeveloperHTTPFixture.respond { _ in
      Issue.record("Invalid configuration reached the network")
      return (500, Data())
    }
    let client = try model(endpoint: "http://example.invalid")
    await client.refresh()
    #expect(client.snapshot == nil)
    #expect(client.error?.contains("launcher") == true)
    let missing = DeveloperRunnerModel(configurationPath: "/missing-developer-fixture")
    await missing.send("start")
    #expect(missing.error?.contains("launcher") == true)
    #expect(!missing.sending)
  }

  @Test func cancelledObserverReturns() async throws {
    let body = snapshot(1)
    DeveloperHTTPFixture.respond { _ in (200, body) }
    let client = try model()
    let observation = Task { await client.observe() }
    for _ in 0..<50 {
      if client.snapshot != nil { break }
      try await Task.sleep(for: .milliseconds(10))
    }
    #expect(client.snapshot != nil)
    observation.cancel()
    await observation.value
    #expect(!client.sending)
  }
}
