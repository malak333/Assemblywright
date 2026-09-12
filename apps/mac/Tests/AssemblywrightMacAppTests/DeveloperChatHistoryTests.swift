import Foundation
import Testing
@testable import AssemblywrightMacApp

@Suite("Developer conversation history")
struct DeveloperChatHistoryTests {
  @Test(.enabled(if: ProcessInfo.processInfo.environment["ASSEMBLYWRIGHT_DEVELOPER_LIVE_CONFIG"] != nil))
  @MainActor
  func installedWindowsHistoryReopensExactConversationsWithoutSending() async throws {
    let path = try #require(ProcessInfo.processInfo.environment["ASSEMBLYWRIGHT_DEVELOPER_LIVE_CONFIG"])
    let model = DeveloperProjectChatModel(configurationPath: path)
    await model.refreshHistory()
    #expect(model.historyError == nil)
    #expect(model.historyLoaded)
    let rows = model.conversations
    try #require(!rows.isEmpty, "This read-only live check requires an existing saved chat.")
    for row in rows.prefix(5) {
      model.select(project: row.project, chatId: row.id)
      await model.refresh()
      let snapshot = try #require(model.snapshot)
      #expect(model.error == nil)
      #expect(snapshot.historySupported == true)
      #expect(snapshot.project == row.project)
      #expect(snapshot.chatId == row.id)
      let sequences = snapshot.messages.compactMap(\.sequence)
      #expect(sequences.count == snapshot.messages.count)
      #expect(Set(sequences).count == sequences.count)
    }
    #expect(!model.sending)
    #expect(model.draft.isEmpty)
  }

  @Test
  func accessControlBlocksOtherChatAndRecoveryStates() {
    func state(chatRunning: Bool = false) -> DeveloperRunnerSnapshot {
      .init(mode: "supervised_developer", host: "fixture", workspaceRoot: "/fixture", revision: 1,
        autoRun: false, emergencyPaused: false, running: false, chatRunning: chatRunning,
        queue: [], repairLimit: 3, repairActive: false, modelTargets: nil,
        reviewRequired: true, reviewProvider: "openai.codex", reviewModel: "gpt-5.6-sol",
        planningRequired: true, planningProvider: "openai.codex", planningModel: "gpt-5.6-sol",
        planningRunning: false, planningSessions: [])
    }
    #expect(state().canChangeChatAccess)
    #expect(!state(chatRunning: true).canChangeChatAccess)
    var recovery = state()
    recovery.githubSetupUnresolved = true
    #expect(!recovery.canChangeChatAccess)
    recovery = state()
    recovery.githubPublicationUnresolved = true
    #expect(!recovery.canChangeChatAccess)
  }

  @Test @MainActor
  func draftsAndAttachmentsStayWithEachConversation() throws {
    let model = DeveloperProjectChatModel(configurationPath: "/unused")
    let attachment = DeveloperChatAttachment(name: "notes.txt", mediaType: "text/plain",
      dataBase64: Data("draft notes".utf8).base64EncodedString())
    model.select(project: "alpha", chatId: "a")
    model.draft.message = "First topic"
    model.draft.attachments = [attachment]
    model.draft.error = "First draft error"
    model.select(project: "alpha", chatId: "b")
    #expect(model.draft.isEmpty)
    #expect(model.draft.error == nil)
    model.draft.message = "Second topic"
    model.select(project: "beta", chatId: "c")
    #expect(model.draft.isEmpty)
    model.select(project: "alpha", chatId: "a")
    #expect(model.draft.message == "First topic")
    #expect(model.draft.attachments == [attachment])
    #expect(model.draft.error == "First draft error")
    model.select(project: "alpha", chatId: "b")
    #expect(model.draft.message == "Second topic")
    #expect(model.draft.attachments.isEmpty)
  }

