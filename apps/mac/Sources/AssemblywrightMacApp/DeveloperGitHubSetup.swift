import Foundation

struct DeveloperGitHubAccount: Decodable, Equatable {
  let state: String
  let login: String?
  let message: String

  var isSignedIn: Bool { state == "signed_in" && DeveloperGitHubSetupValidation.validLogin(login) }
  var isWellFormed: Bool {
    ["unknown", "signed_out", "signed_in", "unavailable"].contains(state)
      && (state != "signed_in" || isSignedIn)
  }
}

struct DeveloperGitHubRepository: Decodable, Equatable, Identifiable {
  let nameWithOwner: String
  let url: String
  let visibility: String
  let defaultBranch: String
  let canPush: Bool

  var id: String { nameWithOwner.lowercased() }
  var canSelect: Bool {
    canPush && isWellFormed && DeveloperGitHubPresentation.validBaseBranch(defaultBranch)
  }
  var isWellFormed: Bool {
    guard ["public", "private", "internal"].contains(visibility),
      let canonical = DeveloperGitHubURL.repository(url),
      let identity = DeveloperGitHubSetupValidation.repositoryIdentity(nameWithOwner) else {
      return false
    }
    return canonical.path.lowercased() == "/\(identity.owner)/\(identity.name)".lowercased()
      && (defaultBranch.isEmpty || DeveloperGitHubPresentation.validBaseBranch(defaultBranch))
  }
}

struct DeveloperGitHubSignIn: Decodable, Equatable {
  let operationId: String
  let userCode: String?
  let verificationUrl: String?
  let state: String
  let message: String

  var operationUUID: UUID? {
    guard let value = UUID(uuidString: operationId),
      value.uuidString.lowercased() == operationId else { return nil }
    return value
  }
  var hasSafeChallenge: Bool {
    state == "waiting" && DeveloperGitHubSetupValidation.validDeviceCode(userCode)
      && DeveloperGitHubSetupValidation.deviceURL(verificationUrl) != nil
  }
  var isWellFormed: Bool {
    operationUUID != nil
      && ["starting", "waiting", "succeeded", "cancelled", "failed", "attention"].contains(state)
      && (state != "waiting" || hasSafeChallenge)
  }
}

struct DeveloperGitHubCreation: Decodable, Equatable {
  let operationId: String
  let repositoryUrl: String?
  let repositoryId: String?
  let nameWithOwner: String?
  let visibility: String?
  let defaultBranch: String?
  let state: String
  let message: String

  var operationUUID: UUID? {
    guard let value = UUID(uuidString: operationId),
      value.uuidString.lowercased() == operationId else { return nil }
    return value
  }
  var isWellFormed: Bool {
    guard operationUUID != nil,
      ["creating", "succeeded", "attention", "existing", "absent"].contains(state) else {
      return false
    }
    switch state {
    case "succeeded":
      return repositoryId?.isEmpty == false && verifiedRepositoryIdentity != nil
        && defaultBranch.map(DeveloperGitHubPresentation.validBaseBranch) == true
    case "existing": return verifiedRepositoryIdentity != nil
    default: return targetIdentity != nil
    }
  }

  var targetIdentity: (owner: String, name: String, visibility: String)? {
    guard let nameWithOwner,
      let identity = DeveloperGitHubSetupValidation.repositoryIdentity(nameWithOwner),
      let visibility, ["public", "private"].contains(visibility) else { return nil }
    return (identity.owner, identity.name, visibility)
  }

  var verifiedRepositoryIdentity: (owner: String, name: String)? {
    guard let nameWithOwner, let identity = DeveloperGitHubSetupValidation.repositoryIdentity(nameWithOwner),
      let repositoryUrl, let canonical = DeveloperGitHubURL.repository(repositoryUrl),
      canonical.path.lowercased() == "/\(identity.owner)/\(identity.name)".lowercased(),
      ["public", "private"].contains(visibility ?? "") else { return nil }
    if let defaultBranch, !defaultBranch.isEmpty,
      !DeveloperGitHubPresentation.validBaseBranch(defaultBranch) { return nil }
    return identity
  }
}

struct DeveloperGitHubSetupSnapshot: Decodable, Equatable {
  let revision: UInt64
  let account: DeveloperGitHubAccount
  let repositories: [DeveloperGitHubRepository]
  let repositoryPage: Int
  let hasMore: Bool
  let signIn: DeveloperGitHubSignIn?
  let creation: DeveloperGitHubCreation?
  let busy: Bool
  let canMutate: Bool

