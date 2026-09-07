import Foundation
import Testing
@testable import AssemblywrightMacApp

@Suite("Developer repair escalation")
struct DeveloperRepairEscalationTests {
  private func runner(_ changes: [String: Any] = [:]) throws -> DeveloperRunnerSnapshot {
    var value: [String: Any] = ["mode": "supervised_developer", "host": "fixture",
      "workspace_root": "C:/fixture", "revision": 12, "auto_run": false,
      "emergency_paused": false, "running": false, "chat_running": false,
      "escalation_running": false, "repair_limit": 3, "repair_active": false,
      "queue": [["id": "feature", "project": "example", "instruction": "Modernize GUI",
        "validation": "python tests.py", "status": "failed", "checkpoint": "repair_3_applied",
        "message": "Entry not found", "changed_files": [], "repair_attempts": 3, "model_target": "mac"]],
      "review_required": true, "review_provider": "openai.codex", "review_model": "gpt-5.6-sol",
      "planning_required": true, "planning_provider": "openai.codex", "planning_model": "gpt-5.6-sol",
      "planning_running": false, "planning_sessions": []]
    value.merge(changes) { _, new in new }
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return try decoder.decode(DeveloperRunnerSnapshot.self, from: JSONSerialization.data(withJSONObject: value))
  }

  private func proposal(_ changes: [String: Any] = [:]) throws -> DeveloperRepairProposal {
    var value: [String: Any] = ["status": "ready", "feature_id": "feature", "proposal_id": "proposal",
      "model_target": "mac", "chat_model_target": "windows", "chat_request_id": "diagnosis-request",
      "diagnosis_sha256": String(repeating: "d", count: 64), "diagnosis": "The test checks a stale widget class.",
      "summary": "Correct the widget test while preserving behavior coverage.",
      "binding": ["feature_id": "feature", "checkpoint": "repair_3_applied", "revision": 12],
      "files": [["path": "tests/test_gui.py", "before": "assert isinstance(entry, tk.Entry)",
        "after": "assert isinstance(entry, ttk.Entry)", "protected": true]], "count": 1]
    value.merge(changes) { _, new in new }
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return try decoder.decode(DeveloperRepairProposal.self, from: JSONSerialization.data(withJSONObject: value))
  }

  @Test
  func exhaustedRepairOffersEscalationButCannotReuseLegacyBudget() throws {
    let state = try runner()
    let feature = try #require(state.nextFeature)
    #expect(!state.canRepair(feature))
    #expect(state.canEscalate(feature))
    let oldRunner = try runner(["escalation_running": NSNull()])
    #expect(!oldRunner.canEscalate(feature))
    for blocked in ["running", "chat_running", "planning_running", "emergency_paused", "escalation_running"] {
      let state = try runner([blocked: true])
      #expect(!state.canEscalate(feature))
    }
  }