  @Test @MainActor
  func creationRenameAndSendCarryExactConversation() async throws {
    let fixture = try ChatHistoryFixture()
    defer { fixture.stop() }
    let model = DeveloperProjectChatModel(configurationPath: fixture.config.path)
    model.select(project: "alpha", chatId: "a")
    await model.refresh()
    let selected = try #require(await model.createConversation(project: "alpha"))
    #expect(selected.project == "alpha")
    #expect(selected.chatId == "created")
    model.select(project: selected.project, chatId: selected.chatId)
    await model.refresh()
    #expect(await model.rename(title: "Different topic", renderedSelection: selected, expectedRevision: 1))
    model.draft.message = "Question for this chat"
    #expect(await model.send(message: model.draft.message))
    #expect(model.draft.isEmpty)
    let calls = try await fixture.calls()
    let create = try #require(calls.first { $0["path"] as? String == "/chat/conversations" })
    let createBody = try #require(create["body"] as? [String: Any])
    #expect(createBody["project"] as? String == "alpha")
    #expect(createBody["reuse_chat_id"] as? String == "a")
    #expect(UUID(uuidString: createBody["id"] as? String ?? "") != nil)
    let rename = try #require(calls.first { $0["path"] as? String == "/chat/rename" }?["body"] as? [String: Any])
    #expect(rename["chat_id"] as? String == "created")
    #expect(rename["expected_revision"] as? Int == 1)
    let send = try #require(calls.first { $0["path"] as? String == "/chat" }?["body"] as? [String: Any])
    #expect(send["chat_id"] as? String == "created")
    #expect(send["message"] as? String == "Question for this chat")
  }

  @Test @MainActor
  func draftBearingChatIsNeverOfferedForEmptyReuse() async throws {
    let fixture = try ChatHistoryFixture()
    defer { fixture.stop() }
    let model = DeveloperProjectChatModel(configurationPath: fixture.config.path)
    model.select(project: "alpha", chatId: "a")
    model.draft.message = "Unsent"
    _ = await model.createConversation(project: "alpha")
    let body = try #require(try await fixture.calls().first { $0["path"] as? String == "/chat/conversations" }?["body"] as? [String: Any])
    #expect(body["reuse_chat_id"] == nil)
    #expect(model.draft.message == "Unsent")
  }

