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
    public var pendingKeyControl: JarvisTrustedWakePendingKeyControl?
    public var proofBoundary: String

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case sessionId = "session_id"
        case challenge, rule
        case attentionRequired = "attention_required"
        case ambiguousDispatchCount = "ambiguous_dispatch_count"
        case pendingKeyControl = "pending_key_control"
        case proofBoundary = "proof_boundary"
    }
}

public let jarvisTrustedWakeRotateConfirmation = "ROTATE TRUSTED WAKE KEY"
public let jarvisTrustedWakeRecoverConfirmation = "RECOVER LOST TRUSTED WAKE KEY AND BLOCK PENDING WORK"
public let jarvisTrustedWakeCancelConfirmation = "CANCEL TRUSTED WAKE KEY CHANGE"

public enum JarvisTrustedWakeKeyControlOperation: String, Codable, Equatable, Sendable {
    case rotate
    case recover
}

public struct JarvisTrustedWakePendingKeyControl: Codable, Equatable, Sendable {
    public var operation: JarvisTrustedWakeKeyControlOperation
    public var sourceGeneration: UInt64
    public var targetGeneration: UInt64
    public var oldFingerprint: String
    public var newFingerprint: String
    public var expiresAt: String
    public var createdAt: String

    enum CodingKeys: String, CodingKey {
        case operation
        case sourceGeneration = "source_generation"
        case targetGeneration = "target_generation"
        case oldFingerprint = "old_fingerprint"
        case newFingerprint = "new_fingerprint"
        case expiresAt = "expires_at"
        case createdAt = "created_at"
    }
}

public struct JarvisTrustedWakeKeyControlProof: Codable, Equatable, Sendable {
    public var payloadBase64: String
    public var signatureDERBase64: String

    enum CodingKeys: String, CodingKey {
        case payloadBase64 = "payload_b64"
        case signatureDERBase64 = "signature_der_b64"
    }
}

public struct JarvisTrustedWakeKeyControlPrepareRequest: Codable, Equatable, Sendable {
    public var operation: JarvisTrustedWakeKeyControlOperation
    public var expectedGeneration: UInt64
    public var expectedFingerprint: String
    public var newPublicKeyX963Base64: String
    public var confirmation: String
    public var proof: JarvisTrustedWakeKeyControlProof?

    enum CodingKeys: String, CodingKey {
        case operation, confirmation, proof
        case expectedGeneration = "expected_generation"
        case expectedFingerprint = "expected_fingerprint"
        case newPublicKeyX963Base64 = "new_public_key_x963_b64"
    }
}

public struct JarvisTrustedWakeKeyControlPrepareResponse: Codable, Equatable, Sendable {
    public var pending: JarvisTrustedWakePendingKeyControl
    public var grantToken: String
    public var blockedAcceptedCount: Int
    public var proofBoundary: String

    enum CodingKeys: String, CodingKey {
        case pending
        case grantToken = "grant_token"
        case blockedAcceptedCount = "blocked_accepted_count"
        case proofBoundary = "proof_boundary"
    }
}

public struct JarvisTrustedWakeKeyControlCancelRequest: Codable, Equatable, Sendable {
    public var expectedGeneration: UInt64
    public var expectedFingerprint: String
    public var confirmation: String

    enum CodingKeys: String, CodingKey {
        case expectedGeneration = "expected_generation"
        case expectedFingerprint = "expected_fingerprint"
        case confirmation
    }
}

public struct JarvisTrustedWakeKeyControlInstallDocument: Codable, Equatable, Sendable {
    public var ruleId: UUID
    public var targetGeneration: UInt64
    public var newPublicKeyX963Base64: String
    public var grantToken: String

    enum CodingKeys: String, CodingKey {
        case ruleId = "rule_id"
        case targetGeneration = "target_generation"
        case newPublicKeyX963Base64 = "new_public_key_x963_b64"
        case grantToken = "grant_token"
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

public protocol TrustedWakeKeyControlInstallProviding: Sendable {
    func installData(minimumValiditySeconds: TimeInterval) throws -> Data?
}

public protocol TrustedWakeKeyRingManaging: TrustedWakeKeyControlInstallProviding {
    func stage(
        operation: JarvisTrustedWakeKeyControlOperation,
        status: JarvisTrustedWakeStatus,
        confirmation: String
    ) throws -> JarvisTrustedWakeKeyControlPrepareRequest
    func persist(response: JarvisTrustedWakeKeyControlPrepareResponse) throws
    func reconcile(status: JarvisTrustedWakeStatus) throws -> Bool
    func discardUnjournaledCandidate() throws
    func cancelLocalPending() throws
}

public struct TrustedWakeKeyRing: TrustedWakeKeyRingManaging {
    public init() {}

