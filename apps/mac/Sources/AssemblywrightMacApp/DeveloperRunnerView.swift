import Foundation
import SwiftUI

struct DeveloperRunnerConfiguration: Decodable {
  let endpoint: String
  let token: String
}
struct DeveloperRunnerModelTarget: Decodable, Identifiable {
  let id: String
  let name: String
  let model: String
}
struct DeveloperRunnerFeature: Decodable, Identifiable {
  let id: String
  let project: String
  let instruction: String
  let validation: String
  let status: String
  let checkpoint: String
  let message: String
  let changedFiles: [String]
  let repairAttempts: Int?
  let modelTarget: String?
  var reviewStatus: String? = nil
  var reviewModel: String? = nil
  var reviewSummary: String? = nil
  var reviewAttempts: Int? = nil
  var planningStatus: String? = nil
  var escalationCount: Int? = nil
  var escalationStatus: String? = nil
  var publicationStatus: String? = nil
  var publicationStage: String? = nil
  var publicationMessage: String? = nil
  var publicationRepositoryUrl: String? = nil
  var publicationBaseBranch: String? = nil
  var publicationBranch: String? = nil
  var publicationCommitSha: String? = nil
  var publicationPrUrl: String? = nil
  var publicationMergedSha: String? = nil
  var canReconcilePublication: Bool? = nil

  var hasApprovedReview: Bool { reviewStatus == "approved" }
  var resultLabel: String {
    if status == "succeeded" && !hasApprovedReview { return "Tests passed · not reviewed" }
    return status.replacingOccurrences(of: "_", with: " ").capitalized
  }
  var reviewLabel: String {
    switch reviewStatus {
    case "approved": return "Codex review: approved"
    case "rejected": return "Codex review: changes requested"
    case "reviewing", "in_progress": return "Codex review: in progress"
    case "legacy_unreviewed": return "Codex review: not run on this older result"
    case nil, "not_started", "pending": return "Codex review: pending"
    default: return "Codex review: approval unavailable"
    }
  }

  var modelComputer: String {
    switch modelTarget ?? "mac" {
    case "mac": return "Mac"
    case "windows": return "Windows"
    default: return "Unavailable model computer"
    }
  }

  var startBinding: [String: Any] {
    ["expected_feature_id": id, "expected_model_target": modelTarget ?? "mac",
     "expected_status": status, "expected_checkpoint": checkpoint]
  }

  var requiresEscalationRecovery: Bool {
    checkpoint.hasPrefix("escalation_") && checkpoint.hasSuffix("_apply_interrupted")
  }

  var canRemove: Bool { ["queued", "paused", "failed"].contains(status) }
  var isFinished: Bool { ["succeeded", "removed"].contains(status) }
  var canRepair: Bool {
    guard status == "failed", !requiresEscalationRecovery, !["unavailable", "interrupted"].contains(reviewStatus ?? ""), let attempts = repairAttempts
    else { return false }
    return attempts >= 0 && attempts < 3
  }
}
struct DeveloperRunnerSnapshot: Decodable {
  let mode: String
  let host: String
  let workspaceRoot: String
  let revision: UInt64
  let autoRun: Bool
  let emergencyPaused: Bool
  let running: Bool
  let chatRunning: Bool?
  let queue: [DeveloperRunnerFeature]
  let repairLimit: Int?
  let repairActive: Bool?
  let modelTargets: [DeveloperRunnerModelTarget]?
  let reviewRequired: Bool
  let reviewProvider: String
  let reviewModel: String
  let planningRequired: Bool
  let planningProvider: String
  let planningModel: String
  let planningRunning: Bool
  let planningSessions: [DeveloperPlanningSummary]
  var escalationRunning: Bool? = nil
  var chatModelSelection: Bool? = nil
  var chatHistory: Bool? = nil
  var aiSettings: DeveloperAISettings? = nil
  var planningReasoningEffort: String? = nil
  var reviewReasoningEffort: String? = nil
  var githubPublicationSupported: Bool? = nil
  var githubPublicationRunning: Bool? = nil
  var githubPublicationUnresolved: Bool? = nil
  var canManageGithubConnections: Bool? = nil
  var githubConnections: [DeveloperGitHubConnection]? = nil
  var githubSetupBusy: Bool? = nil
  var githubSetupUnresolved: Bool? = nil