  var isWellFormed: Bool {
    account.isWellFormed && (0...100).contains(repositoryPage)
      && (repositoryPage != 0 || (repositories.isEmpty && !hasMore))
      && repositories.allSatisfy(\.isWellFormed)
      && (signIn?.isWellFormed ?? true) && (creation?.isWellFormed ?? true)
  }
}

enum DeveloperGitHubSetupValidation {
  static let fixedDeviceURL = URL(string: "https://github.com/login/device")!

  static func validLogin(_ value: String?) -> Bool {
    guard let value, !value.isEmpty, value.count <= 39,
      value.first != "-", value.last != "-", !value.contains("--") else { return false }
    return value.unicodeScalars.allSatisfy {
      CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-")
        .contains($0)
    }
  }

  static func validRepositoryName(_ value: String) -> Bool {
    !value.isEmpty && value.count <= 100 && value != "." && value != ".."
      && value.unicodeScalars.allSatisfy {
        CharacterSet(charactersIn:
          "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.").contains($0)
      }
  }

  static func repositoryIdentity(_ value: String) -> (owner: String, name: String)? {
    let parts = value.split(separator: "/", omittingEmptySubsequences: false)
    guard parts.count == 2, validLogin(String(parts[0])),
      validRepositoryName(String(parts[1])) else { return nil }
    return (String(parts[0]), String(parts[1]))
  }

  static func validDeviceCode(_ value: String?) -> Bool {
    guard let value, value.count == 9 else { return false }
    for (index, scalar) in value.unicodeScalars.enumerated() {
      if index == 4 {
        if scalar != "-" { return false }
      } else if !CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789").contains(scalar) {
        return false
      }
    }
    return true
  }

  static func deviceURL(_ value: String?) -> URL? {
    guard value == fixedDeviceURL.absoluteString else { return nil }
    return fixedDeviceURL
  }
}

enum DeveloperGitHubSetupRequest {
  static func refresh(revision: UInt64) -> [String: Any] {
    ["action": "refresh_account", "expected_revision": revision]
  }
  static func list(page: Int, revision: UInt64) -> [String: Any] {
    ["action": "list_repositories", "page": page, "expected_revision": revision]
  }
  static func beginSignIn(operationID: UUID, revision: UInt64) -> [String: Any] {
    ["action": "begin_sign_in", "operation_id": operationID.uuidString.lowercased(),
     "expected_revision": revision]
  }
  static func signInAction(_ action: String, operationID: String,
    revision: UInt64) -> [String: Any] {
    ["action": action, "operation_id": operationID, "expected_revision": revision]
  }
  static func create(operationID: UUID, expectedLogin: String, name: String,
    visibility: String, revision: UInt64) -> [String: Any] {
    ["action": "create_repository", "operation_id": operationID.uuidString.lowercased(),
     "expected_login": expectedLogin, "name": name, "visibility": visibility,
     "expected_revision": revision]
  }
  static func reconcileCreation(operationID: String, revision: UInt64) -> [String: Any] {
    ["action": "reconcile_creation", "operation_id": operationID,
     "expected_revision": revision]
  }
}

enum DeveloperGitHubSetupAcknowledgement {
  private static func signInAccountMatches(_ value: DeveloperGitHubSetupSnapshot) -> Bool {
    switch value.signIn?.state {
    case "succeeded": return value.account.isSignedIn
    case "starting", "waiting": return !value.account.isSignedIn
    case "cancelled", "failed", "attention": return true
    default: return false
    }
  }