    public func stage(
        operation: JarvisTrustedWakeKeyControlOperation,
        status: JarvisTrustedWakeStatus,
        confirmation: String
    ) throws -> JarvisTrustedWakeKeyControlPrepareRequest {
        #if canImport(CryptoKit) && canImport(Security)
        return try TrustedWakeKeychain.withKeyControlLock {
            try TrustedWakeKeychain.stage(
                operation: operation,
                status: status,
                confirmation: confirmation
            )
        }
        #else
        throw TrustedWakeError.unavailable
        #endif
    }

    public func persist(response: JarvisTrustedWakeKeyControlPrepareResponse) throws {
        #if canImport(CryptoKit) && canImport(Security)
        try TrustedWakeKeychain.withKeyControlLock {
            try TrustedWakeKeychain.persist(response: response)
        }
        #else
        throw TrustedWakeError.unavailable
        #endif
    }

    public func installData(minimumValiditySeconds: TimeInterval) throws -> Data? {
        #if canImport(CryptoKit) && canImport(Security)
        return try TrustedWakeKeychain.withKeyControlLock {
            try TrustedWakeKeychain.pendingInstallData(
                minimumValiditySeconds: minimumValiditySeconds
            )
        }
        #else
        return nil
        #endif
    }

    public func reconcile(status: JarvisTrustedWakeStatus) throws -> Bool {
        #if canImport(CryptoKit) && canImport(Security)
        return try TrustedWakeKeychain.withKeyControlLock {
            try TrustedWakeKeychain.reconcile(status: status)
        }
        #else
        return false
        #endif
    }

    public func discardUnjournaledCandidate() throws {
        #if canImport(CryptoKit) && canImport(Security)
        try TrustedWakeKeychain.withKeyControlLock {
            try TrustedWakeKeychain.discardUnjournaledCandidate()
        }
        #endif
    }

    public func cancelLocalPending() throws {
        #if canImport(CryptoKit) && canImport(Security)
        try TrustedWakeKeychain.withKeyControlLock {
            try TrustedWakeKeychain.cancelLocalPending()
        }
        #endif
    }
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
        let key = try TrustedWakeKeychain.loadActiveSigningKey()
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
    case keyControlAlreadyPending
    case keyControlConfirmationMismatch
    case keyControlJournalMismatch
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

private struct KeyControlProofPayload: Codable {
    var domain: String
    var schemaVersion: UInt16
    var operation: JarvisTrustedWakeKeyControlOperation
    var ruleId: UUID
    var expectedGeneration: UInt64
    var expectedFingerprint: String
    var newFingerprint: String
    var sessionId: UUID
    var challenge: String
    var confirmation: String
    var occurredAt: Date
    var nonce: String