  var hasRequiredPlanner: Bool {
    planningRequired && planningProvider == "openai.codex"
      && validRoleBinding(model: planningModel, effort: planningReasoningEffort,
        selection: aiSettings?.orchestrator)
  }

  var hasRequiredReviewer: Bool {
    reviewRequired && reviewProvider == "openai.codex"
      && validRoleBinding(model: reviewModel, effort: reviewReasoningEffort,
        selection: aiSettings?.reviewer)
  }

  private func validRoleBinding(model: String, effort: String?,
    selection: DeveloperAISelection?) -> Bool {
    guard let selection else {
      return aiSettings == nil && model == "gpt-5.6-sol" && (effort == nil || effort == "high")
    }
    return selection.model == model && selection.reasoningEffort == effort
      && selection.isWellFormed
  }

  var availableModelTargets: [DeveloperRunnerModelTarget] {
    guard let modelTargets else {
      return [DeveloperRunnerModelTarget(id: "mac", name: "Mac", model: "Local model")]
    }
    return modelTargets.filter { ["mac", "windows"].contains($0.id) && !$0.model.isEmpty }
  }

  var availableChatModelTargets: [DeveloperRunnerModelTarget] {
    guard chatModelSelection == true, modelTargets != nil else { return [] }
    return availableModelTargets
  }

  func canSelectChatModel(_ id: String) -> Bool {
    availableChatModelTargets.contains { $0.id == id }
  }

  func canSelectModel(_ id: String) -> Bool {
    availableModelTargets.contains { $0.id == id }
  }

  var canChangeChatAccess: Bool {
    !running && !planningRunning && chatRunning != true && escalationRunning != true
      && githubPublicationRunning != true && githubPublicationUnresolved != true
      && githubSetupBusy != true && githubSetupUnresolved != true && !emergencyPaused
  }

  var visibleQueue: [DeveloperRunnerFeature] { queue.filter { $0.status != "removed" } }
  var nextFeature: DeveloperRunnerFeature? { queue.first { !$0.isFinished } }

  func canRemove(_ feature: DeveloperRunnerFeature) -> Bool {
    !running && escalationRunning != true && githubPublicationUnresolved != true
      && githubSetupBusy != true && githubSetupUnresolved != true
      && queue.contains { $0.id == feature.id && $0.canRemove }
  }

  func canEscalate(_ feature: DeveloperRunnerFeature) -> Bool {
    !running && !planningRunning && !emergencyPaused && chatRunning != true && escalationRunning == false
      && githubPublicationUnresolved != true
      && githubSetupBusy != true && githubSetupUnresolved != true
      && nextFeature?.id == feature.id && nextFeature?.status == "failed"
      && nextFeature?.checkpoint == feature.checkpoint
  }

  func canRepair(_ feature: DeveloperRunnerFeature) -> Bool {
    !running && escalationRunning != true && !planningRunning && !emergencyPaused
      && githubPublicationUnresolved != true && githubSetupBusy != true && githubSetupUnresolved != true
      && repairLimit == 3 && repairActive == false
      && nextFeature?.id == feature.id
      && nextFeature?.canRepair == true
  }

  var canStartFeature: Bool {
    githubPublicationSupported == true && githubPublicationUnresolved != true
      && githubSetupBusy != true && githubSetupUnresolved != true
      && githubConnections != nil && githubConnections?.allSatisfy(\.isUsable) == true
  }

  var canStop: Bool {
    running || planningRunning || escalationRunning == true || githubPublicationRunning == true
      || githubSetupBusy == true
  }

