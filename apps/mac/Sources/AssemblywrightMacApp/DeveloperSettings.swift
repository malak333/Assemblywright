import Foundation
import SwiftUI

struct DeveloperAISelection: Codable, Equatable {
  var model: String
  var reasoningEffort: String

  static func validModelID(_ model: String) -> Bool {
    model.hasPrefix("gpt-") && model.utf8.count <= 128
      && model.utf8.allSatisfy {
        (97...122).contains($0) || (48...57).contains($0) || [45, 46, 95].contains($0)
      }
  }

  var isWellFormed: Bool {
    Self.validModelID(model)
      && ["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"]
        .contains(reasoningEffort)
  }
}

struct DeveloperAISettings: Decodable, Equatable {
  let revision: UInt64
  let orchestrator: DeveloperAISelection
  let reviewer: DeveloperAISelection
}

struct DeveloperRunnerSettingsView: View {
  @State private var selectedTab = 0
  
  private let configurationPath: String
  
  init(configurationPath: String) {
    self.configurationPath = configurationPath
  }
  
  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      HStack {
        Text("Settings").font(.title2.bold())
        Spacer()
      }
      
      Picker("Settings", selection: $selectedTab) {
        Text("Orchestrator").tag(0)
        Text("Reviewer").tag(1)
      }
      .pickerStyle(.segmented)
      .padding(.horizontal)
      
      Divider()
      
      switch selectedTab {
      case 0:
        VStack(alignment: .leading, spacing: 12) {
          Text("Orchestrator").font(.headline)
          Text("Orchestrator settings are configured on Windows.")
            .foregroundStyle(.secondary)
        }
        .padding()
      case 1:
        VStack(alignment: .leading, spacing: 12) {
          Text("Reviewer").font(.headline)
          Text("Reviewer settings are configured on Windows.")
            .foregroundStyle(.secondary)
        }
        .padding()
      default:
        EmptyView()
      }
    }
  }
}
