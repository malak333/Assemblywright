import Foundation

#if canImport(CryptoKit) && canImport(Security)
import CryptoKit
import Security
#endif

public let jarvisTrustedWakeRuleID = UUID(uuidString: "4A617276-6973-4000-8000-000000000010")!

public struct JarvisTrustedWakeRule: Codable, Equatable, Sendable {
    public var id: UUID
    public var enabled: Bool
    public var keyFingerprint: String
    public var generation: UInt64
    public var highestCounter: UInt64
    public var createdAt: String
    public var updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, enabled, generation
        case keyFingerprint = "key_fingerprint"
        case highestCounter = "highest_counter"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }
}

public struct JarvisTrustedWakeStatus: Codable, Equatable, Sendable {
    public var schemaVersion: UInt16
    public var sessionId: UUID
    public var challenge: String
    public var rule: JarvisTrustedWakeRule?
    public var attentionRequired: Bool
    public var ambiguousDispatchCount: Int
    public var proofBoundary: String

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case sessionId = "session_id"
        case challenge, rule
        case attentionRequired = "attention_required"
        case ambiguousDispatchCount = "ambiguous_dispatch_count"
        case proofBoundary = "proof_boundary"
    }
}

public struct JarvisTrustedWakeRuleEnablement: Codable, Equatable, Sendable {
    public var enabled: Bool
    public var expectedGeneration: UInt64
    public init(enabled: Bool, expectedGeneration: UInt64) {
        self.enabled = enabled
        self.expectedGeneration = expectedGeneration
    }

    enum CodingKeys: String, CodingKey {
        case enabled
        case expectedGeneration = "expected_generation"
    }
}

public struct JarvisTrustedWakeEnvelope: Codable, Equatable, Sendable {
    public var payloadBase64: String
    public var signatureDERBase64: String

    enum CodingKeys: String, CodingKey {
        case payloadBase64 = "payload_b64"
        case signatureDERBase64 = "signature_der_b64"
    }
}

public struct JarvisTrustedWakeEventResponse: Codable, Sendable {
    public var event: JarvisTrustedWakeEvent
    public var idempotentRetry: Bool
    public var proofBoundary: String

    enum CodingKeys: String, CodingKey {
        case event
        case idempotentRetry = "idempotent_retry"
        case proofBoundary = "proof_boundary"
    }
}

public struct JarvisTrustedWakeEvent: Codable, Equatable, Sendable {
    public var id: UUID
    public var ruleId: UUID
    public var counter: UInt64
    public var state: String
    public var taskId: UUID?
    public var schedulerJobId: UUID

    enum CodingKeys: String, CodingKey {
        case id, counter, state
        case ruleId = "rule_id"
        case taskId = "task_id"
        case schedulerJobId = "scheduler_job_id"
    }
}

public struct JarvisTrustedWakeAttentionItem: Codable, Equatable, Sendable, Identifiable {
    public var eventId: UUID
    public var schedulerJobId: UUID
    public var ruleGeneration: UInt64
    public var state: String
    public var receivedAt: String
    public var updatedAt: String
    public var id: UUID { eventId }

    enum CodingKeys: String, CodingKey {
        case eventId = "event_id"
        case schedulerJobId = "scheduler_job_id"
        case ruleGeneration = "rule_generation"
        case state
        case receivedAt = "received_at"
        case updatedAt = "updated_at"
    }
}

public struct JarvisTrustedWakeResolutionRequest: Codable, Equatable, Sendable {
    public var expectedGeneration: UInt64
    public var expectedState: String

    enum CodingKeys: String, CodingKey {
        case expectedGeneration = "expected_generation"
        case expectedState = "expected_state"
    }
}

public protocol TrustedWakeBootstrapProviding: Sendable {
    func bootstrapData() throws -> Data?
}

public struct NoopTrustedWakeBootstrapProvider: TrustedWakeBootstrapProviding {
    public init() {}
    public func bootstrapData() throws -> Data? { nil }
}

public struct TrustedWakeBootstrapProvider: TrustedWakeBootstrapProviding {
    public init() {}

    public func bootstrapData() throws -> Data? {
        #if canImport(CryptoKit) && canImport(Security)
        let key = try TrustedWakeKeychain.loadOrCreateSigningKey()
        let document = BootstrapDocument(
            ruleId: jarvisTrustedWakeRuleID,
            publicKeyX963Base64: key.publicKey.x963Representation.base64EncodedString(),
            command: "Perform the enabled local system-wake check.",
            allowRotation: false
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(document)
        #else
        return nil
        #endif
    }
}

public protocol TrustedWakeEnvelopeSigning: Sendable {
    func envelope(status: JarvisTrustedWakeStatus, occurredAt: Date) throws -> JarvisTrustedWakeEnvelope
}

public extension TrustedWakeEnvelopeSigning {
    func envelope(status: JarvisTrustedWakeStatus) throws -> JarvisTrustedWakeEnvelope {
        try envelope(status: status, occurredAt: Date())
    }
}

public struct TrustedWakeEnvelopeSigner: TrustedWakeEnvelopeSigning {
    public init() {}

