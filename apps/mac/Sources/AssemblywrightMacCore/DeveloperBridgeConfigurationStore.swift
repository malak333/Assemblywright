import Darwin
import Foundation

public enum AssemblywrightDeveloperBridgeConfigurationStoreError: Error, Equatable, Sendable {
    case unsafeStore
    case invalidConfiguration
}

public struct AssemblywrightDeveloperBridgeStoredConfiguration:
    Codable, Equatable, Sendable
{
    public static let schemaVersion: UInt16 = 1

    public let schemaVersion: UInt16
    public let helperPath: String
    public let teamIdentifier: String

    public init(helperPath: String, teamIdentifier: String) throws {
        self.schemaVersion = Self.schemaVersion
        self.helperPath = helperPath
        self.teamIdentifier = teamIdentifier
        try validate()
    }

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case helperPath = "helper_path"
        case teamIdentifier = "team_identifier"
    }

    public func validate() throws {
        guard schemaVersion == Self.schemaVersion,
              AssemblywrightDeveloperBridgeProcessConfiguration
                .isValidTeamIdentifier(teamIdentifier),
              helperPath.utf8.count <= 4 * 1_024,
              helperPath.hasPrefix("/"),
              !helperPath.contains("\0"),
              !helperPath.split(separator: "/").contains("..") else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.invalidConfiguration
        }

        let helperURL = URL(fileURLWithPath: helperPath)
        guard helperURL.standardizedFileURL.path == helperPath,
              helperURL.resolvingSymlinksInPath().standardizedFileURL.path == helperPath else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.invalidConfiguration
        }
        var metadata = stat()
        guard lstat(helperPath, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFREG,
              (metadata.st_uid == getuid() || metadata.st_uid == 0),
              metadata.st_mode & 0o022 == 0,
              access(helperPath, X_OK) == 0 else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.invalidConfiguration
        }
    }

    public var processConfiguration: AssemblywrightDeveloperBridgeProcessConfiguration {
        AssemblywrightDeveloperBridgeProcessConfiguration(
            executableURL: URL(fileURLWithPath: helperPath),
            expectedTeamIdentifier: teamIdentifier
        )
    }
}

public struct AssemblywrightDeveloperBridgeConfigurationStore: Sendable {
    public static let maximumDocumentBytes = 8 * 1_024

    public let fileURL: URL

    public init(fileURL: URL = Self.defaultURL()) {
        self.fileURL = fileURL.standardizedFileURL
    }

