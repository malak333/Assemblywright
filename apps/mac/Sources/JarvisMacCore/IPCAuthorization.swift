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

public final class JarvisIPCSessionAuthorization: @unchecked Sendable {
    public let mode: JarvisIPCAuthorizationMode
    public let tokenFileURL: URL

    private let lock = NSLock()
    private let randomBytes: @Sendable (Int) throws -> Data
    private var active: JarvisIPCLaunchAuthorization?
    private var nextGeneration: UInt64 = 0

    public init(
        mode: JarvisIPCAuthorizationMode = .explicitUnauthenticated,
        tokenFileURL: URL? = nil,
        fileManager: FileManager = .default,
        randomBytes: (@Sendable (Int) throws -> Data)? = nil
    ) {
        self.mode = mode
        self.randomBytes = randomBytes ?? Self.secureRandomBytes
        if let tokenFileURL {
            self.tokenFileURL = tokenFileURL
        } else {
            let support = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
                ?? fileManager.homeDirectoryForCurrentUser
                    .appending(path: "Library", directoryHint: .isDirectory)
                    .appending(path: "Application Support", directoryHint: .isDirectory)
            self.tokenFileURL = support
                .appending(path: "Jarvis", directoryHint: .isDirectory)
                .appending(path: "ipc-session-auth.json")
        }
        if mode == .appSupervised {
            try? fileManager.removeItem(at: self.tokenFileURL)
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
            let file = JarvisIPCAuthFile(
                version: 1,
                scheme: "bearer",
                token: token,
                generation: candidate.generation
            )
            try Self.atomicWrite(JSONEncoder().encode(file), to: tokenFileURL)
            active = candidate
            lock.unlock()
            return candidate
        } catch {
            lock.unlock()
            throw JarvisIPCAuthorizationError.tokenFileUnavailable
        }
    }

    public func clear(generation: UInt64) {
        guard mode == .appSupervised else { return }
        lock.lock()
        guard active?.generation == generation else {
            lock.unlock()
            return
        }
        active = nil
        try? FileManager.default.removeItem(at: tokenFileURL)
        lock.unlock()
    }

    public func clearActive() {
        guard mode == .appSupervised else { return }
        lock.lock()
        active = nil
        try? FileManager.default.removeItem(at: tokenFileURL)
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
            if shouldRemoveTemporary { try? FileManager.default.removeItem(at: temporary) }
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
