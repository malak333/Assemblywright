import CryptoKit
import Foundation

public enum AssemblywrightMacAssemblyLineError: Error, Equatable, Sendable {
  case invalidRequest
  case requestTooLarge
  case invalidProjection
  case invalidReceipt
  case rejected
  case ambiguous
  case outcomeUnknown
}

public enum AssemblywrightMacAssemblyLineHelperExitStatus {
  public static let rejectedBeforeEffect: Int32 = 20
  public static let outcomeUnknown: Int32 = 21
}

public enum AssemblywrightMacAssemblyLinePlanningAction: String, CaseIterable, Codable, Sendable {
  case projectDraft
  case featureDraft
  case frozenSpecification
  case projectBrainstorm
  case featureBrainstorm
  case projectApproval
  case featureApproval
  case repositoryCreation
  case autoRun

  public var helperArguments: [String] {
    switch self {
    case .projectDraft: ["assembly-line", "project-draft", "--confirm"]
    case .featureDraft: ["assembly-line", "feature-draft", "--confirm"]
    case .frozenSpecification: ["assembly-line", "frozen-specification", "--confirm"]
    case .projectBrainstorm: ["assembly-line", "project-brainstorm", "--confirm"]
    case .featureBrainstorm: ["assembly-line", "feature-brainstorm", "--confirm"]
    case .projectApproval: ["assembly-line", "approve-project", "--confirm"]
    case .featureApproval: ["assembly-line", "approve-feature", "--confirm"]
    case .repositoryCreation: ["assembly-line", "create-repository", "--confirm"]
    case .autoRun: ["assembly-line", "auto-run", "--confirm"]
    }
  }

  fileprivate func remoteRequest(_ request: [String: Any], requestData: Data) throws ->
    (path: String, body: Data)
  {
    switch self {
    case .projectDraft: return ("/v1/distributed/assembly-line/project-drafts", requestData)
    case .featureDraft: return ("/v1/distributed/assembly-line/feature-drafts", requestData)
    case .frozenSpecification:
      return ("/v1/distributed/assembly-line/frozen-specifications", requestData)
    case .projectBrainstorm:
      return ("/v1/distributed/assembly-line/project-brainstorms", requestData)
    case .featureBrainstorm:
      return ("/v1/distributed/assembly-line/feature-brainstorms", requestData)
    case .projectApproval:
      return ("/v1/distributed/assembly-line/project-approvals", requestData)
    case .featureApproval:
      return ("/v1/distributed/assembly-line/feature-approvals", requestData)
    case .repositoryCreation:
      guard let repositoryID = AssemblyLineStrictJSON.canonicalUUIDText(request["repository_id"])
      else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
      return (
        "/v1/distributed/assembly-line/repositories/\(repositoryID)/create",
        Data()
      )
    case .autoRun: return ("/v1/distributed/assembly-line/auto-run", requestData)
    }
  }

  fileprivate var persistedRequestKeys: [String] {
    switch self {
    case .projectDraft:
      [
        "schema_version", "draft_id", "draft_revision", "repository", "visibility",
        "orchestrator_catalog", "orchestrator", "idea",
      ]
    case .featureDraft:
      [
        "schema_version", "draft_id", "draft_revision", "repository",
        "expected_repository_revision", "orchestrator_catalog", "orchestrator", "idea",
      ]
    case .frozenSpecification:
      [
        "schema_version", "specification_id", "specification_revision", "target_kind", "draft_id",
        "draft_revision", "draft_sha256", "repository", "visibility",
        "orchestrator_catalog_revision", "orchestrator_catalog_sha256",
        "orchestrator_profile_sha256", "specification", "specification_sha256",
      ]
    case .projectBrainstorm, .featureBrainstorm:
      [
        "schema_version", "draft", "information_classification",
        "owner_cloud_disclosure_sha256",
      ]
    case .projectApproval, .featureApproval:
      [
        "schema_version", "approval_id", "approved_at_ms", "owner_control_revision",
        "target_kind", "repository", "visibility", "expected_repository_revision",
        "expected_queue_revision", "draft_id", "draft_revision", "draft_sha256",
        "orchestrator_catalog_revision", "orchestrator_catalog_sha256", "specification_id",
        "specification_revision", "specification_sha256", "orchestrator_profile_sha256",
        "owner_approval_sha256",
      ]
    case .repositoryCreation:
      ["schema_version", "repository_id"]
    case .autoRun:
      ["schema_version", "request_id", "expected_state_revision", "auto_run"]
    }
  }
}

public enum AssemblywrightMacProjectVisibility: String, Codable, Sendable {
  case `public`
  case `private`
}

public enum AssemblywrightMacPlanningInformationClassification: String, Sendable {
  case `public`
  case `private`
  case restricted
  case secret
}

public enum AssemblywrightMacRepositoryCreationLifecycle: String, Codable, Sendable {
  case creationPending = "creation_pending"
  case reconciling
  case created
  case conflict
  case reconciliationRequired = "reconciliation_required"
  case failed
}

public enum AssemblywrightMacFeatureQueueLifecycle: String, Codable, Sendable {
  case queued
  case starting
  case active
  case stopping
  case pausedAtCheckpoint = "paused_at_checkpoint"
  case emergencyPaused = "emergency_paused"
  case waitingForHostReconnect = "waiting_for_host_reconnect"
  case reconciliationRequired = "reconciliation_required"
  case incompleteTermination = "incomplete_termination"
}

public enum AssemblywrightMacAssemblyLineLifecycle: String, Codable, Sendable {
  case stopped
  case starting
  case running
  case stopping
  case pausedAtCheckpoint = "paused_at_checkpoint"
  case emergencyPaused = "emergency_paused"
  case waitingForHostReconnect = "waiting_for_host_reconnect"
  case reconciliationRequired = "reconciliation_required"
  case incompleteTermination = "incomplete_termination"
  case waitingForOwnerStart = "waiting_for_owner_start"
}

public enum AssemblywrightMacRuntimeAvailabilityStatus: String, Codable, Sendable {
  case available
  case unavailable
}

public enum AssemblywrightMacRuntimeUnavailableReason: String, Codable, Sendable {
  case notConfigured = "not_configured"
  case notAuthenticated = "not_authenticated"
  case disconnected
  case unhealthy
  case emergencyPaused = "emergency_paused"
  case identityDrift = "identity_drift"
  case evidenceRequired = "evidence_required"
}

public struct AssemblywrightMacOrchestratorProfile: Codable, Equatable, Sendable {
  public let configurationRevision: UInt64
  public let providerID: String
  public let modelID: String

  enum CodingKeys: String, CodingKey, CaseIterable {
    case configurationRevision = "configuration_revision"
    case providerID = "provider_id"
    case modelID = "model_id"
  }
}

public struct AssemblywrightMacOrchestratorCatalog: Codable, Equatable, Sendable {
  public let schemaVersion: UInt16
  public let catalogRevision: UInt64
  public let profiles: [AssemblywrightMacOrchestratorProfile]
  public let defaultProfileSHA256: [UInt8]
  public let catalogSHA256: [UInt8]

  enum CodingKeys: String, CodingKey, CaseIterable {
    case schemaVersion = "schema_version"
    case catalogRevision = "catalog_revision"
    case profiles
    case defaultProfileSHA256 = "default_profile_sha256"
    case catalogSHA256 = "catalog_sha256"
  }
}

public enum AssemblywrightMacBrainstormingTargetKind: String, Codable, Sendable {
  case project
  case feature
}

public struct AssemblywrightMacBrainstormingAcceptanceCriterion: Codable, Equatable, Sendable,
  Identifiable
{
  public let id: String
  public let requirement: String
}

public struct AssemblywrightMacBrainstormingSpecificationDocument: Codable, Equatable, Sendable {
  public let title: String
  public let outcome: String
  public let acceptanceCriteria: [AssemblywrightMacBrainstormingAcceptanceCriterion]
  public let obligations: [String]

  enum CodingKeys: String, CodingKey, CaseIterable {
    case title, outcome
    case acceptanceCriteria = "acceptance_criteria"
    case obligations
  }
}

public struct AssemblywrightMacAssemblyLineRepositoryIdentity: Codable, Equatable, Sendable {
  public struct GitURL: Codable, Equatable, Sendable {
    public let url: String

    enum CodingKeys: String, CodingKey, CaseIterable { case url }
  }

  public let repositoryID: UUID
  public let gitURL: GitURL

  enum CodingKeys: String, CodingKey, CaseIterable {
    case repositoryID = "repository_id"
    case gitURL = "git_url"
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(repositoryID.uuidString.lowercased(), forKey: .repositoryID)
    try container.encode(gitURL, forKey: .gitURL)
  }
}

public struct AssemblywrightMacFrozenBrainstormingSpecification: Codable, Equatable, Sendable {
  public let schemaVersion: UInt16
  public let specificationID: UUID
  public let specificationRevision: UInt64
  public let targetKind: AssemblywrightMacBrainstormingTargetKind
  public let draftID: UUID
  public let draftRevision: UInt64
  public let draftSHA256: [UInt8]
  public let repository: AssemblywrightMacAssemblyLineRepositoryIdentity
  public let visibility: AssemblywrightMacProjectVisibility?
  public let orchestratorCatalogRevision: UInt64
  public let orchestratorCatalogSHA256: [UInt8]
  public let orchestratorProfileSHA256: [UInt8]
  public let specification: AssemblywrightMacBrainstormingSpecificationDocument
  public let specificationSHA256: [UInt8]

  enum CodingKeys: String, CodingKey, CaseIterable {
    case schemaVersion = "schema_version"
    case specificationID = "specification_id"
    case specificationRevision = "specification_revision"
    case targetKind = "target_kind"
    case draftID = "draft_id"
    case draftRevision = "draft_revision"
    case draftSHA256 = "draft_sha256"
    case repository, visibility
    case orchestratorCatalogRevision = "orchestrator_catalog_revision"
    case orchestratorCatalogSHA256 = "orchestrator_catalog_sha256"
    case orchestratorProfileSHA256 = "orchestrator_profile_sha256"
    case specification
    case specificationSHA256 = "specification_sha256"
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(schemaVersion, forKey: .schemaVersion)
    try container.encode(specificationID.uuidString.lowercased(), forKey: .specificationID)
    try container.encode(specificationRevision, forKey: .specificationRevision)
    try container.encode(targetKind, forKey: .targetKind)
    try container.encode(draftID.uuidString.lowercased(), forKey: .draftID)
    try container.encode(draftRevision, forKey: .draftRevision)
    try container.encode(draftSHA256, forKey: .draftSHA256)
    try container.encode(repository, forKey: .repository)
    try container.encode(visibility, forKey: .visibility)
    try container.encode(orchestratorCatalogRevision, forKey: .orchestratorCatalogRevision)
    try container.encode(orchestratorCatalogSHA256, forKey: .orchestratorCatalogSHA256)
    try container.encode(orchestratorProfileSHA256, forKey: .orchestratorProfileSHA256)
    try container.encode(specification, forKey: .specification)
    try container.encode(specificationSHA256, forKey: .specificationSHA256)
  }