  func githubConnection(for project: String) -> DeveloperGitHubConnection? {
    githubConnections?.first { $0.project == project }
  }
}

@MainActor
final class DeveloperRunnerModel: ObservableObject {
  @Published var snapshot: DeveloperRunnerSnapshot?
  @Published var error: String?
  @Published var sending = false
  private let configurationPath: String
  private var configuration: DeveloperRunnerConfiguration? {
    try? JSONDecoder().decode(DeveloperRunnerConfiguration.self,
      from: Data(contentsOf: URL(fileURLWithPath: configurationPath)))
  }
  private let session: URLSession
  private var actionError: String?

  init(configurationPath: String, session: URLSession? = nil) {
    self.configurationPath = configurationPath
    let settings = URLSessionConfiguration.ephemeral
    settings.timeoutIntervalForRequest = 10
    self.session = session ?? URLSession(configuration: settings)
  }

  private func request(path: String, body: [String: Any]? = nil,
    timeoutInterval: TimeInterval = 10) async throws
    -> DeveloperRunnerSnapshot
  {
    guard let configuration, let base = URL(string: configuration.endpoint),
      ["127.0.0.1", "localhost", "::1"].contains(base.host ?? ""), base.scheme == "http"
    else {
      throw NSError(
        domain: "Developer runner", code: 1,
        userInfo: [
          NSLocalizedDescriptionKey:
            "Open the developer build with its launcher to connect to Windows."
        ])
    }
    var request = URLRequest(url: base.appendingPathComponent(path))
    request.timeoutInterval = timeoutInterval
    request.setValue("Bearer \(configuration.token)", forHTTPHeaderField: "Authorization")
    if let body {
      request.httpMethod = "POST"
      request.setValue("application/json", forHTTPHeaderField: "Content-Type")
      request.httpBody = try JSONSerialization.data(withJSONObject: body)
    }
    let (data, response) = try await session.data(for: request)
    guard let response = response as? HTTPURLResponse, response.statusCode == 200 else {
      let detail = (try? JSONSerialization.jsonObject(with: data)) as? [String: String]
      throw NSError(
        domain: "Developer runner", code: 2,
        userInfo: [NSLocalizedDescriptionKey: detail?["error"] ?? "Windows runner is unavailable."])
    }
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let snapshot = try decoder.decode(DeveloperRunnerSnapshot.self, from: data)
    guard snapshot.mode == "supervised_developer", snapshot.hasRequiredReviewer, snapshot.hasRequiredPlanner else {
      throw NSError(domain: "Developer runner", code: 3,
        userInfo: [NSLocalizedDescriptionKey: "Update the Windows developer runner with its launcher. This app requires ChatGPT brainstorming before implementation and Codex review before success."])
    }
    return snapshot
  }

  func refresh() async {
    do {
      let observed = try await request(path: "status")
      if observed.revision >= (snapshot?.revision ?? 0) { snapshot = observed }
      error = actionError
    } catch {
      snapshot = nil
      self.error = error.localizedDescription
    }
  }

  func observe() async {
    while !Task.isCancelled {
      await refresh()
      try? await Task.sleep(for: .milliseconds(800))
    }
  }

  func send(_ action: String, values: [String: Any] = [:]) async {
    if sending && action != "emergency" && action != "stop" { return }
    sending = true
    defer { sending = false }
    do {
      var body = values
      body["action"] = action
      let updated = try await request(path: "control", body: body)
      if updated.revision >= (snapshot?.revision ?? 0) { snapshot = updated }
      actionError = nil
      error = nil
    } catch {
      actionError = error.localizedDescription
      self.error = actionError
    }
  }

