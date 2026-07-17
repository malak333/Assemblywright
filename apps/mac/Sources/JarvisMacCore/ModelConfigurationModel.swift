import Foundation

public enum JarvisModelProviderSelection: String, CaseIterable, Identifiable, Sendable {
    case codexAccount
    case codex
    case fake
    case ollama

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .codexAccount:
            return "Codex account"
        case .codex:
            return "OpenAI API"
        case .fake:
            return "Fake local"
        case .ollama:
            return "Ollama"
        }
    }
}

public enum JarvisReasoningEffort: String, CaseIterable, Identifiable, Sendable {
    case low
    case medium
    case high
    case xhigh
    case max
    case ultra

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .low: return "Light"
        case .medium: return "Medium"
        case .high: return "High"
        case .xhigh: return "Extra High"
        case .max: return "Max"
        case .ultra: return "Ultra"
        }
    }
}

public struct JarvisModelConfiguration: Equatable, Sendable {
    public var provider: JarvisModelProviderSelection
    public var localModel: String
    public var ollamaBaseURL: String
    public var codexModel: String
    public var codexBaseURL: String
    public var codexExecutable: String
    public var reasoningEffort: JarvisReasoningEffort
    public var requiresCloudPromptApproval: Bool
    public var timeoutMilliseconds: String

    public init(
        provider: JarvisModelProviderSelection = .fake,
        localModel: String = "fake-local-model",
        ollamaBaseURL: String = "http://127.0.0.1:11434",
        codexModel: String = "gpt-4.1-mini",
        codexBaseURL: String = "https://api.openai.com/v1",
        codexExecutable: String = JarvisModelConfiguration.defaultCodexExecutable(),
        reasoningEffort: JarvisReasoningEffort = .medium,
        requiresCloudPromptApproval: Bool = false,
        timeoutMilliseconds: String = "60000"
    ) {
        self.provider = provider
        self.localModel = localModel
        self.ollamaBaseURL = ollamaBaseURL
        self.codexModel = codexModel
        self.codexBaseURL = codexBaseURL
        self.codexExecutable = codexExecutable
        self.reasoningEffort = reasoningEffort
        self.requiresCloudPromptApproval = requiresCloudPromptApproval
        self.timeoutMilliseconds = timeoutMilliseconds
    }

