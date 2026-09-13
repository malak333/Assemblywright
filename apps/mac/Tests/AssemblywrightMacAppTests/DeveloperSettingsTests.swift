import Foundation
import Testing
@testable import AssemblywrightMacApp

@Suite("Developer AI settings")
struct DeveloperSettingsTests {
  private let models = [
    DeveloperAIModel(id: "gpt-6-astra", name: "GPT-6-Astra",
      reasoningEfforts: ["low", "medium", "high", "xhigh", "max", "ultra"], defaultReasoningEffort: "medium"),
    DeveloperAIModel(id: "gpt-5.3-codex-spark", name: "GPT-5.3-Codex-Spark",
      reasoningEfforts: ["low", "medium", "high", "xhigh"], defaultReasoningEffort: "high"),
  ]

  @Test func modelChangePreservesSupportedEffortAndUsesDefaultForUnsupportedEffort() {
    var selection = DeveloperAISelection(model: "gpt-6-astra", reasoningEffort: "ultra")
    selection.selectModel("gpt-5.3-codex-spark", from: models)
    #expect(selection.model == "gpt-5.3-codex-spark")
    #expect(selection.reasoningEffort == "high")
    selection.reasoningEffort = "xhigh"
    selection.selectModel("gpt-6-astra", from: models)
    #expect(selection.reasoningEffort == "xhigh")
    selection.selectModel("unknown", from: models)
    #expect(selection.model == "gpt-6-astra")
  }

