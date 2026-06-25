import Foundation

public enum JarvisModelProviderSelection: String, CaseIterable, Identifiable, Sendable {
    case codex
    case fake
    case ollama

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .codex:
            return "Codex"
        case .fake:
            return "Fake local"
        case .ollama:
            return "Ollama"
        }
    }
}

public struct JarvisModelConfiguration: Equatable, Sendable {
    public var provider: JarvisModelProviderSelection
    public var localModel: String
    public var ollamaBaseURL: String
    public var codexModel: String
    public var codexBaseURL: String
    public var timeoutMilliseconds: String

    public init(
        provider: JarvisModelProviderSelection = .fake,
        localModel: String = "fake-local-model",
        ollamaBaseURL: String = "http://127.0.0.1:11434",
        codexModel: String = "gpt-4.1-mini",
        codexBaseURL: String = "https://api.openai.com/v1",
        timeoutMilliseconds: String = "60000"
    ) {
        self.provider = provider
        self.localModel = localModel
        self.ollamaBaseURL = ollamaBaseURL
        self.codexModel = codexModel
        self.codexBaseURL = codexBaseURL
        self.timeoutMilliseconds = timeoutMilliseconds
    }

    public static func fromEnvironment(_ environment: [String: String] = ProcessInfo.processInfo.environment) -> Self {
        let provider = JarvisModelProviderSelection(
            rawValue: environment["JARVIS_LOCAL_MODEL_PROVIDER"]?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        ) ?? (environment["JARVIS_CHATGPT_ENABLED"] == "true" ? .codex : .fake)
        let model = environment["JARVIS_LOCAL_MODEL"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        let baseURL = environment["JARVIS_OLLAMA_BASE_URL"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        let codexModel = environment["JARVIS_CHATGPT_MODEL"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        let codexBaseURL = (environment["JARVIS_OPENAI_BASE_URL"] ?? environment["JARVIS_CHATGPT_BASE_URL"])?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let timeout = (provider == .codex
            ? environment["JARVIS_CHATGPT_TIMEOUT_MS"]
            : environment["JARVIS_LOCAL_MODEL_TIMEOUT_MS"])?
            .trimmingCharacters(in: .whitespacesAndNewlines)

        return JarvisModelConfiguration(
            provider: provider,
            localModel: model?.isEmpty == false ? model! : (provider == .ollama ? "llama3.2" : "fake-local-model"),
            ollamaBaseURL: baseURL?.isEmpty == false ? baseURL! : "http://127.0.0.1:11434",
            codexModel: codexModel?.isEmpty == false ? codexModel! : "gpt-4.1-mini",
            codexBaseURL: codexBaseURL?.isEmpty == false ? codexBaseURL! : "https://api.openai.com/v1",
            timeoutMilliseconds: timeout?.isEmpty == false ? timeout! : "60000"
        )
    }

    public var launchEnvironmentOverrides: [String: String] {
        switch provider {
        case .codex:
            return [
                "JARVIS_LOCAL_MODEL_ENABLED": "false",
                "JARVIS_CHATGPT_ENABLED": "true",
                "JARVIS_CHATGPT_MODEL": sanitizedCodexModel,
                "JARVIS_OPENAI_BASE_URL": sanitizedCodexBaseURL,
                "JARVIS_CHATGPT_TIMEOUT_MS": sanitizedTimeoutMilliseconds,
                "JARVIS_CHATGPT_REQUIRES_APPROVAL": "true"
            ]
        case .fake:
            return [
                "JARVIS_LOCAL_MODEL_ENABLED": "true",
                "JARVIS_LOCAL_MODEL_PROVIDER": "fake",
                "JARVIS_LOCAL_MODEL": "fake-local-model",
                "JARVIS_CHATGPT_ENABLED": "false"
            ]
        case .ollama:
            return [
                "JARVIS_LOCAL_MODEL_ENABLED": "true",
                "JARVIS_LOCAL_MODEL_PROVIDER": "ollama",
                "JARVIS_LOCAL_MODEL": sanitizedModel,
                "JARVIS_OLLAMA_BASE_URL": sanitizedOllamaBaseURL,
                "JARVIS_LOCAL_MODEL_TIMEOUT_MS": sanitizedTimeoutMilliseconds,
                "JARVIS_CHATGPT_ENABLED": "false"
            ]
        }
    }

    public var sanitizedModel: String {
        let trimmed = localModel.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? (provider == .ollama ? "llama3.2" : "fake-local-model") : trimmed
    }

    public var sanitizedOllamaBaseURL: String {
        let trimmed = ollamaBaseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "http://127.0.0.1:11434" : trimmed.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }

    public var sanitizedCodexModel: String {
        let trimmed = codexModel.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "gpt-4.1-mini" : trimmed
    }

    public var sanitizedCodexBaseURL: String {
        let trimmed = codexBaseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "https://api.openai.com/v1" : trimmed.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }

    public var sanitizedTimeoutMilliseconds: String {
        let trimmed = timeoutMilliseconds.trimmingCharacters(in: .whitespacesAndNewlines)
        guard Int(trimmed).map({ $0 > 0 }) == true else {
            return "60000"
        }
        return trimmed
    }
}

public protocol JarvisLocalModelRuntimeControlling: Sendable {
    func listOllamaModels(baseURL: URL) async throws -> [JarvisOllamaModelInfo]
    func pullOllamaModel(
        model: String,
        baseURL: URL,
        progress: @escaping @Sendable (JarvisOllamaPullProgress) async -> Void
    ) async throws
    func loadOllamaModel(model: String, baseURL: URL) async throws
    func unloadOllamaModel(model: String, baseURL: URL) async throws
}

public struct OllamaModelRuntimeController: JarvisLocalModelRuntimeControlling {
    private let urlSession: URLSession

    public init(urlSession: URLSession = .shared) {
        self.urlSession = urlSession
    }

    public func listOllamaModels(baseURL: URL) async throws -> [JarvisOllamaModelInfo] {
        let url = baseURL.appending(path: "api").appending(path: "tags")
        let (data, response) = try await urlSession.data(from: url)
        guard let http = response as? HTTPURLResponse, 200..<300 ~= http.statusCode else {
            throw URLError(.badServerResponse)
        }
        let tags = try JSONDecoder().decode(OllamaTagsResponse.self, from: data)
        return tags.models.map { model in
            JarvisOllamaModelInfo(
                name: model.name,
                installed: true,
                diskSizeBytes: model.size,
                estimatedRamBytes: model.size,
                details: model.details.summary
            )
        }
    }

    public func pullOllamaModel(
        model: String,
        baseURL: URL,
        progress: @escaping @Sendable (JarvisOllamaPullProgress) async -> Void
    ) async throws {
        let url = baseURL.appending(path: "api").appending(path: "pull")
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(OllamaPullRequest(model: model, stream: true))

        let (bytes, response) = try await urlSession.bytes(for: request)
        guard let http = response as? HTTPURLResponse, 200..<300 ~= http.statusCode else {
            throw URLError(.badServerResponse)
        }

        let decoder = JSONDecoder()
        for try await line in bytes.lines {
            guard !line.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { continue }
            let event = try decoder.decode(OllamaPullResponse.self, from: Data(line.utf8))
            if let error = event.error?.trimmingCharacters(in: .whitespacesAndNewlines), !error.isEmpty {
                throw OllamaPullError(message: error)
            }
            await progress(event.progress)
        }
    }

    public func loadOllamaModel(model: String, baseURL: URL) async throws {
        try await sendGenerateRequest(model: model, baseURL: baseURL, keepAlive: "5m")
    }

    public func unloadOllamaModel(model: String, baseURL: URL) async throws {
        try await sendGenerateRequest(model: model, baseURL: baseURL, keepAlive: "0")
    }

    private func sendGenerateRequest(model: String, baseURL: URL, keepAlive: String) async throws {
        let url = baseURL.appending(path: "api").appending(path: "generate")
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(OllamaGenerateRequest(
            model: model,
            prompt: "",
            stream: false,
            keepAlive: keepAlive
        ))

        let (_, response) = try await urlSession.data(for: request)
        guard let http = response as? HTTPURLResponse, 200..<300 ~= http.statusCode else {
            throw URLError(.badServerResponse)
        }
    }
}

public struct JarvisOllamaModelInfo: Equatable, Identifiable, Sendable {
    public var name: String
    public var installed: Bool
    public var diskSizeBytes: Int64?
    public var estimatedRamBytes: Int64?
    public var details: String?

    public var id: String { name }

    public init(
        name: String,
        installed: Bool,
        diskSizeBytes: Int64? = nil,
        estimatedRamBytes: Int64? = nil,
        details: String? = nil
    ) {
        self.name = name
        self.installed = installed
        self.diskSizeBytes = diskSizeBytes
        self.estimatedRamBytes = estimatedRamBytes
        self.details = details
    }

    public var memoryLine: String {
        guard let estimatedRamBytes else {
            return "RAM estimate unavailable until downloaded"
        }
        return "Estimated RAM: \(Self.formatBytes(estimatedRamBytes))"
    }

    public var sizeLine: String {
        guard let diskSizeBytes else {
            return "Download required"
        }
        return "Model size: \(Self.formatBytes(diskSizeBytes))"
    }

    public func matches(name otherName: String) -> Bool {
        Self.namesMatch(name, otherName)
    }

    public static func namesMatch(_ lhs: String, _ rhs: String) -> Bool {
        normalizedName(lhs) == normalizedName(rhs)
    }

    public static func formatBytes(_ bytes: Int64) -> String {
        let formatter = ByteCountFormatter()
        formatter.allowedUnits = [.useGB, .useMB]
        formatter.countStyle = .memory
        return formatter.string(fromByteCount: bytes)
    }

    private static func normalizedName(_ name: String) -> String {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.hasSuffix(":latest") ? String(trimmed.dropLast(":latest".count)) : trimmed
    }
}

public struct JarvisOllamaPullProgress: Equatable, Sendable {
    public var status: String
    public var completedBytes: Int64?
    public var totalBytes: Int64?

    public init(status: String, completedBytes: Int64? = nil, totalBytes: Int64? = nil) {
        self.status = status
        self.completedBytes = completedBytes
        self.totalBytes = totalBytes
    }

    public var fractionCompleted: Double? {
        guard let completedBytes, let totalBytes, totalBytes > 0 else { return nil }
        return min(max(Double(completedBytes) / Double(totalBytes), 0), 1)
    }

    public var detailLine: String {
        guard let completedBytes, let totalBytes, totalBytes > 0 else {
            return status
        }
        let completed = JarvisOllamaModelInfo.formatBytes(completedBytes)
        let total = JarvisOllamaModelInfo.formatBytes(totalBytes)
        return "\(status) \(completed) of \(total)"
    }
}

private struct OllamaTagsResponse: Decodable {
    var models: [OllamaTagModel]
}

private struct OllamaTagModel: Decodable {
    var name: String
    var size: Int64
    var details: OllamaTagDetails
}

private struct OllamaTagDetails: Decodable {
    var family: String?
    var parameterSize: String?
    var quantizationLevel: String?

    enum CodingKeys: String, CodingKey {
        case family
        case parameterSize = "parameter_size"
        case quantizationLevel = "quantization_level"
    }

    var summary: String? {
        [family, parameterSize, quantizationLevel]
            .compactMap { value in
                let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines)
                return trimmed?.isEmpty == false ? trimmed : nil
            }
            .joined(separator: " / ")
            .nilIfEmpty
    }
}

private struct OllamaGenerateRequest: Encodable {
    var model: String
    var prompt: String
    var stream: Bool
    var keepAlive: String

    enum CodingKeys: String, CodingKey {
        case model
        case prompt
        case stream
        case keepAlive = "keep_alive"
    }
}

private struct OllamaPullRequest: Encodable {
    var model: String
    var stream: Bool
}

private struct OllamaPullResponse: Decodable {
    var status: String?
    var total: Int64?
    var completed: Int64?
    var error: String?

    var progress: JarvisOllamaPullProgress {
        JarvisOllamaPullProgress(status: status ?? "download response", completedBytes: completed, totalBytes: total)
    }
}

private struct OllamaPullError: LocalizedError {
    var message: String

    var errorDescription: String? {
        message
    }
}

private extension String {
    var nilIfEmpty: String? {
        isEmpty ? nil : self
    }
}

@MainActor
public final class ModelConfigurationModel: ObservableObject {
    @Published public var configuration: JarvisModelConfiguration
    @Published public private(set) var availableModels: [JarvisOllamaModelInfo]
    @Published public private(set) var activeProvider: String?
    @Published public private(set) var activeModel: String?
    @Published public private(set) var statusMessage: String?
    @Published public var codexAPIKeyEntry: String
    @Published public private(set) var hasStoredCodexCredential: Bool
    @Published public private(set) var isWorking: Bool
    @Published public private(set) var downloadProgress: JarvisOllamaPullProgress?

    private let controller: any JarvisLocalModelRuntimeControlling
    private let credentialStore: any JarvisCredentialStore

    public init(
        configuration: JarvisModelConfiguration = .fromEnvironment(),
        controller: any JarvisLocalModelRuntimeControlling = OllamaModelRuntimeController(),
        credentialStore: any JarvisCredentialStore = KeychainJarvisCredentialStore()
    ) {
        self.configuration = configuration
        self.controller = controller
        self.credentialStore = credentialStore
        self.availableModels = []
        self.activeProvider = nil
        self.activeModel = nil
        self.statusMessage = nil
        self.codexAPIKeyEntry = ""
        self.hasStoredCodexCredential = false
        self.isWorking = false
        self.downloadProgress = nil
        refreshCodexCredentialState()
    }

    public var launchEnvironmentOverrides: [String: String] {
        configuration.launchEnvironmentOverrides
    }

    public var canControlSelectedModelRuntime: Bool {
        configuration.provider == .ollama && URL(string: configuration.sanitizedOllamaBaseURL) != nil
    }

    public var selectedModelIsInstalled: Bool {
        let selected = configuration.sanitizedModel
        return availableModels.contains { $0.matches(name: selected) && $0.installed }
    }

    public func applyHealth(_ health: JarvisHealth?) {
        if health?.chatgptEnabled == true {
            activeProvider = "codex"
            activeModel = health?.chatgptModel
        } else {
            activeProvider = health?.localModelProvider
            activeModel = health?.localModel
        }
    }

    public func refreshCodexCredentialState() {
        hasStoredCodexCredential = ((try? credentialStore.readCredential(.openAIAPIKey)) ?? nil)?
            .isEmpty == false
    }

    public func saveCodexCredential() {
        let trimmed = codexAPIKeyEntry.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            statusMessage = "Enter a Codex API key before saving."
            refreshCodexCredentialState()
            return
        }

        do {
            try credentialStore.saveCredential(trimmed, for: .openAIAPIKey)
            codexAPIKeyEntry = ""
            hasStoredCodexCredential = true
            statusMessage = "Codex application credential saved to Keychain."
        } catch {
            statusMessage = "Codex credential save failed: \(error)"
            refreshCodexCredentialState()
        }
    }

    public func deleteCodexCredential() {
        do {
            try credentialStore.deleteCredential(.openAIAPIKey)
            codexAPIKeyEntry = ""
            hasStoredCodexCredential = false
            statusMessage = "Codex application credential removed."
        } catch {
            statusMessage = "Codex credential removal failed: \(error)"
            refreshCodexCredentialState()
        }
    }

    public func saveEnteredCodexCredentialIfNeeded() {
        guard !codexAPIKeyEntry.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            refreshCodexCredentialState()
            return
        }
        saveCodexCredential()
    }

