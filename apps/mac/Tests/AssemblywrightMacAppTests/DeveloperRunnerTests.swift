import Foundation
import Testing
@testable import AssemblywrightMacApp

@Suite("Developer runner")
struct DeveloperRunnerTests {
  private func feature(_ id: String, status: String, attempts: Int? = 0) -> DeveloperRunnerFeature {
    DeveloperRunnerFeature(
      id: id, project: "example", instruction: "Add a feature", validation: "python tests.py",
      status: status, checkpoint: "applied", message: "", changedFiles: ["example.py"],
      repairAttempts: attempts, modelTarget: nil)
  }

  private func snapshot(_ queue: [DeveloperRunnerFeature], running: Bool = false,
    emergencyPaused: Bool = false, repairLimit: Int? = 3, repairActive: Bool? = false,
    modelTargets: [DeveloperRunnerModelTarget]? = nil)
    -> DeveloperRunnerSnapshot
  {
    DeveloperRunnerSnapshot(
      mode: "supervised_developer", host: "fixture", workspaceRoot: "/fixture", revision: 1,
      autoRun: true, emergencyPaused: emergencyPaused, running: running, chatRunning: false, queue: queue,
      repairLimit: repairLimit, repairActive: repairActive, modelTargets: modelTargets,
      reviewRequired: true, reviewProvider: "openai.codex", reviewModel: "gpt-5.6-sol",
      planningRequired: true, planningProvider: "openai.codex", planningModel: "gpt-5.6-sol",
      planningRunning: false, planningSessions: [])
  }

