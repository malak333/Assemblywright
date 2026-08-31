import AssemblywrightMacCore
import Foundation
import SwiftUI

enum AssemblyLineProjectVisibility: String, CaseIterable, Identifiable {
  case `public` = "Public"
  case `private` = "Private"

  var id: Self { self }
}

struct CanonicalGitHubRepositoryURL: Equatable {
  let value: String

  init?(_ candidate: String) {
    guard candidate.utf8.count <= 256,
      candidate.unicodeScalars.allSatisfy({ $0.value >= 0x20 && $0.value != 0x7f }),
      !candidate.contains(where: { "?#%\\".contains($0) })
    else { return nil }

    let prefix = "https://"
    guard candidate.lowercased().hasPrefix(prefix) else { return nil }
    var remainder = String(candidate.dropFirst(prefix.count))
    if remainder.hasSuffix("/") {
      remainder.removeLast()
    }
    let parts = remainder.split(separator: "/", omittingEmptySubsequences: false)
    guard parts.count == 3,
      parts[0].lowercased() == "github.com",
      !parts[0].contains("@"),
      !parts[0].contains(":"),
      Self.validName(String(parts[1]), maximum: 39, allowRepositoryCharacters: false)
    else { return nil }

    var repository = String(parts[2])
    if repository.hasSuffix(".git") {
      repository.removeLast(4)
    }
    guard Self.validName(repository, maximum: 100, allowRepositoryCharacters: true) else {
      return nil
    }
    value = "https://github.com/\(parts[1].lowercased())/\(repository.lowercased())"
  }

  private static func validName(
    _ value: String,
    maximum: Int,
    allowRepositoryCharacters: Bool
  ) -> Bool {
    guard !value.isEmpty,
      value.utf8.count <= maximum,
      !value.hasPrefix("-"), !value.hasPrefix("."), !value.hasPrefix("_"),
      !value.hasSuffix("-"), !value.hasSuffix("."), !value.hasSuffix("_"),
      !value.contains("--")
    else { return false }
    return value.utf8.allSatisfy { byte in
      (byte >= 0x30 && byte <= 0x39)
        || (byte >= 0x41 && byte <= 0x5a)
        || (byte >= 0x61 && byte <= 0x7a)
        || byte == 0x2d
        || (allowRepositoryCharacters && (byte == 0x5f || byte == 0x2e))
    }
  }
}

struct AssemblyLineQueuedFeaturePresentation: Identifiable, Equatable {
  let id: UUID
  let title: String
  let repositoryURL: CanonicalGitHubRepositoryURL
}

struct AssemblyLineOwnerApprovalConfirmationPresentation: Equatable {
  let repositoryURL: String
  let visibility: String
  let specificationID: String
  let specificationSHA256: String
  let ownerApprovalSHA256: String

  init(_ preview: AssemblywrightMacOwnerApprovalPreview) {
    repositoryURL = preview.repositoryURL
    visibility = preview.visibility?.rawValue.capitalized ?? "Existing repository"
    specificationID = preview.specificationID.uuidString.lowercased()
    specificationSHA256 = Self.hex(preview.specificationSHA256)
    ownerApprovalSHA256 = Self.hex(preview.ownerApprovalSHA256)
  }

  var summary: String {
    "Repository: \(repositoryURL)\nVisibility: \(visibility)\nSpecification ID: "
      + "\(specificationID)\nSpecification SHA-256: \(specificationSHA256)\n"
      + "Owner approval SHA-256: \(ownerApprovalSHA256)"
  }

  private static func hex(_ bytes: [UInt8]) -> String {
    bytes.map { String(format: "%02x", $0) }.joined()
  }
}

struct AssemblyLineFrozenReviewInputBinding: Equatable {
  let repositoryURL: String
  let visibility: AssemblyLineProjectVisibility?
  let idea: String
  let orchestratorCatalogSHA256: [UInt8]

  func matches(
    repositoryURL: String,
    visibility: AssemblyLineProjectVisibility?,
    idea: String,
    orchestratorCatalogSHA256: [UInt8]
  ) -> Bool {
    self.repositoryURL == repositoryURL
      && self.visibility == visibility
      && self.idea == idea
      && self.orchestratorCatalogSHA256 == orchestratorCatalogSHA256
  }
}

