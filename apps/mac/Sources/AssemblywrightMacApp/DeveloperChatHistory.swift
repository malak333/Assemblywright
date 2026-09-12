import Foundation

struct DeveloperChatSelection: Hashable, Codable {
  let project: String
  let chatId: String
}

struct DeveloperChatDraft {
  var message = ""
  var attachments: [DeveloperChatAttachment] = []
  var error: String?
  var isEmpty: Bool { message.isEmpty && attachments.isEmpty }
}

struct DeveloperChatConversation: Decodable, Identifiable {
  let id: String
  let project: String
  let title: String
  let revision: UInt64
  let updatedAt: UInt64
  var selection: DeveloperChatSelection { .init(project: project, chatId: id) }
  var updatedDate: Date { Date(timeIntervalSince1970: Double(updatedAt) / 1_000) }
}

struct DeveloperActiveChat: Decodable, Equatable {
  let project: String
  let chatId: String
  let requestId: String
  var title: String? = nil
  var selection: DeveloperChatSelection { .init(project: project, chatId: chatId) }
}

private struct DeveloperConversationPage: Decodable {
  let conversations: [DeveloperChatConversation]
  let nextCursor: String?
  let activeChat: DeveloperActiveChat?
}

@MainActor
final class DeveloperProjectChatModel: ObservableObject {
  @Published var snapshot: DeveloperChatSnapshot?
  @Published var projects: [String] = []
  @Published var error: String?
  @Published var sending = false
  @Published var draft = DeveloperChatDraft()
  @Published private(set) var conversations: [DeveloperChatConversation] = []
  @Published private(set) var activeChat: DeveloperActiveChat?
  @Published private(set) var historyError: String?
  @Published private(set) var historyLoaded = false
  @Published private(set) var hasMoreConversations = false
  @Published private(set) var loadingHistory = false
  @Published private(set) var loadingEarlier = false
  @Published private(set) var creating = false
  @Published private(set) var renaming = false
  @Published private(set) var changingAccess = false
  @Published private(set) var resolvingApproval = false
  @Published private(set) var selection = DeveloperChatSelection(project: "", chatId: "")
  var project: String { selection.project }
  var chatId: String { selection.chatId }
  var navigationToken: UUID { generation }
  var hasEarlierMessages: Bool { nextBefore != nil }
  private let configurationPath: String
  private let session: URLSession
  private var generation = UUID()
  private var historyGeneration = UUID()
  private var historyPages = 1
  private var nextBefore: String?
  private var retainedMessages: [DeveloperChatMessage] = []
  private var drafts: [DeveloperChatSelection: DeveloperChatDraft] = [:]
  private var actionErrors: [DeveloperChatSelection: String] = [:]
  private var historyActionError: String?
  private struct Pending {
    let message: String
    let attachments: [DeveloperChatAttachment]
    let modelTarget: String
    let id: String
  }
  private var pending: [DeveloperChatSelection: Pending] = [:]
  private var pendingCreates: [String: (id: String, reuse: String?)] = [:]

  init(configurationPath: String) {
    self.configurationPath = configurationPath
    let settings = URLSessionConfiguration.ephemeral
    settings.timeoutIntervalForRequest = 15
    session = URLSession(configuration: settings)
  }