  public static func decodeStrict(
    _ data: Data,
    matchingDraft draftData: Data,
    projection: AssemblywrightMacAssemblyLineOwnerProjection
  ) throws -> Self {
    try AssemblyLineStrictJSON.decodeFrozenResponse(
      data,
      matchingDraft: draftData,
      projection: projection
    )
  }
}

public struct AssemblywrightMacOwnerApprovalPreview: Equatable, Sendable {
  public let requestData: Data
  public let targetKind: AssemblywrightMacBrainstormingTargetKind
  public let repositoryURL: String
  public let visibility: AssemblywrightMacProjectVisibility?
  public let specificationID: UUID
  public let specificationSHA256: [UInt8]
  public let ownerApprovalSHA256: [UInt8]
}

public struct AssemblywrightMacRepositoryCreationProjection: Codable, Equatable, Sendable {
  public let schemaVersion: UInt16
  public let repository: AssemblywrightMacAssemblyLineRepositoryIdentity
  public let repositoryRevision: UInt64
  public let lifecycleRevision: UInt64
  public let visibility: AssemblywrightMacProjectVisibility
  public let approvedSpecificationID: UUID
  public let approvedSpecificationRevision: UInt64
  public let approvedSpecificationSHA256: [UInt8]
  public let ownerApprovalSHA256: [UInt8]
  public let lifecycle: AssemblywrightMacRepositoryCreationLifecycle
  public let effectPossible: Bool
  public let creationEvidenceSHA256: [UInt8]?

  enum CodingKeys: String, CodingKey, CaseIterable {
    case schemaVersion = "schema_version"
    case repository
    case repositoryRevision = "repository_revision"
    case lifecycleRevision = "lifecycle_revision"
    case visibility
    case approvedSpecificationID = "approved_specification_id"
    case approvedSpecificationRevision = "approved_specification_revision"
    case approvedSpecificationSHA256 = "approved_specification_sha256"
    case ownerApprovalSHA256 = "owner_approval_sha256"
    case lifecycle
    case effectPossible = "effect_possible"
    case creationEvidenceSHA256 = "creation_evidence_sha256"
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(schemaVersion, forKey: .schemaVersion)
    try container.encode(repository, forKey: .repository)
    try container.encode(repositoryRevision, forKey: .repositoryRevision)
    try container.encode(lifecycleRevision, forKey: .lifecycleRevision)
    try container.encode(visibility, forKey: .visibility)
    try container.encode(
      approvedSpecificationID.uuidString.lowercased(),
      forKey: .approvedSpecificationID
    )
    try container.encode(approvedSpecificationRevision, forKey: .approvedSpecificationRevision)
    try container.encode(approvedSpecificationSHA256, forKey: .approvedSpecificationSHA256)
    try container.encode(ownerApprovalSHA256, forKey: .ownerApprovalSHA256)
    try container.encode(lifecycle, forKey: .lifecycle)
    try container.encode(effectPossible, forKey: .effectPossible)
    try container.encode(creationEvidenceSHA256, forKey: .creationEvidenceSHA256)
  }
}

public struct AssemblywrightMacFeatureQueueEntryProjection: Codable, Equatable, Sendable {
  public let schemaVersion: UInt16
  public let featureID: UUID
  public let repositoryID: UUID
  public let specificationID: UUID
  public let specificationRevision: UInt64
  public let specificationSHA256: [UInt8]
  public let ownerApprovalSHA256: [UInt8]
  public let position: UInt16
  public let lifecycleRevision: UInt64
  public let lifecycle: AssemblywrightMacFeatureQueueLifecycle

  enum CodingKeys: String, CodingKey, CaseIterable {
    case schemaVersion = "schema_version"
    case featureID = "feature_id"
    case repositoryID = "repository_id"
    case specificationID = "specification_id"
    case specificationRevision = "specification_revision"
    case specificationSHA256 = "specification_sha256"
    case ownerApprovalSHA256 = "owner_approval_sha256"
    case position
    case lifecycleRevision = "lifecycle_revision"
    case lifecycle
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(schemaVersion, forKey: .schemaVersion)
    try container.encode(featureID.uuidString.lowercased(), forKey: .featureID)
    try container.encode(repositoryID.uuidString.lowercased(), forKey: .repositoryID)
    try container.encode(specificationID.uuidString.lowercased(), forKey: .specificationID)
    try container.encode(specificationRevision, forKey: .specificationRevision)
    try container.encode(specificationSHA256, forKey: .specificationSHA256)
    try container.encode(ownerApprovalSHA256, forKey: .ownerApprovalSHA256)
    try container.encode(position, forKey: .position)
    try container.encode(lifecycleRevision, forKey: .lifecycleRevision)
    try container.encode(lifecycle, forKey: .lifecycle)
  }
}

public struct AssemblywrightMacAssemblyLineState: Codable, Equatable, Sendable {
  public let schemaVersion: UInt16
  public let stateRevision: UInt64
  public let queueRevision: UInt64
  public let queueCount: UInt16
  public let autoRun: Bool
  public let lifecycle: AssemblywrightMacAssemblyLineLifecycle
  public let sessionID: UUID?
  public let activeChildEpochID: UUID?
  public let activeFeatureID: UUID?

  enum CodingKeys: String, CodingKey, CaseIterable {
    case schemaVersion = "schema_version"
    case stateRevision = "state_revision"
    case queueRevision = "queue_revision"
    case queueCount = "queue_count"
    case autoRun = "auto_run"
    case lifecycle
    case sessionID = "session_id"
    case activeChildEpochID = "active_child_epoch_id"
    case activeFeatureID = "active_feature_id"
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(schemaVersion, forKey: .schemaVersion)
    try container.encode(stateRevision, forKey: .stateRevision)
    try container.encode(queueRevision, forKey: .queueRevision)
    try container.encode(queueCount, forKey: .queueCount)
    try container.encode(autoRun, forKey: .autoRun)
    try container.encode(lifecycle, forKey: .lifecycle)
    try Self.encodeUUID(sessionID, into: &container, key: .sessionID)
    try Self.encodeUUID(activeChildEpochID, into: &container, key: .activeChildEpochID)
    try Self.encodeUUID(activeFeatureID, into: &container, key: .activeFeatureID)
  }

  private static func encodeUUID(
    _ value: UUID?,
    into container: inout KeyedEncodingContainer<CodingKeys>,
    key: CodingKeys
  ) throws {
    if let value {
      try container.encode(value.uuidString.lowercased(), forKey: key)
    } else {
      try container.encodeNil(forKey: key)
    }
  }
}

public struct AssemblywrightMacRuntimeComponentAvailability: Codable, Equatable, Sendable {
  public let bindingRevision: UInt64
  public let bindingSHA256: [UInt8]
  public let status: AssemblywrightMacRuntimeAvailabilityStatus
  public let unavailableReason: AssemblywrightMacRuntimeUnavailableReason?

  enum CodingKeys: String, CodingKey, CaseIterable {
    case bindingRevision = "binding_revision"
    case bindingSHA256 = "binding_sha256"
    case status
    case unavailableReason = "unavailable_reason"
  }
}

public struct AssemblywrightMacAssemblyLineRuntimeAvailability: Codable, Equatable, Sendable {
  public let schemaVersion: UInt16
  public let availabilityRevision: UInt64
  public let observedAtMilliseconds: UInt64
  public let brainstormingProvider: AssemblywrightMacRuntimeComponentAvailability
  public let githubCreation: AssemblywrightMacRuntimeComponentAvailability
  public let windowsExecutor: AssemblywrightMacRuntimeComponentAvailability
  public let macExecutor: AssemblywrightMacRuntimeComponentAvailability
  public let protectedBrokers: AssemblywrightMacRuntimeComponentAvailability

  enum CodingKeys: String, CodingKey, CaseIterable {
    case schemaVersion = "schema_version"
    case availabilityRevision = "availability_revision"
    case observedAtMilliseconds = "observed_at_ms"
    case brainstormingProvider = "brainstorming_provider"
    case githubCreation = "github_creation"
    case windowsExecutor = "windows_executor"
    case macExecutor = "mac_executor"
    case protectedBrokers = "protected_brokers"
  }
}

public struct AssemblywrightMacAssemblyLineOwnerProjection: Codable, Equatable, Sendable {
  public static let expectedSchemaVersion: UInt16 = 1
  public static let maximumBytes = 256 * 1_024
  public let schemaVersion: UInt16
  public let ownerControlRevision: UInt64
  public let emergencyPauseRevision: UInt64
  public let emergencyPaused: Bool
  public let orchestratorCatalog: AssemblywrightMacOrchestratorCatalog
  public let repositories: [AssemblywrightMacRepositoryCreationProjection]
  public let queue: [AssemblywrightMacFeatureQueueEntryProjection]
  public let assemblyLine: AssemblywrightMacAssemblyLineState
  public let availability: AssemblywrightMacAssemblyLineRuntimeAvailability

  enum CodingKeys: String, CodingKey, CaseIterable {
    case schemaVersion = "schema_version"
    case ownerControlRevision = "owner_control_revision"
    case emergencyPauseRevision = "emergency_pause_revision"
    case emergencyPaused = "emergency_paused"
    case orchestratorCatalog = "orchestrator_catalog"
    case repositories, queue
    case assemblyLine = "assembly_line"
    case availability
  }

  public static func decodeStrict(_ data: Data) throws -> Self {
    guard !data.isEmpty, data.count <= maximumBytes else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    do {
      var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
      try scanner.validateNoDuplicateObjectKeysRecursively()
      let raw = try AssemblyLineStrictJSON.object(data)
      try AssemblyLineStrictJSON.validateProjection(raw)
      let decoded = try JSONDecoder().decode(Self.self, from: data)
      guard decoded.schemaVersion == expectedSchemaVersion else {
        throw AssemblywrightMacAssemblyLineError.invalidProjection
      }
      return decoded
    } catch let error as AssemblywrightMacAssemblyLineError { throw error } catch {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
  }
}

public enum AssemblywrightMacAssemblyLineOwnerControl {
  public static let maximumRequestBytes = 96 * 1_024
  public static let maximumResponseBytes = 256 * 1_024
  public static let maximumBrainstormingCloudRequestBytes = 24 * 1_024

  public static func projectBrainstormRequest(
    from projection: AssemblywrightMacAssemblyLineOwnerProjection,
    repositoryID: UUID = UUID(),
    repositoryURL: String,
    visibility: AssemblywrightMacProjectVisibility,
    idea: String,
    informationClassification: AssemblywrightMacPlanningInformationClassification,
    ownerConfirmedCloudDisclosure: Bool,
    draftID: UUID = UUID()
  ) throws -> Data {
    try brainstormRequest(
      action: .projectBrainstorm,
      projection: projection,
      repositoryID: repositoryID,
      repositoryURL: repositoryURL,
      visibility: visibility,
      idea: idea,
      informationClassification: informationClassification,
      ownerConfirmedCloudDisclosure: ownerConfirmedCloudDisclosure,
      draftID: draftID
    )
  }