  func saveGitHubConnection(project: String, repositoryURL: String, baseBranch: String,
    expectedRevision: UInt64) async throws {
    guard let current = snapshot, current.revision == expectedRevision,
      current.githubPublicationSupported == true,
      current.canManageGithubConnections == true, current.githubPublicationUnresolved != true,
      current.githubSetupBusy != true, current.githubSetupUnresolved != true,
      DeveloperGitHubPresentation.validProject(project),
      DeveloperGitHubURL.repository(repositoryURL) != nil,
      DeveloperGitHubPresentation.validBaseBranch(baseBranch) else {
      throw githubStateChangedError()
    }
    try await sendPublication(DeveloperGitHubRequest.saveConnection(project: project,
      repositoryURL: repositoryURL, baseBranch: baseBranch, expectedRevision: expectedRevision),
      expectedRevision: expectedRevision) { updated in
        DeveloperGitHubAcknowledgement.saved(updated, expectedRevision: expectedRevision,
          project: project, repositoryURL: repositoryURL, baseBranch: baseBranch)
      }
  }

  func disconnectGitHub(project: String, expectedRevision: UInt64) async throws {
    guard let current = snapshot, current.revision == expectedRevision,
      current.githubPublicationSupported == true,
      current.canManageGithubConnections == true, current.githubPublicationUnresolved != true,
      current.githubSetupBusy != true, current.githubSetupUnresolved != true,
      current.githubConnection(for: project) != nil else { throw githubStateChangedError() }
    try await sendPublication(DeveloperGitHubRequest.disconnect(project: project,
      expectedRevision: expectedRevision), expectedRevision: expectedRevision) { updated in
        DeveloperGitHubAcknowledgement.disconnected(updated, expectedRevision: expectedRevision,
          project: project)
      }
  }

  func reconcilePublication(_ feature: DeveloperRunnerFeature,
    expectedRevision: UInt64) async throws {
    guard let current = snapshot, current.revision == expectedRevision,
      current.githubPublicationSupported == true, current.githubPublicationUnresolved == true,
      current.queue.contains(where: { $0.id == feature.id && $0.checkpoint == feature.checkpoint
        && $0.publicationStatus == "attention" && $0.canReconcilePublication == true })
      else { throw githubStateChangedError() }
    try await sendPublication(DeveloperGitHubRequest.reconcile(featureID: feature.id,
      expectedRevision: expectedRevision, expectedCheckpoint: feature.checkpoint),
      expectedRevision: expectedRevision) { updated in
        DeveloperGitHubAcknowledgement.reconciled(updated, expectedRevision: expectedRevision,
          featureID: feature.id)
      }
  }

  private func githubStateChangedError() -> NSError {
    NSError(domain: "Developer GitHub", code: 2,
      userInfo: [NSLocalizedDescriptionKey:
        "The project, feature, or publication state changed. Reload before continuing."])
  }

  private func sendPublication(_ body: [String: Any], expectedRevision: UInt64,
    accepts: (DeveloperRunnerSnapshot) -> Bool) async throws {
    guard !sending else {
      throw NSError(domain: "Developer GitHub", code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Wait for the current request to finish."])
    }
    sending = true
    defer { sending = false }
    do {
      let updated = try await request(path: "publication", body: body, timeoutInterval: 210)
      guard updated.revision > expectedRevision,
        updated.revision >= (snapshot?.revision ?? expectedRevision), accepts(updated) else {
        throw NSError(domain: "Developer GitHub", code: 3,
          userInfo: [NSLocalizedDescriptionKey:
            "Windows did not confirm the requested GitHub state change. Reload before continuing."])
      }
      snapshot = updated
      actionError = nil
      error = nil
    } catch {
      actionError = error.localizedDescription
      self.error = actionError
      throw error
    }
  }
}

struct DeveloperRunnerView: View {
  private let configurationPath: String
  @StateObject private var model: DeveloperRunnerModel
  @StateObject private var connection: DeveloperConnectionModel
  @State private var confirmingStart = false
  @State private var startFeature: DeveloperRunnerFeature?
  @State private var confirmingRepair = false
  @State private var repairFeature: DeveloperRunnerFeature?
  @State private var showingEscalation = false
  @State private var escalationFeatureId: String?
  @State private var showingSettings = false
  @State private var showingGitHub = false
  @AppStorage("developerChatProject") private var chatProject = ""
  @AppStorage("developerChatRepairFeature") private var chatRepairFeature = ""

