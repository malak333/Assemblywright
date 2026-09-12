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
  var sequence: UInt64? = nil
  var chatId: String? = nil

  var authorLabel: String {
    if role == "user" { return "You" }
    switch modelTarget ?? "windows" {
    case "mac": return "Mac AI"
    case "windows": return "Windows AI"
    default: return "AI (unavailable model)"
    }
  }
}

enum DeveloperToolAccessMode: String, Codable, CaseIterable, Identifiable {
  case ask
  case auto
  case full

  var id: String { rawValue }
  var label: String {
    switch self {
    case .ask: return "Ask for approval"
    case .auto: return "Approve for me"
    case .full: return "Full access"
    }
  }
  var help: String {
    switch self {
    case .ask: return "Ask before commands, file changes, installations, or internet access."
    case .auto: return "Allow project edits and web searches. Ask before commands, installations, URL fetches, or outside-project access."
    case .full: return "Allow project tools, internet access, and files available to the Windows account."
    }
  }
}

struct DeveloperToolAccess: Decodable {
  let mode: DeveloperToolAccessMode
  let revision: UInt64
  let available: Bool
  let unavailableReason: String?
  let executionHost: String
}

private enum DeveloperJSONValue: Decodable {
  case string(String)
  case number(Double)
  case boolean(Bool)
  case object([String: DeveloperJSONValue])
  case array([DeveloperJSONValue])
  case null

  init(from decoder: Decoder) throws {
    let value = try decoder.singleValueContainer()
    if value.decodeNil() { self = .null }
    else if let decoded = try? value.decode(String.self) { self = .string(decoded) }
    else if let decoded = try? value.decode(Bool.self) { self = .boolean(decoded) }
    else if let decoded = try? value.decode(Double.self) { self = .number(decoded) }
    else if let decoded = try? value.decode([String: DeveloperJSONValue].self) { self = .object(decoded) }
    else { self = .array(try value.decode([DeveloperJSONValue].self)) }
  }

  var displayText: String {
    switch self {
    case .string(let value): return value
    case .number(let value):
      if value.rounded() == value, let integer = Int(exactly: value) { return String(integer) }
      return String(value)
    case .boolean(let value): return value ? "true" : "false"
    case .null: return "null"
    case .array(let values): return values.map(\.displayText).joined(separator: "\n")
    case .object(let values):
      return values.keys.sorted().map { "\($0): \(values[$0]!.displayText)" }.joined(separator: "\n")
    }
  }
}

struct DeveloperToolAction: Decodable, Identifiable {
  let id: String
  let requestId: String
  let tool: String
  let summary: String
  let status: String
  let output: String?
}

struct DeveloperToolApproval: Decodable {
  let id: String
  let requestId: String
  let summary: String
  let tool: String
  fileprivate let details: DeveloperJSONValue
  let accessRevision: UInt64
  var detailText: String { details.displayText }
}

struct DeveloperChatSnapshot: Decodable {
  let project: String
  var messages: [DeveloperChatMessage]
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
  var toolAccess: DeveloperToolAccess? = nil
  var toolActions: [DeveloperToolAction]? = nil
  var pendingApproval: DeveloperToolApproval? = nil
  var chatId: String? = nil
  var title: String? = nil
  var revision: UInt64? = nil
  var historySupported: Bool? = nil
  var nextBefore: String? = nil
  var activeChat: DeveloperActiveChat? = nil
}

struct DeveloperChatComposer: View {
  @Binding var text: String

  var body: some View {
    TextEditor(text: $text)
      .font(.body)
      .scrollContentBackground(.hidden)
      .padding(4)
      .frame(minHeight: 64, idealHeight: 84, maxHeight: 140)
      .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 5))
      .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(.quaternary))
      .overlay(alignment: .topLeading) {
        if text.isEmpty {
          Text("Ask a question about this project")
            .foregroundStyle(.tertiary)
            .padding(.horizontal, 9).padding(.vertical, 8)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
        }
      }
      .accessibilityLabel("Ask a question about this project")
      .help("Shift+Enter inserts a new line. Use Send question to send your message.")
  }
}