  public static func featureBrainstormRequest(
    from projection: AssemblywrightMacAssemblyLineOwnerProjection,
    repositoryURL: String,
    idea: String,
    informationClassification: AssemblywrightMacPlanningInformationClassification,
    ownerConfirmedCloudDisclosure: Bool,
    draftID: UUID = UUID()
  ) throws -> Data {
    guard
      let repository = projection.repositories.first(where: {
        $0.repository.gitURL.url == repositoryURL && $0.lifecycle == .created
      })
    else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
    return try brainstormRequest(
      action: .featureBrainstorm,
      projection: projection,
      repositoryID: repository.repository.repositoryID,
      repositoryURL: repositoryURL,
      visibility: nil,
      idea: idea,
      informationClassification: informationClassification,
      ownerConfirmedCloudDisclosure: ownerConfirmedCloudDisclosure,
      draftID: draftID,
      expectedRepositoryRevision: repository.repositoryRevision
    )
  }

  public static func ownerApprovalRequest(
    for frozen: AssemblywrightMacFrozenBrainstormingSpecification,
    from projection: AssemblywrightMacAssemblyLineOwnerProjection,
    approvalID: UUID = UUID(),
    approvedAtMilliseconds: UInt64
  ) throws -> Data {
    guard approvalID != AssemblyLineStrictJSON.nilUUID, approvedAtMilliseconds > 0,
      frozen.orchestratorCatalogRevision == projection.orchestratorCatalog.catalogRevision,
      frozen.orchestratorCatalogSHA256 == projection.orchestratorCatalog.catalogSHA256,
      !projection.emergencyPaused
    else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
    var request: [String: Any] = [
      "schema_version": 1,
      "approval_id": approvalID.uuidString.lowercased(),
      "approved_at_ms": approvedAtMilliseconds,
      "owner_control_revision": projection.ownerControlRevision,
      "target_kind": frozen.targetKind.rawValue,
      "repository": try AssemblyLineStrictJSON.object(JSONEncoder().encode(frozen.repository)),
      "visibility": frozen.visibility?.rawValue ?? NSNull(),
      "expected_repository_revision": NSNull(),
      "expected_queue_revision": NSNull(),
      "draft_id": frozen.draftID.uuidString.lowercased(),
      "draft_revision": frozen.draftRevision,
      "draft_sha256": frozen.draftSHA256,
      "orchestrator_catalog_revision": frozen.orchestratorCatalogRevision,
      "orchestrator_catalog_sha256": frozen.orchestratorCatalogSHA256,
      "specification_id": frozen.specificationID.uuidString.lowercased(),
      "specification_revision": frozen.specificationRevision,
      "specification_sha256": frozen.specificationSHA256,
      "orchestrator_profile_sha256": frozen.orchestratorProfileSHA256,
    ]
    let action: AssemblywrightMacAssemblyLinePlanningAction
    switch frozen.targetKind {
    case .project:
      guard frozen.visibility != nil,
        !projection.repositories.contains(where: {
          $0.repository.repositoryID == frozen.repository.repositoryID
            || $0.repository.gitURL.url == frozen.repository.gitURL.url
        })
      else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
      request["expected_repository_revision"] = 0
      action = .projectApproval
    case .feature:
      guard frozen.visibility == nil,
        let repository = projection.repositories.first(where: {
          $0.repository == frozen.repository && $0.lifecycle == .created
        })
      else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
      request["expected_repository_revision"] = repository.repositoryRevision
      request["expected_queue_revision"] = projection.assemblyLine.queueRevision
      action = .featureApproval
    }
    request["owner_approval_sha256"] = try AssemblyLineStrictJSON.sha256(request)
    let data = try AssemblyLineStrictJSON.canonicalData(request)
    try validateRequest(action: action, requestData: data, against: projection)
    return data
  }

  public static func ownerApprovalPreview(
    for frozen: AssemblywrightMacFrozenBrainstormingSpecification,
    from projection: AssemblywrightMacAssemblyLineOwnerProjection,
    approvalID: UUID = UUID(),
    approvedAtMilliseconds: UInt64
  ) throws -> AssemblywrightMacOwnerApprovalPreview {
    let requestData = try ownerApprovalRequest(
      for: frozen,
      from: projection,
      approvalID: approvalID,
      approvedAtMilliseconds: approvedAtMilliseconds
    )
    let request = try AssemblyLineStrictJSON.object(requestData)
    guard let ownerApprovalSHA256 = AssemblyLineStrictJSON.digest(
      request["owner_approval_sha256"]
    ) else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
    return AssemblywrightMacOwnerApprovalPreview(
      requestData: requestData,
      targetKind: frozen.targetKind,
      repositoryURL: frozen.repository.gitURL.url,
      visibility: frozen.visibility,
      specificationID: frozen.specificationID,
      specificationSHA256: frozen.specificationSHA256,
      ownerApprovalSHA256: ownerApprovalSHA256
    )
  }

  public static func repositoryCreationRequest(repositoryID: UUID) throws -> Data {
    guard repositoryID != AssemblyLineStrictJSON.nilUUID else {
      throw AssemblywrightMacAssemblyLineError.invalidRequest
    }
    let data = try AssemblyLineStrictJSON.canonicalData([
      "schema_version": 1,
      "repository_id": repositoryID.uuidString.lowercased(),
    ])
    try validateStoredRequest(action: .repositoryCreation, requestData: data)
    return data
  }

  private static func brainstormRequest(
    action: AssemblywrightMacAssemblyLinePlanningAction,
    projection: AssemblywrightMacAssemblyLineOwnerProjection,
    repositoryID: UUID,
    repositoryURL: String,
    visibility: AssemblywrightMacProjectVisibility?,
    idea: String,
    informationClassification: AssemblywrightMacPlanningInformationClassification,
    ownerConfirmedCloudDisclosure: Bool,
    draftID: UUID,
    expectedRepositoryRevision: UInt64? = nil
  ) throws -> Data {
    guard idea.utf8.count <= 16 * 1_024 else {
      throw AssemblywrightMacAssemblyLineError.requestTooLarge
    }
    let profile = try AssemblyLineStrictJSON.defaultProfile(
      in: projection.orchestratorCatalog
    )
    guard [.projectBrainstorm, .featureBrainstorm].contains(action),
      repositoryID != AssemblyLineStrictJSON.nilUUID, draftID != AssemblyLineStrictJSON.nilUUID,
      informationClassification == .public, ownerConfirmedCloudDisclosure,
      !projection.emergencyPaused,
      projection.availability.brainstormingProvider.status == .available,
      projection.availability.brainstormingProvider.unavailableReason == nil,
      let catalog = try? AssemblyLineStrictJSON.object(JSONEncoder().encode(
        projection.orchestratorCatalog
      ))
    else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
    var request: [String: Any] = [
      "schema_version": 1,
      "draft_id": draftID.uuidString.lowercased(),
      "draft_revision": 1,
      "repository": [
        "repository_id": repositoryID.uuidString.lowercased(),
        "git_url": ["url": repositoryURL],
      ],
      "orchestrator_catalog": catalog,
      "orchestrator": try AssemblyLineStrictJSON.object(JSONEncoder().encode(profile)),
      "idea": idea,
    ]
    if action == .projectBrainstorm {
      guard let visibility else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
      request["visibility"] = visibility.rawValue
    } else {
      guard let expectedRepositoryRevision else {
        throw AssemblywrightMacAssemblyLineError.invalidRequest
      }
      request["expected_repository_revision"] = expectedRepositoryRevision
    }
    let draftSHA256 = try AssemblyLineStrictJSON.sha256(request)
    let profileObject = try AssemblyLineStrictJSON.object(JSONEncoder().encode(profile))
    let disclosureBinding: [String: Any] = [
      "schema_version": 1,
      "target_kind": action == .projectBrainstorm ? "project" : "feature",
      "draft_sha256": draftSHA256,
      "information_classification": "public",
      "provider_id": profile.providerID,
      "model_id": profile.modelID,
      "orchestrator_catalog_revision": projection.orchestratorCatalog.catalogRevision,
      "orchestrator_catalog_sha256": projection.orchestratorCatalog.catalogSHA256,
      "orchestrator_profile_sha256": try AssemblyLineStrictJSON.sha256(profileObject),
    ]
    var disclosurePreimage = Data("assemblywright.owner-cloud-disclosure.v1\0".utf8)
    disclosurePreimage.append(try AssemblyLineStrictJSON.canonicalData(disclosureBinding))
    let envelope: [String: Any] = [
      "schema_version": 1,
      "draft": request,
      "information_classification": "public",
      "owner_cloud_disclosure_sha256": Array(SHA256.hash(data: disclosurePreimage)),
    ]
    let data = try AssemblyLineStrictJSON.canonicalData(envelope)
    guard data.count <= maximumBrainstormingCloudRequestBytes else {
      throw AssemblywrightMacAssemblyLineError.requestTooLarge
    }
    try validateRequest(action: action, requestData: data, against: projection)
    return data
  }

  public static func autoRunRequest(
    from projection: AssemblywrightMacAssemblyLineOwnerProjection,
    enabled: Bool,
    requestID: UUID = UUID()
  ) throws -> Data {
    guard requestID != AssemblyLineStrictJSON.nilUUID,
      projection.assemblyLine.stateRevision > 0
    else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
    let data = try JSONSerialization.data(
      withJSONObject: [
        "schema_version": Int(AssemblywrightMacAssemblyLineOwnerProjection.expectedSchemaVersion),
        "request_id": requestID.uuidString.lowercased(),
        "expected_state_revision": projection.assemblyLine.stateRevision,
        "auto_run": enabled,
      ],
      options: [.sortedKeys]
    )
    guard data.count <= maximumRequestBytes else {
      throw AssemblywrightMacAssemblyLineError.requestTooLarge
    }
    let request = try AssemblyLineStrictJSON.decodeRequest(data, action: .autoRun)
    try AssemblyLineStrictJSON.validateRequest(request, action: .autoRun, against: projection)
    return data
  }

  public static func validateRequest(
    action: AssemblywrightMacAssemblyLinePlanningAction,
    requestData: Data,
    against projection: AssemblywrightMacAssemblyLineOwnerProjection
  ) throws {
    let actionLimit = requestLimit(for: action)
    guard !requestData.isEmpty, requestData.count <= actionLimit else {
      throw requestData.count > actionLimit
        ? AssemblywrightMacAssemblyLineError.requestTooLarge
        : AssemblywrightMacAssemblyLineError.invalidRequest
    }
    let request = try AssemblyLineStrictJSON.decodeRequest(requestData, action: action)
    try AssemblyLineStrictJSON.validateRequest(request, action: action, against: projection)
  }

  static func validateStoredRequest(
    action: AssemblywrightMacAssemblyLinePlanningAction,
    requestData: Data
  ) throws {
    let actionLimit = requestLimit(for: action)
    guard !requestData.isEmpty, requestData.count <= actionLimit else {
      throw AssemblywrightMacAssemblyLineError.invalidRequest
    }
    let request = try AssemblyLineStrictJSON.decodeRequest(requestData, action: action)
    guard AssemblyLineStrictJSON.uint(request["schema_version"]) == 1,
      AssemblyLineStrictJSON.exact(request, action.persistedRequestKeys),
      !AssemblyLineStrictJSON.containsSensitiveString(request)
    else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
    if action == .repositoryCreation,
      AssemblyLineStrictJSON.uuid(request["repository_id"]) == nil
    {
      throw AssemblywrightMacAssemblyLineError.invalidRequest
    }
    if action == .projectBrainstorm || action == .featureBrainstorm {
      _ = try AssemblyLineStrictJSON.validateCloudBrainstormRequest(request, action: action)
    }
  }

