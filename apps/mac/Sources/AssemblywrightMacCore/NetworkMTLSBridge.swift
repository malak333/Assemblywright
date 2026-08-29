import Foundation
import Network
import Security

public struct NetworkAssemblywrightMacTLSChannelFactory: AssemblywrightMacAuthenticatedTLSChannelFactory, Sendable {
    private let identityStore: KeychainAssemblywrightMacBridgeIdentityStore

    public init(
        identityStore: KeychainAssemblywrightMacBridgeIdentityStore =
            .init(identityProfile: .standard)
    ) {
        self.identityStore = identityStore
    }

    public func connect(profile: AssemblywrightMacBridgeProfile) async throws -> any AssemblywrightMacAuthenticatedTLSChannel {
        let validated = try Self.validatedOrdinaryProfile(
            profile,
            keychainProfile: identityStore.loadInstalledProfile()
        )
        let material = try loadIdentityMaterial(matching: validated)
        return try await connectValidated(profile: validated, material: material)
    }

    public func connectForLocalModelReconciliation(
        profile: AssemblywrightMacBridgeProfile,
        installedProfile: AssemblywrightMacBridgeProfile,
        requestedModelID: String
    ) async throws -> any AssemblywrightMacAuthenticatedTLSChannel {
        let validated = try Self.validatedLocalModelReconciliationTarget(
            profile,
            installedProfile: installedProfile,
            keychainProfile: identityStore.loadInstalledProfile(),
            requestedModelID: requestedModelID
        )
        let material = try loadIdentityMaterial(matching: installedProfile)
        return try await connectValidated(profile: validated, material: material)
    }

    static func validatedOrdinaryProfile(
        _ profile: AssemblywrightMacBridgeProfile,
        keychainProfile: AssemblywrightMacBridgeProfile?
    ) throws -> AssemblywrightMacBridgeProfile {
        guard keychainProfile == profile else {
            throw AssemblywrightMacDeveloperBridgeError.bindingMismatch
        }
        return profile
    }

    static func validatedLocalModelReconciliationTarget(
        _ profile: AssemblywrightMacBridgeProfile,
        installedProfile: AssemblywrightMacBridgeProfile,
        keychainProfile: AssemblywrightMacBridgeProfile?,
        requestedModelID: String
    ) throws -> AssemblywrightMacBridgeProfile {
        guard keychainProfile == installedProfile,
              profile == (try AssemblywrightMacLocalModelSelectionControl.reconciliationTarget(
                installedProfile: installedProfile,
                requestedModelID: requestedModelID
              )) else {
            throw AssemblywrightMacDeveloperBridgeError.bindingMismatch
        }
        return profile
    }

    private func loadIdentityMaterial(
        matching profile: AssemblywrightMacBridgeProfile
    ) throws -> AssemblywrightMacTLSIdentityMaterial {
        let material = try identityStore.loadTLSIdentityMaterial()
        guard try identityStore.loadInstalledProfile() == profile else {
            throw AssemblywrightMacDeveloperBridgeError.bindingMismatch
        }
        return material
    }

    private func connectValidated(
        profile: AssemblywrightMacBridgeProfile,
        material: AssemblywrightMacTLSIdentityMaterial
    ) async throws -> any AssemblywrightMacAuthenticatedTLSChannel {
        let endpoint = try ParsedMasterEndpoint(profile.masterEndpoint)
        let tls = NWProtocolTLS.Options()
        let options = tls.securityProtocolOptions
        sec_protocol_options_set_min_tls_protocol_version(options, .TLSv13)
        sec_protocol_options_set_max_tls_protocol_version(options, .TLSv13)
        sec_protocol_options_set_peer_authentication_required(options, true)
        sec_protocol_options_set_tls_server_name(options, endpoint.host)
        sec_protocol_options_add_tls_application_protocol(options, "http/1.1")
        guard let protocolIdentity = sec_identity_create(material.identity) else {
            throw AssemblywrightMacDeveloperBridgeError.identityUnavailable
        }
        sec_protocol_options_set_local_identity(options, protocolIdentity)

        let trustMaterial = TrustMaterial(ca: material.caCertificate, serverName: endpoint.host)
        let verificationQueue = DispatchQueue(label: "com.nobiletechnology.assemblywright.developer-bridge.verify")
        sec_protocol_options_set_verify_block(options, { _, protocolTrust, complete in
            let retainedTrust = sec_trust_copy_ref(protocolTrust)
            let trust = retainedTrust.takeRetainedValue()
            let policy = SecPolicyCreateSSL(true, trustMaterial.serverName as CFString)
            guard SecTrustSetPolicies(trust, policy) == errSecSuccess,
                  SecTrustSetAnchorCertificates(trust, [trustMaterial.ca] as CFArray) == errSecSuccess,
                  SecTrustSetAnchorCertificatesOnly(trust, true) == errSecSuccess else {
                complete(false)
                return
            }
            complete(SecTrustEvaluateWithError(trust, nil))
        }, verificationQueue)

        let parameters = NWParameters(tls: tls, tcp: NWProtocolTCP.Options())
        parameters.includePeerToPeer = false
        let connection = NWConnection(
            host: NWEndpoint.Host(endpoint.host),
            port: NWEndpoint.Port(rawValue: endpoint.port)!,
            using: parameters
        )
        let channel = NetworkAssemblywrightMacTLSChannel(
            connection: connection,
            hostHeader: profile.masterEndpoint
        )
        try await channel.start()
        return channel
    }
}