struct AssemblyLineOwnerPresentation: Equatable {
  static let newProjectTitle = "New Project"
  static let newFeatureTitle = "New Feature"
  static let assemblyLineTitle = "Assembly Line"
  static let brainstormProjectLabel = "Brainstorm Project"
  static let brainstormFeatureLabel = "Brainstorm Feature"
  static let startLabel = "Start"
  static let stopLabel = "Stop"
  static let emergencyPauseLabel = "Emergency Pause"
  static let recoveryLabel = "Retry Exact Pending Action"
  static let executionUnavailableReason =
    "Start is unavailable until the executors and protected brokers are installed"

  var projectVisibility: AssemblyLineProjectVisibility = .public
  var autoRun = true
  var queuedFeatures: [AssemblyLineQueuedFeaturePresentation] = []
  var autoRunControlEnabled = false
  var pendingPlanningAction: AssemblywrightMacAssemblyLinePlanningAction?
  var brainstormingAvailable = false
  var githubCreationAvailable = false
  var planningReason = "Connect to the Windows master"
  var githubCreationReason = "Connect to the Windows master"
  var orchestratorLabel = "Unavailable"

  var hasQueuedFeature: Bool { !queuedFeatures.isEmpty }
  var canStart: Bool { false }
  var canStop: Bool { false }
  var canEmergencyPause: Bool { false }
  var recoveryRequired: Bool { pendingPlanningAction != nil }

  var recoveryStatus: String? {
    pendingPlanningAction.map {
      "The previous \(Self.actionName($0)) result is unknown. Retry sends the exact saved request."
    }
  }

  var startReason: String {
    hasQueuedFeature ? Self.executionUnavailableReason : "Add at least one feature to start"
  }

  static func observed(
    status: AssemblywrightDeveloperBridgeAppStatus,
    ownerActionInProgress: Bool,
    projectVisibility: AssemblyLineProjectVisibility,
    pendingPlanningAction: AssemblywrightMacAssemblyLinePlanningAction? = nil
  ) -> Self {
    guard status.phase == .connected, let projection = status.assemblyLine else {
      return Self(
        projectVisibility: projectVisibility,
        pendingPlanningAction: pendingPlanningAction
      )
    }
    let repositoryURLs = Dictionary(
      uniqueKeysWithValues: projection.repositories.compactMap { repository in
        CanonicalGitHubRepositoryURL(repository.repository.gitURL.url).map {
          (repository.repository.repositoryID, $0)
        }
      }
    )
    let queued = projection.queue.compactMap { feature -> AssemblyLineQueuedFeaturePresentation? in
      guard let url = repositoryURLs[feature.repositoryID] else { return nil }
      return AssemblyLineQueuedFeaturePresentation(
        id: feature.featureID,
        title: "Feature \(feature.featureID.uuidString.lowercased().prefix(8))",
        repositoryURL: url
      )
    }
    return Self(
      projectVisibility: projectVisibility,
      autoRun: projection.assemblyLine.autoRun,
      queuedFeatures: queued,
      autoRunControlEnabled: !ownerActionInProgress && pendingPlanningAction == nil
        && !projection.emergencyPaused,
      pendingPlanningAction: pendingPlanningAction,
      brainstormingAvailable: !ownerActionInProgress && pendingPlanningAction == nil
        && !projection.emergencyPaused
        && projection.availability.brainstormingProvider.status == .available,
      githubCreationAvailable: !ownerActionInProgress && pendingPlanningAction == nil
        && !projection.emergencyPaused
        && projection.availability.githubCreation.status == .available,
      planningReason: availabilityReason(
        projection.availability.brainstormingProvider,
        paused: projection.emergencyPaused,
        label: "Brainstorming"
      ),
      githubCreationReason: availabilityReason(
        projection.availability.githubCreation,
        paused: projection.emergencyPaused,
        label: "Repository creation"
      ),
      orchestratorLabel: projection.orchestratorCatalog.profiles.first(where: {
        $0.providerID == "openai.codex" && $0.modelID == "gpt-5.6-sol"
      }).map { "\($0.providerID) / \($0.modelID)" } ?? "Unavailable"
    )
  }

