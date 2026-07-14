import Foundation
import Darwin
import Security

public enum JarvisIPCAuthorizationMode: Equatable, Sendable {
    case explicitUnauthenticated
    case appSupervised
}

public enum JarvisIPCAuthorizationError: Error, Equatable {
    case credentialUnavailable
    case nonLoopbackEndpoint
    case secureRandomUnavailable
    case tokenFileUnavailable
    case unixSocketUnavailable
    case unixSocketPathInvalid
    case unixSocketParentUnsafe
}

public enum JarvisIPCTransportMode: Equatable, Sendable {
    case loopbackTCP
    case unixSocket
}

public struct JarvisIPCUnixSocketDescriptor: Equatable, Sendable {
    public static let kind = "unix_socket_v1"

    public let socketURL: URL

    public init(socketURL: URL) {
        self.socketURL = socketURL
    }
}

public struct JarvisIPCCLIHandoffConfiguration: Equatable, Sendable {
    public static let enableEnvironmentKey = "JARVIS_MAC_ENABLE_IPC_CLI_HANDOFF"
    public static let fileEnvironmentKey = "JARVIS_MAC_IPC_AUTH_FILE"
    public static let disabled = Self(isEnabled: false, fileURL: nil, staleFileURL: nil)

    public let isEnabled: Bool
    public let fileURL: URL?
    fileprivate let staleFileURL: URL?

    private init(isEnabled: Bool, fileURL: URL?, staleFileURL: URL?) {
        self.isEnabled = isEnabled
        self.fileURL = fileURL
        self.staleFileURL = staleFileURL
    }

    public static func enabled(fileURL: URL? = nil) -> Self {
        guard let fileURL else {
            return Self(isEnabled: true, fileURL: nil, staleFileURL: nil)
        }
        guard fileURL.isFileURL, fileURL.path.hasPrefix("/") else {
            return .disabled
        }
        return Self(isEnabled: true, fileURL: fileURL, staleFileURL: fileURL)
    }

    public static func fromEnvironment(
        _ environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> Self {
        let configuredPath = environment[fileEnvironmentKey]
        let absoluteFileURL = configuredPath.flatMap { path in
            path.hasPrefix("/") ? URL(fileURLWithPath: path) : nil
        }
        guard environment[enableEnvironmentKey] == "true" else {
            return Self(isEnabled: false, fileURL: nil, staleFileURL: absoluteFileURL)
        }
        guard configuredPath == nil || absoluteFileURL != nil else {
            return .disabled
        }
        return Self(isEnabled: true, fileURL: absoluteFileURL, staleFileURL: absoluteFileURL)
    }
}

public struct JarvisIPCLaunchAuthorization: Equatable, Sendable {
    public let token: String
    public let generation: UInt64

    public var headerValue: String { "Bearer \(token)" }
}

private struct JarvisIPCAuthFile: Codable {
    let version: Int
    let scheme: String
    let token: String
    let generation: UInt64
}

private struct JarvisIPCActiveUnixSocket {
    let generation: UInt64
    let descriptor: JarvisIPCUnixSocketDescriptor
    var device: dev_t?
    var inode: ino_t?
}

public final class JarvisIPCSessionAuthorization: @unchecked Sendable {
    public static let unixSocketDirectoryEnvironmentKey = "JARVIS_MAC_IPC_SOCKET_DIRECTORY"
    public let mode: JarvisIPCAuthorizationMode
    public let tokenFileURL: URL
    public let cliHandoffConfiguration: JarvisIPCCLIHandoffConfiguration
    public let transportMode: JarvisIPCTransportMode

    private let lock = NSLock()
    private let staleTokenFileURLs: [URL]
    private let randomBytes: @Sendable (Int) throws -> Data
    private let socketDirectoryURL: URL
    private let socketDirectoryIsValid: Bool
    private let socketIdentifier: @Sendable () throws -> String
    private var active: JarvisIPCLaunchAuthorization?
    private var activeSocket: JarvisIPCActiveUnixSocket?
    private var nextGeneration: UInt64 = 0

