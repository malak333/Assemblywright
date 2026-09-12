import Foundation
import AppKit
import SwiftUI
import Testing
@testable import AssemblywrightMacApp

@Suite("Developer project chat")
struct DeveloperProjectChatTests {
  @Test @MainActor
  func shiftReturnInsertsNewlineAtCursorAndReplacesSelection() throws {
    _ = NSApplication.shared
    var draft = "first second"
    let host = NSHostingView(rootView: DeveloperChatComposer(text: Binding(
      get: { draft }, set: { draft = $0 })))
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 140),
      styleMask: [.titled], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.contentView = host
    window.makeKeyAndOrderFront(nil)
    defer { window.close() }
    host.layoutSubtreeIfNeeded()
    RunLoop.main.run(until: Date().addingTimeInterval(0.1))

    func editor(in view: NSView) -> NSTextView? {
      if let textView = view as? NSTextView { return textView }
      return view.subviews.lazy.compactMap { editor(in: $0) }.first
    }
    let textView = try #require(editor(in: host))
    #expect(window.makeFirstResponder(textView))
    let shiftReturn = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero,
      modifierFlags: [.shift], timestamp: 0, windowNumber: window.windowNumber,
      context: nil, characters: "\r", charactersIgnoringModifiers: "\r",
      isARepeat: false, keyCode: 36))

    textView.setSelectedRange(NSRange(location: 5, length: 1))
    window.sendEvent(shiftReturn)
    #expect(textView.string == "first\nsecond")
    #expect(draft == "first\nsecond")

    textView.setSelectedRange(NSRange(location: 0, length: 0))
    window.sendEvent(shiftReturn)
    #expect(draft == "\nfirst\nsecond")
    #expect(window.firstResponder === textView)
  }

  @Test
  func preservesProjectAndContextDisclosure() throws {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let snapshot = try decoder.decode(DeveloperChatSnapshot.self, from: Data("""
      {"project":"temperature-demo","messages":[
        {"role":"user","content":"How do I open the GUI?"},
        {"role":"assistant","content":"On Windows run python temperature_gui.py."}],
       "running":false,"request_id":"request","error":null,
       "context_limit":262144,"context_tokens":1234,
       "context_files":["README.md","temperature_gui.py"],"omitted_messages":2,
       "tool_access":{"mode":"ask","revision":7,"available":true,
         "unavailable_reason":null,"execution_host":"windows"},
       "tool_actions":[{"id":"action-1","request_id":"request","tool":"shell",
         "summary":"Install PySide6","status":"pending_approval","output":null}],
       "pending_approval":{"id":"approval-1","request_id":"request",
         "summary":"Install PySide6","tool":"shell",
         "details":{"command":"python -m pip install PySide6","directory":"C:/project","estimate":1e100},
         "access_revision":7}}
      """.utf8))
    #expect(snapshot.project == "temperature-demo")
    #expect(snapshot.messages.map(\.role) == ["user", "assistant"])
    #expect(snapshot.messages[1].content.contains("Windows"))
    #expect(snapshot.contextLimit == 262144)
    #expect(snapshot.contextTokens == 1234)
    #expect(snapshot.contextFiles == ["README.md", "temperature_gui.py"])
    #expect(snapshot.omittedMessages == 2)
    #expect(!snapshot.running)
    #expect(snapshot.toolAccess?.mode == .ask)
    #expect(snapshot.toolAccess?.revision == 7)
    #expect(snapshot.toolAccess?.executionHost == "windows")
    #expect(snapshot.toolActions?.first?.status == "pending_approval")
    #expect(snapshot.pendingApproval?.accessRevision == 7)
    #expect(snapshot.pendingApproval?.detailText.contains("python -m pip install PySide6") == true)
    #expect(snapshot.pendingApproval?.detailText.contains("1e+100") == true)
  }

  @Test @MainActor
  func approvalViewPresentsExactDetailsAndDecisions() throws {
    _ = NSApplication.shared
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let snapshot = try decoder.decode(DeveloperChatSnapshot.self, from: Data("""
      {"project":"demo","messages":[],"running":true,"request_id":"request",
       "tool_access":{"mode":"ask","revision":4,"available":true,"execution_host":"windows"},
       "pending_approval":{"id":"approval","request_id":"request","summary":"Install PySide6",
         "tool":"shell","details":{"command":"python -m pip install PySide6","directory":"C:/demo"},
         "access_revision":4}}
      """.utf8))
    let approval = try #require(snapshot.pendingApproval)
    var decisions: [String] = []
    let host = NSHostingView(rootView: DeveloperToolApprovalView(
      approval: approval, project: "demo", disabled: false, decide: { decisions.append($0) }))
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 520, height: 240),
      styleMask: [.titled], backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    window.contentView = host
    window.makeKeyAndOrderFront(nil)
    defer { window.close() }
    host.layoutSubtreeIfNeeded()
    RunLoop.main.run(until: Date().addingTimeInterval(0.1))

    func descendants<T: NSView>(_ type: T.Type, in view: NSView) -> [T] {
      (view as? T).map { [$0] } ?? view.subviews.flatMap { descendants(type, in: $0) }
    }
    let labels = descendants(NSTextField.self, in: host).map(\.stringValue).joined(separator: "\n")
    #expect(labels.contains("Project: demo"))
    #expect(labels.contains("Execution computer: Windows"))
    #expect(labels.contains("python -m pip install PySide6"))
    #expect(labels.contains("C:/demo"))
    let buttons = descendants(NSButton.self, in: host)
    let approve = try #require(buttons.first { $0.title == "Approve once" })
    let deny = try #require(buttons.first { $0.title == "Deny" })
    #expect(approve.isEnabled)
    #expect(deny.isEnabled)
    approve.performClick(nil)
    deny.performClick(nil)
    #expect(decisions == ["approve", "deny"])
  }

  @Test @MainActor
  func accessAndApprovalRequestsBindProjectAndRevision() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let requests = root.appendingPathComponent("requests.jsonl")
    let script = #"""
