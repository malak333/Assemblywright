import AppKit
import Foundation
import SwiftUI
import UniformTypeIdentifiers

struct DeveloperChatConversation: Identifiable, Equatable {
  let id: UUID
  var project: String
  var title: String
  var messages: [DeveloperChatMessage]
  var updatedAt: Date
  
  var isDraft: Bool { messages.isEmpty }
  
  static func == (lhs: DeveloperChatConversation, rhs: DeveloperChatConversation) -> Bool {
    lhs.id == rhs.id
  }
}

struct DeveloperChatGroup: Identifiable, Equatable {
  let projectId: String
  var conversations: [DeveloperChatConversation]
  var isExpanded: Bool
  
  init(projectId: String, conversations: [DeveloperChatConversation], isExpanded: Bool = true) {
    self.projectId = projectId
    self.conversations = conversations.sorted(by: { $0.updatedAt > $1.updatedAt })
    self.isExpanded = isExpanded
  }
  
  var id: String { projectId }
}

private struct ConversationListResponse: Decodable {
  let projects: [ConversationProject]
}

private struct ConversationProject: Decodable {
  let project: String
  let conversations: [ConversationSummary]
}

private struct ConversationSummary: Decodable {
  let id: String
  let title: String
  let messages: [DeveloperChatMessage]
  let updatedAt: Int64
  
  private enum CodingKeys: String, CodingKey {
    case id
    case title
    case messages
    case updatedAt = "updated_at"
  }
}

@MainActor
final class DeveloperChatHistoryModel: ObservableObject {
  @Published var groups: [DeveloperChatGroup] = []
  @Published var selectedConversationId: UUID?
  @Published var errorMessage: String?
  @Published var sending = false
  var activeProject: String = ""
  private let configurationPath: String
  private var configuration: DeveloperRunnerConfiguration? {
    try? JSONDecoder().decode(DeveloperRunnerConfiguration.self,
      from: Data(contentsOf: URL(fileURLWithPath: configurationPath)))
  }
  private let session: URLSession
  
  init(configurationPath: String) {
    self.configurationPath = configurationPath
    let settings = URLSessionConfiguration.ephemeral
    settings.timeoutIntervalForRequest = 15
    self.session = URLSession(configuration: settings)
  }
  
  func loadConversations() async {
    guard let configuration, let base = URL(string: configuration.endpoint) else { return }
    let components = URLComponents(url: base.appendingPathComponent("chat/conversations"),
                                   resolvingAgainstBaseURL: false)
    guard let url = components?.url else { return }
    
    var request = URLRequest(url: url)
    request.setValue("Bearer \(configuration.token)", forHTTPHeaderField: "Authorization")
    
    do {
      let (data, _) = try await session.data(for: request)
      let list = try JSONDecoder().decode(ConversationListResponse.self, from: data)
      let loaded = list.projects.map { project in
        let conversations = project.conversations.compactMap { summary -> DeveloperChatConversation? in
          guard let id = UUID(uuidString: summary.id) else { return nil }
          return DeveloperChatConversation(
            id: id,
            project: project.project,
            title: summary.title,
            messages: summary.messages,
            updatedAt: Date(timeIntervalSince1970: Double(summary.updatedAt)),
          )
        }
        return DeveloperChatGroup(projectId: project.project, conversations: conversations, isExpanded: true)
      }.sorted(by: { $0.projectId < $1.projectId })
      groups = loaded
      if let selectedConversationId {
        let stillExists = loaded.contains {
          $0.conversations.contains { $0.id == selectedConversationId }
        }
        if !stillExists {
          self.selectedConversationId = loaded.first?.conversations.first?.id
        }
      } else {
        self.selectedConversationId = loaded.first?.conversations.first?.id
      }
      if activeProject.isEmpty {
        activeProject = loaded.first?.projectId ?? ""
      }
    } catch {
      errorMessage = "Failed to load projects: \(error.localizedDescription)"
    }
  }
  
