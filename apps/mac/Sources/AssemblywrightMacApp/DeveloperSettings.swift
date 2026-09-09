import Foundation
import SwiftUI

struct GitHubRepoInfo: Identifiable, Decodable, Equatable {
  let id: String
  let name: String
  let url: String
  let lastFeature: String?
  let lastStatus: String?
}

@MainActor
final class DeveloperGitHubModel: ObservableObject {
  @Published var repos: [GitHubRepoInfo] = []
  @Published var selectedRepo: String?
  @Published var errorMessage: String?
  @Published var loading = false
  private let configurationPath: String
  private var configuration: DeveloperRunnerConfiguration? {
    try? JSONDecoder().decode(DeveloperRunnerConfiguration.self,
      from: Data(contentsOf: URL(fileURLWithPath: configurationPath)))
  }
  private let session: URLSession
  
  init(configurationPath: String) {
    self.configurationPath = configurationPath
    let settings = URLSessionConfiguration.ephemeral
    settings.timeoutIntervalForRequest = 15
    self.session = URLSession(configuration: settings)
  }
  
  func loadRepos() async {
    guard let configuration, let base = URL(string: configuration.endpoint) else { return }
    loading = true
    
    var components = URLComponents(url: base.appendingPathComponent("github/repos"),
                                   resolvingAgainstBaseURL: false)
    components?.queryItems = [URLQueryItem(name: "project", value: "")]
    guard let url = components?.url else { return }
    
    var request = URLRequest(url: url)
    request.setValue("Bearer \(configuration.token)", forHTTPHeaderField: "Authorization")
    
    do {
      let (data, _) = try await session.data(for: request)
      struct RepoList: Decodable { let repos: [GitHubRepoInfo] }
      let list = try JSONDecoder().decode(RepoList.self, from: data)
      repos = list.repos
      loading = false
    } catch {
      errorMessage = "Failed to load GitHub repos: \(error.localizedDescription)"
      loading = false
    }
  }
  
  func openRepo(_ repoUrl: String) {
    if let url = URL(string: repoUrl) {
      NSWorkspace.shared.open(url)
    }
  }
}

struct DeveloperGitHubView: View {
  @StateObject private var model: DeveloperGitHubModel
  @State private var showingSignIn = false
  
  private let configurationPath: String
  
  init(configurationPath: String) {
    self.configurationPath = configurationPath
    _model = StateObject(wrappedValue: DeveloperGitHubModel(configurationPath: configurationPath))
  }
  
  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      HStack {
        Text("GitHub").font(.title2.bold())
        Spacer()
        Button(action: { showingSignIn = true }) {
          Label("Sign in", systemImage: "person.crop.circle.badge.plus")
        }
        .buttonStyle(.plain)
      }
      
      if model.loading {
        ProgressView("Loading...")
      } else if let errorMessage = model.errorMessage {
        Text(errorMessage).foregroundStyle(.red)
      } else if model.repos.isEmpty {
        VStack(spacing: 8) {
          Image(systemName: "doc.badge.gearshape")
            .font(.largeTitle)
            .foregroundStyle(.secondary)
          Text("No repositories connected")
            .font(.headline)
          Text("Sign in to GitHub to connect a repository")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
      } else {
        List {
          ForEach(model.repos) { repo in
            Button(action: { model.openRepo(repo.url) }) {
              HStack {
                VStack(alignment: .leading) {
                  Text(repo.name).font(.headline)
                  if let lastFeature = repo.lastFeature, !lastFeature.isEmpty {
                    Text("Last feature: \(lastFeature)").font(.caption).foregroundStyle(.secondary)
                  }
                }
                Spacer()
                Image(systemName: "arrow.up.right")
                  .font(.caption)
              }
            }
          }
        }
      }
      
      Divider()
      
      VStack(alignment: .leading, spacing: 8) {
        Text("Automatic Publication").font(.headline)
        Text("Features that pass validation will automatically create a pull request and merge after checks pass.")
          .font(.caption)
          .foregroundStyle(.secondary)
        Toggle("Enable auto-merge", isOn: .constant(true))
          .disabled(model.repos.isEmpty)
      }
    }
    .padding()
    .task { await model.loadRepos() }
    .sheet(isPresented: $showingSignIn) {
      Text("Sign in to GitHub (Windows runner handles authentication)")
        .padding()
    }
  }
}

struct DeveloperRunnerSettingsView: View {
  @StateObject private var githubModel: DeveloperGitHubModel
  @State private var selectedTab = 0
  
  private let configurationPath: String
  
  init(configurationPath: String) {
    self.configurationPath = configurationPath
    _githubModel = StateObject(wrappedValue: DeveloperGitHubModel(configurationPath: configurationPath))
  }
  
  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      HStack {
        Text("Settings").font(.title2.bold())
        Spacer()
      }
      
      Picker("Settings", selection: $selectedTab) {
        Text("GitHub").tag(0)
        Text("Orchestrator").tag(1)
        Text("Reviewer").tag(2)
      }
      .pickerStyle(.segmented)
      .padding(.horizontal)
      
      Divider()
      
      switch selectedTab {
      case 0:
        DeveloperGitHubView(configurationPath: configurationPath)
      case 1:
        VStack(alignment: .leading, spacing: 12) {
          Text("Orchestrator").font(.headline)
          Text("Orchestrator settings are configured on Windows.")
            .foregroundStyle(.secondary)
        }
        .padding()
      case 2:
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
