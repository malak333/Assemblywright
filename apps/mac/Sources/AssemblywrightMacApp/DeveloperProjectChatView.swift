import AppKit
import UniformTypeIdentifiers
import Foundation
import SwiftUI

struct DeveloperChatMessage: Decodable {
  let role: String
  let content: String
  let attachments: [DeveloperChatAttachment]?
  var modelTarget: String? = nil
  var model: String? = nil
  var requestId: String? = nil
  var contentSha256: String? = nil

  var authorLabel: String {
    if role == "user" { return "You" }
    switch modelTarget ?? "windows" {
    case "mac": return "Mac AI"
    case "windows": return "Windows AI"
    default: return "AI (unavailable model)"
    }
  }
}

struct DeveloperChatSnapshot: Decodable {
  let project: String
  let messages: [DeveloperChatMessage]
  let running: Bool
  let requestId: String?
  let error: String?
  let contextLimit: Int?
  let contextTokens: Int?
  let contextFiles: [String]?
  let omittedMessages: Int?
  let omittedFiles: Int?
  var modelTarget: String? = nil
  var model: String? = nil
}

@MainActor
final class DeveloperProjectChatModel: ObservableObject {
  @Published var snapshot: DeveloperChatSnapshot?
  @Published var projects: [String] = []
  @Published var error: String?
  @Published var sending = false
  private(set) var project = ""
  private let configurationPath: String
  private var configuration: DeveloperRunnerConfiguration? {
    try? JSONDecoder().decode(DeveloperRunnerConfiguration.self,
      from: Data(contentsOf: URL(fileURLWithPath: configurationPath)))
  }
  private let session: URLSession
  private var actionError: String?
  private var pending: (project: String, message: String, attachments: [DeveloperChatAttachment], modelTarget: String, id: String)?

  init(configurationPath: String) {
    self.configurationPath = configurationPath
    let settings = URLSessionConfiguration.ephemeral
    settings.timeoutIntervalForRequest = 15
    session = URLSession(configuration: settings)
  }

  private func requestData(path: String, project: String, body: [String: Any]? = nil) async throws
    -> Data
  {
    guard let configuration, let base = URL(string: configuration.endpoint),
      ["127.0.0.1", "localhost", "::1"].contains(base.host ?? ""), base.scheme == "http",
      var components = URLComponents(url: base.appendingPathComponent(path), resolvingAgainstBaseURL: false)
    else { throw URLError(.badURL) }
    if body == nil { components.queryItems = [URLQueryItem(name: "project", value: project)] }
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
      throw NSError(domain: "Project chat", code: 1,
        userInfo: [NSLocalizedDescriptionKey: detail?["error"] ?? "Project chat is unavailable. Reopen the developer build with its launcher."])
    }
    return data
  }

  private func request(path: String, project: String, body: [String: Any]? = nil) async throws
    -> DeveloperChatSnapshot
  {
    let data = try await requestData(path: path, project: project, body: body)
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let result = try decoder.decode(DeveloperChatSnapshot.self, from: data)
    guard result.project == project else { throw URLError(.cannotParseResponse) }
    return result
  }

  func observeProjects() async {
    struct ProjectList: Decodable { let projects: [String] }
    while !Task.isCancelled {
      if let data = try? await requestData(path: "chat/projects", project: ""),
        let list = try? JSONDecoder().decode(ProjectList.self, from: data), !Task.isCancelled {
        projects = list.projects
      } else if !Task.isCancelled {
        projects = []
      }
      try? await Task.sleep(for: .seconds(10))
    }
  }

  func observe(project selected: String) async {
    project = selected
    snapshot = nil
    error = nil
    actionError = nil
    guard !selected.isEmpty else { return }
    while !Task.isCancelled && project == selected {
      do {
        let state = try await request(path: "chat", project: selected)
        guard !Task.isCancelled && project == selected else { return }
        snapshot = state
        error = actionError
      } catch {
        if !Task.isCancelled && project == selected {
          snapshot = nil
          self.error = error.localizedDescription
        }
      }
      try? await Task.sleep(for: .seconds(1))
    }
  }

  func send(message: String, attachments: [DeveloperChatAttachment] = [], modelTarget: String = "windows") async -> Bool {
    let selected = project
    guard ["mac", "windows"].contains(modelTarget), !sending, !selected.isEmpty, (!message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !attachments.isEmpty) else { return false }
    do { try DeveloperChatAttachment.validateSelection(attachments) }
    catch { self.error = error.localizedDescription; return false }
    sending = true
    defer { sending = false }
    if pending?.project != selected || pending?.message != message || pending?.attachments != attachments || pending?.modelTarget != modelTarget {
      pending = (selected, message, attachments, modelTarget, UUID().uuidString.lowercased())
    }
    guard let pending else { return false }
    do {
      let state = try await request(path: "chat", project: selected,
        body: ["project": selected, "message": message, "id": pending.id,
          "attachments": attachments.map(\.wireValue), "model_target": pending.modelTarget])
      if project == selected { snapshot = state; error = nil; actionError = nil }
      self.pending = nil
      return true
    } catch {
      if project == selected { actionError = error.localizedDescription; self.error = actionError }
      return false
    }
  }

  func cancel() async {
    guard let id = snapshot?.requestId else { return }
    let selected = project
    do {
      let state = try await request(path: "chat/cancel", project: selected,
        body: ["id": id])
      if project == selected { snapshot = state; error = nil; actionError = nil }
    } catch {
      if project == selected { actionError = error.localizedDescription; self.error = actionError }
    }
  }
}