    public init(
        mode: JarvisIPCAuthorizationMode = .explicitUnauthenticated,
        tokenFileURL: URL? = nil,
        cliHandoffConfiguration: JarvisIPCCLIHandoffConfiguration = .disabled,
        transportMode: JarvisIPCTransportMode = .loopbackTCP,
        socketDirectoryPath: String? = nil,
        fileManager: FileManager = .default,
        randomBytes: (@Sendable (Int) throws -> Data)? = nil,
        socketIdentifier: (@Sendable () throws -> String)? = nil
    ) {
        self.mode = mode
        self.cliHandoffConfiguration = cliHandoffConfiguration
        self.transportMode = transportMode
        self.randomBytes = randomBytes ?? Self.secureRandomBytes
        self.socketIdentifier = socketIdentifier ?? {
            try Self.secureRandomBytes(count: 8)
                .map { String(format: "%02x", $0) }
                .joined()
        }
        let defaultTokenFileURL = Self.defaultTokenFileURL(fileManager: fileManager)
        let configuredSocketDirectory = socketDirectoryPath.map { URL(fileURLWithPath: $0) }
        self.socketDirectoryIsValid = socketDirectoryPath == nil
            || (socketDirectoryPath?.hasPrefix("/") == true
                && configuredSocketDirectory?.standardizedFileURL.path == socketDirectoryPath)
        self.socketDirectoryURL = configuredSocketDirectory
            ?? defaultTokenFileURL.deletingLastPathComponent()
                .appending(path: "run", directoryHint: .isDirectory)
        let legacyTokenFileURL = tokenFileURL.flatMap(Self.absoluteFileURL)
        self.tokenFileURL = cliHandoffConfiguration.isEnabled
            ? cliHandoffConfiguration.fileURL ?? legacyTokenFileURL ?? defaultTokenFileURL
            : defaultTokenFileURL
        self.staleTokenFileURLs = Array(Set([
            defaultTokenFileURL,
            legacyTokenFileURL,
            cliHandoffConfiguration.staleFileURL,
            self.tokenFileURL
        ].compactMap { $0 }))
        if mode == .appSupervised {
            removeStaleTokenFiles()
        }
    }

    public func authorizationHeader() throws -> String? {
        guard mode == .appSupervised else { return nil }
        lock.lock()
        defer { lock.unlock() }
        guard let active else { throw JarvisIPCAuthorizationError.credentialUnavailable }
        return active.headerValue
    }

    public func rotateForLaunch() throws -> JarvisIPCLaunchAuthorization? {
        guard mode == .appSupervised else { return nil }
        let bytes: Data
        do {
            bytes = try randomBytes(32)
        } catch {
            throw JarvisIPCAuthorizationError.secureRandomUnavailable
        }
        guard bytes.count == 32 else { throw JarvisIPCAuthorizationError.secureRandomUnavailable }
        let token = bytes.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
        guard token.count == 43 else { throw JarvisIPCAuthorizationError.secureRandomUnavailable }

        lock.lock()
        guard nextGeneration < UInt64.max else {
            lock.unlock()
            throw JarvisIPCAuthorizationError.credentialUnavailable
        }
        nextGeneration += 1
        let candidate = JarvisIPCLaunchAuthorization(token: token, generation: nextGeneration)
        do {
            if cliHandoffConfiguration.isEnabled {
                let file = JarvisIPCAuthFile(
                    version: 1,
                    scheme: "bearer",
                    token: token,
                    generation: candidate.generation
                )
                try Self.atomicWrite(JSONEncoder().encode(file), to: tokenFileURL)
            }
            active = candidate
            lock.unlock()
            return candidate
        } catch {
            active = nil
            removeStaleTokenFiles()
            lock.unlock()
            throw JarvisIPCAuthorizationError.tokenFileUnavailable
        }
    }

    public func prepareUnixSocketForLaunch(
        generation: UInt64
    ) throws -> JarvisIPCUnixSocketDescriptor? {
        guard mode == .appSupervised, transportMode == .unixSocket else { return nil }
        lock.lock()
        defer { lock.unlock() }
        guard active?.generation == generation else {
            throw JarvisIPCAuthorizationError.credentialUnavailable
        }
        guard socketDirectoryIsValid else {
            throw JarvisIPCAuthorizationError.unixSocketPathInvalid
        }
        if let activeSocket, activeSocket.generation == generation {
            return activeSocket.descriptor
        }
        cleanupActiveSocketLocked()
        try Self.prepareSocketDirectory(socketDirectoryURL)
        for _ in 0..<8 {
            let identifier = try socketIdentifier()
            guard !identifier.isEmpty,
                  identifier.utf8.count <= 32,
                  identifier.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-") }) else {
                throw JarvisIPCAuthorizationError.unixSocketPathInvalid
            }
            let socketURL = socketDirectoryURL.appending(path: "core-\(identifier).sock")
            guard Self.isRepresentableUnixSocketPath(socketURL) else {
                throw JarvisIPCAuthorizationError.unixSocketPathInvalid
            }
            guard Self.leafDoesNotExist(socketURL) else { continue }
            let descriptor = JarvisIPCUnixSocketDescriptor(socketURL: socketURL)
            activeSocket = JarvisIPCActiveUnixSocket(
                generation: generation,
                descriptor: descriptor,
                device: nil,
                inode: nil
            )
            return descriptor
        }
        throw JarvisIPCAuthorizationError.unixSocketPathInvalid
    }

