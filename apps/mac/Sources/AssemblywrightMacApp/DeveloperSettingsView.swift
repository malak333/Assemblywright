import Foundation
import SwiftUI

struct DeveloperAIModel: Decodable, Identifiable, Equatable {
  let id: String
  let name: String
  let reasoningEfforts: [String]
  let defaultReasoningEffort: String
}

struct DeveloperAISelection: Codable, Equatable {
  var model: String
  var reasoningEffort: String

  static func validModelID(_ model: String) -> Bool {
    model.hasPrefix("gpt-") && model.utf8.count <= 128
      && model.utf8.allSatisfy {
        (97...122).contains($0) || (48...57).contains($0) || [45, 46, 95].contains($0)
      }
  }

  var isWellFormed: Bool {
    Self.validModelID(model)
      && ["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"].contains(reasoningEffort)
  }

  var requestBody: [String: Any] {
    ["model": model, "reasoning_effort": reasoningEffort]
  }

  func isSupported(by models: [DeveloperAIModel]) -> Bool {
    isWellFormed && models.contains { $0.id == model && $0.reasoningEfforts.contains(reasoningEffort) }
  }

  mutating func selectModel(_ id: String, from models: [DeveloperAIModel]) {
    guard let option = models.first(where: { $0.id == id }) else { return }
    model = option.id
    if !option.reasoningEfforts.contains(reasoningEffort) {
      reasoningEffort = option.defaultReasoningEffort
    }
  }

  static func effortLabel(_ effort: String) -> String {
    switch effort {
    case "xhigh": return "Extra high"
    default: return effort.capitalized
    }
  }
}

struct DeveloperAISettings: Decodable, Equatable {
  let revision: UInt64
  var orchestrator: DeveloperAISelection
  var reviewer: DeveloperAISelection

  var requestBody: [String: Any] {
    ["expected_revision": revision, "orchestrator": orchestrator.requestBody,
     "reviewer": reviewer.requestBody]
  }

  func isSupported(by models: [DeveloperAIModel]) -> Bool {
    orchestrator.isSupported(by: models) && reviewer.isSupported(by: models)
  }
}

struct DeveloperSettingsView: View {
  @ObservedObject var runner: DeveloperRunnerModel
  @Environment(\.dismiss) private var dismiss
  @State private var draft: DeveloperAISettings?
  @State private var loading = true
  @State private var error: String?
  @State private var saved = false

  private var models: [DeveloperAIModel] { runner.snapshot?.aiModels ?? [] }
  private var canSave: Bool {
    !loading && !runner.sending && runner.snapshot?.canSaveAISettings == true
      && draft?.isSupported(by: models) == true && draft != runner.snapshot?.aiSettings
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 20) {
      HStack {
        Label("Settings", systemImage: "gearshape").font(.title2.bold())
        Spacer()
        Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
          .disabled(runner.sending)
      }
      Text("Choose the AI for each role").font(.headline)
      Text("Use the models available through your ChatGPT account in Codex. Each model offers its supported reasoning levels.")
        .foregroundStyle(.secondary)
      if loading { ProgressView("Loading settings…") }
      if draft != nil {
        rolePicker("Orchestrator", detail: "Brainstorms requirements and prepares the implementation plan.",
          keyPath: \.orchestrator)
        rolePicker("Reviewer", detail: "Reviews the generated code and test results before a feature succeeds.",
          keyPath: \.reviewer)
        Text("Changes apply to new brainstorming sessions and newly queued features. To update an existing feature, use Change reviewer on its card.")
          .font(.caption).foregroundStyle(.secondary)
        if runner.snapshot?.aiCatalogSource?.contains("bundled") == true {
          Text("Showing the bundled Codex model list. Model access depends on your account.")
            .font(.caption).foregroundStyle(.secondary)
        }
        if runner.snapshot?.canSaveAISettings != true {
          Text("Wait for active work to finish before saving settings.")
            .font(.callout).foregroundStyle(.orange)
        }
        if draft?.revision != runner.snapshot?.aiSettings?.revision {
          Text("Settings changed on Windows. Reload settings before saving.")
            .foregroundStyle(.orange)
        }
      }
      if let error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
      if saved { Label("Settings saved on Windows", systemImage: "checkmark.circle").foregroundStyle(.green) }
      HStack {
        Button("Reload settings") { Task { await reload() } }
          .disabled(loading || runner.sending)
        Spacer()
        Button("Save") {
          guard let draft else { return }
          Task {
            do {
              try await runner.saveAISettings(draft)
              self.draft = runner.snapshot?.aiSettings
              error = nil
              saved = true
            } catch { self.error = error.localizedDescription; saved = false }
          }
        }
        .buttonStyle(.borderedProminent)
        .disabled(!canSave || draft?.revision != runner.snapshot?.aiSettings?.revision)
        .accessibilityIdentifier("developer-settings-save")
      }
    }
    .padding(26)
    .frame(width: 560)
    .fixedSize(horizontal: false, vertical: true)
    .task { await reload() }
  }

  private func reload() async {
    loading = true
    defer { loading = false }
    do {
      try await runner.refreshAISettings()
      guard let settings = runner.snapshot?.aiSettings, !models.isEmpty else {
        throw NSError(domain: "Developer settings", code: 2,
          userInfo: [NSLocalizedDescriptionKey: "Update the Windows developer runner to use AI settings."])
      }
      draft = settings
      error = nil
      saved = false
    } catch { self.error = error.localizedDescription }
  }

  private func rolePicker(_ title: String, detail: String,
    keyPath: WritableKeyPath<DeveloperAISettings, DeveloperAISelection>) -> some View {
    GroupBox {
      VStack(alignment: .leading, spacing: 10) {
        Text(title).font(.headline)
        Text(detail).font(.caption).foregroundStyle(.secondary)
        Picker("Model", selection: Binding(
          get: { draft?[keyPath: keyPath].model ?? "" },
          set: { id in draft?[keyPath: keyPath].selectModel(id, from: models); saved = false }
        )) {
          ForEach(models) { option in Text(option.name).tag(option.id) }
          if let selected = draft?[keyPath: keyPath].model,
            !models.contains(where: { $0.id == selected }) {
            Text("\(selected) (unavailable)").tag(selected)
          }
        }
        .accessibilityIdentifier("developer-settings-\(title.lowercased())-model")
        Picker("Reasoning", selection: Binding(
          get: { draft?[keyPath: keyPath].reasoningEffort ?? "" },
          set: { draft?[keyPath: keyPath].reasoningEffort = $0; saved = false }
        )) {
          ForEach(models.first(where: { $0.id == draft?[keyPath: keyPath].model })?.reasoningEfforts ?? [], id: \.self) {
            Text(DeveloperAISelection.effortLabel($0)).tag($0)
          }
          if let selected = draft?[keyPath: keyPath].reasoningEffort,
            models.first(where: { $0.id == draft?[keyPath: keyPath].model })?.reasoningEfforts.contains(selected) != true {
            Text("\(DeveloperAISelection.effortLabel(selected)) (unavailable)").tag(selected)
          }
        }
        .accessibilityIdentifier("developer-settings-\(title.lowercased())-reasoning")
      }.padding(8)
    }.disabled(loading || runner.sending)
  }
}

