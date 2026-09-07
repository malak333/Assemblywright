import Foundation
import SwiftUI

struct DeveloperConnectionStatus: Decodable {
  let phase: String
  let message: String
  let updatedAt: Double
  let attempt: Int

  func isFresh(at now: Date) -> Bool {
    let age = now.timeIntervalSince1970 - updatedAt
    return age >= -5 && age <= 20
  }
}

@MainActor
final class DeveloperConnectionModel: ObservableObject {
  @Published private(set) var status: DeveloperConnectionStatus?
  @Published private(set) var launchError: String?
  private let directory: URL
  private let canActivate: Bool
  private var activation: Process?

  init(configurationPath: String) {
    directory = URL(fileURLWithPath: configurationPath).deletingLastPathComponent()
    let expected = FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent("Library/Application Support/Assemblywright/Developer")
    canActivate = directory.standardizedFileURL == expected.standardizedFileURL
      && Bundle.main.object(forInfoDictionaryKey: "AssemblywrightDeveloperBuild") as? Bool == true
  }

  var needsAttention: Bool { status?.phase == "needs_attention" || launchError != nil }
  var title: String {
    if needsAttention { return "Connection needs attention" }
    switch status?.phase {
    case "connected": return "Connected to Windows"
    case "reconnecting": return "Reconnecting to Windows…"
    case "stopped": return "Windows connection is stopped"
    default: return "Connecting to Windows…"
    }
  }
  var message: String {
    launchError ?? status?.message
      ?? "The developer build manages its connection in the background. No SSH terminal is required."
  }

  func refresh(at now: Date = Date()) {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let observed = try? decoder.decode(DeveloperConnectionStatus.self,
      from: Data(contentsOf: directory.appendingPathComponent("connection-status.json")))
    status = observed?.isFresh(at: now) == true ? observed : nil
    if status != nil { launchError = nil }
  }

  func observe() async {
    if canActivate && activation == nil {
      if FileManager.default.fileExists(atPath: directory.appendingPathComponent("connection.json").path) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        process.arguments = ["kickstart", "gui/\(getuid())/com.nobiletechnology.assemblywright.developer-connection"]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do { try process.run(); activation = process }
        catch { launchError = "Open the developer launcher once to install its background connection." }
      } else {
        launchError = "Open the developer launcher once to configure its background connection."
      }
    }
    while !Task.isCancelled {
      refresh()
      if status == nil, let activation, !activation.isRunning && activation.terminationStatus != 0 {
        launchError = "Open the developer launcher once to install its background connection."
      }
      try? await Task.sleep(for: .seconds(1))
    }
  }
}
