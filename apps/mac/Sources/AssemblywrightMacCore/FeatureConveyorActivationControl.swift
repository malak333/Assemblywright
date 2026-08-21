import Foundation

public enum AssemblywrightMacFeatureConveyorActivationStatus: String, Codable, Sendable {
    case inactive, active
}

public enum AssemblywrightMacFeatureConveyorActivationBlocker: String, Codable, Sendable {
    case none
    case emergencyPaused = "emergency_paused"
    case evidenceRequired = "evidence_required"
    case alreadyActivated = "already_activated"
}

public enum AssemblywrightMacFeatureConveyorOrchestrationStage: String, Codable, Sendable {
    case implementing, validating, reviewing, publishing
    case verifyingMain = "verifying_main"
    case repairing, paused
    case attentionRequired = "attention_required"
    case failed, succeeded, quarantined
}

public struct AssemblywrightMacFeatureConveyorEvidenceReference: Codable, Equatable, Sendable {
    public let evidenceID: UUID
    public let revision: UInt64
    public let receiptSHA256: [UInt8]

    enum CodingKeys: String, CodingKey, CaseIterable {
        case evidenceID = "evidence_id"
        case revision
        case receiptSHA256 = "receipt_sha256"
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(evidenceID.uuidString.lowercased(), forKey: .evidenceID)
        try container.encode(revision, forKey: .revision)
        try container.encode(receiptSHA256, forKey: .receiptSHA256)
    }
}

public struct AssemblywrightMacFeatureConveyorActivationEvidenceProjection: Codable, Equatable, Sendable {
    public let repositoryGateProof: AssemblywrightMacFeatureConveyorEvidenceReference?
    public let restrictedWorkerLive: AssemblywrightMacFeatureConveyorEvidenceReference?
    public let reviewProviderLive: AssemblywrightMacFeatureConveyorEvidenceReference?
    public let githubPublicationLive: AssemblywrightMacFeatureConveyorEvidenceReference?
    public let restartRecoveryLive: AssemblywrightMacFeatureConveyorEvidenceReference?
    public let macWindowsControlEventStreamingLive: AssemblywrightMacFeatureConveyorEvidenceReference?

    enum CodingKeys: String, CodingKey, CaseIterable {
        case repositoryGateProof = "repository_gate_proof"
        case restrictedWorkerLive = "restricted_worker_live"
        case reviewProviderLive = "review_provider_live"
        case githubPublicationLive = "github_publication_live"
        case restartRecoveryLive = "restart_recovery_live"
        case macWindowsControlEventStreamingLive = "mac_windows_control_event_streaming_live"
    }

    public var readyCount: Int { references.compactMap { $0 }.count }
    public var isComplete: Bool { readyCount == references.count }
    public var referencesForPresentation: [AssemblywrightMacFeatureConveyorEvidenceReference?] { references }
    fileprivate var references: [AssemblywrightMacFeatureConveyorEvidenceReference?] {
        [repositoryGateProof, restrictedWorkerLive, reviewProviderLive, githubPublicationLive,
         restartRecoveryLive, macWindowsControlEventStreamingLive]
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        for (key, value) in zip(CodingKeys.allCases, references) {
            if let value { try container.encode(value, forKey: key) }
            else { try container.encodeNil(forKey: key) }
        }
    }
}

public struct AssemblywrightMacFeatureConveyorOwnerActiveFeature: Codable, Equatable, Sendable {
    public let featureID: UUID
    public let specificationRevision: UInt64
    public let lifecycleRevision: UInt64
    public let lifecycleStatus: AssemblywrightMacFeatureConveyorLifecycleStatus
    public let orchestrationRevision: UInt64
    public let stage: AssemblywrightMacFeatureConveyorOrchestrationStage
    public let ownerPaused: Bool

    enum CodingKeys: String, CodingKey, CaseIterable {
        case featureID = "feature_id"
        case specificationRevision = "specification_revision"
        case lifecycleRevision = "lifecycle_revision"
        case lifecycleStatus = "lifecycle_status"
        case orchestrationRevision = "orchestration_revision"
        case stage
        case ownerPaused = "owner_paused"
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(featureID.uuidString.lowercased(), forKey: .featureID)
        try container.encode(specificationRevision, forKey: .specificationRevision)
        try container.encode(lifecycleRevision, forKey: .lifecycleRevision)
        try container.encode(lifecycleStatus, forKey: .lifecycleStatus)
        try container.encode(orchestrationRevision, forKey: .orchestrationRevision)
        try container.encode(stage, forKey: .stage)
        try container.encode(ownerPaused, forKey: .ownerPaused)
    }
}

