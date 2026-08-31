import Foundation
import Darwin
import CoreFoundation

public struct AssemblywrightIPCTransportRequest: Equatable, Sendable {
    public let method: String
    public let path: String
    public let authorization: String
    public let accept: String?
    public let contentType: String?
    public let body: Data?

    public init(
        method: String,
        path: String,
        authorization: String,
        accept: String? = nil,
        contentType: String? = nil,
        body: Data? = nil
    ) {
        self.method = method
        self.path = path
        self.authorization = authorization
        self.accept = accept
        self.contentType = contentType
        self.body = body
    }
}

public struct AssemblywrightIPCTransportResponse: Equatable, Sendable {
    public let status: Int
    public let contentType: String?
    public let body: Data
}

public enum AssemblywrightUnixSocketTransportError: Error, Equatable, Sendable {
    case cancelled
    case invalidRequest
    case invalidSocketPath
    case connectionFailed
    case peerIdentityUnavailable
    case peerUIDMismatch
    case timedOut
    case writeFailed
    case readFailed
    case frameTooLarge
    case invalidResponse
}

public protocol AssemblywrightUnixSocketRequesting: Sendable {
    func send(
        _ request: AssemblywrightIPCTransportRequest,
        to socketURL: URL
    ) async throws -> AssemblywrightIPCTransportResponse
}

public struct DarwinAssemblywrightUnixSocketTransport: AssemblywrightUnixSocketRequesting {
    public static let maximumRequestFrameBytes = 2 * 1024 * 1024
    public static let maximumRequestBodyBytes = 1 * 1024 * 1024
    public static let maximumResponseFrameBytes = 12 * 1024 * 1024
    public static let maximumResponseBodyBytes = 8 * 1024 * 1024
    public static let maximumTimeoutSeconds = 610
    public static let authenticatedPeerRequestTimeoutSeconds = 47
    public var timeoutSeconds: Int
    private let peerIdentityPolicy: @Sendable () throws -> AssemblywrightIPCPeerIdentityPolicy?
    private let peerIdentityVerifier: any AssemblywrightUnixPeerIdentityVerifying

    public init(
        timeoutSeconds: Int = 300,
        peerIdentityPolicy: @escaping @Sendable () throws -> AssemblywrightIPCPeerIdentityPolicy? = { nil },
        peerIdentityVerifier: any AssemblywrightUnixPeerIdentityVerifying = SecurityAssemblywrightUnixPeerIdentityVerifier()
    ) {
        self.timeoutSeconds = min(max(timeoutSeconds, 1), Self.maximumTimeoutSeconds)
        self.peerIdentityPolicy = peerIdentityPolicy
        self.peerIdentityVerifier = peerIdentityVerifier
    }

    static func validatePeerUID(_ peerUID: uid_t, currentEUID: uid_t) throws {
        guard peerUID == currentEUID else {
            throw AssemblywrightUnixSocketTransportError.peerUIDMismatch
        }
    }

    public func send(
        _ request: AssemblywrightIPCTransportRequest,
        to socketURL: URL
    ) async throws -> AssemblywrightIPCTransportResponse {
        let operation = AssemblywrightUnixSocketOperation(
            request: request,
            socketURL: socketURL,
            timeoutSeconds: timeoutSeconds,
            peerIdentityPolicy: peerIdentityPolicy,
            peerIdentityVerifier: peerIdentityVerifier
        )
        return try await withTaskCancellationHandler {
            try await Task.detached(priority: .userInitiated) {
                try operation.execute()
            }.value
        } onCancel: {
            operation.cancel()
        }
    }
}

private final class AssemblywrightUnixSocketOperation: @unchecked Sendable {
    private let request: AssemblywrightIPCTransportRequest
    private let socketURL: URL
    private let deadlineNanoseconds: UInt64
    private let peerIdentityPolicy: @Sendable () throws -> AssemblywrightIPCPeerIdentityPolicy?
    private let peerIdentityVerifier: any AssemblywrightUnixPeerIdentityVerifying
    private let lock = NSLock()
    private var descriptor: Int32 = -1
    private var cancelled = false

    init(
        request: AssemblywrightIPCTransportRequest,
        socketURL: URL,
        timeoutSeconds: Int,
        peerIdentityPolicy: @escaping @Sendable () throws -> AssemblywrightIPCPeerIdentityPolicy?,
        peerIdentityVerifier: any AssemblywrightUnixPeerIdentityVerifying
    ) {
        self.request = request
        self.socketURL = socketURL
        self.peerIdentityPolicy = peerIdentityPolicy
        self.peerIdentityVerifier = peerIdentityVerifier
        let timeoutNanoseconds = UInt64(timeoutSeconds) * 1_000_000_000
        let now = DispatchTime.now().uptimeNanoseconds
        let (deadline, overflowed) = now.addingReportingOverflow(timeoutNanoseconds)
        self.deadlineNanoseconds = overflowed ? UInt64.max : deadline
    }

