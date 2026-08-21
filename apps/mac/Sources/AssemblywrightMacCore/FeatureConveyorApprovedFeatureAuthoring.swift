import CryptoKit
import Foundation

public enum AssemblywrightMacApprovedFeatureAuthoringError: Error, Equatable, Sendable {
    case invalidDraft
    case invalidAuthenticatedStatus
    case requestTooLarge
    case invalidReceipt
}

public struct AssemblywrightMacApprovedFeatureManifest: Equatable, Sendable {
    public let acceptance: [String]
    public let outcome: String
    public let title: String?
    public let scope: String?
    public let allowedPaths: [String]
    public let assumptions: [String]
    public let risks: [String]
    public let nonGoals: [String]
    public let decisions: [String]
    public let requiredCapabilities: [String]
    public let unitTestObligations: [String]
    public let e2eScenarios: [String]
    public let documentationObligations: [String]
    public let knowledgeBaseObligations: [String]
    public let prohibitedData: [String]
    public let publicationChecks: [String]
    public let baseBranch: String?
    public let securityClassification: String?
    public let mergeStrategy: String?
    public let postMergeGate: String?

    public init(
        acceptance: [String],
        outcome: String,
        title: String? = nil,
        scope: String? = nil,
        allowedPaths: [String] = [],
        assumptions: [String] = [],
        risks: [String] = [],
        nonGoals: [String] = [],
        decisions: [String] = [],
        requiredCapabilities: [String] = [],
        unitTestObligations: [String] = [],
        e2eScenarios: [String] = [],
        documentationObligations: [String] = [],
        knowledgeBaseObligations: [String] = [],
        prohibitedData: [String] = [],
        publicationChecks: [String] = [],
        baseBranch: String? = nil,
        securityClassification: String? = nil,
        mergeStrategy: String? = nil,
        postMergeGate: String? = nil
    ) {
        self.acceptance = acceptance
        self.outcome = outcome
        self.title = title
        self.scope = scope
        self.allowedPaths = allowedPaths
        self.assumptions = assumptions
        self.risks = risks
        self.nonGoals = nonGoals
        self.decisions = decisions
        self.requiredCapabilities = requiredCapabilities
        self.unitTestObligations = unitTestObligations
        self.e2eScenarios = e2eScenarios
        self.documentationObligations = documentationObligations
        self.knowledgeBaseObligations = knowledgeBaseObligations
        self.prohibitedData = prohibitedData
        self.publicationChecks = publicationChecks
        self.baseBranch = baseBranch
        self.securityClassification = securityClassification
        self.mergeStrategy = mergeStrategy
        self.postMergeGate = postMergeGate
    }
}

public struct AssemblywrightMacFeatureConveyorGrantRevisions: Equatable, Sendable {
    public let registration: UInt64
    public let cloudDisclosure: UInt64
    public let autonomousPublication: UInt64

    public init(
        registration: UInt64,
        cloudDisclosure: UInt64,
        autonomousPublication: UInt64
    ) {
        self.registration = registration
        self.cloudDisclosure = cloudDisclosure
        self.autonomousPublication = autonomousPublication
    }
}

public struct AssemblywrightMacApprovedFeaturePreparedRequest: Equatable, Sendable {
    public let draft: AssemblywrightMacFeatureConveyorApprovedFeatureDraft
    public let deviceID: String
    public let connectionEpoch: UInt64
    public let expectedQueueRevision: UInt64
    public let ownerControlDesignationRevision: UInt64
    public let emergencyPaused: Bool
    public let emergencyPauseRevision: UInt64
    public let exactRequestSHA256: [UInt8]
    let requestData: Data

    init(
        draft: AssemblywrightMacFeatureConveyorApprovedFeatureDraft,
        deviceID: String,
        connectionEpoch: UInt64,
        expectedQueueRevision: UInt64,
        ownerControlDesignationRevision: UInt64,
        emergencyPaused: Bool,
        emergencyPauseRevision: UInt64,
        requestData: Data
    ) {
        self.draft = draft
        self.deviceID = deviceID
        self.connectionEpoch = connectionEpoch
        self.expectedQueueRevision = expectedQueueRevision
        self.ownerControlDesignationRevision = ownerControlDesignationRevision
        self.emergencyPaused = emergencyPaused
        self.emergencyPauseRevision = emergencyPauseRevision
        self.requestData = requestData
        exactRequestSHA256 = Array(SHA256.hash(data: requestData))
    }
}

public struct AssemblywrightMacApprovedFeaturePendingRecovery: Equatable, Sendable {
    public let preparedRequest: AssemblywrightMacApprovedFeaturePreparedRequest