public struct AssemblywrightMacFeatureConveyorOwnerControlProjection: Codable, Equatable, Sendable {
    public static let expectedSchemaVersion: UInt64 = 1
    public let schemaVersion: UInt64
    public let queueRevision: UInt64
    public let emergencyPaused: Bool
    public let emergencyPauseRevision: UInt64
    public let ownerControlDesignationRevision: UInt64
    public let activationStatus: AssemblywrightMacFeatureConveyorActivationStatus
    public let activationID: UUID?
    public let activationReady: Bool
    public let activationBlocker: AssemblywrightMacFeatureConveyorActivationBlocker
    public let activeFeature: AssemblywrightMacFeatureConveyorOwnerActiveFeature?
    public let evidence: AssemblywrightMacFeatureConveyorActivationEvidenceProjection

    enum CodingKeys: String, CodingKey, CaseIterable {
        case schemaVersion = "schema_version"
        case queueRevision = "queue_revision"
        case emergencyPaused = "emergency_paused"
        case emergencyPauseRevision = "emergency_pause_revision"
        case ownerControlDesignationRevision = "owner_control_designation_revision"
        case activationStatus = "activation_status"
        case activationID = "activation_id"
        case activationReady = "activation_ready"
        case activationBlocker = "activation_blocker"
        case activeFeature = "active_feature"
        case evidence
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(schemaVersion, forKey: .schemaVersion)
        try container.encode(queueRevision, forKey: .queueRevision)
        try container.encode(emergencyPaused, forKey: .emergencyPaused)
        try container.encode(emergencyPauseRevision, forKey: .emergencyPauseRevision)
        try container.encode(ownerControlDesignationRevision, forKey: .ownerControlDesignationRevision)
        try container.encode(activationStatus, forKey: .activationStatus)
        if let activationID {
            try container.encode(activationID.uuidString.lowercased(), forKey: .activationID)
        }
        else { try container.encodeNil(forKey: .activationID) }
        try container.encode(activationReady, forKey: .activationReady)
        try container.encode(activationBlocker, forKey: .activationBlocker)
        if let activeFeature { try container.encode(activeFeature, forKey: .activeFeature) }
        else { try container.encodeNil(forKey: .activeFeature) }
        try container.encode(evidence, forKey: .evidence)
    }

