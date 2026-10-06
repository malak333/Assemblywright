import Foundation
import Testing
@testable import AssemblywrightMacApp

private final class DeveloperPermissionsHTTPFixture: URLProtocol, @unchecked Sendable {
  nonisolated(unsafe) static var handler: ((URLRequest) throws -> (Int, Data))?
  override class func canInit(with request: URLRequest) -> Bool { true }
  override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
  override func startLoading() {
    do {
      let (status, data) = try Self.handler!(request)
      client?.urlProtocol(self, didReceive: HTTPURLResponse(url: request.url!, statusCode: status,
        httpVersion: nil, headerFields: nil)!, cacheStoragePolicy: .notAllowed)
      client?.urlProtocol(self, didLoad: data)
      client?.urlProtocolDidFinishLoading(self)
    } catch { client?.urlProtocol(self, didFailWithError: error) }
  }
  override func stopLoading() {}
}

@Suite("Developer global permissions", .serialized)
struct DeveloperPermissionsTests {
  private func value(_ mode: DeveloperToolAccessMode, _ revision: UInt64) -> DeveloperGlobalPermissions {
    DeveloperGlobalPermissions(mode: mode, revision: revision, available: true,
      executionHost: "windows")
  }

  @Test
  func acknowledgementRequiresExactGlobalModeAndNextRevision() {
    let original = value(.ask, 8)
    #expect(DeveloperPermissionsAcknowledgement.saved(value(.full, 9), from: original,
      requested: .full))
    #expect(!DeveloperPermissionsAcknowledgement.saved(value(.full, 8), from: original,
      requested: .full))
    #expect(!DeveloperPermissionsAcknowledgement.saved(value(.auto, 9), from: original,
      requested: .full))
    #expect(!DeveloperPermissionsAcknowledgement.saved(
      DeveloperGlobalPermissions(mode: .full, revision: 9, available: true,
        executionHost: "mac"), from: original, requested: .full))
  }

  @Test @MainActor
  func modelReadsAndWritesOnlyGlobalPermissionsRoute() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let requests = root.appendingPathComponent("requests.jsonl")
    let script = #"""
import http.server,json,sys
state={'mode':'ask','revision':7,'available':True,'execution_host':'windows'}
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  with open(sys.argv[1],'a') as f:f.write(json.dumps({'method':'GET','path':self.path})+'\n')
  self.reply(200,state)
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  with open(sys.argv[1],'a') as f:f.write(json.dumps({'method':'POST','path':self.path,'body':body})+'\n')
  state.update(mode=body['mode'],revision=8);self.reply(200,state)
 def reply(self,status,value):
  b=json.dumps(value).encode();self.send_response(status);self.send_header('Content-Length',str(len(b)));self.end_headers();self.wfile.write(b)
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
sys.stdout.buffer.write(server.server_port.to_bytes(4,'big'));sys.stdout.buffer.flush();server.serve_forever()
"""#
    let server = Process(), output = Pipe()
    server.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    server.arguments = ["python3", "-u", "-c", script, requests.path]
    server.standardOutput = output
    server.standardError = FileHandle.nullDevice
    try server.run()
    defer { if server.isRunning { server.terminate(); server.waitUntilExit() } }
    let handshake = try #require(try output.fileHandleForReading.read(upToCount: 4))
    let port = handshake.reduce(0) { ($0 << 8) | Int($1) }
    let config = root.appendingPathComponent("runtime.json")
    try JSONSerialization.data(withJSONObject: ["endpoint": "http://127.0.0.1:\(port)",
      "token": "fixture"]).write(to: config)
    let model = DeveloperPermissionsModel(configurationPath: config.path)

    await model.refresh()
    #expect(model.permissions == value(.ask, 7))
    await model.save(.full, expectedRevision: 7)
    #expect(model.permissions == value(.full, 8))
    let lines = try String(contentsOf: requests, encoding: .utf8).split(separator: "\n")
    let rows = try lines.map { try JSONSerialization.jsonObject(with: Data($0.utf8)) as! [String: Any] }
    #expect(rows.count == 2)
    #expect(rows.allSatisfy { $0["path"] as? String == "/permissions" })
    let body = try #require(rows[1]["body"] as? [String: Any])
    #expect(body["mode"] as? String == "full")
    #expect(body["expected_revision"] as? Int == 7)
    #expect(body["project"] == nil)
  }

  @Test @MainActor
  func olderRunnerFailsClosedWithoutProjectFallback() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let script = #"""
import http.server,json,sys
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  b=b'{"error":"Not found"}';self.send_response(404);self.send_header('Content-Length',str(len(b)));self.end_headers();self.wfile.write(b)
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
sys.stdout.buffer.write(server.server_port.to_bytes(4,'big'));sys.stdout.buffer.flush();server.serve_forever()
"""#
    let server = Process(), output = Pipe()
    server.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    server.arguments = ["python3", "-u", "-c", script]
    server.standardOutput = output
    server.standardError = FileHandle.nullDevice
    try server.run()
    defer { if server.isRunning { server.terminate(); server.waitUntilExit() } }
    let handshake = try #require(try output.fileHandleForReading.read(upToCount: 4))
    let port = handshake.reduce(0) { ($0 << 8) | Int($1) }
    let config = root.appendingPathComponent("runtime.json")
    try JSONSerialization.data(withJSONObject: ["endpoint": "http://127.0.0.1:\(port)",
      "token": "fixture"]).write(to: config)
    let model = DeveloperPermissionsModel(configurationPath: config.path)
    await model.refresh()
    #expect(model.permissions == nil)
    #expect(model.error == "Not found")
  }

  @Test @MainActor
  func staleSaveReceiptClearsAuthorityAndRequiresReload() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let config = root.appendingPathComponent("runtime.json")
    try JSONSerialization.data(withJSONObject: ["endpoint": "http://127.0.0.1:12345",
      "token": "fixture"]).write(to: config)
    DeveloperPermissionsHTTPFixture.handler = { request in
      let mode = request.httpMethod == "POST" ? "full" : "ask"
      return (200, Data("{\"mode\":\"\(mode)\",\"revision\":7,\"available\":true,\"execution_host\":\"windows\"}".utf8))
    }
    let configuration = URLSessionConfiguration.ephemeral
    configuration.protocolClasses = [DeveloperPermissionsHTTPFixture.self]
    let model = DeveloperPermissionsModel(configurationPath: config.path,
      session: URLSession(configuration: configuration))
    await model.refresh()
    #expect(model.permissions == value(.ask, 7))
    await model.save(.full, expectedRevision: 7)
    #expect(model.permissions == nil)
    #expect(model.error != nil)
  }
}
