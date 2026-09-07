import Foundation
import Testing
@testable import AssemblywrightMacApp

@Suite("Developer background connection")
struct DeveloperConnectionTests {
  @Test
  @MainActor
  func staleSupervisorStatusCannotClaimConnection() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let model = DeveloperConnectionModel(configurationPath: root.appendingPathComponent("runtime.json").path)
    let now = Date()
    func write(_ phase: String, age: Double) throws {
      let bytes = try JSONSerialization.data(withJSONObject: ["phase": phase,
        "message": "Check the configured Windows connection.", "updated_at": now.timeIntervalSince1970 - age,
        "attempt": 3])
      try bytes.write(to: root.appendingPathComponent("connection-status.json"), options: .atomic)
      model.refresh(at: now)
    }
    try write("needs_attention", age: 1)
    #expect(model.needsAttention)
    try write("reconnecting", age: 2)
    #expect(model.title == "Reconnecting to Windows…")
    try write("connected", age: 1)
    #expect(model.title == "Connected to Windows")
    try write("connected", age: 30)
    #expect(model.status == nil)
    #expect(model.title == "Connecting to Windows…")
    try write("connected", age: -30)
    #expect(model.status == nil)
    #expect(model.title == "Connecting to Windows…")
  }

  @Test
  @MainActor
  func runtimeCredentialRefreshAndDisconnectUseFreshServerState() async throws {
    let script = #"""
import http.server,json,sys
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  allowed=self.headers.get('Authorization')=='Bearer refreshed-fixture-token'
  body=json.dumps({'mode':'supervised_developer','host':'fixture','workspace_root':'C:/fixture','revision':1,'auto_run':False,'emergency_paused':False,'running':False,'chat_running':False,'queue':[],'review_required':True,'review_provider':'openai.codex','review_model':'gpt-5.6-sol','planning_required':True,'planning_provider':'openai.codex','planning_model':'gpt-5.6-sol','planning_running':False,'planning_sessions':[]} if allowed else {'error':'Token changed'}).encode()
  self.send_response(200 if allowed else 401);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
sys.stdout.buffer.write(server.server_port.to_bytes(4,'big'));sys.stdout.buffer.flush();server.serve_forever()
"""#
    let server = Process(), output = Pipe()
    server.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    server.arguments = ["python3", "-u", "-c", script]
    server.standardInput = FileHandle.nullDevice
    server.standardOutput = output
    server.standardError = FileHandle.nullDevice
    try server.run()
    defer {
      if server.isRunning { server.terminate(); server.waitUntilExit() }
      try? output.fileHandleForReading.close()
    }
    let received = try output.fileHandleForReading.read(upToCount: 4)
    let handshake = try #require(received)
    #expect(handshake.count == 4)
    let port = handshake.reduce(0) { ($0 << 8) | Int($1) }
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let config = root.appendingPathComponent("runtime.json")
    func configure(_ token: String) throws {
      try JSONSerialization.data(withJSONObject: ["endpoint": "http://127.0.0.1:\(port)", "token": token])
        .write(to: config, options: .atomic)
    }
    try configure("old-fixture-token")
    let model = DeveloperRunnerModel(configurationPath: config.path)
    let task = Task { await model.observe() }
    defer { task.cancel() }
    for _ in 0..<50 {
      if model.error != nil { break }
      try await Task.sleep(for: .milliseconds(100))
    }
    #expect(model.snapshot == nil)
    #expect(model.error != nil)
    try configure("refreshed-fixture-token")
    for _ in 0..<50 {
      if model.snapshot != nil { break }
      try await Task.sleep(for: .milliseconds(100))
    }
    #expect(model.snapshot?.host == "fixture")
    #expect(model.error == nil)
    server.terminate(); server.waitUntilExit()
    for _ in 0..<120 {
      if model.snapshot == nil { break }
      try await Task.sleep(for: .milliseconds(100))
    }
    #expect(model.snapshot == nil)
    #expect(model.error != nil)
  }
}
