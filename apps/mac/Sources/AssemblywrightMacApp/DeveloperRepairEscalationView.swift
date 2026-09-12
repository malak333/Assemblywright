import Foundation
import SwiftUI

struct DeveloperRepairBinding: Decodable {
  let featureId: String
  let checkpoint: String
  let revision: UInt64
}

struct DeveloperRepairFile: Decodable, Identifiable {
  let path: String
  let before: String?
  let after: String
  let protected: Bool
  var id: String { path }
}

struct DeveloperRepairProposal: Decodable {
  let status: String
  let featureId: String
  let proposalId: String?
  let modelTarget: String?
  let summary: String?
  let error: String?
  let files: [DeveloperRepairFile]?
  let binding: DeveloperRepairBinding?
  let count: Int?
  var diagnosis: String? = nil
  var chatRequestId: String? = nil
  var diagnosisSha256: String? = nil
  var chatModelTarget: String? = nil
  var chatModel: String? = nil
  var chatId: String? = nil

  var showsApproveAction: Bool { status == "ready" }
  var showsDiscardAction: Bool { ["ready", "unavailable"].contains(status) }
  var showsStopAction: Bool { status == "preparing" }
  var locksModelSelection: Bool { ["preparing", "ready"].contains(status) }
  var showsPrepareAction: Bool {
    ["cancelled", "unavailable", "failed", "interrupted"].contains(status)
  }
  var prepareActionTitle: String { showsPrepareAction ? "Prepare fresh repair" : "Prepare repair" }

  var statusMessage: String? {
    switch status {
    case "approved", "applying":
      return "Applying the exact approved changes on Windows."
    case "applied", "validating":
      return "The approved changes were applied. Windows is running the original validation."
    case "reviewing":
      return "Validation passed. OpenAI/Codex is reviewing the exact validated changes."
    case "succeeded":
      return "Repair succeeded. Validation and independent OpenAI/Codex review both passed."
    case "failed":
      return "The approved changes were applied, but validation or independent OpenAI/Codex review failed. Review the feature card, then prepare a fresh repair proposal."
    case "interrupted":
      return "Repair application was interrupted. Review the feature card, then prepare a fresh proposal; the remaining edits will not be replayed."
    case "cancelled":
      return "This proposal was discarded. You can prepare a fresh repair proposal."
    case "unavailable":
      return "This proposal is unavailable. Review the error, discard it if needed, then prepare a fresh repair proposal."
    default:
      return nil
    }
  }

  func canApprove(feature: DeveloperRunnerFeature, runner: DeveloperRunnerSnapshot) -> Bool {
    status == "ready" && proposalId?.isEmpty == false && featureId == feature.id
      && diagnosis?.isEmpty == false && chatRequestId?.isEmpty == false && diagnosisSha256?.count == 64
      && ["mac", "windows"].contains(chatModelTarget ?? "") && ["mac", "windows"].contains(modelTarget ?? "")
      && binding?.featureId == feature.id && binding?.checkpoint == feature.checkpoint
      && binding?.revision == runner.revision && feature.status == "failed"
      && runner.nextFeature?.id == feature.id && !runner.running && !runner.planningRunning
      && runner.chatRunning != true && !runner.emergencyPaused && runner.escalationRunning != true
      && files?.isEmpty == false
  }
}

@MainActor
final class DeveloperRepairEscalationModel: ObservableObject {
  @Published var proposal: DeveloperRepairProposal?
  @Published var error: String?
  @Published var sending = false
  @Published private(set) var observed = false
  private let configurationPath: String
  private let session: URLSession

  init(configurationPath: String) {
    self.configurationPath = configurationPath
    let settings = URLSessionConfiguration.ephemeral
    settings.timeoutIntervalForRequest = 15
    session = URLSession(configuration: settings)
  }

  private func request(featureId: String, body: [String: Any]? = nil) async throws -> DeveloperRepairProposal {
    let configuration = try JSONDecoder().decode(DeveloperRunnerConfiguration.self,
      from: Data(contentsOf: URL(fileURLWithPath: configurationPath)))
    guard let base = URL(string: configuration.endpoint), base.scheme == "http",
      ["127.0.0.1", "localhost", "::1"].contains(base.host ?? ""),
      var components = URLComponents(url: base.appendingPathComponent("repair/escalation"), resolvingAgainstBaseURL: false)
    else { throw URLError(.badURL) }
    if body == nil { components.queryItems = [URLQueryItem(name: "id", value: featureId)] }
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
      throw NSError(domain: "Repair", code: 1,
        userInfo: [NSLocalizedDescriptionKey: detail?["error"] ?? "Repair is unavailable. Reconnect to Windows and refresh."])
    }
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let result = try decoder.decode(DeveloperRepairProposal.self, from: data)
    guard result.featureId == featureId else { throw URLError(.cannotParseResponse) }
    return result
  }

  func observe(featureId: String) async {
    proposal = nil
    observed = false
    while !Task.isCancelled {
      do {
        let current = try await request(featureId: featureId)
        guard !Task.isCancelled else { return }
        proposal = current
        observed = true
      } catch {
        guard !Task.isCancelled else { return }
        proposal = nil
        observed = false
        self.error = error.localizedDescription
      }
      try? await Task.sleep(for: .seconds(1))
    }
  }

  func send(featureId: String, values: [String: Any]) async {
    guard !sending else { return }
    sending = true
    defer { sending = false }
    do {
      var body = values
      body["feature_id"] = featureId
      proposal = try await request(featureId: featureId, body: body)
      observed = true
      error = nil
    } catch {
      proposal = nil
      observed = false
      self.error = error.localizedDescription
    }
  }
}

