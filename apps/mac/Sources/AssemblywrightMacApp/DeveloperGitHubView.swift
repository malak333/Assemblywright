import Foundation
import SwiftUI

struct DeveloperGitHubConnection: Decodable, Equatable, Identifiable {
  let project: String
  let repositoryUrl: String
  let baseBranch: String
  let automaticMerge: Bool

  var id: String { project }
  var safeRepositoryURL: URL? { DeveloperGitHubURL.repository(repositoryUrl) }
  var isUsable: Bool {
    automaticMerge && safeRepositoryURL != nil
      && DeveloperGitHubPresentation.validBaseBranch(baseBranch)
  }
}

enum DeveloperGitHubURL {
  static func repository(_ value: String) -> URL? {
    guard let components = strictGitHubComponents(value), components.query == nil,
      components.fragment == nil else { return nil }
    let parts = components.percentEncodedPath.split(separator: "/", omittingEmptySubsequences: true)
    let repositoryPart = parts.count == 2 && parts[1].hasSuffix(".git")
      ? String(parts[1].dropLast(4)) : (parts.count == 2 ? String(parts[1]) : "")
    guard parts.count == 2, validPathPart(String(parts[0])), validPathPart(repositoryPart),
      components.percentEncodedPath == "/\(parts[0])/\(parts[1])"
        || components.percentEncodedPath == "/\(parts[0])/\(parts[1])/" else { return nil }
    var normalized = components
    normalized.path = "/\(parts[0])/\(repositoryPart)"
    normalized.percentEncodedQuery = nil
    normalized.fragment = nil
    return normalized.url
  }

  static func pullRequest(_ value: String?, repository repositoryValue: String?) -> URL? {
    guard let value, let components = strictGitHubComponents(value),
      components.query == nil, components.fragment == nil else { return nil }
    let parts = components.percentEncodedPath.split(separator: "/", omittingEmptySubsequences: true)
    guard parts.count == 4, validPathPart(String(parts[0])), validPathPart(String(parts[1])),
      parts[2] == "pull", !parts[3].isEmpty, parts[3].allSatisfy(\.isNumber),
      components.percentEncodedPath == "/\(parts[0])/\(parts[1])/pull/\(parts[3])"
        || components.percentEncodedPath == "/\(parts[0])/\(parts[1])/pull/\(parts[3])/",
      let repositoryURL = repository(repositoryValue ?? "") else { return nil }
    let expected = repositoryURL.path.split(separator: "/", omittingEmptySubsequences: true)
    guard expected.count == 2,
      parts[0].lowercased() == expected[0].lowercased(),
      parts[1].lowercased() == expected[1].lowercased() else { return nil }
    return components.url
  }

  private static func strictGitHubComponents(_ value: String) -> URLComponents? {
    guard value == value.trimmingCharacters(in: .whitespacesAndNewlines),
      let components = URLComponents(string: value), components.scheme == "https",
      components.host?.lowercased() == "github.com", components.user == nil,
      components.password == nil, components.port == nil else { return nil }
    return components
  }

  private static func validPathPart(_ part: String) -> Bool {
    !part.isEmpty && part != "." && part != ".." && !part.contains("%")
      && part.unicodeScalars.allSatisfy { scalar in
        CharacterSet.alphanumerics.contains(scalar) || ["-", "_", "."].contains(Character(scalar))
      }
  }
}

enum DeveloperGitHubRequest {
  static func saveConnection(project: String, repositoryURL: String, baseBranch: String,
    expectedRevision: UInt64) -> [String: Any] {
    ["action": "save_connection", "project": project, "repository_url": repositoryURL,
     "base_branch": baseBranch, "expected_revision": expectedRevision]
  }

  static func disconnect(project: String, expectedRevision: UInt64) -> [String: Any] {
    ["action": "disconnect", "project": project, "expected_revision": expectedRevision]
  }

  static func reconcile(featureID: String, expectedRevision: UInt64,
    expectedCheckpoint: String) -> [String: Any] {
    ["action": "reconcile", "feature_id": featureID, "expected_revision": expectedRevision,
     "expected_checkpoint": expectedCheckpoint]
  }
}

