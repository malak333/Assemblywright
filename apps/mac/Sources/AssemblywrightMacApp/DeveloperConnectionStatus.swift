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

struct DeveloperConnectionActivationPolicy {
  static func shouldKickstart(
    canActivate: Bool,
    connectionConfigurationExists: Bool
  ) -> Bool {
    canActivate && connectionConfigurationExists
  }

  static func canActivate(
    configurationPath: String,
    environment: [String: String] = ProcessInfo.processInfo.environment,
    bundleInfo: [String: Any] = Bundle.main.infoDictionary ?? [:],
    homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser
  ) -> Bool {
    let expectedConfiguration = homeDirectory.appendingPathComponent(
      "Library/Application Support/Assemblywright/Developer/runtime.json"
    )
    guard URL(fileURLWithPath: configurationPath).standardizedFileURL
      == expectedConfiguration.standardizedFileURL else {
      return false
    }
    guard case .developer = AssemblywrightAppRuntime.resolve(
      environment: environment,
      bundleInfo: bundleInfo,
      homeDirectory: homeDirectory
    ) else {
      return false
    }
    let productionDeveloperRuntime =
      bundleInfo[AssemblywrightAppRuntime.bundleRuntimeKey] as? String
        == AssemblywrightAppRuntime.developerRuntimeValue
    let legacyDeveloperBuild =
      bundleInfo["AssemblywrightDeveloperBuild"] as? Bool == true
    return productionDeveloperRuntime || legacyDeveloperBuild
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
    canActivate = DeveloperConnectionActivationPolicy.canActivate(
      configurationPath: configurationPath
    )
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
      ?? "Assemblywright manages its Developer connection in the background. No SSH terminal is required."
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
      let connectionConfigurationExists = FileManager.default.fileExists(
        atPath: directory.appendingPathComponent("connection.json").path
      )
      if DeveloperConnectionActivationPolicy.shouldKickstart(
        canActivate: canActivate,
        connectionConfigurationExists: connectionConfigurationExists
      ) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        process.arguments = ["kickstart", "gui/\(getuid())/com.nobiletechnology.assemblywright.developer-connection"]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do { try process.run(); activation = process }
        catch { launchError = "Open Assemblywright.command once to install its background connection." }
      } else {
        launchError = "Open Assemblywright.command once to configure its background connection."
      }
    }
    while !Task.isCancelled {
      refresh()
      if status == nil, let activation, !activation.isRunning && activation.terminationStatus != 0 {
        launchError = "Open Assemblywright.command once to install its background connection."
      }
      try? await Task.sleep(for: .seconds(1))
    }
  }
}