  private static func availabilityReason(
    _ component: AssemblywrightMacRuntimeComponentAvailability,
    paused: Bool,
    label: String
  ) -> String {
    if paused { return "Emergency Pause is active" }
    if component.status == .available { return "\(label) is available" }
    let reason: String
    switch component.unavailableReason {
    case .notConfigured: reason = "not configured"
    case .notAuthenticated: reason = "not authenticated"
    case .disconnected: reason = "disconnected"
    case .unhealthy: reason = "unhealthy"
    case .emergencyPaused: reason = "Emergency Pause is active"
    case .identityDrift: reason = "identity drift"
    case .evidenceRequired: reason = "evidence required"
    case nil: reason = "unavailable"
    }
    return "\(label) unavailable: \(reason)"
  }

  private static func actionName(_ action: AssemblywrightMacAssemblyLinePlanningAction) -> String {
    switch action {
    case .projectDraft: "project brainstorm"
    case .featureDraft: "feature brainstorm"
    case .frozenSpecification: "specification freeze"
    case .projectBrainstorm: "project brainstorm"
    case .featureBrainstorm: "feature brainstorm"
    case .projectApproval: "project approval"
    case .featureApproval: "feature approval"
    case .repositoryCreation: "repository creation/reconciliation"
    case .autoRun: "auto-run update"
    }
  }
}

struct AssemblyLineDeveloperDiagnosticsPresentation: Equatable {
  let bridge: String
  let master: String?
  let connectionEpoch: String?
  let statusCode: String?
  let queue: String?

  init(status: AssemblywrightDeveloperBridgeAppStatus) {
    bridge = DeveloperBridgeStatusPresentation(status: status).phaseLabel
    master = status.masterEndpoint
    connectionEpoch = status.connectionEpoch.map(String.init)
    statusCode = status.errorCode
    queue = status.featureConveyor.map {
      FeatureConveyorStatusPresentation(status: $0).queueLabel
    }
  }
}

struct AssemblyLineOwnerView: View {
  private enum CloudBrainstormTarget {
    case project
    case feature
  }

  @ObservedObject var developerBridge: AssemblywrightDeveloperBridgeProcessLifecycle
  @State private var projectURL = ""
  @State private var projectIdea = ""
  @State private var featureURL = ""
  @State private var featureIdea = ""
  @State private var projectVisibility: AssemblyLineProjectVisibility = .public
  @State private var showsDeveloperDetails = false
  @State private var projectReview: AssemblywrightMacFrozenBrainstormingSpecification?
  @State private var featureReview: AssemblywrightMacFrozenBrainstormingSpecification?
  @State private var projectReviewBinding: AssemblyLineFrozenReviewInputBinding?
  @State private var featureReviewBinding: AssemblyLineFrozenReviewInputBinding?
  @State private var projectReviewGeneration: UInt64 = 0
  @State private var featureReviewGeneration: UInt64 = 0
  @State private var confirmsProjectApproval = false
  @State private var confirmsFeatureApproval = false
  @State private var projectApprovalPreview: AssemblywrightMacOwnerApprovalPreview?
  @State private var featureApprovalPreview: AssemblywrightMacOwnerApprovalPreview?
  @State private var cloudBrainstormTarget: CloudBrainstormTarget?

  private var presentation: AssemblyLineOwnerPresentation {
    .observed(
      status: developerBridge.status,
      ownerActionInProgress: developerBridge.ownerActionInProgress,
      projectVisibility: projectVisibility,
      pendingPlanningAction: developerBridge.pendingAssemblyLinePlanningAction
    )
  }