    public static func decodeStrict(_ data: Data) throws -> Self {
        guard !data.isEmpty, data.count <= 8 * 1_024 else { throw ControlError.invalidProjection }
        do {
            var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
            try scanner.validateNoDuplicateObjectKeysRecursively()
        } catch { throw ControlError.invalidProjection }
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys) == Set(CodingKeys.allCases.map(\.rawValue)),
              let value = try? JSONDecoder().decode(Self.self, from: data),
              value.validate(object: object) else { throw ControlError.invalidProjection }
        return value
    }

    private func validate(object: [String: Any]) -> Bool {
        guard schemaVersion == Self.expectedSchemaVersion,
              strictUInt(object["schema_version"]) == schemaVersion,
              strictUInt(object["queue_revision"]) == queueRevision,
              strictBool(object["emergency_paused"]) == emergencyPaused,
              strictUInt(object["emergency_pause_revision"]) == emergencyPauseRevision,
              strictUInt(object["owner_control_designation_revision"]) == ownerControlDesignationRevision,
              ownerControlDesignationRevision > 0,
              object["activation_status"] as? String == activationStatus.rawValue,
              strictBool(object["activation_ready"]) == activationReady,
              object["activation_blocker"] as? String == activationBlocker.rawValue,
              validOptionalUUID(object["activation_id"], expected: activationID),
              validEvidence(object["evidence"]),
              validActiveFeature(object["active_feature"]) else { return false }
        let isActive = activationStatus == .active
        guard isActive == (activationID != nil),
              activationReady == (!isActive && !emergencyPaused && evidence.isComplete) else { return false }
        let expected: AssemblywrightMacFeatureConveyorActivationBlocker = isActive ? .alreadyActivated
            : emergencyPaused ? .emergencyPaused : evidence.isComplete ? .none : .evidenceRequired
        return activationBlocker == expected
    }

    private func validEvidence(_ raw: Any?) -> Bool {
        guard let object = raw as? [String: Any],
              Set(object.keys) == Set(AssemblywrightMacFeatureConveyorActivationEvidenceProjection.CodingKeys.allCases.map(\.rawValue)) else { return false }
        return zip(AssemblywrightMacFeatureConveyorActivationEvidenceProjection.CodingKeys.allCases, evidence.references)
            .allSatisfy { key, value in validEvidenceReference(object[key.rawValue], expected: value) }
    }

    private func validEvidenceReference(_ raw: Any?, expected: AssemblywrightMacFeatureConveyorEvidenceReference?) -> Bool {
        if raw is NSNull { return expected == nil }
        guard let expected, let object = raw as? [String: Any],
              Set(object.keys) == Set(AssemblywrightMacFeatureConveyorEvidenceReference.CodingKeys.allCases.map(\.rawValue)),
              validUUID(object["evidence_id"], expected: expected.evidenceID),
              strictUInt(object["revision"]) == expected.revision, expected.revision > 0,
              validDigest(object["receipt_sha256"], expected: expected.receiptSHA256) else { return false }
        return true
    }

    private func validActiveFeature(_ raw: Any?) -> Bool {
        if raw is NSNull { return activeFeature == nil }
        guard let activeFeature, let object = raw as? [String: Any],
              Set(object.keys) == Set(AssemblywrightMacFeatureConveyorOwnerActiveFeature.CodingKeys.allCases.map(\.rawValue)),
              validUUID(object["feature_id"], expected: activeFeature.featureID),
              strictUInt(object["specification_revision"]) == activeFeature.specificationRevision,
              strictUInt(object["lifecycle_revision"]) == activeFeature.lifecycleRevision,
              object["lifecycle_status"] as? String == activeFeature.lifecycleStatus.rawValue,
              strictUInt(object["orchestration_revision"]) == activeFeature.orchestrationRevision,
              object["stage"] as? String == activeFeature.stage.rawValue,
              strictBool(object["owner_paused"]) == activeFeature.ownerPaused,
              activeFeature.specificationRevision > 0, activeFeature.lifecycleRevision > 0,
              !activeFeature.ownerPaused
                || (activeFeature.stage == .paused && activeFeature.orchestrationRevision > 0) else {
            return false
        }
        return true
    }
}

public enum ControlError: Error, Equatable, Sendable {
    case invalidProjection
    case invalidRequest
    case rejected
    case invalidReceipt
}

private func strictUInt(_ raw: Any?) -> UInt64? {
    guard let number = raw as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID() else { return nil }
    let text = number.stringValue
    guard let value = UInt64(text), String(value) == text else { return nil }
    return value
}

private func strictBool(_ raw: Any?) -> Bool? {
    guard let number = raw as? NSNumber, CFGetTypeID(number) == CFBooleanGetTypeID() else { return nil }
    return number.boolValue
}

private func validUUID(_ raw: Any?, expected: UUID) -> Bool {
    guard let text = raw as? String, text == text.lowercased(), UUID(uuidString: text) == expected,
          expected != UUID(uuid: (0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0)) else { return false }
    return true
}

private func validOptionalUUID(_ raw: Any?, expected: UUID?) -> Bool {
    if raw is NSNull { return expected == nil }
    guard let expected else { return false }
    return validUUID(raw, expected: expected)
}

private func validDigest(_ raw: Any?, expected: [UInt8]) -> Bool {
    guard expected.count == 32, expected.contains(where: { $0 != 0 }), let values = raw as? [Any], values.count == 32 else { return false }
    return zip(values, expected).allSatisfy { strictUInt($0.0) == UInt64($0.1) }
}

public enum AssemblywrightMacOwnerControlAction: String, CaseIterable, Sendable {
    case pause, resume
    case cancelActiveFeature = "cancel-active-feature"
    case abandonAndAdvance = "abandon-and-advance"
    case activation

    public var path: String {
        switch self {
        case .pause: "/v1/distributed/feature-conveyor/orchestration/pause"
        case .resume: "/v1/distributed/feature-conveyor/orchestration/resume"
        case .cancelActiveFeature: "/v1/distributed/feature-conveyor/cancel-active-feature"
        case .abandonAndAdvance: "/v1/distributed/feature-conveyor/abandon-and-advance"
        case .activation: "/v1/distributed/feature-conveyor/activation"
        }
    }
}

public enum AssemblywrightMacFeatureConveyorActivationControl {
    public static let maximumFrameBytes = 8 * 1_024