  @Test
  func olderOrUnboundReviewRunnersCannotClaimRequiredReview() throws {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    var wire: [String: Any] = ["mode": "supervised_developer", "host": "fixture",
      "workspace_root": "/fixture", "revision": 1, "auto_run": true,
      "emergency_paused": false, "running": false, "queue": []]
    #expect(throws: (any Error).self) {
      try decoder.decode(DeveloperRunnerSnapshot.self, from: JSONSerialization.data(withJSONObject: wire))
    }
    wire["review_required"] = true
    wire["review_provider"] = "openai.codex"
    wire["review_model"] = "gpt-5.6-sol"
    #expect(throws: (any Error).self) {
      try decoder.decode(DeveloperRunnerSnapshot.self, from: JSONSerialization.data(withJSONObject: wire))
    }
    wire["planning_required"] = true
    wire["planning_provider"] = "openai.codex"
    wire["planning_model"] = "gpt-5.6-sol"
    wire["planning_running"] = false
    wire["planning_sessions"] = []
    func decode() throws -> DeveloperRunnerSnapshot {
      try decoder.decode(DeveloperRunnerSnapshot.self, from: JSONSerialization.data(withJSONObject: wire))
    }
    #expect(try decode().hasRequiredReviewer)
    #expect(try decode().hasRequiredPlanner)
    wire["planning_required"] = false
    #expect(try !decode().hasRequiredPlanner)
    wire["planning_required"] = true
    wire["planning_model"] = "local"
    #expect(try !decode().hasRequiredPlanner)
    wire["planning_model"] = "gpt-5.6-sol"
    wire["review_required"] = false
    #expect(try !decode().hasRequiredReviewer)
    wire["review_required"] = true
    wire["review_model"] = "local-model"
    #expect(try !decode().hasRequiredReviewer)
    wire["review_model"] = "gpt-5.6-sol"
    wire["review_provider"] = "local"
    #expect(try !decode().hasRequiredReviewer)
  }

  @Test
  func successRequiresExplicitReviewForPresentation() throws {
    let legacy = feature("old", status: "succeeded")
    #expect(!legacy.hasApprovedReview)
    #expect(legacy.resultLabel == "Tests passed · not reviewed")
    var reviewed = feature("new", status: "succeeded")
    reviewed.reviewStatus = "approved"
    #expect(reviewed.hasApprovedReview)
    #expect(reviewed.resultLabel == "Succeeded")
    #expect(reviewed.reviewLabel == "Codex review: approved")
    for state in ["legacy_unreviewed", "rejected", "unavailable", "unknown", "pending"] {
      reviewed.reviewStatus = state
      #expect(!reviewed.hasApprovedReview)
      #expect(reviewed.resultLabel == "Tests passed · not reviewed")
    }
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let data = Data("""
      {"id":"item","project":"example","instruction":"feature","validation":"tests",\
      "status":"succeeded","checkpoint":"reviewed","message":"","changed_files":[],\
      "review_status":"approved","review_model":"gpt-5.6-sol","review_attempts":1,\
      "review_summary":"Meets the requested behavior."}
      """.utf8)
    let parsed = try decoder.decode(DeveloperRunnerFeature.self, from: data)
    #expect(parsed.hasApprovedReview)
    #expect(parsed.reviewModel == "gpt-5.6-sol")
    #expect(parsed.reviewAttempts == 1)
  }

  @Test
  func modelSelectionRequiresConfiguredTargetAndSupportsLegacyMac() throws {
    #expect(snapshot([]).canSelectModel("mac"))
    #expect(!snapshot([]).canSelectModel("windows"))
    #expect(!snapshot([], modelTargets: []).canSelectModel("mac"))
    let state = snapshot([], modelTargets: [
      DeveloperRunnerModelTarget(id: "mac", name: "Mac", model: "mac-model"),
      DeveloperRunnerModelTarget(id: "windows", name: "Windows", model: "windows-model"),
      DeveloperRunnerModelTarget(id: "cloud", name: "Cloud", model: "unsupported"),
    ])
    #expect(state.canSelectModel("windows"))
    #expect(!state.canSelectModel("cloud"))
    #expect(state.availableModelTargets.map(\.id) == ["mac", "windows"])
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let data = Data("""
      {"id":"item","project":"example","instruction":"feature","validation":"tests",\
      "status":"queued","checkpoint":"not_started","message":"","changed_files":[],\
      "model_target":"windows"}
      """.utf8)
    let selected = try decoder.decode(DeveloperRunnerFeature.self, from: data)
    #expect(selected.modelComputer == "Windows")
    #expect(feature("legacy", status: "queued").modelComputer == "Mac")
  }

  @Test(arguments: [-1, 0, 1, 2, 3, 4])
  func repairRequiresRemainingAttemptsAndIdleFirstFailure(attempts: Int) {
    let failed = feature("failed", status: "failed", attempts: attempts)
    #expect(snapshot([failed]).canRepair(failed) == (0..<3).contains(attempts))
    #expect(!snapshot([failed], running: true).canRepair(failed))
    #expect(!snapshot([failed], emergencyPaused: true).canRepair(failed))
    #expect(!snapshot([feature("earlier", status: "queued"), failed]).canRepair(failed))
    #expect(!snapshot([]).canRepair(failed))
  }

  @Test
  func repairRejectsStaleOrUnavailableAdmission() {
    for status in ["queued", "paused", "running", "succeeded", "removed", "unknown"] {
      #expect(!feature("item", status: status).canRepair)
    }
    #expect(!feature("item", status: "failed", attempts: nil).canRepair)
    for reviewState in ["unavailable", "interrupted"] {
      var unavailable = feature("unavailable", status: "failed")
      unavailable.reviewStatus = reviewState
      #expect(!snapshot([unavailable]).canRepair(unavailable))
    }
    let stale = feature("item", status: "failed")
    #expect(!snapshot([stale], repairLimit: nil).canRepair(stale))
    #expect(!snapshot([stale], repairLimit: 4).canRepair(stale))
    #expect(!snapshot([stale], repairActive: true).canRepair(stale))
    #expect(!snapshot([stale], repairActive: nil).canRepair(stale))
    #expect(!snapshot([feature("item", status: "succeeded")]).canRepair(stale))
    #expect(!snapshot([feature("item", status: "failed", attempts: 3)]).canRepair(stale))
  }

  @Test
  func removedFailureNoLongerBlocksOrAppearsInQueue() {
    let removed = feature("failed-item", status: "removed")
    let queued = feature("next-item", status: "queued")
    let state = snapshot([feature("done", status: "succeeded"), removed, queued])
    #expect(state.nextFeature?.id == queued.id)
    #expect(state.visibleQueue.map(\.id) == ["done", "next-item"])
    #expect(snapshot([removed]).visibleQueue.isEmpty)
    #expect(snapshot([removed]).nextFeature == nil)
    #expect(snapshot([]).nextFeature == nil)
  }

  @Test(arguments: ["queued", "paused", "failed", "running", "succeeded", "removed", "unknown"])
  func removalAvailabilityUsesCurrentRunnerState(status: String) {
    let item = feature("item", status: status)
    #expect(snapshot([item]).canRemove(item) == ["queued", "paused", "failed"].contains(status))
    #expect(!snapshot([item], running: true).canRemove(item))
    #expect(!snapshot([]).canRemove(item))
    #expect(!snapshot([feature("item", status: "succeeded")]).canRemove(item))
  }

  @Test(.enabled(if: ProcessInfo.processInfo.environment["ASSEMBLYWRIGHT_DEVELOPER_LIVE_CONFIG"] != nil))
  @MainActor
  func swiftModelObservesWindowsRunner() async throws {
    let path = try #require(ProcessInfo.processInfo.environment["ASSEMBLYWRIGHT_DEVELOPER_LIVE_CONFIG"])
    let model = DeveloperRunnerModel(configurationPath: path)
    let observation = Task { await model.observe() }
    defer { observation.cancel() }
    for _ in 0..<100 {
      if model.snapshot != nil { break }
      try await Task.sleep(for: .milliseconds(100))
    }
    let snapshot = try #require(model.snapshot)
    #expect(model.error == nil)
    #expect(snapshot.mode == "supervised_developer")
    #expect(!snapshot.host.isEmpty)
    #expect(snapshot.queue.allSatisfy { UUID(uuidString: $0.id) != nil })
    #expect(snapshot.queue.allSatisfy {
      ["queued", "running", "paused", "failed", "succeeded", "removed"].contains($0.status)
    })
  }
}