struct DeveloperProjectChatView: View {
  @StateObject private var model: DeveloperProjectChatModel
  @AppStorage("developerChatProject") private var selectedProject = ""
  @AppStorage("developerChatRepairFeature") private var requestedRepairFeature = ""
  @FocusState private var questionFocused: Bool
  @AppStorage("developerChatModelTarget") private var selectedModelTarget = "windows"
  @State private var message = ""
  @State private var attachments: [DeveloperChatAttachment] = []
  @State private var attachmentError: String?
  @State private var choosingAttachments = false
  @State private var repairDiagnosis: DeveloperChatMessage?
  @State private var repairFeatureId: String?
  @State private var showingRepair = false
  private let configurationPath: String
  let projects: [String]
  @ObservedObject var runner: DeveloperRunnerModel

  init(configurationPath: String, projects: [String], runner: DeveloperRunnerModel) {
    _model = StateObject(wrappedValue: DeveloperProjectChatModel(configurationPath: configurationPath))
    self.configurationPath = configurationPath
    self.projects = projects
    self.runner = runner
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text("Project chat").font(.title2.bold())
      Text("Ask about your code, results, or how to run your project.")
        .foregroundStyle(.secondary)
      Picker("Project", selection: $selectedProject) {
        Text("Choose a project").tag("")
        ForEach(Array(Set(projects + model.projects)).sorted(), id: \.self) { Text($0).tag($0) }
      }
      .accessibilityIdentifier("developer-chat-project")
      .disabled(model.sending || model.snapshot?.running == true)
      Picker("AI", selection: $selectedModelTarget) {
        ForEach(runner.snapshot?.availableChatModelTargets ?? []) { target in
          Text("\(target.name) AI · \(target.model)").tag(target.id)
        }
      }
      .accessibilityIdentifier("developer-chat-model")
      .disabled(model.sending || model.snapshot?.running == true)
      if runner.snapshot?.canSelectChatModel(selectedModelTarget) != true {
        Text(runner.snapshot?.chatModelSelection == true
          ? "The selected AI is unavailable. Choose a configured AI."
          : "Update the developer build to enable AI selection in project chat.")
          .font(.caption).foregroundStyle(.secondary)
      }
      Divider()
      ScrollViewReader { proxy in
        ScrollView {
          LazyVStack(alignment: .leading, spacing: 16) {
            if model.snapshot?.messages.isEmpty != false {
              Text("Try “How do I open the GUI?” or “Why did this test fail?”")
                .foregroundStyle(.secondary)
            }
            ForEach(Array((model.snapshot?.messages ?? []).enumerated()), id: \.offset) { index, item in
              VStack(alignment: .leading, spacing: 5) {
                Text(item.authorLabel).font(.caption.bold())
                Text(item.content).textSelection(.enabled)
                if item.role == "assistant", item.requestId != nil, item.contentSha256 != nil,
                  let feature = runner.snapshot?.nextFeature, feature.project == selectedProject, feature.status == "failed" {
                  Button("Repair this feature…") {
                    repairDiagnosis = item
                    repairFeatureId = feature.id
                    showingRepair = true
                  }.disabled(model.sending || runner.snapshot?.canEscalate(feature) != true || model.snapshot?.running == true)
                    .accessibilityIdentifier("developer-chat-repair-\(index)")
                }
                ForEach(Array((item.attachments ?? []).enumerated()), id: \.offset) { _, attachment in
                  attachmentPreview(attachment)
                }
              }.frame(maxWidth: .infinity, alignment: .leading).id(index)
            }
            if model.snapshot?.running == true { ProgressView("\(model.snapshot?.modelTarget == "mac" ? "Mac AI" : "Windows AI") is answering…") }
          }.padding(.vertical, 8)
        }
        .onChange(of: model.snapshot?.messages.count) { _, count in
          if let count, count > 0 { proxy.scrollTo(count - 1, anchor: .bottom) }
        }
      }
      if let error = model.error ?? model.snapshot?.error {
        Text(error).font(.callout).foregroundStyle(.red).textSelection(.enabled)
      }
      if let state = model.snapshot, let capacity = state.contextLimit, capacity > 0 {
        Text("Context: \(state.contextTokens ?? 0) / \(capacity) tokens")
          .font(.caption).foregroundStyle(.secondary)
      }
      if let omitted = model.snapshot?.omittedMessages, omitted > 0 {
        Text("\(omitted) earlier messages were left out of this reply's context.")
          .font(.caption).foregroundStyle(.secondary)
      }
      if let omitted = model.snapshot?.omittedFiles, omitted > 0 {
        Text("Some project files or folders were left out of this reply's context.")
          .font(.caption).foregroundStyle(.secondary)
      }
      if let files = model.snapshot?.contextFiles, !files.isEmpty {
        DisclosureGroup("Project files used (\(files.count))") {
          ScrollView { Text(files.joined(separator: "\n")).font(.caption).textSelection(.enabled) }
            .frame(maxHeight: 100)
        }.font(.caption)
      }
      if let attachmentError {
        Text(attachmentError).font(.callout).foregroundStyle(.red)
      }
      if !attachments.isEmpty {
        ScrollView(.horizontal) {
          HStack(alignment: .top, spacing: 8) {
            ForEach(Array(attachments.enumerated()), id: \.offset) { index, attachment in
              VStack(alignment: .leading, spacing: 4) {
                attachmentPreview(attachment)
                Button("Remove") { attachments.remove(at: index); attachmentError = nil }
                  .accessibilityLabel("Remove \(attachment.name)")
                  .disabled(model.sending)
              }.frame(maxWidth: 150)
            }
          }
        }.frame(maxHeight: 125)
      }
      HStack {
        Button { choosingAttachments = true } label: { Label("Attach…", systemImage: "paperclip") }
          .accessibilityIdentifier("developer-chat-attach")
        Button("Paste image") { pasteImage() }
          .accessibilityIdentifier("developer-chat-paste-image")
        Text("Images or text files · up to 4").font(.caption).foregroundStyle(.secondary)
      }.disabled(selectedProject.isEmpty || model.sending || model.snapshot?.running == true)
      TextField("Ask a question about this project", text: $message, axis: .vertical)
        .lineLimit(3...7).textFieldStyle(.roundedBorder)
        .accessibilityIdentifier("developer-chat-message")
        .focused($questionFocused)
      HStack {
        Button("Send question") {
          let submitted = message
          let submittedAttachments = attachments
          let submittedProject = selectedProject
          let submittedTarget = selectedModelTarget
          Task {
            if await model.send(message: submitted, attachments: submittedAttachments, modelTarget: submittedTarget),
              selectedProject == submittedProject, message == submitted, attachments == submittedAttachments {
              message = ""; attachments = []; attachmentError = nil
            }
          }
        }.buttonStyle(.borderedProminent)
          .disabled(selectedProject.isEmpty || model.snapshot?.project != selectedProject || runner.snapshot?.canSelectChatModel(selectedModelTarget) != true || model.sending
            || model.snapshot?.running == true || (message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && attachments.isEmpty))
          .accessibilityIdentifier("developer-chat-send")
        if model.snapshot?.running == true {
          Button("Stop reply") { Task { await model.cancel() } }
        }
      }
      Text("Answers stay in chat. Use Repair this feature to review a fix, or Add a feature for new work.")
        .font(.caption).foregroundStyle(.secondary)
    }.padding(20)
      .onChange(of: requestedRepairFeature) { _, _ in
        if let feature = runner.snapshot?.nextFeature, feature.project == selectedProject, feature.status == "failed" {
          message = "Why did this feature fail, and what exact correction preserves its requirements? Check both the implementation and tests: " + feature.instruction
          questionFocused = true
        }
      }
      .sheet(isPresented: $showingRepair) {
        if let repairFeatureId {
          DeveloperRepairEscalationView(configurationPath: configurationPath, runner: runner,
            featureId: repairFeatureId, diagnosis: repairDiagnosis, selectedModel: selectedModelTarget)
        }
      }
      .fileImporter(isPresented: $choosingAttachments, allowedContentTypes: [.image, .text, .json, .sourceCode],
        allowsMultipleSelection: true) { result in
          do {
            let urls = try result.get()
            guard urls.count + attachments.count <= DeveloperChatAttachment.maximumCount else {
              throw AttachmentError("Attach up to four files per message.")
            }
            let prepared = try urls.map { try DeveloperChatAttachment.read($0) }
            try addAttachments(prepared)
          } catch { attachmentError = error.localizedDescription }
        }
      .onChange(of: selectedProject) { _, _ in
        message = ""; attachments = []; attachmentError = nil
      }
      .task { await model.observeProjects() }
      .task(id: selectedProject) { await model.observe(project: selectedProject) }
  }

  @ViewBuilder
  private func attachmentPreview(_ attachment: DeveloperChatAttachment) -> some View {
    HStack(spacing: 6) {
      if attachment.isImage, let data = attachment.data, let image = NSImage(data: data) {
        Image(nsImage: image).resizable().scaledToFit().frame(width: 65, height: 65)
      } else {
        Image(systemName: "doc.text").font(.title2).foregroundStyle(.secondary)
      }
      Text(attachment.name).font(.caption).lineLimit(2).help(attachment.name)
    }
  }

  private func addAttachments(_ prepared: [DeveloperChatAttachment]) throws {
    let combined = attachments + prepared
    try DeveloperChatAttachment.validateSelection(combined)
    attachments = combined
    attachmentError = nil
  }

  private func pasteImage() {
    do {
      let board = NSPasteboard.general
      if let png = board.data(forType: .png) {
        try addAttachments([DeveloperChatAttachment.image(png, name: "Pasted screenshot.png")])
      } else if let tiff = board.data(forType: .tiff) {
        try addAttachments([DeveloperChatAttachment.image(tiff, name: "Pasted image.tiff")])
      } else {
        throw AttachmentError("Copy an image first, then choose Paste image.")
      }
    } catch { attachmentError = error.localizedDescription }
  }

}
