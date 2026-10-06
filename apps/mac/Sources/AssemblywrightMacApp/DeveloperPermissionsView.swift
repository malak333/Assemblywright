import Foundation
import SwiftUI

struct DeveloperGlobalPermissions: Codable, Equatable {
  let mode: DeveloperToolAccessMode
  let revision: UInt64
  let available: Bool
  let executionHost: String

  var isWellFormed: Bool { revision > 0 && executionHost == "windows" }
}

enum DeveloperPermissionsAcknowledgement {
  static func saved(_ updated: DeveloperGlobalPermissions,
    from original: DeveloperGlobalPermissions, requested: DeveloperToolAccessMode) -> Bool
  {
    guard updated.isWellFormed, updated.mode == requested else { return false }
    if requested == original.mode { return updated.revision == original.revision }
    return original.revision < UInt64.max && updated.revision == original.revision + 1
  }
}

@MainActor
final class DeveloperPermissionsModel: ObservableObject {
  @Published private(set) var permissions: DeveloperGlobalPermissions?
  @Published private(set) var loading = false
  @Published private(set) var saving = false
  @Published var error: String?
  private let configurationPath: String
  private let session: URLSession

  init(configurationPath: String, session: URLSession? = nil) {
    self.configurationPath = configurationPath
    if let session { self.session = session }
    else {
      let configuration = URLSessionConfiguration.ephemeral
      configuration.timeoutIntervalForRequest = 10
      self.session = URLSession(configuration: configuration)
    }
  }

  func refresh() async {
    guard !loading && !saving else { return }
    loading = true
    defer { loading = false }
    do {
      permissions = try await request()
      error = nil
    } catch {
      permissions = nil
      self.error = error.localizedDescription
    }
  }

  func save(_ mode: DeveloperToolAccessMode, expectedRevision: UInt64) async {
    guard !loading, !saving, let original = permissions, original.revision == expectedRevision,
      original.mode != mode else { return }
    saving = true
    defer { saving = false }
    do {
      let updated = try await request(body: ["mode": mode.rawValue,
        "expected_revision": expectedRevision])
      guard DeveloperPermissionsAcknowledgement.saved(updated, from: original,
        requested: mode) else { throw URLError(.cannotParseResponse) }
      permissions = updated
      error = nil
    } catch {
      permissions = nil
      self.error = error.localizedDescription
    }
  }

  private func request(body: [String: Any]? = nil) async throws -> DeveloperGlobalPermissions {
    let configuration = try JSONDecoder().decode(DeveloperRunnerConfiguration.self,
      from: Data(contentsOf: URL(fileURLWithPath: configurationPath)))
    guard let base = URL(string: configuration.endpoint), base.scheme == "http",
      ["127.0.0.1", "localhost", "::1"].contains(base.host ?? "") else { throw URLError(.badURL) }
    var request = URLRequest(url: base.appendingPathComponent("permissions"))
    request.setValue("Bearer \(configuration.token)", forHTTPHeaderField: "Authorization")
    if let body {
      request.httpMethod = "POST"
      request.setValue("application/json", forHTTPHeaderField: "Content-Type")
      request.httpBody = try JSONSerialization.data(withJSONObject: body)
    }
    let (data, response) = try await session.data(for: request)
    guard (response as? HTTPURLResponse)?.statusCode == 200 else {
      let detail = (try? JSONSerialization.jsonObject(with: data)) as? [String: String]
      throw NSError(domain: "Developer Permissions", code: 1, userInfo: [
        NSLocalizedDescriptionKey: detail?["error"]
          ?? "Global Permissions are unavailable. Update the Windows Developer runner and reopen this build."])
    }
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    let value = try decoder.decode(DeveloperGlobalPermissions.self, from: data)
    guard value.isWellFormed else { throw URLError(.cannotParseResponse) }
    return value
  }
}

struct DeveloperPermissionsView: View {
  @StateObject private var model: DeveloperPermissionsModel
  @State private var selected: DeveloperToolAccessMode = .ask
  @ObservedObject var runner: DeveloperRunnerModel
  @Environment(\.dismiss) private var dismiss

  init(configurationPath: String, runner: DeveloperRunnerModel) {
    _model = StateObject(wrappedValue: DeveloperPermissionsModel(configurationPath: configurationPath))
    self.runner = runner
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      HStack {
        Label("Permissions", systemImage: "shield.lefthalf.filled").font(.title2.bold())
        Spacer()
        Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
      }
      Text("This setting applies to every Developer project, new chat, and new feature on Windows.")
        .foregroundStyle(.secondary)
      Picker("Access", selection: $selected) {
        ForEach(DeveloperToolAccessMode.allCases) { mode in Text(mode.label).tag(mode) }
      }
      .pickerStyle(.radioGroup)
      .disabled(model.permissions == nil || model.loading || model.saving)
      Text(selected.help).font(.caption).foregroundStyle(.secondary)
      if model.permissions?.available == false {
        Text("The permission is saved, but project tools are unavailable in the current Windows runner.")
          .font(.caption).foregroundStyle(.orange)
      }
      if let error = model.error {
        Text(error).foregroundStyle(.red).textSelection(.enabled)
        Button("Reload") { Task { await model.refresh() } }.disabled(model.loading)
      }
      HStack {
        if model.loading || model.saving { ProgressView().controlSize(.small) }
        Spacer()
        Button("Save") {
          if let permissions = model.permissions {
            Task { await model.save(selected, expectedRevision: permissions.revision) }
          }
        }
        .buttonStyle(.borderedProminent)
        .disabled(model.permissions == nil || model.permissions?.mode == selected || model.loading || model.saving
          || runner.snapshot?.canChangeChatAccess != true)
        .accessibilityIdentifier("developer-permissions-save")
      }
      if runner.snapshot?.canChangeChatAccess != true {
        Text("Stop active work, clear Emergency Pause, and resolve publication or setup work before changing permissions.")
          .font(.caption).foregroundStyle(.secondary)
      }
    }
    .padding(24).frame(width: 430)
    .task { await model.refresh() }
    .onChange(of: model.permissions, initial: true) { _, value in
      if let value { selected = value.mode }
    }
  }
}