    public var draft: AssemblywrightMacFeatureConveyorApprovedFeatureDraft {
        preparedRequest.draft
    }

    public var exactRequestSHA256: [UInt8] {
        preparedRequest.exactRequestSHA256
    }

    var requestData: Data {
        preparedRequest.requestData
    }

    init(preparedRequest: AssemblywrightMacApprovedFeaturePreparedRequest) {
        self.preparedRequest = preparedRequest
    }
}

public struct AssemblywrightMacFeatureConveyorApprovedFeatureDraft: Equatable, Sendable {
    public static let maximumManifestBytes = 256 * 1_024
    public static let maximumRequestBytes = 320 * 1_024
    public static let maximumDependencies = 100
    public static let expectedProviderID = "openai.codex"
    public static let expectedModelID = "gpt-5.6-sol"

    public let featureID: UUID
    public let repositoryID: UUID
    public let specificationRevision: UInt64
    public let manifest: AssemblywrightMacApprovedFeatureManifest
    public let designSHA256: [UInt8]
    public let brainstormingSHA256: [UInt8]
    public let ownerApprovalSHA256: [UInt8]
    public let grants: AssemblywrightMacFeatureConveyorGrantRevisions
    public let providerID: String
    public let modelID: String
    public let dependencies: [UUID]

    public init(
        featureID: UUID,
        repositoryID: UUID,
        specificationRevision: UInt64,
        manifest: AssemblywrightMacApprovedFeatureManifest,
        designSHA256: [UInt8],
        brainstormingSHA256: [UInt8],
        ownerApprovalSHA256: [UInt8],
        grants: AssemblywrightMacFeatureConveyorGrantRevisions,
        providerID: String,
        modelID: String,
        dependencies: [UUID] = []
    ) {
        self.featureID = featureID
        self.repositoryID = repositoryID
        self.specificationRevision = specificationRevision
        self.manifest = manifest
        self.designSHA256 = designSHA256
        self.brainstormingSHA256 = brainstormingSHA256
        self.ownerApprovalSHA256 = ownerApprovalSHA256
        self.grants = grants
        self.providerID = providerID
        self.modelID = modelID
        self.dependencies = dependencies
    }

    /// Deterministic client construction only. The Windows master remains the sole
    /// authority that recomputes and accepts the canonical manifest digest.
    public func canonicalManifestData() throws -> Data {
        try validate()
        let data = try Self.encode(EncodedManifest(manifest))
        guard !data.isEmpty, data.count <= Self.maximumManifestBytes else {
            throw AssemblywrightMacApprovedFeatureAuthoringError.requestTooLarge
        }
        return data
    }

    public func manifestSHA256() throws -> [UInt8] {
        Array(SHA256.hash(data: try canonicalManifestData()))
    }

    public func encodeRequest(
        from status: AssemblywrightDeveloperBridgeAppStatus
    ) throws -> Data {
        guard status.phase == .connected,
              let featureConveyor = status.featureConveyor,
              let ownerControl = status.ownerControl,
              !ownerControl.emergencyPaused,
              featureConveyor.queueRevision == ownerControl.queueRevision else {
            throw AssemblywrightMacApprovedFeatureAuthoringError.invalidAuthenticatedStatus
        }
        let manifestData = try canonicalManifestData()
        let manifestSHA256 = Array(SHA256.hash(data: manifestData))
        let request = EncodedRequest(
            schemaVersion: 1,
            expectedQueueRevision: ownerControl.queueRevision,
            ownerControlDesignationRevision: ownerControl.ownerControlDesignationRevision,
            emergencyPauseRevision: ownerControl.emergencyPauseRevision,
            specification: EncodedSpecification(
                featureID: Self.uuidString(featureID),
                revision: specificationRevision,
                repositoryID: Self.uuidString(repositoryID),
                manifest: EncodedManifest(manifest),
                manifestSHA256: manifestSHA256,
                designSHA256: designSHA256,
                brainstormingSHA256: brainstormingSHA256,
                ownerApprovalSHA256: ownerApprovalSHA256,
                grants: EncodedGrants(grants),
                providerID: providerID,
                modelID: modelID,
                dependencies: dependencies.map(Self.uuidString)
            )
        )
        let data = try Self.encode(request)
        guard !data.isEmpty, data.count <= Self.maximumRequestBytes else {
            throw AssemblywrightMacApprovedFeatureAuthoringError.requestTooLarge
        }
        return data
    }

