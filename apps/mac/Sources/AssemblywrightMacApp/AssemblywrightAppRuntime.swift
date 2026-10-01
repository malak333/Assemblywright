import Foundation

enum AssemblywrightAppRuntime: Equatable {
  case developer(configurationPath: String)
  case protectedService

  static let developerConfigurationEnvironmentKey = "ASSEMBLYWRIGHT_DEVELOPER_CONFIG"
  static let runtimeEnvironmentKey = "ASSEMBLYWRIGHT_RUNTIME"
  static let bundleRuntimeKey = "AssemblywrightRuntime"
  static let developerRuntimeValue = "developer"
  static let protectedServiceRuntimeValue = "protected-service"

  static func resolve(
    environment: [String: String] = ProcessInfo.processInfo.environment,
    bundleInfo: [String: Any] = Bundle.main.infoDictionary ?? [:],
    homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser
  ) -> Self {
    if let explicitConfiguration = environment[developerConfigurationEnvironmentKey],
       !explicitConfiguration.isEmpty {
      return .developer(configurationPath: explicitConfiguration)
    }

    if environment[runtimeEnvironmentKey] == protectedServiceRuntimeValue {
      return .protectedService
    }

    if bundleInfo[bundleRuntimeKey] as? String == protectedServiceRuntimeValue {
      return .protectedService
    }

    let retainedConfiguration = homeDirectory.appendingPathComponent(
      "Library/Application Support/Assemblywright/Developer/runtime.json"
    ).path
    return .developer(configurationPath: retainedConfiguration)
  }
}
