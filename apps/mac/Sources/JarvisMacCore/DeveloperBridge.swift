import CryptoKit
import Darwin
import Foundation

public enum JarvisMacDeveloperBridgeError: Error, Equatable, Sendable, CustomStringConvertible {
    case documentTooLarge
    case invalidDocument
    case invalidInvitation
    case invitationExpired
    case noStagedEnrollment
    case bindingMismatch
    case identityUnavailable
    case keychainFailure(Int32)
    case certificateInvalid
    case masterIdentityRejected
    case channelBindingUnavailable
    case tlsProtocolRejected
    case connectionFailed
    case invalidResponse
    case responseTooLarge
    case unauthenticatedSession
    case requestInFlight
    case cancelled

    public var description: String {
        switch self {
        case .documentTooLarge: "Enrollment document exceeds its fixed byte limit."
        case .invalidDocument: "Enrollment document does not match the exact versioned contract."
        case .invalidInvitation: "Enrollment invitation contains invalid or unsupported values."
        case .invitationExpired: "Enrollment invitation has expired."
        case .noStagedEnrollment: "No matching Keychain enrollment is staged."
        case .bindingMismatch: "Enrollment or handshake identity does not match the staged binding."
        case .identityUnavailable: "The device-bound Keychain identity is unavailable."
        case let .keychainFailure(status): "Keychain operation failed with status \(status)."
        case .certificateInvalid: "The issued device certificate failed local validation."
        case .masterIdentityRejected: "The Windows master certificate did not match the enrolled authority and endpoint."
        case .channelBindingUnavailable: "The TLS exporter channel binding is unavailable."
        case .tlsProtocolRejected: "The connection did not negotiate TLS 1.3."
        case .connectionFailed: "The authenticated bridge connection failed."
        case .invalidResponse: "The master returned an invalid or incompatible response."
        case .responseTooLarge: "The master response exceeds the fixed wire limit."
        case .unauthenticatedSession: "Application traffic requires an accepted authenticated handshake."
        case .requestInFlight: "Only one bridge request may be active on a connection."
        case .cancelled: "The bridge operation was cancelled."
        }
    }
}

public struct JarvisMacBridgeCapability: Codable, Equatable, Sendable {
    public let id: String
    public let kind: String
    public let provider: String
    public let model: String
    public let maxContextBytes: UInt32
    public let maxResultBytes: UInt32

    public init(
        id: String,
        kind: String,
        provider: String,
        model: String,
        maxContextBytes: UInt32,
        maxResultBytes: UInt32
    ) {
        self.id = id
        self.kind = kind
        self.provider = provider
        self.model = model
        self.maxContextBytes = maxContextBytes
        self.maxResultBytes = maxResultBytes
    }

    enum CodingKeys: String, CodingKey {
        case id, kind, provider, model
        case maxContextBytes = "max_context_bytes"
        case maxResultBytes = "max_result_bytes"
    }

    fileprivate func validate() throws {
        guard validIdentifier(id, maximum: 64),
              kind == "local_inference" || kind == "apple_integration",
              validIdentifier(provider, maximum: 64),
              !model.isEmpty, model.utf8.count <= 128,
              maxContextBytes > 0, maxContextBytes <= 256 * 1_024,
              maxResultBytes > 0, maxResultBytes <= 768 * 1_024 else {
            throw JarvisMacDeveloperBridgeError.invalidInvitation
        }
    }
}

public struct JarvisMacEnrollmentInvitation: Codable, Equatable, Sendable {
    public let schemaVersion: UInt16
    public let status: String
    public let grantID: String
    public let deviceID: String
    public let deviceName: String
    public let role: String
    public let registryRevision: UInt64
    public let expiresAtMilliseconds: UInt64
    public let capabilities: [JarvisMacBridgeCapability]
    public let masterEndpoint: String
    public let caFingerprintSHA256: String

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case status
        case grantID = "grant_id"
        case deviceID = "device_id"
        case deviceName = "device_name"
        case role
        case registryRevision = "registry_revision"
        case expiresAtMilliseconds = "expires_at_ms"
        case capabilities
        case masterEndpoint = "master_endpoint"
        case caFingerprintSHA256 = "ca_fingerprint_sha256"
    }

    fileprivate func validate(nowMilliseconds: UInt64) throws {
        guard schemaVersion == 1,
              status == "enrollment_invitation_ready",
              validUUID(grantID), validUUID(deviceID),
              !deviceName.isEmpty, deviceName.utf8.count <= 128,
              role == "mac_bridge",
              registryRevision > 0,
              !capabilities.isEmpty, capabilities.count <= 64,
              validEndpoint(masterEndpoint),
              validLowercaseHex(caFingerprintSHA256, count: 64) else {
            throw JarvisMacDeveloperBridgeError.invalidInvitation
        }
        guard expiresAtMilliseconds > nowMilliseconds else {
            throw JarvisMacDeveloperBridgeError.invitationExpired
        }
        var identifiers = Set<String>()
        for capability in capabilities {
            try capability.validate()
            guard identifiers.insert(capability.id).inserted else {
                throw JarvisMacDeveloperBridgeError.invalidInvitation
            }
        }
    }
}