  func sendMessage(conversationId: UUID, message: String, attachments: [DeveloperChatAttachment] = [],
                   modelTarget: String = "windows") async -> Bool {
    guard let configuration, let base = URL(string: configuration.endpoint),
          var components = URLComponents(url: base.appendingPathComponent("chat"),
                                        resolvingAgainstBaseURL: false) else { return false }
    
    components.queryItems = [URLQueryItem(name: "project", value: activeProject)]
    guard let url = components.url else { return false }
    
    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("Bearer \(configuration.token)", forHTTPHeaderField: "Authorization")
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.httpBody = try? JSONSerialization.data(withJSONObject: [
      "project": activeProject,
      "conversation_id": conversationId.uuidString,
      "message": message,
      "attachments": attachments.map(\.wireValue),
      "model_target": modelTarget
    ])
    
    do {
      let (data, response) = try await session.data(for: request)
      guard (response as? HTTPURLResponse)?.statusCode == 200 else { return false }
      
      let decoder = JSONDecoder()
      decoder.keyDecodingStrategy = .convertFromSnakeCase
      let snapshot = try decoder.decode(DeveloperChatSnapshot.self, from: data)
      
      if let groupIndex = groups.firstIndex(where: { $0.projectId == activeProject }),
         let convIndex = groups[groupIndex].conversations.firstIndex(where: {
           $0.id == conversationId
         }) {
        groups[groupIndex].conversations[convIndex].messages = snapshot.messages
        groups[groupIndex].conversations[convIndex].updatedAt = Date()
      } else if let groupIndex = groups.firstIndex(where: { $0.projectId == activeProject }) {
        let title = snapshot.messages
          .first(where: { $0.role == "user" })?
          .content
          .trimmingCharacters(in: .whitespacesAndNewlines)
          .split(separator: "\n")
          .first
          .map(String.init)
          ?? "New conversation"
        groups[groupIndex].conversations.insert(
          DeveloperChatConversation(
            id: conversationId,
            project: activeProject,
            title: title,
            messages: snapshot.messages,
            updatedAt: Date(),
          ),
          at: 0,
        )
      }
      
      return true
    } catch {
      errorMessage = "Failed to send message: \(error.localizedDescription)"
      return false
    }
  }
  
  func createNewConversation(project: String) {
    let newId = UUID()
    let newConversation = DeveloperChatConversation(
      id: newId,
      project: project,
      title: "New conversation",
      messages: [],
      updatedAt: Date()
    )
    
    if let groupIndex = groups.firstIndex(where: { $0.projectId == project }) {
      groups[groupIndex].conversations.insert(newConversation, at: 0)
      groups[groupIndex].isExpanded = true
    } else {
      groups.append(DeveloperChatGroup(projectId: project,
                                       conversations: [newConversation],
                                       isExpanded: true))
    }
    
    selectedConversationId = newId
    activeProject = project
  }
  
  func selectConversation(project: String, id: UUID) {
    activeProject = project
    selectedConversationId = id
  }
}

struct DeveloperChatHistoryView: View {
  @StateObject private var model: DeveloperChatHistoryModel
  @AppStorage("developerChatModelTarget") private var selectedModelTarget = "windows"
  @State private var message = ""
  @State private var attachments: [DeveloperChatAttachment] = []
  @State private var choosingAttachments = false
  @StateObject private var runner: DeveloperRunnerModel
  
  private let configurationPath: String
  
  init(configurationPath: String, runner: DeveloperRunnerModel) {
    self.configurationPath = configurationPath
    _model = StateObject(wrappedValue: DeveloperChatHistoryModel(configurationPath: configurationPath))
    _runner = StateObject(wrappedValue: runner)
  }
  
  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack {
        Text("Conversations").font(.title2.bold())
        Spacer()
      }
      .padding(.horizontal)
      .padding(.vertical, 8)
      
      Divider()
      
      Button(action: {
        if model.activeProject.isEmpty, !model.groups.isEmpty {
          model.activeProject = model.groups[0].projectId
        }
        model.createNewConversation(project: model.activeProject)
      }) {
        Label("New Chat", systemImage: "plus")
      }
      .buttonStyle(.plain)
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(.horizontal)
      .padding(.vertical, 8)
      
      Divider()
      
      ScrollView {
        LazyVStack(spacing: 0) {
          ForEach(model.groups) { group in
            GroupRow(group: group,
                     selectedId: model.selectedConversationId,
                     onSelect: model.selectConversation)
          }
        }
      }
      .frame(maxHeight: .infinity)
      
      Divider()
      
