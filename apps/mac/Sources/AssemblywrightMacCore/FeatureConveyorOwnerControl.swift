import Foundation

public enum AssemblywrightMacFeatureConveyorOwnerControlError: Error, Equatable, Sendable,
    CustomStringConvertible
{
    case invalidRequest
    case requestTooLarge
    case rejected
    case invalidReceipt

    public var description: String {
        switch self {
        case .invalidRequest:
            "The approved-feature request does not match the exact owner-control contract."
        case .requestTooLarge:
            "The approved-feature request exceeds the fixed owner-control limit."
        case .rejected:
            "The Windows master rejected the approved-feature enqueue action."
        case .invalidReceipt:
            "The Windows master returned an invalid owner-control receipt."
        }
    }
}

public struct AssemblywrightMacFeatureConveyorApprovedFeatureReceipt: Codable, Equatable,
    Sendable
{
    public static let expectedSchemaVersion: UInt64 = 1

    public let schemaVersion: UInt64
    public let featureID: UUID
    public let specificationRevision: UInt64
    public let lifecycleRevision: UInt64
    public let queueRevision: UInt64
    public let ownerControlDesignationRevision: UInt64
    public let emergencyPauseRevision: UInt64
    public let status: String

    enum CodingKeys: String, CodingKey, CaseIterable {
        case schemaVersion = "schema_version"
        case featureID = "feature_id"
        case specificationRevision = "specification_revision"
        case lifecycleRevision = "lifecycle_revision"
        case queueRevision = "queue_revision"
        case ownerControlDesignationRevision = "owner_control_designation_revision"
        case emergencyPauseRevision = "emergency_pause_revision"
        case status
    }
}

public enum AssemblywrightMacFeatureConveyorOwnerControl {
    public static let approvedFeaturesPath =
        "/v1/distributed/feature-conveyor/approved-features"
    public static let maximumRequestBytes = 320 * 1_024
    public static let maximumReceiptBytes = 4 * 1_024

    public static func approveAndEnqueue(
        requestData: Data,
        using session: any AssemblywrightMacBridgeSession
    ) async throws -> AssemblywrightMacFeatureConveyorApprovedFeatureReceipt {
        do {
            let binding = try validateRequest(requestData)
            let response = try await session.send(
                AssemblywrightMacBridgeHTTPRequest(
                    method: "POST",
                    path: approvedFeaturesPath,
                    body: requestData
                )
            )
            guard response.status == 200 else {
                throw AssemblywrightMacFeatureConveyorOwnerControlError.rejected
            }
            let receipt = try decodeReceipt(response.body)
            let (nextQueueRevision, queueRevisionOverflow) = binding.expectedQueueRevision
                .addingReportingOverflow(1)
            guard !queueRevisionOverflow,
                  receipt.featureID == binding.featureID,
                  receipt.specificationRevision == binding.specificationRevision,
                  receipt.lifecycleRevision == 1,
                  receipt.queueRevision == nextQueueRevision,
                  receipt.ownerControlDesignationRevision
                    == binding.ownerControlDesignationRevision,
                  receipt.emergencyPauseRevision == binding.emergencyPauseRevision else {
                throw AssemblywrightMacFeatureConveyorOwnerControlError.invalidReceipt
            }
            await session.cancel()
            return receipt
        } catch let error as AssemblywrightMacFeatureConveyorOwnerControlError {
            await session.cancel()
            throw error
        } catch {
            await session.cancel()
            throw AssemblywrightMacFeatureConveyorOwnerControlError.invalidRequest
        }
    }

    private struct RequestBinding {
        let featureID: UUID
        let specificationRevision: UInt64
        let expectedQueueRevision: UInt64
        let ownerControlDesignationRevision: UInt64
        let emergencyPauseRevision: UInt64
    }

