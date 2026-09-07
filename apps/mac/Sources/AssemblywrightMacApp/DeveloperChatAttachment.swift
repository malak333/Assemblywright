import AppKit
import Foundation
import ImageIO
import UniformTypeIdentifiers

struct DeveloperChatAttachment: Codable, Equatable {
  let name: String
  let mediaType: String
  let dataBase64: String

  var wireValue: [String: String] {
    ["name": name, "media_type": mediaType, "data_base64": dataBase64]
  }

  var data: Data? { Data(base64Encoded: dataBase64) }
  var isImage: Bool { mediaType.hasPrefix("image/") }

  static let maximumCount = 4
  static let maximumTotalBytes = 6 * 1_024 * 1_024
  static let maximumImageBytes = 2 * 1_024 * 1_024
  static let maximumTextBytes = 128 * 1_024

  static func validateSelection(_ attachments: [Self]) throws {
    guard attachments.count <= maximumCount else { throw AttachmentError("Attach up to four files per message.") }
    guard attachments.reduce(0, { $0 + ($1.data?.count ?? maximumTotalBytes + 1) }) <= maximumTotalBytes else {
      throw AttachmentError("Attachments must total 6 MB or less.")
    }
  }

  static func read(_ url: URL) throws -> Self {
    let access = url.startAccessingSecurityScopedResource()
    defer { if access { url.stopAccessingSecurityScopedResource() } }
    let values = try url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
    guard values.isRegularFile == true, let size = values.fileSize, size <= 20 * 1_024 * 1_024 else {
      throw AttachmentError("Choose an image or text file smaller than 20 MB.")
    }
    let handle = try FileHandle(forReadingFrom: url)
    defer { try? handle.close() }
    let bytes = try handle.read(upToCount: 20 * 1_024 * 1_024 + 1) ?? Data()
    guard bytes.count <= 20 * 1_024 * 1_024 else { throw AttachmentError("That file is too large.") }
    let name = safeName(url.lastPathComponent)
    if let source = CGImageSourceCreateWithData(bytes as CFData, nil), CGImageSourceGetCount(source) > 0 {
      return try image(bytes, name: name)
    }
    let textExtensions: Set<String> = ["txt", "md", "log", "csv", "json", "yaml", "yml", "toml", "xml", "swift", "rs", "py", "js", "ts", "html", "css"]
    guard textExtensions.contains(url.pathExtension.lowercased()), bytes.count <= maximumTextBytes,
      let text = String(data: bytes, encoding: .utf8), !text.contains("\0")
    else { throw AttachmentError("Choose an image or a UTF-8 text file up to 128 KB. PDFs and other documents are not supported yet.") }
    return Self(name: name, mediaType: "text/plain", dataBase64: bytes.base64EncodedString())
  }

  static func image(_ bytes: Data, name: String) throws -> Self {
    guard bytes.count <= 20 * 1_024 * 1_024,
      let source = CGImageSourceCreateWithData(bytes as CFData, nil),
      CGImageSourceGetCount(source) == 1,
      let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
      let width = properties[kCGImagePropertyPixelWidth] as? Int,
      let height = properties[kCGImagePropertyPixelHeight] as? Int,
      width > 0, height > 0, width <= 40_000, height <= 40_000,
      Int64(width) * Int64(height) <= 80_000_000,
      let thumbnail = CGImageSourceCreateThumbnailAtIndex(source, 0, [
        kCGImageSourceCreateThumbnailFromImageAlways: true,
        kCGImageSourceThumbnailMaxPixelSize: 1_600,
        kCGImageSourceCreateThumbnailWithTransform: true,
        kCGImageSourceShouldCacheImmediately: true,
      ] as CFDictionary)
    else { throw AttachmentError("That image cannot be read or is too large.") }
    for quality in [0.9, 0.75, 0.55] {
      let output = NSMutableData()
      guard let destination = CGImageDestinationCreateWithData(output, UTType.jpeg.identifier as CFString, 1, nil) else {
        throw AttachmentError("The image could not be prepared.")
      }
      CGImageDestinationAddImage(destination, thumbnail, [kCGImageDestinationLossyCompressionQuality: quality] as CFDictionary)
      if CGImageDestinationFinalize(destination), output.length <= maximumImageBytes {
        let filename = (safeName(name) as NSString).deletingPathExtension + ".jpg"
        return Self(name: filename, mediaType: "image/jpeg", dataBase64: (output as Data).base64EncodedString())
      }
    }
    throw AttachmentError("The prepared image exceeds 2 MB. Try a smaller screenshot.")
  }

  private static func safeName(_ value: String) -> String {
    let clean = value.unicodeScalars.filter { !CharacterSet.controlCharacters.contains($0) && $0 != "/" && $0 != "\\" && $0 != ":" }
    let name = String(String.UnicodeScalarView(clean)).prefix(120)
    return name.isEmpty ? "attachment" : String(name)
  }
}

struct AttachmentError: LocalizedError {
  let message: String
  init(_ message: String) { self.message = message }
  var errorDescription: String? { message }
}