  private static let publicCloudDisclosure =
    "Public information only. Your idea and the selected public planning metadata will be sent "
    + "to openai.codex. Do not include private, restricted, secret, credential, or path data."

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 18) {
        Text("Build with Assemblywright")
          .font(.largeTitle.bold())

        GroupBox(AssemblyLineOwnerPresentation.newProjectTitle) {
          VStack(alignment: .leading, spacing: 10) {
            TextField("https://github.com/owner/repository", text: $projectURL)
              .textFieldStyle(.roundedBorder)
            if let message = repositoryURLMessage(projectURL) {
              Text(message)
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            Picker("Visibility", selection: $projectVisibility) {
              ForEach(AssemblyLineProjectVisibility.allCases) { visibility in
                Text(visibility.rawValue).tag(visibility)
              }
            }
            .pickerStyle(.segmented)
            LabeledContent("Orchestrator", value: presentation.orchestratorLabel)
            TextField("What do you want to build?", text: $projectIdea, axis: .vertical)
              .lineLimit(2...5)
              .textFieldStyle(.roundedBorder)
            Text(Self.publicCloudDisclosure)
              .font(.caption)
              .foregroundStyle(.secondary)
            Button(AssemblyLineOwnerPresentation.brainstormProjectLabel) {
              cloudBrainstormTarget = .project
            }
            .disabled(!canBrainstormProject)
            .accessibilityIdentifier("assembly-line-brainstorm-project")
            Text(presentation.planningReason)
              .font(.caption)
              .foregroundStyle(.secondary)
            if let projectReview {
              AssemblyLineFrozenSpecificationReview(specification: projectReview)
              Button("Approve Project and Create Repository") {
                prepareProjectApproval()
              }
              .disabled(!presentation.brainstormingAvailable
                || !presentation.githubCreationAvailable
                || developerBridge.ownerActionInProgress
                || developerBridge.pendingAssemblyLinePlanningAction != nil)
              .accessibilityIdentifier("assembly-line-approve-project")
            }
          }
          .padding(.top, 6)
        }

        GroupBox(AssemblyLineOwnerPresentation.newFeatureTitle) {
          VStack(alignment: .leading, spacing: 10) {
            TextField("Repository GitHub URL", text: $featureURL)
              .textFieldStyle(.roundedBorder)
            if let message = repositoryURLMessage(featureURL) {
              Text(message)
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            TextField("What should this feature do?", text: $featureIdea, axis: .vertical)
              .lineLimit(2...5)
              .textFieldStyle(.roundedBorder)
            Text(Self.publicCloudDisclosure)
              .font(.caption)
              .foregroundStyle(.secondary)
            LabeledContent("Orchestrator", value: presentation.orchestratorLabel)
            Button(AssemblyLineOwnerPresentation.brainstormFeatureLabel) {
              cloudBrainstormTarget = .feature
            }
            .disabled(!canBrainstormFeature)
            .accessibilityIdentifier("assembly-line-brainstorm-feature")
            Text(presentation.planningReason)
              .font(.caption)
              .foregroundStyle(.secondary)
            if let featureReview {
              AssemblyLineFrozenSpecificationReview(specification: featureReview)
              Button("Approve Feature and Add to Queue") {
                prepareFeatureApproval()
              }
              .disabled(!presentation.brainstormingAvailable
                || developerBridge.ownerActionInProgress
                || developerBridge.pendingAssemblyLinePlanningAction != nil)
              .accessibilityIdentifier("assembly-line-approve-feature")
            }
          }
          .padding(.top, 6)
        }

        GroupBox(AssemblyLineOwnerPresentation.assemblyLineTitle) {
          VStack(alignment: .leading, spacing: 10) {
            Toggle("Auto-run", isOn: autoRunBinding)
              .disabled(!presentation.autoRunControlEnabled)
            if presentation.queuedFeatures.isEmpty {
              Text("No features queued")
                .foregroundStyle(.secondary)
            } else {
              ForEach(Array(presentation.queuedFeatures.enumerated()), id: \.element.id) {
                index, feature in
                LabeledContent("\(index + 1). \(feature.title)", value: feature.repositoryURL.value)
              }
            }
            HStack {
              Button(AssemblyLineOwnerPresentation.startLabel) {}
                .disabled(!presentation.canStart)
              Button(AssemblyLineOwnerPresentation.stopLabel) {}
                .disabled(!presentation.canStop)
              Button(AssemblyLineOwnerPresentation.emergencyPauseLabel, role: .destructive) {}
                .disabled(!presentation.canEmergencyPause)
            }
            Text(presentation.startReason)
              .font(.caption)
              .foregroundStyle(.secondary)
            if let recoveryStatus = presentation.recoveryStatus {
              Text(recoveryStatus)
                .font(.caption)
                .foregroundStyle(.orange)
              Button(AssemblyLineOwnerPresentation.recoveryLabel) {
                Task {
                  await developerBridge.reconcilePendingAssemblyLinePlanningMutation()
                }
              }
              .disabled(developerBridge.ownerActionInProgress)
              .accessibilityIdentifier("assembly-line-reconcile-pending")
            }
            if let errorCode = developerBridge.ownerActionErrorCode {
              Text("Action failed: \(errorCode)")
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            ForEach(pendingRepositoryCreations, id: \.repository.repositoryID) { repository in
              Button("Create/Reconcile \(repository.repository.gitURL.url)") {
                createOrReconcile(repository.repository.repositoryID)
              }
              .disabled(!presentation.githubCreationAvailable)
              .accessibilityIdentifier("assembly-line-create-reconcile-repository")
            }
            if !pendingRepositoryCreations.isEmpty {
              Text(presentation.githubCreationReason)
                .font(.caption)
                .foregroundStyle(.secondary)
            }
          }
          .padding(.top, 6)
        }

        DisclosureGroup("Developer details", isExpanded: $showsDeveloperDetails) {
          AssemblyLineDeveloperDiagnosticsView(
            presentation: AssemblyLineDeveloperDiagnosticsPresentation(
              status: developerBridge.status)
          )
        }
      }
      .padding(24)
    }
    .confirmationDialog(
      "Approve this frozen project specification?",
      isPresented: $confirmsProjectApproval,
      titleVisibility: .visible
    ) {
      Button("Approve and Create Repository") { approveProject() }
      Button("Cancel", role: .cancel) { projectApprovalPreview = nil }
    } message: {
      Text(projectApprovalPreview.map {
        AssemblyLineOwnerApprovalConfirmationPresentation($0).summary
      } ?? "No frozen project approval is available.")
    }
    .confirmationDialog(
      "Approve this frozen feature specification?",
      isPresented: $confirmsFeatureApproval,
      titleVisibility: .visible
    ) {
      Button("Approve and Add to Queue") { approveFeature() }
      Button("Cancel", role: .cancel) { featureApprovalPreview = nil }
    } message: {
      Text(featureApprovalPreview.map {
        AssemblyLineOwnerApprovalConfirmationPresentation($0).summary
      } ?? "No frozen feature approval is available.")
    }
    .confirmationDialog(
      "Confirm Public cloud brainstorming disclosure",
      isPresented: Binding(
        get: { cloudBrainstormTarget != nil },
        set: { if !$0 { cloudBrainstormTarget = nil } }
      ),
      titleVisibility: .visible
    ) {
      Button("Send Public Idea to openai.codex") {
        let target = cloudBrainstormTarget
        cloudBrainstormTarget = nil
        switch target {
        case .project: brainstormProject()
        case .feature: brainstormFeature()
        case nil: break
        }
      }
      Button("Cancel", role: .cancel) { cloudBrainstormTarget = nil }
    } message: {
      Text(Self.publicCloudDisclosure)
    }
    .onChange(of: projectURL) { _, _ in invalidateProjectReview() }
    .onChange(of: projectIdea) { _, _ in invalidateProjectReview() }
    .onChange(of: projectVisibility) { _, _ in invalidateProjectReview() }
    .onChange(of: featureURL) { _, _ in invalidateFeatureReview() }
    .onChange(of: featureIdea) { _, _ in invalidateFeatureReview() }
    .onChange(of: developerBridge.status.assemblyLine?.orchestratorCatalog.catalogSHA256) {
      _, _ in
      invalidateProjectReview()
      invalidateFeatureReview()
    }
  }

  private var canBrainstormProject: Bool {
    presentation.brainstormingAvailable
      && CanonicalGitHubRepositoryURL(projectURL) != nil
      && validIdea(projectIdea)
  }

  private var canBrainstormFeature: Bool {
    guard presentation.brainstormingAvailable,
      let canonical = CanonicalGitHubRepositoryURL(featureURL), validIdea(featureIdea),
      let projection = developerBridge.status.assemblyLine
    else { return false }
    return projection.repositories.contains {
      $0.repository.gitURL.url == canonical.value && $0.lifecycle == .created
    }
  }

  private var pendingRepositoryCreations: [AssemblywrightMacRepositoryCreationProjection] {
    guard let projection = developerBridge.status.assemblyLine else { return [] }
    return projection.repositories.filter {
      [.creationPending, .reconciling, .reconciliationRequired].contains($0.lifecycle)
    }
  }

  private var autoRunBinding: Binding<Bool> {
    Binding(
      get: { presentation.autoRun },
      set: { enabled in
        guard presentation.autoRunControlEnabled,
          let projection = developerBridge.status.assemblyLine,
          enabled != projection.assemblyLine.autoRun,
          let request = try? AssemblywrightMacAssemblyLineOwnerControl.autoRunRequest(
            from: projection,
            enabled: enabled
          )
        else { return }
        Task {
          await developerBridge.performAssemblyLinePlanningAction(
            .autoRun,
            requestData: request
          )
        }
      }
    )
  }

  private func brainstormProject() {
    guard let projection = developerBridge.status.assemblyLine,
      let canonical = CanonicalGitHubRepositoryURL(projectURL), validIdea(projectIdea),
      let request = try? AssemblywrightMacAssemblyLineOwnerControl.projectBrainstormRequest(
        from: projection,
        repositoryURL: canonical.value,
        visibility: projectVisibility == .public ? .public : .private,
        idea: projectIdea,
        informationClassification: .public,
        ownerConfirmedCloudDisclosure: true
      )
    else { return }
    let binding = AssemblyLineFrozenReviewInputBinding(
      repositoryURL: canonical.value,
      visibility: projectVisibility,
      idea: projectIdea,
      orchestratorCatalogSHA256: projection.orchestratorCatalog.catalogSHA256
    )
    let generation = projectReviewGeneration
    Task {
      guard let response = await developerBridge.performAssemblyLinePlanningAction(
        .projectBrainstorm,
        requestData: request
      ),
        let frozen = try? AssemblywrightMacFrozenBrainstormingSpecification.decodeStrict(
          response,
          matchingDraft: request,
          projection: projection
        ),
        let currentProjection = developerBridge.status.assemblyLine,
        projectReviewGeneration == generation,
        binding.matches(
          repositoryURL: CanonicalGitHubRepositoryURL(projectURL)?.value ?? "",
          visibility: projectVisibility,
          idea: projectIdea,
          orchestratorCatalogSHA256: currentProjection.orchestratorCatalog.catalogSHA256
        )
      else { return }
      projectReview = frozen
      projectReviewBinding = binding
    }
  }

  private func brainstormFeature() {
    guard let projection = developerBridge.status.assemblyLine,
      let canonical = CanonicalGitHubRepositoryURL(featureURL), validIdea(featureIdea),
      let request = try? AssemblywrightMacAssemblyLineOwnerControl.featureBrainstormRequest(
        from: projection,
        repositoryURL: canonical.value,
        idea: featureIdea,
        informationClassification: .public,
        ownerConfirmedCloudDisclosure: true
      )
    else { return }
    let binding = AssemblyLineFrozenReviewInputBinding(
      repositoryURL: canonical.value,
      visibility: nil,
      idea: featureIdea,
      orchestratorCatalogSHA256: projection.orchestratorCatalog.catalogSHA256
    )
    let generation = featureReviewGeneration
    Task {
      guard let response = await developerBridge.performAssemblyLinePlanningAction(
        .featureBrainstorm,
        requestData: request
      ),
        let frozen = try? AssemblywrightMacFrozenBrainstormingSpecification.decodeStrict(
          response,
          matchingDraft: request,
          projection: projection
        ),
        let currentProjection = developerBridge.status.assemblyLine,
        featureReviewGeneration == generation,
        binding.matches(
          repositoryURL: CanonicalGitHubRepositoryURL(featureURL)?.value ?? "",
          visibility: nil,
          idea: featureIdea,
          orchestratorCatalogSHA256: currentProjection.orchestratorCatalog.catalogSHA256
        )
      else { return }
      featureReview = frozen
      featureReviewBinding = binding
    }
  }

  private func prepareProjectApproval() {
    guard let frozen = projectReview,
      let binding = projectReviewBinding,
      let projection = developerBridge.status.assemblyLine,
      binding.matches(
        repositoryURL: CanonicalGitHubRepositoryURL(projectURL)?.value ?? "",
        visibility: projectVisibility,
        idea: projectIdea,
        orchestratorCatalogSHA256: projection.orchestratorCatalog.catalogSHA256
      ),
      let preview = try? AssemblywrightMacAssemblyLineOwnerControl.ownerApprovalPreview(
        for: frozen,
        from: projection,
        approvedAtMilliseconds: UInt64(Date().timeIntervalSince1970 * 1_000)
      )
    else { return }
    projectApprovalPreview = preview
    confirmsProjectApproval = true
  }

  private func approveProject() {
    guard let preview = projectApprovalPreview else { return }
    projectApprovalPreview = nil
    Task {
      if await developerBridge.performAssemblyLinePlanningAction(
        .projectApproval,
        requestData: preview.requestData
      ) != nil {
        projectReview = nil
      }
    }
  }

  private func prepareFeatureApproval() {
    guard let frozen = featureReview,
      let binding = featureReviewBinding,
      let projection = developerBridge.status.assemblyLine,
      binding.matches(
        repositoryURL: CanonicalGitHubRepositoryURL(featureURL)?.value ?? "",
        visibility: nil,
        idea: featureIdea,
        orchestratorCatalogSHA256: projection.orchestratorCatalog.catalogSHA256
      ),
      let preview = try? AssemblywrightMacAssemblyLineOwnerControl.ownerApprovalPreview(
        for: frozen,
        from: projection,
        approvedAtMilliseconds: UInt64(Date().timeIntervalSince1970 * 1_000)
      )
    else { return }
    featureApprovalPreview = preview
    confirmsFeatureApproval = true
  }

  private func approveFeature() {
    guard let preview = featureApprovalPreview else { return }
    featureApprovalPreview = nil
    Task {
      if await developerBridge.performAssemblyLinePlanningAction(
        .featureApproval,
        requestData: preview.requestData
      ) != nil {
        featureReview = nil
      }
    }
  }

  private func createOrReconcile(_ repositoryID: UUID) {
    guard presentation.githubCreationAvailable,
      let request = try? AssemblywrightMacAssemblyLineOwnerControl.repositoryCreationRequest(
        repositoryID: repositoryID
      )
    else { return }
    Task {
      _ = await developerBridge.performAssemblyLinePlanningAction(
        .repositoryCreation,
        requestData: request
      )
    }
  }

  private func validIdea(_ idea: String) -> Bool {
    let trimmed = idea.trimmingCharacters(in: .whitespacesAndNewlines)
    return !trimmed.isEmpty && trimmed == idea && idea.utf8.count <= 16 * 1_024
  }

  private func invalidateProjectReview() {
    projectReviewGeneration &+= 1
    projectReview = nil
    projectReviewBinding = nil
    projectApprovalPreview = nil
    confirmsProjectApproval = false
    if cloudBrainstormTarget == .project {
      cloudBrainstormTarget = nil
    }
  }

  private func invalidateFeatureReview() {
    featureReviewGeneration &+= 1
    featureReview = nil
    featureReviewBinding = nil
    featureApprovalPreview = nil
    confirmsFeatureApproval = false
    if cloudBrainstormTarget == .feature {
      cloudBrainstormTarget = nil
    }
  }

  private func repositoryURLMessage(_ candidate: String) -> String? {
    guard !candidate.isEmpty else { return nil }
    guard let canonical = CanonicalGitHubRepositoryURL(candidate) else {
      return "Enter a GitHub URL like https://github.com/owner/repository"
    }
    return canonical.value == candidate ? "Valid GitHub URL" : "Will use \(canonical.value)"
  }
}