private final class TrustMaterial: @unchecked Sendable {
    let ca: SecCertificate
    let serverName: String

    init(ca: SecCertificate, serverName: String) {
        self.ca = ca
        self.serverName = serverName
    }
}

private final class ConnectionHolder: @unchecked Sendable {
    let connection: NWConnection
    init(_ connection: NWConnection) { self.connection = connection }
    func cancel() { connection.cancel() }
}

enum AssemblywrightMacHTTP1ResponseParser {
    static func parseResponseIfComplete(
        _ readBuffer: inout Data,
        maximumHeaderBytes: Int,
        maximumWireBytes: Int
    ) throws -> AssemblywrightMacBridgeHTTPResponse? {
        let delimiter = Data("\r\n\r\n".utf8)
        guard let headerRange = readBuffer.range(of: delimiter) else {
            guard readBuffer.count <= maximumHeaderBytes else {
                throw AssemblywrightMacDeveloperBridgeError.invalidResponse
            }
            return nil
        }
        guard headerRange.lowerBound <= maximumHeaderBytes,
              let header = String(data: readBuffer[..<headerRange.lowerBound], encoding: .ascii)
        else {
            throw AssemblywrightMacDeveloperBridgeError.invalidResponse
        }
        let lines = header.components(separatedBy: "\r\n")
        guard let statusLine = lines.first else {
            throw AssemblywrightMacDeveloperBridgeError.invalidResponse
        }
        let statusParts = statusLine.split(separator: " ", maxSplits: 2)
        guard statusParts.count >= 2, statusParts[0] == "HTTP/1.1",
              let status = Int(statusParts[1]), (100 ... 599).contains(status)
        else {
            throw AssemblywrightMacDeveloperBridgeError.invalidResponse
        }
        var contentLengths: [Int] = []
        for line in lines.dropFirst() {
            guard let separator = line.firstIndex(of: ":") else {
                throw AssemblywrightMacDeveloperBridgeError.invalidResponse
            }
            let rawName = line[..<separator]
            guard !rawName.isEmpty,
                  rawName.utf8.allSatisfy(Self.isHTTPFieldNameByte) else {
                throw AssemblywrightMacDeveloperBridgeError.invalidResponse
            }
            let name = rawName.lowercased()
            let value = line[line.index(after: separator)...]
                .trimmingCharacters(in: .whitespaces)
            if name == "transfer-encoding" {
                throw AssemblywrightMacDeveloperBridgeError.invalidResponse
            }
            if name == "content-length" {
                guard !value.isEmpty,
                      value.utf8.allSatisfy({ (48 ... 57).contains($0) }),
                      let length = Int(value) else {
                    throw AssemblywrightMacDeveloperBridgeError.invalidResponse
                }
                contentLengths.append(length)
            }
        }
        let contentLength: Int
        if status == 204 {
            guard contentLengths.isEmpty
                    || (contentLengths.count == 1 && contentLengths.first == 0)
            else {
                throw AssemblywrightMacDeveloperBridgeError.invalidResponse
            }
            contentLength = 0
        } else {
            guard contentLengths.count == 1, let length = contentLengths.first else {
                throw AssemblywrightMacDeveloperBridgeError.invalidResponse
            }
            contentLength = length
        }
        guard contentLength >= 0, contentLength <= maximumWireBytes else {
            throw AssemblywrightMacDeveloperBridgeError.invalidResponse
        }
        let bodyStart = headerRange.upperBound
        guard readBuffer.count >= bodyStart + contentLength else { return nil }
        if status == 204, readBuffer.count != bodyStart {
            throw AssemblywrightMacDeveloperBridgeError.invalidResponse
        }
        let body = readBuffer.subdata(in: bodyStart ..< (bodyStart + contentLength))
        readBuffer.removeSubrange(0 ..< (bodyStart + contentLength))
        return AssemblywrightMacBridgeHTTPResponse(status: status, body: body)
    }