  public static func perform(
    action: AssemblywrightMacAssemblyLinePlanningAction,
    requestData: Data,
    using session: any AssemblywrightMacBridgeSession
  ) async throws -> Data {
    var postAttempted = false
    do {
      guard !requestData.isEmpty else {
        throw AssemblywrightMacAssemblyLineError.invalidRequest
      }
      let actionLimit = requestLimit(for: action)
      guard requestData.count <= actionLimit else {
        throw AssemblywrightMacAssemblyLineError.requestTooLarge
      }
      let request = try AssemblyLineStrictJSON.decodeRequest(requestData, action: action)
      let prior = try await fetchProjection(using: session)
      try validateRequest(action: action, requestData: requestData, against: prior)
      let remote = try action.remoteRequest(request, requestData: requestData)
      postAttempted = true
      let response = try await session.send(
        .init(method: "POST", path: remote.path, body: remote.body))
      guard response.body.count <= maximumResponseBytes else {
        throw AssemblywrightMacAssemblyLineError.invalidReceipt
      }
      guard response.status == 200 else {
        throw try AssemblyLineStrictJSON.errorDisposition(
          status: response.status,
          data: response.body,
          action: action,
          prior: prior,
          request: request
        )
      }
      try validateHelperOutput(
        action: action,
        requestData: requestData,
        responseData: response.body
      )
      let post = try await fetchProjection(using: session)
      try AssemblyLineStrictJSON.validateResponse(
        response.body,
        action: action,
        request: request,
        prior: prior,
        post: post
      )
      await session.cancel()
      return response.body
    } catch {
      await session.cancel()
      if postAttempted {
        if error as? AssemblywrightMacAssemblyLineError == .rejected {
          throw error
        }
        throw AssemblywrightMacAssemblyLineError.outcomeUnknown
      }
      throw error
    }
  }

  public static func validateHelperOutput(
    action: AssemblywrightMacAssemblyLinePlanningAction,
    requestData: Data,
    responseData: Data
  ) throws {
    guard !requestData.isEmpty, requestData.count <= maximumRequestBytes,
      !responseData.isEmpty, responseData.count <= maximumResponseBytes
    else { throw AssemblywrightMacAssemblyLineError.invalidReceipt }
    do {
      let request = try AssemblyLineStrictJSON.decodeRequest(requestData, action: action)
      switch action {
      case .projectDraft, .featureDraft, .frozenSpecification:
        _ = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(responseData)
      case .projectBrainstorm, .featureBrainstorm:
        _ = try AssemblyLineStrictJSON.decodeFrozenResponseWithoutProjection(
          responseData,
          matchingDraft: requestData
        )
      case .projectApproval:
        var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: responseData)
        try scanner.validateNoDuplicateObjectKeysRecursively()
        let raw = try AssemblyLineStrictJSON.object(responseData)
        let receipt = try AssemblyLineStrictJSON.validateRepository(raw)
        let typed = try JSONDecoder().decode(
          AssemblywrightMacRepositoryCreationProjection.self, from: responseData)
        guard
          let (repositoryID, url) = try? AssemblyLineStrictJSON.validateRepositoryIdentity(
            request["repository"]),
          receipt.id == repositoryID, receipt.url == url, !receipt.created,
          typed.repositoryRevision == 1, typed.lifecycleRevision == 1,
          typed.visibility.rawValue == request["visibility"] as? String,
          typed.approvedSpecificationID
            == AssemblyLineStrictJSON.uuid(
              request["specification_id"]),
          typed.approvedSpecificationRevision
            == AssemblyLineStrictJSON.uint(
              request["specification_revision"]),
          typed.approvedSpecificationSHA256
            == AssemblyLineStrictJSON.digest(
              request["specification_sha256"]),
          typed.ownerApprovalSHA256
            == AssemblyLineStrictJSON.digest(
              request["owner_approval_sha256"]),
          typed.lifecycle == .creationPending, !typed.effectPossible,
          typed.creationEvidenceSHA256 == nil
        else { throw AssemblywrightMacAssemblyLineError.invalidReceipt }
      case .featureApproval:
        var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: responseData)
        try scanner.validateNoDuplicateObjectKeysRecursively()
        let raw = try AssemblyLineStrictJSON.object(responseData)
        let receipt = try AssemblyLineStrictJSON.validateQueueEntry(raw)
        let typed = try JSONDecoder().decode(
          AssemblywrightMacFeatureQueueEntryProjection.self, from: responseData)
        let repositoryID =
          (try? AssemblyLineStrictJSON.validateRepositoryIdentity(
            request["repository"]))?.0
        guard receipt.featureID == AssemblyLineStrictJSON.uuid(request["approval_id"]),
          typed.repositoryID == repositoryID,
          typed.specificationID == AssemblyLineStrictJSON.uuid(request["specification_id"]),
          typed.specificationRevision
            == AssemblyLineStrictJSON.uint(
              request["specification_revision"]),
          typed.specificationSHA256
            == AssemblyLineStrictJSON.digest(
              request["specification_sha256"]),
          typed.ownerApprovalSHA256
            == AssemblyLineStrictJSON.digest(
              request["owner_approval_sha256"]),
          typed.lifecycleRevision == 1, typed.lifecycle == .queued
        else { throw AssemblywrightMacAssemblyLineError.invalidReceipt }
      case .repositoryCreation:
        var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: responseData)
        try scanner.validateNoDuplicateObjectKeysRecursively()
        let raw = try AssemblyLineStrictJSON.object(responseData)
        let receipt = try AssemblyLineStrictJSON.validateRepository(raw)
        guard receipt.id == AssemblyLineStrictJSON.uuid(request["repository_id"])
        else { throw AssemblywrightMacAssemblyLineError.invalidReceipt }
      case .autoRun:
        var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: responseData)
        try scanner.validateNoDuplicateObjectKeysRecursively()
        let raw = try AssemblyLineStrictJSON.object(responseData)
        guard
          AssemblyLineStrictJSON.exact(
            raw, ["schema_version", "request_id", "resulting_state"]),
          AssemblyLineStrictJSON.uint(raw["schema_version"]) == 1,
          AssemblyLineStrictJSON.canonicalUUIDText(raw["request_id"])
            == AssemblyLineStrictJSON.canonicalUUIDText(request["request_id"]),
          let state = raw["resulting_state"] as? [String: Any],
          (try? AssemblyLineStrictJSON.validateState(state)) != nil,
          let expected = AssemblyLineStrictJSON.uint(request["expected_state_revision"]),
          expected < UInt64.max,
          AssemblyLineStrictJSON.uint(state["state_revision"]) == expected + 1,
          state["auto_run"] as? Bool == request["auto_run"] as? Bool
        else { throw AssemblywrightMacAssemblyLineError.invalidReceipt }
      }
    } catch let error as AssemblywrightMacAssemblyLineError {
      throw error == .invalidRequest ? .invalidReceipt : error
    } catch {
      throw AssemblywrightMacAssemblyLineError.invalidReceipt
    }
  }

  private static func fetchProjection(
    using session: any AssemblywrightMacBridgeSession
  ) async throws -> AssemblywrightMacAssemblyLineOwnerProjection {
    let response = try await session.send(
      .init(
        method: "GET",
        path: AssemblywrightMacBridgeSupervisor.assemblyLinePath
      ))
    guard response.status == 200, response.body.count <= maximumResponseBytes else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    return try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(response.body)
  }

  private static func requestLimit(
    for action: AssemblywrightMacAssemblyLinePlanningAction
  ) -> Int {
    switch action {
    case .projectDraft, .featureDraft: 16 * 1_024
    case .projectBrainstorm, .featureBrainstorm: maximumBrainstormingCloudRequestBytes
    default: maximumRequestBytes
    }
  }
}