struct DeveloperFeatureReviewerView: View {
  @ObservedObject var runner: DeveloperRunnerModel
  let feature: DeveloperRunnerFeature
  @Environment(\.dismiss) private var dismiss
  @State private var observedFeature: DeveloperRunnerFeature?
  @State private var revision: UInt64?
  @State private var draft: DeveloperAISelection?
  @State private var loading = true
  @State private var error: String?

  private var models: [DeveloperAIModel] { runner.snapshot?.aiModels ?? [] }
  private var canSave: Bool {
    guard let observedFeature, let draft, let revision else { return false }
    return !loading && !runner.sending && runner.snapshot?.revision == revision
      && runner.snapshot?.canChangeReviewer(observedFeature) == true
      && draft.isSupported(by: models) && draft != observedFeature.reviewerSelection
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 18) {
      Text("Change reviewer").font(.title2.bold())
      Text(feature.project).font(.headline)
      Text("Choose the model and reasoning level for this feature's next review. Saved files and repair history are kept.")
        .foregroundStyle(.secondary)
      if loading { ProgressView("Loading reviewer…") }
      if let draft {
        Picker("Model", selection: Binding(
          get: { self.draft?.model ?? "" },
          set: { self.draft?.selectModel($0, from: models) }
        )) {
          ForEach(models) { Text($0.name).tag($0.id) }
          if !models.contains(where: { $0.id == draft.model }) {
            Text("\(draft.model) (unavailable)").tag(draft.model)
          }
        }.accessibilityIdentifier("developer-feature-reviewer-model")
        Picker("Reasoning", selection: Binding(
          get: { self.draft?.reasoningEffort ?? "" },
          set: { self.draft?.reasoningEffort = $0 }
        )) {
          ForEach(models.first(where: { $0.id == draft.model })?.reasoningEfforts ?? [], id: \.self) {
            Text(DeveloperAISelection.effortLabel($0)).tag($0)
          }
          if models.first(where: { $0.id == draft.model })?.reasoningEfforts.contains(draft.reasoningEffort) != true {
            Text("\(DeveloperAISelection.effortLabel(draft.reasoningEffort)) (unavailable)").tag(draft.reasoningEffort)
          }
        }.accessibilityIdentifier("developer-feature-reviewer-reasoning")
      }
      Text("Saving does not start the feature. Use Start or Resume when ready; Windows will validate the files and request a fresh review.")
        .font(.callout)
      if runner.snapshot?.aiCatalogSource?.contains("bundled") == true {
        Text("Showing the bundled Codex model list. Model access depends on your account.")
          .font(.caption).foregroundStyle(.secondary)
      }
      if let revision, runner.snapshot?.revision != revision {
        Text("Work changed on Windows. Reload before saving.").foregroundStyle(.orange)
      } else if let observedFeature, runner.snapshot?.canChangeReviewer(observedFeature) != true {
        Text("Stop active work before changing this feature's reviewer.").foregroundStyle(.orange)
      }
      if let error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
      HStack {
        Button("Reload") { Task { await reload() } }.disabled(loading || runner.sending)
        Spacer()
        Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction).disabled(runner.sending)
        Button("Save reviewer") {
          guard let observedFeature, let revision, let draft else { return }
          Task {
            do {
              try await runner.changeReviewer(observedFeature, revision: revision, selection: draft)
              dismiss()
            } catch { self.error = error.localizedDescription }
          }
        }.buttonStyle(.borderedProminent).disabled(!canSave)
          .accessibilityIdentifier("developer-feature-reviewer-save")
      }
    }.padding(26).frame(width: 540).fixedSize(horizontal: false, vertical: true)
      .task { await reload() }
  }

  private func reload() async {
    loading = true
    defer { loading = false }
    do {
      try await runner.refreshAISettings()
      guard let snapshot = runner.snapshot,
        let current = snapshot.queue.first(where: { $0.id == feature.id }),
        let selection = current.reviewerSelection else {
        throw NSError(domain: "Developer reviewer", code: 2,
          userInfo: [NSLocalizedDescriptionKey: "This feature is unavailable. Close this window and refresh the assembly line."])
      }
      observedFeature = current
      revision = snapshot.revision
      draft = selection
      error = nil
    } catch { self.error = error.localizedDescription }
  }
}