    private static func isHTTPFieldNameByte(_ byte: UInt8) -> Bool {
        switch byte {
        case 48 ... 57, 65 ... 90, 97 ... 122:
            true
        case 33, 35, 36, 37, 38, 39, 42, 43, 45, 46, 94, 95, 96, 124, 126:
            true
        default:
            false
        }
    }
}

private actor NetworkAssemblywrightMacTLSChannel: AssemblywrightMacAuthenticatedTLSChannel {
    static let maximumWireBytes = 1_024 * 1_024
    static let maximumHeaderBytes = 32 * 1_024
    static let startTimeoutNanoseconds: UInt64 = 10 * 1_000_000_000
    static let requestTimeoutNanoseconds: UInt64 = 15 * 1_000_000_000

    private let holder: ConnectionHolder
    private let hostHeader: String
    private var readBuffer = Data()
    private var ready = false
    private var closed = false
    private var requestInFlight = false

    init(connection: NWConnection, hostHeader: String) {
        holder = ConnectionHolder(connection)
        self.hostHeader = hostHeader
    }

    func start() async throws {
        let holder = holder
        let timeout = cancellationDeadline(
            holder: holder,
            nanoseconds: Self.startTimeoutNanoseconds
        )
        defer { timeout.cancel() }
        do {
            try await withTaskCancellationHandler {
                try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
                    let gate = ContinuationGate(continuation)
                    holder.connection.stateUpdateHandler = { state in
                        switch state {
                        case .ready:
                            gate.resume()
                        case .failed, .cancelled:
                            gate.resume(throwing: AssemblywrightMacDeveloperBridgeError.connectionFailed)
                        default:
                            break
                        }
                    }
                    holder.connection.start(
                        queue: DispatchQueue(label: "com.nobiletechnology.assemblywright.developer-bridge.connection")
                    )
                }
            } onCancel: {
                holder.cancel()
            }
            ready = true
        } catch is CancellationError {
            holder.cancel()
            closed = true
            throw AssemblywrightMacDeveloperBridgeError.cancelled
        } catch {
            holder.cancel()
            closed = true
            if Task.isCancelled { throw AssemblywrightMacDeveloperBridgeError.cancelled }
            throw error
        }
    }

    func tlsExporter(label: String, length: Int) async throws -> Data {
        guard ready, !closed, length == 32,
              let metadata = holder.connection.metadata(definition: NWProtocolTLS.definition)
                as? NWProtocolTLS.Metadata else {
            throw AssemblywrightMacDeveloperBridgeError.channelBindingUnavailable
        }
        let securityMetadata = metadata.securityProtocolMetadata
        guard sec_protocol_metadata_get_negotiated_tls_protocol_version(securityMetadata) == .TLSv13 else {
            throw AssemblywrightMacDeveloperBridgeError.tlsProtocolRejected
        }
        let labelBytes = label.utf8CString
        let secret: DispatchData? = labelBytes.withUnsafeBufferPointer { buffer in
            guard let pointer = buffer.baseAddress else { return nil }
            return sec_protocol_metadata_create_secret(
                securityMetadata, label.utf8.count, pointer, length
            ) as DispatchData?
        }
        guard let secret else {
            throw AssemblywrightMacDeveloperBridgeError.channelBindingUnavailable
        }
        let data = secret.withUnsafeBytes { (pointer: UnsafePointer<UInt8>) in
            Data(bytes: pointer, count: secret.count)
        }
        guard data.count == length else {
            throw AssemblywrightMacDeveloperBridgeError.channelBindingUnavailable
        }
        return data
    }

    func send(_ request: AssemblywrightMacBridgeHTTPRequest) async throws -> AssemblywrightMacBridgeHTTPResponse {
        guard ready, !closed else { throw AssemblywrightMacDeveloperBridgeError.unauthenticatedSession }
        guard !requestInFlight else { throw AssemblywrightMacDeveloperBridgeError.requestInFlight }
        guard request.method == "GET" || request.method == "POST",
              request.path.hasPrefix("/"), !request.path.contains("\r"), !request.path.contains("\n"),
              request.body.count <= Self.maximumWireBytes else {
            throw AssemblywrightMacDeveloperBridgeError.invalidDocument
        }
        requestInFlight = true
        let timeout = cancellationDeadline(
            holder: holder,
            nanoseconds: Self.requestTimeoutNanoseconds
        )
        defer {
            timeout.cancel()
            requestInFlight = false
        }
        var wire = Data(
            "\(request.method) \(request.path) HTTP/1.1\r\nHost: \(hostHeader)\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: \(request.body.count)\r\nConnection: keep-alive\r\n\r\n".utf8
        )
        wire.append(request.body)
        do {
            try Task.checkCancellation()
            try await sendData(wire)
            return try await readResponse()
        } catch is CancellationError {
            await cancel()
            throw AssemblywrightMacDeveloperBridgeError.cancelled
        } catch {
            await cancel()
            if Task.isCancelled { throw AssemblywrightMacDeveloperBridgeError.cancelled }
            throw error
        }
    }

    func cancel() async {
        guard !closed else { return }
        closed = true
        ready = false
        holder.cancel()
    }

    private func sendData(_ data: Data) async throws {
        let holder = holder
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                holder.connection.send(content: data, completion: .contentProcessed { error in
                    if error == nil {
                        continuation.resume(returning: ())
                    } else {
                        continuation.resume(throwing: AssemblywrightMacDeveloperBridgeError.connectionFailed)
                    }
                })
            }
        } onCancel: {
            holder.cancel()
        }
    }

    private func readResponse() async throws -> AssemblywrightMacBridgeHTTPResponse {
        while true {
            if let response = try parseResponseIfComplete() { return response }
            let chunk = try await receiveChunk()
            guard !chunk.isEmpty else { throw AssemblywrightMacDeveloperBridgeError.invalidResponse }
            guard readBuffer.count <= Self.maximumWireBytes + Self.maximumHeaderBytes - chunk.count else {
                throw AssemblywrightMacDeveloperBridgeError.responseTooLarge
            }
            readBuffer.append(chunk)
        }
    }

    private func receiveChunk() async throws -> Data {
        let holder = holder
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                holder.connection.receive(
                    minimumIncompleteLength: 1,
                    maximumLength: 64 * 1_024
                ) { data, _, isComplete, error in
                    if let error {
                        _ = error
                        continuation.resume(throwing: AssemblywrightMacDeveloperBridgeError.connectionFailed)
                    } else if let data, !data.isEmpty {
                        continuation.resume(returning: data)
                    } else if isComplete {
                        continuation.resume(returning: Data())
                    } else {
                        continuation.resume(throwing: AssemblywrightMacDeveloperBridgeError.invalidResponse)
                    }
                }
            }
        } onCancel: {
            holder.cancel()
        }
    }

    private func parseResponseIfComplete() throws -> AssemblywrightMacBridgeHTTPResponse? {
        try AssemblywrightMacHTTP1ResponseParser.parseResponseIfComplete(
            &readBuffer,
            maximumHeaderBytes: Self.maximumHeaderBytes,
            maximumWireBytes: Self.maximumWireBytes
        )
    }
}

