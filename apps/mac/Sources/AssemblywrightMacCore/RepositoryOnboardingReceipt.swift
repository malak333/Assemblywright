import Foundation

public enum AssemblywrightMacRepositoryOnboardingReceiptError: Error, Equatable, Sendable {
    case invalid
    case tooLarge
}

/// A path-free handoff from the Windows owner-control onboarding flow.
///
/// Decoding this receipt grants no authority. It only supplies bounded identifiers that
/// Windows independently revalidates when an approved feature is enqueued.
public struct AssemblywrightMacRepositoryOnboardingReceipt: Equatable, Sendable {
    public static let maximumBytes = 4 * 1_024

    public let repositoryID: UUID
    public let registrationGrantRevision: UInt64
    public let cloudDisclosureGrantRevision: UInt64
    public let autonomousPublicationGrantRevision: UInt64
    public let baseBranch: String
    public let headCommit: String
    public let scopeSHA256: String
    public let approvalPlanSHA256: String
    public let preflightFingerprintSHA256: String

    public static func decodeStrict(_ data: Data) throws -> Self {
        guard !data.isEmpty else {
            throw AssemblywrightMacRepositoryOnboardingReceiptError.invalid
        }
        guard data.count <= maximumBytes else {
            throw AssemblywrightMacRepositoryOnboardingReceiptError.tooLarge
        }

        do {
            var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
            try scanner.validateNoDuplicateObjectKeysRecursively()
        } catch {
            throw AssemblywrightMacRepositoryOnboardingReceiptError.invalid
        }

        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys) == Set([
                  "schema_version", "status", "repository_id",
                  "registration_grant_revision", "cloud_disclosure_grant_revision",
                  "autonomous_publication_grant_revision", "base_branch", "head_commit",
                  "scope_sha256", "approval_plan_sha256", "preflight_fingerprint_sha256"
              ]),
              strictInteger(object["schema_version"]) == 1,
              object["status"] as? String == "repository_onboarding_ready",
              let repositoryID = strictUUID(object["repository_id"]),
              strictInteger(object["registration_grant_revision"]) == 1,
              strictInteger(object["cloud_disclosure_grant_revision"]) == 1,
              strictInteger(object["autonomous_publication_grant_revision"]) == 1,
              object["base_branch"] as? String == "main",
              let headCommit = strictLowercaseHex(object["head_commit"], count: 40),
              let scopeSHA256 = strictNonzeroDigest(object["scope_sha256"]),
              let approvalPlanSHA256 = strictNonzeroDigest(object["approval_plan_sha256"]),
              let preflightFingerprintSHA256 = strictNonzeroDigest(
                  object["preflight_fingerprint_sha256"]
              ) else {
            throw AssemblywrightMacRepositoryOnboardingReceiptError.invalid
        }

        return Self(
            repositoryID: repositoryID,
            registrationGrantRevision: 1,
            cloudDisclosureGrantRevision: 1,
            autonomousPublicationGrantRevision: 1,
            baseBranch: "main",
            headCommit: headCommit,
            scopeSHA256: scopeSHA256,
            approvalPlanSHA256: approvalPlanSHA256,
            preflightFingerprintSHA256: preflightFingerprintSHA256
        )
    }

    private static func strictInteger(_ value: Any?) -> UInt64? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else {
            return nil
        }
        let text = number.stringValue
        guard let parsed = UInt64(text), String(parsed) == text else { return nil }
        return parsed
    }

    private static func strictUUID(_ value: Any?) -> UUID? {
        guard let text = value as? String,
              text == text.lowercased(),
              let uuid = UUID(uuidString: text),
              uuid.uuidString.lowercased() == text,
              uuid != UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)) else {
            return nil
        }
        return uuid
    }

    private static func strictNonzeroDigest(_ value: Any?) -> String? {
        guard let digest = strictLowercaseHex(value, count: 64),
              digest.contains(where: { $0 != "0" }) else {
            return nil
        }
        return digest
    }

    private static func strictLowercaseHex(_ value: Any?, count: Int) -> String? {
        guard let text = value as? String,
              text.utf8.count == count,
              text.utf8.allSatisfy({
                  (0x30 ... 0x39).contains($0) || (0x61 ... 0x66).contains($0)
              }) else {
            return nil
        }
        return text
    }
}