private struct AssemblyLineFrozenSpecificationReview: View {
  let specification: AssemblywrightMacFrozenBrainstormingSpecification

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text("Review Frozen Specification")
        .font(.headline)
      LabeledContent("Title", value: specification.specification.title)
      Text(specification.specification.outcome)
      Text("Acceptance criteria")
        .font(.subheadline.bold())
      ForEach(specification.specification.acceptanceCriteria) { criterion in
        Text("\(criterion.id): \(criterion.requirement)")
      }
      Text("Obligations")
        .font(.subheadline.bold())
      ForEach(specification.specification.obligations, id: \.self) { obligation in
        Text(obligation)
      }
      Text("Specification digest: \(digestPrefix(specification.specificationSHA256))")
        .font(.caption.monospaced())
        .foregroundStyle(.secondary)
    }
    .padding(10)
    .background(.quaternary, in: RoundedRectangle(cornerRadius: 8))
    .accessibilityElement(children: .contain)
  }

  private func digestPrefix(_ bytes: [UInt8]) -> String {
    bytes.prefix(8).map { String(format: "%02x", $0) }.joined()
  }
}

struct AssemblyLineDeveloperDiagnosticsView: View {
  let presentation: AssemblyLineDeveloperDiagnosticsPresentation

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      LabeledContent("Bridge", value: presentation.bridge)
      if let endpoint = presentation.master {
        LabeledContent("Master", value: endpoint)
      }
      if let epoch = presentation.connectionEpoch {
        LabeledContent("Connection epoch", value: epoch)
      }
      if let errorCode = presentation.statusCode {
        LabeledContent("Status code", value: errorCode)
      }
      if let queue = presentation.queue {
        LabeledContent("Queue", value: queue)
      }
      Text("Read-only diagnostics. No owner actions are available here.")
        .font(.caption)
        .foregroundStyle(.secondary)
    }
    .padding(.top, 8)
  }
}