    public static func activationRequest(
        from projection: AssemblywrightMacFeatureConveyorOwnerControlProjection
    ) throws -> Data {
        guard projection.activationReady, projection.activationStatus == .inactive,
              !projection.emergencyPaused, projection.evidence.isComplete else {
            throw ControlError.invalidRequest
        }
        return try encode([
            "schema_version": 1,
            "expected_queue_revision": projection.queueRevision,
            "expected_owner_control_designation_revision": projection.ownerControlDesignationRevision,
            "expected_emergency_pause_revision": projection.emergencyPauseRevision,
            "evidence": evidenceObject(projection.evidence)
        ])
    }

    public static func orchestrationRequest(
        from projection: AssemblywrightMacFeatureConveyorOwnerControlProjection,
        action: AssemblywrightMacOwnerControlAction
    ) throws -> Data {
        guard action == .pause || action == .resume,
              !projection.emergencyPaused, let feature = projection.activeFeature,
              feature.orchestrationRevision > 0,
              (action == .pause
                ? !feature.ownerPaused && feature.stage != .paused
                : feature.ownerPaused) else {
            throw ControlError.invalidRequest
        }
        return try encode(baseFeatureRequest(projection, feature: feature).merging([
            "expected_orchestration_revision": feature.orchestrationRevision
        ]) { _, new in new })
    }

    public static func cancelRequest(
        from projection: AssemblywrightMacFeatureConveyorOwnerControlProjection
    ) throws -> Data {
        guard let feature = projection.activeFeature else { throw ControlError.invalidRequest }
        return try encode(baseFeatureRequest(projection, feature: feature))
    }

    public static func abandonRequest(
        from projection: AssemblywrightMacFeatureConveyorOwnerControlProjection,
        safeReconciliationSHA256: [UInt8],
        merged: Bool,
        verifiedHealthyMainSHA256: [UInt8]?
    ) throws -> Data {
        guard let feature = projection.activeFeature,
              [.cancelled, .quarantined, .attentionRequired, .failed].contains(feature.lifecycleStatus),
              safeReconciliationSHA256.count == 32,
              safeReconciliationSHA256.contains(where: { $0 != 0 }),
              verifiedHealthyMainSHA256.map({ $0.count == 32 && $0.contains(where: { $0 != 0 }) }) ?? true,
              !merged || verifiedHealthyMainSHA256 != nil else { throw ControlError.invalidRequest }
        var evidence: [String: Any] = [
            "safe_reconciliation_sha256": safeReconciliationSHA256,
            "merged": merged
        ]
        evidence["verified_healthy_main_sha256"] = verifiedHealthyMainSHA256 ?? NSNull()
        return try encode(baseFeatureRequest(projection, feature: feature).merging([
            "evidence": evidence
        ]) { _, new in new })
    }

    public static func perform(
        action: AssemblywrightMacOwnerControlAction,
        requestData: Data,
        using session: any AssemblywrightMacBridgeSession
    ) async throws -> Data {
        do {
            let request = try validateRequest(requestData, action: action)
            let response = try await session.send(.init(method: "POST", path: action.path, body: requestData))
            guard response.status == 200 else { throw ControlError.rejected }
            try validateReceipt(response.body, request: request, action: action)
            await session.cancel()
            return response.body
        } catch let error as ControlError {
            await session.cancel()
            throw error
        } catch {
            await session.cancel()
            throw ControlError.invalidRequest
        }
    }

    private static func baseFeatureRequest(
        _ projection: AssemblywrightMacFeatureConveyorOwnerControlProjection,
        feature: AssemblywrightMacFeatureConveyorOwnerActiveFeature
    ) -> [String: Any] {
        [
            "schema_version": 1,
            "feature_id": feature.featureID.uuidString.lowercased(),
            "expected_lifecycle_revision": feature.lifecycleRevision,
            "expected_queue_revision": projection.queueRevision,
            "expected_owner_control_designation_revision": projection.ownerControlDesignationRevision,
            "expected_emergency_pause_revision": projection.emergencyPauseRevision
        ]
    }

    private static func evidenceObject(
        _ projection: AssemblywrightMacFeatureConveyorActivationEvidenceProjection
    ) -> [String: Any] {
        let keys = AssemblywrightMacFeatureConveyorActivationEvidenceProjection.CodingKeys.allCases
        return Dictionary(uniqueKeysWithValues: zip(keys, projection.references).compactMap { key, reference in
            reference.map { (key.rawValue, referenceObject($0)) }
        })
    }

