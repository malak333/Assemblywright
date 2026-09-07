import AppKit
import Foundation
import ImageIO
import Testing
import UniformTypeIdentifiers
@testable import AssemblywrightMacApp

@Suite("Developer chat attachments")
struct DeveloperChatAttachmentTests {
  private func imageFixture(width: Int, height: Int) throws -> Data {
    let context = try #require(CGContext(data: nil, width: width, height: height,
      bitsPerComponent: 8, bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    context.setFillColor(CGColor(red: 0.9, green: 0.2, blue: 0.1, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    let image = try #require(context.makeImage())
    let data = NSMutableData()
    let destination = try #require(CGImageDestinationCreateWithData(data, UTType.jpeg.identifier as CFString, 1, nil))
    CGImageDestinationAddImage(destination, image, [kCGImagePropertyOrientation: 6, kCGImagePropertyGPSDictionary: [kCGImagePropertyGPSLatitude: 40.0, kCGImagePropertyGPSLatitudeRef: "N"]] as CFDictionary)
    #expect(CGImageDestinationFinalize(destination))
    return data as Data
  }

  @Test
  func screenshotIsResizedAndMetadataIsRemoved() throws {
    let original = try imageFixture(width: 2400, height: 1200)
    let originalSource = try #require(CGImageSourceCreateWithData(original as CFData, nil))
    let originalProperties = try #require(CGImageSourceCopyPropertiesAtIndex(originalSource, 0, nil) as? [CFString: Any])
    #expect(originalProperties[kCGImagePropertyGPSDictionary] != nil)
    let attachment = try DeveloperChatAttachment.image(original, name: "Screenshot.png")
    #expect(attachment.name == "Screenshot.jpg")
    #expect(attachment.mediaType == "image/jpeg")
    let bytes = try #require(attachment.data)
    #expect(bytes.count <= DeveloperChatAttachment.maximumImageBytes)
    let source = try #require(CGImageSourceCreateWithData(bytes as CFData, nil))
    let properties = try #require(CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any])
    #expect(properties[kCGImagePropertyPixelWidth] as? Int == 800)
    #expect(properties[kCGImagePropertyPixelHeight] as? Int == 1600)
    #expect(properties[kCGImagePropertyGPSDictionary] == nil)
  }

  @Test
  func invalidImagesAndAttachmentLimitsAreRejected() throws {
    #expect(throws: AttachmentError.self) { try DeveloperChatAttachment.image(Data("not an image".utf8), name: "fake.png") }
    #expect(throws: AttachmentError.self) { try DeveloperChatAttachment.image(Data(repeating: 0, count: 20 * 1024 * 1024 + 1), name: "huge.png") }
    let named = try DeveloperChatAttachment.image(imageFixture(width: 5, height: 5), name: "../a\\b:\n.png")
    #expect(!named.name.contains("/")); #expect(!named.name.contains("\\"))
    #expect(!named.name.contains(":")); #expect(!named.name.contains("\n"))
    let text = DeveloperChatAttachment(name: "notes.txt", mediaType: "text/plain", dataBase64: Data("notes".utf8).base64EncodedString())
    #expect(throws: AttachmentError.self) { try DeveloperChatAttachment.validateSelection(Array(repeating: text, count: 5)) }
    let large = DeveloperChatAttachment(name: "large.jpg", mediaType: "image/jpeg", dataBase64: Data(repeating: 1, count: 2 * 1024 * 1024).base64EncodedString())
    #expect(throws: AttachmentError.self) { try DeveloperChatAttachment.validateSelection(Array(repeating: large, count: 4)) }
  }

  @Test
  func textFilesRemainExactAndUnsupportedDocumentsReject() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let file = root.appendingPathComponent("notes.md")
    let original = Data("# Context\nKeep this as reference, not instructions.\n".utf8)
    try original.write(to: file)
    let attachment = try DeveloperChatAttachment.read(file)
    #expect(attachment.data == original)
    #expect(attachment.mediaType == "text/plain")
    #expect(attachment.name == "notes.md")
    let unsupported = root.appendingPathComponent("document.pdf")
    try Data("%PDF-fixture".utf8).write(to: unsupported)
    #expect(throws: AttachmentError.self) { try DeveloperChatAttachment.read(unsupported) }
    try Data([0xFF, 0xFE]).write(to: file)
    #expect(throws: AttachmentError.self) { try DeveloperChatAttachment.read(file) }
    try Data("contains\0nul".utf8).write(to: file)
    #expect(throws: AttachmentError.self) { try DeveloperChatAttachment.read(file) }
    try Data(repeating: 65, count: 128 * 1024 + 1).write(to: file)
    #expect(throws: AttachmentError.self) { try DeveloperChatAttachment.read(file) }
  }

  @Test
  func wireAttachmentsDecodeWithChatAndLegacyMessagesStillWork() throws {
    let attachment = try DeveloperChatAttachment.image(imageFixture(width: 20, height: 10), name: "small.png")
    let bytes = try JSONSerialization.data(withJSONObject: ["role": "user", "content": "What is shown?", "attachments": [attachment.wireValue]])
    let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
    let message = try decoder.decode(DeveloperChatMessage.self, from: bytes)
    #expect(message.attachments == [attachment])
    let legacy = try decoder.decode(DeveloperChatMessage.self, from: Data("{\"role\":\"user\",\"content\":\"old\"}".utf8))
    #expect(legacy.attachments == nil)
  }

  @Test
  @MainActor
  func uncertainRetryBindsAttachmentBytesAndProject() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let requests = root.appendingPathComponent("requests.jsonl")
    let script = #"""
import http.server,json,sys,urllib.parse
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  project=urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)['project'][0]
  self.reply(200,{'project':project,'messages':[],'running':False})
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  with open(sys.argv[1],'a') as f:f.write(json.dumps(body)+'\n')
  if body['project']=='beta':self.reply(200,{'project':'wrong-project','messages':[],'running':False})
  else:self.reply(503,{'error':'Uncertain fixture response; retry exact input'})
 def reply(self,status,value):
  b=json.dumps(value).encode();self.send_response(status);self.send_header('Content-Length',str(len(b)));self.end_headers();self.wfile.write(b)
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
sys.stdout.buffer.write(server.server_port.to_bytes(4,'big'));sys.stdout.buffer.flush();server.serve_forever()
"""#
    let server = Process(), output = Pipe()
    server.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    server.arguments = ["python3", "-u", "-c", script, requests.path]
    server.standardInput = FileHandle.nullDevice; server.standardOutput = output; server.standardError = FileHandle.nullDevice
    try server.run()
    defer { if server.isRunning { server.terminate(); server.waitUntilExit() }; try? output.fileHandleForReading.close() }
    let received = try output.fileHandleForReading.read(upToCount: 4)
    let handshake = try #require(received)
    #expect(handshake.count == 4)
    let port = handshake.reduce(0) { ($0 << 8) | Int($1) }
    let config = root.appendingPathComponent("runtime.json")
    try JSONSerialization.data(withJSONObject: ["endpoint": "http://127.0.0.1:\(port)", "token": "fixture"])
      .write(to: config)
    let model = DeveloperProjectChatModel(configurationPath: config.path)
    let observer = Task { await model.observe(project: "alpha") }
    defer { observer.cancel() }
    for _ in 0..<30 { if model.snapshot != nil { break }; try await Task.sleep(for: .milliseconds(50)) }
    let first = DeveloperChatAttachment(name: "notes.txt", mediaType: "text/plain", dataBase64: Data("first".utf8).base64EncodedString())
    let changed = DeveloperChatAttachment(name: "notes.txt", mediaType: "text/plain", dataBase64: Data("changed".utf8).base64EncodedString())
    #expect(await model.send(message: "", attachments: [first]) == false)
    #expect(await model.send(message: "", attachments: [first]) == false)
    #expect(await model.send(message: "", attachments: [changed]) == false)
    #expect(await model.send(message: "", attachments: [changed], modelTarget: "mac") == false)
    #expect(await model.send(message: "", attachments: [changed], modelTarget: "mac") == false)
    observer.cancel()
    let next = Task { await model.observe(project: "beta") }
    defer { next.cancel() }
    for _ in 0..<30 { if model.project == "beta" { break }; try await Task.sleep(for: .milliseconds(50)) }
    #expect(await model.send(message: "", attachments: [changed]) == false)
    let lines = try String(contentsOf: requests, encoding: .utf8).split(separator: "\n")
    let values = try lines.map { try JSONSerialization.jsonObject(with: Data($0.utf8)) as! [String: Any] }
    #expect(values.count == 6)
    #expect(values[0]["id"] as? String == values[1]["id"] as? String)
    #expect(values[1]["id"] as? String != values[2]["id"] as? String)
    #expect(values[2]["id"] as? String != values[3]["id"] as? String)
    #expect(values[3]["id"] as? String == values[4]["id"] as? String)
    #expect(values[4]["id"] as? String != values[5]["id"] as? String)
    #expect(values[0]["model_target"] as? String == "windows")
    #expect(values[3]["model_target"] as? String == "mac")
    #expect(values[5]["project"] as? String == "beta")
    #expect(model.snapshot?.project == "beta")
    #expect(model.error != nil)
    #expect((values[0]["attachments"] as? [[String: String]])?.first == first.wireValue)
  }
}