    public static func defaultURL() -> URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(
                "Library/Application Support/Assemblywright",
                isDirectory: true
            )
            .appendingPathComponent("developer-bridge-configuration-v1.json")
    }

    public func load() throws -> AssemblywrightDeveloperBridgeStoredConfiguration? {
        let filename = try validatedFilename()
        let directory = fileURL.deletingLastPathComponent()
        var directoryMetadata = stat()
        guard lstat(directory.path, &directoryMetadata) == 0 else {
            if errno == ENOENT { return nil }
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        let (_, directoryDescriptor) = try openValidatedDirectory(create: false)
        defer { _ = close(directoryDescriptor) }
        var pathMetadata = stat()
        guard fstatat(
            directoryDescriptor,
            filename,
            &pathMetadata,
            AT_SYMLINK_NOFOLLOW
        ) == 0 else {
            if errno == ENOENT { return nil }
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        let descriptor = openat(
            directoryDescriptor,
            filename,
            O_RDONLY | O_NOFOLLOW | O_CLOEXEC
        )
        guard descriptor >= 0 else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        var metadata = stat()
        guard fstat(descriptor, &metadata) == 0,
              metadata.st_dev == pathMetadata.st_dev,
              metadata.st_ino == pathMetadata.st_ino,
              metadata.st_mode & S_IFMT == S_IFREG,
              metadata.st_uid == getuid(),
              metadata.st_mode & 0o077 == 0,
              metadata.st_nlink == 1,
              metadata.st_size >= 0,
              metadata.st_size <= Self.maximumDocumentBytes else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        let data: Data
        do {
            data = try handle.read(upToCount: Self.maximumDocumentBytes + 1) ?? Data()
        } catch {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
        guard data.count == Int(metadata.st_size),
              data.count <= Self.maximumDocumentBytes,
              (try? scanner.validateNoDuplicateObjectKeysRecursively()) != nil,
              let object = try? JSONSerialization.jsonObject(with: data),
              let dictionary = object as? [String: Any],
              Set(dictionary.keys) == ["schema_version", "helper_path", "team_identifier"],
              let stored = try? JSONDecoder().decode(
                AssemblywrightDeveloperBridgeStoredConfiguration.self,
                from: data
              ) else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        do {
            try stored.validate()
        } catch {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        return stored
    }

    public func save(_ stored: AssemblywrightDeveloperBridgeStoredConfiguration) throws {
        try stored.validate()
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(stored)
        guard data.count <= Self.maximumDocumentBytes else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }

        let (_, directoryDescriptor) = try openValidatedDirectory(create: true)
        defer { _ = close(directoryDescriptor) }
        let filename = try validatedFilename()
        let temporaryName =
            ".developer-bridge-configuration-\(UUID().uuidString.lowercased()).tmp"
        let descriptor = openat(
            directoryDescriptor,
            temporaryName,
            O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
            0o600
        )
        guard descriptor >= 0 else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        var published = false
        defer {
            _ = close(descriptor)
            if !published { _ = unlinkat(directoryDescriptor, temporaryName, 0) }
        }
        guard writeAll(data, to: descriptor),
              fsync(descriptor) == 0,
              renameat(directoryDescriptor, temporaryName, directoryDescriptor, filename) == 0,
              fsync(directoryDescriptor) == 0 else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        published = true
    }

    private func openValidatedDirectory(create: Bool) throws -> (URL, Int32) {
        let directory = fileURL.deletingLastPathComponent()
        if create {
            var existing = stat()
            if lstat(directory.path, &existing) != 0 {
                guard errno == ENOENT else {
                    throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
                }
                try FileManager.default.createDirectory(
                    at: directory,
                    withIntermediateDirectories: true,
                    attributes: [.posixPermissions: 0o700]
                )
            }
        }
        var metadata = stat()
        guard lstat(directory.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFDIR,
              metadata.st_uid == getuid(),
              metadata.st_mode & 0o077 == 0 else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        let descriptor = open(directory.path, O_RDONLY | O_DIRECTORY | O_CLOEXEC)
        guard descriptor >= 0 else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        var opened = stat()
        guard fstat(descriptor, &opened) == 0,
              opened.st_dev == metadata.st_dev,
              opened.st_ino == metadata.st_ino else {
            _ = close(descriptor)
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        return (directory, descriptor)
    }

    private func validatedFilename() throws -> String {
        let filename = fileURL.lastPathComponent
        let directory = fileURL.deletingLastPathComponent()
        guard !filename.isEmpty, filename != ".", filename != "..",
              !filename.contains("/"), !filename.contains("\0"),
              directory.appendingPathComponent(filename).standardizedFileURL == fileURL else {
            throw AssemblywrightDeveloperBridgeConfigurationStoreError.unsafeStore
        }
        return filename
    }

    private func writeAll(_ data: Data, to descriptor: Int32) -> Bool {
        data.withUnsafeBytes { bytes in
            guard let baseAddress = bytes.baseAddress else { return data.isEmpty }
            var offset = 0
            while offset < bytes.count {
                let written = Darwin.write(
                    descriptor,
                    baseAddress.advanced(by: offset),
                    bytes.count - offset
                )
                if written > 0 {
                    offset += written
                } else if written < 0, errno == EINTR {
                    continue
                } else {
                    return false
                }
            }
            return true
        }
    }
}