enum DeveloperGitHubAcknowledgement {
  static func saved(_ snapshot: DeveloperRunnerSnapshot, expectedRevision: UInt64,
    project: String, repositoryURL: String, baseBranch: String) -> Bool {
    guard snapshot.revision > expectedRevision,
      let connection = snapshot.githubConnection(for: project) else { return false }
    return connection.project == project && connection.baseBranch == baseBranch
      && connection.automaticMerge
      && DeveloperGitHubURL.repository(connection.repositoryUrl)?.absoluteString
        == DeveloperGitHubURL.repository(repositoryURL)?.absoluteString
  }

  static func disconnected(_ snapshot: DeveloperRunnerSnapshot, expectedRevision: UInt64,
    project: String) -> Bool {
    snapshot.revision > expectedRevision && snapshot.githubConnections != nil
      && snapshot.githubConnection(for: project) == nil
  }

  static func reconciled(_ snapshot: DeveloperRunnerSnapshot, expectedRevision: UInt64,
    featureID: String) -> Bool {
    snapshot.revision > expectedRevision && snapshot.githubPublicationRunning == true
      && snapshot.githubPublicationUnresolved == true
      && snapshot.queue.contains { feature in
        feature.id == featureID && feature.status == "running"
          && feature.checkpoint == "publication_reconciling"
          && feature.publicationStatus == "pending" && feature.canReconcilePublication == false
      }
  }
}

enum DeveloperGitHubPresentation {
  static func validProject(_ value: String) -> Bool {
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return value == trimmed && !trimmed.isEmpty && trimmed.count <= 200
      && !trimmed.unicodeScalars.contains { CharacterSet.controlCharacters.contains($0) }
  }

  static func validBaseBranch(_ value: String) -> Bool {
    !value.isEmpty && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
      && !value.hasPrefix("/") && !value.hasSuffix("/") && !value.hasSuffix(".")
      && !value.contains("..") && !value.contains("@{") && !value.contains("\\")
      && !value.unicodeScalars.contains { CharacterSet.whitespacesAndNewlines.contains($0)
        || CharacterSet.controlCharacters.contains($0) || "~^:?*[".unicodeScalars.contains($0) }
  }

  static func statusLabel(_ feature: DeveloperRunnerFeature) -> String {
    switch feature.publicationStatus {
    case "local_only": return "Local only · not published to GitHub"
    case "pending": return "GitHub publication pending"
    case "running": return stageLabel(feature.publicationStage)
    case "attention": return "GitHub publication needs attention"
    case "succeeded":
      return hasVerifiedMergeEvidence(feature) ? "Merged on GitHub"
        : "GitHub publication result could not be verified"
    case nil where feature.status == "succeeded":
      return "Earlier local result · GitHub publication not recorded"
    case nil: return "GitHub publication status unavailable"
    default: return "GitHub publication status unavailable"
    }
  }

  static func hasVerifiedMergeEvidence(_ feature: DeveloperRunnerFeature) -> Bool {
    feature.status == "succeeded" && feature.checkpoint == "publication_merged"
      && feature.publicationStage == "complete"
      && validCommit(feature.publicationCommitSha) && validCommit(feature.publicationMergedSha)
      && DeveloperGitHubURL.pullRequest(feature.publicationPrUrl,
        repository: feature.publicationRepositoryUrl) != nil
  }

  private static func validCommit(_ value: String?) -> Bool {
    guard let value, [40, 64].contains(value.count) else { return false }
    return value.unicodeScalars.allSatisfy {
      CharacterSet(charactersIn: "0123456789abcdef").contains($0)
    }
  }

  static func stageLabel(_ stage: String?) -> String {
    switch stage {
    case "prepare_candidate": return "Preparing the reviewed candidate"
    case "push_branch": return "Pushing the feature branch"
    case "open_pull_request": return "Opening the pull request"
    case "wait_required_checks": return "Waiting for required GitHub checks"
    case "merge_pull_request": return "Merging after required checks"
    case "verify_remote_base": return "Verifying the merged base branch"
    case "complete": return "Finalizing GitHub publication"
    case "local_only": return "Local only · not published to GitHub"
    default: return "GitHub publication in progress"
    }
  }