public struct JarvisMacEnrollmentCSR: Codable, Equatable, Sendable {
    public let schemaVersion: UInt16
    public let status: String
    public let grantID: String
    public let deviceID: String
    public let csrPEM: String

    public init(
        schemaVersion: UInt16,
        status: String,
        grantID: String,
        deviceID: String,
        csrPEM: String
    ) {
        self.schemaVersion = schemaVersion
        self.status = status
        self.grantID = grantID
        self.deviceID = deviceID
        self.csrPEM = csrPEM
    }

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case status
        case grantID = "grant_id"
        case deviceID = "device_id"
        case csrPEM = "csr_pem"
    }
}

public struct JarvisMacIssuedDeviceCertificate: Codable, Equatable, Sendable {
    public let status: String
    public let operation: String
    public let deviceID: String
    public let deviceName: String
    public let role: String
    public let registryRevision: UInt64
    public let serialHex: String
    public let issuedAtMilliseconds: UInt64
    public let notAfterMilliseconds: UInt64
    public let certificateSHA256: String
    public let certificatePEM: String
    public let caCertificatePEM: String

    enum CodingKeys: String, CodingKey {
        case status, operation, role
        case deviceID = "device_id"
        case deviceName = "device_name"
        case registryRevision = "registry_revision"
        case serialHex = "serial_hex"
        case issuedAtMilliseconds = "issued_at_ms"
        case notAfterMilliseconds = "not_after_ms"
        case certificateSHA256 = "certificate_sha256"
        case certificatePEM = "certificate_pem"
        case caCertificatePEM = "ca_certificate_pem"
    }

    fileprivate func validate() throws {
        guard status == "device_certificate_issued",
              operation == "enroll",
              validUUID(deviceID), !deviceName.isEmpty, deviceName.utf8.count <= 128,
              role == "mac_bridge", registryRevision > 0,
              validLowercaseHex(serialHex, minimum: 2, maximum: 40),
              issuedAtMilliseconds < notAfterMilliseconds,
              validLowercaseHex(certificateSHA256, count: 64),
              validPEM(certificatePEM, label: "CERTIFICATE"),
              validPEM(caCertificatePEM, label: "CERTIFICATE") else {
            throw JarvisMacDeveloperBridgeError.invalidDocument
        }
    }
}

public struct JarvisMacBridgeProfile: Codable, Equatable, Sendable {
    public let deviceID: String
    public let deviceName: String
    public let role: String
    public let registryRevision: UInt64
    public let capabilities: [JarvisMacBridgeCapability]
    public let masterEndpoint: String
    public let certificateNotAfterMilliseconds: UInt64

    public init(
        deviceID: String,
        deviceName: String,
        role: String,
        registryRevision: UInt64,
        capabilities: [JarvisMacBridgeCapability],
        masterEndpoint: String,
        certificateNotAfterMilliseconds: UInt64
    ) {
        self.deviceID = deviceID
        self.deviceName = deviceName
        self.role = role
        self.registryRevision = registryRevision
        self.capabilities = capabilities
        self.masterEndpoint = masterEndpoint
        self.certificateNotAfterMilliseconds = certificateNotAfterMilliseconds
    }
}

public protocol JarvisMacBridgeIdentityStore: Sendable {
    func stageIdentity(for invitation: JarvisMacEnrollmentInvitation) throws -> JarvisMacEnrollmentCSR
    func loadStagedInvitation() throws -> JarvisMacEnrollmentInvitation?
    func install(
        _ receipt: JarvisMacIssuedDeviceCertificate,
        for invitation: JarvisMacEnrollmentInvitation
    ) throws -> JarvisMacBridgeProfile
    func loadInstalledProfile() throws -> JarvisMacBridgeProfile?
}

