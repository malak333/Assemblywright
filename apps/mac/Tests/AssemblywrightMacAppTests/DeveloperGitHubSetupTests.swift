import Foundation
import Testing
@testable import AssemblywrightMacApp

@Suite("Developer GitHub setup")
struct DeveloperGitHubSetupTests {
  @Test(.enabled(if: ProcessInfo.processInfo.environment["ASSEMBLYWRIGHT_DEVELOPER_LIVE_CONFIG"] != nil))
  @MainActor
  func swiftSetupModelObservesInstalledWindowsRunner() async throws {
    let path = try #require(ProcessInfo.processInfo.environment["ASSEMBLYWRIGHT_DEVELOPER_LIVE_CONFIG"])
    let model = DeveloperGitHubSetupModel(configurationPath: path)
    let observation = Task { await model.observe() }
    defer { observation.cancel() }
    for _ in 0..<100 {
      if model.snapshot != nil { break }
      try await Task.sleep(for: .milliseconds(100))
    }
    let snapshot = try #require(model.snapshot)
    #expect(model.error == nil)
    #expect(snapshot.isWellFormed)
  }

  private func decode(_ json: String) throws -> DeveloperGitHubSetupSnapshot {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return try decoder.decode(DeveloperGitHubSetupSnapshot.self, from: Data(json.utf8))
  }