    public func refreshAvailableModels() async {
        guard configuration.provider == .ollama else {
            availableModels = []
            return
        }

        isWorking = true
        defer { isWorking = false }

        do {
            let installedModels = try await controller.listOllamaModels(baseURL: try ollamaBaseURL())
            availableModels = mergedModels(installedModels: installedModels)
            statusMessage = "Loaded \(installedModels.count) installed Ollama model(s)."
        } catch {
            availableModels = mergedModels(installedModels: [])
            statusMessage = "Ollama model inventory failed: \(error)"
        }
    }

    public func selectModel(_ model: JarvisOllamaModelInfo) async {
        configuration.localModel = model.name
        guard !model.installed else {
            statusMessage = "\(model.name) selected."
            return
        }
        await downloadSelectedModel()
    }

    public func downloadSelectedModel() async {
        downloadProgress = JarvisOllamaPullProgress(status: "Starting download")
        await runOllamaAction(action: "downloaded and reloaded") {
            try await controller.pullOllamaModel(
                model: configuration.sanitizedModel,
                baseURL: try ollamaBaseURL(),
                progress: { [weak self] progress in
                    await MainActor.run {
                        self?.downloadProgress = progress
                    }
                }
            )
            try await refreshAvailableModelsAfterDownload()
        }
        downloadProgress = nil
    }