    public func prepareRequest(
        from status: AssemblywrightDeveloperBridgeAppStatus
    ) throws -> AssemblywrightMacApprovedFeaturePreparedRequest {
        guard let deviceID = status.deviceID,
              UUID(uuidString: deviceID)?.uuidString.lowercased() == deviceID,
              let connectionEpoch = status.connectionEpoch, connectionEpoch > 0,
              let ownerControl = status.ownerControl else {
            throw AssemblywrightMacApprovedFeatureAuthoringError.invalidAuthenticatedStatus
        }
        let requestData = try encodeRequest(from: status)
        return AssemblywrightMacApprovedFeaturePreparedRequest(
            draft: self,
            deviceID: deviceID,
            connectionEpoch: connectionEpoch,
            expectedQueueRevision: ownerControl.queueRevision,
            ownerControlDesignationRevision: ownerControl.ownerControlDesignationRevision,
            emergencyPaused: ownerControl.emergencyPaused,
            emergencyPauseRevision: ownerControl.emergencyPauseRevision,
            requestData: requestData
        )
    }

    public static func validateCommandReceipt(
        _ receiptData: Data,
        requestData: Data
    ) throws -> AssemblywrightMacFeatureConveyorApprovedFeatureReceipt {
        guard !receiptData.isEmpty,
              receiptData.count <= AssemblywrightMacFeatureConveyorOwnerControl.maximumReceiptBytes,
              !requestData.isEmpty,
              requestData.count <= Self.maximumRequestBytes else {
            throw AssemblywrightMacApprovedFeatureAuthoringError.invalidReceipt
        }
        do {
            var requestScanner = AssemblywrightStrictJSONObjectKeyScanner(data: requestData)
            try requestScanner.validateNoDuplicateObjectKeysRecursively()
            var receiptScanner = AssemblywrightStrictJSONObjectKeyScanner(data: receiptData)
            try receiptScanner.validateNoDuplicateObjectKeysRecursively()
        } catch {
            throw AssemblywrightMacApprovedFeatureAuthoringError.invalidReceipt
        }
        guard let request = try? JSONSerialization.jsonObject(with: requestData) as? [String: Any],
              Set(request.keys) == Set([
                "schema_version", "expected_queue_revision",
                "owner_control_designation_revision", "emergency_pause_revision", "specification"
              ]),
              strictUInt(request["schema_version"]) == 1,
              let expectedQueueRevision = strictUInt(request["expected_queue_revision"]),
              let designationRevision = strictUInt(request["owner_control_designation_revision"]),
              designationRevision > 0,
              let pauseRevision = strictUInt(request["emergency_pause_revision"]),
              let specification = request["specification"] as? [String: Any],
              Set(specification.keys) == Set([
                "feature_id", "revision", "repository_id", "manifest", "manifest_sha256",
                "design_sha256", "brainstorming_sha256", "owner_approval_sha256", "grants",
                "provider_id", "model_id", "dependencies"
              ]),
              let featureID = strictUUID(specification["feature_id"]),
              let specificationRevision = strictUInt(specification["revision"]),
              specificationRevision > 0,
              expectedQueueRevision != UInt64.max,
              let receiptObject = try? JSONSerialization.jsonObject(with: receiptData) as? [String: Any],
              Set(receiptObject.keys) == Set([
                "schema_version", "feature_id", "specification_revision", "lifecycle_revision",
                "queue_revision", "owner_control_designation_revision",
                "emergency_pause_revision", "status"
              ]),
              strictUInt(receiptObject["schema_version"]) == 1,
              strictUUID(receiptObject["feature_id"]) == featureID,
              strictUInt(receiptObject["specification_revision"]) == specificationRevision,
              strictUInt(receiptObject["lifecycle_revision"]) == 1,
              strictUInt(receiptObject["queue_revision"]) == expectedQueueRevision + 1,
              strictUInt(receiptObject["owner_control_designation_revision"])
                == designationRevision,
              strictUInt(receiptObject["emergency_pause_revision"]) == pauseRevision,
              receiptObject["status"] as? String == "queued",
              let receipt = try? JSONDecoder().decode(
                AssemblywrightMacFeatureConveyorApprovedFeatureReceipt.self,
                from: receiptData
              ) else {
            throw AssemblywrightMacApprovedFeatureAuthoringError.invalidReceipt
        }
        return receipt
    }