  private func requestData(path: String, project: String, chatId: String = "",
    query: [URLQueryItem] = [], body: [String: Any]? = nil) async throws -> Data
  {
    let configuration = try JSONDecoder().decode(DeveloperRunnerConfiguration.self,
      from: Data(contentsOf: URL(fileURLWithPath: configurationPath)))
    guard let base = URL(string: configuration.endpoint), base.scheme == "http",
      ["127.0.0.1", "localhost", "::1"].contains(base.host ?? ""),
      var components = URLComponents(url: base.appendingPathComponent(path), resolvingAgainstBaseURL: false)
    else { throw URLError(.badURL) }
    if body == nil {
      components.queryItems = [URLQueryItem(name: "project", value: project)] + query
      if !chatId.isEmpty { components.queryItems?.append(URLQueryItem(name: "chat_id", value: chatId)) }
    }
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
        userInfo: [NSLocalizedDescriptionKey: detail?["error"] ?? "Chat is unavailable. Reconnect to Windows and try again."])
    }
    return data
  }

  private func decode<T: Decodable>(_ type: T.Type, _ data: Data) throws -> T {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return try decoder.decode(type, from: data)
  }

  private func request(path: String, selected: DeveloperChatSelection,
    query: [URLQueryItem] = [], body: [String: Any]? = nil) async throws -> DeveloperChatSnapshot
  {
    let data = try await requestData(path: path, project: selected.project,
      chatId: selected.chatId, query: query, body: body)
    let result = try decode(DeveloperChatSnapshot.self, data)
    guard result.project == selected.project,
      selected.chatId.isEmpty || (result.chatId == selected.chatId && result.historySupported == true)
    else { throw URLError(.cannotParseResponse) }
    return result
  }

  private func body(_ values: [String: Any], for selected: DeveloperChatSelection) -> [String: Any] {
    var result = values
    result["project"] = selected.project
    if !selected.chatId.isEmpty { result["chat_id"] = selected.chatId }
    return result
  }

  func select(project: String, chatId: String = "") {
    let new = DeveloperChatSelection(project: project, chatId: chatId)
    guard selection != new else { return }
    drafts[selection] = draft
    selection = new
    generation = UUID()
    draft = drafts[new] ?? DeveloperChatDraft()
    snapshot = nil
    retainedMessages = []
    nextBefore = nil
    error = actionErrors[new]
  }

  private func stillSelected(_ selected: DeveloperChatSelection, _ token: UUID) -> Bool {
    selection == selected && generation == token && !Task.isCancelled
  }

  private func apply(_ state: DeveloperChatSnapshot) {
    var state = state
    // Stable sequence IDs keep polling from dropping pages the owner opened.
    // Legacy snapshots have no sequence IDs and retain their original behavior.
    if state.messages.allSatisfy({ $0.sequence != nil }), !retainedMessages.isEmpty {
      var merged: [UInt64: DeveloperChatMessage] = [:]
      for item in retainedMessages + state.messages {
        if let sequence = item.sequence { merged[sequence] = item }
      }
      state.messages = merged.keys.sorted().compactMap { merged[$0] }
    }
    if retainedMessages.isEmpty { nextBefore = state.nextBefore }
    snapshot = state
    if state.historySupported == true { activeChat = state.activeChat }
    error = actionErrors[selection]
  }

  func refresh() async {
    let selected = selection, token = generation
    guard !selected.project.isEmpty else { return }
    do {
      let state = try await request(path: "chat", selected: selected)
      guard stillSelected(selected, token) else { return }
      apply(state)
    } catch {
      guard stillSelected(selected, token) else { return }
      snapshot = nil
      self.error = error.localizedDescription
    }
  }

  func observe(project: String, chatId: String = "") async {
    guard !Task.isCancelled else { return }
    select(project: project, chatId: chatId)
    let selected = selection, token = generation
    guard !project.isEmpty else { return }
    while stillSelected(selected, token) {
      await refresh()
      try? await Task.sleep(for: .seconds(1))
    }
  }

  func observeProjects() async {
    struct ProjectList: Decodable { let projects: [String] }
    while !Task.isCancelled {
      if let data = try? await requestData(path: "chat/projects", project: ""),
        let list = try? decode(ProjectList.self, data), !Task.isCancelled {
        projects = list.projects
      }
      try? await Task.sleep(for: .seconds(10))
    }
  }

  func refreshHistory() async {
    guard !loadingHistory else { return }
    loadingHistory = true
    defer { loadingHistory = false }
    let token = historyGeneration
    do {
      var rows: [DeveloperChatConversation] = []
      var cursor: String?
      var active: DeveloperActiveChat?
      for _ in 0..<historyPages {
        let query = cursor.map { [URLQueryItem(name: "cursor", value: $0)] } ?? []
        let data = try await requestData(path: "chat/conversations", project: "", query: query)
        let page = try decode(DeveloperConversationPage.self, data)
        rows += page.conversations
        cursor = page.nextCursor
        active = page.activeChat
        if cursor == nil { break }
      }
      guard token == historyGeneration, !Task.isCancelled else { return }
      var ids = Set<String>()
      conversations = rows.filter { ids.insert($0.id).inserted }
      activeChat = active
      hasMoreConversations = cursor != nil
      historyLoaded = true
      historyError = historyActionError
    } catch {
      guard token == historyGeneration, !Task.isCancelled else { return }
      historyError = error.localizedDescription
      historyLoaded = false
      // Previously observed history stays readable as labels only; every action
      // still requires a freshly validated conversation snapshot.
    }
  }

  func observeHistory() async {
    while !Task.isCancelled {
      await refreshHistory()
      try? await Task.sleep(for: .seconds(2))
    }
  }

  func loadMoreConversations() async {
    guard hasMoreConversations, !loadingHistory else { return }
    historyPages += 1
    await refreshHistory()
  }

  func loadEarlier() async {
    guard let before = nextBefore, !loadingEarlier, let current = snapshot else { return }
    let selected = selection, token = generation
    loadingEarlier = true
    defer { loadingEarlier = false }
    do {
      let page = try await request(path: "chat", selected: selected,
        query: [URLQueryItem(name: "before", value: before)])
      guard stillSelected(selected, token) else { return }
      retainedMessages = page.messages + retainedMessages + current.messages
      nextBefore = page.nextBefore
      // Merge into the latest snapshot, not the one captured before pagination.
      if let latest = snapshot { apply(latest) }
    } catch {
      if stillSelected(selected, token) { self.error = error.localizedDescription }
    }
  }

  func createConversation(project: String) async -> DeveloperChatSelection? {
    guard !project.isEmpty, !creating else { return nil }
    creating = true
    historyActionError = nil
    defer { creating = false }
    historyGeneration = UUID()
    if pendingCreates[project] == nil {
      let reuse = selection.project == project && draft.isEmpty && !chatId.isEmpty ? chatId : nil
      pendingCreates[project] = (UUID().uuidString.lowercased(), reuse)
    }
    guard let pending = pendingCreates[project] else { return nil }
    var values: [String: Any] = ["project": project, "id": pending.id]
    if let reuse = pending.reuse { values["reuse_chat_id"] = reuse }
    do {
      let data = try await requestData(path: "chat/conversations", project: project, body: values)
      let state = try decode(DeveloperChatSnapshot.self, data)
      guard state.project == project, let id = state.chatId, !id.isEmpty,
        state.historySupported == true else { throw URLError(.cannotParseResponse) }
      pendingCreates[project] = nil
      historyGeneration = UUID()
      historyError = nil
      await refreshHistory()
      return .init(project: project, chatId: id)
    } catch {
      historyActionError = error.localizedDescription
      historyError = historyActionError
      return nil
    }
  }

  func prepareRepairConversation(project: String, currentChatId: String, question: String) async -> DeveloperChatSelection? {
    // Synchronize the queue's selected project before suspending for creation.
    // A later UI synchronization of that same selection is then a no-op.
    select(project: project, chatId: currentChatId)
    let selected = selection, token = generation
    guard let created = await createConversation(project: project), stillSelected(selected, token) else { return nil }
    select(project: created.project, chatId: created.chatId)
    if draft.isEmpty { draft.message = question }
    return created
  }

  func rename(title: String, renderedSelection: DeveloperChatSelection, expectedRevision: UInt64) async -> Bool {
    let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
    guard selection == renderedSelection, snapshot?.chatId == renderedSelection.chatId,
      !trimmed.isEmpty, trimmed.count <= 80, !renaming else { return false }
    guard snapshot?.revision == expectedRevision else {
      error = "This conversation changed. Cancel and open Rename again to use its latest version."
      actionErrors[renderedSelection] = error
      return false
    }
    let token = generation
    renaming = true
    defer { renaming = false }
    historyGeneration = UUID()
    do {
      let state = try await request(path: "chat/rename", selected: renderedSelection,
        body: body(["title": trimmed, "expected_revision": expectedRevision], for: renderedSelection))
      actionErrors[renderedSelection] = nil
      historyGeneration = UUID()
      if stillSelected(renderedSelection, token) { apply(state) }
      await refreshHistory()
      return true
    } catch {
      actionErrors[renderedSelection] = error.localizedDescription
      if stillSelected(renderedSelection, token) { self.error = error.localizedDescription }
      return false
    }
  }

  func send(message: String, attachments: [DeveloperChatAttachment] = [], modelTarget: String = "windows",
    renderedSelection: DeveloperChatSelection? = nil) async -> Bool {
    let selected = selection, token = generation
    guard renderedSelection == nil || renderedSelection == selected else { return false }
    guard ["mac", "windows"].contains(modelTarget), !sending, !selected.project.isEmpty,
      (!message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !attachments.isEmpty) else { return false }
    do { try DeveloperChatAttachment.validateSelection(attachments) }
    catch { self.error = error.localizedDescription; return false }
    sending = true
    defer { sending = false }
    // An exact retry reuses its ID. An owner-edited message is a distinct request;
    // it cannot reinterpret the earlier request or bypass Windows admission.
    if pending[selected]?.message != message || pending[selected]?.attachments != attachments
      || pending[selected]?.modelTarget != modelTarget {
      pending[selected] = Pending(message: message, attachments: attachments, modelTarget: modelTarget,
        id: UUID().uuidString.lowercased())
    }
    guard let pending = pending[selected] else { return false }
    do {
      let state = try await request(path: "chat", selected: selected,
        body: body(["message": message, "id": pending.id,
          "attachments": attachments.map(\.wireValue), "model_target": pending.modelTarget], for: selected))
      actionErrors[selected] = nil
      if stillSelected(selected, token) { apply(state) }
      self.pending[selected] = nil
      // Clear only the submitted draft, even when the owner navigated away.
      if stillSelected(selected, token), draft.message == message, draft.attachments == attachments {
        draft = DeveloperChatDraft()
      } else if var saved = drafts[selected], saved.message == message, saved.attachments == attachments {
        saved = DeveloperChatDraft()
        drafts[selected] = saved
      }
      return true
    } catch {
      actionErrors[selected] = error.localizedDescription
      if stillSelected(selected, token) { self.error = error.localizedDescription }
      return false
    }
  }

  func cancel(_ renderedActive: DeveloperActiveChat? = nil) async {
    let selected: DeveloperChatSelection
    let id: String
    if let active = renderedActive {
      guard activeChat?.selection == active.selection,
        activeChat?.requestId == active.requestId else { return }
      selected = active.selection
      id = active.requestId
    } else {
      guard let requestId = snapshot?.requestId else { return }
      selected = selection
      id = requestId
    }
    let token = generation
    do {
      let state = try await request(path: "chat/cancel", selected: selected,
        body: body(["id": id], for: selected))
      actionErrors[selected] = nil
      if stillSelected(selected, token) { apply(state) }
      await refreshHistory()
    } catch {
      actionErrors[selected] = error.localizedDescription
      if stillSelected(selected, token) { self.error = error.localizedDescription }
      else { historyError = error.localizedDescription }
    }
  }

  func setAccessMode(_ mode: DeveloperToolAccessMode, renderedProject: String,
    renderedChatId: String = "", expectedRevision: UInt64) async
  {
    let selected = DeveloperChatSelection(project: renderedProject, chatId: renderedChatId)
    guard selection == selected, snapshot?.project == renderedProject,
      !project.isEmpty, !changingAccess, !sending,
      let access = snapshot?.toolAccess, access.available, access.mode != mode,
      access.revision == expectedRevision else { return }
    let token = generation
    changingAccess = true
    defer { changingAccess = false }
    do {
      let state = try await request(path: "chat/access", selected: selected,
        body: body(["mode": mode.rawValue, "expected_revision": expectedRevision], for: selected))
      actionErrors[selected] = nil
      if stillSelected(selected, token) { apply(state) }
    } catch {
      actionErrors[selected] = error.localizedDescription
      if stillSelected(selected, token) { snapshot = nil; self.error = error.localizedDescription }
    }
  }

  func decideApproval(_ decision: String, renderedProject: String, renderedChatId: String = "",
    approvalId: String, requestId: String, accessRevision: UInt64) async
  {
    let selected = DeveloperChatSelection(project: renderedProject, chatId: renderedChatId)
    guard ["approve", "deny"].contains(decision), selection == selected,
      snapshot?.project == renderedProject, !project.isEmpty, !resolvingApproval,
      let approval = snapshot?.pendingApproval, approval.id == approvalId,
      approval.requestId == requestId, approval.accessRevision == accessRevision else { return }
    let token = generation
    resolvingApproval = true
    defer { resolvingApproval = false }
    do {
      let state = try await request(path: "chat/approval", selected: selected,
        body: body(["request_id": requestId, "approval_id": approvalId,
          "access_revision": accessRevision, "decision": decision], for: selected))
      actionErrors[selected] = nil
      if stillSelected(selected, token) { apply(state) }
    } catch {
      actionErrors[selected] = error.localizedDescription
      if stillSelected(selected, token) { snapshot = nil; self.error = error.localizedDescription }
    }
  }
}