struct DeveloperToolApprovalView: View {
  let approval: DeveloperToolApproval
  let project: String
  let disabled: Bool
  let decide: (String) -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 7) {
      Label("Approval required", systemImage: "hand.raised.fill").font(.headline)
      Text(approval.summary).font(.body)
      Text("Project: \(project)").font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
      Text("Execution computer: Windows").font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
      Text(approval.tool).font(.caption.bold()).foregroundStyle(.secondary)
      Text(approval.detailText)
        .font(.system(.caption, design: .monospaced))
        .textSelection(.enabled)
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary, in: RoundedRectangle(cornerRadius: 5))
      DeveloperToolApprovalButtons(disabled: disabled, decide: decide)
    }
    .padding(10)
    .background(Color.accentColor.opacity(0.10), in: RoundedRectangle(cornerRadius: 7))
    .accessibilityIdentifier("developer-chat-tool-approval")
  }
}

private struct DeveloperToolApprovalButtons: NSViewRepresentable {
  let disabled: Bool
  let decide: (String) -> Void

  final class Coordinator: NSObject {
    var decide: (String) -> Void
    init(decide: @escaping (String) -> Void) { self.decide = decide }
    @objc func approve() { decide("approve") }
    @objc func deny() { decide("deny") }
  }

  func makeCoordinator() -> Coordinator { Coordinator(decide: decide) }