    public func ensureSelectedModelAvailable() async {
        guard configuration.provider == .ollama else { return }
        let selected = configuration.sanitizedModel
        if availableModels.isEmpty {
            await refreshAvailableModels()
        }
        let isInstalled = availableModels.contains { $0.matches(name: selected) && $0.installed }
        if !isInstalled {
            await downloadSelectedModel()
        }
    }

    public func loadSelectedModel() async {
        await ensureSelectedModelAvailable()
        await runOllamaAction(action: "started") {
            try await controller.loadOllamaModel(
                model: configuration.sanitizedModel,
                baseURL: try ollamaBaseURL()
            )
        }
    }

    public func unloadSelectedModel() async {
        await runOllamaAction(action: "stopped") {
            try await controller.unloadOllamaModel(
                model: configuration.sanitizedModel,
                baseURL: try ollamaBaseURL()
            )
        }
    }

    private func runOllamaAction(action: String, operation: () async throws -> Void) async {
        guard configuration.provider == .ollama else {
            statusMessage = "Model runtime controls are available for Ollama only."
            return
        }

        isWorking = true
        defer { isWorking = false }

        do {
            try await operation()
            statusMessage = "\(configuration.sanitizedModel) \(action) through Ollama."
        } catch {
            statusMessage = "Ollama model \(action) failed: \(error)"
        }
    }