    func cancel() {
        lock.lock()
        cancelled = true
        let activeDescriptor = descriptor
        descriptor = -1
        lock.unlock()
        if activeDescriptor >= 0 {
            _ = Darwin.shutdown(activeDescriptor, SHUT_RDWR)
            _ = Darwin.close(activeDescriptor)
        }
    }

    func execute() throws -> AssemblywrightIPCTransportResponse {
        let payload = try encodeRequest()
        guard payload.count <= DarwinAssemblywrightUnixSocketTransport.maximumRequestFrameBytes else {
            throw AssemblywrightUnixSocketTransportError.frameTooLarge
        }
        let socketDescriptor = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard socketDescriptor >= 0 else {
            throw AssemblywrightUnixSocketTransportError.connectionFailed
        }
        guard install(socketDescriptor) else {
            _ = Darwin.close(socketDescriptor)
            throw AssemblywrightUnixSocketTransportError.cancelled
        }
        defer { finish(socketDescriptor) }
        try configure(socketDescriptor)
        try connect(socketDescriptor)
        try verifyPeer(socketDescriptor)

        var length = UInt32(payload.count).bigEndian
        let prefix = withUnsafeBytes(of: &length) { Data($0) }
        try writeAll(prefix, to: socketDescriptor)
        try writeAll(payload, to: socketDescriptor)
        try shutdownWrite(socketDescriptor)

        let responsePrefix = try readExactly(4, from: socketDescriptor)
        var encodedLength: UInt32 = 0
        _ = withUnsafeMutableBytes(of: &encodedLength) { responsePrefix.copyBytes(to: $0) }
        let responseLength = Int(UInt32(bigEndian: encodedLength))
        guard responseLength > 0,
              responseLength <= DarwinAssemblywrightUnixSocketTransport.maximumResponseFrameBytes else {
            throw AssemblywrightUnixSocketTransportError.frameTooLarge
        }
        return try decodeResponse(try readExactly(responseLength, from: socketDescriptor))
    }