  static func startConfirmation(feature: DeveloperRunnerFeature,
    connection: DeveloperGitHubConnection?) -> String {
    let work = "The model on \(feature.modelComputer) prepares \(feature.project). Windows applies changes, runs validation, and sends the feature request and bounded project code to OpenAI/Codex for review."
    guard let connection else {
      return work + " Codex findings can trigger up to three local repairs. This project is not connected to GitHub, so a successful result stays local and is labeled unpublished. Auto-run continues only after validation and reviewer approval."
    }
    guard connection.isUsable else {
      return work + " The saved GitHub destination is invalid. Update the connection before starting. Codex findings can trigger up to three local repairs."
    }
    return work + " Codex findings can trigger up to three local repairs. After approval, Windows publishes the exact reviewed commit to \(connection.repositoryUrl) on its own feature branch, opens a pull request into \(connection.baseBranch), and merges automatically only after required GitHub checks pass. Auto-run continues after Windows verifies the merged remote base."
  }
}

struct DeveloperGitHubFeatureStatus: View {
  let feature: DeveloperRunnerFeature
  @ObservedObject var runner: DeveloperRunnerModel

  private var safePR: URL? {
    DeveloperGitHubURL.pullRequest(feature.publicationPrUrl,
      repository: feature.publicationRepositoryUrl)
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 4) {
      HStack(spacing: 7) {
        if feature.publicationStatus == "running" { ProgressView().controlSize(.small) }
        Text(DeveloperGitHubPresentation.statusLabel(feature))
          .foregroundStyle(feature.publicationStatus == "attention" ? .orange
            : DeveloperGitHubPresentation.hasVerifiedMergeEvidence(feature) ? .green : .secondary)
        if let safePR { Link("Open pull request", destination: safePR) }
        if feature.publicationStatus == "attention" && feature.canReconcilePublication == true {
          Button("Reconcile…") {
            guard let revision = runner.snapshot?.revision else { return }
            Task { try? await runner.reconcilePublication(feature, expectedRevision: revision) }
          }
          .disabled(runner.sending || runner.snapshot?.githubPublicationUnresolved != true)
          .help("Inspect the existing branch, pull request, checks, and merge before resuming.")
          .accessibilityIdentifier("developer-reconcile-publication-\(feature.id)")
        }
      }
      if feature.publicationPrUrl != nil && safePR == nil {
        Text("The reported pull request link could not be verified.").foregroundStyle(.orange)
      }
      if let message = feature.publicationMessage, !message.isEmpty {
        Text(message).textSelection(.enabled)
      }
      if let branch = feature.publicationBranch, !branch.isEmpty {
        Text("Branch: \(branch)").monospaced().textSelection(.enabled)
      }
      if let sha = feature.publicationMergedSha ?? feature.publicationCommitSha, !sha.isEmpty {
        Text("Commit: \(sha)").monospaced().textSelection(.enabled)
      }
    }.font(.caption)
  }
}

struct DeveloperGitHubView: View {
  @ObservedObject var runner: DeveloperRunnerModel
  let projects: [String]
  @StateObject private var setup: DeveloperGitHubSetupModel
  @Environment(\.dismiss) private var dismiss
  @State private var selectedProject = ""
  @State private var repositoryURL = ""
  @State private var baseBranch = "main"
  @State private var localError: String?
  @State private var repositoryName = ""
  @State private var repositoryVisibility = "private"
  @State private var confirmingCreation = false

  init(runner: DeveloperRunnerModel, projects: [String], configurationPath: String) {
    self.runner = runner
    self.projects = projects
    _setup = StateObject(wrappedValue: DeveloperGitHubSetupModel(configurationPath: configurationPath))
  }

  private var allProjects: [String] {
    Array(Set(projects + (runner.snapshot?.githubConnections?.map(\.project) ?? []))).sorted()
  }
  private var project: String { allProjects.isEmpty ? selectedProject : selectedProject }
  private var connection: DeveloperGitHubConnection? {
    runner.snapshot?.githubConnection(for: project)
  }
  private var canEdit: Bool {
    runner.snapshot?.githubPublicationSupported == true
      && runner.snapshot?.canManageGithubConnections == true
      && runner.snapshot?.githubSetupBusy != true && runner.snapshot?.githubSetupUnresolved != true
      && !runner.sending
  }
  private var inputIsValid: Bool {
    DeveloperGitHubPresentation.validProject(project)
      && DeveloperGitHubURL.repository(repositoryURL) != nil
      && DeveloperGitHubPresentation.validBaseBranch(baseBranch)
  }