public struct JarvisMacEnrollmentCoordinator: Sendable {
    public static let maximumDocumentBytes = 64 * 1_024
    private let identityStore: any JarvisMacBridgeIdentityStore
    private let nowMilliseconds: @Sendable () -> UInt64

    public init(
        identityStore: any JarvisMacBridgeIdentityStore = KeychainJarvisMacBridgeIdentityStore(),
        nowMilliseconds: @escaping @Sendable () -> UInt64 = {
            UInt64(max(Date().timeIntervalSince1970 * 1_000, 0))
        }
    ) {
        self.identityStore = identityStore
        self.nowMilliseconds = nowMilliseconds
    }

    /// Consumes only the public invitation. The Windows-local grant secret is deliberately absent.
    public func prepare(invitationData: Data) throws -> Data {
        let invitation: JarvisMacEnrollmentInvitation = try decodeExact(
            invitationData,
            keys: [
                "schema_version", "status", "grant_id", "device_id", "device_name", "role",
                "registry_revision", "expires_at_ms", "capabilities", "master_endpoint",
                "ca_fingerprint_sha256"
            ]
        )
        try invitation.validate(nowMilliseconds: nowMilliseconds())
        let reply = try identityStore.stageIdentity(for: invitation)
        guard reply.schemaVersion == 1,
              reply.status == "enrollment_csr_ready",
              reply.grantID == invitation.grantID,
              reply.deviceID == invitation.deviceID,
              validPEM(reply.csrPEM, label: "CERTIFICATE REQUEST") else {
            throw JarvisMacDeveloperBridgeError.bindingMismatch
        }
        let data = try strictEncoder.encode(reply)
        guard data.count <= Self.maximumDocumentBytes else {
            throw JarvisMacDeveloperBridgeError.documentTooLarge
        }
        return data
    }

    public func install(issuedReceiptData: Data) throws -> JarvisMacBridgeProfile {
        let receipt: JarvisMacIssuedDeviceCertificate = try decodeExact(
            issuedReceiptData,
            keys: [
                "status", "operation", "device_id", "device_name", "role", "registry_revision",
                "serial_hex", "issued_at_ms", "not_after_ms", "certificate_sha256",
                "certificate_pem", "ca_certificate_pem"
            ]
        )
        try receipt.validate()
        guard let invitation = try identityStore.loadStagedInvitation() else {
            throw JarvisMacDeveloperBridgeError.noStagedEnrollment
        }
        guard receipt.deviceID == invitation.deviceID,
              receipt.deviceName == invitation.deviceName,
              receipt.role == invitation.role,
              receipt.registryRevision == invitation.registryRevision else {
            throw JarvisMacDeveloperBridgeError.bindingMismatch
        }
        return try identityStore.install(receipt, for: invitation)
    }

    public func status() throws -> JarvisMacBridgeProfile? {
        try identityStore.loadInstalledProfile()
    }
}

public struct JarvisMacBridgeHTTPRequest: Equatable, Sendable {
    public let method: String
    public let path: String
    public let body: Data

    public init(method: String, path: String, body: Data = Data()) {
        self.method = method
        self.path = path
        self.body = body
    }
}

public struct JarvisMacBridgeHTTPResponse: Equatable, Sendable {
    public let status: Int
    public let body: Data

    public init(status: Int, body: Data) {
        self.status = status
        self.body = body
    }
}

public protocol JarvisMacAuthenticatedTLSChannel: Sendable {
    func tlsExporter(label: String, length: Int) async throws -> Data
    func send(_ request: JarvisMacBridgeHTTPRequest) async throws -> JarvisMacBridgeHTTPResponse
    func cancel() async
}

public protocol JarvisMacAuthenticatedTLSChannelFactory: Sendable {
    func connect(profile: JarvisMacBridgeProfile) async throws -> any JarvisMacAuthenticatedTLSChannel
}

public struct JarvisMacAuthenticatedBridgeSession: Sendable {
    public let connectionEpoch: UInt64
    public let profile: JarvisMacBridgeProfile
    private let channel: any JarvisMacAuthenticatedTLSChannel
    private let requestGate: JarvisMacBridgeRequestGate

