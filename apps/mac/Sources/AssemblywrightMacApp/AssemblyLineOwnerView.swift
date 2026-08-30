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
  static let planningUnavailableReason = "Brainstorming provider not configured"
  static let executionUnavailableReason =
    "Start is unavailable until the executors and protected brokers are installed"

  var projectVisibility: AssemblyLineProjectVisibility = .public
  var autoRun = true
  var queuedFeatures: [AssemblyLineQueuedFeaturePresentation] = []
  var autoRunControlEnabled = false
  var pendingPlanningAction: AssemblywrightMacAssemblyLinePlanningAction?

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
      autoRunControlEnabled: !ownerActionInProgress && pendingPlanningAction == nil,
      pendingPlanningAction: pendingPlanningAction
    )
  }

  private static func actionName(_ action: AssemblywrightMacAssemblyLinePlanningAction) -> String {
    switch action {
    case .projectDraft: "project brainstorm"
    case .featureDraft: "feature brainstorm"
    case .frozenSpecification: "specification freeze"
    case .projectApproval: "project approval"
    case .featureApproval: "feature approval"
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
  @ObservedObject var developerBridge: AssemblywrightDeveloperBridgeProcessLifecycle
  @State private var projectURL = ""
  @State private var projectIdea = ""
  @State private var featureURL = ""
  @State private var featureIdea = ""
  @State private var orchestrator = "Orchestrator AI"
  @State private var projectVisibility: AssemblyLineProjectVisibility = .public
  @State private var showsDeveloperDetails = false

  private var presentation: AssemblyLineOwnerPresentation {
    .observed(
      status: developerBridge.status,
      ownerActionInProgress: developerBridge.ownerActionInProgress,
      projectVisibility: projectVisibility,
      pendingPlanningAction: developerBridge.pendingAssemblyLinePlanningAction
    )
  }

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
            Picker("Orchestrator", selection: $orchestrator) {
              Text("Orchestrator AI").tag("Orchestrator AI")
            }
            TextField("What do you want to build?", text: $projectIdea, axis: .vertical)
              .lineLimit(2...5)
              .textFieldStyle(.roundedBorder)
            Button(AssemblyLineOwnerPresentation.brainstormProjectLabel) {}
              .disabled(true)
            planningUnavailableMessage
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
            LabeledContent("Orchestrator", value: orchestrator)
            Button(AssemblyLineOwnerPresentation.brainstormFeatureLabel) {}
              .disabled(true)
            planningUnavailableMessage
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
  }

  private var planningUnavailableMessage: some View {
    Text(AssemblyLineOwnerPresentation.planningUnavailableReason)
      .font(.caption)
      .foregroundStyle(.secondary)
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

  private func repositoryURLMessage(_ candidate: String) -> String? {
    guard !candidate.isEmpty else { return nil }
    guard let canonical = CanonicalGitHubRepositoryURL(candidate) else {
      return "Enter a GitHub URL like https://github.com/owner/repository"
    }
    return canonical.value == candidate ? "Valid GitHub URL" : "Will use \(canonical.value)"
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
