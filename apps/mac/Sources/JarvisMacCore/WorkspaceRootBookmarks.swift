import Foundation

public let jarvisWorkspaceRootStartupEnvelopeMaximumBytes = 64 * 1024
public let jarvisTrustedWakeStartupDocumentMaximumBytes = 8 * 1024
private let jarvisWorkspaceRootMaximumGrants = 8
private let jarvisWorkspaceRootBookmarkMaximumBytes = 128 * 1024
private let jarvisWorkspaceRootPathMaximumBytes = 4 * 1024

public struct JarvisWorkspaceRootGrant: Equatable, Identifiable, Sendable {
    public let id: String
    public let status: String

    public init(id: String, status: String = "configured") {
        self.id = id
        self.status = status
    }
}

public struct JarvisStoredWorkspaceRootBookmark: Codable, Equatable, Sendable {
    public let id: String
    public let bookmarkData: Data

    public init(id: String, bookmarkData: Data) {
        self.id = id
        self.bookmarkData = bookmarkData
    }
}

public protocol JarvisWorkspaceRootBookmarkStoring: Sendable {
    func load() throws -> [JarvisStoredWorkspaceRootBookmark]
    func save(_ records: [JarvisStoredWorkspaceRootBookmark]) throws
}

public struct ApplicationSupportWorkspaceRootBookmarkStore: JarvisWorkspaceRootBookmarkStoring, @unchecked Sendable {
    public let fileURL: URL
    private let fileManager: FileManager

    public init(
        fileURL: URL? = nil,
        fileManager: FileManager = .default
    ) {
        self.fileManager = fileManager
        if let fileURL {
            self.fileURL = fileURL
        } else {
            let support = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
                ?? fileManager.homeDirectoryForCurrentUser
                    .appending(path: "Library", directoryHint: .isDirectory)
                    .appending(path: "Application Support", directoryHint: .isDirectory)
            self.fileURL = support
                .appending(path: "Jarvis", directoryHint: .isDirectory)
                .appending(path: "workspace-root-bookmarks.json")
        }
    }

    public func load() throws -> [JarvisStoredWorkspaceRootBookmark] {
        guard fileManager.fileExists(atPath: fileURL.path) else { return [] }
        let data = try Data(contentsOf: fileURL, options: .mappedIfSafe)
        guard data.count <= 1024 * 1024 else {
            throw JarvisWorkspaceRootBookmarkError.storeUnavailable
        }
        return try JSONDecoder().decode([JarvisStoredWorkspaceRootBookmark].self, from: data)
    }

    public func save(_ records: [JarvisStoredWorkspaceRootBookmark]) throws {
        let directory = fileURL.deletingLastPathComponent()
        try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        try fileManager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
        let data = try JSONEncoder().encode(records)
        guard data.count <= 1024 * 1024 else {
            throw JarvisWorkspaceRootBookmarkError.storeUnavailable
        }
        try data.write(to: fileURL, options: .atomic)
        try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: fileURL.path)
    }
}

public struct JarvisResolvedWorkspaceRootBookmark: Sendable {
    public let url: URL
    public let isStale: Bool

    public init(url: URL, isStale: Bool) {
        self.url = url
        self.isStale = isStale
    }
}

public protocol JarvisSecurityScopedBookmarkAccessing: Sendable {
    func createBookmark(for url: URL) throws -> Data
    func resolveBookmark(_ data: Data) throws -> JarvisResolvedWorkspaceRootBookmark
    func isDirectory(_ url: URL) throws -> Bool
    func startAccessing(_ url: URL) -> Bool
    func stopAccessing(_ url: URL)
}

public struct FoundationSecurityScopedBookmarkAccessor: JarvisSecurityScopedBookmarkAccessing {
    public init() {}

    public func createBookmark(for url: URL) throws -> Data {
        try url.bookmarkData(options: [.withSecurityScope], includingResourceValuesForKeys: [.isDirectoryKey], relativeTo: nil)
    }

    public func resolveBookmark(_ data: Data) throws -> JarvisResolvedWorkspaceRootBookmark {
        var stale = false
        let url = try URL(
            resolvingBookmarkData: data,
            options: [.withSecurityScope, .withoutUI],
            relativeTo: nil,
            bookmarkDataIsStale: &stale
        )
        return JarvisResolvedWorkspaceRootBookmark(url: url, isStale: stale)
    }

    public func isDirectory(_ url: URL) throws -> Bool {
        try url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory == true
    }

    public func startAccessing(_ url: URL) -> Bool {
        url.startAccessingSecurityScopedResource()
    }