    private static func validateRequest(_ data: Data) throws -> RequestBinding {
        guard !data.isEmpty else {
            throw AssemblywrightMacFeatureConveyorOwnerControlError.invalidRequest
        }
        guard data.count <= maximumRequestBytes else {
            throw AssemblywrightMacFeatureConveyorOwnerControlError.requestTooLarge
        }
        do {
            var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
            try scanner.validateNoDuplicateObjectKeysRecursively()
        } catch {
            throw AssemblywrightMacFeatureConveyorOwnerControlError.invalidRequest
        }
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys) == Set([
                "schema_version", "expected_queue_revision",
                "owner_control_designation_revision", "emergency_pause_revision",
                "specification"
              ]),
              strictInteger(object["schema_version"]) == 1,
              let expectedQueueRevision = strictInteger(object["expected_queue_revision"]),
              let designationRevision = strictInteger(
                object["owner_control_designation_revision"]
              ), designationRevision > 0,
              let pauseRevision = strictInteger(object["emergency_pause_revision"]),
              let specification = object["specification"] as? [String: Any],
              Set(specification.keys) == Set([
                "feature_id", "revision", "repository_id", "manifest", "manifest_sha256",
                "design_sha256", "brainstorming_sha256", "owner_approval_sha256", "grants",
                "provider_id", "model_id", "dependencies"
              ]),
              let featureID = strictUUID(specification["feature_id"]),
              strictUUID(specification["repository_id"]) != nil,
              let specificationRevision = strictInteger(specification["revision"]),
              specificationRevision > 0,
              let manifest = specification["manifest"] as? [String: Any],
              strictDigest(specification["manifest_sha256"])?.contains(where: { $0 != 0 })
                == true,
              strictDigest(specification["design_sha256"])?.contains(where: { $0 != 0 }) == true,
              strictDigest(specification["brainstorming_sha256"])?.contains(where: { $0 != 0 })
                == true,
              strictDigest(specification["owner_approval_sha256"])?.contains(where: { $0 != 0 })
                == true,
              validIdentifier(specification["provider_id"], maximum: 128),
              validIdentifier(specification["model_id"], maximum: 128),
              validDependencies(specification["dependencies"], featureID: featureID),
              validGrants(specification["grants"]),
              JSONSerialization.isValidJSONObject(manifest) else {
            throw AssemblywrightMacFeatureConveyorOwnerControlError.invalidRequest
        }
        return RequestBinding(
            featureID: featureID,
            specificationRevision: specificationRevision,
            expectedQueueRevision: expectedQueueRevision,
            ownerControlDesignationRevision: designationRevision,
            emergencyPauseRevision: pauseRevision
        )
    }

    private static func decodeReceipt(
        _ data: Data
    ) throws -> AssemblywrightMacFeatureConveyorApprovedFeatureReceipt {
        guard !data.isEmpty, data.count <= maximumReceiptBytes else {
            throw AssemblywrightMacFeatureConveyorOwnerControlError.invalidReceipt
        }
        do {
            var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
            try scanner.validateNoDuplicateObjectKeysRecursively()
        } catch {
            throw AssemblywrightMacFeatureConveyorOwnerControlError.invalidReceipt
        }
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys)
                == Set(AssemblywrightMacFeatureConveyorApprovedFeatureReceipt.CodingKeys.allCases
                    .map(\.rawValue)),
              strictInteger(object["schema_version"])
                == AssemblywrightMacFeatureConveyorApprovedFeatureReceipt.expectedSchemaVersion,
              strictUUID(object["feature_id"]) != nil,
              strictInteger(object["specification_revision"]).map({ $0 > 0 }) == true,
              strictInteger(object["lifecycle_revision"]).map({ $0 > 0 }) == true,
              strictInteger(object["queue_revision"]).map({ $0 > 0 }) == true,
              strictInteger(object["owner_control_designation_revision"]).map({ $0 > 0 })
                == true,
              strictInteger(object["emergency_pause_revision"]) != nil,
              object["status"] as? String == "queued",
              let receipt = try? JSONDecoder().decode(
                AssemblywrightMacFeatureConveyorApprovedFeatureReceipt.self,
                from: data
              ) else {
            throw AssemblywrightMacFeatureConveyorOwnerControlError.invalidReceipt
        }
        return receipt
    }

    private static func strictInteger(_ value: Any?) -> UInt64? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else { return nil }
        let text = number.stringValue
        guard let parsed = UInt64(text), String(parsed) == text else { return nil }
        return parsed
    }

    private static func strictUUID(_ value: Any?) -> UUID? {
        guard let text = value as? String,
              text == text.lowercased(),
              let uuid = UUID(uuidString: text),
              uuid != UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)) else {
            return nil
        }
        return uuid
    }

    private static func strictDigest(_ value: Any?) -> [UInt8]? {
        guard let values = value as? [Any], values.count == 32 else { return nil }
        var digest: [UInt8] = []
        digest.reserveCapacity(32)
        for value in values {
            guard let integer = strictInteger(value), integer <= UInt8.max else { return nil }
            digest.append(UInt8(integer))
        }
        return digest
    }

    private static func validIdentifier(_ value: Any?, maximum: Int) -> Bool {
        guard let text = value as? String, !text.isEmpty, text.utf8.count <= maximum,
              text == text.trimmingCharacters(in: .whitespacesAndNewlines) else { return false }
        return text.utf8.allSatisfy { (0x21 ... 0x7e).contains($0) }
    }

    private static func validDependencies(_ value: Any?, featureID: UUID) -> Bool {
        guard let values = value as? [Any], values.count <= 100 else { return false }
        let identifiers = values.compactMap(strictUUID)
        return identifiers.count == values.count
            && Set(identifiers).count == identifiers.count
            && !identifiers.contains(featureID)
    }

    private static func validGrants(_ value: Any?) -> Bool {
        guard let grants = value as? [String: Any],
              Set(grants.keys) == Set([
                "registration", "cloud_disclosure", "autonomous_publication"
              ]) else { return false }
        return grants.values.allSatisfy { strictInteger($0).map({ $0 > 0 }) == true }
    }
}
