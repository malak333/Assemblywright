import Foundation
import Testing
@testable import AssemblywrightMacApp

@Suite("Developer GitHub publication")
struct DeveloperGitHubTests {
  private func decodeSnapshot(publicationSupported: Bool? = true,
    unresolved: Bool? = false, featureStatus: String = "queued",
    featureCheckpoint: String = "not_started", revision: UInt64 = 9,
    connectionRepository: String = "https://github.com/owner/demo",
    setupBusy: Bool = false, setupUnresolved: Bool = false,
    publicationRunning: Bool = false,
    planningModel: String = "gpt-5.6-sol", reviewModel: String = "gpt-5.6-sol",
    planningEffort: String? = nil, reviewEffort: String? = nil,
    aiSettings: [String: Any]? = nil) throws -> DeveloperRunnerSnapshot {
    var feature: [String: Any] = [
      "id": "feature-1", "project": "demo", "instruction": "Add publishing",
      "validation": "swift test", "status": featureStatus, "checkpoint": featureCheckpoint,
      "message": "", "changed_files": [], "repair_attempts": 0,
    ]
    if featureStatus == "failed" {
      feature["review_status"] = "rejected"
      if featureCheckpoint == "publication_attention" {
        feature["publication_status"] = "attention"
        feature["can_reconcile_publication"] = true
      }
    }
    var wire: [String: Any] = [
      "mode": "supervised_developer", "host": "fixture", "workspace_root": "/fixture",
      "revision": revision, "auto_run": true, "emergency_paused": false, "running": false,
      "chat_running": false, "queue": [feature], "repair_limit": 3, "repair_active": false,
      "review_required": true, "review_provider": "openai.codex", "review_model": reviewModel,
      "planning_required": true, "planning_provider": "openai.codex",
      "planning_model": planningModel, "planning_running": false, "planning_sessions": [],
    ]
    if let planningEffort { wire["planning_reasoning_effort"] = planningEffort }
    if let reviewEffort { wire["review_reasoning_effort"] = reviewEffort }
    if let aiSettings { wire["ai_settings"] = aiSettings }
    if let publicationSupported { wire["github_publication_supported"] = publicationSupported }
    if let unresolved { wire["github_publication_unresolved"] = unresolved }
    wire["github_publication_running"] = publicationRunning
    wire["github_setup_busy"] = setupBusy
    wire["github_setup_unresolved"] = setupUnresolved
    wire["can_manage_github_connections"] = true
    wire["github_connections"] = [["project": "demo",
      "repository_url": connectionRepository, "base_branch": "main",
      "automatic_merge": true]]
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return try decoder.decode(DeveloperRunnerSnapshot.self,
      from: JSONSerialization.data(withJSONObject: wire))
  }