    public func stopAccessing(_ url: URL) {
        url.stopAccessingSecurityScopedResource()
    }
}

enum JarvisWorkspaceRootBookmarkError: Error, Equatable, CustomStringConvertible {
    case invalidSelection
    case duplicateSelection
    case grantLimitReached
    case invalidGrant(String)
    case accessDenied(String)
    case storeUnavailable

    var description: String {
        switch self {
        case .invalidSelection: "selected workspace root is not an accessible directory"
        case .duplicateSelection: "selected workspace root is already configured"
        case .grantLimitReached: "workspace root grant limit reached"
        case let .invalidGrant(id): "workspace root grant \(id) is unavailable"
        case let .accessDenied(id): "workspace root grant \(id) could not be activated"
        case .storeUnavailable: "workspace root grant storage is unavailable"
        }
    }
}

struct JarvisWorkspaceRootLaunchRoot: Sendable {
    let id: String
    let path: String
}

public final class JarvisWorkspaceRootAccessLease: @unchecked Sendable {
    let roots: [JarvisWorkspaceRootLaunchRoot]
    private let lock = NSLock()
    private var releaseHandler: (() -> Void)?

    init(roots: [JarvisWorkspaceRootLaunchRoot], releaseHandler: @escaping () -> Void) {
        self.roots = roots
        self.releaseHandler = releaseHandler
    }

    public func release() {
        lock.lock()
        let handler = releaseHandler
        releaseHandler = nil
        lock.unlock()
        handler?()
    }

    deinit { release() }
}

@MainActor
public protocol JarvisWorkspaceRootGrantProviding: AnyObject {
    func acquireForCoreLaunch() throws -> JarvisWorkspaceRootAccessLease
}

@MainActor
public final class JarvisWorkspaceRootBookmarkCoordinator: ObservableObject, JarvisWorkspaceRootGrantProviding {
    @Published public private(set) var grants: [JarvisWorkspaceRootGrant] = []
    @Published public private(set) var lastError: String?

    private let store: any JarvisWorkspaceRootBookmarkStoring
    private let accessor: any JarvisSecurityScopedBookmarkAccessing
    private let idGenerator: @Sendable () -> String

    public init(
        store: any JarvisWorkspaceRootBookmarkStoring = ApplicationSupportWorkspaceRootBookmarkStore(),
        accessor: any JarvisSecurityScopedBookmarkAccessing = FoundationSecurityScopedBookmarkAccessor(),
        idGenerator: @escaping @Sendable () -> String = {
            let hex = UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased()
            return "root_" + hex.prefix(24)
        }
    ) {
        self.store = store
        self.accessor = accessor
        self.idGenerator = idGenerator
        refresh()
    }