    public func captureActiveUnixSocketIdentity(generation: UInt64) throws {
        guard mode == .appSupervised, transportMode == .unixSocket else { return }
        lock.lock()
        defer { lock.unlock() }
        guard var activeSocket,
              activeSocket.generation == generation else {
            throw JarvisIPCAuthorizationError.unixSocketUnavailable
        }
        var metadata = stat()
        let inspected = activeSocket.descriptor.socketURL.withUnsafeFileSystemRepresentation { path -> Bool in
            guard let path else { return false }
            return Darwin.lstat(path, &metadata) == 0
        }
        guard inspected,
              metadata.st_mode & S_IFMT == S_IFSOCK,
              metadata.st_uid == Darwin.geteuid(),
              metadata.st_mode & mode_t(0o7777) == mode_t(0o600) else {
            throw JarvisIPCAuthorizationError.unixSocketUnavailable
        }
        activeSocket.device = metadata.st_dev
        activeSocket.inode = metadata.st_ino
        self.activeSocket = activeSocket
    }

    public func activeUnixSocketURL() throws -> URL? {
        guard mode == .appSupervised, transportMode == .unixSocket else { return nil }
        lock.lock()
        defer { lock.unlock() }
        guard let active, let activeSocket, activeSocket.generation == active.generation else {
            throw JarvisIPCAuthorizationError.unixSocketUnavailable
        }
        return activeSocket.descriptor.socketURL
    }

    public func clear(generation: UInt64) {
        guard mode == .appSupervised else { return }
        lock.lock()
        guard active?.generation == generation else {
            lock.unlock()
            return
        }
        active = nil
        cleanupActiveSocketLocked()
        removeStaleTokenFiles()
        lock.unlock()
    }

    public func clearActive() {
        guard mode == .appSupervised else { return }
        lock.lock()
        active = nil
        cleanupActiveSocketLocked()
        removeStaleTokenFiles()
        lock.unlock()
    }

    public var activeGeneration: UInt64? {
        lock.lock()
        defer { lock.unlock() }
        return active?.generation
    }

    public static func isStrictLoopbackEndpoint(_ url: URL) -> Bool {
        guard url.scheme?.lowercased() == "http",
              url.user == nil,
              url.password == nil,
              let host = url.host?.lowercased() else {
            return false
        }
        if host == "::1" { return true }
        let octets = host.split(separator: ".", omittingEmptySubsequences: false)
        return octets.count == 4
            && octets.first == "127"
            && octets.allSatisfy { UInt8($0) != nil }
    }

    private static func secureRandomBytes(count: Int) throws -> Data {
        var data = Data(count: count)
        let status = data.withUnsafeMutableBytes { buffer in
            SecRandomCopyBytes(kSecRandomDefault, count, buffer.baseAddress!)
        }
        guard status == errSecSuccess else {
            throw JarvisIPCAuthorizationError.secureRandomUnavailable
        }
        return data
    }

    private static func absoluteFileURL(_ url: URL) -> URL? {
        guard url.isFileURL, url.path.hasPrefix("/") else { return nil }
        return url
    }

    private static func defaultTokenFileURL(fileManager: FileManager) -> URL {
        let support = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? fileManager.homeDirectoryForCurrentUser
                .appending(path: "Library", directoryHint: .isDirectory)
                .appending(path: "Application Support", directoryHint: .isDirectory)
        return support
            .appending(path: "Jarvis", directoryHint: .isDirectory)
            .appending(path: "ipc-session-auth.json")
    }

    private static func prepareSocketDirectory(_ url: URL) throws {
        guard url.isFileURL, url.path.hasPrefix("/") else {
            throw JarvisIPCAuthorizationError.unixSocketPathInvalid
        }
        let jarvisDirectory = url.deletingLastPathComponent()
        try createOrValidateOwnedDirectory(jarvisDirectory, requireOwnerOnlyMode: false)
        try createOrValidateOwnedDirectory(url, requireOwnerOnlyMode: true)
    }