    enum CodingKeys: String, CodingKey {
        case domain, operation, challenge, confirmation, nonce
        case schemaVersion = "schema_version"
        case ruleId = "rule_id"
        case expectedGeneration = "expected_generation"
        case expectedFingerprint = "expected_fingerprint"
        case newFingerprint = "new_fingerprint"
        case sessionId = "session_id"
        case occurredAt = "occurred_at"
    }
}

private struct KeyControlJournal: Codable {
    var operation: JarvisTrustedWakeKeyControlOperation
    var sourceGeneration: UInt64
    var targetGeneration: UInt64
    var oldFingerprint: String
    var newFingerprint: String
    var grantToken: String
    var expiresAt: String
}

public func trustedWakeGrantIsExpired(_ expiresAt: String, now: Date = Date()) -> Bool {
    !trustedWakeGrantHasMinimumValidity(expiresAt, minimumValiditySeconds: 0, now: now)
}

public func trustedWakeGrantHasMinimumValidity(
    _ expiresAt: String,
    minimumValiditySeconds: TimeInterval,
    now: Date = Date()
) -> Bool {
    let fractional = ISO8601DateFormatter()
    fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let standard = ISO8601DateFormatter()
    standard.formatOptions = [.withInternetDateTime]
    guard let expiry = fractional.date(from: expiresAt) ?? standard.date(from: expiresAt) else {
        return false
    }
    return expiry > now.addingTimeInterval(max(minimumValiditySeconds, 0))
}

public enum TrustedWakeKeyReconcileDisposition: Equatable, Sendable {
    case wait
    case promoteCandidate
    case clearConfirmedCancel
}

public func trustedWakeKeyReconcileDisposition(
    rule: JarvisTrustedWakeRule?,
    pending: JarvisTrustedWakePendingKeyControl?,
    targetGeneration: UInt64,
    oldFingerprint: String,
    newFingerprint: String
) -> TrustedWakeKeyReconcileDisposition {
    guard pending == nil, let rule, !rule.enabled, rule.generation == targetGeneration else {
        return .wait
    }
    if rule.keyFingerprint == newFingerprint { return .promoteCandidate }
    if rule.keyFingerprint == oldFingerprint { return .clearConfirmedCancel }
    return .wait
}

#if canImport(CryptoKit) && canImport(Security)
private enum TrustedWakeKeychain {
    static let service = "com.nobiletechnology.jarvis.trusted-wake"
    static let keyAccount = "p256-private-key"
    static let stagedKeyAccount = "p256-private-key-staged"
    static let keyControlJournalAccount = "key-control-journal"
    static let counterAccount = "event-counter"
    static let counterLock = NSLock()
    static let keyControlLock = NSLock()

    static func withKeyControlLock<T>(_ operation: () throws -> T) rethrows -> T {
        keyControlLock.lock()
        defer { keyControlLock.unlock() }
        return try operation()
    }

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

    static func loadActiveSigningKey() throws -> P256.Signing.PrivateKey {
        guard let data = try read(account: keyAccount),
              let key = try? P256.Signing.PrivateKey(rawRepresentation: data) else {
            throw TrustedWakeError.invalidKey
        }
        return key
    }

    static func stage(
        operation: JarvisTrustedWakeKeyControlOperation,
        status: JarvisTrustedWakeStatus,
        confirmation: String
    ) throws -> JarvisTrustedWakeKeyControlPrepareRequest {
        guard try read(account: keyControlJournalAccount) == nil,
              try read(account: stagedKeyAccount) == nil else {
            throw TrustedWakeError.keyControlAlreadyPending
        }
        let required = operation == .rotate
            ? jarvisTrustedWakeRotateConfirmation
            : jarvisTrustedWakeRecoverConfirmation
        guard confirmation == required else {
            throw TrustedWakeError.keyControlConfirmationMismatch
        }
        guard let rule = status.rule else { throw TrustedWakeError.invalidKey }
        let candidate = P256.Signing.PrivateKey()
        let publicData = candidate.publicKey.x963Representation
        let fingerprint = SHA256.hash(data: publicData).map { String(format: "%02x", $0) }.joined()
        guard fingerprint != rule.keyFingerprint else { throw TrustedWakeError.invalidKey }
        var proof: JarvisTrustedWakeKeyControlProof?
        if operation == .rotate {
            let active = try loadActiveSigningKey()
            let payload = KeyControlProofPayload(
                domain: "jarvis.trusted-wake.key-control.v1",
                schemaVersion: 1,
                operation: operation,
                ruleId: rule.id,
                expectedGeneration: rule.generation,
                expectedFingerprint: rule.keyFingerprint,
                newFingerprint: fingerprint,
                sessionId: status.sessionId,
                challenge: status.challenge,
                confirmation: confirmation,
                occurredAt: Date(),
                nonce: UUID().uuidString.lowercased()
            )
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            encoder.outputFormatting = [.sortedKeys]
            let bytes = try encoder.encode(payload)
            guard bytes.count <= 4 * 1024 else { throw TrustedWakeError.payloadTooLarge }
            let signature = try active.signature(for: bytes)
            proof = JarvisTrustedWakeKeyControlProof(
                payloadBase64: bytes.base64EncodedString(),
                signatureDERBase64: signature.derRepresentation.base64EncodedString()
            )
        }
        try save(candidate.rawRepresentation, account: stagedKeyAccount)
        return JarvisTrustedWakeKeyControlPrepareRequest(
            operation: operation,
            expectedGeneration: rule.generation,
            expectedFingerprint: rule.keyFingerprint,
            newPublicKeyX963Base64: publicData.base64EncodedString(),
            confirmation: confirmation,
            proof: proof
        )
    }