    public func refresh() {
        do {
            let records = try store.load()
            guard records.count <= jarvisWorkspaceRootMaximumGrants else {
                throw JarvisWorkspaceRootBookmarkError.storeUnavailable
            }
            var seenIDs = Set<String>()
            grants = try records.map { record in
                guard record.id.range(of: #"^[a-z0-9_-]{1,32}$"#, options: .regularExpression) != nil,
                      seenIDs.insert(record.id).inserted,
                      !record.bookmarkData.isEmpty,
                      record.bookmarkData.count <= jarvisWorkspaceRootBookmarkMaximumBytes else {
                    throw JarvisWorkspaceRootBookmarkError.storeUnavailable
                }
                return JarvisWorkspaceRootGrant(id: record.id)
            }
            lastError = nil
        } catch {
            grants = []
            lastError = JarvisWorkspaceRootBookmarkError.storeUnavailable.description
        }
    }

    @discardableResult
    public func addRoot(_ url: URL) throws -> JarvisWorkspaceRootGrant {
        do {
            guard try accessor.isDirectory(url) else { throw JarvisWorkspaceRootBookmarkError.invalidSelection }
            var records = try store.load()
            guard records.count < jarvisWorkspaceRootMaximumGrants else {
                throw JarvisWorkspaceRootBookmarkError.grantLimitReached
            }
            for record in records {
                let resolved = try accessor.resolveBookmark(record.bookmarkData)
                if resolved.url.standardizedFileURL == url.standardizedFileURL {
                    throw JarvisWorkspaceRootBookmarkError.duplicateSelection
                }
            }
            let id = idGenerator()
            guard id.range(of: #"^[a-z0-9_-]{1,32}$"#, options: .regularExpression) != nil,
                  !records.contains(where: { $0.id == id }) else {
                throw JarvisWorkspaceRootBookmarkError.invalidSelection
            }
            let bookmark = try accessor.createBookmark(for: url)
            guard !bookmark.isEmpty, bookmark.count <= jarvisWorkspaceRootBookmarkMaximumBytes else {
                throw JarvisWorkspaceRootBookmarkError.invalidSelection
            }
            records.append(JarvisStoredWorkspaceRootBookmark(id: id, bookmarkData: bookmark))
            try store.save(records)
            refresh()
            return JarvisWorkspaceRootGrant(id: id)
        } catch let error as JarvisWorkspaceRootBookmarkError {
            lastError = error.description
            throw error
        } catch {
            lastError = JarvisWorkspaceRootBookmarkError.storeUnavailable.description
            throw JarvisWorkspaceRootBookmarkError.storeUnavailable
        }
    }

    public func removeRoot(id: String) throws {
        do {
            var records = try store.load()
            records.removeAll { $0.id == id }
            try store.save(records)
            refresh()
        } catch {
            lastError = JarvisWorkspaceRootBookmarkError.storeUnavailable.description
            throw JarvisWorkspaceRootBookmarkError.storeUnavailable
        }
    }

    public func acquireForCoreLaunch() throws -> JarvisWorkspaceRootAccessLease {
        let original: [JarvisStoredWorkspaceRootBookmark]
        do {
            original = try store.load()
        } catch {
            throw JarvisWorkspaceRootBookmarkError.storeUnavailable
        }

        var updated = original
        var activeURLs: [URL] = []
        var roots: [JarvisWorkspaceRootLaunchRoot] = []
        do {
            guard original.count <= jarvisWorkspaceRootMaximumGrants else {
                throw JarvisWorkspaceRootBookmarkError.grantLimitReached
            }
            var seenIDs = Set<String>()
            var seenPaths = Set<String>()
            for (index, record) in original.enumerated() {
                guard record.id.range(of: #"^[a-z0-9_-]{1,32}$"#, options: .regularExpression) != nil,
                      seenIDs.insert(record.id).inserted,
                      !record.bookmarkData.isEmpty,
                      record.bookmarkData.count <= jarvisWorkspaceRootBookmarkMaximumBytes else {
                    throw JarvisWorkspaceRootBookmarkError.invalidGrant(record.id)
                }
                let resolved: JarvisResolvedWorkspaceRootBookmark
                do {
                    resolved = try accessor.resolveBookmark(record.bookmarkData)
                } catch {
                    throw JarvisWorkspaceRootBookmarkError.invalidGrant(record.id)
                }
                guard accessor.startAccessing(resolved.url) else {
                    throw JarvisWorkspaceRootBookmarkError.accessDenied(record.id)
                }
                activeURLs.append(resolved.url)
                do {
                    guard try accessor.isDirectory(resolved.url) else {
                        throw JarvisWorkspaceRootBookmarkError.invalidGrant(record.id)
                    }
                } catch {
                    throw JarvisWorkspaceRootBookmarkError.invalidGrant(record.id)
                }
                let path = resolved.url.standardizedFileURL.path
                guard path.utf8.count <= jarvisWorkspaceRootPathMaximumBytes,
                      seenPaths.insert(path).inserted else {
                    throw JarvisWorkspaceRootBookmarkError.invalidGrant(record.id)
                }
                if resolved.isStale {
                    do {
                        let refreshed = try accessor.createBookmark(for: resolved.url)
                        guard !refreshed.isEmpty,
                              refreshed.count <= jarvisWorkspaceRootBookmarkMaximumBytes else {
                            throw JarvisWorkspaceRootBookmarkError.invalidGrant(record.id)
                        }
                        updated[index] = JarvisStoredWorkspaceRootBookmark(
                            id: record.id,
                            bookmarkData: refreshed
                        )
                    } catch {
                        throw JarvisWorkspaceRootBookmarkError.invalidGrant(record.id)
                    }
                }
                roots.append(JarvisWorkspaceRootLaunchRoot(id: record.id, path: path))
            }
            if updated != original { try store.save(updated) }
        } catch {
            for url in activeURLs.reversed() { accessor.stopAccessing(url) }
            if let error = error as? JarvisWorkspaceRootBookmarkError { throw error }
            throw JarvisWorkspaceRootBookmarkError.storeUnavailable
        }

        let accessor = self.accessor
        return JarvisWorkspaceRootAccessLease(roots: roots) {
            for url in activeURLs.reversed() { accessor.stopAccessing(url) }
        }
    }
}
