import Foundation
import Testing
@testable import AssemblywrightMacApp

@Suite("Assemblywright app runtime selection")
struct AssemblywrightAppRuntimeTests {
  private let home = URL(fileURLWithPath: "/Users/owner")

  @Test("Plain production bundle defaults to the retained Developer runtime")
  func productionDefault() {
    #expect(
      AssemblywrightAppRuntime.resolve(environment: [:], bundleInfo: [:], homeDirectory: home)
        == .developer(
          configurationPath:
            "/Users/owner/Library/Application Support/Assemblywright/Developer/runtime.json"
        )
    )
  }

  @Test("Only the exact protected-service bundle value selects the protected shell")
  func protectedServiceRequiresExactBundleValue() {
    #expect(
      AssemblywrightAppRuntime.resolve(
        environment: [:],
        bundleInfo: [AssemblywrightAppRuntime.bundleRuntimeKey: "protected-service"],
        homeDirectory: home
      ) == .protectedService
    )
    for invalid in ["protected", "Protected-Service", "protected-service ", ""] {
      #expect(
        AssemblywrightAppRuntime.resolve(
          environment: [:],
          bundleInfo: [AssemblywrightAppRuntime.bundleRuntimeKey: invalid],
          homeDirectory: home
        ) == .developer(
          configurationPath:
            "/Users/owner/Library/Application Support/Assemblywright/Developer/runtime.json"
        )
      )
    }
  }

  @Test("Only the exact protected-service environment value selects the protected shell")
  func protectedServiceEnvironmentRequiresExactValue() {
    #expect(
      AssemblywrightAppRuntime.resolve(
        environment: [AssemblywrightAppRuntime.runtimeEnvironmentKey: "protected-service"],
        bundleInfo: [:],
        homeDirectory: home
      ) == .protectedService
    )
    for invalid in ["protected", "Protected-Service", "protected-service ", ""] {
      #expect(
        AssemblywrightAppRuntime.resolve(
          environment: [AssemblywrightAppRuntime.runtimeEnvironmentKey: invalid],
          bundleInfo: [:],
          homeDirectory: home
        ) == .developer(
          configurationPath:
            "/Users/owner/Library/Application Support/Assemblywright/Developer/runtime.json"
        )
      )
    }
  }

  @Test("Explicit Developer configuration remains compatible and wins over bundle selection")
  func explicitDeveloperConfigurationWins() {
    #expect(
      AssemblywrightAppRuntime.resolve(
        environment: [
          AssemblywrightAppRuntime.developerConfigurationEnvironmentKey: "/private/runtime.json",
          AssemblywrightAppRuntime.runtimeEnvironmentKey:
            AssemblywrightAppRuntime.protectedServiceRuntimeValue,
        ],
        bundleInfo: [
          AssemblywrightAppRuntime.bundleRuntimeKey:
            AssemblywrightAppRuntime.protectedServiceRuntimeValue
        ],
        homeDirectory: home
      ) == .developer(configurationPath: "/private/runtime.json")
    )
  }
}