    private static func createOrValidateOwnedDirectory(
        _ url: URL,
        requireOwnerOnlyMode: Bool
    ) throws {
        let result = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return EINVAL }
            if Darwin.mkdir(path, mode_t(0o700)) == 0 { return 0 }
            return errno
        }
        guard result == 0 || result == EEXIST else {
            throw JarvisIPCAuthorizationError.unixSocketParentUnsafe
        }
        var metadata = stat()
        let inspected = url.withUnsafeFileSystemRepresentation { path -> Bool in
            guard let path else { return false }
            return Darwin.lstat(path, &metadata) == 0
        }
        guard inspected,
              metadata.st_mode & S_IFMT == S_IFDIR,
              metadata.st_uid == Darwin.geteuid() else {
            throw JarvisIPCAuthorizationError.unixSocketParentUnsafe
        }
        guard !requireOwnerOnlyMode
                || metadata.st_mode & mode_t(0o7777) == mode_t(0o700) else {
            throw JarvisIPCAuthorizationError.unixSocketParentUnsafe
        }
    }

    private static func isRepresentableUnixSocketPath(_ url: URL) -> Bool {
        guard url.isFileURL, url.path.hasPrefix("/") else { return false }
        // Darwin sockaddr_un.sun_path is 104 bytes including the trailing NUL.
        return url.path.utf8.count < 104
    }

    private static func leafDoesNotExist(_ url: URL) -> Bool {
        var metadata = stat()
        let result = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return EINVAL }
            if Darwin.lstat(path, &metadata) == 0 { return 0 }
            return errno
        }
        return result == ENOENT
    }

    private func cleanupActiveSocketLocked() {
        guard let activeSocket,
              let device = activeSocket.device,
              let inode = activeSocket.inode else {
            self.activeSocket = nil
            return
        }
        Self.unlinkOwnedSocketLeafIfSafe(
            at: activeSocket.descriptor.socketURL,
            device: device,
            inode: inode
        )
        self.activeSocket = nil
    }

    private static func unlinkOwnedSocketLeafIfSafe(at url: URL, device: dev_t, inode: ino_t) {
        guard url.isFileURL, url.path.hasPrefix("/") else { return }
        url.withUnsafeFileSystemRepresentation { path in
            guard let path else { return }
            var metadata = stat()
            guard Darwin.lstat(path, &metadata) == 0,
                  metadata.st_uid == Darwin.geteuid(),
                  metadata.st_mode & S_IFMT == S_IFSOCK,
                  metadata.st_dev == device,
                  metadata.st_ino == inode else { return }
            _ = Darwin.unlink(path)
        }
    }

    private func removeStaleTokenFiles() {
        for url in staleTokenFileURLs {
            Self.removeLeafIfSafe(at: url)
        }
    }

    private static func removeLeafIfSafe(at url: URL) {
        guard url.isFileURL, url.path.hasPrefix("/") else { return }
        url.withUnsafeFileSystemRepresentation { path in
            guard let path else { return }
            var metadata = stat()
            guard Darwin.lstat(path, &metadata) == 0 else { return }
            let fileType = metadata.st_mode & S_IFMT
            let isSingleLinkRegularFile = fileType == S_IFREG && metadata.st_nlink == 1
            let isSymbolicLink = fileType == S_IFLNK
            guard isSingleLinkRegularFile || isSymbolicLink else { return }

            // unlink is deliberately the final filesystem operation. It removes
            // a symlink leaf without following it, and fails if a raced target
            // has become a directory rather than recursively deleting anything.
            _ = Darwin.unlink(path)
        }
    }

    private static func atomicWrite(_ data: Data, to url: URL) throws {
        let directory = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        guard chmod(directory.path, mode_t(0o700)) == 0 else {
            throw JarvisIPCAuthorizationError.tokenFileUnavailable
        }
        let temporary = directory.appending(path: ".\(url.lastPathComponent).\(UUID().uuidString).tmp")
        let descriptor = open(temporary.path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, mode_t(0o600))
        guard descriptor >= 0 else { throw JarvisIPCAuthorizationError.tokenFileUnavailable }
        var shouldRemoveTemporary = true
        defer {
            _ = close(descriptor)
            if shouldRemoveTemporary { Self.removeLeafIfSafe(at: temporary) }
        }
        try data.withUnsafeBytes { bytes in
            guard let base = bytes.baseAddress else { return }
            var written = 0
            while written < bytes.count {
                let result = Darwin.write(descriptor, base.advanced(by: written), bytes.count - written)
                guard result > 0 else { throw JarvisIPCAuthorizationError.tokenFileUnavailable }
                written += result
            }
        }
        guard fsync(descriptor) == 0,
              rename(temporary.path, url.path) == 0,
              chmod(url.path, mode_t(0o600)) == 0 else {
            throw JarvisIPCAuthorizationError.tokenFileUnavailable
        }
        shouldRemoveTemporary = false
    }
}