  init(configurationPath: String) {
    self.configurationPath = configurationPath
    _model = StateObject(wrappedValue: DeveloperRunnerModel(configurationPath: configurationPath))
    _connection = StateObject(wrappedValue: DeveloperConnectionModel(configurationPath: configurationPath))
  }
  private var next: DeveloperRunnerFeature? {
    model.snapshot?.nextFeature
  }
  private var startLabel: String {
    if let next, ["paused", "failed"].contains(next.status) { return "Resume" }
    return (model.snapshot?.queue.contains { $0.status == "succeeded" } ?? false)
      ? "Start next feature" : "Start"
  }
  var body: some View {
    HSplitView {
    ScrollView {
      VStack(alignment: .leading, spacing: 22) {
        HStack(alignment: .firstTextBaseline) {
          Text("Build with Assemblywright").font(.largeTitle.bold())
          Spacer()
          Button { showingGitHub = true } label: {
            Image(systemName: "arrow.triangle.branch").font(.title2)
          }
          .buttonStyle(.plain)
          .help("GitHub publication")
          .accessibilityLabel("GitHub publication")
          .accessibilityIdentifier("developer-github-publication")
          Button(action: { showingSettings = true }) {
            Image(systemName: "gearshape").font(.title2)
              .foregroundStyle(.primary)
          }
          .buttonStyle(.plain)
          .help("Settings")
          .accessibilityIdentifier("developer-settings")
          Text("Developer build").font(.caption.bold()).padding(7).background(
            .orange.opacity(0.15), in: Capsule())
        }
        if let snapshot = model.snapshot {
          Label("Connected to \(snapshot.host)", systemImage: "checkmark.circle.fill")
            .foregroundStyle(.green)
          Text(
            "Plan features with ChatGPT, then let the Mac implement the approved documents. Windows runs tests and requests Codex review."
          ).foregroundStyle(.secondary)
        } else {
          VStack(alignment: .leading, spacing: 6) {
            Label(connection.title, systemImage: connection.needsAttention ? "exclamationmark.circle" : "network")
              .foregroundStyle(connection.needsAttention ? .orange : .secondary)
            Text(connection.message).font(.caption).foregroundStyle(.secondary)
          }
        }
        if let error = model.error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
        if model.snapshot != nil && model.snapshot?.githubPublicationSupported != true {
          Label("Update the Windows developer runner before starting new work. Automatic GitHub publication is unavailable.",
            systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
        }
        if model.snapshot?.githubSetupUnresolved == true {
          Label("GitHub setup needs reconciliation before starting or changing feature work.",
            systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
        }
        DeveloperPlanningView(configurationPath: configurationPath, runner: model.snapshot)
        GroupBox("Assembly line") {
          VStack(alignment: .leading, spacing: 14) {
            HStack {
              Toggle(
                "Auto-run next feature",
                isOn: Binding(
                  get: { model.snapshot?.autoRun ?? true },
                  set: { enabled in
                    Task { await model.send("auto_run", values: ["enabled": enabled]) }
                  })
              )
              .disabled(model.snapshot == nil)
              Spacer()
              if model.snapshot?.running == true {
                ProgressView().controlSize(.small)
                Text("Running")
              } else if model.snapshot?.escalationRunning == true {
                ProgressView().controlSize(.small)
                Text("Preparing repair")
              } else if model.snapshot?.planningRunning == true {
                ProgressView().controlSize(.small)
                Text("Brainstorming")
              } else if model.snapshot?.emergencyPaused == true {
                Text("Emergency paused").foregroundStyle(.orange)
              } else {
                Text(next == nil ? "Ready" : "Waiting for you").foregroundStyle(.secondary)
              }
            }
            HStack {
              Button(startLabel) {
                startFeature = next
                confirmingStart = true
              }
                .buttonStyle(.borderedProminent)
                .disabled(
                  next == nil || next?.requiresEscalationRecovery == true || model.snapshot?.running == true || model.snapshot?.escalationRunning == true || model.snapshot?.planningRunning == true
                    || model.snapshot?.emergencyPaused == true || model.snapshot?.canStartFeature != true
                    || model.sending)
              Button("Stop") { Task { await model.send("stop") } }.disabled(
                model.snapshot?.canStop != true)
              Button("Emergency Pause", role: .destructive) {
                Task { await model.send("emergency") }
              }.disabled(model.snapshot == nil)
              if model.snapshot?.emergencyPaused == true {
                Button("Clear Emergency Pause") { Task { await model.send("clear_emergency") } }
                  .disabled(model.snapshot?.running == true || model.snapshot?.planningRunning == true || model.snapshot?.escalationRunning == true)
              }
            }
            Text(
              "Runs under your Windows account. Review the project and validation command before starting. OpenAI/Codex reviews generated code before success. Connected projects publish through a feature branch and merge after required GitHub checks pass."
            )
            .font(.caption).foregroundStyle(.secondary)
            Divider()
            if model.snapshot?.visibleQueue.isEmpty != false {
              Text("Add your first feature to begin.").foregroundStyle(.secondary)
            }
            ForEach(model.snapshot?.visibleQueue ?? []) { feature in
              VStack(alignment: .leading, spacing: 6) {
                HStack {
                  Text(feature.project).font(.headline)
                  Spacer()
                  Text(feature.resultLabel)
                    .foregroundStyle(
                      feature.status == "succeeded" && feature.hasApprovedReview
                        ? .green : feature.status == "failed" ? .red : .primary)
                  if feature.status == "failed" {
                    Button("Repair and retry") {
                      repairFeature = feature
                      confirmingRepair = true
                    }
                    .disabled(model.sending || model.snapshot?.canRepair(feature) != true)
                    .help("Ask the local model to fix this failure and rerun validation, up to three repair attempts per feature.")
                    .accessibilityIdentifier("developer-repair-\(feature.id)")
                  }
                  if feature.status == "failed" {
                    Button(feature.escalationStatus == "ready" ? "Review repair…" : "Ask AI to repair…") {
                      if feature.escalationStatus == "ready" {
                        escalationFeatureId = feature.id
                        showingEscalation = true
                      } else {
                        chatProject = feature.project
                        chatRepairFeature = feature.id + ":" + UUID().uuidString
                      }
                    }.disabled(model.sending || model.snapshot?.nextFeature?.id != feature.id)
                      .accessibilityIdentifier("developer-escalate-\(feature.id)")
                  }
                  if feature.canRemove {
                    Button("Remove", role: .destructive) {
                      Task { await model.send("remove", values: ["id": feature.id]) }
                    }
                    .disabled(model.sending || model.snapshot?.canRemove(feature) != true)
                    .help("Remove from the queue. Saved project files are kept. Stop the runner first if it is running.")
                    .accessibilityLabel("Remove feature from \(feature.project)")
                    .accessibilityIdentifier("developer-remove-\(feature.id)")
                  }
                }
                Text(feature.instruction)
                Text("Model computer: \(feature.modelComputer)")
                  .font(.caption).foregroundStyle(.secondary)
                if let attempts = feature.repairAttempts, attempts > 0 {
                  Text("Repair attempts: \(attempts) of 3")
                    .font(.caption).foregroundStyle(.secondary)
                }
                if let count = feature.escalationCount, count > 0 {
                  Text("Repair escalations: \(count) · \(feature.escalationStatus ?? "recorded")")
                    .font(.caption).foregroundStyle(.secondary)
                }
                Text("Checkpoint: \(feature.checkpoint.replacingOccurrences(of: "_", with: " "))")
                  .font(.caption).foregroundStyle(.secondary)
                if feature.planningStatus == "legacy_unplanned" {
                  Text("Brainstorming: not recorded for this earlier feature").font(.caption).foregroundStyle(.secondary)
                }
                Text(feature.reviewLabel).font(.caption)
                  .foregroundStyle(feature.hasApprovedReview ? .green : .secondary)
                DeveloperGitHubFeatureStatus(feature: feature, runner: model)
                if let summary = feature.reviewSummary, !summary.isEmpty {
                  Text(summary).font(.caption).textSelection(.enabled)
                }
                if feature.requiresEscalationRecovery {
                  Text("Repair application was interrupted. Ask AI to inspect the current files and prepare a fresh proposal.")
                    .font(.callout).foregroundStyle(.orange)
                }
                Text(feature.message).font(.caption).textSelection(.enabled)
                if !feature.changedFiles.isEmpty {
                  Text(feature.changedFiles.joined(separator: " · ")).font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                }
              }.padding(10).frame(maxWidth: .infinity, alignment: .leading).background(
                .quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 8))
            }
          }.padding(10)
        }
      }.padding(28)
    }.frame(minWidth: 650, minHeight: 680)
      DeveloperProjectChatView(configurationPath: configurationPath,
        projects: Array(Set(model.snapshot?.queue.map(\.project) ?? [])).sorted(),
        runner: model)
        .frame(minWidth: 340, idealWidth: 520, maxWidth: .infinity)
    }.frame(minWidth: 1000, minHeight: 680)
      .task { await model.observe() }
    .task { await connection.observe() }
      .sheet(isPresented: $showingSettings) {
        DeveloperRunnerSettingsView(configurationPath: configurationPath)
      }
      .sheet(isPresented: $showingGitHub) {
        DeveloperGitHubView(runner: model,
          projects: Array(Set(model.snapshot?.queue.map(\.project) ?? [])).sorted(),
          configurationPath: configurationPath)
      }
      .sheet(isPresented: $showingEscalation) {
        if let escalationFeatureId {
          DeveloperRepairEscalationView(configurationPath: configurationPath, runner: model,
            featureId: escalationFeatureId, diagnosis: nil, selectedModel: "mac")
        }
      }
      .confirmationDialog(
        "Repair this feature on Windows?", isPresented: $confirmingRepair,
        titleVisibility: .visible, presenting: repairFeature
      ) { feature in
        Button("Repair and retry") {
          if let attempts = feature.repairAttempts {
            Task {
              await model.send("repair", values: ["id": feature.id, "expected_attempts": attempts])
            }
          }
        }
        .disabled(model.sending || model.snapshot?.canRepair(feature) != true)
        Button("Cancel", role: .cancel) {}
      } message: { feature in
        Text(
          "The model on \(feature.modelComputer) will use the failure and current files to repair \(feature.project), "
            + "then rerun the same validation command and Codex review. Up to \(max(0, 3 - (feature.repairAttempts ?? 3))) "
            + "repair attempts remain. Stop and Emergency Pause stay available. "
            + "Auto-run advances only after validation and Codex approval."
        )
      }
      .confirmationDialog(
        "Run the queued work on Windows?", isPresented: $confirmingStart,
        titleVisibility: .visible, presenting: startFeature
      ) { feature in
        Button(["paused", "failed"].contains(feature.status) ? "Resume" : "Start next feature") {
          Task {
            await model.send(["paused", "failed"].contains(feature.status) ? "resume" : "start",
              values: feature.startBinding)
          }
        }
        .disabled(model.sending || model.snapshot?.running == true || model.snapshot?.planningRunning == true
          || model.snapshot?.emergencyPaused == true
          || model.snapshot?.canStartFeature != true
          || next?.id != feature.id || next?.status != feature.status
          || next?.checkpoint != feature.checkpoint || next?.modelTarget != feature.modelTarget)
        Button("Cancel", role: .cancel) {}
      } message: { feature in
        Text(DeveloperGitHubPresentation.startConfirmation(feature: feature,
          connection: model.snapshot?.githubConnection(for: feature.project)))
      }
  }
}