import http.server,json,sys,urllib.parse
snapshot={'project':'demo','messages':[],'running':True,'request_id':'request-1',
 'tool_access':{'mode':'ask','revision':9,'available':True,'unavailable_reason':None,'execution_host':'windows'},
 'tool_actions':[],'pending_approval':{'id':'approval-1','request_id':'request-1','summary':'Install PySide6',
 'tool':'shell','details':{'command':'python -m pip install PySide6'},'access_revision':9}}
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self): self.reply(200,snapshot)
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  with open(sys.argv[1],'a') as f:f.write(json.dumps({'path':self.path,'body':body})+'\n')
  self.reply(200,snapshot)
 def reply(self,status,value):
  b=json.dumps(value).encode();self.send_response(status);self.send_header('Content-Length',str(len(b)));self.end_headers();self.wfile.write(b)
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
sys.stdout.buffer.write(server.server_port.to_bytes(4,'big'));sys.stdout.buffer.flush();server.serve_forever()
"""#
    let server = Process(), output = Pipe()
    server.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    server.arguments = ["python3", "-u", "-c", script, requests.path]
    server.standardInput = FileHandle.nullDevice
    server.standardOutput = output
    server.standardError = FileHandle.nullDevice
    try server.run()
    defer {
      if server.isRunning { server.terminate(); server.waitUntilExit() }
      try? output.fileHandleForReading.close()
    }
    let handshake = try #require(try output.fileHandleForReading.read(upToCount: 4))
    #expect(handshake.count == 4)
    let port = handshake.reduce(0) { ($0 << 8) | Int($1) }
    let config = root.appendingPathComponent("runtime.json")
    try JSONSerialization.data(withJSONObject: ["endpoint": "http://127.0.0.1:\(port)", "token": "fixture"])
      .write(to: config)
    let model = DeveloperProjectChatModel(configurationPath: config.path)
    let observer = Task { await model.observe(project: "demo") }
    defer { observer.cancel() }
    for _ in 0..<30 {
      if model.snapshot?.pendingApproval != nil { break }
      try await Task.sleep(for: .milliseconds(50))
    }
    #expect(model.snapshot?.pendingApproval != nil)
    await model.setAccessMode(.auto, renderedProject: "demo", expectedRevision: 9)
    await model.decideApproval("approve", renderedProject: "demo", approvalId: "approval-1",
      requestId: "request-1", accessRevision: 9)

    let lines = try String(contentsOf: requests, encoding: .utf8).split(separator: "\n")
    let values = try lines.map { try JSONSerialization.jsonObject(with: Data($0.utf8)) as! [String: Any] }
    #expect(values.count == 2)
    #expect(values[0]["path"] as? String == "/chat/access")
    let access = try #require(values[0]["body"] as? [String: Any])
    #expect(access["project"] as? String == "demo")
    #expect(access["mode"] as? String == "auto")
    #expect(access["expected_revision"] as? Int == 9)
    #expect(values[1]["path"] as? String == "/chat/approval")
    let approvalBody = try #require(values[1]["body"] as? [String: Any])
    #expect(approvalBody["request_id"] as? String == "request-1")
    #expect(approvalBody["approval_id"] as? String == "approval-1")
    #expect(approvalBody["access_revision"] as? Int == 9)
    #expect(approvalBody["decision"] as? String == "approve")
  }

  @Test @MainActor
  func rejectsApprovalWhenRenderedBindingIsStale() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let config = root.appendingPathComponent("runtime.json")
    try JSONSerialization.data(withJSONObject: ["endpoint": "http://127.0.0.1:1", "token": "fixture"])
      .write(to: config)
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let newer = try decoder.decode(DeveloperChatSnapshot.self, from: Data("""
      {"project":"demo","messages":[],"running":true,"request_id":"request-2",
       "tool_access":{"mode":"ask","revision":10,"available":true,"execution_host":"windows"},
       "pending_approval":{"id":"approval-2","request_id":"request-2","summary":"New action",
         "tool":"shell","details":{"command":"new command"},"access_revision":10}}
      """.utf8))
    let model = DeveloperProjectChatModel(configurationPath: config.path)
    model.select(project: "demo")
    model.snapshot = newer

    await model.setAccessMode(.full, renderedProject: "demo", expectedRevision: 9)
    await model.decideApproval("approve", renderedProject: "demo", approvalId: "approval-1",
      requestId: "request-1", accessRevision: 9)

    #expect(model.snapshot?.toolAccess?.revision == 10)
    #expect(model.snapshot?.toolAccess?.mode == .ask)
    #expect(model.snapshot?.pendingApproval?.id == "approval-2")
    #expect(model.error == nil)
    #expect(!model.resolvingApproval)
  }

  @Test
  func startConfirmationRetainsTargetAndCheckpoint() {
    let feature = DeveloperRunnerFeature(id: "original", project: "example", instruction: "change",
      validation: "tests", status: "paused", checkpoint: "applied", message: "", changedFiles: [],
      repairAttempts: 0, modelTarget: "windows")
    #expect(feature.startBinding["expected_feature_id"] as? String == "original")
    #expect(feature.startBinding["expected_model_target"] as? String == "windows")
    #expect(feature.startBinding["expected_status"] as? String == "paused")
    #expect(feature.startBinding["expected_checkpoint"] as? String == "applied")
  }
}