  static func refresh(_ value: DeveloperGitHubSetupSnapshot, after revision: UInt64) -> Bool {
    value.revision > revision && value.isWellFormed && value.account.state != "unknown"
  }
  static func page(_ value: DeveloperGitHubSetupSnapshot, expectedPage: Int,
    after revision: UInt64) -> Bool {
    (1...100).contains(expectedPage) && value.revision > revision && value.isWellFormed
      && value.repositoryPage == expectedPage
  }
  static func begin(_ value: DeveloperGitHubSetupSnapshot, operationID: UUID,
    after revision: UInt64) -> Bool {
    value.revision > revision && value.isWellFormed
      && value.signIn?.operationId == operationID.uuidString.lowercased()
      && signInAccountMatches(value)
  }
  static func cancel(_ value: DeveloperGitHubSetupSnapshot, operationID: String,
    after revision: UInt64) -> Bool {
    value.revision > revision && value.isWellFormed && value.signIn?.operationId == operationID
      && ["cancelled", "succeeded", "failed", "attention"].contains(value.signIn?.state ?? "")
      && signInAccountMatches(value)
  }
  static func reconcileSignIn(_ value: DeveloperGitHubSetupSnapshot, operationID: String,
    after revision: UInt64) -> Bool {
    value.revision > revision && value.isWellFormed && value.signIn?.operationId == operationID
      && ["succeeded", "failed", "attention"].contains(value.signIn?.state ?? "")
      && signInAccountMatches(value)
  }
  static func create(_ value: DeveloperGitHubSetupSnapshot, operationID: UUID,
    expectedLogin: String, name: String, visibility: String, after revision: UInt64) -> Bool {
    guard value.revision > revision, value.isWellFormed,
      let creation = value.creation,
      creation.operationId == operationID.uuidString.lowercased(),
      ["creating", "succeeded", "attention"].contains(creation.state),
      creation.visibility == visibility,
      let identity = DeveloperGitHubSetupValidation.repositoryIdentity(creation.nameWithOwner ?? "")
      else { return false }
    return identity.owner.caseInsensitiveCompare(expectedLogin) == .orderedSame
      && identity.name.caseInsensitiveCompare(name) == .orderedSame
  }
  static func reconcileCreation(_ value: DeveloperGitHubSetupSnapshot,
    expected: DeveloperGitHubCreation, after revision: UInt64) -> Bool {
    guard value.revision > revision, value.isWellFormed, let actual = value.creation,
      actual.operationId == expected.operationId,
      ["succeeded", "existing", "attention", "absent"].contains(actual.state),
      let expectedTarget = expected.targetIdentity, let actualTarget = actual.targetIdentity,
      expectedTarget.owner.caseInsensitiveCompare(actualTarget.owner) == .orderedSame,
      expectedTarget.name.caseInsensitiveCompare(actualTarget.name) == .orderedSame,
      expectedTarget.visibility == actualTarget.visibility else { return false }
    if let repositoryID = expected.repositoryId,
      actual.repositoryId != repositoryID { return false }
    if let repositoryURL = expected.repositoryUrl {
      guard DeveloperGitHubURL.repository(repositoryURL)?.absoluteString
        == DeveloperGitHubURL.repository(actual.repositoryUrl ?? "")?.absoluteString else { return false }
    }
    return actual.state != "succeeded"
      || (actual.repositoryId?.isEmpty == false && actual.verifiedRepositoryIdentity != nil)
  }
}

@MainActor
final class DeveloperGitHubSetupModel: ObservableObject {
  @Published var snapshot: DeveloperGitHubSetupSnapshot?
  @Published var repositories: [DeveloperGitHubRepository] = []
  @Published var error: String?
  @Published var sending = false
  private let configurationPath: String
  private let observationSession: URLSession
  private let mutationSession: URLSession

  init(configurationPath: String) {
    self.configurationPath = configurationPath
    let observation = URLSessionConfiguration.ephemeral
    observation.timeoutIntervalForRequest = 10
    observationSession = URLSession(configuration: observation)
    let mutation = URLSessionConfiguration.ephemeral
    mutation.timeoutIntervalForRequest = 210
    mutationSession = URLSession(configuration: mutation)
  }

  func observe() async {
    while !Task.isCancelled {
      do {
        let value = try await request(session: observationSession)
        guard value.isWellFormed else { throw responseError() }
        if !sending && value.revision >= (snapshot?.revision ?? 0) { acceptObserved(value) }
      } catch { self.error = error.localizedDescription }
      try? await Task.sleep(for: .milliseconds(800))
    }
  }

  func refreshAccount() async {
    let expectedRevision = revision
    await mutate(DeveloperGitHubSetupRequest.refresh(revision: expectedRevision)) {
      DeveloperGitHubSetupAcknowledgement.refresh($0, after: expectedRevision)
    }
  }

  func listRepositories(page: Int) async {
    let expectedRevision = revision
    guard snapshot?.account.isSignedIn == true, page == 1 || page == (snapshot?.repositoryPage ?? 0) + 1
      else { error = "Reload the first repository page before continuing."; return }
    let expectedAccount = snapshot?.account
    await mutate(DeveloperGitHubSetupRequest.list(page: page, revision: expectedRevision)) { value in
      DeveloperGitHubSetupAcknowledgement.page(value, expectedPage: page, after: expectedRevision)
        && value.account == expectedAccount
    } accept: { value in
      if page == 1 { self.repositories = value.repositories }
      else { self.repositories = self.repositories + value.repositories.filter { item in
        !self.repositories.contains { $0.id == item.id }
      } }
    }
  }

  func beginSignIn() async {
    guard snapshot?.account.state == "signed_out" else { return }
    let expectedRevision = revision, operationID = UUID()
    await mutate(DeveloperGitHubSetupRequest.beginSignIn(operationID: operationID,
      revision: expectedRevision)) {
        DeveloperGitHubSetupAcknowledgement.begin($0, operationID: operationID,
          after: expectedRevision)
      }
  }