  @Test func selectionRequiresAdvertisedModelAndEffort() {
    #expect(DeveloperAISelection(model: "gpt-6-astra", reasoningEffort: "ultra").isSupported(by: models))
    for invalid in [
      DeveloperAISelection(model: "gpt-5.3-codex-spark", reasoningEffort: "ultra"),
      DeveloperAISelection(model: "gpt-future", reasoningEffort: "high"),
      DeveloperAISelection(model: "gpt-6-astra", reasoningEffort: "unknown"),
      DeveloperAISelection(model: "gpt-6-astra\n--config", reasoningEffort: "high"),
      DeveloperAISelection(model: "local", reasoningEffort: "high"),
    ] { #expect(!invalid.isSupported(by: models)) }
    #expect(!DeveloperAISelection(model: "gpt-6-astra", reasoningEffort: "high").isSupported(by: []))
  }

  @Test func saveBindsBothRolesAndObservedSettingsRevision() throws {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let settings = try decoder.decode(DeveloperAISettings.self, from: Data("""
      {"revision":7,"orchestrator":{"model":"gpt-6-astra","reasoning_effort":"ultra"},
      "reviewer":{"model":"gpt-5.3-codex-spark","reasoning_effort":"high"}}
      """.utf8))
    #expect(settings.isSupported(by: models))
    let encoded = try JSONSerialization.data(withJSONObject: settings.requestBody)
    let wire = try #require(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
    #expect(Set(wire.keys) == ["expected_revision", "orchestrator", "reviewer"])
    #expect((wire["expected_revision"] as? NSNumber)?.uint64Value == 7)
    #expect((wire["orchestrator"] as? [String: String]) ==
      ["model": "gpt-6-astra", "reasoning_effort": "ultra"])
    #expect((wire["reviewer"] as? [String: String]) ==
      ["model": "gpt-5.3-codex-spark", "reasoning_effort": "high"])
  }

  @Test func configuredRunnerAcceptsNewModelsAndRejectsBindingDriftOrBusySave() throws {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    var wire: [String: Any] = [
      "mode": "supervised_developer", "host": "fixture", "workspace_root": "/fixture", "revision": 10,
      "auto_run": true, "emergency_paused": false, "running": false, "chat_running": false,
      "escalation_running": false, "queue": [], "review_required": true,
      "review_provider": "openai.codex", "review_model": "gpt-5.3-codex-spark",
      "review_reasoning_effort": "high", "planning_reasoning_effort": "ultra",
      "planning_required": true, "planning_provider": "openai.codex", "planning_model": "gpt-6-astra",
      "planning_running": false, "planning_sessions": [],
      "ai_settings": ["revision": 7,
        "orchestrator": ["model": "gpt-6-astra", "reasoning_effort": "ultra"],
        "reviewer": ["model": "gpt-5.3-codex-spark", "reasoning_effort": "high"]],
      "ai_models": [["id": "gpt-6-astra", "name": "GPT-6-Astra",
        "reasoning_efforts": ["high", "ultra"], "default_reasoning_effort": "high"]],
    ]
    func decode() throws -> DeveloperRunnerSnapshot {
      try decoder.decode(DeveloperRunnerSnapshot.self, from: JSONSerialization.data(withJSONObject: wire))
    }
    #expect(try decode().hasRequiredPlanner)
    #expect(try decode().hasRequiredReviewer)
    #expect(try decode().canSaveAISettings)
    wire["planning_reasoning_effort"] = "high"
    #expect(try !decode().hasRequiredPlanner)
    wire["planning_reasoning_effort"] = "ultra"
    wire["review_reasoning_effort"] = "ultra"
    #expect(try !decode().hasRequiredReviewer)
    wire.removeValue(forKey: "review_reasoning_effort")
    #expect(try !decode().hasRequiredReviewer)
    wire["review_reasoning_effort"] = "high"
    for flag in ["running", "chat_running", "planning_running", "escalation_running"] {
      wire[flag] = true
      #expect(try !decode().canSaveAISettings)
      wire[flag] = false
    }
    wire["review_model"] = "gpt-6-astra"
    #expect(try !decode().hasRequiredReviewer)
    wire["planning_provider"] = "local"
    #expect(try !decode().hasRequiredPlanner)
  }

  @Test func existingFeatureReviewerUsesExactObservedBindingAndIdleCapability() throws {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    var feature: [String: Any] = [
      "id": "existing", "project": "inches-feet-demo", "instruction": "Convert inches and feet",
      "validation": "python tests.py", "status": "failed", "checkpoint": "review_3_unavailable",
      "message": "Review unavailable", "changed_files": ["app.py"], "repair_attempts": 3,
      "review_status": "unavailable", "review_model": "gpt-5.6-sol", "review_reasoning_effort": "high",
      "can_change_reviewer": true,
    ]
    func decodeFeature() throws -> DeveloperRunnerFeature {
      try decoder.decode(DeveloperRunnerFeature.self, from: JSONSerialization.data(withJSONObject: feature))
    }
    let original = try decodeFeature()
    let requested = DeveloperAISelection(model: "gpt-5.3-codex-spark", reasoningEffort: "high")
    let body = original.reviewerChangeBody(revision: 41, selection: requested)
    #expect(Set(body.keys) == ["id", "expected_revision", "expected_checkpoint", "expected_model",
      "expected_reasoning_effort", "reviewer"])
    #expect(body["id"] as? String == "existing")
    #expect(body["expected_revision"] as? UInt64 == 41)
    #expect(body["expected_checkpoint"] as? String == "review_3_unavailable")
    #expect(body["expected_model"] as? String == "gpt-5.6-sol")
    #expect(body["expected_reasoning_effort"] as? String == "high")
    #expect(body["reviewer"] as? [String: String] == requested.requestBody as? [String: String])
    var wire: [String: Any] = [
      "mode": "supervised_developer", "host": "fixture", "workspace_root": "/fixture", "revision": 41,
      "auto_run": true, "emergency_paused": false, "running": false, "chat_running": false,
      "escalation_running": false, "review_required": true, "review_provider": "openai.codex",
      "review_model": "gpt-5.3-codex-spark", "review_reasoning_effort": "high",
      "planning_required": true, "planning_provider": "openai.codex", "planning_model": "gpt-6-astra",
      "planning_running": false, "planning_sessions": [], "feature_reviewer_selection": true,
      "ai_settings": ["revision": 7,
        "orchestrator": ["model": "gpt-6-astra", "reasoning_effort": "high"],
        "reviewer": ["model": "gpt-5.3-codex-spark", "reasoning_effort": "high"]],
      "ai_models": [["id": "gpt-5.3-codex-spark", "name": "Spark",
        "reasoning_efforts": ["high"], "default_reasoning_effort": "high"]],
    ]
    func decode() throws -> DeveloperRunnerSnapshot {
      wire["queue"] = [feature]
      return try decoder.decode(DeveloperRunnerSnapshot.self, from: JSONSerialization.data(withJSONObject: wire))
    }
    #expect(try decode().canChangeReviewer(original))
    for flag in ["running", "chat_running", "planning_running", "escalation_running", "emergency_paused"] {
      wire[flag] = true
      #expect(try !decode().canChangeReviewer(original))
      wire[flag] = false
    }
    for flag in ["github_publication_running", "github_publication_unresolved", "github_setup_busy",
      "github_setup_unresolved"] {
      wire[flag] = true
      #expect(try !decode().canChangeReviewer(original))
      wire[flag] = false
    }
    wire.removeValue(forKey: "feature_reviewer_selection")
    #expect(try !decode().canChangeReviewer(original))
    wire["feature_reviewer_selection"] = true
    feature["can_change_reviewer"] = false
    #expect(try !decode().canChangeReviewer(original))
    feature["can_change_reviewer"] = true
    feature["checkpoint"] = "review_4_unavailable"
    #expect(try !decode().canChangeReviewer(original))
    feature["checkpoint"] = "review_3_unavailable"
    feature["review_model"] = "gpt-6-astra"
    #expect(try !decode().canChangeReviewer(original))
    feature["review_model"] = "gpt-5.6-sol"
    for status in ["succeeded", "removed", "running", "unknown"] {
      feature["status"] = status
      #expect(try !decode().canChangeReviewer(decodeFeature()))
    }
    feature["status"] = "failed"
    feature["checkpoint"] = "escalation_5_apply_interrupted"
    #expect(try !decode().canChangeReviewer(decodeFeature()))
    for checkpoint in ["staged_tool_candidate_quarantined", "tool_effects_quarantined",
      "tool_workspace_changed_requires_proposal"] {
      feature["checkpoint"] = checkpoint
      #expect(try !decode().canChangeReviewer(decodeFeature()))
    }
  }

  @Test(.enabled(if: ProcessInfo.processInfo.environment["ASSEMBLYWRIGHT_SETTINGS_LIVE_CHECK"] == "1"))
  @MainActor
  func observesInstalledSettingsWithoutChangingSelections() async throws {
    let config = try #require(ProcessInfo.processInfo.environment["ASSEMBLYWRIGHT_DEVELOPER_LIVE_CONFIG"])
    let model = DeveloperRunnerModel(configurationPath: config)
    try await model.refreshAISettings()
    let snapshot = try #require(model.snapshot)
    let settings = try #require(snapshot.aiSettings)
    #expect(settings.orchestrator.isWellFormed)
    #expect(settings.reviewer.isWellFormed)
    #expect(snapshot.aiModels?.isEmpty == false)
    #expect(snapshot.hasRequiredPlanner)
    #expect(snapshot.hasRequiredReviewer)
  }
}