  @Test
  func approvalRequiresExactReadyProposalAndCurrentIdleFeature() throws {
    let state = try runner()
    let feature = try #require(state.nextFeature)
    let ready = try proposal()
    #expect(ready.canApprove(feature: feature, runner: state))
    #expect(ready.files?.first?.protected == true)
    #expect(ready.files?.first?.before?.contains("tk.Entry") == true)
    for blocked in ["running", "chat_running", "planning_running", "emergency_paused", "escalation_running"] {
      let state = try runner([blocked: true])
      #expect(!ready.canApprove(feature: feature, runner: state))
    }
    #expect(!ready.canApprove(feature: feature, runner: try runner(["revision": 13])))
    #expect(!ready.canApprove(feature: feature, runner: try runner(["queue": []])))
    for status in ["preparing", "cancelled", "applied", "unavailable"] {
      #expect(!(try proposal(["status": status])).canApprove(feature: feature, runner: state))
    }
    #expect(!(try proposal(["diagnosis": NSNull()])).canApprove(feature: feature, runner: state))
    #expect(!(try proposal(["chat_model_target": "cloud"])).canApprove(feature: feature, runner: state))
    #expect(!(try proposal(["files": []])).canApprove(feature: feature, runner: state))
    #expect(!(try proposal(["feature_id": "other"])).canApprove(feature: feature, runner: state))
    #expect(!(try proposal(["binding": ["feature_id": "feature", "checkpoint": "changed", "revision": 12]]))
      .canApprove(feature: feature, runner: state))
  }

  @Test
  func repairActionsFollowProposalLifecycle() throws {
    let ready = try proposal()
    #expect(ready.showsApproveAction)
    #expect(ready.showsDiscardAction)
    #expect(!ready.showsPrepareAction)
    #expect(!ready.showsStopAction)
    #expect(ready.locksModelSelection)

    let preparing = try proposal(["status": "preparing"])
    #expect(preparing.showsStopAction)
    #expect(!preparing.showsApproveAction)
    #expect(!preparing.showsPrepareAction)
    #expect(preparing.locksModelSelection)

    for status in ["approved", "applying", "applied", "validating", "reviewing", "succeeded"] {
      let current = try proposal(["status": status])
      #expect(!current.showsApproveAction)
      #expect(!current.showsDiscardAction)
      #expect(!current.showsPrepareAction)
      #expect(!current.showsStopAction)
      #expect(!current.locksModelSelection)
    }

    for status in ["cancelled", "unavailable", "failed", "interrupted"] {
      let terminal = try proposal(["status": status])
      #expect(!terminal.showsApproveAction)
      #expect(terminal.showsPrepareAction)
      #expect(!terminal.locksModelSelection)
      #expect(terminal.prepareActionTitle == "Prepare fresh repair")
    }
    #expect((try proposal(["status": "unavailable"])).showsDiscardAction)
  }

  @Test
  func failedRepairExplainsAppliedChangesAndFreshProposal() throws {
    let message = try #require((try proposal(["status": "failed"])).statusMessage)
    #expect(message.contains("approved changes were applied"))
    #expect(message.contains("validation or independent OpenAI/Codex review failed"))
    #expect(message.contains("prepare a fresh repair proposal"))
  }

  @Test
  func interruptedEscalationCannotOfferOrdinaryRepairEvenWithUnusedBudget() {
    for attempts in [0, 1, 2, 3] {
      let feature = DeveloperRunnerFeature(id: "feature", project: "example", instruction: "Modernize GUI",
        validation: "tests", status: "failed", checkpoint: "escalation_1_apply_interrupted", message: "",
        changedFiles: [], repairAttempts: attempts, modelTarget: "mac")
      #expect(feature.requiresEscalationRecovery)
      #expect(!feature.canRepair)
    }
  }

  @Test
  func chatSelectionRequiresExplicitCapabilityWithoutLegacyFallback() throws {
    let catalog: [[String: String]] = [["id": "mac", "name": "Mac", "model": "mac-coder"],
      ["id": "windows", "name": "Windows", "model": "windows-coder"]]
    #expect((try runner()).availableChatModelTargets.isEmpty)
    #expect((try runner(["model_targets": catalog])).availableChatModelTargets.isEmpty)
    #expect((try runner(["chat_model_selection": true])).availableChatModelTargets.isEmpty)
    let current = try runner(["chat_model_selection": true, "model_targets": catalog])
    #expect(current.canSelectChatModel("mac"))
    #expect(current.canSelectChatModel("windows"))
    #expect(!current.canSelectChatModel("cloud"))
  }

  @Test
  func chatAttributionAndDiagnosisBindingSurviveModelSwitch() throws {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let snapshot = try decoder.decode(DeveloperChatSnapshot.self, from: Data("""
      {"project":"example","running":false,"model_target":"mac","messages":[
        {"role":"assistant","content":"Older Windows answer"},
        {"role":"assistant","content":"The test is stale","request_id":"windows-turn","model_target":"windows","model":"windows-coder","content_sha256":"windows-digest"},
        {"role":"assistant","content":"Verify the input behavior","request_id":"mac-turn","model_target":"mac","model":"mac-coder","content_sha256":"mac-digest"}
      ]}
      """.utf8))
    #expect(snapshot.messages.map(\.authorLabel) == ["Windows AI", "Windows AI", "Mac AI"])
    #expect(snapshot.messages[0].requestId == nil)
    #expect(snapshot.messages[1].requestId == "windows-turn")
    #expect(snapshot.messages[2].contentSha256 == "mac-digest")
    #expect(snapshot.modelTarget == "mac")
  }
  @Test(.enabled(if: ProcessInfo.processInfo.environment["ASSEMBLYWRIGHT_DEVELOPER_LIVE_REPAIR_ID"] != nil))
  @MainActor
  func installedWindowsProposalDecodesAndIsReviewableWithoutApplying() async throws {
    let environment = ProcessInfo.processInfo.environment
    let path = try #require(environment["ASSEMBLYWRIGHT_DEVELOPER_LIVE_CONFIG"])
    let featureId = try #require(environment["ASSEMBLYWRIGHT_DEVELOPER_LIVE_REPAIR_ID"])
    let runner = DeveloperRunnerModel(configurationPath: path)
    let repair = DeveloperRepairEscalationModel(configurationPath: path)
    let runnerObserver = Task { await runner.observe() }
    let repairObserver = Task { await repair.observe(featureId: featureId) }
    defer { runnerObserver.cancel(); repairObserver.cancel() }
    for _ in 0..<100 {
      if let state = runner.snapshot, let proposal = repair.proposal,
        let feature = state.queue.first(where: { $0.id == featureId }),
        proposal.canApprove(feature: feature, runner: state) { break }
      try await Task.sleep(for: .milliseconds(100))
    }
    let state = try #require(runner.snapshot)
    let proposal = try #require(repair.proposal)
    let feature = try #require(state.queue.first { $0.id == featureId })
    #expect(state.canSelectChatModel("mac"))
    #expect(state.canSelectChatModel("windows"))
    #expect(proposal.canApprove(feature: feature, runner: state))
    #expect(proposal.modelTarget == "mac")
    #expect(proposal.chatModelTarget == "mac")
    #expect(proposal.files?.contains { $0.protected } == true)
    #expect(proposal.diagnosis?.isEmpty == false)
    #expect(!state.running)
    #expect(feature.repairAttempts == 3)
    // Observation only: this test never sends prepare, approval, or queue control.
  }

}