    private func refreshAvailableModelsAfterDownload() async throws {
        let installedModels = try await controller.listOllamaModels(baseURL: try ollamaBaseURL())
        availableModels = mergedModels(installedModels: installedModels)
    }

    private func ollamaBaseURL() throws -> URL {
        guard let url = URL(string: configuration.sanitizedOllamaBaseURL) else {
            throw URLError(.badURL)
        }
        return url
    }

    private func mergedModels(installedModels: [JarvisOllamaModelInfo]) -> [JarvisOllamaModelInfo] {
        var byName = Dictionary(uniqueKeysWithValues: installedModels.map { ($0.name, $0) })
        for model in Self.recommendedModels where !byName.keys.contains(where: { JarvisOllamaModelInfo.namesMatch($0, model.name) }) {
            byName[model.name] = model
        }
        if !byName.keys.contains(where: { JarvisOllamaModelInfo.namesMatch($0, configuration.sanitizedModel) }) {
            byName[configuration.sanitizedModel] = JarvisOllamaModelInfo(
                name: configuration.sanitizedModel,
                installed: false
            )
        }
        return byName.values.sorted { left, right in
            if left.installed != right.installed {
                return left.installed && !right.installed
            }
            return left.name.localizedStandardCompare(right.name) == .orderedAscending
        }
    }