  @Test @MainActor
  func uncertainRetriesRemainBoundToEachChatAfterNavigation() async throws {
    let fixture = try ChatHistoryFixture()
    defer { fixture.stop() }
    try await fixture.control(["fail_sends": true])
    let model = DeveloperProjectChatModel(configurationPath: fixture.config.path)
    model.select(project: "alpha", chatId: "a")
    #expect(await model.send(message: "Same text") == false)
    model.select(project: "alpha", chatId: "b")
    #expect(await model.send(message: "Same text") == false)
    model.select(project: "alpha", chatId: "a")
    #expect(await model.send(message: "Same text") == false)
    let calls = try await fixture.calls()
    let bodies = try calls.map { try #require($0["body"] as? [String: Any]) }
    #expect(bodies.count == 3)
    #expect(bodies[0]["chat_id"] as? String == "a")
    #expect(bodies[1]["chat_id"] as? String == "b")
    #expect(bodies[2]["chat_id"] as? String == "a")
    #expect(bodies[0]["id"] as? String == bodies[2]["id"] as? String)
    #expect(bodies[0]["id"] as? String != bodies[1]["id"] as? String)
  }

  @Test @MainActor
  func lateResponseCannotReplaceReopenedChatOrNewerState() async throws {
    let fixture = try ChatHistoryFixture()
    defer { fixture.stop() }
    try await fixture.control(["block_chat": "a"])
    let model = DeveloperProjectChatModel(configurationPath: fixture.config.path)
    model.select(project: "alpha", chatId: "a")
    let first = Task { await model.refresh() }
    try await fixture.waitUntilBlocked()
    model.select(project: "alpha", chatId: "b")
    await model.refresh()
    #expect(model.snapshot?.chatId == "b")
    model.select(project: "alpha", chatId: "a")
    await model.refresh()
    #expect(model.snapshot?.title == "a read 2")
    try await fixture.control(["release": true])
    await first.value
    #expect(model.snapshot?.title == "a read 2")
    #expect(model.error == nil)
  }

  @Test @MainActor
  func rejectsWrongConversationAndMissingHistoryCapability() async throws {
    let fixture = try ChatHistoryFixture()
    defer { fixture.stop() }
    let model = DeveloperProjectChatModel(configurationPath: fixture.config.path)
    model.select(project: "alpha", chatId: "a")
    try await fixture.control(["wrong_chat": true])
    await model.refresh()
    #expect(model.snapshot == nil)
    #expect(model.error != nil)
    try await fixture.control(["wrong_chat": false, "history_supported": false])
    await model.refresh()
    #expect(model.snapshot == nil)
    #expect(model.error != nil)
  }

  @Test @MainActor
  func olderMessagesSurviveSubsequentPollingWithoutDuplicates() async throws {
    let fixture = try ChatHistoryFixture()
    defer { fixture.stop() }
    let model = DeveloperProjectChatModel(configurationPath: fixture.config.path)
    model.select(project: "alpha", chatId: "a")
    await model.refresh()
    #expect(model.snapshot?.messages.compactMap(\.sequence) == [3, 4])
    #expect(model.hasEarlierMessages)
    await model.loadEarlier()
    #expect(model.snapshot?.messages.compactMap(\.sequence) == [1, 2, 3, 4])
    #expect(!model.hasEarlierMessages)
    await model.refresh()
    #expect(model.snapshot?.messages.compactMap(\.sequence) == [1, 2, 3, 4])
    model.select(project: "alpha", chatId: "b")
    await model.refresh()
    #expect(model.snapshot?.messages.compactMap(\.sequence) == [3, 4])
  }

  @Test @MainActor
  func repairShortcutSynchronizesProjectAndPreservesPreviousDraft() async throws {
    let fixture = try ChatHistoryFixture()
    defer { fixture.stop() }
    let model = DeveloperProjectChatModel(configurationPath: fixture.config.path)
    model.select(project: "beta", chatId: "b")
    model.draft.message = "Keep my other topic"
    let shortcut = Task {
      await model.prepareRepairConversation(project: "alpha", currentChatId: "a", question: "Diagnose this failure")
    }
    // Model synchronization can run after the shortcut starts, as SwiftUI's two
    // shared AppStorage changes are delivered independently.
    for _ in 0..<100 {
      if model.project == "alpha" { break }
      await Task.yield()
    }
    model.select(project: "alpha", chatId: "a")
    let created = try #require(await shortcut.value)
    #expect(created == model.selection)
    #expect(model.draft.message == "Diagnose this failure")
    model.select(project: "beta", chatId: "b")
    #expect(model.draft.message == "Keep my other topic")
  }

  @Test @MainActor
  func sendCapturedBeforeNavigationCannotTargetAnotherChat() async throws {
    let fixture = try ChatHistoryFixture()
    defer { fixture.stop() }
    let model = DeveloperProjectChatModel(configurationPath: fixture.config.path)
    model.select(project: "alpha", chatId: "a")
    let rendered = model.selection
    model.select(project: "alpha", chatId: "b")
    #expect(await model.send(message: "Belongs to a", renderedSelection: rendered) == false)
    #expect(try await fixture.calls().isEmpty)
  }

  @Test @MainActor
  func activeChatStopAndApprovalsRemainBoundWhileBrowsing() async throws {
    let fixture = try ChatHistoryFixture()
    defer { fixture.stop() }
    try await fixture.control(["active": true])
    let model = DeveloperProjectChatModel(configurationPath: fixture.config.path)
    model.select(project: "alpha", chatId: "a")
    await model.refresh()
    let active = try #require(model.activeChat)
    model.select(project: "alpha", chatId: "b")
    try await fixture.control(["active_title": "Renamed active topic"])
    await model.refresh()
    #expect(model.activeChat?.title == "Renamed active topic")
    // The fixture deliberately repeats approval/request identifiers across chats.
    // A stale rendered chat identity still must not send an approval.
    await model.decideApproval("approve", renderedProject: "alpha", renderedChatId: "a",
      approvalId: "approval", requestId: "request", accessRevision: 9)
    #expect(try await fixture.calls().isEmpty)
    await model.decideApproval("deny", renderedProject: "alpha", renderedChatId: "b",
      approvalId: "approval", requestId: "request", accessRevision: 9)
    await model.cancel(active)
    #expect(model.snapshot?.chatId == "b")
    let calls = try await fixture.calls()
    let approval = try #require(calls.first { $0["path"] as? String == "/chat/approval" }?["body"] as? [String: Any])
    #expect(approval["chat_id"] as? String == "b")
    #expect(approval["decision"] as? String == "deny")
    let stop = try #require(calls.first { $0["path"] as? String == "/chat/cancel" }?["body"] as? [String: Any])
    #expect(stop["chat_id"] as? String == "a")
    #expect(stop["project"] as? String == "alpha")
    #expect(stop["id"] as? String == "request")
  }
}

/// A real loopback HTTP process exercises URL/body decoding, asynchronous races,
/// and Swift state isolation without invoking models or an installed runner.
@MainActor private final class ChatHistoryFixture {
  let root: URL
  let config: URL
  private let process = Process()
  private let output = Pipe()
  private let endpoint: URL