private enum AssemblyLineStrictJSON {
  static let nilUUID = UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))

  static func object(_ data: Data) throws -> [String: Any] {
    guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
      throw AssemblywrightMacAssemblyLineError.invalidRequest
    }
    return object
  }

  static func exact(_ object: [String: Any], _ keys: [String]) -> Bool {
    Set(object.keys) == Set(keys)
  }

  static func uint(_ raw: Any?, positive: Bool = false, maximum: UInt64 = .max) -> UInt64? {
    guard let number = raw as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID() else {
      return nil
    }
    let text = number.stringValue
    guard let value = UInt64(text), String(value) == text, value <= maximum,
      !positive || value > 0
    else { return nil }
    return value
  }

  static func uuid(_ raw: Any?) -> UUID? {
    guard let text = raw as? String, text == text.lowercased(),
      let value = UUID(uuidString: text), value != nilUUID,
      value.uuidString.lowercased() == text
    else { return nil }
    return value
  }

  static func digest(_ raw: Any?, optional: Bool = false) -> [UInt8]? {
    if optional, raw is NSNull { return [] }
    guard let values = raw as? [Any], values.count == 32 else { return nil }
    let bytes = values.compactMap { value -> UInt8? in
      guard let integer = uint(value), integer <= 255 else { return nil }
      return UInt8(integer)
    }
    guard bytes.count == 32, bytes.contains(where: { $0 != 0 }) else { return nil }
    return bytes
  }

  static func canonicalUUIDText(_ raw: Any?) -> String? { uuid(raw)?.uuidString.lowercased() }

  static func canonicalGitHubURL(_ raw: Any?) -> String? {
    guard let object = raw as? [String: Any], exact(object, ["url"]),
      let candidate = object["url"] as? String, candidate.utf8.count <= 256,
      !candidate.contains(where: { $0.isNewline || $0.isWhitespace }),
      !candidate.contains(where: { "?#%\\".contains($0) }),
      candidate.hasPrefix("https://github.com/")
    else { return nil }
    let parts = candidate.dropFirst("https://github.com/".count).split(
      separator: "/", omittingEmptySubsequences: false)
    guard parts.count == 2,
      validGitHubName(String(parts[0]), maximum: 39, repository: false),
      validGitHubName(String(parts[1]), maximum: 100, repository: true),
      candidate == candidate.lowercased(), !candidate.hasSuffix(".git"),
      !candidate.hasSuffix("/")
    else { return nil }
    return candidate
  }

  static func validGitHubName(_ value: String, maximum: Int, repository: Bool) -> Bool {
    guard !value.isEmpty, value.utf8.count <= maximum,
      !value.hasPrefix("-"), !value.hasPrefix("."), !value.hasPrefix("_"),
      !value.hasSuffix("-"), !value.hasSuffix("."), !value.hasSuffix("_"),
      !value.contains("--")
    else { return false }
    return value.utf8.allSatisfy { byte in
      byte >= 0x30 && byte <= 0x39 || byte >= 0x61 && byte <= 0x7a
        || byte == 0x2d || repository && (byte == 0x5f || byte == 0x2e)
    }
  }

  static func validateRepositoryIdentity(_ raw: Any?) throws -> (UUID, String) {
    guard let object = raw as? [String: Any],
      exact(object, ["repository_id", "git_url"]),
      let id = uuid(object["repository_id"]),
      let url = canonicalGitHubURL(object["git_url"])
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    return (id, url)
  }

  static func validateProfile(_ raw: Any?, catalogRevision: UInt64) throws -> [String: Any] {
    guard let object = raw as? [String: Any],
      exact(object, ["configuration_revision", "provider_id", "model_id"]),
      uint(object["configuration_revision"], positive: true) == catalogRevision,
      validIdentifier(object["provider_id"], maximum: 128),
      validIdentifier(object["model_id"], maximum: 256)
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    return object
  }

  static func validIdentifier(_ raw: Any?, maximum: Int) -> Bool {
    guard let value = raw as? String, !value.isEmpty, value.utf8.count <= maximum,
      value == value.trimmingCharacters(in: .whitespacesAndNewlines),
      !value.contains(where: {
        $0.isWhitespace || $0.isNewline || $0.isASCII && $0.asciiValue! < 0x20
      })
    else { return false }
    return value.utf8.allSatisfy { byte in
      byte >= 0x30 && byte <= 0x39 || byte >= 0x41 && byte <= 0x5a
        || byte >= 0x61 && byte <= 0x7a || [0x2d, 0x2e, 0x5f].contains(byte)
    }
  }

  static func canonicalData(_ object: Any) throws -> Data {
    try JSONSerialization.data(
      withJSONObject: object, options: [.sortedKeys, .withoutEscapingSlashes])
  }

  static func sha256(_ object: Any) throws -> [UInt8] {
    Array(SHA256.hash(data: try canonicalData(object)))
  }

  static func validateCatalog(_ raw: Any?) throws -> AssemblywrightMacOrchestratorCatalog {
    guard let object = raw as? [String: Any],
      exact(
        object,
        [
          "schema_version", "catalog_revision", "profiles", "default_profile_sha256",
          "catalog_sha256",
        ]),
      uint(object["schema_version"]) == 1,
      let revision = uint(object["catalog_revision"], positive: true),
      let rawProfiles = object["profiles"] as? [Any],
      (1...64).contains(rawProfiles.count),
      let defaultDigest = digest(object["default_profile_sha256"]),
      let catalogDigest = digest(object["catalog_sha256"])
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    var previous: (String, String)?
    var defaultMatches = 0
    var defaultProfileIdentity: (String, String)?
    for rawProfile in rawProfiles {
      let profile = try validateProfile(rawProfile, catalogRevision: revision)
      let current = (profile["provider_id"] as! String, profile["model_id"] as! String)
      if let previous, previous.0 > current.0 || previous.0 == current.0 && previous.1 >= current.1
      {
        throw AssemblywrightMacAssemblyLineError.invalidProjection
      }
      if try sha256(profile) == defaultDigest {
        defaultMatches += 1
        defaultProfileIdentity = current
      }
      previous = current
    }
    var digestObject = object
    digestObject.removeValue(forKey: "catalog_sha256")
    guard defaultMatches == 1,
      defaultProfileIdentity?.0 == "openai.codex",
      defaultProfileIdentity?.1 == "gpt-5.6-sol",
      try sha256(digestObject) == catalogDigest
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    let data = try canonicalData(object)
    return try JSONDecoder().decode(AssemblywrightMacOrchestratorCatalog.self, from: data)
  }

  static func defaultProfile(
    in catalog: AssemblywrightMacOrchestratorCatalog
  ) throws -> AssemblywrightMacOrchestratorProfile {
    var matches: [AssemblywrightMacOrchestratorProfile] = []
    for profile in catalog.profiles {
      let raw = try object(JSONEncoder().encode(profile))
      if try sha256(raw) == catalog.defaultProfileSHA256 { matches.append(profile) }
    }
    guard matches.count == 1, let profile = matches.first,
      profile.providerID == "openai.codex", profile.modelID == "gpt-5.6-sol"
    else { throw AssemblywrightMacAssemblyLineError.invalidProjection }
    return profile
  }

  static func validateProjection(_ object: [String: Any]) throws {
    guard
      exact(
        object,
        [
          "schema_version", "owner_control_revision", "emergency_pause_revision",
          "emergency_paused", "orchestrator_catalog", "repositories", "queue", "assembly_line",
          "availability",
        ]),
      uint(object["schema_version"]) == 1,
      let ownerRevision = uint(object["owner_control_revision"], positive: true),
      ownerRevision > 0,
      let pauseRevision = uint(object["emergency_pause_revision"]),
      let paused = object["emergency_paused"] as? Bool,
      let repositories = object["repositories"] as? [Any], repositories.count <= 100,
      let queue = object["queue"] as? [Any], queue.count <= 100
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    _ = try validateCatalog(object["orchestrator_catalog"])
    var repositoryIDs = Set<UUID>()
    var createdIDs = Set<UUID>()
    var previousURL: String?
    for raw in repositories {
      let repository = try validateRepository(raw)
      if let previousURL,
        previousURL >= repository.url || !repositoryIDs.insert(repository.id).inserted
      {
        throw AssemblywrightMacAssemblyLineError.invalidProjection
      }
      if repository.created { createdIDs.insert(repository.id) }
      previousURL = repository.url
    }
    let state = try validateState(object["assembly_line"])
    guard state.queueCount == queue.count,
      state.queueRevision == uint((object["assembly_line"] as? [String: Any])?["queue_revision"])
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    var featureIDs = Set<UUID>()
    var activeCount = 0
    var activeEntry: (UUID, String)?
    for (index, raw) in queue.enumerated() {
      let entry = try validateQueueEntry(raw)
      guard entry.position == index + 1, featureIDs.insert(entry.featureID).inserted,
        createdIDs.contains(entry.repositoryID)
      else {
        throw AssemblywrightMacAssemblyLineError.invalidProjection
      }
      if entry.lifecycle != "queued" {
        activeCount += 1
        guard index == 0 else { throw AssemblywrightMacAssemblyLineError.invalidProjection }
        activeEntry = (entry.featureID, entry.lifecycle)
      }
    }
    let expectedActive =
      state.lifecycle == "stopped" || state.lifecycle == "waiting_for_owner_start" ? 0 : 1
    guard activeCount == expectedActive,
      activeEntry?.0 == state.activeFeatureID,
      activeEntry?.1 == expectedQueueLifecycle(state.lifecycle),
      !(state.lifecycle == "emergency_paused" && !paused),
      !(paused
        && !["stopped", "emergency_paused", "incomplete_termination"].contains(state.lifecycle))
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    try validateAvailability(object["availability"], stateLifecycle: state.lifecycle)
    _ = pauseRevision
  }

  static func validateRepository(_ raw: Any?) throws -> (id: UUID, url: String, created: Bool) {
    guard let object = raw as? [String: Any],
      exact(
        object,
        [
          "schema_version", "repository", "repository_revision", "lifecycle_revision", "visibility",
          "approved_specification_id", "approved_specification_revision",
          "approved_specification_sha256",
          "owner_approval_sha256", "lifecycle", "effect_possible", "creation_evidence_sha256",
        ]), uint(object["schema_version"]) == 1,
      let (id, url) = try? validateRepositoryIdentity(object["repository"]),
      uint(object["repository_revision"], positive: true) != nil,
      uint(object["lifecycle_revision"], positive: true) != nil,
      ["public", "private"].contains(object["visibility"] as? String ?? ""),
      uuid(object["approved_specification_id"]) != nil,
      uint(object["approved_specification_revision"], positive: true) != nil,
      digest(object["approved_specification_sha256"]) != nil,
      digest(object["owner_approval_sha256"]) != nil,
      let lifecycle = object["lifecycle"] as? String,
      let effect = object["effect_possible"] as? Bool
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    let evidence = digest(object["creation_evidence_sha256"], optional: true)
    guard evidence != nil else { throw AssemblywrightMacAssemblyLineError.invalidProjection }
    let valid: Bool
    switch lifecycle {
    case "creation_pending", "conflict", "failed": valid = !effect && evidence!.isEmpty
    case "reconciling", "reconciliation_required": valid = effect && evidence!.isEmpty
    case "created": valid = effect && !evidence!.isEmpty
    default: valid = false
    }
    guard valid else { throw AssemblywrightMacAssemblyLineError.invalidProjection }
    return (id, url, lifecycle == "created")
  }

  static func validateQueueEntry(_ raw: Any?) throws -> (
    featureID: UUID, repositoryID: UUID, position: Int, lifecycle: String
  ) {
    guard let object = raw as? [String: Any],
      exact(
        object,
        [
          "schema_version", "feature_id", "repository_id", "specification_id",
          "specification_revision",
          "specification_sha256", "owner_approval_sha256", "position", "lifecycle_revision",
          "lifecycle",
        ]), uint(object["schema_version"]) == 1,
      let featureID = uuid(object["feature_id"]), let repositoryID = uuid(object["repository_id"]),
      uuid(object["specification_id"]) != nil,
      uint(object["specification_revision"], positive: true) != nil,
      digest(object["specification_sha256"]) != nil, digest(object["owner_approval_sha256"]) != nil,
      let position = uint(object["position"], positive: true, maximum: 100),
      uint(object["lifecycle_revision"], positive: true) != nil,
      let lifecycle = object["lifecycle"] as? String,
      [
        "queued", "starting", "active", "stopping", "paused_at_checkpoint", "emergency_paused",
        "waiting_for_host_reconnect", "reconciliation_required", "incomplete_termination",
      ].contains(lifecycle)
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    return (featureID, repositoryID, Int(position), lifecycle)
  }

  static func validateState(_ raw: Any?) throws -> (
    queueCount: Int, queueRevision: UInt64, lifecycle: String, activeFeatureID: UUID?
  ) {
    guard let object = raw as? [String: Any],
      exact(
        object,
        [
          "schema_version", "state_revision", "queue_revision", "queue_count", "auto_run",
          "lifecycle",
          "session_id", "active_child_epoch_id", "active_feature_id",
        ]), uint(object["schema_version"]) == 1,
      uint(object["state_revision"], positive: true) != nil,
      let queueRevision = uint(object["queue_revision"]),
      let queueCount = uint(object["queue_count"], maximum: 100), object["auto_run"] is Bool,
      let lifecycle = object["lifecycle"] as? String,
      [
        "stopped", "starting", "running", "stopping", "paused_at_checkpoint", "emergency_paused",
        "waiting_for_host_reconnect", "reconciliation_required", "incomplete_termination",
        "waiting_for_owner_start",
      ].contains(lifecycle)
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    let session = optionalUUID(object["session_id"])
    let child = optionalUUID(object["active_child_epoch_id"])
    let feature = optionalUUID(object["active_feature_id"])
    guard session.valid, child.valid, feature.valid,
      (child.value != nil) == (feature.value != nil),
      !(queueCount > 0 && queueRevision == 0)
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    if ["stopped", "waiting_for_owner_start"].contains(lifecycle) {
      guard session.value == nil, child.value == nil else {
        throw AssemblywrightMacAssemblyLineError.invalidProjection
      }
    } else {
      guard session.value != nil, child.value != nil,
        lifecycle != "running" || queueCount > 0
      else { throw AssemblywrightMacAssemblyLineError.invalidProjection }
    }
    return (Int(queueCount), queueRevision, lifecycle, feature.value)
  }

  static func expectedQueueLifecycle(_ lifecycle: String) -> String? {
    switch lifecycle {
    case "stopped", "waiting_for_owner_start": nil
    case "starting": "starting"
    case "running": "active"
    case "stopping": "stopping"
    case "paused_at_checkpoint": "paused_at_checkpoint"
    case "emergency_paused": "emergency_paused"
    case "waiting_for_host_reconnect": "waiting_for_host_reconnect"
    case "reconciliation_required": "reconciliation_required"
    case "incomplete_termination": "incomplete_termination"
    default: nil
    }
  }

  static func optionalUUID(_ raw: Any?) -> (valid: Bool, value: UUID?) {
    if raw is NSNull { return (true, nil) }
    guard let value = uuid(raw) else { return (false, nil) }
    return (true, value)
  }

  static func validateAvailability(_ raw: Any?, stateLifecycle: String) throws {
    guard let object = raw as? [String: Any],
      exact(
        object,
        [
          "schema_version", "availability_revision", "observed_at_ms", "brainstorming_provider",
          "github_creation", "windows_executor", "mac_executor", "protected_brokers",
        ]), uint(object["schema_version"]) == 1,
      uint(object["availability_revision"], positive: true) != nil,
      uint(object["observed_at_ms"], positive: true) != nil
    else {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
    var executionUnavailable = false
    for key in [
      "brainstorming_provider", "github_creation", "windows_executor", "mac_executor",
      "protected_brokers",
    ] {
      guard let component = object[key] as? [String: Any],
        exact(component, ["binding_revision", "binding_sha256", "status", "unavailable_reason"]),
        uint(component["binding_revision"], positive: true) != nil,
        digest(component["binding_sha256"]) != nil,
        let status = component["status"] as? String,
        ["available", "unavailable"].contains(status)
      else {
        throw AssemblywrightMacAssemblyLineError.invalidProjection
      }
      if status == "available" {
        guard component["unavailable_reason"] is NSNull else {
          throw AssemblywrightMacAssemblyLineError.invalidProjection
        }
      } else {
        guard let reason = component["unavailable_reason"] as? String,
          [
            "not_configured", "not_authenticated", "disconnected", "unhealthy", "emergency_paused",
            "identity_drift", "evidence_required",
          ].contains(reason)
        else {
          throw AssemblywrightMacAssemblyLineError.invalidProjection
        }
        if ["windows_executor", "mac_executor", "protected_brokers"].contains(key) {
          executionUnavailable = true
        }
      }
    }
    if executionUnavailable && ["starting", "running", "stopping"].contains(stateLifecycle) {
      throw AssemblywrightMacAssemblyLineError.invalidProjection
    }
  }

  static func decodeRequest(_ data: Data, action: AssemblywrightMacAssemblyLinePlanningAction)
    throws -> [String: Any]
  {
    do {
      var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
      try scanner.validateNoDuplicateObjectKeysRecursively()
      return try object(data)
    } catch { throw AssemblywrightMacAssemblyLineError.invalidRequest }
  }

  static func validateRequest(
    _ request: [String: Any],
    action: AssemblywrightMacAssemblyLinePlanningAction,
    against projection: AssemblywrightMacAssemblyLineOwnerProjection
  ) throws {
    guard uint(request["schema_version"]) == 1 else {
      throw AssemblywrightMacAssemblyLineError.invalidRequest
    }
    if action == .projectBrainstorm || action == .featureBrainstorm {
      let draft = try validateCloudBrainstormRequest(request, action: action)
      let persistenceAction: AssemblywrightMacAssemblyLinePlanningAction =
        action == .projectBrainstorm ? .projectDraft : .featureDraft
      try validateRequest(draft, action: persistenceAction, against: projection)
      guard !projection.emergencyPaused,
        projection.availability.brainstormingProvider.status == .available,
        projection.availability.brainstormingProvider.unavailableReason == nil,
        let profile = draft["orchestrator"] as? [String: Any],
        let selected = try? defaultProfile(in: projection.orchestratorCatalog),
        selected.providerID == profile["provider_id"] as? String,
        selected.modelID == profile["model_id"] as? String
      else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
      return
    }
    let projectionObject = try object(JSONEncoder().encode(projection))
    let catalogObject = projectionObject["orchestrator_catalog"] as? [String: Any]
    switch action {
    case .projectDraft, .featureDraft:
      let expected =
        action == .projectDraft
        ? [
          "schema_version", "draft_id", "draft_revision", "repository", "visibility",
          "orchestrator_catalog", "orchestrator", "idea",
        ]
        : [
          "schema_version", "draft_id", "draft_revision", "repository",
          "expected_repository_revision", "orchestrator_catalog", "orchestrator", "idea",
        ]
      guard exact(request, expected), uuid(request["draft_id"]) != nil,
        uint(request["draft_revision"], positive: true) != nil,
        (try? validateRepositoryIdentity(request["repository"])) != nil,
        let requestCatalog = request["orchestrator_catalog"] as? [String: Any],
        let catalogObject, try canonicalData(requestCatalog) == canonicalData(catalogObject),
        let profile = request["orchestrator"] as? [String: Any],
        (try? validateProfile(
          profile, catalogRevision: projection.orchestratorCatalog.catalogRevision)) != nil,
        projection.orchestratorCatalog.profiles.contains(where: {
          $0.providerID == profile["provider_id"] as? String
            && $0.modelID == profile["model_id"] as? String
            && $0.configurationRevision == uint(profile["configuration_revision"])
        }), validPlanningText(request["idea"], maximum: 16 * 1_024)
      else {
        throw AssemblywrightMacAssemblyLineError.invalidRequest
      }
      if action == .projectDraft {
        guard ["public", "private"].contains(request["visibility"] as? String ?? "") else {
          throw AssemblywrightMacAssemblyLineError.invalidRequest
        }
      } else {
        guard let expectedRevision = uint(request["expected_repository_revision"], positive: true),
          let (repositoryID, _) = try? validateRepositoryIdentity(request["repository"]),
          projection.repositories.contains(where: {
            $0.repository.repositoryID == repositoryID && $0.repositoryRevision == expectedRevision
              && $0.lifecycle == .created
          })
        else {
          throw AssemblywrightMacAssemblyLineError.invalidRequest
        }
      }
    case .projectBrainstorm, .featureBrainstorm:
      throw AssemblywrightMacAssemblyLineError.invalidRequest
    case .frozenSpecification:
      try validateFrozenRequest(request, catalog: projection.orchestratorCatalog)
    case .projectApproval, .featureApproval:
      try validateApprovalRequest(request, action: action, projection: projection)
    case .repositoryCreation:
      guard exact(request, ["schema_version", "repository_id"]),
        let repositoryID = uuid(request["repository_id"]),
        !projection.emergencyPaused,
        projection.availability.githubCreation.status == .available,
        projection.availability.githubCreation.unavailableReason == nil,
        projection.repositories.contains(where: {
          $0.repository.repositoryID == repositoryID
            && [.creationPending, .reconciling, .reconciliationRequired, .created]
              .contains($0.lifecycle)
        })
      else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
    case .autoRun:
      guard
        exact(request, ["schema_version", "request_id", "expected_state_revision", "auto_run"]),
        uuid(request["request_id"]) != nil,
        let expected = uint(request["expected_state_revision"], positive: true),
        let requested = request["auto_run"] as? Bool,
        expected == projection.assemblyLine.stateRevision
          || expected < UInt64.max
            && expected + 1 == projection.assemblyLine.stateRevision
            && requested == projection.assemblyLine.autoRun
      else {
        throw AssemblywrightMacAssemblyLineError.invalidRequest
      }
    }
  }

  static func validateCloudBrainstormRequest(
    _ request: [String: Any],
    action: AssemblywrightMacAssemblyLinePlanningAction
  ) throws -> [String: Any] {
    guard action == .projectBrainstorm || action == .featureBrainstorm,
      exact(
        request,
        [
          "schema_version", "draft", "information_classification",
          "owner_cloud_disclosure_sha256",
        ]
      ),
      uint(request["schema_version"]) == 1,
      request["information_classification"] as? String == "public",
      let draft = request["draft"] as? [String: Any],
      let profile = draft["orchestrator"] as? [String: Any],
      let providerID = profile["provider_id"] as? String,
      let modelID = profile["model_id"] as? String,
      let catalog = draft["orchestrator_catalog"] as? [String: Any],
      let catalogRevision = uint(catalog["catalog_revision"], positive: true),
      let catalogSHA256 = digest(catalog["catalog_sha256"]),
      let supplied = digest(request["owner_cloud_disclosure_sha256"])
    else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
    let binding: [String: Any] = [
      "schema_version": 1,
      "target_kind": action == .projectBrainstorm ? "project" : "feature",
      "draft_sha256": try sha256(draft),
      "information_classification": "public",
      "provider_id": providerID,
      "model_id": modelID,
      "orchestrator_catalog_revision": catalogRevision,
      "orchestrator_catalog_sha256": catalogSHA256,
      "orchestrator_profile_sha256": try sha256(profile),
    ]
    var preimage = Data("assemblywright.owner-cloud-disclosure.v1\0".utf8)
    preimage.append(try canonicalData(binding))
    guard Array(SHA256.hash(data: preimage)) == supplied else {
      throw AssemblywrightMacAssemblyLineError.invalidRequest
    }
    try validateStandaloneDraft(
      draft,
      action: action == .projectBrainstorm ? .projectDraft : .featureDraft
    )
    return draft
  }

  static func validateStandaloneDraft(
    _ request: [String: Any],
    action: AssemblywrightMacAssemblyLinePlanningAction
  ) throws {
    let expected: [String]
    switch action {
    case .projectDraft:
      expected = [
        "schema_version", "draft_id", "draft_revision", "repository", "visibility",
        "orchestrator_catalog", "orchestrator", "idea",
      ]
    case .featureDraft:
      expected = [
        "schema_version", "draft_id", "draft_revision", "repository",
        "expected_repository_revision", "orchestrator_catalog", "orchestrator", "idea",
      ]
    default:
      throw AssemblywrightMacAssemblyLineError.invalidRequest
    }
    do {
      guard exact(request, expected), uint(request["schema_version"]) == 1,
        uuid(request["draft_id"]) != nil,
        uint(request["draft_revision"], positive: true) != nil,
        (try? validateRepositoryIdentity(request["repository"])) != nil,
        let catalogObject = request["orchestrator_catalog"] as? [String: Any],
        let profileObject = request["orchestrator"] as? [String: Any]
      else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
      let catalog = try validateCatalog(catalogObject)
      _ = try validateProfile(profileObject, catalogRevision: catalog.catalogRevision)
      guard catalog.profiles.contains(where: {
        $0.providerID == profileObject["provider_id"] as? String
          && $0.modelID == profileObject["model_id"] as? String
          && $0.configurationRevision == uint(profileObject["configuration_revision"])
      }), validPlanningText(request["idea"], maximum: 16 * 1_024)
      else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
      if action == .projectDraft {
        guard ["public", "private"].contains(request["visibility"] as? String ?? "") else {
          throw AssemblywrightMacAssemblyLineError.invalidRequest
        }
      } else {
        guard uint(request["expected_repository_revision"], positive: true) != nil else {
          throw AssemblywrightMacAssemblyLineError.invalidRequest
        }
      }
    } catch {
      throw AssemblywrightMacAssemblyLineError.invalidRequest
    }
  }

  static func validPlanningText(_ raw: Any?, maximum: Int) -> Bool {
    guard let text = raw as? String, !text.isEmpty, text.utf8.count <= maximum,
      text == text.trimmingCharacters(in: .whitespacesAndNewlines),
      !text.contains(where: {
        $0.isASCII && ($0.asciiValue! < 0x20 && ![0x09, 0x0a, 0x0d].contains($0.asciiValue!))
      })
    else { return false }
    return !isPathOrSecretShaped(text)
  }

  static func isPathOrSecretShaped(_ value: String) -> Bool {
    let lower = value.lowercased()
    let compact = lower.filter { !$0.isWhitespace }
    if lower.contains("-----begin ")
      || compact.contains("authorization:bearer")
      || compact.contains("authorization:basic")
      || containsSensitiveAssignment(lower)
      || containsPattern(lower, #"(?:^|[^a-z0-9_-])github_pat_[a-z0-9_-]{8,}"#)
      || containsPattern(lower, #"(?:^|[^a-z0-9_-])gh[pousr]_[a-z0-9_-]{16,}"#)
      || containsPattern(lower, #"(?:^|[^a-z0-9_-])sk-[a-z0-9_-]{17,}"#)
      || containsPattern(lower, #"akia[a-z0-9]{16}"#)
      || containsEmbeddedJWT(value)
      || containsPattern(value, #"://[^\s/?#]*@"#)
    {
      return true
    }
    let pathMarkers = [
      "/users/", "/home/", "/.ssh/", "~/.ssh", "/etc/", "/var/", "/private/", "/tmp/",
      "/opt/", "/usr/", "/root/", "file://", "ssh://", "git://", "git@",
    ]
    return pathMarkers.contains(where: compact.contains)
      || compact.hasSuffix("/users") || compact.hasSuffix("/home")
      || compact.contains("\\\\")
      || containsPattern(compact, #"[a-z]:[\\/]"#)
  }

  static func containsSensitiveAssignment(_ lower: String) -> Bool {
    let sensitive = [
      "password", "token", "secret", "apikey", "clientsecret", "accesstoken", "apitoken",
    ]
    let characters = Array(lower)
    for separator in characters.indices
    where characters[separator] == ":" || characters[separator] == "=" {
      var start = separator
      while start > characters.startIndex {
        let candidate = characters.index(before: start)
        let character = characters[candidate]
        guard character.isASCII,
          character.isLetter || character.isNumber || character.isWhitespace
            || ["_", "-", "."].contains(character)
        else { break }
        start = candidate
      }
      let normalized = characters[start..<separator]
        .filter { $0.isASCII && ($0.isLetter || $0.isNumber) }
        .map(String.init)
        .joined()
      if sensitive.contains(where: normalized.hasSuffix) { return true }
    }
    return false
  }

  static func containsEmbeddedJWT(_ value: String) -> Bool {
    guard
      let range = value.range(
        of: #"eyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+"#,
        options: .regularExpression
      )
    else { return false }
    return value[range].utf8.count >= 32
  }

  static func containsPattern(_ value: String, _ pattern: String) -> Bool {
    value.range(of: pattern, options: .regularExpression) != nil
  }

  static func containsSensitiveString(_ raw: Any) -> Bool {
    if let value = raw as? String {
      if canonicalGitHubURL(["url": value] as [String: Any]) != nil { return false }
      return isPathOrSecretShaped(value)
    }
    if let values = raw as? [Any] { return values.contains(where: containsSensitiveString) }
    if let object = raw as? [String: Any] {
      return object.values.contains(where: containsSensitiveString)
    }
    return false
  }

  static func validateFrozenRequest(
    _ request: [String: Any], catalog: AssemblywrightMacOrchestratorCatalog
  ) throws {
    guard
      exact(
        request,
        [
          "schema_version", "specification_id", "specification_revision", "target_kind", "draft_id",
          "draft_revision", "draft_sha256", "repository", "visibility",
          "orchestrator_catalog_revision", "orchestrator_catalog_sha256",
          "orchestrator_profile_sha256", "specification", "specification_sha256",
        ]),
      uuid(request["specification_id"]) != nil, uuid(request["draft_id"]) != nil,
      uint(request["specification_revision"], positive: true) != nil,
      uint(request["draft_revision"], positive: true) != nil,
      digest(request["draft_sha256"]) != nil,
      (try? validateRepositoryIdentity(request["repository"])) != nil,
      uint(request["orchestrator_catalog_revision"], positive: true) == catalog.catalogRevision,
      digest(request["orchestrator_catalog_sha256"]) == catalog.catalogSHA256,
      digest(request["orchestrator_profile_sha256"]) != nil,
      let target = request["target_kind"] as? String, ["project", "feature"].contains(target),
      target == "project"
        ? ["public", "private"].contains(request["visibility"] as? String ?? "")
        : request["visibility"] is NSNull,
      let specification = request["specification"] as? [String: Any],
      exact(specification, ["title", "outcome", "acceptance_criteria", "obligations"]),
      validPlanningText(specification["title"], maximum: 256),
      validPlanningText(specification["outcome"], maximum: 8 * 1_024),
      let acceptance = specification["acceptance_criteria"] as? [[String: Any]],
      (1...64).contains(acceptance.count),
      let obligations = specification["obligations"] as? [String],
      (1...64).contains(obligations.count),
      obligations.allSatisfy({ validPlanningText($0, maximum: 2 * 1_024) }),
      Set(obligations).count == obligations.count,
      let specificationDigest = digest(request["specification_sha256"]),
      try sha256(specification) == specificationDigest
    else {
      throw AssemblywrightMacAssemblyLineError.invalidRequest
    }
    var ids = Set<String>()
    for criterion in acceptance {
      guard exact(criterion, ["id", "requirement"]), validIdentifier(criterion["id"], maximum: 128),
        let id = criterion["id"] as? String, ids.insert(id).inserted,
        validPlanningText(criterion["requirement"], maximum: 4 * 1_024)
      else {
        throw AssemblywrightMacAssemblyLineError.invalidRequest
      }
    }
  }

  static func validateApprovalRequest(
    _ request: [String: Any], action: AssemblywrightMacAssemblyLinePlanningAction,
    projection: AssemblywrightMacAssemblyLineOwnerProjection
  ) throws {
    guard
      exact(
        request,
        [
          "schema_version", "approval_id", "approved_at_ms", "owner_control_revision",
          "target_kind", "repository", "visibility", "expected_repository_revision",
          "expected_queue_revision", "draft_id", "draft_revision", "draft_sha256",
          "orchestrator_catalog_revision", "orchestrator_catalog_sha256", "specification_id",
          "specification_revision", "specification_sha256", "orchestrator_profile_sha256",
          "owner_approval_sha256",
        ]),
      uuid(request["approval_id"]) != nil, uuid(request["draft_id"]) != nil,
      uuid(request["specification_id"]) != nil,
      uint(request["approved_at_ms"], positive: true) != nil,
      let requestOwnerRevision = uint(request["owner_control_revision"], positive: true),
      requestOwnerRevision == projection.ownerControlRevision
        || requestOwnerRevision < UInt64.max
          && requestOwnerRevision + 1 == projection.ownerControlRevision,
      uint(request["draft_revision"], positive: true) != nil,
      uint(request["specification_revision"], positive: true) != nil,
      digest(request["draft_sha256"]) != nil, digest(request["specification_sha256"]) != nil,
      digest(request["orchestrator_profile_sha256"]) != nil,
      uint(request["orchestrator_catalog_revision"], positive: true)
        == projection.orchestratorCatalog.catalogRevision,
      digest(request["orchestrator_catalog_sha256"])
        == projection.orchestratorCatalog.catalogSHA256,
      (try? validateRepositoryIdentity(request["repository"])) != nil,
      !projection.emergencyPaused
    else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
    if action == .projectApproval {
      guard request["target_kind"] as? String == "project",
        ["public", "private"].contains(request["visibility"] as? String ?? ""),
        uint(request["expected_repository_revision"]) == 0,
        request["expected_queue_revision"] is NSNull,
        projection.availability.githubCreation.status == .available,
        projection.availability.githubCreation.unavailableReason == nil
      else { throw AssemblywrightMacAssemblyLineError.invalidRequest }
    } else {
      guard request["target_kind"] as? String == "feature", request["visibility"] is NSNull,
        uint(request["expected_repository_revision"], positive: true) != nil,
        let expectedQueue = uint(request["expected_queue_revision"]),
        expectedQueue == projection.assemblyLine.queueRevision
          || expectedQueue < UInt64.max
            && expectedQueue + 1 == projection.assemblyLine.queueRevision
      else {
        throw AssemblywrightMacAssemblyLineError.invalidRequest
      }
    }
    var approval = request
    let supplied = digest(approval.removeValue(forKey: "owner_approval_sha256"))
    guard supplied != nil, try sha256(approval) == supplied else {
      throw AssemblywrightMacAssemblyLineError.invalidRequest
    }
  }

  static func decodeFrozenResponse(
    _ data: Data,
    matchingDraft draftData: Data,
    projection: AssemblywrightMacAssemblyLineOwnerProjection
  ) throws -> AssemblywrightMacFrozenBrainstormingSpecification {
    let envelope = try decodeRequest(draftData, action: .projectBrainstorm)
    let action: AssemblywrightMacAssemblyLinePlanningAction =
      ((envelope["draft"] as? [String: Any])?["visibility"] == nil)
      ? .featureBrainstorm : .projectBrainstorm
    let draft = try validateCloudBrainstormRequest(envelope, action: action)
    guard let rawCatalog = draft["orchestrator_catalog"] as? [String: Any],
      try canonicalData(rawCatalog)
        == canonicalData(object(JSONEncoder().encode(projection.orchestratorCatalog)))
    else { throw AssemblywrightMacAssemblyLineError.invalidReceipt }
    return try decodeFrozenResponseWithoutProjection(data, matchingDraft: draftData)
  }

  static func decodeFrozenResponseWithoutProjection(
    _ data: Data,
    matchingDraft draftData: Data
  ) throws -> AssemblywrightMacFrozenBrainstormingSpecification {
    do {
      guard !data.isEmpty,
        data.count <= AssemblywrightMacAssemblyLineOwnerControl.maximumResponseBytes
      else { throw AssemblywrightMacAssemblyLineError.invalidReceipt }
      var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
      try scanner.validateNoDuplicateObjectKeysRecursively()
      let envelope = try object(draftData)
      let action: AssemblywrightMacAssemblyLinePlanningAction =
        ((envelope["draft"] as? [String: Any])?["visibility"] == nil)
        ? .featureBrainstorm : .projectBrainstorm
      let draft = try validateCloudBrainstormRequest(envelope, action: action)
      let raw = try object(data)
      guard let catalogRaw = draft["orchestrator_catalog"] as? [String: Any] else {
        throw AssemblywrightMacAssemblyLineError.invalidReceipt
      }
      let catalog = try validateCatalog(catalogRaw)
      try validateFrozenRequest(raw, catalog: catalog)
      let project = draft["visibility"] != nil
      guard let rawRepository = raw["repository"] as? [String: Any],
        let draftRepository = draft["repository"] as? [String: Any],
        canonicalUUIDText(raw["draft_id"]) == canonicalUUIDText(draft["draft_id"]),
        uint(raw["draft_revision"]) == uint(draft["draft_revision"]),
        digest(raw["draft_sha256"]) == (try sha256(draft)),
        try canonicalData(rawRepository) == canonicalData(draftRepository),
        raw["target_kind"] as? String == (project ? "project" : "feature"),
        project
          ? raw["visibility"] as? String == draft["visibility"] as? String
          : raw["visibility"] is NSNull,
        uint(raw["orchestrator_catalog_revision"])
          == uint(catalogRaw["catalog_revision"]),
        digest(raw["orchestrator_catalog_sha256"])
          == digest(catalogRaw["catalog_sha256"]),
        let profile = draft["orchestrator"] as? [String: Any],
        digest(raw["orchestrator_profile_sha256"]) == (try sha256(profile))
      else { throw AssemblywrightMacAssemblyLineError.invalidReceipt }
      return try JSONDecoder().decode(
        AssemblywrightMacFrozenBrainstormingSpecification.self,
        from: data
      )
    } catch let error as AssemblywrightMacAssemblyLineError {
      throw error == .invalidRequest ? .invalidReceipt : error
    } catch {
      throw AssemblywrightMacAssemblyLineError.invalidReceipt
    }
  }

  static func errorDisposition(
    status: Int,
    data: Data,
    action: AssemblywrightMacAssemblyLinePlanningAction,
    prior: AssemblywrightMacAssemblyLineOwnerProjection,
    request: [String: Any]
  ) throws -> AssemblywrightMacAssemblyLineError {
    guard data.count <= 1_024 else { return .ambiguous }
    var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
    guard (try? scanner.validateNoDuplicateObjectKeysRecursively()) != nil,
      let body = try? object(data), exact(body, ["error"]),
      let code = body["error"] as? String, code.utf8.count <= 64
    else { return .ambiguous }
    switch (status, code) {
    case (401, "unauthorized"), (413, "payload_too_large"),
      (422, "assembly_line_request_rejected"):
      return .rejected
    case (409, "brainstorming_rejected")
      where action == .projectBrainstorm || action == .featureBrainstorm:
      return .rejected
    case (503, "planning_runtime_unavailable")
      where action == .projectBrainstorm || action == .featureBrainstorm:
      return .rejected
    case (409, "github_creation_conflict") where action == .repositoryCreation:
      return .rejected
    case (503, "github_creation_unavailable") where action == .repositoryCreation:
      if let repositoryID = uuid(request["repository_id"]),
        prior.repositories.contains(where: {
          $0.repository.repositoryID == repositoryID
            && $0.lifecycle == .creationPending && !$0.effectPossible
        })
      {
        return .rejected
      }
      return .ambiguous
    case (409, "brainstorming_reconciliation_required"),
      (409, "github_creation_reconciliation_required"):
      return .ambiguous
    default:
      return .ambiguous
    }
  }

  static func validateResponse(
    _ data: Data, action: AssemblywrightMacAssemblyLinePlanningAction, request: [String: Any],
    prior: AssemblywrightMacAssemblyLineOwnerProjection,
    post: AssemblywrightMacAssemblyLineOwnerProjection
  ) throws {
    switch action {
    case .projectDraft, .featureDraft, .frozenSpecification:
      let response = try AssemblywrightMacAssemblyLineOwnerProjection.decodeStrict(data)
      guard projectionEquivalentExceptObservation(response, post),
        projectionPlanningMutation(from: prior, to: post)
      else {
        throw AssemblywrightMacAssemblyLineError.invalidReceipt
      }
    case .projectBrainstorm, .featureBrainstorm:
      _ = try decodeFrozenResponse(data, matchingDraft: try canonicalData(request), projection: prior)
      let freshRevision = prior.ownerControlRevision <= UInt64.max - 2
        ? prior.ownerControlRevision + 2 : nil
      guard post.ownerControlRevision == prior.ownerControlRevision
          || post.ownerControlRevision == freshRevision,
        post.emergencyPauseRevision == prior.emergencyPauseRevision,
        post.emergencyPaused == prior.emergencyPaused,
        post.orchestratorCatalog == prior.orchestratorCatalog,
        post.repositories == prior.repositories,
        post.queue == prior.queue,
        post.assemblyLine == prior.assemblyLine,
        post.availability.schemaVersion == prior.availability.schemaVersion,
        post.availability.availabilityRevision == prior.availability.availabilityRevision,
        post.availability.brainstormingProvider == prior.availability.brainstormingProvider,
        post.availability.githubCreation == prior.availability.githubCreation,
        post.availability.windowsExecutor == prior.availability.windowsExecutor,
        post.availability.macExecutor == prior.availability.macExecutor,
        post.availability.protectedBrokers == prior.availability.protectedBrokers
      else { throw AssemblywrightMacAssemblyLineError.invalidReceipt }
    case .projectApproval:
      var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
      try scanner.validateNoDuplicateObjectKeysRecursively()
      let raw = try object(data)
      let receipt = try validateRepository(raw)
      let typed = try JSONDecoder().decode(
        AssemblywrightMacRepositoryCreationProjection.self, from: data)
      guard let (repositoryID, url) = try? validateRepositoryIdentity(request["repository"]),
        receipt.id == repositoryID, receipt.url == url, !receipt.created,
        typed.repositoryRevision == 1, typed.lifecycleRevision == 1,
        typed.visibility.rawValue == request["visibility"] as? String,
        typed.approvedSpecificationID == uuid(request["specification_id"]),
        typed.approvedSpecificationRevision == uint(request["specification_revision"]),
        typed.approvedSpecificationSHA256 == digest(request["specification_sha256"]),
        typed.ownerApprovalSHA256 == digest(request["owner_approval_sha256"]),
        typed.lifecycle == .creationPending, !typed.effectPossible,
        typed.creationEvidenceSHA256 == nil,
        post.repositories.contains(typed)
      else {
        throw AssemblywrightMacAssemblyLineError.invalidReceipt
      }
    case .featureApproval:
      var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
      try scanner.validateNoDuplicateObjectKeysRecursively()
      let raw = try object(data)
      let receipt = try validateQueueEntry(raw)
      let typed = try JSONDecoder().decode(
        AssemblywrightMacFeatureQueueEntryProjection.self, from: data)
      let repositoryID = (try? validateRepositoryIdentity(request["repository"]))?.0
      guard let id = uuid(request["approval_id"]), receipt.featureID == id,
        receipt.position <= post.queue.count,
        typed.repositoryID == repositoryID,
        typed.specificationID == uuid(request["specification_id"]),
        typed.specificationRevision == uint(request["specification_revision"]),
        typed.specificationSHA256 == digest(request["specification_sha256"]),
        typed.ownerApprovalSHA256 == digest(request["owner_approval_sha256"]),
        typed.lifecycleRevision == 1, typed.lifecycle == .queued,
        post.queue.contains(typed)
      else {
        throw AssemblywrightMacAssemblyLineError.invalidReceipt
      }
    case .repositoryCreation:
      var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
      try scanner.validateNoDuplicateObjectKeysRecursively()
      let raw = try object(data)
      let receipt = try validateRepository(raw)
      let typed = try JSONDecoder().decode(
        AssemblywrightMacRepositoryCreationProjection.self,
        from: data
      )
      guard receipt.id == uuid(request["repository_id"]), post.repositories.contains(typed)
      else { throw AssemblywrightMacAssemblyLineError.invalidReceipt }
    case .autoRun:
      var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
      try scanner.validateNoDuplicateObjectKeysRecursively()
      let raw = try object(data)
      guard exact(raw, ["schema_version", "request_id", "resulting_state"]),
        uint(raw["schema_version"]) == 1,
        canonicalUUIDText(raw["request_id"]) == canonicalUUIDText(request["request_id"]),
        let stateRaw = raw["resulting_state"] as? [String: Any],
        (try? validateState(stateRaw)) != nil,
        let expected = uint(request["expected_state_revision"]), expected < UInt64.max,
        uint(stateRaw["state_revision"]) == expected + 1,
        stateRaw["auto_run"] as? Bool == request["auto_run"] as? Bool,
        post.assemblyLine.stateRevision == expected + 1,
        post.assemblyLine.autoRun == request["auto_run"] as? Bool
      else {
        throw AssemblywrightMacAssemblyLineError.invalidReceipt
      }
    }
  }

  static func projectionPlanningMutation(
    from prior: AssemblywrightMacAssemblyLineOwnerProjection,
    to post: AssemblywrightMacAssemblyLineOwnerProjection
  ) -> Bool {
    let nextRevision =
      prior.ownerControlRevision == UInt64.max
      ? nil : prior.ownerControlRevision + 1
    guard
      post.ownerControlRevision == prior.ownerControlRevision
        || post.ownerControlRevision == nextRevision,
      post.emergencyPauseRevision == prior.emergencyPauseRevision,
      post.emergencyPaused == prior.emergencyPaused,
      post.orchestratorCatalog == prior.orchestratorCatalog,
      post.repositories == prior.repositories, post.queue == prior.queue,
      post.assemblyLine == prior.assemblyLine
    else { return false }
    return post.availability.schemaVersion == prior.availability.schemaVersion
      && post.availability.availabilityRevision == prior.availability.availabilityRevision
      && post.availability.brainstormingProvider == prior.availability.brainstormingProvider
      && post.availability.githubCreation == prior.availability.githubCreation
      && post.availability.windowsExecutor == prior.availability.windowsExecutor
      && post.availability.macExecutor == prior.availability.macExecutor
      && post.availability.protectedBrokers == prior.availability.protectedBrokers
  }

  static func projectionEquivalentExceptObservation(
    _ left: AssemblywrightMacAssemblyLineOwnerProjection,
    _ right: AssemblywrightMacAssemblyLineOwnerProjection
  ) -> Bool {
    left.schemaVersion == right.schemaVersion
      && left.ownerControlRevision == right.ownerControlRevision
      && left.emergencyPauseRevision == right.emergencyPauseRevision
      && left.emergencyPaused == right.emergencyPaused
      && left.orchestratorCatalog == right.orchestratorCatalog
      && left.repositories == right.repositories
      && left.queue == right.queue
      && left.assemblyLine == right.assemblyLine
      && left.availability.schemaVersion == right.availability.schemaVersion
      && left.availability.availabilityRevision == right.availability.availabilityRevision
      && left.availability.brainstormingProvider == right.availability.brainstormingProvider
      && left.availability.githubCreation == right.availability.githubCreation
      && left.availability.windowsExecutor == right.availability.windowsExecutor
      && left.availability.macExecutor == right.availability.macExecutor
      && left.availability.protectedBrokers == right.availability.protectedBrokers
  }
}