    fileprivate init(
        connectionEpoch: UInt64,
        profile: JarvisMacBridgeProfile,
        channel: any JarvisMacAuthenticatedTLSChannel
    ) {
        self.connectionEpoch = connectionEpoch
        self.profile = profile
        self.channel = channel
        requestGate = JarvisMacBridgeRequestGate()
    }

    public func send(_ request: JarvisMacBridgeHTTPRequest) async throws -> JarvisMacBridgeHTTPResponse {
        try Task.checkCancellation()
        try await requestGate.begin()
        do {
            let response = try await channel.send(request)
            await requestGate.finish()
            return response
        } catch {
            await requestGate.finish()
            throw error
        }
    }

    public func cancel() async { await channel.cancel() }
}

actor JarvisMacBridgeRequestGate {
    private var active = false

    func begin() throws {
        guard !active else { throw JarvisMacDeveloperBridgeError.requestInFlight }
        active = true
    }

    func finish() {
        active = false
    }
}

public struct JarvisMacMTLSBridgeTransport: Sendable {
    public static let protocolVersion: UInt16 = 1
    public static let exporterLabel = "EXPORTER-Jarvis-Developer-Mode-v1"
    private let factory: any JarvisMacAuthenticatedTLSChannelFactory

    public init(factory: any JarvisMacAuthenticatedTLSChannelFactory = NetworkJarvisMacTLSChannelFactory()) {
        self.factory = factory
    }

    public func connect(profile: JarvisMacBridgeProfile) async throws -> JarvisMacAuthenticatedBridgeSession {
        let channel = try await factory.connect(profile: profile)
        do {
            try Task.checkCancellation()
            let exporter = try await channel.tlsExporter(label: Self.exporterLabel, length: 32)
            guard exporter.count == 32 else {
                throw JarvisMacDeveloperBridgeError.channelBindingUnavailable
            }
            let digest = Array(SHA256.hash(data: exporter))
            guard digest.contains(where: { $0 != 0 }) else {
                throw JarvisMacDeveloperBridgeError.channelBindingUnavailable
            }
            let body = try strictEncoder.encode(AuthenticatedHandshake(
                handshake: Handshake(
                    protocolVersion: Self.protocolVersion,
                    deviceID: profile.deviceID,
                    deviceName: profile.deviceName,
                    role: profile.role,
                    registryRevision: profile.registryRevision,
                    capabilities: profile.capabilities
                ),
                tlsExporterSHA256: digest
            ))
            guard body.count <= 64 * 1_024 else {
                throw JarvisMacDeveloperBridgeError.documentTooLarge
            }
            let response = try await channel.send(JarvisMacBridgeHTTPRequest(
                method: "POST",
                path: "/v1/distributed/connections/accept",
                body: body
            ))
            guard response.status == 200 else {
                throw JarvisMacDeveloperBridgeError.invalidResponse
            }
            let acceptance: HandshakeAcceptance = try decodeExact(
                response.body,
                keys: [
                    "protocol_version", "status", "connection_epoch",
                    "accepted_registry_revision", "reason_code"
                ],
                maximum: 64 * 1_024
            )
            guard acceptance.protocolVersion == Self.protocolVersion,
                  acceptance.status == "accepted",
                  acceptance.connectionEpoch > 0,
                  acceptance.acceptedRegistryRevision == profile.registryRevision,
                  acceptance.reasonCode == nil else {
                throw JarvisMacDeveloperBridgeError.bindingMismatch
            }
            return JarvisMacAuthenticatedBridgeSession(
                connectionEpoch: acceptance.connectionEpoch,
                profile: profile,
                channel: channel
            )
        } catch is CancellationError {
            await channel.cancel()
            throw JarvisMacDeveloperBridgeError.cancelled
        } catch {
            await channel.cancel()
            throw error
        }
    }
}

private struct Handshake: Encodable {
    let protocolVersion: UInt16
    let deviceID: String
    let deviceName: String
    let role: String
    let registryRevision: UInt64
    let capabilities: [JarvisMacBridgeCapability]

    enum CodingKeys: String, CodingKey {
        case protocolVersion = "protocol_version"
        case deviceID = "device_id"
        case deviceName = "device_name"
        case role
        case registryRevision = "registry_revision"
        case capabilities
    }
}

private struct AuthenticatedHandshake: Encodable {
    let handshake: Handshake
    let tlsExporterSHA256: [UInt8]