  init() throws {
    root = FileManager.default.temporaryDirectory.appendingPathComponent("chat-history-" + UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    config = root.appendingPathComponent("runtime.json")
    let script = #"""
import http.server,json,threading,urllib.parse
state={'calls':[],'reads':{},'history_supported':True,'blocked':False}
release=threading.Event()
class Handler(http.server.BaseHTTPRequestHandler):
 def reply(self,value,status=200):
  raw=json.dumps(value).encode();self.send_response(status);self.send_header('Content-Length',str(len(raw)));self.end_headers()
  try:self.wfile.write(raw)
  except (BrokenPipeError,ConnectionResetError):pass
 def snapshot(self,project,chat,before=None):
  sequences=[1,2] if before else [3,4]
  return {'project':project,'chat_id':'other' if state.get('wrong_chat') else chat,
   'title':chat+' read '+str(state['reads'].get(chat,0)),'revision':1,
   'history_supported':state['history_supported'],'messages':[{'role':'assistant','content':chat+' '+str(n),'sequence':n} for n in sequences],
   'next_before':None if before else 'older','running':False,'request_id':'request',
   'active_chat':self.active(),'tool_access':{'mode':'ask','revision':9,'available':True,'execution_host':'windows'},
   'pending_approval':{'id':'approval','request_id':'request','summary':'Action','tool':'shell','details':{},'access_revision':9}}
 def active(self):
  return {'project':'alpha','chat_id':'a','request_id':'request','title':state.get('active_title','Active topic')} if state.get('active') else None
 def do_GET(self):
  path=urllib.parse.urlsplit(self.path);query=urllib.parse.parse_qs(path.query)
  if path.path=='/fixture':return self.reply(state)
  if path.path=='/chat/conversations':return self.reply({'conversations':[],'next_cursor':None,'active_chat':self.active()})
  project=query.get('project',['alpha'])[0];chat=query.get('chat_id',['legacy'])[0]
  state['reads'][chat]=state['reads'].get(chat,0)+1
  value=self.snapshot(project,chat,query.get('before'))
  if state.get('block_chat')==chat and state['reads'][chat]==1:
   state['blocked']=True;release.wait(10)
  self.reply(value)
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  if self.path=='/fixture':
   state.update(body)
   if body.get('release'):release.set()
   return self.reply({})
  state['calls'].append({'path':self.path,'body':body})
  if self.path=='/chat' and state.get('fail_sends'):return self.reply({'error':'Retry the exact request'},503)
  chat='created' if self.path=='/chat/conversations' else body.get('chat_id','legacy')
  self.reply(self.snapshot(body.get('project','alpha'),chat))
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
import sys
sys.stdout.buffer.write(server.server_port.to_bytes(4,'big'));sys.stdout.buffer.flush();server.serve_forever()
"""#
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = ["python3", "-u", "-c", script]
    process.standardInput = FileHandle.nullDevice
    process.standardOutput = output
    process.standardError = FileHandle.nullDevice
    try process.run()
    let data = try #require(try output.fileHandleForReading.read(upToCount: 4))
    let port = data.reduce(0) { ($0 << 8) | Int($1) }
    endpoint = try #require(URL(string: "http://127.0.0.1:\(port)/fixture"))
    try JSONSerialization.data(withJSONObject: ["endpoint": "http://127.0.0.1:\(port)", "token": "fixture"])
      .write(to: config)
  }

  func stop() {
    if process.isRunning { process.terminate(); process.waitUntilExit() }
    try? output.fileHandleForReading.close()
    try? FileManager.default.removeItem(at: root)
  }

  func control(_ values: [String: Any]) async throws {
    var request = URLRequest(url: endpoint)
    request.httpMethod = "POST"
    request.httpBody = try JSONSerialization.data(withJSONObject: values)
    _ = try await URLSession.shared.data(for: request)
  }

  func state() async throws -> [String: Any] {
    let (data, _) = try await URLSession.shared.data(from: endpoint)
    return try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
  }

  func calls() async throws -> [[String: Any]] {
    try await state()["calls"] as? [[String: Any]] ?? []
  }

  func waitUntilBlocked() async throws {
    for _ in 0..<100 {
      if try await state()["blocked"] as? Bool == true { return }
      try await Task.sleep(for: .milliseconds(20))
    }
    Issue.record("Fixture request did not reach the expected synchronization point")
  }
}