private func cancellationDeadline(
    holder: ConnectionHolder,
    nanoseconds: UInt64
) -> Task<Void, Never> {
    Task {
        do {
            try await Task.sleep(nanoseconds: nanoseconds)
        } catch {
            return
        }
        guard !Task.isCancelled else { return }
        holder.cancel()
    }
}

private final class ContinuationGate: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Void, Error>?

    init(_ continuation: CheckedContinuation<Void, Error>) {
        self.continuation = continuation
    }

    func resume() {
        let continuation = lock.withLock { () -> CheckedContinuation<Void, Error>? in
            defer { self.continuation = nil }
            return self.continuation
        }
        continuation?.resume(returning: ())
    }

    func resume(throwing error: Error) {
        let continuation = lock.withLock { () -> CheckedContinuation<Void, Error>? in
            defer { self.continuation = nil }
            return self.continuation
        }
        continuation?.resume(throwing: error)
    }
}

private struct ParsedMasterEndpoint {
    let host: String
    let port: UInt16

    init(_ value: String) throws {
        guard let separator = value.lastIndex(of: ":"), separator != value.startIndex,
              let port = UInt16(value[value.index(after: separator)...]), port > 0 else {
            throw AssemblywrightMacDeveloperBridgeError.invalidInvitation
        }
        var host = String(value[..<separator])
        if host.hasPrefix("[") && host.hasSuffix("]") {
            host = String(host.dropFirst().dropLast())
        }
        guard !host.isEmpty else { throw AssemblywrightMacDeveloperBridgeError.invalidInvitation }
        self.host = host
        self.port = port
    }
}