    enum CodingKeys: String, CodingKey {
        case handshake
        case tlsExporterSHA256 = "tls_exporter_sha256"
    }
}

private struct HandshakeAcceptance: Decodable {
    let protocolVersion: UInt16
    let status: String
    let connectionEpoch: UInt64
    let acceptedRegistryRevision: UInt64
    let reasonCode: String?

    enum CodingKeys: String, CodingKey {
        case protocolVersion = "protocol_version"
        case status
        case connectionEpoch = "connection_epoch"
        case acceptedRegistryRevision = "accepted_registry_revision"
        case reasonCode = "reason_code"
    }
}

private let strictEncoder: JSONEncoder = {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    return encoder
}()

private func decodeExact<T: Decodable>(
    _ data: Data,
    keys: Set<String>,
    maximum: Int = JarvisMacEnrollmentCoordinator.maximumDocumentBytes
) throws -> T {
    guard !data.isEmpty else { throw JarvisMacDeveloperBridgeError.invalidDocument }
    guard data.count <= maximum else { throw JarvisMacDeveloperBridgeError.documentTooLarge }
    guard let object = try? JSONSerialization.jsonObject(with: data),
          let dictionary = object as? [String: Any],
          Set(dictionary.keys) == keys,
          let decoded = try? JSONDecoder().decode(T.self, from: data) else {
        throw JarvisMacDeveloperBridgeError.invalidDocument
    }
    return decoded
}

private func validUUID(_ value: String) -> Bool {
    guard let uuid = UUID(uuidString: value) else { return false }
    return uuid != UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))
}

private func validIdentifier(_ value: String, maximum: Int) -> Bool {
    !value.isEmpty && value.utf8.count <= maximum && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
        && value.utf8.allSatisfy { $0 >= 0x21 && $0 <= 0x7e }
}

private func validLowercaseHex(_ value: String, count: Int) -> Bool {
    value.utf8.count == count && value.utf8.allSatisfy(isLowercaseHex)
}

private func validLowercaseHex(_ value: String, minimum: Int, maximum: Int) -> Bool {
    value.utf8.count >= minimum && value.utf8.count <= maximum
        && value.utf8.count.isMultiple(of: 2) && value.utf8.allSatisfy(isLowercaseHex)
}

private func isLowercaseHex(_ byte: UInt8) -> Bool {
    (byte >= 48 && byte <= 57) || (byte >= 97 && byte <= 102)
}

private func validPEM(_ value: String, label: String) -> Bool {
    guard value.utf8.count <= 64 * 1_024 else { return false }
    let normalized: String
    if value.contains("\r\n") {
        let withoutCRLF = value.replacingOccurrences(of: "\r\n", with: "")
        guard !withoutCRLF.contains("\r"), !withoutCRLF.contains("\n") else { return false }
        normalized = value.replacingOccurrences(of: "\r\n", with: "\n")
    } else {
        guard !value.contains("\r") else { return false }
        normalized = value
    }
    return normalized.hasPrefix("-----BEGIN \(label)-----\n")
        && normalized.hasSuffix("-----END \(label)-----\n")
}

private func validEndpoint(_ value: String) -> Bool {
    guard value.utf8.count <= 255, !value.contains("/"), !value.contains("@"),
          let separator = value.lastIndex(of: ":"), separator != value.startIndex else { return false }
    let encodedHost = String(value[..<separator])
    let host: String
    if encodedHost.hasPrefix("[") && encodedHost.hasSuffix("]") {
        host = String(encodedHost.dropFirst().dropLast())
    } else {
        guard !encodedHost.contains(":") else { return false }
        host = encodedHost
    }
    let port = UInt16(value[value.index(after: separator)...])
    guard !host.isEmpty, port != nil, port != 0 else { return false }
    var ipv4 = in_addr()
    if inet_pton(AF_INET, host, &ipv4) == 1 {
        let bytes = withUnsafeBytes(of: &ipv4.s_addr) { Array($0) }
        return !bytes.allSatisfy { $0 == 0 } && !(224...239).contains(Int(bytes[0]))
    }
    var ipv6 = in6_addr()
    if inet_pton(AF_INET6, host, &ipv6) == 1 {
        let bytes = withUnsafeBytes(of: &ipv6) { Array($0) }
        return !bytes.allSatisfy { $0 == 0 } && bytes.first != 0xff
    }
    return false
}
