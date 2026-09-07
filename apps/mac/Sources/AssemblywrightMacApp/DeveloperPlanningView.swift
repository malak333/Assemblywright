import Foundation
import SwiftUI

struct DeveloperPlanningSummary: Decodable, Identifiable {
  let featureId: String
  let project: String
  let instruction: String
  let stage: String
  let revision: UInt64
  let running: Bool
  var id: String { featureId }
}

struct DeveloperPlanningQuestion: Decodable {
  let text: String
  let choices: [String]
}
struct DeveloperPlanningAnswer: Decodable {
  let question: String
  let answer: String
}
struct DeveloperPlanningAssumptions: Decodable {
  let performance: String
  let scale: String
  let securityPrivacy: String
  let reliabilityAvailability: String
  let maintenanceOwnership: String
  let other: [String]
  var entries: [(String, String)] {
    [("Performance", performance), ("Scale", scale), ("Privacy and security", securityPrivacy),
     ("Reliability", reliabilityAvailability), ("Maintenance", maintenanceOwnership)]
  }
}
struct DeveloperPlanningApproach: Decodable, Identifiable {
  let id: String
  let title: String
  let summary: String
  let tradeoffs: [String]
  let recommended: Bool
}
struct DeveloperPlanningSection: Decodable, Identifiable {
  let id: String
  let title: String
  let body: String
  let confirmed: Bool
}
struct DeveloperPlanningDecision: Decodable {
  let decision: String
  let alternatives: [String]
  let reason: String
}
struct DeveloperPlanningDocuments: Decodable {
  let understanding: String
  let assumptions: String
  let decisionLog: String
  let design: String
  let implementationPlan: String
  let planSha256: String
  var entries: [(String, String)] {
    [("Requirements", understanding), ("Assumptions", assumptions),
     ("Decision log", decisionLog), ("Design", design), ("Implementation plan", implementationPlan)]
  }
}
struct DeveloperPlanningSnapshot: Decodable {
  let schemaVersion: Int
  let featureId: String
  let revision: UInt64
  let stage: String
  let running: Bool
  let availability: String
  let provider: String
  let model: String
  let project: String
  let instruction: String
  let validation: String
  let modelTarget: String
  let question: DeveloperPlanningQuestion?
  let answers: [DeveloperPlanningAnswer]?
  let understandingSummary: [String]
  let assumptions: DeveloperPlanningAssumptions?
  let openQuestions: [String]
  let approaches: [DeveloperPlanningApproach]
  let selectedApproachId: String?
  let designSections: [DeveloperPlanningSection]
  let decisionLog: [DeveloperPlanningDecision]
  let documents: DeveloperPlanningDocuments?
  let error: String?
  let lastRequestId: String?

  var hasRequiredPlanner: Bool {
    schemaVersion == 1 && provider == "openai.codex" && model == "gpt-5.6-sol"
  }
  var canRespond: Bool { hasRequiredPlanner && !running && availability == "available" }
  var currentSection: DeveloperPlanningSection? { designSections.first { !$0.confirmed } }
  var canConfirmUnderstanding: Bool {
    canRespond && stage == "understanding" && openQuestions.isEmpty
      && (5...7).contains(understandingSummary.count) && assumptions != nil
  }
  var canApprove: Bool {
    canRespond && stage == "ready" && openQuestions.isEmpty && selectedApproachId != nil
      && !designSections.isEmpty && designSections.allSatisfy(\.confirmed)
      && !decisionLog.isEmpty && documents?.planSha256.count == 64
  }
}

@MainActor
final class DeveloperPlanningModel: ObservableObject {
  @Published var snapshot: DeveloperPlanningSnapshot?
  @Published var error: String?
  @Published var sending = false
  private let configurationPath: String
  private var configuration: DeveloperRunnerConfiguration? {
    try? JSONDecoder().decode(DeveloperRunnerConfiguration.self,
      from: Data(contentsOf: URL(fileURLWithPath: configurationPath)))
  }
  private let session: URLSession
  private var selectedId = ""
  private var pending: (signature: Data, body: [String: Any])?