      if let selectedConv = selectedConversation {
        VStack(alignment: .leading, spacing: 8) {
          if !attachments.isEmpty {
            ScrollView(.horizontal) {
              HStack(spacing: 8) {
                ForEach(Array(attachments.enumerated()), id: \.offset) { index, attachment in
                  VStack(alignment: .leading, spacing: 4) {
                    attachmentPreview(attachment)
                    Button("Remove") { attachments.remove(at: index) }
                      .font(.caption)
                      .disabled(model.sending)
                  }.frame(maxWidth: 100)
                }
              }
            }.frame(maxHeight: 100)
          }
          
          HStack {
            Button { choosingAttachments = true } label: {
              Label("Attach", systemImage: "paperclip")
            }.disabled(model.sending)
            
            TextField("Ask a question...", text: $message, axis: .vertical)
              .lineLimit(3...6)
              .textFieldStyle(.roundedBorder)
              .textFieldStyle(.plain)
          }
          
          HStack {
            Spacer()
            Button("Send") {
              Task {
                if await model.sendMessage(
                  conversationId: selectedConv.id,
                  message: message,
                  attachments: attachments,
                  modelTarget: selectedModelTarget
                ) {
                  message = ""
                  attachments = []
                }
              }
            }
            .buttonStyle(.borderedProminent)
            .disabled(model.sending || message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
          }
        }
        .padding(.horizontal)
        .padding(.vertical, 12)
      } else {
        Text("Select or create a conversation").foregroundStyle(.secondary)
          .padding()
      }
    }
    .fileImporter(isPresented: $choosingAttachments,
                  allowedContentTypes: [.image, .text, .json],
                  allowsMultipleSelection: true) { result in
      do {
        let urls = try result.get()
        guard urls.count + attachments.count <= 4 else {
          throw AttachmentError("Attach up to four files.")
        }
        let prepared = try urls.map { try DeveloperChatAttachment.read($0) }
        attachments.append(contentsOf: prepared)
      } catch { /* TODO: handle error */ }
    }
    .task { await model.loadConversations() }
  }
  
  private var selectedConversation: DeveloperChatConversation? {
    guard let id = model.selectedConversationId else { return nil }
    for group in model.groups {
      if let conv = group.conversations.first(where: { $0.id == id }) {
        return conv
      }
    }
    return nil
  }
  
  private func attachmentPreview(_ attachment: DeveloperChatAttachment) -> some View {
    HStack(spacing: 4) {
      if attachment.isImage, let data = attachment.data, let image = NSImage(data: data) {
        Image(nsImage: image).resizable().scaledToFit().frame(width: 50, height: 50)
      } else {
        Image(systemName: "doc.text").font(.caption)
      }
      Text(attachment.name).font(.caption).lineLimit(1)
    }
  }
}

private struct GroupRow: View {
  let group: DeveloperChatGroup
  let selectedId: UUID?
  let onSelect: (String, UUID) -> Void
  
  var body: some View {
    VStack(spacing: 0) {
      Button(action: {
        if group.conversations.isEmpty { return }
        onSelect(group.projectId, group.conversations[0].id)
      }) {
        HStack {
          Image(systemName: group.isExpanded ? "chevron.down" : "chevron.right")
            .font(.caption)
          Text(group.projectId)
            .font(.headline)
            .lineLimit(1)
          Spacer()
          Text("\(group.conversations.count)")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .padding(.horizontal)
        .padding(.vertical, 6)
      }
      .buttonStyle(.plain)
      .frame(maxWidth: .infinity, alignment: .leading)
      
      if group.isExpanded && !group.conversations.isEmpty {
        ForEach(group.conversations) { conversation in
          ConversationRow(conversation: conversation,
                         isSelected: conversation.id == selectedId,
                         onSelect: onSelect)
        }
      }
    }
  }
}

private struct ConversationRow: View {
  let conversation: DeveloperChatConversation
  let isSelected: Bool
  let onSelect: (String, UUID) -> Void
  
  var body: some View {
    Button(action: { onSelect(conversation.project, conversation.id) }) {
      HStack(alignment: .top, spacing: 8) {
        Image(systemName: conversation.isDraft ? "doc.badge.plus" : "bubble")
          .font(.caption)
          .foregroundStyle(isSelected ? Color.accentColor : .secondary)
        
        VStack(alignment: .leading, spacing: 2) {
          Text(conversation.title)
            .font(.subheadline)
            .lineLimit(1)
          if !conversation.messages.isEmpty {
            Text(lastMessagePreview)
              .font(.caption)
              .foregroundStyle(.secondary)
              .lineLimit(2)
          }
        }
        
        Spacer()
      }
      .padding(.horizontal)
      .padding(.vertical, 4)
      .background(isSelected ? Color(nsColor: .controlBackgroundColor) : Color.clear)
    }
    .buttonStyle(.plain)
    .frame(maxWidth: .infinity, alignment: .leading)
  }
  
  private var lastMessagePreview: String {
    let content = conversation.messages.last?.content ?? ""
    let preview = String(content.prefix(60)).trimmingCharacters(in: .whitespacesAndNewlines)
    return content.count > 60 ? preview + "…" : (preview.isEmpty ? "Empty" : preview)
  }
}