  @Test
  func decodesPublicationContractAndLegacyFixtures() throws {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let feature = try decoder.decode(DeveloperRunnerFeature.self, from: Data(#"""
      {"id":"feature-1","project":"demo","instruction":"feature","validation":"tests",
       "status":"running","checkpoint":"publication_wait_required_checks","message":"",
       "changed_files":[],"publication_status":"running",
       "publication_stage":"wait_required_checks","publication_message":"Checks are pending.",
       "publication_repository_url":"https://github.com/owner/demo",
       "publication_base_branch":"main","publication_branch":"assemblywright/feature-1",
       "publication_commit_sha":"1111111111111111111111111111111111111111",
       "publication_pr_url":"https://github.com/owner/demo/pull/42",
       "can_reconcile_publication":false}
      """#.utf8))
    #expect(feature.publicationStatus == "running")
    #expect(feature.publicationStage == "wait_required_checks")
    #expect(DeveloperGitHubPresentation.statusLabel(feature) == "Waiting for required GitHub checks")
    #expect(DeveloperGitHubURL.pullRequest(feature.publicationPrUrl,
      repository: feature.publicationRepositoryUrl)?.absoluteString ==
      "https://github.com/owner/demo/pull/42")

    let legacy = try decoder.decode(DeveloperRunnerFeature.self, from: Data(#"""
      {"id":"old","project":"demo","instruction":"feature","validation":"tests",
       "status":"succeeded","checkpoint":"review_approved","message":"","changed_files":[]}
      """#.utf8))
    #expect(legacy.publicationStatus == nil)
    #expect(DeveloperGitHubPresentation.statusLabel(legacy) ==
      "Earlier local result · GitHub publication not recorded")

    let legacySnapshot = try decodeSnapshot(publicationSupported: nil, unresolved: nil)
    #expect(legacySnapshot.githubPublicationSupported == nil)
    #expect(!legacySnapshot.canStartFeature)
  }

  @Test
  func selectedModelBindingsAcceptNondefaultAndRejectMetadataDrift() throws {
    let selected: [String: Any] = [
      "revision": 9,
      "orchestrator": ["model": "gpt-5.3-codex-spark", "reasoning_effort": "high"],
      "reviewer": ["model": "gpt-6-astra", "reasoning_effort": "xhigh"],
    ]
    let valid = try decodeSnapshot(planningModel: "gpt-5.3-codex-spark",
      reviewModel: "gpt-6-astra", planningEffort: "high", reviewEffort: "xhigh",
      aiSettings: selected)
    #expect(valid.hasRequiredPlanner)
    #expect(valid.hasRequiredReviewer)

    let driftedPlanner = try decodeSnapshot(planningModel: "gpt-5.6-sol",
      reviewModel: "gpt-6-astra", planningEffort: "high", reviewEffort: "xhigh",
      aiSettings: selected)
    #expect(!driftedPlanner.hasRequiredPlanner)
    #expect(driftedPlanner.hasRequiredReviewer)

    let driftedEffort = try decodeSnapshot(planningModel: "gpt-5.3-codex-spark",
      reviewModel: "gpt-6-astra", planningEffort: "medium", reviewEffort: "xhigh",
      aiSettings: selected)
    #expect(!driftedEffort.hasRequiredPlanner)
    #expect(driftedEffort.hasRequiredReviewer)

    let invalidSelection: [String: Any] = [
      "revision": 9,
      "orchestrator": ["model": "gpt-INVALID", "reasoning_effort": "high"],
      "reviewer": ["model": "gpt-6-astra", "reasoning_effort": "xhigh"],
    ]
    let malformed = try decodeSnapshot(planningModel: "gpt-INVALID",
      reviewModel: "gpt-6-astra", planningEffort: "high", reviewEffort: "xhigh",
      aiSettings: invalidSelection)
    #expect(!malformed.hasRequiredPlanner)
    #expect(malformed.hasRequiredReviewer)
  }

  @Test
  func repositoryAndPullRequestLinksRejectSpoofingAndDrift() {
    #expect(DeveloperGitHubURL.repository("https://github.com/owner/repo") != nil)
    #expect(DeveloperGitHubURL.repository("https://github.com/owner/repo.git")?.path == "/owner/repo")
    for invalid in [
      "http://github.com/owner/repo", "https://github.com.evil.test/owner/repo",
      "https://github.com@evil.test/owner/repo", "https://github.com/owner/repo/issues",
      "https://github.com/owner%2Frepo", "https://github.com/owner/repo?tab=readme",
      "https://github.com/owner//repo", "https://github.com/owner/.git",
      " https://github.com/owner/repo",
    ] {
      #expect(DeveloperGitHubURL.repository(invalid) == nil)
    }
    let repository = "https://github.com/owner/repo"
    #expect(DeveloperGitHubURL.pullRequest("https://github.com/owner/repo/pull/17",
      repository: repository) != nil)
    for invalid in [
      "https://github.com/other/repo/pull/17", "https://github.com/owner/other/pull/17",
      "https://github.com/owner/repo/issues/17", "https://github.com/owner/repo/pull/main",
      "https://github.com/owner/repo//pull/17",
      "https://github.com.evil.test/owner/repo/pull/17",
      "https://github.com/owner/repo/pull/17?diff=split",
    ] {
      #expect(DeveloperGitHubURL.pullRequest(invalid, repository: repository) == nil)
    }
  }

  @Test
  func succeededLabelRequiresCompleteBoundMergeEvidence() throws {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let data = Data(#"""
      {"id":"feature-1","project":"demo","instruction":"feature","validation":"tests",
       "status":"succeeded","checkpoint":"publication_merged","message":"","changed_files":[],
       "review_status":"approved","publication_status":"succeeded","publication_stage":"complete",
       "publication_repository_url":"https://github.com/owner/demo",
       "publication_commit_sha":"1111111111111111111111111111111111111111",
       "publication_pr_url":"https://github.com/owner/demo/pull/42",
       "publication_merged_sha":"2222222222222222222222222222222222222222"}
      """#.utf8)
    let verified = try decoder.decode(DeveloperRunnerFeature.self, from: data)
    #expect(DeveloperGitHubPresentation.hasVerifiedMergeEvidence(verified))
    #expect(DeveloperGitHubPresentation.statusLabel(verified) == "Merged on GitHub")
    var malformed = verified
    malformed.publicationPrUrl = "https://github.com.evil.test/owner/demo/pull/42"
    #expect(!DeveloperGitHubPresentation.hasVerifiedMergeEvidence(malformed))
    #expect(DeveloperGitHubPresentation.statusLabel(malformed) ==
      "GitHub publication result could not be verified")
    malformed = verified
    malformed.publicationMergedSha = nil
    #expect(!DeveloperGitHubPresentation.hasVerifiedMergeEvidence(malformed))
  }

  @Test
  func unresolvedPublicationBlocksFeatureActions() throws {
    let ready = try decodeSnapshot()
    let queued = try #require(ready.nextFeature)
    #expect(ready.canStartFeature)
    #expect(ready.canRemove(queued))

    let blocked = try decodeSnapshot(unresolved: true)
    let blockedQueued = try #require(blocked.nextFeature)
    #expect(!blocked.canStartFeature)
    #expect(!blocked.canRemove(blockedQueued))

    for setupGate in [try decodeSnapshot(setupBusy: true),
      try decodeSnapshot(setupUnresolved: true)] {
      let gated = try #require(setupGate.nextFeature)
      #expect(!setupGate.canStartFeature)
      #expect(!setupGate.canRemove(gated))
    }

    let failed = try decodeSnapshot(unresolved: false, featureStatus: "failed",
      featureCheckpoint: "validation_failed")
    let repairable = try #require(failed.nextFeature)
    #expect(failed.canRepair(repairable))
    let blockedFailure = try decodeSnapshot(unresolved: true, featureStatus: "failed",
      featureCheckpoint: "publication_attention")
    #expect(!blockedFailure.canRepair(try #require(blockedFailure.nextFeature)))
  }

  @Test
  func publicationReconciliationKeepsStopAvailableWhileOtherwiseIdle() throws {
    let ordinaryIdle = try decodeSnapshot()
    #expect(!ordinaryIdle.canStop)

    let reconciling = try decodeSnapshot(unresolved: true,
      featureStatus: "running", featureCheckpoint: "publication_reconciling",
      publicationRunning: true)
    #expect(reconciling.running == false)
    #expect(reconciling.planningRunning == false)
    #expect(reconciling.escalationRunning != true)
    #expect(reconciling.githubPublicationRunning == true)
    #expect(reconciling.canStop)
    var settingUp = ordinaryIdle
    settingUp.githubSetupBusy = true
    #expect(settingUp.canStop)
  }

  @Test
  func connectionAndReconciliationBodiesBindRenderedState() {
    let save = DeveloperGitHubRequest.saveConnection(project: "demo",
      repositoryURL: "https://github.com/owner/demo", baseBranch: "main", expectedRevision: 9)
    #expect(save["action"] as? String == "save_connection")
    #expect(save["project"] as? String == "demo")
    #expect(save["repository_url"] as? String == "https://github.com/owner/demo")
    #expect(save["base_branch"] as? String == "main")
    #expect(save["expected_revision"] as? UInt64 == 9)
    let disconnect = DeveloperGitHubRequest.disconnect(project: "demo", expectedRevision: 10)
    #expect(disconnect["action"] as? String == "disconnect")
    #expect(disconnect["expected_revision"] as? UInt64 == 10)
    let reconcile = DeveloperGitHubRequest.reconcile(featureID: "feature-1",
      expectedRevision: 11, expectedCheckpoint: "publication_attention")
    #expect(reconcile["action"] as? String == "reconcile")
    #expect(reconcile["feature_id"] as? String == "feature-1")
    #expect(reconcile["expected_checkpoint"] as? String == "publication_attention")
  }

  @Test
  func mutationAcknowledgementsRejectStaleOrUnchangedState() throws {
    let unchanged = try decodeSnapshot()
    #expect(!DeveloperGitHubAcknowledgement.saved(unchanged, expectedRevision: 9,
      project: "demo", repositoryURL: "https://github.com/owner/demo", baseBranch: "main"))
    let canonical = try decodeSnapshot(revision: 10,
      connectionRepository: "https://github.com/owner/demo.git")
    #expect(DeveloperGitHubAcknowledgement.saved(canonical, expectedRevision: 9,
      project: "demo", repositoryURL: "https://github.com/owner/demo", baseBranch: "main"))
    #expect(!DeveloperGitHubAcknowledgement.disconnected(unchanged, expectedRevision: 9,
      project: "demo"))
    let attention = try decodeSnapshot(unresolved: true, featureStatus: "failed",
      featureCheckpoint: "publication_attention")
    #expect(!DeveloperGitHubAcknowledgement.reconciled(attention, expectedRevision: 9,
      featureID: "feature-1"))
  }

  @Test @MainActor
  func modelUsesOnlyAuthenticatedPublicationRoute() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let requests = root.appendingPathComponent("requests.jsonl")
    let script = #"""
import http.server,json,sys
snapshot={'mode':'supervised_developer','host':'fixture','workspace_root':'/fixture','revision':10,
 'auto_run':True,'emergency_paused':False,'running':False,'chat_running':False,'queue':[],
 'repair_limit':3,'repair_active':False,'review_required':True,'review_provider':'openai.codex',
 'review_model':'gpt-5.6-sol','planning_required':True,'planning_provider':'openai.codex',
 'planning_model':'gpt-5.6-sol','planning_running':False,'planning_sessions':[],
 'github_publication_supported':True,'github_publication_running':False,
 'github_publication_unresolved':False,'can_manage_github_connections':True,'github_connections':[]}
class Handler(http.server.BaseHTTPRequestHandler):
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  with open(sys.argv[1],'a') as f:f.write(json.dumps({'path':self.path,'authorization':self.headers.get('Authorization'),'body':body})+'\n')
  response=dict(snapshot)
  if body['action']=='save_connection':
   response['github_connections']=[{'project':body['project'],'repository_url':body['repository_url'],'base_branch':body['base_branch'],'automatic_merge':True}]
  elif body['action']=='reconcile':
   response['running']=True;response['github_publication_running']=True;response['github_publication_unresolved']=True
   response['queue']=[{'id':body['feature_id'],'project':'demo','instruction':'feature','validation':'tests','status':'running','checkpoint':'publication_reconciling','message':'','changed_files':[],'publication_status':'pending','publication_stage':'wait_required_checks','can_reconcile_publication':False}]
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
    let model = DeveloperRunnerModel(configurationPath: config.path)
    model.snapshot = try decodeSnapshot()
    try await model.saveGitHubConnection(project: "demo",
      repositoryURL: "https://github.com/owner/demo", baseBranch: "main", expectedRevision: 9)
    model.snapshot = try decodeSnapshot()
    try await model.disconnectGitHub(project: "demo", expectedRevision: 9)
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let feature = try decoder.decode(DeveloperRunnerFeature.self, from: Data(#"""
      {"id":"feature-1","project":"demo","instruction":"feature","validation":"tests",
       "status":"failed","checkpoint":"publication_attention","message":"","changed_files":[],
       "publication_status":"attention","can_reconcile_publication":true}
      """#.utf8))
    model.snapshot = try decodeSnapshot(unresolved: true, featureStatus: "failed",
      featureCheckpoint: "publication_attention")
    try await model.reconcilePublication(feature, expectedRevision: 9)

    let lines = try String(contentsOf: requests, encoding: .utf8).split(separator: "\n")
    let captured = try lines.map {
      try JSONSerialization.jsonObject(with: Data($0.utf8)) as! [String: Any]
    }
    #expect(captured.count == 3)
    #expect(captured.allSatisfy { $0["path"] as? String == "/publication" })
    #expect(captured.allSatisfy { $0["authorization"] as? String == "Bearer fixture-token" })
    let actions = try captured.map { try #require($0["body"] as? [String: Any])["action"] as? String }
    #expect(actions == ["save_connection", "disconnect", "reconcile"])
  }
}