  init(configurationPath: String, session: URLSession? = nil) {
    self.configurationPath = configurationPath
    if let session {
      self.session = session
    } else {
      let settings = URLSessionConfiguration.ephemeral
      settings.timeoutIntervalForRequest = 10
      self.session = URLSession(configuration: settings)
    }
  }

  private func request(id: String, body: [String: Any]? = nil) async throws -> DeveloperPlanningSnapshot {
    guard let configuration, let base = URL(string: configuration.endpoint),
      ["127.0.0.1", "localhost", "::1"].contains(base.host ?? ""), base.scheme == "http",
      var components = URLComponents(url: base.appendingPathComponent("planning"), resolvingAgainstBaseURL: false)
    else { throw URLError(.badURL) }
    if body == nil { components.queryItems = [URLQueryItem(name: "id", value: id)] }
    guard let url = components.url else { throw URLError(.badURL) }
    var request = URLRequest(url: url)
    request.setValue("Bearer \(configuration.token)", forHTTPHeaderField: "Authorization")
    if let body {
      request.httpMethod = "POST"
      request.setValue("application/json", forHTTPHeaderField: "Content-Type")
      request.httpBody = try JSONSerialization.data(withJSONObject: body)
    }
    let (data, response) = try await session.data(for: request)
    guard (response as? HTTPURLResponse)?.statusCode == 200 else {
      let detail = (try? JSONSerialization.jsonObject(with: data)) as? [String: String]
      throw NSError(domain: "Feature planning", code: 1,
        userInfo: [NSLocalizedDescriptionKey: detail?["error"] ?? "Brainstorming is unavailable. Reopen the developer build with its launcher."])
    }
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let state = try decoder.decode(DeveloperPlanningSnapshot.self, from: data)
    guard state.featureId == id && state.hasRequiredPlanner else { throw URLError(.cannotParseResponse) }
    return state
  }

  private func accept(_ state: DeveloperPlanningSnapshot) {
    guard state.featureId == selectedId, state.revision >= (snapshot?.revision ?? 0) else { return }
    snapshot = state
    if state.lastRequestId == pending?.body["request_id"] as? String { pending = nil; error = nil }
    if pending == nil { error = nil }
  }

  func observe(id: String) async {
    if selectedId != id { selectedId = id; snapshot = nil; pending = nil; error = nil }
    guard !id.isEmpty else { return }
    while !Task.isCancelled && selectedId == id {
      do {
        let state = try await request(id: id)
        guard !Task.isCancelled && selectedId == id else { return }
        accept(state)
      } catch {
        if !Task.isCancelled && selectedId == id {
          snapshot = nil
          self.error = error.localizedDescription
        }
      }
      try? await Task.sleep(for: .milliseconds(800))
    }
  }

  func start(id: String, project: String, instruction: String, validation: String,
    modelTarget: String) async -> Bool {
    guard !sending, ["mac", "windows"].contains(modelTarget) else { return false }
    if selectedId != id { selectedId = id; snapshot = nil; pending = nil; error = nil }
    return await send("start", values: ["project": project, "instruction": instruction,
      "validation": validation, "model_target": modelTarget], revision: 0)
  }

  @discardableResult
  func send(_ action: String, values: [String: Any] = [:], revision: UInt64? = nil) async -> Bool {
    guard !sending, !selectedId.isEmpty else { return false }
    let id = selectedId
    var body = values
    body["action"] = action
    body["feature_id"] = id
    body["expected_revision"] = revision ?? snapshot?.revision ?? 0
    guard let signature = try? JSONSerialization.data(withJSONObject: body, options: [.sortedKeys]) else { return false }
    if pending?.signature == signature { body = pending!.body }
    else {
      body["request_id"] = UUID().uuidString.lowercased()
      pending = (signature, body)
    }
    sending = true
    defer { sending = false }
    do {
      let state = try await request(id: id, body: body)
      guard selectedId == id else { return false }
      accept(state)
      pending = nil
      error = nil
      return true
    } catch {
      if selectedId == id { self.error = error.localizedDescription }
      return false
    }
  }
}

struct DeveloperPlanningView: View {
  @StateObject private var model: DeveloperPlanningModel
  let runner: DeveloperRunnerSnapshot?
  @AppStorage("developerPlanningFeature") private var selectedId = ""
  @State private var newId = UUID().uuidString.lowercased()
  @State private var project = "first-project"
  @State private var instruction = ""
  @State private var validation = "python -m unittest discover -s tests -v"
  @State private var implementationTarget = "mac"
  @State private var answer = ""
  @State private var revisionFeedback = ""