struct DeveloperRepairEscalationView: View {
  @Environment(\.dismiss) private var dismiss
  @ObservedObject var runner: DeveloperRunnerModel
  @StateObject private var model: DeveloperRepairEscalationModel
  @State private var selectedModel: String
  let featureId: String
  let diagnosis: DeveloperChatMessage?

  init(configurationPath: String, runner: DeveloperRunnerModel, featureId: String,
       diagnosis: DeveloperChatMessage?, selectedModel: String) {
    self.runner = runner
    self.featureId = featureId
    self.diagnosis = diagnosis
    _selectedModel = State(initialValue: selectedModel)
    _model = StateObject(wrappedValue: DeveloperRepairEscalationModel(configurationPath: configurationPath))
  }

  private var feature: DeveloperRunnerFeature? {
    runner.snapshot?.queue.first { $0.id == featureId }
  }

  private var diagnosisRequestId: String? { diagnosis?.requestId ?? model.proposal?.chatRequestId }
  private var diagnosisDigest: String? { diagnosis?.contentSha256 ?? model.proposal?.diagnosisSha256 }
  private var diagnosisChatId: String? { diagnosis?.chatId ?? model.proposal?.chatId }

  private func modelLabel(for id: String?) -> String {
    guard let id else { return "Unavailable model computer" }
    if let target = runner.snapshot?.availableModelTargets.first(where: { $0.id == id }) {
      return "\(target.name) AI · \(target.model)"
    }
    switch id {
    case "mac": return "Mac AI"
    case "windows": return "Windows AI"
    default: return "Unavailable model computer"
    }
  }

  private var canPrepare: Bool {
    guard let state = runner.snapshot, let feature, model.observed,
      diagnosisRequestId != nil, diagnosisDigest != nil,
      !["preparing", "ready"].contains(model.proposal?.status ?? "none"), !model.sending else { return false }
    return state.canEscalate(feature) && state.canSelectModel(selectedModel)
  }