  var body: some View {
    ScrollView {
    VStack(alignment: .leading, spacing: 16) {
      HStack {
        Text("GitHub publication").font(.title2.bold())
        Spacer()
        Button("Done") { dismiss() }
      }
      Text("Connect an existing repository. Future successful features use their own branch, open a pull request, and merge automatically after required checks pass.")
        .foregroundStyle(.secondary)
      accountSection
      if setup.snapshot?.account.isSignedIn == true { repositoriesSection }
      if runner.snapshot?.githubPublicationSupported != true {
        Label("Update the Windows developer runner to configure automatic GitHub publication.",
          systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
      } else if runner.snapshot?.githubPublicationUnresolved == true {
        Label("A publication needs reconciliation before connections or new work can change.",
          systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
      }
      if allProjects.isEmpty {
        TextField("Project name", text: $selectedProject)
          .accessibilityIdentifier("developer-github-project")
      } else {
        Picker("Project", selection: $selectedProject) {
          ForEach(allProjects, id: \.self) { Text($0).tag($0) }
        }.accessibilityIdentifier("developer-github-project")
      }
      if connection == nil {
        Text("Not connected · successful features stay local and unpublished")
          .font(.caption).foregroundStyle(.secondary)
      } else if connection?.isUsable == true {
        Text("Connected · automatic pull request and merge after required checks")
          .font(.caption).foregroundStyle(.green)
      } else {
        Text("The saved connection is invalid and must be updated before starting work.")
          .font(.caption).foregroundStyle(.orange)
      }
      TextField("Repository URL", text: $repositoryURL,
        prompt: Text("https://github.com/owner/repository"))
        .textContentType(.URL)
        .accessibilityIdentifier("developer-github-repository")
      TextField("Base branch", text: $baseBranch, prompt: Text("main"))
        .accessibilityIdentifier("developer-github-base-branch")
      Label("Automatic merge after required checks", systemImage: "checkmark.shield")
        .foregroundStyle(.secondary)
      if !repositoryURL.isEmpty && DeveloperGitHubURL.repository(repositoryURL) == nil {
        Text("Enter an HTTPS github.com repository URL with only an owner and repository name.")
          .font(.caption).foregroundStyle(.red)
      }
      if !baseBranch.isEmpty && !DeveloperGitHubPresentation.validBaseBranch(baseBranch) {
        Text("Enter a valid Git branch name.").font(.caption).foregroundStyle(.red)
      }
      if let localError { Text(localError).font(.caption).foregroundStyle(.red).textSelection(.enabled) }
      if let error = setup.error {
        Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled)
          .fixedSize(horizontal: false, vertical: true)
      }
      HStack {
        Button(connection == nil ? "Save connection" : "Update connection") { save() }
          .buttonStyle(.borderedProminent).disabled(!canEdit || !inputIsValid)
          .accessibilityIdentifier("developer-github-save")
        if connection != nil {
          Button("Disconnect", role: .destructive) { disconnect() }
            .disabled(!canEdit).accessibilityIdentifier("developer-github-disconnect")
        }
        if runner.sending { ProgressView().controlSize(.small) }
      }
      if setup.snapshot?.account.isSignedIn == true { creationSection }
    }
    .padding(24)
    }
    .frame(width: 680, height: 760)
    .onAppear { selectInitialProject() }
    .task { await setup.observe() }
    .onChange(of: selectedProject) { _, _ in loadConnection() }
    .onChange(of: runner.snapshot?.revision) { _, _ in
      if !runner.sending { loadConnection() }
    }
    .onChange(of: runner.sending) { _, sending in
      if !sending { loadConnection() }
    }
    .confirmationDialog("Create this GitHub repository?", isPresented: $confirmingCreation,
      titleVisibility: .visible) {
      Button("Create \(repositoryVisibility) repository") {
        let name = repositoryName, visibility = repositoryVisibility
        Task { await setup.createRepository(name: name, visibility: visibility) }
      }
      .disabled(!canCreateRepository)
      Button("Cancel", role: .cancel) {}
    } message: {
      Text("Account: \(setup.snapshot?.account.login ?? "Unavailable")\nRepository: \(repositoryName)\nVisibility: \(repositoryVisibility.capitalized)\n\nThis initializes a README and does not upload project files or connect this project.")
    }
  }

  @ViewBuilder private var accountSection: some View {
    GroupBox("GitHub account") {
      VStack(alignment: .leading, spacing: 8) {
        let account = setup.snapshot?.account
        Label(accountLabel(account), systemImage: account?.isSignedIn == true
          ? "person.crop.circle.badge.checkmark" : "person.crop.circle.badge.exclamationmark")
        if let message = account?.message, !message.isEmpty {
          Text(message).font(.caption).textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
        }
        HStack {
          Button("Refresh account") { Task { await setup.refreshAccount() } }
            .disabled(setup.sending || setup.snapshot?.canMutate != true || setup.snapshot?.busy == true)
          if account?.state == "signed_out" {
            Button("Sign in to GitHub") { Task { await setup.beginSignIn() } }
              .buttonStyle(.borderedProminent)
              .disabled(setup.sending || setup.snapshot?.canMutate != true || setup.snapshot?.busy == true)
          }
        }
        if let signIn = setup.snapshot?.signIn {
          Divider()
          Text(["starting", "waiting"].contains(signIn.state) ? signIn.message
            : "Last sign-in attempt: \(signIn.message)")
            .font(.caption).textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
          if signIn.hasSafeChallenge, let code = signIn.userCode,
            let deviceURL = DeveloperGitHubSetupValidation.deviceURL(signIn.verificationUrl) {
            Text("One-time code: \(code)").font(.headline.monospaced()).textSelection(.enabled)
            Link("Open GitHub device authorization", destination: deviceURL)
          }
          if ["starting", "waiting"].contains(signIn.state) {
            Button("Cancel sign-in", role: .destructive) {
              Task { await setup.cancelSignIn() }
            }.disabled(setup.sending || setup.snapshot?.canMutate != true)
          } else if signIn.state == "attention" {
            Button("Reconcile sign-in") { Task { await setup.reconcileSignIn() } }
              .disabled(setup.sending || setup.snapshot?.canMutate != true)
          }
        }
      }.frame(maxWidth: .infinity, alignment: .leading).padding(6)
    }
  }

  @ViewBuilder private var repositoriesSection: some View {
    GroupBox("Your repositories") {
      VStack(alignment: .leading, spacing: 8) {
        HStack {
          Button("Load repositories") { Task { await setup.listRepositories(page: 1) } }
            .disabled(setup.sending || setup.snapshot?.canMutate != true || setup.snapshot?.busy == true)
          if setup.snapshot?.hasMore == true && !setup.repositories.isEmpty {
            Button("Load more") {
              Task { await setup.listRepositories(page: (setup.snapshot?.repositoryPage ?? 0) + 1) }
            }.disabled(setup.sending || setup.snapshot?.canMutate != true || setup.snapshot?.busy == true)
          }
          if setup.sending { ProgressView().controlSize(.small) }
        }
        if setup.repositories.isEmpty {
          Text("Load the repositories available to the signed-in account.")
            .font(.caption).foregroundStyle(.secondary)
        }
        ForEach(setup.repositories) { repository in
          HStack {
            VStack(alignment: .leading) {
              if let safeURL = DeveloperGitHubURL.repository(repository.url) {
                Link(repository.nameWithOwner, destination: safeURL)
              } else {
                Text("Invalid repository response").foregroundStyle(.orange)
              }
              if repository.defaultBranch.isEmpty {
                Text("\(repository.visibility.capitalized) · no default branch")
                  .font(.caption).foregroundStyle(.orange)
                Text("Initialize a README and base branch on GitHub, then configure required checks before connecting.")
                  .font(.caption).foregroundStyle(.secondary)
              } else {
                Text("\(repository.visibility.capitalized) · default branch \(repository.defaultBranch)")
                  .font(.caption).foregroundStyle(.secondary)
              }
            }
            Spacer()
            Button("Select") {
              repositoryURL = repository.url
              baseBranch = repository.defaultBranch
            }.disabled(!repository.canSelect)
          }
          .accessibilityIdentifier("developer-github-repository-\(repository.id)")
          Divider()
        }
      }.frame(maxWidth: .infinity, alignment: .leading).padding(6)
    }
  }

  @ViewBuilder private var creationSection: some View {
    GroupBox("Create a repository") {
      VStack(alignment: .leading, spacing: 8) {
        Text("Creates a new repository initialized with a README. Save the project connection afterward.")
          .font(.caption).foregroundStyle(.secondary)
        TextField("Repository name", text: $repositoryName)
          .accessibilityIdentifier("developer-github-create-name")
        Picker("Visibility", selection: $repositoryVisibility) {
          Text("Private").tag("private")
          Text("Public").tag("public")
        }.pickerStyle(.segmented).accessibilityIdentifier("developer-github-create-visibility")
        Button("Review repository creation…") { confirmingCreation = true }
          .disabled(!canCreateRepository)
          .accessibilityIdentifier("developer-github-create")
        if let creation = setup.snapshot?.creation {
          Text(creation.message).font(.caption).textSelection(.enabled)
            .fixedSize(horizontal: false, vertical: true)
          if creation.state == "succeeded", let url = creation.repositoryUrl,
            let safeURL = DeveloperGitHubURL.repository(url) {
            Link("Open created repository", destination: safeURL)
            Text("Repository created. Select it above or enter its URL, then save the connection. Publication readiness is checked separately.")
              .font(.caption).foregroundStyle(.green)
          } else if creation.state == "existing" {
            Text("An existing repository was found. Load repositories and select it explicitly if it is the intended destination.")
              .font(.caption).foregroundStyle(.orange)
          } else if creation.state == "absent" {
            Text("GitHub currently reports no repository at this address. You may review a new creation request.")
              .font(.caption).foregroundStyle(.secondary)
          }
          if creation.state == "attention" {
            Button("Reconcile creation") { Task { await setup.reconcileCreation() } }
              .disabled(setup.sending || setup.snapshot?.canMutate != true)
          }
        }
      }.frame(maxWidth: .infinity, alignment: .leading).padding(6)
    }
  }

  private var canCreateRepository: Bool {
    setup.snapshot?.account.isSignedIn == true && setup.snapshot?.canMutate == true
      && setup.snapshot?.busy == false && !setup.sending
      && DeveloperGitHubSetupValidation.validRepositoryName(repositoryName)
      && ["private", "public"].contains(repositoryVisibility)
      && setup.snapshot?.creation?.state != "attention"
  }

  private func accountLabel(_ account: DeveloperGitHubAccount?) -> String {
    switch account?.state {
    case "signed_in": return "Signed in as \(account?.login ?? "Unavailable")"
    case "signed_out": return "Not signed in"
    case "unavailable": return "GitHub account unavailable"
    default: return "Checking GitHub account"
    }
  }

  private func selectInitialProject() {
    if selectedProject.isEmpty, let first = allProjects.first { selectedProject = first }
    loadConnection()
  }

  private func loadConnection() {
    guard let connection else {
      repositoryURL = ""
      baseBranch = "main"
      return
    }
    repositoryURL = connection.repositoryUrl
    baseBranch = connection.baseBranch
  }

  private func save() {
    guard let revision = runner.snapshot?.revision, inputIsValid else { return }
    localError = nil
    Task {
      do {
        try await runner.saveGitHubConnection(project: project, repositoryURL: repositoryURL,
          baseBranch: baseBranch, expectedRevision: revision)
      } catch { localError = error.localizedDescription }
    }
  }

  private func disconnect() {
    guard let revision = runner.snapshot?.revision, !project.isEmpty else { return }
    localError = nil
    Task {
      do { try await runner.disconnectGitHub(project: project, expectedRevision: revision) }
      catch { localError = error.localizedDescription }
    }
  }
}
