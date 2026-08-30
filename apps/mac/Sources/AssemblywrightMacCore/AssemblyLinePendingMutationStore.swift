import CryptoKit
import Darwin
import Foundation

public enum AssemblywrightMacAssemblyLinePendingStoreError: Error, Equatable, Sendable {
  case unsafeStore
}

struct AssemblywrightMacPendingAssemblyLinePlanningMutation: Codable, Equatable, Sendable {
  let schemaVersion: UInt16
  let action: AssemblywrightMacAssemblyLinePlanningAction
  let requestData: Data
  let requestSHA256: [UInt8]

  init(action: AssemblywrightMacAssemblyLinePlanningAction, requestData: Data) throws {
    try AssemblywrightMacAssemblyLineOwnerControl.validateStoredRequest(
      action: action,
      requestData: requestData
    )
    schemaVersion = 1
    self.action = action
    self.requestData = requestData
    requestSHA256 = Array(SHA256.hash(data: requestData))
  }

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case action
    case requestData = "request_data"
    case requestSHA256 = "request_sha256"
  }

  func validate() throws {
    guard schemaVersion == 1, requestSHA256.count == 32,
      requestSHA256 == Array(SHA256.hash(data: requestData))
    else { throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore }
    do {
      try AssemblywrightMacAssemblyLineOwnerControl.validateStoredRequest(
        action: action,
        requestData: requestData
      )
    } catch {
      throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore
    }
  }
}

public struct AssemblywrightMacAssemblyLinePendingMutationStore: Sendable {
  public static let maximumDocumentBytes = 160 * 1_024
  public let fileURL: URL

  public init(fileURL: URL = Self.defaultURL()) {
    self.fileURL = fileURL.standardizedFileURL
  }

  public static func defaultURL() -> URL {
    FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent("Library/Application Support/Assemblywright", isDirectory: true)
      .appendingPathComponent("assembly-line-pending-mutation-v1.json")
  }

  func load() throws -> AssemblywrightMacPendingAssemblyLinePlanningMutation? {
    var pathMetadata = stat()
    guard lstat(fileURL.path, &pathMetadata) == 0 else {
      if errno == ENOENT { return nil }
      throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore
    }
    let descriptor = open(fileURL.path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
    guard descriptor >= 0 else {
      throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore
    }
    let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
    var metadata = stat()
    guard fstat(descriptor, &metadata) == 0,
      metadata.st_mode & S_IFMT == S_IFREG,
      metadata.st_uid == getuid(), metadata.st_mode & 0o077 == 0,
      metadata.st_size >= 0, metadata.st_size <= Self.maximumDocumentBytes
    else { throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore }
    let data: Data
    do {
      data = try handle.read(upToCount: Self.maximumDocumentBytes + 1) ?? Data()
    } catch {
      throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore
    }
    guard data.count == Int(metadata.st_size) else {
      throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore
    }
    var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
    guard data.count <= Self.maximumDocumentBytes,
      (try? scanner.validateNoDuplicateObjectKeysRecursively()) != nil,
      let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
      Set(object.keys) == ["schema_version", "action", "request_data", "request_sha256"],
      let mutation = try? JSONDecoder().decode(
        AssemblywrightMacPendingAssemblyLinePlanningMutation.self,
        from: data
      )
    else { throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore }
    try mutation.validate()
    return mutation
  }

  func save(_ mutation: AssemblywrightMacPendingAssemblyLinePlanningMutation) throws {
    try mutation.validate()
    let data = try sortedEncoder().encode(mutation)
    guard data.count <= Self.maximumDocumentBytes else {
      throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore
    }
    let (directory, descriptor) = try openValidatedDirectory(create: true)
    defer { _ = close(descriptor) }
    let temporary = directory.appendingPathComponent(
      ".assembly-line-pending-\(UUID().uuidString.lowercased()).tmp"
    )
    let fileDescriptor = open(temporary.path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0o600)
    guard fileDescriptor >= 0 else {
      throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore
    }
    var published = false
    defer {
      _ = close(fileDescriptor)
      if !published { _ = unlink(temporary.path) }
    }
    let written = data.withUnsafeBytes { bytes in
      Darwin.write(fileDescriptor, bytes.baseAddress, bytes.count)
    }
    guard written == data.count, fsync(fileDescriptor) == 0,
      rename(temporary.path, fileURL.path) == 0, fsync(descriptor) == 0
    else { throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore }
    published = true
  }

  func clear() throws {
    var metadata = stat()
    guard lstat(fileURL.path, &metadata) == 0 else {
      if errno == ENOENT { return }
      throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore
    }
    _ = try validatedRegularFile()
    let (_, descriptor) = try openValidatedDirectory(create: false)
    defer { _ = close(descriptor) }
    guard unlink(fileURL.path) == 0, fsync(descriptor) == 0 else {
      throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore
    }
  }

  private func validatedRegularFile() throws -> stat {
    var metadata = stat()
    guard lstat(fileURL.path, &metadata) == 0,
      metadata.st_mode & S_IFMT == S_IFREG,
      metadata.st_uid == getuid(), metadata.st_mode & 0o077 == 0
    else { throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore }
    return metadata
  }

  private func openValidatedDirectory(create: Bool) throws -> (URL, Int32) {
    let directory = fileURL.deletingLastPathComponent()
    if create {
      try FileManager.default.createDirectory(
        at: directory,
        withIntermediateDirectories: true,
        attributes: [.posixPermissions: 0o700]
      )
      try FileManager.default.setAttributes(
        [.posixPermissions: 0o700],
        ofItemAtPath: directory.path
      )
    }
    var metadata = stat()
    guard lstat(directory.path, &metadata) == 0,
      metadata.st_mode & S_IFMT == S_IFDIR,
      metadata.st_uid == getuid(), metadata.st_mode & 0o077 == 0
    else { throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore }
    let descriptor = open(directory.path, O_RDONLY | O_DIRECTORY | O_CLOEXEC)
    guard descriptor >= 0 else {
      throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore
    }
    var opened = stat()
    guard fstat(descriptor, &opened) == 0,
      opened.st_dev == metadata.st_dev, opened.st_ino == metadata.st_ino
    else {
      _ = close(descriptor)
      throw AssemblywrightMacAssemblyLinePendingStoreError.unsafeStore
    }
    return (directory, descriptor)
  }

  private func sortedEncoder() -> JSONEncoder {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    return encoder
  }
}
