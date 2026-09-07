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

  private func snapshot(_ revision: Int, mode: String = "supervised_developer") -> Data {
    Data("""
      {"mode":"\(mode)","host":"fixture-windows","workspace_root":"fixture",
       "revision":\(revision),"auto_run":true,"emergency_paused":false,
       "running":false,"queue":[],"review_required":true,"review_provider":"openai.codex",
       "review_model":"gpt-5.6-sol","planning_required":true,"planning_provider":"openai.codex",
       "planning_model":"gpt-5.6-sol","planning_running":false,"planning_sessions":[]}
      """.utf8)
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