    public func envelope(status: JarvisTrustedWakeStatus, occurredAt: Date) throws -> JarvisTrustedWakeEnvelope {
        guard let rule = status.rule, rule.enabled else {
            throw TrustedWakeError.ruleDisabled
        }
        #if canImport(CryptoKit) && canImport(Security)
        let key = try TrustedWakeKeychain.loadOrCreateSigningKey()
        let counter = try TrustedWakeKeychain.nextCounter(
            durableHighWater: rule.highestCounter,
            occurredAt: occurredAt
        )
        let payload = SignedPayload(
            schemaVersion: 1,
            ruleId: rule.id,
            ruleGeneration: rule.generation,
            sessionId: status.sessionId,
            challenge: status.challenge,
            counter: counter,
            occurredAt: occurredAt,
            nonce: UUID().uuidString.lowercased()
        )
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let payloadData = try encoder.encode(payload)
        guard payloadData.count <= 4 * 1024 else { throw TrustedWakeError.payloadTooLarge }
        let signature = try key.signature(for: payloadData)
        return JarvisTrustedWakeEnvelope(
            payloadBase64: payloadData.base64EncodedString(),
            signatureDERBase64: signature.derRepresentation.base64EncodedString()
        )
        #else
        throw TrustedWakeError.unavailable
        #endif
    }
}

public enum TrustedWakeError: Error, Sendable {
    case unavailable
    case ruleDisabled
    case payloadTooLarge
    case keychainStatus(Int32)
    case invalidKey
    case counterExhausted
}

func nextTrustedWakeCounter(
    persisted: UInt64?,
    epochMilliseconds: UInt64,
    durableHighWater: UInt64
) throws -> UInt64 {
    let persistedValue = persisted ?? 0
    guard persistedValue < UInt64.max, durableHighWater < UInt64.max else {
        throw TrustedWakeError.counterExhausted
    }
    return max(persistedValue + 1, epochMilliseconds, durableHighWater + 1)
}

private struct BootstrapDocument: Codable {
    var ruleId: UUID
    var publicKeyX963Base64: String
    var command: String
    var allowRotation: Bool

    enum CodingKeys: String, CodingKey {
        case ruleId = "rule_id"
        case publicKeyX963Base64 = "public_key_x963_b64"
        case command
        case allowRotation = "allow_rotation"
    }
}

private struct SignedPayload: Codable {
    var schemaVersion: UInt16
    var ruleId: UUID
    var ruleGeneration: UInt64
    var sessionId: UUID
    var challenge: String
    var counter: UInt64
    var occurredAt: Date
    var nonce: String

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case ruleId = "rule_id"
        case ruleGeneration = "rule_generation"
        case sessionId = "session_id"
        case challenge, counter
        case occurredAt = "occurred_at"
        case nonce
    }
}

#if canImport(CryptoKit) && canImport(Security)
private enum TrustedWakeKeychain {
    static let service = "com.nobiletechnology.jarvis.trusted-wake"
    static let keyAccount = "p256-private-key"
    static let counterAccount = "event-counter"
    static let counterLock = NSLock()

    static func loadOrCreateSigningKey() throws -> P256.Signing.PrivateKey {
        if let data = try read(account: keyAccount) {
            guard let key = try? P256.Signing.PrivateKey(rawRepresentation: data) else {
                throw TrustedWakeError.invalidKey
            }
            return key
        }
        let key = P256.Signing.PrivateKey()
        try save(key.rawRepresentation, account: keyAccount)
        return key
    }

    static func nextCounter(durableHighWater: UInt64, occurredAt: Date) throws -> UInt64 {
        counterLock.lock()
        defer { counterLock.unlock() }
        let persisted = try read(account: counterAccount)
            .flatMap { String(data: $0, encoding: .utf8) }
            .flatMap(UInt64.init)
        let millisecondsSinceEpoch = occurredAt.timeIntervalSince1970 * 1_000
        guard millisecondsSinceEpoch.isFinite,
              millisecondsSinceEpoch < Double(UInt64.max) else {
            throw TrustedWakeError.counterExhausted
        }
        let epochMilliseconds = UInt64(max(millisecondsSinceEpoch.rounded(.down), 1))
        let next = try nextTrustedWakeCounter(
            persisted: persisted,
            epochMilliseconds: epochMilliseconds,
            durableHighWater: durableHighWater
        )
        try save(Data(String(next).utf8), account: counterAccount)
        return next
    }

    private static func read(account: String) throws -> Data? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = item as? Data else {
            throw TrustedWakeError.keychainStatus(status)
        }
        return data
    }

    private static func save(_ data: Data, account: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        ]
        let update = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if update == errSecSuccess { return }
        guard update == errSecItemNotFound else { throw TrustedWakeError.keychainStatus(update) }
        var add = query
        add.merge(attributes) { _, new in new }
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw TrustedWakeError.keychainStatus(status) }
    }
}
#endif