    private func validate() throws {
        guard featureID != Self.nilUUID, repositoryID != Self.nilUUID,
              specificationRevision > 0,
              Self.validDigest(designSHA256), Self.validDigest(brainstormingSHA256),
              Self.validDigest(ownerApprovalSHA256),
              grants.registration > 0, grants.cloudDisclosure > 0,
              grants.autonomousPublication > 0,
              providerID == Self.expectedProviderID,
              modelID == Self.expectedModelID,
              dependencies.count <= Self.maximumDependencies,
              dependencies.allSatisfy({ $0 != Self.nilUUID && $0 != featureID }),
              Set(dependencies).count == dependencies.count,
              !manifest.acceptance.isEmpty, manifest.acceptance.count <= 256,
              Set(manifest.acceptance).count == manifest.acceptance.count,
              manifest.acceptance.allSatisfy({
                Self.validIdentifier($0) && Self.validText($0)
              }),
              Self.validText(manifest.outcome),
              Self.validOptionalText(manifest.title), Self.validOptionalText(manifest.scope),
              Self.validTextArray(manifest.allowedPaths, maximumCount: 256),
              Self.validTextArray(manifest.assumptions),
              Self.validTextArray(manifest.risks),
              Self.validTextArray(manifest.nonGoals),
              Self.validTextArray(manifest.decisions),
              Self.validTextArray(manifest.requiredCapabilities),
              Self.validTextArray(manifest.unitTestObligations),
              Self.validTextArray(manifest.e2eScenarios),
              Self.validTextArray(manifest.documentationObligations),
              Self.validTextArray(manifest.knowledgeBaseObligations),
              Self.validTextArray(manifest.prohibitedData),
              Self.validTextArray(manifest.publicationChecks),
              Self.validOptionalText(manifest.baseBranch),
              Self.validOptionalText(manifest.securityClassification),
              Self.validOptionalText(manifest.mergeStrategy),
              Self.validOptionalText(manifest.postMergeGate) else {
            throw AssemblywrightMacApprovedFeatureAuthoringError.invalidDraft
        }
    }

    private static let nilUUID = UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))

    private static func encode<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return try encoder.encode(value)
    }

    private static func uuidString(_ value: UUID) -> String {
        value.uuidString.lowercased()
    }

    private static func validDigest(_ value: [UInt8]) -> Bool {
        value.count == 32 && value.contains(where: { $0 != 0 })
    }

    private static func validIdentifier(_ value: String) -> Bool {
        !value.isEmpty && value.utf8.count <= 128 && value.utf8.allSatisfy {
            (0x21 ... 0x7e).contains($0) && $0 != 0x22 && $0 != 0x5c
        }
    }

    private static func validOptionalText(_ value: String?) -> Bool {
        value.map(validText) ?? true
    }

    private static func validTextArray(_ values: [String], maximumCount: Int = 128) -> Bool {
        values.count <= maximumCount && values.allSatisfy(validText)
    }

    private static func validText(_ value: String) -> Bool {
        guard !value.isEmpty, value.utf8.count <= 4_096,
              value == value.trimmingCharacters(in: .whitespacesAndNewlines),
              value.unicodeScalars.allSatisfy({
                !CharacterSet.controlCharacters.contains($0)
              }),
              !isSecretShaped(value) else { return false }
        return true
    }

    private static func isSecretShaped(_ value: String) -> Bool {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        let lower = trimmed.lowercased()
        if lower.contains("-----begin ") || lower.contains("bearer ")
            || lower.contains("basic ") || trimmed.contains("ghp_")
            || trimmed.contains("github_pat_")
            || trimmed.range(
                of: #"sk-[A-Za-z0-9_-]{17,}"#,
                options: .regularExpression
            ) != nil
            || trimmed.range(
                of: #"AKIA[A-Z0-9]{16}"#,
                options: .regularExpression
            ) != nil
            || Self.containsEmbeddedJWT(in: trimmed) {
            return true
        }
        return trimmed.range(
            of: #"://[^\s/?#]*@"#,
            options: .regularExpression
        ) != nil
    }

    private static func containsEmbeddedJWT(in value: String) -> Bool {
        guard let range = value.range(
            of: #"eyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+"#,
            options: .regularExpression
        ) else { return false }
        return value[range].utf8.count >= 32
    }
}

private struct EncodedValidationGate: Encodable {
    let schemaVersion: UInt64 = 1
    let commandIDs = [
        "requirements_binding", "coverage", "focused_unit_tests", "native_e2e",
        "documentation", "knowledge_base", "formatting", "lint", "build", "safety",
        "changed_paths", "secret_scan", "repository_validation"
    ]

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case commandIDs = "command_ids"
    }
}