    static func persist(response: JarvisTrustedWakeKeyControlPrepareResponse) throws {
        guard let stagedData = try read(account: stagedKeyAccount),
              let staged = try? P256.Signing.PrivateKey(rawRepresentation: stagedData) else {
            throw TrustedWakeError.keyControlJournalMismatch
        }
        let fingerprint = SHA256.hash(data: staged.publicKey.x963Representation)
            .map { String(format: "%02x", $0) }.joined()
        guard fingerprint == response.pending.newFingerprint else {
            throw TrustedWakeError.keyControlJournalMismatch
        }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        try save(
            encoder.encode(KeyControlJournal(
                operation: response.pending.operation,
                sourceGeneration: response.pending.sourceGeneration,
                targetGeneration: response.pending.targetGeneration,
                oldFingerprint: response.pending.oldFingerprint,
                newFingerprint: response.pending.newFingerprint,
                grantToken: response.grantToken,
                expiresAt: response.pending.expiresAt
            )),
            account: keyControlJournalAccount
        )
    }

    static func pendingInstallData(minimumValiditySeconds: TimeInterval) throws -> Data? {
        guard let journalData = try read(account: keyControlJournalAccount) else { return nil }
        let journal = try JSONDecoder().decode(KeyControlJournal.self, from: journalData)
        guard trustedWakeGrantHasMinimumValidity(
            journal.expiresAt,
            minimumValiditySeconds: minimumValiditySeconds
        ) else {
            throw TrustedWakeError.keyControlJournalMismatch
        }
        guard let stagedData = try read(account: stagedKeyAccount),
              let staged = try? P256.Signing.PrivateKey(rawRepresentation: stagedData) else {
            throw TrustedWakeError.keyControlJournalMismatch
        }
        let publicData = staged.publicKey.x963Representation
        let fingerprint = SHA256.hash(data: publicData).map { String(format: "%02x", $0) }.joined()
        guard fingerprint == journal.newFingerprint else {
            throw TrustedWakeError.keyControlJournalMismatch
        }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(JarvisTrustedWakeKeyControlInstallDocument(
            ruleId: jarvisTrustedWakeRuleID,
            targetGeneration: journal.targetGeneration,
            newPublicKeyX963Base64: publicData.base64EncodedString(),
            grantToken: journal.grantToken
        ))
    }

    static func reconcile(status: JarvisTrustedWakeStatus) throws -> Bool {
        guard let journalData = try read(account: keyControlJournalAccount) else { return false }
        let journal = try JSONDecoder().decode(KeyControlJournal.self, from: journalData)
        let disposition = trustedWakeKeyReconcileDisposition(
            rule: status.rule,
            pending: status.pendingKeyControl,
            targetGeneration: journal.targetGeneration,
            oldFingerprint: journal.oldFingerprint,
            newFingerprint: journal.newFingerprint
        )
        if disposition == .clearConfirmedCancel {
            try delete(account: keyControlJournalAccount)
            try delete(account: stagedKeyAccount)
            return true
        }
        guard disposition == .promoteCandidate else { return false }
        guard let stagedData = try read(account: stagedKeyAccount),
              let staged = try? P256.Signing.PrivateKey(rawRepresentation: stagedData) else {
            throw TrustedWakeError.keyControlJournalMismatch
        }
        let fingerprint = SHA256.hash(data: staged.publicKey.x963Representation)
            .map { String(format: "%02x", $0) }.joined()
        guard fingerprint == journal.newFingerprint else {
            throw TrustedWakeError.keyControlJournalMismatch
        }
        try save(staged.rawRepresentation, account: keyAccount)
        try delete(account: keyControlJournalAccount)
        try delete(account: stagedKeyAccount)
        try delete(account: counterAccount)
        return true
    }

    static func discardUnjournaledCandidate() throws {
        guard try read(account: keyControlJournalAccount) == nil else { return }
        try delete(account: stagedKeyAccount)
    }

    static func cancelLocalPending() throws {
        try delete(account: keyControlJournalAccount)
        try delete(account: stagedKeyAccount)
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

    private static func delete(account: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw TrustedWakeError.keychainStatus(status)
        }
    }
}
#endif