  var body: some View {
    VStack(spacing: 0) {
      HStack {
        Text("Repair this feature").font(.title2.bold())
        Spacer()
        Button("Close") { dismiss() }.keyboardShortcut(.cancelAction)
      }
      .padding(.horizontal, 24).padding(.vertical, 18)
      Divider()
      ScrollView {
        VStack(alignment: .leading, spacing: 14) {
      if let feature {
        Text(feature.project).font(.headline)
        Text(feature.instruction)
        Text("Earlier repair attempts: \(feature.repairAttempts ?? 0). Each approved escalation applies one proposed fix.")
          .font(.caption).foregroundStyle(.secondary)
      }
      if let proposal = model.proposal, proposal.locksModelSelection {
        LabeledContent("Repair with") {
          Text(modelLabel(for: proposal.modelTarget))
        }
        .accessibilityIdentifier("developer-escalation-model-locked")
      } else {
        Picker("Repair with", selection: $selectedModel) {
          ForEach(runner.snapshot?.availableModelTargets ?? []) { target in
            Text("\(target.name) AI · \(target.model)").tag(target.id)
          }
        }.disabled(model.sending)
          .accessibilityIdentifier("developer-escalation-model")
      }
      if let diagnosis {
        DisclosureGroup("Selected for a new proposal · \(diagnosis.authorLabel)") {
          Text(diagnosis.content).textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
      } else if model.proposal?.chatRequestId == nil {
        Text("Ask an AI about the failed feature in project chat, then use Repair this feature beneath its reply.")
          .foregroundStyle(.secondary)
      }
      if let proposal = model.proposal {
        if proposal.status == "preparing" {
          ProgressView("Preparing a repair for your review…")
        }
        if let summary = proposal.summary, !summary.isEmpty {
          Text(summary).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
        }
        if let error = proposal.error {
          Text(error).foregroundStyle(.red).textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
        }
        if let files = proposal.files, !files.isEmpty {
          if let boundDiagnosis = proposal.diagnosis {
            DisclosureGroup("This proposal uses the diagnosis from \(proposal.chatModelTarget == "mac" ? "Mac AI" : "Windows AI")") {
              Text(boundDiagnosis).textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
          }
          if proposal.status == "ready", proposal.binding?.revision != runner.snapshot?.revision {
            Text("The project or queue changed. Discard this proposal, then prepare a fresh one before approving.")
              .foregroundStyle(.orange)
          }
          Text("Proposed changes from \(proposal.modelTarget == "mac" ? "Mac AI" : "Windows AI")")
            .font(.headline)
          VStack(alignment: .leading, spacing: 18) {
            ForEach(files) { file in
              VStack(alignment: .leading, spacing: 8) {
                Text(file.path).font(.headline.monospaced())
                if file.protected {
                  Label("Protected test or validation input — approval permits this exact correction.", systemImage: "checkmark.shield")
                    .foregroundStyle(.orange)
                }
                Text("Before").font(.caption.bold())
                Text(file.before ?? "New file").font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                Divider()
                Text("After").font(.caption.bold())
                Text(file.after).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
              }
              .frame(maxWidth: .infinity, alignment: .leading)
              .padding(12).background(.quaternary.opacity(0.3), in: RoundedRectangle(cornerRadius: 8))
            }
          }
          Text("Approval applies only these changes on Windows, runs the original validation command, and sends the candidate to OpenAI/Codex for independent review. Auto-run can advance only after both gates pass.")
            .font(.caption).foregroundStyle(.secondary)
        }
        if let statusMessage = proposal.statusMessage {
          Text(statusMessage).font(.callout).fixedSize(horizontal: false, vertical: true)
        }
      }
      if let error = model.error {
        Text(error).foregroundStyle(.red).textSelection(.enabled)
          .fixedSize(horizontal: false, vertical: true)
      }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(24)
      }
      Divider()
      VStack(alignment: .leading, spacing: 8) {
        HStack {
          if model.proposal?.showsStopAction == true {
            Button("Stop preparing") { Task { await runner.send("stop") } }
          }
          if let proposal = model.proposal, proposal.showsDiscardAction, let id = proposal.proposalId {
            Button("Discard proposal") {
              guard let feature, let state = runner.snapshot else { return }
              Task {
                await model.send(featureId: featureId, values: ["action": "cancel", "proposal_id": id,
                  "expected_checkpoint": feature.checkpoint, "expected_revision": state.revision])
              }
            }.disabled(model.sending || runner.snapshot == nil)
              .accessibilityIdentifier("developer-escalation-discard")
          }
          if let proposal = model.proposal, proposal.showsApproveAction {
            Button("Approve and apply") {
              guard let binding = proposal.binding, let id = proposal.proposalId else { return }
              Task {
                var values: [String: Any] = ["action": "approve_and_apply",
                  "proposal_id": id, "expected_checkpoint": binding.checkpoint, "expected_revision": binding.revision]
                if let chatId = proposal.chatId { values["chat_id"] = chatId }
                await model.send(featureId: featureId, values: values)
              }
            }.buttonStyle(.borderedProminent)
              .disabled(model.sending || feature == nil || runner.snapshot == nil
                || !(feature.flatMap { feature in runner.snapshot.map { proposal.canApprove(feature: feature, runner: $0) } } ?? false))
              .accessibilityIdentifier("developer-escalation-approve")
          }
          if model.proposal == nil || model.proposal?.showsPrepareAction == true {
            Button(model.proposal?.prepareActionTitle ?? "Prepare repair") {
              guard let feature, let state = runner.snapshot, let id = diagnosisRequestId,
                let digest = diagnosisDigest else { return }
              Task {
                var values: [String: Any] = ["action": "prepare", "expected_checkpoint": feature.checkpoint,
                  "expected_revision": state.revision, "model_target": selectedModel,
                  "chat_request_id": id, "diagnosis_sha256": digest]
                if let chatId = diagnosisChatId { values["chat_id"] = chatId }
                await model.send(featureId: featureId, values: values)
              }
            }.disabled(!canPrepare).accessibilityIdentifier("developer-escalation-prepare")
          }
          Spacer()
        }
        if model.proposal == nil || model.proposal?.showsPrepareAction == true {
          Text("Preparation leaves project files unchanged.")
            .font(.caption).foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
      }
      .padding(.horizontal, 24).padding(.vertical, 16)
    }
    .frame(minWidth: 680, idealWidth: 800, maxWidth: 900,
      minHeight: 480, idealHeight: 640, maxHeight: 700)
      .task { await model.observe(featureId: featureId) }
  }
}