private struct EncodedManifest: Encodable {
    let acceptance: [String]
    let outcome: String
    let title: String?
    let scope: String?
    let allowedPaths: [String]
    let validationGate = EncodedValidationGate()
    let assumptions: [String]
    let risks: [String]
    let nonGoals: [String]
    let decisions: [String]
    let requiredCapabilities: [String]
    let unitTestObligations: [String]
    let e2eScenarios: [String]
    let documentationObligations: [String]
    let knowledgeBaseObligations: [String]
    let prohibitedData: [String]
    let publicationChecks: [String]
    let baseBranch: String?
    let securityClassification: String?
    let mergeStrategy: String?
    let postMergeGate: String?

    init(_ value: AssemblywrightMacApprovedFeatureManifest) {
        acceptance = value.acceptance
        outcome = value.outcome
        title = value.title
        scope = value.scope
        allowedPaths = value.allowedPaths
        assumptions = value.assumptions
        risks = value.risks
        nonGoals = value.nonGoals
        decisions = value.decisions
        requiredCapabilities = value.requiredCapabilities
        unitTestObligations = value.unitTestObligations
        e2eScenarios = value.e2eScenarios
        documentationObligations = value.documentationObligations
        knowledgeBaseObligations = value.knowledgeBaseObligations
        prohibitedData = value.prohibitedData
        publicationChecks = value.publicationChecks
        baseBranch = value.baseBranch
        securityClassification = value.securityClassification
        mergeStrategy = value.mergeStrategy
        postMergeGate = value.postMergeGate
    }

    enum CodingKeys: String, CodingKey {
        case acceptance, outcome, title, scope, assumptions, risks, decisions
        case allowedPaths = "allowed_paths"
        case validationGate = "validation_gate"
        case nonGoals = "non_goals"
        case requiredCapabilities = "required_capabilities"
        case unitTestObligations = "unit_test_obligations"
        case e2eScenarios = "e2e_scenarios"
        case documentationObligations = "documentation_obligations"
        case knowledgeBaseObligations = "knowledge_base_obligations"
        case prohibitedData = "prohibited_data"
        case publicationChecks = "publication_checks"
        case baseBranch = "base_branch"
        case securityClassification = "security_classification"
        case mergeStrategy = "merge_strategy"
        case postMergeGate = "post_merge_gate"
    }
}

private struct EncodedGrants: Encodable {
    let registration: UInt64
    let cloudDisclosure: UInt64
    let autonomousPublication: UInt64

    init(_ value: AssemblywrightMacFeatureConveyorGrantRevisions) {
        registration = value.registration
        cloudDisclosure = value.cloudDisclosure
        autonomousPublication = value.autonomousPublication
    }

    enum CodingKeys: String, CodingKey {
        case registration
        case cloudDisclosure = "cloud_disclosure"
        case autonomousPublication = "autonomous_publication"
    }
}

private struct EncodedSpecification: Encodable {
    let featureID: String
    let revision: UInt64
    let repositoryID: String
    let manifest: EncodedManifest
    let manifestSHA256: [UInt8]
    let designSHA256: [UInt8]
    let brainstormingSHA256: [UInt8]
    let ownerApprovalSHA256: [UInt8]
    let grants: EncodedGrants
    let providerID: String
    let modelID: String
    let dependencies: [String]

    enum CodingKeys: String, CodingKey {
        case featureID = "feature_id"
        case revision
        case repositoryID = "repository_id"
        case manifest
        case manifestSHA256 = "manifest_sha256"
        case designSHA256 = "design_sha256"
        case brainstormingSHA256 = "brainstorming_sha256"
        case ownerApprovalSHA256 = "owner_approval_sha256"
        case grants
        case providerID = "provider_id"
        case modelID = "model_id"
        case dependencies
    }
}

private struct EncodedRequest: Encodable {
    let schemaVersion: UInt64
    let expectedQueueRevision: UInt64
    let ownerControlDesignationRevision: UInt64
    let emergencyPauseRevision: UInt64
    let specification: EncodedSpecification

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case expectedQueueRevision = "expected_queue_revision"
        case ownerControlDesignationRevision = "owner_control_designation_revision"
        case emergencyPauseRevision = "emergency_pause_revision"
        case specification
    }
}

private func strictUInt(_ value: Any?) -> UInt64? {
    guard let number = value as? NSNumber,
          CFGetTypeID(number) != CFBooleanGetTypeID() else { return nil }
    let text = number.stringValue
    guard let result = UInt64(text), String(result) == text else { return nil }
    return result
}

private func strictUUID(_ value: Any?) -> UUID? {
    guard let text = value as? String, text == text.lowercased(),
          let uuid = UUID(uuidString: text), uuid != UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)) else {
        return nil
    }
    return uuid
}