  func cancelSignIn() async {
    guard let signIn = snapshot?.signIn else { return }
    let expectedRevision = revision
    await mutate(DeveloperGitHubSetupRequest.signInAction("cancel_sign_in",
      operationID: signIn.operationId, revision: expectedRevision), accepts: {
        DeveloperGitHubSetupAcknowledgement.cancel($0, operationID: signIn.operationId,
          after: expectedRevision)
      }, allowBusy: true)
  }

  func reconcileSignIn() async {
    guard let signIn = snapshot?.signIn, signIn.state == "attention" else { return }
    let expectedRevision = revision
    await mutate(DeveloperGitHubSetupRequest.signInAction("reconcile_sign_in",
      operationID: signIn.operationId, revision: expectedRevision)) {
        DeveloperGitHubSetupAcknowledgement.reconcileSignIn($0, operationID: signIn.operationId,
          after: expectedRevision)
      }
  }

  func createRepository(name: String, visibility: String) async {
    guard let login = snapshot?.account.login, snapshot?.account.isSignedIn == true,
      DeveloperGitHubSetupValidation.validRepositoryName(name),
      ["private", "public"].contains(visibility) else { return }
    let expectedRevision = revision, operationID = UUID()
    await mutate(DeveloperGitHubSetupRequest.create(operationID: operationID,
      expectedLogin: login, name: name, visibility: visibility, revision: expectedRevision)) {
        DeveloperGitHubSetupAcknowledgement.create($0, operationID: operationID,
          expectedLogin: login, name: name, visibility: visibility, after: expectedRevision)
      }
  }

  func reconcileCreation() async {
    guard let creation = snapshot?.creation, creation.state == "attention" else { return }
    let expectedRevision = revision
    await mutate(DeveloperGitHubSetupRequest.reconcileCreation(operationID: creation.operationId,
      revision: expectedRevision)) {
        DeveloperGitHubSetupAcknowledgement.reconcileCreation($0,
          expected: creation, after: expectedRevision)
      }
  }

  private var revision: UInt64 { snapshot?.revision ?? 0 }

  private func mutate(_ body: [String: Any],
    accepts: @escaping (DeveloperGitHubSetupSnapshot) -> Bool,
    accept: ((DeveloperGitHubSetupSnapshot) -> Void)? = nil,
    allowBusy: Bool = false) async {
    guard !sending, snapshot?.canMutate == true,
      (allowBusy || snapshot?.busy == false) else {
      error = "GitHub setup is busy or changed. Reload before continuing."
      return
    }
    sending = true
    defer { sending = false }
    do {
      let value = try await request(body: body, session: mutationSession)
      guard accepts(value) else { throw responseError() }
      let oldAccount = snapshot?.account
      snapshot = value
      if oldAccount != value.account { repositories = [] }
      accept?(value)
      error = nil
    } catch { self.error = error.localizedDescription }
  }

  private func acceptObserved(_ value: DeveloperGitHubSetupSnapshot) {
    if snapshot?.account != value.account { repositories = [] }
    snapshot = value
  }

  private func request(body: [String: Any]? = nil, session: URLSession) async throws
    -> DeveloperGitHubSetupSnapshot {
    let configuration = try JSONDecoder().decode(DeveloperRunnerConfiguration.self,
      from: Data(contentsOf: URL(fileURLWithPath: configurationPath)))
    guard let base = URL(string: configuration.endpoint), base.scheme == "http",
      ["127.0.0.1", "localhost", "::1"].contains(base.host ?? "") else {
      throw NSError(domain: "Developer GitHub setup", code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Open the developer build with its launcher to connect to Windows."])
    }
    var request = URLRequest(url: base.appendingPathComponent("github"))
    request.setValue("Bearer \(configuration.token)", forHTTPHeaderField: "Authorization")
    if let body {
      request.httpMethod = "POST"
      request.setValue("application/json", forHTTPHeaderField: "Content-Type")
      request.httpBody = try JSONSerialization.data(withJSONObject: body)
    }
    let (data, response) = try await session.data(for: request)
    guard let response = response as? HTTPURLResponse, response.statusCode == 200 else {
      let detail = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
      throw NSError(domain: "Developer GitHub setup", code: 2,
        userInfo: [NSLocalizedDescriptionKey: detail?["error"] as? String
          ?? "Windows could not complete GitHub setup."])
    }
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return try decoder.decode(DeveloperGitHubSetupSnapshot.self, from: data)
  }

  private func responseError() -> NSError {
    NSError(domain: "Developer GitHub setup", code: 3,
      userInfo: [NSLocalizedDescriptionKey:
        "Windows returned an invalid or stale GitHub setup response. Reload before continuing."])
  }
}