    public static func fromEnvironment(_ environment: [String: String] = ProcessInfo.processInfo.environment) -> Self {
        let rawProvider = environment["JARVIS_LOCAL_MODEL_PROVIDER"]?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let chatGPTAuth = (environment["JARVIS_CHATGPT_AUTH"] ?? environment["JARVIS_CHATGPT_AUTH_MODE"])?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        let provider = JarvisModelProviderSelection(
            rawValue: rawProvider
        ) ?? (environment["JARVIS_CHATGPT_ENABLED"] == "true"
            ? (chatGPTAuth == "codex_account" ? .codexAccount : .codex)
            : .fake)
        let model = environment["JARVIS_LOCAL_MODEL"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        let baseURL = environment["JARVIS_OLLAMA_BASE_URL"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        let codexModel = environment["JARVIS_CHATGPT_MODEL"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        let codexBaseURL = (environment["JARVIS_OPENAI_BASE_URL"] ?? environment["JARVIS_CHATGPT_BASE_URL"])?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let codexExecutable = environment["JARVIS_CODEX_EXECUTABLE"]?.trimmingCharacters(in: .whitespacesAndNewlines)
        let reasoningEffort = environment["JARVIS_CHATGPT_REASONING_EFFORT"]
            .flatMap { JarvisReasoningEffort(rawValue: $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()) }
            ?? .medium
        let requiresCloudPromptApproval = environment["JARVIS_CHATGPT_REQUIRES_APPROVAL"]
            .flatMap(Bool.init) ?? false
        let timeout = (provider == .codex || provider == .codexAccount
            ? environment["JARVIS_CHATGPT_TIMEOUT_MS"]
            : environment["JARVIS_LOCAL_MODEL_TIMEOUT_MS"])?
            .trimmingCharacters(in: .whitespacesAndNewlines)

        return JarvisModelConfiguration(
            provider: provider,
            localModel: model?.isEmpty == false ? model! : (provider == .ollama ? "llama3.2" : "fake-local-model"),
            ollamaBaseURL: baseURL?.isEmpty == false ? baseURL! : "http://127.0.0.1:11434",
            codexModel: codexModel?.isEmpty == false ? codexModel! : (provider == .codexAccount ? "gpt-5.6-sol" : "gpt-4.1-mini"),
            codexBaseURL: codexBaseURL?.isEmpty == false ? codexBaseURL! : "https://api.openai.com/v1",
            codexExecutable: codexExecutable?.isEmpty == false ? codexExecutable! : defaultCodexExecutable(),
            reasoningEffort: reasoningEffort,
            requiresCloudPromptApproval: requiresCloudPromptApproval,
            timeoutMilliseconds: timeout?.isEmpty == false ? timeout! : "60000"
        )
    }

    public var launchEnvironmentOverrides: [String: String] {
        switch provider {
        case .codexAccount:
            return [
                "JARVIS_LOCAL_MODEL_ENABLED": "false",
                "JARVIS_CHATGPT_ENABLED": "true",
                "JARVIS_CHATGPT_AUTH": "codex_account",
                "JARVIS_CHATGPT_MODEL": sanitizedCodexModel,
                "JARVIS_CODEX_EXECUTABLE": sanitizedCodexExecutable,
                "JARVIS_CHATGPT_TIMEOUT_MS": sanitizedTimeoutMilliseconds,
                "JARVIS_CHATGPT_REQUIRES_APPROVAL": requiresCloudPromptApproval ? "true" : "false",
                "JARVIS_CHATGPT_REASONING_EFFORT": reasoningEffort.rawValue
            ]
        case .codex:
            return [
                "JARVIS_LOCAL_MODEL_ENABLED": "false",
                "JARVIS_CHATGPT_ENABLED": "true",
                "JARVIS_CHATGPT_AUTH": "api_key",
                "JARVIS_CHATGPT_MODEL": sanitizedCodexModel,
                "JARVIS_OPENAI_BASE_URL": sanitizedCodexBaseURL,
                "JARVIS_CHATGPT_TIMEOUT_MS": sanitizedTimeoutMilliseconds,
                "JARVIS_CHATGPT_REQUIRES_APPROVAL": requiresCloudPromptApproval ? "true" : "false",
                "JARVIS_CHATGPT_REASONING_EFFORT": reasoningEffort.rawValue
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
        return trimmed.isEmpty ? (provider == .codexAccount ? "gpt-5.6-sol" : "gpt-4.1-mini") : trimmed
    }

    public static let codexAccountModels = [
        "gpt-5.6-sol",
        "gpt-5.6-terra",
        "gpt-5.6-luna",
        "gpt-5.5",
        "gpt-5.4",
        "gpt-5.4-mini",
        "gpt-5.3-codex-spark"
    ]

    public var supportedReasoningEfforts: [JarvisReasoningEffort] {
        switch sanitizedCodexModel {
        case "gpt-5.6-sol", "gpt-5.6-terra":
            return [.low, .medium, .high, .xhigh, .max, .ultra]
        case "gpt-5.6-luna":
            return [.low, .medium, .high, .xhigh, .max]
        default:
            return [.low, .medium, .high, .xhigh]
        }
    }

    public var sanitizedCodexBaseURL: String {
        let trimmed = codexBaseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "https://api.openai.com/v1" : trimmed.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }

    public var sanitizedCodexExecutable: String {
        let trimmed = codexExecutable.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? Self.defaultCodexExecutable() : trimmed
    }

    public static func defaultCodexExecutable(fileManager: FileManager = .default) -> String {
        let candidates = [
            "/Applications/ChatGPT.app/Contents/Resources/codex",
            "/Applications/Codex.app/Contents/Resources/codex"
        ]
        return candidates.first(where: fileManager.isExecutableFile(atPath:)) ?? "codex"
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

public struct JarvisCommandResult: Equatable, Sendable {
    public var exitCode: Int32
    public var output: String

    public init(exitCode: Int32, output: String) {
        self.exitCode = exitCode
        self.output = output
    }
}

public protocol JarvisCommandRunning: Sendable {
    func run(
        executableURL: URL,
        arguments: [String],
        environment: [String: String]
    ) async throws -> JarvisCommandResult
}

public struct FoundationJarvisCommandRunner: JarvisCommandRunning {
    private static let maximumCapturedOutputBytes = 64 * 1024

    public init() {}

    public func run(
        executableURL: URL,
        arguments: [String],
        environment: [String: String]
    ) async throws -> JarvisCommandResult {
        try await Task.detached(priority: .userInitiated) {
            let process = Process()
            let outputPipe = Pipe()
            process.executableURL = executableURL
            process.arguments = arguments
            process.environment = environment
            process.standardOutput = outputPipe
            process.standardError = outputPipe

            try process.run()
            try? outputPipe.fileHandleForWriting.close()
            let rawOutput = outputPipe.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()

            let wasTruncated = rawOutput.count > Self.maximumCapturedOutputBytes
            let boundedOutput = rawOutput.prefix(Self.maximumCapturedOutputBytes)
            var output = String(decoding: boundedOutput, as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if wasTruncated {
                output += output.isEmpty ? "Output truncated." : "\nOutput truncated."
            }
            return JarvisCommandResult(exitCode: process.terminationStatus, output: output)
        }.value
    }
}

public struct JarvisOllamaUpgradeResult: Equatable, Sendable {
    public var previousVersion: String
    public var installedVersion: String
    public var serviceRestarted: Bool

    public init(previousVersion: String, installedVersion: String, serviceRestarted: Bool) {
        self.previousVersion = previousVersion
        self.installedVersion = installedVersion
        self.serviceRestarted = serviceRestarted
    }
}

public protocol JarvisOllamaUpdating: Sendable {
    func upgradeOllama(
        progress: @escaping @Sendable (String) async -> Void
    ) async throws -> JarvisOllamaUpgradeResult
}

public struct HomebrewOllamaUpdater: JarvisOllamaUpdating {
    private let commandRunner: any JarvisCommandRunning
    private let brewCandidates: [URL]
    private let executableExists: @Sendable (String) -> Bool
    private let environment: [String: String]

    public init(
        commandRunner: any JarvisCommandRunning = FoundationJarvisCommandRunner(),
        brewCandidates: [URL] = [
            URL(fileURLWithPath: "/opt/homebrew/bin/brew"),
            URL(fileURLWithPath: "/usr/local/bin/brew")
        ],
        environment: [String: String] = ProcessInfo.processInfo.environment,
        executableExists: @escaping @Sendable (String) -> Bool = {
            FileManager.default.isExecutableFile(atPath: $0)
        }
    ) {
        self.commandRunner = commandRunner
        self.brewCandidates = brewCandidates
        self.executableExists = executableExists
        self.environment = Self.commandEnvironment(from: environment, brewCandidates: brewCandidates)
    }

    public func upgradeOllama(
        progress: @escaping @Sendable (String) async -> Void
    ) async throws -> JarvisOllamaUpgradeResult {
        guard let brewURL = brewCandidates.first(where: { executableExists($0.path) }) else {
            throw OllamaUpgradeError.homebrewUnavailable
        }

        await progress("Checking the Homebrew Ollama installation…")
        let before = try await commandRunner.run(
            executableURL: brewURL,
            arguments: ["list", "--formula", "--versions", "ollama"],
            environment: environment
        )
        guard before.exitCode == 0,
              let previousVersion = Self.ollamaVersion(from: before.output)
        else {
            throw OllamaUpgradeError.notHomebrewManaged
        }

        let services = try await checkedRun(
            brewURL,
            arguments: ["services", "list"],
            action: "inspect the Ollama service"
        )
        let serviceWasRunning = Self.ollamaServiceIsRunning(in: services.output)

        await progress("Upgrading Ollama with Homebrew…")
        _ = try await checkedRun(
            brewURL,
            arguments: ["upgrade", "ollama"],
            action: "upgrade Ollama"
        )

        let after = try await checkedRun(
            brewURL,
            arguments: ["list", "--formula", "--versions", "ollama"],
            action: "verify the upgraded Ollama formula"
        )
        guard let installedVersion = Self.ollamaVersion(from: after.output) else {
            throw OllamaUpgradeError.versionUnavailableAfterUpgrade
        }

        if serviceWasRunning {
            await progress("Restarting the Ollama Homebrew service…")
            _ = try await checkedRun(
                brewURL,
                arguments: ["services", "restart", "ollama"],
                action: "restart the Ollama service"
            )
        }

        return JarvisOllamaUpgradeResult(
            previousVersion: previousVersion,
            installedVersion: installedVersion,
            serviceRestarted: serviceWasRunning
        )
    }

    private func checkedRun(
        _ executableURL: URL,
        arguments: [String],
        action: String
    ) async throws -> JarvisCommandResult {
        let result = try await commandRunner.run(
            executableURL: executableURL,
            arguments: arguments,
            environment: environment
        )
        guard result.exitCode == 0 else {
            throw OllamaUpgradeError.commandFailed(action: action, detail: result.output)
        }
        return result
    }

    private static func commandEnvironment(
        from environment: [String: String],
        brewCandidates: [URL]
    ) -> [String: String] {
        let allowedKeys = ["HOME", "USER", "LOGNAME", "TMPDIR", "LANG", "LC_ALL"]
        var filtered = environment.filter { allowedKeys.contains($0.key) }
        let brewDirectories = brewCandidates.map { $0.deletingLastPathComponent().path }
        filtered["PATH"] = (brewDirectories + ["/usr/bin", "/bin", "/usr/sbin", "/sbin"])
            .joined(separator: ":")
        filtered["HOMEBREW_NO_ANALYTICS"] = "1"
        filtered["HOMEBREW_NO_ENV_HINTS"] = "1"
        return filtered
    }

    private static func ollamaVersion(from output: String) -> String? {
        output
            .split(whereSeparator: \.isNewline)
            .first(where: { $0.split(whereSeparator: \.isWhitespace).first == "ollama" })?
            .split(whereSeparator: \.isWhitespace)
            .dropFirst()
            .first
            .map(String.init)
    }

    private static func ollamaServiceIsRunning(in output: String) -> Bool {
        for line in output.split(whereSeparator: \.isNewline) {
            let fields = line.split(whereSeparator: \.isWhitespace)
            if fields.count >= 2 && fields[0] == "ollama" && fields[1] == "started" {
                return true
            }
        }
        return false
    }
}

private enum OllamaUpgradeError: LocalizedError {
    case homebrewUnavailable
    case notHomebrewManaged
    case versionUnavailableAfterUpgrade
    case commandFailed(action: String, detail: String)

    var errorDescription: String? {
        switch self {
        case .homebrewUnavailable:
            return "Homebrew was not found in /opt/homebrew/bin or /usr/local/bin. Update Ollama using its original installer."
        case .notHomebrewManaged:
            return "Ollama is not installed as a Homebrew formula. Update Ollama using its original installer."
        case .versionUnavailableAfterUpgrade:
            return "Homebrew finished, but the installed Ollama version could not be verified."
        case let .commandFailed(action, detail):
            let suffix = detail.isEmpty ? "" : " \(detail)"
            return "Homebrew could not \(action).\(suffix)"
        }
    }
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
        let normalized = Self.normalized(message)
        if normalized.localizedCaseInsensitiveContains("requires a newer version of Ollama") {
            return "Update Ollama before retrying. \(normalized)"
        }
        return normalized
    }

    private static func normalized(_ message: String) -> String {
        message
            .replacingOccurrences(of: "\\n", with: "\n")
            .split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: " ")
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
    @Published public private(set) var activeReasoningEffort: String?
    @Published public private(set) var activeCloudPromptApprovalRequired: Bool?
    @Published public private(set) var statusMessage: String?
    @Published public var codexAPIKeyEntry: String
    @Published public private(set) var hasStoredCodexCredential: Bool
    @Published public private(set) var isWorking: Bool
    @Published public private(set) var downloadProgress: JarvisOllamaPullProgress?

    private let controller: any JarvisLocalModelRuntimeControlling
    private let ollamaUpdater: any JarvisOllamaUpdating
    private let credentialStore: any JarvisCredentialStore

    public init(
        configuration: JarvisModelConfiguration = .fromEnvironment(),
        controller: any JarvisLocalModelRuntimeControlling = OllamaModelRuntimeController(),
        ollamaUpdater: any JarvisOllamaUpdating = HomebrewOllamaUpdater(),
        credentialStore: any JarvisCredentialStore = KeychainJarvisCredentialStore()
    ) {
        self.configuration = configuration
        self.controller = controller
        self.ollamaUpdater = ollamaUpdater
        self.credentialStore = credentialStore
        self.availableModels = []
        self.activeProvider = nil
        self.activeModel = nil
        self.activeReasoningEffort = nil
        self.activeCloudPromptApprovalRequired = nil
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

    public var canUpgradeLocalOllama: Bool {
        guard configuration.provider == .ollama,
              let host = URL(string: configuration.sanitizedOllamaBaseURL)?.host?.lowercased()
        else {
            return false
        }
        return host == "127.0.0.1" || host == "localhost" || host == "::1"
    }

    public func applyHealth(_ health: JarvisHealth?) {
        if health?.chatgptEnabled == true {
            activeProvider = health?.chatgptAuthMode == "codex_account" ? "codex account" : "openai api"
            activeModel = health?.chatgptModel
            activeReasoningEffort = health?.chatgptReasoningEffort
            activeCloudPromptApprovalRequired = health?.chatgptRequiresApproval
        } else {
            activeProvider = health?.localModelProvider
            activeModel = health?.localModel
            activeReasoningEffort = nil
            activeCloudPromptApprovalRequired = nil
        }
    }

    public func refreshCodexCredentialState() {
        guard configuration.provider == .codex else {
            hasStoredCodexCredential = false
            return
        }
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
        guard configuration.provider == .codex else {
            hasStoredCodexCredential = false
            return
        }
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
        await runOllamaAction(action: "downloaded and reloaded", failureAction: "download") {
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

    public func upgradeOllama() async {
        guard canUpgradeLocalOllama else {
            statusMessage = "Ollama can be upgraded here only when the configured endpoint is on this Mac."
            return
        }

        isWorking = true
        statusMessage = "Preparing to upgrade Ollama…"
        defer { isWorking = false }

        do {
            let result = try await ollamaUpdater.upgradeOllama { [weak self] progress in
                await MainActor.run {
                    self?.statusMessage = progress
                }
            }
            let versionSummary = result.previousVersion == result.installedVersion
                ? "Ollama \(result.installedVersion) is already current."
                : "Ollama upgraded from \(result.previousVersion) to \(result.installedVersion)."
            if result.serviceRestarted {
                statusMessage = "\(versionSummary) The Homebrew service restarted; retry the model download."
            } else {
                statusMessage = "\(versionSummary) Ollama was not running as a Homebrew service; restart Ollama manually before retrying the model download."
            }
        } catch {
            statusMessage = "Ollama upgrade failed: \(Self.errorMessage(error))"
        }
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

    private func runOllamaAction(
        action: String,
        failureAction: String? = nil,
        operation: () async throws -> Void
    ) async {
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
            statusMessage = "Ollama model \(failureAction ?? action) failed: \(Self.errorMessage(error))"
        }
    }

    private static func errorMessage(_ error: any Error) -> String {
        error.localizedDescription
            .replacingOccurrences(of: "\\n", with: "\n")
            .split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: " ")
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