    private func install(_ socketDescriptor: Int32) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard !cancelled else { return false }
        descriptor = socketDescriptor
        return true
    }

    private func finish(_ socketDescriptor: Int32) {
        lock.lock()
        let ownsDescriptor = descriptor == socketDescriptor
        if ownsDescriptor { descriptor = -1 }
        lock.unlock()
        if ownsDescriptor { _ = Darwin.close(socketDescriptor) }
    }

    private func checkCancellation() throws {
        lock.lock()
        let isCancelled = cancelled
        lock.unlock()
        if isCancelled { throw AssemblywrightUnixSocketTransportError.cancelled }
    }

    private func configure(_ socketDescriptor: Int32) throws {
        var enabled: Int32 = 1
        guard Darwin.setsockopt(
            socketDescriptor,
            SOL_SOCKET,
            SO_NOSIGPIPE,
            &enabled,
            socklen_t(MemoryLayout.size(ofValue: enabled))
        ) == 0 else {
            throw AssemblywrightUnixSocketTransportError.connectionFailed
        }
    }

    private func connect(_ socketDescriptor: Int32) throws {
        guard socketURL.isFileURL, socketURL.path.hasPrefix("/") else {
            throw AssemblywrightUnixSocketTransportError.invalidSocketPath
        }
        let pathBytes = Array(socketURL.path.utf8)
        guard !pathBytes.isEmpty, pathBytes.count < 104 else {
            throw AssemblywrightUnixSocketTransportError.invalidSocketPath
        }
        var address = sockaddr_un()
        let addressLength = MemoryLayout.offset(of: \sockaddr_un.sun_path)! + pathBytes.count + 1
        address.sun_len = UInt8(addressLength)
        address.sun_family = sa_family_t(AF_UNIX)
        withUnsafeMutableBytes(of: &address.sun_path) { destination in
            destination.copyBytes(from: pathBytes)
            destination[pathBytes.count] = 0
        }
        while true {
            try applyRemainingTimeout(SO_SNDTIMEO, to: socketDescriptor)
            let result = withUnsafePointer(to: &address) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    Darwin.connect(socketDescriptor, $0, socklen_t(addressLength))
                }
            }
            if result == 0 { return }
            let code = errno
            if code == EINTR { continue }
            try checkCancellation()
            if Self.isTimeoutError(code) || deadlineExpired {
                throw AssemblywrightUnixSocketTransportError.timedOut
            }
            throw AssemblywrightUnixSocketTransportError.connectionFailed
        }
    }

    private func verifyPeer(_ socketDescriptor: Int32) throws {
        var peerUID: uid_t = 0
        var peerGID: gid_t = 0
        guard Darwin.getpeereid(socketDescriptor, &peerUID, &peerGID) == 0 else {
            throw AssemblywrightUnixSocketTransportError.peerIdentityUnavailable
        }
        try DarwinAssemblywrightUnixSocketTransport.validatePeerUID(
            peerUID,
            currentEUID: Darwin.geteuid()
        )
        guard let policy = try peerIdentityPolicy() else {
            throw AssemblywrightUnixSocketTransportError.peerIdentityUnavailable
        }
        do {
            try peerIdentityVerifier.verifyPeer(on: socketDescriptor, policy: policy)
        } catch {
            throw AssemblywrightUnixSocketTransportError.peerIdentityUnavailable
        }
    }

    private func writeAll(_ data: Data, to socketDescriptor: Int32) throws {
        try data.withUnsafeBytes { bytes in
            guard let baseAddress = bytes.baseAddress else { return }
            var written = 0
            while written < bytes.count {
                try checkCancellation()
                try applyRemainingTimeout(SO_SNDTIMEO, to: socketDescriptor)
                let result = Darwin.write(
                    socketDescriptor,
                    baseAddress.advanced(by: written),
                    bytes.count - written
                )
                if result < 0, errno == EINTR { continue }
                guard result > 0 else {
                    let code = errno
                    try checkCancellation()
                    if Self.isTimeoutError(code) || deadlineExpired {
                        throw AssemblywrightUnixSocketTransportError.timedOut
                    }
                    throw AssemblywrightUnixSocketTransportError.writeFailed
                }
                written += result
            }
        }
    }

    private func readExactly(_ count: Int, from socketDescriptor: Int32) throws -> Data {
        var data = Data(count: count)
        var readCount = 0
        try data.withUnsafeMutableBytes { bytes in
            guard let baseAddress = bytes.baseAddress else { return }
            while readCount < count {
                try checkCancellation()
                try applyRemainingTimeout(SO_RCVTIMEO, to: socketDescriptor)
                let result = Darwin.read(
                    socketDescriptor,
                    baseAddress.advanced(by: readCount),
                    count - readCount
                )
                if result < 0, errno == EINTR { continue }
                guard result > 0 else {
                    let code = errno
                    try checkCancellation()
                    if (result < 0 && Self.isTimeoutError(code)) || deadlineExpired {
                        throw AssemblywrightUnixSocketTransportError.timedOut
                    }
                    throw AssemblywrightUnixSocketTransportError.readFailed
                }
                readCount += result
            }
        }
        return data
    }

    private func shutdownWrite(_ socketDescriptor: Int32) throws {
        try checkCancellation()
        guard !deadlineExpired else {
            throw AssemblywrightUnixSocketTransportError.timedOut
        }
        while Darwin.shutdown(socketDescriptor, SHUT_WR) != 0 {
            if errno == EINTR { continue }
            try checkCancellation()
            throw AssemblywrightUnixSocketTransportError.writeFailed
        }
    }

    private var deadlineExpired: Bool {
        DispatchTime.now().uptimeNanoseconds >= deadlineNanoseconds
    }

    private func applyRemainingTimeout(_ option: Int32, to socketDescriptor: Int32) throws {
        try checkCancellation()
        let now = DispatchTime.now().uptimeNanoseconds
        guard now < deadlineNanoseconds else {
            throw AssemblywrightUnixSocketTransportError.timedOut
        }
        let remaining = deadlineNanoseconds - now
        var seconds = remaining / 1_000_000_000
        var microseconds = (remaining % 1_000_000_000 + 999) / 1_000
        if microseconds == 1_000_000 {
            seconds += 1
            microseconds = 0
        }
        var timeout = timeval(
            tv_sec: Int(seconds),
            tv_usec: Int32(microseconds)
        )
        let result = Darwin.setsockopt(
            socketDescriptor,
            SOL_SOCKET,
            option,
            &timeout,
            socklen_t(MemoryLayout.size(ofValue: timeout))
        )
        if result != 0 {
            // Darwin may reject an SO_RCVTIMEO update with EINVAL after the
            // local write side is closed. Polling for the exact remaining
            // monotonic budget makes the following read non-blocking in
            // practice (data or EOF is ready) without weakening the deadline.
            if errno == EINVAL, option == SO_RCVTIMEO {
                try waitForReady(POLLIN, on: socketDescriptor)
                return
            }
            throw AssemblywrightUnixSocketTransportError.connectionFailed
        }
    }

    private func waitForReady(_ event: Int32, on socketDescriptor: Int32) throws {
        while true {
            try checkCancellation()
            let now = DispatchTime.now().uptimeNanoseconds
            guard now < deadlineNanoseconds else {
                throw AssemblywrightUnixSocketTransportError.timedOut
            }
            let remaining = deadlineNanoseconds - now
            let milliseconds = min(
                (remaining + 999_999) / 1_000_000,
                UInt64(Int32.max)
            )
            var state = pollfd(
                fd: socketDescriptor,
                events: Int16(event),
                revents: 0
            )
            let result = Darwin.poll(&state, 1, Int32(milliseconds))
            if result < 0, errno == EINTR { continue }
            if result == 0 { throw AssemblywrightUnixSocketTransportError.timedOut }
            guard result > 0,
                  state.revents & Int16(POLLNVAL | POLLERR) == 0 else {
                try checkCancellation()
                throw AssemblywrightUnixSocketTransportError.connectionFailed
            }
            if state.revents & Int16(event | POLLHUP) != 0 { return }
        }
    }

    private static func isTimeoutError(_ code: Int32) -> Bool {
        code == EAGAIN || code == EWOULDBLOCK || code == ETIMEDOUT
    }

    private func encodeRequest() throws -> Data {
        guard ["GET", "POST", "DELETE", "PATCH"].contains(request.method),
              request.path.hasPrefix("/"), !request.path.hasPrefix("//"),
              !request.path.contains("//"), !request.path.contains("#"),
              !request.path.unicodeScalars.contains(where: { $0.value < 0x20 || $0.value == 0x7f }),
              request.path.utf8.count <= 8 * 1024,
              !request.authorization.isEmpty, request.authorization.utf8.count <= 1024,
              request.accept?.utf8.count ?? 0 <= 1024,
              request.contentType?.utf8.count ?? 0 <= 1024,
              request.body?.count ?? 0 <= DarwinAssemblywrightUnixSocketTransport.maximumRequestBodyBytes else {
            throw AssemblywrightUnixSocketTransportError.invalidRequest
        }
        let object: [String: Any] = [
            "version": 1,
            "method": request.method,
            "path": request.path,
            "authorization": request.authorization,
            "accept": request.accept ?? NSNull(),
            "content_type": request.contentType ?? NSNull(),
            "body_base64": request.body?.base64EncodedString() ?? ""
        ]
        guard JSONSerialization.isValidJSONObject(object) else {
            throw AssemblywrightUnixSocketTransportError.invalidRequest
        }
        return try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
    }

    private func decodeResponse(_ data: Data) throws -> AssemblywrightIPCTransportResponse {
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys) == Set(["version", "status", "content_type", "body_base64"]),
              Self.strictJSONInteger(object["version"]) == 1,
              let status = Self.strictJSONInteger(object["status"]),
              (100...599).contains(status),
              object.keys.contains("content_type"),
              object.keys.contains("body_base64") else {
            throw AssemblywrightUnixSocketTransportError.invalidResponse
        }
        let contentType: String?
        if object["content_type"] is NSNull {
            contentType = nil
        } else if let value = object["content_type"] as? String, value.utf8.count <= 256 {
            contentType = value
        } else {
            throw AssemblywrightUnixSocketTransportError.invalidResponse
        }
        let body: Data
        if let value = object["body_base64"] as? String,
           let decoded = Data(base64Encoded: value),
           decoded.count <= DarwinAssemblywrightUnixSocketTransport.maximumResponseBodyBytes {
            body = decoded
        } else {
            throw AssemblywrightUnixSocketTransportError.invalidResponse
        }
        return AssemblywrightIPCTransportResponse(status: status, contentType: contentType, body: body)
    }

    private static func strictJSONInteger(_ value: Any?) -> Int? {
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else {
            return nil
        }
        switch UnicodeScalar(UInt8(bitPattern: number.objCType.pointee)) {
        case "c", "s", "i", "l", "q", "C", "S", "I", "L", "Q":
            return number.intValue
        default:
            return nil
        }
    }
}