  private func snapshot(revision: Int = 1, busy: Bool = false) throws
    -> DeveloperGitHubSetupSnapshot {
    try decode(#"""
      {"revision":\#(revision),"account":{"state":"signed_in","login":"Owner-1","message":"Ready"},
       "repositories":[],"repository_page":1,"has_more":false,"busy":\#(busy),"can_mutate":true}
      """#)
  }

  @Test
  func acceptsUnloadedRepositoryStateWithoutAcceptingItAsAPage() throws {
    let initial = #"""
      {"revision":2,"account":{"state":"unknown","login":null,"message":"Refresh"},
       "repositories":[],"repository_page":0,"has_more":false,"busy":false,"can_mutate":true}
      """#
    let value = try decode(initial)
    #expect(value.isWellFormed)
    #expect(!DeveloperGitHubSetupAcknowledgement.page(value, expectedPage: 0, after: 1))
    #expect(!DeveloperGitHubSetupAcknowledgement.page(value, expectedPage: 1, after: 1))
    #expect(!(try decode(initial.replacingOccurrences(of: "\"has_more\":false",
      with: "\"has_more\":true"))).isWellFormed)
    #expect(!(try decode(initial.replacingOccurrences(of: "\"repository_page\":0",
      with: "\"repository_page\":-1"))).isWellFormed)
    #expect(!(try decode(initial.replacingOccurrences(of: "\"repositories\":[]",
      with: #"""
      "repositories":[{"name_with_owner":"Owner-1/demo","url":"https://github.com/Owner-1/demo",
       "visibility":"private","default_branch":"main","can_push":true}]
      """#))).isWellFormed)
  }

  @Test
  func validatesAccountRepositoriesAndTransientChallenge() throws {
    let operation = UUID().uuidString.lowercased()
    let value = try decode(#"""
      {"revision":2,"account":{"state":"signed_out","login":null,"message":"Sign in"},
       "repositories":[{"name_with_owner":"Owner-1/demo","url":"https://github.com/Owner-1/demo",
        "visibility":"private","default_branch":"main","can_push":true}],
       "repository_page":1,"has_more":false,
       "sign_in":{"operation_id":"\#(operation)","user_code":"AB12-CD34",
        "verification_url":"https://github.com/login/device","state":"waiting","message":"Continue"},
       "busy":true,"can_mutate":true}
      """#)
    #expect(value.isWellFormed)
    #expect(value.repositories.first?.canSelect == true)
    #expect(value.signIn?.hasSafeChallenge == true)
    #expect(DeveloperGitHubSetupValidation.deviceURL("https://github.com.evil.test/login/device") == nil)
    #expect(!DeveloperGitHubSetupValidation.validDeviceCode("abcd-1234"))
    #expect(!DeveloperGitHubSetupValidation.validLogin("-owner"))

    let malformed = try decode(#"""
      {"revision":2,"account":{"state":"signed_out","login":null,"message":"Sign in"},
       "repositories":[],"repository_page":1,"has_more":false,
       "sign_in":{"operation_id":"\#(operation)","user_code":"AB12-CD34",
        "verification_url":"https://github.com.evil.test/login/device","state":"waiting","message":""},
       "busy":true,"can_mutate":true}
      """#)
    #expect(!malformed.isWellFormed)

    let empty = try decode(#"""
      {"revision":3,"account":{"state":"signed_in","login":"Owner-1","message":"Ready"},
       "repositories":[{"name_with_owner":"Owner-1/empty","url":"https://github.com/Owner-1/empty",
        "visibility":"public","default_branch":"","can_push":true}],
       "repository_page":1,"has_more":false,"busy":false,"can_mutate":true}
      """#)
    #expect(empty.isWellFormed)
    #expect(empty.repositories.first?.canSelect == false)

    let internalRepository = DeveloperGitHubRepository(nameWithOwner: "Owner-1/internal-demo",
      url: "https://github.com/Owner-1/internal-demo", visibility: "internal",
      defaultBranch: "main", canPush: true)
    #expect(internalRepository.isWellFormed)
    #expect(internalRepository.canSelect)
    let unknownVisibility = DeveloperGitHubRepository(nameWithOwner: "Owner-1/demo",
      url: "https://github.com/Owner-1/demo", visibility: "unknown",
      defaultBranch: "main", canPush: true)
    #expect(!unknownVisibility.canSelect)
  }

  @Test
  func emptyExistingRepositoryIsAnObservationNotSuccessfulCreation() throws {
    for state in ["existing", "attention", "succeeded"] {
      let value = try decode(#"""
        {"revision":2,"account":{"state":"signed_in","login":"Owner-1","message":"Ready"},
         "repositories":[],"repository_page":0,"has_more":false,"busy":false,"can_mutate":true,
         "creation":{"operation_id":"00000000-0000-4000-8000-000000000001",
          "repository_url":"https://github.com/Owner-1/demo","repository_id":"42",
          "name_with_owner":"Owner-1/demo","visibility":"public","default_branch":"",
          "state":"\#(state)","message":"Observed"}}
        """#)
      #expect(value.isWellFormed == (state != "succeeded"))
    }
  }

  @Test
  func requestBodiesBindRevisionOperationAccountAndVisibility() {
    let operation = UUID()
    let create = DeveloperGitHubSetupRequest.create(operationID: operation,
      expectedLogin: "Owner-1", name: "demo", visibility: "private", revision: 8)
    #expect(create["action"] as? String == "create_repository")
    #expect(create["operation_id"] as? String == operation.uuidString.lowercased())
    #expect(create["expected_login"] as? String == "Owner-1")
    #expect(create["name"] as? String == "demo")
    #expect(create["visibility"] as? String == "private")
    #expect(create["expected_revision"] as? UInt64 == 8)
    let page = DeveloperGitHubSetupRequest.list(page: 2, revision: 9)
    #expect(page["page"] as? Int == 2)
    let reconcile = DeveloperGitHubSetupRequest.signInAction("reconcile_sign_in",
      operationID: operation.uuidString.lowercased(), revision: 10)
    #expect(reconcile["action"] as? String == "reconcile_sign_in")
  }

  @Test
  func acknowledgementsRejectStaleOrMismatchedEffects() throws {
    let operation = UUID()
    let creation = try decode(#"""
      {"revision":4,"account":{"state":"signed_in","login":"Owner-1","message":"Ready"},
       "repositories":[],"repository_page":1,"has_more":false,"busy":true,"can_mutate":true,
       "creation":{"operation_id":"\#(operation.uuidString.lowercased())","repository_url":null,
        "repository_id":null,"name_with_owner":"owner-1/demo","visibility":"private",
        "default_branch":null,"state":"attention","message":"Creation needs reconciliation"}}
      """#)
    #expect(DeveloperGitHubSetupAcknowledgement.create(creation, operationID: operation,
      expectedLogin: "OWNER-1", name: "Demo", visibility: "private", after: 3))
    #expect(!DeveloperGitHubSetupAcknowledgement.create(creation, operationID: operation,
      expectedLogin: "other", name: "demo", visibility: "private", after: 3))
    #expect(!DeveloperGitHubSetupAcknowledgement.create(creation, operationID: operation,
      expectedLogin: "owner-1", name: "demo", visibility: "private", after: 4))

    let absent = try decode(#"""
      {"revision":5,"account":{"state":"signed_in","login":"Owner-1","message":"Ready"},
       "repositories":[],"repository_page":1,"has_more":false,"busy":false,"can_mutate":true,
       "creation":{"operation_id":"\#(operation.uuidString.lowercased())","repository_url":null,
        "repository_id":null,"name_with_owner":"Owner-1/demo","visibility":"private","default_branch":null,
        "state":"absent","message":"No repository was created"}}
      """#)
    #expect(DeveloperGitHubSetupAcknowledgement.reconcileCreation(absent,
      expected: try #require(creation.creation), after: 4))

    let changedTarget = try decode(#"""
      {"revision":5,"account":{"state":"signed_in","login":"Owner-1","message":"Ready"},
       "repositories":[],"repository_page":1,"has_more":false,"busy":false,"can_mutate":true,
       "creation":{"operation_id":"\#(operation.uuidString.lowercased())",
        "repository_url":"https://github.com/Owner-1/other","repository_id":"repo-2",
        "name_with_owner":"Owner-1/other","visibility":"private","default_branch":"main",
        "state":"succeeded","message":"Created"}}
      """#)
    #expect(!DeveloperGitHubSetupAcknowledgement.reconcileCreation(changedTarget,
      expected: try #require(creation.creation), after: 4))

    let changedVisibility = try decode(#"""
      {"revision":5,"account":{"state":"signed_in","login":"Owner-1","message":"Ready"},
       "repositories":[],"repository_page":1,"has_more":false,"busy":false,"can_mutate":true,
       "creation":{"operation_id":"\#(operation.uuidString.lowercased())",
        "repository_url":null,"repository_id":null,"name_with_owner":"Owner-1/demo",
        "visibility":"public","default_branch":null,"state":"absent","message":"Absent"}}
      """#)
    #expect(!DeveloperGitHubSetupAcknowledgement.reconcileCreation(changedVisibility,
      expected: try #require(creation.creation), after: 4))

    let expectedID = try decode(#"""
      {"revision":4,"account":{"state":"signed_in","login":"Owner-1","message":"Ready"},
       "repositories":[],"repository_page":1,"has_more":false,"busy":false,"can_mutate":true,
       "creation":{"operation_id":"\#(operation.uuidString.lowercased())",
        "repository_url":"https://github.com/Owner-1/demo","repository_id":"repo-1",
        "name_with_owner":"Owner-1/demo","visibility":"private","default_branch":null,
        "state":"attention","message":"Attention"}}
      """#)
    let changedID = try decode(#"""
      {"revision":5,"account":{"state":"signed_in","login":"Owner-1","message":"Ready"},
       "repositories":[],"repository_page":1,"has_more":false,"busy":false,"can_mutate":true,
       "creation":{"operation_id":"\#(operation.uuidString.lowercased())",
        "repository_url":"https://github.com/Owner-1/demo","repository_id":"repo-2",
        "name_with_owner":"Owner-1/demo","visibility":"private","default_branch":"main",
        "state":"succeeded","message":"Created"}}
      """#)
    #expect(!DeveloperGitHubSetupAcknowledgement.reconcileCreation(changedID,
      expected: try #require(expectedID.creation), after: 4))
  }

  @Test
  func signInAcknowledgementBindsCurrentAccountButHistoryMayExpire() throws {
    let operation = UUID()
    let historicalExpired = try decode(#"""
      {"revision":4,"account":{"state":"signed_out","login":null,"message":"Expired"},
       "repositories":[],"repository_page":1,"has_more":false,"busy":false,"can_mutate":true,
       "sign_in":{"operation_id":"\#(operation.uuidString.lowercased())","user_code":null,
        "verification_url":null,"state":"succeeded","message":"Earlier sign-in succeeded"}}
      """#)
    #expect(historicalExpired.isWellFormed)
    #expect(!DeveloperGitHubSetupAcknowledgement.begin(historicalExpired,
      operationID: operation, after: 3))

    let currentSuccess = try decode(#"""
      {"revision":4,"account":{"state":"signed_in","login":"Owner-1","message":"Ready"},
       "repositories":[],"repository_page":1,"has_more":false,"busy":false,"can_mutate":true,
       "sign_in":{"operation_id":"\#(operation.uuidString.lowercased())","user_code":null,
        "verification_url":null,"state":"succeeded","message":"Signed in"}}
      """#)
    #expect(DeveloperGitHubSetupAcknowledgement.begin(currentSuccess,
      operationID: operation, after: 3))

    let contradictoryFailure = try decode(#"""
      {"revision":5,"account":{"state":"signed_in","login":"Owner-1","message":"Ready"},
       "repositories":[],"repository_page":1,"has_more":false,"busy":false,"can_mutate":true,
       "sign_in":{"operation_id":"\#(operation.uuidString.lowercased())","user_code":null,
        "verification_url":null,"state":"failed","message":"Failed"}}
      """#)
    #expect(DeveloperGitHubSetupAcknowledgement.reconcileSignIn(contradictoryFailure,
      operationID: operation.uuidString.lowercased(), after: 4))
  }

  @Test @MainActor
  func modelPaginatesAndCreatesThroughAuthenticatedGitHubRoute() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let requests = root.appendingPathComponent("requests.jsonl")
    let script = #"""
import http.server,json,sys
class Handler(http.server.BaseHTTPRequestHandler):
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  with open(sys.argv[1],'a') as f:f.write(json.dumps({'path':self.path,'auth':self.headers.get('Authorization'),'body':body})+'\n')
  if body['action']=='refresh_account':
   message='GitHub setup failed: '+('full diagnostic detail ' * 30)
   data=json.dumps({'error':message}).encode();self.send_response(400);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data);return
  revision=body['expected_revision']+1
  response={'revision':revision,'account':{'state':'signed_in','login':'Owner-1','message':'Ready'},
   'repositories':[],'repository_page':1,'has_more':False,'busy':False,'can_mutate':True}
  if body['action']=='list_repositories':
   page=body['page'];response['repository_page']=page;response['has_more']=page==1
   response['repositories']=[{'name_with_owner':'Owner-1/repo'+str(page),'url':'https://github.com/Owner-1/repo'+str(page),'visibility':'private','default_branch':'main','can_push':True}]
  elif body['action']=='create_repository':
   response['repository_page']=2
   response['creation']={'operation_id':body['operation_id'],'repository_url':None,'repository_id':None,
    'name_with_owner':body['expected_login']+'/'+body['name'],'visibility':body['visibility'],
    'default_branch':None,'state':'creating','message':'Creating'}
   response['busy']=True
  data=json.dumps(response).encode();self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
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
    let port = handshake.reduce(0) { ($0 << 8) | Int($1) }
    let config = root.appendingPathComponent("runtime.json")
    try JSONSerialization.data(withJSONObject: [
      "endpoint": "http://127.0.0.1:\(port)", "token": "fixture-token",
    ]).write(to: config)
    let model = DeveloperGitHubSetupModel(configurationPath: config.path)
    model.snapshot = try snapshot()
    await model.listRepositories(page: 1)
    await model.listRepositories(page: 2)
    #expect(model.repositories.map(\.nameWithOwner) == ["Owner-1/repo1", "Owner-1/repo2"])
    await model.createRepository(name: "new-repo", visibility: "private")
    #expect(model.snapshot?.creation?.state == "creating")
    #expect(model.snapshot?.creation?.nameWithOwner == "Owner-1/new-repo")
    model.snapshot = try snapshot(revision: 4)
    await model.refreshAccount()
    #expect(model.error?.contains("full diagnostic detail full diagnostic detail") == true)
    #expect((model.error?.count ?? 0) > 300)

    let lines = try String(contentsOf: requests, encoding: .utf8).split(separator: "\n")
    let captured = try lines.map {
      try JSONSerialization.jsonObject(with: Data($0.utf8)) as! [String: Any]
    }
    #expect(captured.count == 4)
    #expect(captured.allSatisfy { $0["path"] as? String == "/github" })
    #expect(captured.allSatisfy { $0["auth"] as? String == "Bearer fixture-token" })
    let createBody = try #require(captured[2]["body"] as? [String: Any])
    #expect(createBody["expected_login"] as? String == "Owner-1")
    #expect(createBody["visibility"] as? String == "private")
  }

  @Test @MainActor
  func busyStateSendsNoMutation() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let config = root.appendingPathComponent("runtime.json")
    try JSONSerialization.data(withJSONObject: [
      "endpoint": "http://127.0.0.1:1", "token": "fixture",
    ]).write(to: config)
    let model = DeveloperGitHubSetupModel(configurationPath: config.path)
    model.snapshot = try snapshot(busy: true)
    await model.refreshAccount()
    #expect(model.error == "GitHub setup is busy or changed. Reload before continuing.")
  }
}