    private static func referenceObject(_ reference: AssemblywrightMacFeatureConveyorEvidenceReference) -> [String: Any] {
        ["evidence_id": reference.evidenceID.uuidString.lowercased(), "revision": reference.revision,
         "receipt_sha256": reference.receiptSHA256]
    }

    private static func encode(_ object: [String: Any]) throws -> Data {
        guard JSONSerialization.isValidJSONObject(object) else { throw ControlError.invalidRequest }
        let data = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        guard data.count <= maximumFrameBytes else { throw ControlError.invalidRequest }
        return data
    }

    private static func strictObject(_ data: Data, maximum: Int = maximumFrameBytes) throws -> [String: Any] {
        guard !data.isEmpty, data.count <= maximum else { throw ControlError.invalidRequest }
        var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
        try scanner.validateNoDuplicateObjectKeysRecursively()
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw ControlError.invalidRequest
        }
        return object
    }

    private static func validateRequest(_ data: Data, action: AssemblywrightMacOwnerControlAction) throws -> [String: Any] {
        let object = try strictObject(data)
        var keys = Set(["schema_version", "expected_queue_revision",
                        "expected_owner_control_designation_revision", "expected_emergency_pause_revision"])
        switch action {
        case .activation: keys.insert("evidence")
        case .pause, .resume:
            keys.formUnion(["feature_id", "expected_lifecycle_revision", "expected_orchestration_revision"])
        case .cancelActiveFeature:
            keys.formUnion(["feature_id", "expected_lifecycle_revision"])
        case .abandonAndAdvance:
            keys.formUnion(["feature_id", "expected_lifecycle_revision", "evidence"])
        }
        guard Set(object.keys) == keys, strictUInt(object["schema_version"]) == 1,
              strictUInt(object["expected_owner_control_designation_revision"]).map({ $0 > 0 }) == true,
              strictUInt(object["expected_queue_revision"]) != nil,
              strictUInt(object["expected_emergency_pause_revision"]) != nil else { throw ControlError.invalidRequest }
        if action != .activation {
            guard let text = object["feature_id"] as? String, text == text.lowercased(),
                  let id = UUID(uuidString: text), validUUID(text, expected: id),
                  strictUInt(object["expected_lifecycle_revision"]).map({ $0 > 0 }) == true else { throw ControlError.invalidRequest }
        }
        if action == .pause || action == .resume {
            guard strictUInt(object["expected_orchestration_revision"]).map({ $0 > 0 }) == true else { throw ControlError.invalidRequest }
        }
        if action == .activation {
            guard let evidence = object["evidence"] as? [String: Any], validateEvidenceSet(evidence) else { throw ControlError.invalidRequest }
        }
        if action == .abandonAndAdvance {
            guard let evidence = object["evidence"] as? [String: Any],
                  Set(evidence.keys) == Set(["safe_reconciliation_sha256", "merged", "verified_healthy_main_sha256"]),
                  let safe = evidence["safe_reconciliation_sha256"] as? [Any], digestIsValid(safe),
                  let merged = strictBool(evidence["merged"]),
                  evidence["verified_healthy_main_sha256"] is NSNull || digestIsValid(evidence["verified_healthy_main_sha256"] as? [Any]),
                  !merged || !(evidence["verified_healthy_main_sha256"] is NSNull) else { throw ControlError.invalidRequest }
        }
        return object
    }

    private static func validateEvidenceSet(_ object: [String: Any]) -> Bool {
        let keys = Set(AssemblywrightMacFeatureConveyorActivationEvidenceProjection.CodingKeys.allCases.map(\.rawValue))
        guard Set(object.keys) == keys else { return false }
        return object.values.allSatisfy { raw in
            guard let ref = raw as? [String: Any],
                  Set(ref.keys) == Set(["evidence_id", "revision", "receipt_sha256"]),
                  let idText = ref["evidence_id"] as? String, idText == idText.lowercased(),
                  let id = UUID(uuidString: idText), validUUID(idText, expected: id),
                  strictUInt(ref["revision"]).map({ $0 > 0 }) == true else { return false }
            return digestIsValid(ref["receipt_sha256"] as? [Any])
        }
    }

    private static func digestIsValid(_ values: [Any]?) -> Bool {
        guard let values, values.count == 32 else { return false }
        let parsed = values.compactMap(strictUInt)
        return parsed.count == 32 && parsed.allSatisfy { $0 <= 255 } && parsed.contains { $0 != 0 }
    }

    private static func validateReceipt(
        _ data: Data, request: [String: Any], action: AssemblywrightMacOwnerControlAction
    ) throws {
        let object: [String: Any]
        do { object = try strictObject(data) } catch { throw ControlError.invalidReceipt }
        let commonMatches = strictUInt(object["schema_version"]) == 1
            && strictUInt(object["queue_revision"]) != nil
            && strictUInt(object["emergency_pause_revision"]) == strictUInt(request["expected_emergency_pause_revision"])
        guard commonMatches else { throw ControlError.invalidReceipt }
        switch action {
        case .activation:
            guard Set(object.keys) == Set(["schema_version", "activation_id", "queue_revision",
                  "owner_control_designation_revision", "emergency_pause_revision", "evidence", "activated_at_ms", "status"]),
                  object["status"] as? String == "active",
                  strictUInt(object["queue_revision"]) == strictUInt(request["expected_queue_revision"]),
                  strictUInt(object["owner_control_designation_revision"]) == strictUInt(request["expected_owner_control_designation_revision"]),
                  strictUInt(object["activated_at_ms"]).map({ $0 > 0 }) == true,
                  let idText = object["activation_id"] as? String, let id = UUID(uuidString: idText), validUUID(idText, expected: id),
                  NSDictionary(dictionary: object["evidence"] as? [String: Any] ?? [:]).isEqual(to: request["evidence"] as? [AnyHashable: Any] ?? [:]) else { throw ControlError.invalidReceipt }
        case .pause, .resume:
            guard Set(object.keys) == Set(["schema_version", "feature_id", "lifecycle_revision", "orchestration_revision", "queue_revision", "owner_control_designation_revision", "emergency_pause_revision", "checkpoint_id", "checkpoint_sha256", "status"]),
                  object["feature_id"] as? String == request["feature_id"] as? String,
                  object["status"] as? String == (action == .pause ? "paused" : "resumed"),
                  strictUInt(object["lifecycle_revision"]) == strictUInt(request["expected_lifecycle_revision"]).map({ $0 + 1 }),
                  strictUInt(object["orchestration_revision"]) == strictUInt(request["expected_orchestration_revision"]).map({ $0 + 1 }),
                  strictUInt(object["queue_revision"]) == strictUInt(request["expected_queue_revision"]),
                  strictUInt(object["owner_control_designation_revision"]) == strictUInt(request["expected_owner_control_designation_revision"]),
                  (object["checkpoint_id"] as? String).map({ text in
                      text == text.lowercased() && UUID(uuidString: text).map({ validUUID(text, expected: $0) }) == true
                  }) == true,
                  digestIsValid(object["checkpoint_sha256"] as? [Any]) else { throw ControlError.invalidReceipt }
        case .cancelActiveFeature:
            guard Set(object.keys) == Set(["schema_version", "feature_id", "lifecycle_revision", "queue_revision", "emergency_pause_revision", "lease_retained", "advancement_authorized", "status"]),
                  object["feature_id"] as? String == request["feature_id"] as? String,
                  object["status"] as? String == "cancelled", strictBool(object["lease_retained"]) == true,
                  strictBool(object["advancement_authorized"]) == false,
                  strictUInt(object["lifecycle_revision"]) == strictUInt(request["expected_lifecycle_revision"]).map({ $0 + 1 }),
                  strictUInt(object["queue_revision"]) == strictUInt(request["expected_queue_revision"]) else { throw ControlError.invalidReceipt }
        case .abandonAndAdvance:
            guard Set(object.keys) == Set(["schema_version", "feature_id", "lifecycle_revision", "queue_revision", "emergency_pause_revision", "lease_released", "status"]),
                  object["feature_id"] as? String == request["feature_id"] as? String,
                  object["status"] as? String == "abandoned", strictBool(object["lease_released"]) == true,
                  strictUInt(object["lifecycle_revision"]) == strictUInt(request["expected_lifecycle_revision"]).map({ $0 + 1 }),
                  strictUInt(object["queue_revision"]) == strictUInt(request["expected_queue_revision"]).map({ $0 + 1 }) else { throw ControlError.invalidReceipt }
        }
    }

    public static func validateCommandReceipt(
        _ data: Data, requestData: Data, action: AssemblywrightMacOwnerControlAction
    ) throws {
        let request: [String: Any]
        do { request = try validateRequest(requestData, action: action) }
        catch { throw ControlError.invalidRequest }
        try validateReceipt(data, request: request, action: action)
    }
}