    public static let recommendedModels: [JarvisOllamaModelInfo] = [
        recommendedModel("llama3.2", gb: 2.0, details: "default local model"),
        recommendedModel("llama3.1:8b", gb: 4.9, details: "general purpose"),
        recommendedModel("mistral", gb: 4.1, details: "general purpose"),
        recommendedModel("phi3:mini", gb: 2.3, details: "small local model"),
        recommendedModel("gemma2:2b", gb: 1.6, details: "small Gemma"),

        recommendedModel("gemma4", gb: 9.6, details: "Gemma 4 latest / E4B"),
        recommendedModel("gemma4:e2b", gb: 7.2, details: "Gemma 4 edge E2B"),
        recommendedModel("gemma4:e4b", gb: 9.6, details: "Gemma 4 edge E4B"),
        recommendedModel("gemma4:12b", gb: 7.6, details: "Gemma 4 workstation 12B"),
        recommendedModel("gemma4:26b", gb: 18.0, details: "Gemma 4 MoE 26B"),
        recommendedModel("gemma4:31b", gb: 20.0, details: "Gemma 4 dense 31B"),
        recommendedModel("gemma4:e2b-mlx", gb: 7.1, details: "Gemma 4 MLX edge E2B"),
        recommendedModel("gemma4:e4b-mlx", gb: 9.6, details: "Gemma 4 MLX edge E4B"),
        recommendedModel("gemma4:12b-mlx", gb: 6.8, details: "Gemma 4 MLX 12B"),
        recommendedModel("gemma4:26b-mlx", gb: 17.0, details: "Gemma 4 MLX 26B"),
        recommendedModel("gemma4:31b-mlx", gb: 20.0, details: "Gemma 4 MLX 31B"),

        recommendedModel("gemma3:270m", gb: 0.3, details: "Gemma 3 text"),
        recommendedModel("gemma3:1b", gb: 0.8, details: "Gemma 3 text"),
        recommendedModel("gemma3:4b", gb: 3.3, details: "Gemma 3 vision"),
        recommendedModel("gemma3:12b", gb: 8.1, details: "Gemma 3 vision"),
        recommendedModel("gemma3:27b", gb: 17.0, details: "Gemma 3 vision"),
        recommendedModel("gemma3:1b-it-qat", gb: 0.8, details: "Gemma 3 QAT"),
        recommendedModel("gemma3:4b-it-qat", gb: 3.3, details: "Gemma 3 QAT"),
        recommendedModel("gemma3:12b-it-qat", gb: 8.1, details: "Gemma 3 QAT"),
        recommendedModel("gemma3:27b-it-qat", gb: 17.0, details: "Gemma 3 QAT"),

        recommendedModel("qwen3.6", gb: 24.0, details: "Qwen3.6 latest"),
        recommendedModel("qwen3.6:27b", gb: 17.0, details: "Qwen3.6 27B"),
        recommendedModel("qwen3.6:35b", gb: 24.0, details: "Qwen3.6 35B"),
        recommendedModel("qwen3.6:27b-mlx", gb: 20.0, details: "Qwen3.6 MLX 27B"),
        recommendedModel("qwen3.6:35b-mlx", gb: 22.0, details: "Qwen3.6 MLX 35B"),

        recommendedModel("qwen3:0.6b", gb: 0.6, details: "Qwen3 small"),
        recommendedModel("qwen3:1.7b", gb: 1.4, details: "Qwen3 small"),
        recommendedModel("qwen3:4b", gb: 2.6, details: "Qwen3"),
        recommendedModel("qwen3:8b", gb: 5.2, details: "Qwen3"),
        recommendedModel("qwen3:14b", gb: 9.3, details: "Qwen3"),
        recommendedModel("qwen3:30b", gb: 19.0, details: "Qwen3 MoE 30B"),
        recommendedModel("qwen3:32b", gb: 20.0, details: "Qwen3"),
        recommendedModel("qwen3:235b", gb: 150.0, details: "Qwen3 very large"),

        recommendedModel("qwen2.5:0.5b", gb: 0.5, details: "Qwen2.5"),
        recommendedModel("qwen2.5:1.5b", gb: 1.1, details: "Qwen2.5"),
        recommendedModel("qwen2.5:3b", gb: 2.0, details: "Qwen2.5"),
        recommendedModel("qwen2.5:7b", gb: 4.7, details: "Qwen2.5"),
        recommendedModel("qwen2.5:14b", gb: 9.0, details: "Qwen2.5"),
        recommendedModel("qwen2.5:32b", gb: 20.0, details: "Qwen2.5"),
        recommendedModel("qwen2.5:72b", gb: 49.0, details: "Qwen2.5 large"),

        recommendedModel("qwen2.5-coder:0.5b", gb: 0.5, details: "Qwen coder"),
        recommendedModel("qwen2.5-coder:1.5b", gb: 1.1, details: "Qwen coder"),
        recommendedModel("qwen2.5-coder:3b", gb: 2.0, details: "Qwen coder"),
        recommendedModel("qwen2.5-coder:7b", gb: 4.7, details: "Qwen coder"),
        recommendedModel("qwen2.5-coder:14b", gb: 9.0, details: "Qwen coder"),
        recommendedModel("qwen2.5-coder:32b", gb: 20.0, details: "Qwen coder"),

        recommendedModel("qwen2.5vl", gb: 6.0, details: "Qwen2.5 vision latest"),
        recommendedModel("qwen2.5vl:3b", gb: 3.2, details: "Qwen2.5 vision"),
        recommendedModel("qwen2.5vl:7b", gb: 6.0, details: "Qwen2.5 vision"),
        recommendedModel("qwen2.5vl:32b", gb: 21.0, details: "Qwen2.5 vision"),
        recommendedModel("qwen2.5vl:72b", gb: 49.0, details: "Qwen2.5 vision large")
    ]

    private static func recommendedModel(_ name: String, gb: Double, details: String) -> JarvisOllamaModelInfo {
        JarvisOllamaModelInfo(
            name: name,
            installed: false,
            diskSizeBytes: nil,
            estimatedRamBytes: Int64(gb * 1_000_000_000),
            details: "\(details) / estimated before download"
        )
    }
}