  func makeNSView(context: Context) -> NSStackView {
    let approve = NSButton(title: "Approve once", target: context.coordinator,
      action: #selector(Coordinator.approve))
    approve.bezelStyle = .rounded
    approve.setAccessibilityIdentifier("developer-chat-approve-tool")
    let deny = NSButton(title: "Deny", target: context.coordinator,
      action: #selector(Coordinator.deny))
    deny.bezelStyle = .rounded
    deny.setAccessibilityIdentifier("developer-chat-deny-tool")
    let stack = NSStackView(views: [approve, deny])
    stack.orientation = .horizontal
    stack.alignment = .centerY
    stack.spacing = 8
    return stack
  }

  func updateNSView(_ stack: NSStackView, context: Context) {
    context.coordinator.decide = decide
    stack.arrangedSubviews.compactMap { $0 as? NSControl }.forEach { $0.isEnabled = !disabled }
  }
}

struct DeveloperProjectChatView: View {
  @StateObject private var model: DeveloperProjectChatModel
  @AppStorage("developerChatProject") private var selectedProject = ""
  @AppStorage("developerChatRepairFeature") private var requestedRepairFeature = ""
  @FocusState private var questionFocused: Bool
  @AppStorage("developerChatModelTarget") private var selectedModelTarget = "windows"
  @AppStorage("developerChatSelections") private var savedSelections = "{}"
  @AppStorage("developerChatHistoryVisible") private var historyVisible = true
  @State private var collapsedProjects: Set<String> = []
  @State private var choosingProject = false
  @State private var renameTitle = ""
  @State private var renameBinding: DeveloperChatRenameBinding?
  @State private var attachmentSelection: DeveloperChatSelection?
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

  private var selections: [String: String] {
    (try? JSONDecoder().decode([String: String].self, from: Data(savedSelections.utf8))) ?? [:]
  }
  private var selection: DeveloperChatSelection {
    .init(project: selectedProject, chatId: selections[selectedProject] ?? "")
  }
  private var projectNames: [String] {
    Array(Set(projects + model.projects + model.conversations.map(\.project))).sorted()
  }
  private var canSend: Bool {
    guard let state = runner.snapshot else { return false }
    return state.chatHistory == true && !selection.chatId.isEmpty && model.selection == selection
      && model.snapshot?.chatId == selection.chatId && model.snapshot?.historySupported == true
      && !model.sending && model.activeChat == nil && !state.running && !state.planningRunning
      && state.chatRunning != true && state.escalationRunning != true && !state.emergencyPaused
      && state.githubSetupBusy != true && state.githubPublicationRunning != true
      && state.canSelectChatModel(selectedModelTarget)
  }
  private var workIsIdle: Bool {
    guard let state = runner.snapshot else { return false }
    return state.canChangeChatAccess && model.activeChat == nil
  }

  var body: some View {
    GeometryReader { geometry in
      let wide = geometry.size.width >= 740
      VStack(spacing: 0) {
        historyHeader(wide: wide)
        if let active = model.activeChat {
          HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 3) {
              Text("Reply or action in progress").font(.caption.bold())
              Text("\(active.project) · \(active.title ?? model.conversations.first { $0.id == active.chatId }?.title ?? "Chat")")
                .font(.caption).lineLimit(2)
              if active.selection != selection || historyVisible {
                Button("Return to active chat") { openChat(active.selection, hideHistory: !wide) }
                  .font(.caption).accessibilityIdentifier("developer-chat-return-active")
              }
            }
            Spacer(minLength: 4)
            Button("Stop") { Task { await model.cancel(active) } }
              .accessibilityLabel("Stop active chat reply or action")
              .accessibilityIdentifier("developer-chat-stop-active")
          }.padding(12).background(Color.accentColor.opacity(0.08))
        }
        if let error = model.historyError {
          Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading).padding(10)
        }
        Divider()
        if runner.snapshot?.chatHistory != true {
          VStack(spacing: 12) {
            Image(systemName: "bubble.left.and.bubble.right").font(.largeTitle).foregroundStyle(.secondary)
            Text(runner.snapshot == nil ? "Connecting to project chats…" : "Update the Developer build to use chat history.")
              .multilineTextAlignment(.center)
          }.frame(maxWidth: .infinity, maxHeight: .infinity).padding(24)
        } else {
          HStack(spacing: 0) {
            if historyVisible {
              historyBrowser(wide: wide)
                .frame(width: wide ? 230 : nil)
              if wide { Divider() }
            }
            if !historyVisible || wide {
              if !selection.chatId.isEmpty {
                conversationContent.frame(maxWidth: .infinity, maxHeight: .infinity)
              } else {
                VStack(spacing: 14) {
                  Image(systemName: "bubble.left.and.bubble.right").font(.largeTitle).foregroundStyle(.secondary)
                  Text("Choose a conversation or start a new chat.").multilineTextAlignment(.center)
                  Button("Browse history") { historyVisible = true }
                }.frame(maxWidth: .infinity, maxHeight: .infinity).padding(24)
              }
            }
          }
        }
      }
      .sheet(isPresented: $choosingProject) {
        VStack(alignment: .leading, spacing: 18) {
          HStack {
            Text("New chat · Choose a project").font(.headline)
            Spacer()
            Button("Close") { choosingProject = false }.keyboardShortcut(.cancelAction)
          }
          if projectNames.isEmpty { Text("Add a project in the build workspace first.").foregroundStyle(.secondary) }
          ScrollView {
            VStack(alignment: .leading, spacing: 12) {
              ForEach(projectNames, id: \.self) { name in
                Button(name) { choosingProject = false; newChat(in: name, hideHistory: !wide) }
                  .disabled(model.creating)
              }
            }.frame(maxWidth: .infinity, alignment: .leading)
          }
        }.padding(24).frame(width: 410, height: 320)
      }
    }
    .sheet(item: $renameBinding) { binding in
      VStack(alignment: .leading, spacing: 16) {
        Text("Rename chat").font(.headline)
        TextField("Chat title", text: $renameTitle).textFieldStyle(.roundedBorder)
          .accessibilityIdentifier("developer-chat-title-input")
        if let error = model.error { Text(error).font(.caption).foregroundStyle(.red) }
        HStack {
          Button("Cancel") { renameBinding = nil }.keyboardShortcut(.cancelAction)
          Spacer()
          Button("Save") {
            Task {
              if await model.rename(title: renameTitle, renderedSelection: binding.selection,
                expectedRevision: binding.revision) { renameBinding = nil }
            }
          }.keyboardShortcut(.defaultAction)
            .disabled(model.renaming || renameTitle.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || renameTitle.count > 80)
        }
      }.padding(24).frame(width: 380)
    }
    .onChange(of: selection, initial: true) { _, new in
      model.select(project: new.project, chatId: new.chatId)
    }
    .onChange(of: requestedRepairFeature) { _, _ in
      // The queue's Ask AI shortcut creates a distinct topic and never overwrites
      // an unsent question in the last opened conversation.
      if let feature = runner.snapshot?.nextFeature, feature.status == "failed" {
        let question = "Why did this feature fail, and what exact correction preserves its requirements? Check both the implementation and tests: " + feature.instruction
        let rememberedChat = selections[feature.project] ?? ""
        Task {
          if let created = await model.prepareRepairConversation(project: feature.project,
            currentChatId: rememberedChat, question: question) {
            openChat(created, hideHistory: true)
            questionFocused = true
          }
        }
      }
    }
    .task { await model.observeProjects() }
    .task(id: runner.snapshot?.chatHistory) {
      if runner.snapshot?.chatHistory == true { await model.observeHistory() }
    }
    .task(id: selection) {
      if !selection.chatId.isEmpty { await model.observe(project: selection.project, chatId: selection.chatId) }
    }
  }

  private func historyHeader(wide: Bool) -> some View {
    HStack(spacing: 10) {
      Button { historyVisible.toggle() } label: { Image(systemName: "sidebar.left") }
        .help("Show or hide chat history").accessibilityLabel("Chat history")
        .accessibilityIdentifier("developer-chat-history-toggle")
      VStack(alignment: .leading, spacing: 3) {
        Text(selectedProject.isEmpty ? "Project chat" : selectedProject)
          .font(.caption).foregroundStyle(.secondary).lineLimit(1)
        Text(model.snapshot?.title ?? model.conversations.first { $0.selection == selection }?.title ?? "Conversations")
          .font(.headline).lineLimit(1)
      }
      Spacer(minLength: 2)
      if let revision = model.snapshot?.revision, model.snapshot?.chatId == selection.chatId {
        Button {
          renameTitle = model.snapshot?.title ?? ""
          renameBinding = .init(selection: selection, revision: revision)
        } label: { Image(systemName: "pencil") }
          .help("Rename chat").accessibilityLabel("Rename chat")
          .accessibilityIdentifier("developer-chat-rename")
      }
      Button {
        if selectedProject.isEmpty { choosingProject = true }
        else { newChat(in: selectedProject, hideHistory: !wide) }
      } label: { Label("New Chat", systemImage: "plus") }
        .disabled(model.creating || runner.snapshot?.chatHistory != true)
        .accessibilityIdentifier("developer-chat-new")
    }.padding(14)
  }

  private func historyBrowser(wide: Bool) -> some View {
    ScrollView {
      LazyVStack(alignment: .leading, spacing: 18) {
        if !model.historyLoaded && model.historyError == nil { ProgressView("Loading chats…") }
        if model.historyLoaded && projectNames.isEmpty {
          Text("Your project conversations will appear here.").foregroundStyle(.secondary)
        }
        ForEach(projectNames, id: \.self) { project in
          VStack(alignment: .leading, spacing: 5) {
            HStack {
              Button {
                if collapsedProjects.contains(project) { collapsedProjects.remove(project) }
                else { collapsedProjects.insert(project) }
              } label: {
                Label(project, systemImage: collapsedProjects.contains(project) ? "chevron.right" : "chevron.down")
                  .font(.subheadline.bold()).lineLimit(2)
              }.buttonStyle(.plain).accessibilityLabel("\(project) chats")
              Spacer(minLength: 4)
              Button { newChat(in: project, hideHistory: !wide) } label: { Image(systemName: "plus") }
                .buttonStyle(.plain).disabled(model.creating)
                .accessibilityLabel("New chat in \(project)")
            }.padding(.horizontal, 7).padding(.bottom, 5)
            if !collapsedProjects.contains(project) {
              let chats = model.conversations.filter { $0.project == project }
              if chats.isEmpty { Text("No chats loaded").font(.caption).foregroundStyle(.secondary).padding(7) }
              ForEach(chats) { conversation in
                Button { openChat(conversation.selection, hideHistory: !wide) } label: {
                  VStack(alignment: .leading, spacing: 5) {
                    Text(conversation.title).font(.callout).lineLimit(2)
                    Text(conversation.updatedDate, style: .relative).font(.caption2).foregroundStyle(.secondary)
                  }.frame(maxWidth: .infinity, alignment: .leading).padding(9)
                    .background(conversation.selection == selection ? Color.accentColor.opacity(0.13) : Color.clear,
                      in: RoundedRectangle(cornerRadius: 7))
                    .contentShape(Rectangle())
                }.buttonStyle(.plain)
                  .accessibilityIdentifier("developer-chat-conversation-\(conversation.id)")
                  .accessibilityAddTraits(conversation.selection == selection ? [.isSelected] : [])
              }
            }
          }
        }
        if model.hasMoreConversations {
          Button("Load more chats") { Task { await model.loadMoreConversations() } }
            .disabled(model.loadingHistory).accessibilityIdentifier("developer-chat-more-conversations")
        }
      }.padding(12).frame(maxWidth: .infinity, alignment: .leading)
    }.background(Color(nsColor: .controlBackgroundColor))
      .accessibilityIdentifier("developer-chat-history")
  }

  private func openChat(_ selected: DeveloperChatSelection, hideHistory: Bool) {
    var saved = selections
    saved[selected.project] = selected.chatId
    if let data = try? JSONEncoder().encode(saved), let value = String(data: data, encoding: .utf8) {
      savedSelections = value
    }
    selectedProject = selected.project
    model.select(project: selected.project, chatId: selected.chatId)
    if hideHistory { historyVisible = false }
  }

  private func newChat(in project: String, hideHistory: Bool, initialMessage: String? = nil) {
    let starting = model.selection
    let token = model.navigationToken
    Task {
      if let created = await model.createConversation(project: project), model.selection == starting,
        model.navigationToken == token {
        openChat(created, hideHistory: hideHistory)
        if let initialMessage, model.draft.isEmpty { model.draft.message = initialMessage }
        questionFocused = true
      }
    }
  }

  @ViewBuilder private var conversationContent: some View {
    let renderedAccess = model.snapshot?.toolAccess
    VStack(alignment: .leading, spacing: 12) {
      HStack(alignment: .top, spacing: 12) {
        Picker("AI", selection: $selectedModelTarget) {
          ForEach(runner.snapshot?.availableChatModelTargets ?? []) { target in
            Text("\(target.name) AI · \(target.model)").tag(target.id)
          }
        }
        .accessibilityIdentifier("developer-chat-model")
        .disabled(model.sending || model.snapshot?.running == true)
        Picker("Access", selection: toolAccessSelection(renderedAccess,
          renderedProject: selectedProject, renderedChatId: selection.chatId)) {
          ForEach(DeveloperToolAccessMode.allCases) { mode in Text(mode.label).tag(mode) }
        }
        .accessibilityIdentifier("developer-chat-tool-access")
        .disabled(!workIsIdle || model.snapshot?.project != selectedProject
          || model.snapshot?.toolAccess?.available != true || model.sending
          || model.changingAccess || model.snapshot?.running == true)
      }
      if runner.snapshot?.canSelectChatModel(selectedModelTarget) != true {
        Text(runner.snapshot?.chatModelSelection == true
          ? "The selected AI is unavailable. Choose a configured AI."
          : "Update the developer build to enable AI selection in project chat.")
          .font(.caption).foregroundStyle(.secondary)
      }
      if let access = model.snapshot?.toolAccess {
        Text("Execution computer: \(access.executionHost == "windows" ? "Windows" : access.executionHost.capitalized) · \(access.mode.help)")
          .font(.caption).foregroundStyle(.secondary)
        if !access.available {
          Text(access.unavailableReason ?? "Tool access is unavailable in this developer build.")
            .font(.caption).foregroundStyle(.secondary)
        }
      }
      Divider()
      ScrollViewReader { proxy in
        ScrollView {
          LazyVStack(alignment: .leading, spacing: 16) {
            if model.hasEarlierMessages {
              Button("Load earlier messages") { Task { await model.loadEarlier() } }
                .disabled(model.loadingEarlier).accessibilityIdentifier("developer-chat-earlier")
            }
            if model.snapshot?.messages.isEmpty != false {
              Text("Start a new topic about this project. Your earlier conversations are in History.")
                .foregroundStyle(.secondary)
            }
            ForEach(Array((model.snapshot?.messages ?? []).enumerated()), id: \.offset) { index, item in
              VStack(alignment: .leading, spacing: 5) {
                Text(item.authorLabel).font(.caption.bold())
                Text(item.content).textSelection(.enabled)
                if item.role == "assistant", item.requestId != nil, item.contentSha256 != nil,
                  let feature = runner.snapshot?.nextFeature, feature.project == selectedProject, feature.status == "failed" {
                  Button("Repair this feature…") {
                    var diagnosis = item
                    diagnosis.chatId = model.snapshot?.chatId
                    repairDiagnosis = diagnosis
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
            if let approval = model.snapshot?.pendingApproval {
              DeveloperToolApprovalView(approval: approval, project: selectedProject,
                disabled: model.resolvingApproval || model.snapshot?.project != selectedProject) {
                  [renderedProject = selectedProject, renderedChatId = selection.chatId, approvalId = approval.id,
                    requestId = approval.requestId, accessRevision = approval.accessRevision] decision in
                  Task { await model.decideApproval(decision, renderedProject: renderedProject, renderedChatId: renderedChatId,
                    approvalId: approvalId, requestId: requestId, accessRevision: accessRevision) }
                }
            }
            if let actions = model.snapshot?.toolActions, !actions.isEmpty {
              VStack(alignment: .leading, spacing: 9) {
                Text("Tool actions").font(.headline)
                ForEach(actions) { action in
                  VStack(alignment: .leading, spacing: 3) {
                    HStack {
                      Text(action.tool).font(.caption.bold())
                      Spacer()
                      Text(action.status.replacingOccurrences(of: "_", with: " ").capitalized)
                        .font(.caption).foregroundStyle(toolStatusColor(action.status))
                    }
                    Text(action.summary).font(.callout)
                    if let output = action.output, !output.isEmpty {
                      Text(output)
                        .font(.system(.caption, design: .monospaced))
                        .lineLimit(8)
                        .textSelection(.enabled)
                    }
                  }
                  .padding(8)
                  .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 5))
                  .accessibilityIdentifier("developer-chat-tool-action-\(action.id)")
                }
              }
            }
            if model.snapshot?.running == true { ProgressView("\(model.snapshot?.modelTarget == "mac" ? "Mac AI" : "Windows AI") is answering…") }
          }.padding(.vertical, 8)
        }
        .onChange(of: model.snapshot?.messages.last?.sequence) { _, _ in
          if let count = model.snapshot?.messages.count, count > 0, !model.loadingEarlier {
            proxy.scrollTo(count - 1, anchor: .bottom)
          }
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
      if let attachmentError = model.draft.error {
        Text(attachmentError).font(.callout).foregroundStyle(.red)
      }
      if !model.draft.attachments.isEmpty {
        ScrollView(.horizontal) {
          HStack(alignment: .top, spacing: 8) {
            ForEach(Array(model.draft.attachments.enumerated()), id: \.offset) { index, attachment in
              VStack(alignment: .leading, spacing: 4) {
                attachmentPreview(attachment)
                Button("Remove") { model.draft.attachments.remove(at: index); model.draft.error = nil }
                  .accessibilityLabel("Remove \(attachment.name)")
                  .disabled(model.sending)
              }.frame(maxWidth: 150)
            }
          }
        }.frame(maxHeight: 125)
      }
      HStack {
        Button { attachmentSelection = selection; choosingAttachments = true } label: { Label("Attach…", systemImage: "paperclip") }
          .accessibilityIdentifier("developer-chat-attach")
        Button("Paste image") { pasteImage() }
          .accessibilityIdentifier("developer-chat-paste-image")
        Text("Images or text files · up to 4").font(.caption).foregroundStyle(.secondary)
      }.disabled(selectedProject.isEmpty || model.sending || model.snapshot?.running == true)
      DeveloperChatComposer(text: $model.draft.message)
        .accessibilityIdentifier("developer-chat-message")
        .focused($questionFocused)
      HStack {
        Button("Send question") {
          let submitted = model.draft.message
          let submittedAttachments = model.draft.attachments
          let submittedSelection = selection
          let submittedTarget = selectedModelTarget
          Task {
            if await model.send(message: submitted, attachments: submittedAttachments, modelTarget: submittedTarget,
              renderedSelection: submittedSelection),
              selection == submittedSelection, model.draft.message == submitted, model.draft.attachments == submittedAttachments {
              model.draft.message = ""; model.draft.attachments = []; model.draft.error = nil
            }
          }
        }.buttonStyle(.borderedProminent)
          .disabled(!canSend || (model.draft.message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && model.draft.attachments.isEmpty))
          .accessibilityIdentifier("developer-chat-send")
      }
      Text("Answers and tool actions stay in chat. Tools run on Windows under the selected access mode. Use Repair this feature to review a failed feature fix, or Add a feature for new work.")
        .font(.caption).foregroundStyle(.secondary)
    }.padding(16)
      .sheet(isPresented: $showingRepair) {
        if let repairFeatureId {
          DeveloperRepairEscalationView(configurationPath: configurationPath, runner: runner,
            featureId: repairFeatureId, diagnosis: repairDiagnosis, selectedModel: selectedModelTarget)
        }
      }
      .fileImporter(isPresented: $choosingAttachments, allowedContentTypes: [.image, .text, .json, .sourceCode],
        allowsMultipleSelection: true) { result in
          guard attachmentSelection == selection else { return }
          do {
            let urls = try result.get()
            guard urls.count + model.draft.attachments.count <= DeveloperChatAttachment.maximumCount else {
              throw AttachmentError("Attach up to four files per message.")
            }
            let prepared = try urls.map { try DeveloperChatAttachment.read($0) }
            try addAttachments(prepared)
          } catch { model.draft.error = error.localizedDescription }
        }

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
    let combined = model.draft.attachments + prepared
    try DeveloperChatAttachment.validateSelection(combined)
    model.draft.attachments = combined
    model.draft.error = nil
  }

  private func toolAccessSelection(_ access: DeveloperToolAccess?, renderedProject: String, renderedChatId: String)
    -> Binding<DeveloperToolAccessMode>
  {
    Binding(get: { access?.mode ?? .ask }, set: { mode in
      guard let access else { return }
      Task { await model.setAccessMode(mode, renderedProject: renderedProject, renderedChatId: renderedChatId,
        expectedRevision: access.revision) }
    })
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
    } catch { model.draft.error = error.localizedDescription }
  }

  private func toolStatusColor(_ status: String) -> Color {
    switch status {
    case "failed", "denied", "cancelled": return .red
    case "completed", "succeeded": return .green
    default: return .secondary
    }
  }

}

private struct DeveloperChatRenameBinding: Identifiable {
  let selection: DeveloperChatSelection
  let revision: UInt64
  var id: DeveloperChatSelection { selection }
}