  init(configurationPath: String, runner: DeveloperRunnerSnapshot?) {
    _model = StateObject(wrappedValue: DeveloperPlanningModel(configurationPath: configurationPath))
    self.runner = runner
  }

  private var disabled: Bool {
    model.sending || runner?.emergencyPaused != false || runner?.running != false
      || runner?.chatRunning == true || runner?.escalationRunning == true
  }
  var body: some View {
    GroupBox("Add a feature · Brainstorm with ChatGPT") {
      VStack(alignment: .leading, spacing: 12) {
        Text("Answer the questions, agree on a design, then send the approved documents to the local model.")
          .foregroundStyle(.secondary)
        HStack {
          Picker("Feature", selection: $selectedId) {
            Text("New feature").tag("")
            ForEach(runner?.planningSessions ?? []) { item in
              Text("\(item.project): \(item.instruction.prefix(55))").tag(item.featureId)
            }
          }.disabled(model.sending)
          if !selectedId.isEmpty {
            Button("New feature") { selectedId = ""; newId = UUID().uuidString.lowercased() }
              .disabled(model.sending || model.snapshot?.running == true)
          }
        }
        if runner?.chatRunning == true {
          Text("Wait for the project chat reply, or stop it, before continuing brainstorming.")
            .font(.caption).foregroundStyle(.secondary)
        }
        if selectedId.isEmpty {
          TextField("Project folder on Windows", text: $project)
          TextField("What should this feature do?", text: $instruction, axis: .vertical).lineLimit(3...8)
          TextField("Validation command", text: $validation)
          Picker("Implement on", selection: $implementationTarget) {
            ForEach(runner?.availableModelTargets ?? []) { target in
              Text("\(target.name) · \(target.model)").tag(target.id)
            }
          }
          .accessibilityIdentifier("developer-planning-model-target")
          .disabled(disabled)
          Text("OpenAI/Codex receives your request, answers, and selected project context for planning. Project chat on the right uses your selected local model.")
            .font(.caption).foregroundStyle(.secondary)
          Button("Brainstorm with ChatGPT") {
            let frozenTarget = implementationTarget
            Task {
              guard runner?.canSelectModel(frozenTarget) == true else { return }
              if await model.start(id: newId, project: project, instruction: instruction,
                validation: validation, modelTarget: frozenTarget) {
                selectedId = newId
                instruction = ""
                newId = UUID().uuidString.lowercased()
              }
            }
          }.buttonStyle(.borderedProminent)
            .accessibilityIdentifier("developer-brainstorm-start")
            .disabled(disabled || runner?.hasRequiredPlanner != true || runner?.planningRunning == true
              || runner?.canSelectModel(implementationTarget) != true
              || project.isEmpty || validation.isEmpty || instruction.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        } else if let state = model.snapshot, state.featureId == selectedId {
          Text(state.instruction).font(.headline).textSelection(.enabled)
          Text("\(state.project) · ChatGPT/Codex · \(state.model) · implement on \(state.modelTarget == "windows" ? "Windows" : "Mac")")
            .font(.caption).foregroundStyle(.secondary)
          if let answers = state.answers, !answers.isEmpty {
            DisclosureGroup("Answers so far") {
              ForEach(Array(answers.enumerated()), id: \.offset) { _, entry in
                VStack(alignment: .leading, spacing: 4) {
                  Text(entry.question).font(.headline)
                  Text(entry.answer).textSelection(.enabled)
                }.padding(.vertical, 4)
              }
            }
          }
          if state.running {
            HStack { ProgressView().controlSize(.small); Text("ChatGPT is preparing the next step…") }
            Button("Cancel brainstorming", role: .destructive) { Task { await model.send("cancel") } }
              .disabled(model.sending)
          } else if state.availability == "unavailable" {
            Text(state.error ?? "ChatGPT could not finish this step. Your answers are saved.").foregroundStyle(.orange)
            Button("Retry this step") { Task { await model.send("retry") } }.disabled(disabled)
          } else {
            stageContent(state)
          }
          if !state.running && ["understanding", "approaches", "design", "ready"].contains(state.stage) {
            DisclosureGroup("Request a change") {
              TextField("What should change?", text: $revisionFeedback, axis: .vertical).lineLimit(2...5)
              Button("Revise the plan") {
                Task { if await model.send("revise", values: ["answer": revisionFeedback]) { revisionFeedback = "" } }
              }.disabled(disabled || revisionFeedback.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
          }
          if !state.running && !["enqueued", "cancelled"].contains(state.stage) {
            Button("Cancel brainstorming", role: .destructive) { Task { await model.send("cancel") } }.disabled(model.sending)
          }
        } else { ProgressView("Loading saved brainstorming…") }
        if let error = model.error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
      }.padding(10).textFieldStyle(.roundedBorder)
    }
    .task(id: selectedId) { await model.observe(id: selectedId) }
    .onChange(of: model.snapshot?.revision) { _, _ in answer = "" }
  }

  @ViewBuilder
  private func stageContent(_ state: DeveloperPlanningSnapshot) -> some View {
    switch state.stage {
    case "questions":
      if let question = state.question {
        Text(question.text).textSelection(.enabled)
        ForEach(question.choices, id: \.self) { choice in
          Button(choice) { Task { await model.send("answer", values: ["answer": choice]) } }.disabled(disabled)
        }
        TextField("Your answer", text: $answer, axis: .vertical).lineLimit(2...6)
        Button("Send answer") {
          Task { if await model.send("answer", values: ["answer": answer]) { answer = "" } }
        }.disabled(disabled || answer.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
      }
    case "understanding":
      Text("Confirm the requirements").font(.headline)
      ForEach(Array(state.understandingSummary.enumerated()), id: \.offset) { _, line in Text("• \(line)") }
      if let assumptions = state.assumptions {
        Text("Assumptions").font(.headline)
        ForEach(assumptions.entries, id: \.0) { label, value in Text("\(label): \(value)") }
        ForEach(Array(assumptions.other.enumerated()), id: \.offset) { _, line in Text("• \(line)") }
      }
      ForEach(Array(state.openQuestions.enumerated()), id: \.offset) { _, line in Text("Unresolved: \(line)").foregroundStyle(.orange) }
      Button("Confirm understanding") { Task { await model.send("confirm_understanding") } }
        .disabled(disabled || !state.canConfirmUnderstanding)
    case "approaches":
      Text("Choose an approach").font(.headline)
      ForEach(state.approaches) { approach in
        VStack(alignment: .leading, spacing: 6) {
          Text(approach.title + (approach.recommended ? " · Recommended" : "")).font(.headline)
          Text(approach.summary)
          ForEach(Array(approach.tradeoffs.enumerated()), id: \.offset) { _, line in Text("• \(line)").font(.callout) }
          Button("Choose \(approach.title)") { Task { await model.send("select_approach", values: ["approach_id": approach.id]) } }
            .disabled(disabled)
        }.padding(8)
      }
    case "design":
      if let section = state.currentSection {
        Text(section.title).font(.headline)
        Text(section.body).textSelection(.enabled)
        Button("Approve this section") { Task { await model.send("confirm_design") } }.disabled(disabled)
      }
    case "ready", "enqueued":
      Text(state.stage == "ready" ? "Review the saved documents" : "Approved and added to the queue").font(.headline)
      if let documents = state.documents {
        ForEach(documents.entries, id: \.0) { title, body in
          DisclosureGroup(title) { Text(body).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading) }
        }
      }
      if state.stage == "ready" {
        Text("The local model will implement these documents. Use Start in the assembly line when you are ready to run it.")
          .font(.caption).foregroundStyle(.secondary)
        Button("Approve documents and add to queue") { Task { await model.send("approve_and_enqueue") } }
          .buttonStyle(.borderedProminent).disabled(disabled || !state.canApprove)
          .accessibilityIdentifier("developer-brainstorm-approve")
      }
    case "cancelled": Text("Brainstorming cancelled. No new work was queued.").foregroundStyle(.secondary)
    default: Text("This planning step needs an updated developer app.").foregroundStyle(.orange)
    }
  }
}
