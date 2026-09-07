import Foundation
import Testing
@testable import AssemblywrightMacApp

private final class DeveloperPlanningHTTPFixture: URLProtocol, @unchecked Sendable {
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

private func planningRequestBody(_ request: URLRequest) throws -> Data {
  if let body = request.httpBody { return body }
  let stream = try #require(request.httpBodyStream)
  stream.open()
  defer { stream.close() }
  var result = Data()
  var buffer = [UInt8](repeating: 0, count: 4096)
  while true {
    let count = stream.read(&buffer, maxLength: buffer.count)
    if count < 0 { throw stream.streamError ?? URLError(.cannotDecodeContentData) }
    if count == 0 { return result }
    result.append(buffer, count: count)
    guard result.count <= 64 * 1024 else { throw URLError(.dataLengthExceedsMaximum) }
  }
}

@Suite("Developer brainstorming", .serialized)
@MainActor
final class DeveloperPlanningTests {
  private var configurationFiles: [URL] = []

  deinit {
    for file in configurationFiles { try? FileManager.default.removeItem(at: file) }
  }

  private func wire() -> [String: Any] {
    ["schema_version": 1, "feature_id": "72e07226-da43-4d64-ac84-bb6fb67d94cb", "revision": 10,
     "stage": "ready", "running": false, "availability": "available", "provider": "openai.codex",
     "model": "gpt-5.6-sol", "project": "example", "instruction": "Add a converter", "validation": "python tests.py",
     "model_target": "mac", "question": NSNull(),
     "understanding_summary": ["Purpose", "Users", "Scope", "Constraints", "Non-goals"],
     "assumptions": ["performance": "Interactive", "scale": "One user", "security_privacy": "Local data",
       "reliability_availability": "Clear errors", "maintenance_ownership": "Owner maintained", "other": []],
     "open_questions": [], "approaches": [], "selected_approach_id": "simple",
     "design_sections": [["id": "architecture", "title": "Architecture", "body": "Separate conversion from presentation.", "confirmed": true]],
     "decision_log": [["decision": "Use standard library", "alternatives": ["External framework"], "reason": "Small scope"]],
     "documents": ["understanding": "Requirements", "assumptions": "Assumptions", "decision_log": "Decision log",
       "design": "Design", "implementation_plan": "Implementation plan", "plan_sha256": String(repeating: "a", count: 64)],
     "error": NSNull(), "last_request_id": NSNull()]
  }
  private func decode(_ wire: [String: Any]) throws -> DeveloperPlanningSnapshot {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return try decoder.decode(DeveloperPlanningSnapshot.self, from: JSONSerialization.data(withJSONObject: wire))
  }

  private func model() throws -> DeveloperPlanningModel {
    let path = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try JSONSerialization.data(withJSONObject: [
      "endpoint": "http://127.0.0.1:17796", "token": "fixture-token",
    ]).write(to: path)
    configurationFiles.append(path)
    let configuration = URLSessionConfiguration.ephemeral
    configuration.protocolClasses = [DeveloperPlanningHTTPFixture.self]
    return DeveloperPlanningModel(configurationPath: path.path,
      session: URLSession(configuration: configuration))
  }

  @Test
  func queueApprovalRequiresCompleteConfirmedPlan() throws {
    #expect(try decode(wire()).canApprove)
    let blockers: [[String: Any]] = [
      ["running": true], ["stage": "questions"], ["availability": "unavailable"],
      ["open_questions": ["Which units?"]], ["selected_approach_id": NSNull()],
      ["design_sections": []], ["decision_log": []], ["documents": NSNull()],
      ["design_sections": [["id": "architecture", "title": "Architecture", "body": "Review this", "confirmed": false]]],
    ]
    for change in blockers {
      #expect(try !decode(wire().merging(change) { _, new in new }).canApprove)
    }
  }

  @Test
  func understandingCannotHideUnansweredQuestionsOrMissingAssumptions() throws {
    var value = wire()
    value["stage"] = "understanding"
    #expect(try decode(value).canConfirmUnderstanding)
    value["open_questions"] = ["Who will use this?"]
    #expect(try !decode(value).canConfirmUnderstanding)
    value["open_questions"] = []
    value["assumptions"] = NSNull()
    #expect(try !decode(value).canConfirmUnderstanding)
  }

  @Test
  func plannerIdentityAndCurrentSectionAreExplicit() throws {
    var value = wire()
    #expect(try decode(value).hasRequiredPlanner)
    value["model"] = "windows-local"
    #expect(try !decode(value).hasRequiredPlanner)
    value["model"] = "gpt-5.6-sol"
    value["provider"] = "local"
    #expect(try !decode(value).hasRequiredPlanner)
    value["design_sections"] = [
      ["id": "first", "title": "Approved", "body": "Already reviewed", "confirmed": true],
      ["id": "second", "title": "Testing", "body": "Review tests", "confirmed": false],
    ]
    #expect(try decode(value).currentSection?.id == "second")
  }

  @Test
  func startFreezesSelectedWindowsImplementationTarget() async throws {
    var response = wire()
    response["revision"] = 1
    response["stage"] = "questions"
    response["model_target"] = "windows"
    let responseBytes = try JSONSerialization.data(withJSONObject: response)
    DeveloperPlanningHTTPFixture.respond { request in
      #expect(request.httpMethod == "POST")
      #expect(request.url?.path == "/planning")
      #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer fixture-token")
      let bytes = try planningRequestBody(request)
      let body = try #require(JSONSerialization.jsonObject(with: bytes) as? [String: Any])
      #expect(body["action"] as? String == "start")
      #expect(body["model_target"] as? String == "windows")
      #expect(body["feature_id"] as? String == response["feature_id"] as? String)
      return (200, responseBytes)
    }
    let client = try model()
    let accepted = await client.start(id: response["feature_id"] as! String,
      project: "example", instruction: "Add a converter", validation: "python tests.py",
      modelTarget: "windows")
    #expect(accepted)
    #expect(client.snapshot?.modelTarget == "windows")
  }

  @Test
  func startRejectsUnknownImplementationTargetBeforeRequest() async throws {
    DeveloperPlanningHTTPFixture.respond { _ in
      Issue.record("An unsupported model target must not reach the runner")
      throw URLError(.badServerResponse)
    }
    let client = try model()
    let accepted = await client.start(id: UUID().uuidString.lowercased(), project: "example",
      instruction: "Add a converter", validation: "python tests.py", modelTarget: "cloud")
    #expect(!accepted)
    #expect(client.snapshot == nil)
  }

  @Test(.enabled(if: ProcessInfo.processInfo.environment["ASSEMBLYWRIGHT_PLANNING_LIVE_ID"] != nil))
  func observesInstalledPlanningWithoutMutatingIt() async throws {
    let env = ProcessInfo.processInfo.environment
    let config = try #require(env["ASSEMBLYWRIGHT_DEVELOPER_LIVE_CONFIG"])
    let id = try #require(env["ASSEMBLYWRIGHT_PLANNING_LIVE_ID"])
    let model = DeveloperPlanningModel(configurationPath: config)
    let task = Task { await model.observe(id: id) }
    defer { task.cancel() }
    for _ in 0..<100 {
      if model.snapshot != nil { break }
      try await Task.sleep(for: .milliseconds(100))
    }
    let state = try #require(model.snapshot)
    #expect(model.error == nil)
    #expect(state.featureId == id)
    #expect(state.hasRequiredPlanner)
  }
}
