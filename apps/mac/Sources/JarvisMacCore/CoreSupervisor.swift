import Foundation

public enum JarvisCoreMode: Equatable, Sendable {
    case stopped
    case starting
    case available
    case degraded(reason: String)
}

public struct JarvisCoreSupervisorSmokeSnapshot: Equatable, Sendable {
    public var mode: JarvisCoreMode
    public var endpoint: URL
    public var executablePath: String?
    public var databasePath: String?
    public var launchArguments: [String]
    public var lastHealthStatus: String?
    public var lastHealthRuntime: String?

    public var executableConfigured: Bool {
        executablePath != nil
    }

    public var canAttemptPackagedCoreSmoke: Bool {
        executableConfigured && mode != .starting
    }

    public var summary: String {
        let executable = executablePath ?? "not configured"
        let health = lastHealthStatus.map { "\($0) / \(lastHealthRuntime ?? "unknown runtime")" } ?? "not checked"
        return "endpoint: \(endpoint.absoluteString), executable: \(executable), health: \(health)"
    }
}

public struct JarvisCoreSupervisorConfiguration: Equatable, Sendable {
    public var endpoint: JarvisEndpoint
    public var bindAddress: String
    public var executableURL: URL?
    public var databaseURL: URL?
    public var serveCommand: [String]
    public var startupTimeoutSeconds: Double
    public var healthPollIntervalNanoseconds: UInt64

    public init(
        endpoint: JarvisEndpoint? = nil,
        bindAddress: String? = nil,
        executableURL: URL? = JarvisCoreSupervisorConfiguration.defaultExecutableURL(),
        databaseURL: URL? = JarvisCoreSupervisorConfiguration.defaultDatabaseURL(),
        serveCommand: [String] = ["serve"],
        startupTimeoutSeconds: Double = 4,
        healthPollIntervalNanoseconds: UInt64 = 200_000_000,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) {
        let resolvedBindAddress = bindAddress ?? Self.defaultBindAddress(environment: environment)
        self.endpoint = endpoint ?? Self.defaultEndpoint(
            bindAddress: resolvedBindAddress,
            environment: environment
        )
        self.bindAddress = resolvedBindAddress
        self.executableURL = executableURL
        self.databaseURL = databaseURL
        self.serveCommand = serveCommand
        self.startupTimeoutSeconds = startupTimeoutSeconds
        self.healthPollIntervalNanoseconds = healthPollIntervalNanoseconds
    }

    public var launchArguments: [String] {
        var arguments = serveCommand
        arguments.append(contentsOf: ["--bind", bindAddress])
        if let databaseURL {
            arguments.append(contentsOf: ["--db-path", databaseURL.path])
        }
        return arguments
    }

    public static func defaultExecutableURL(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        bundle: Bundle = .main,
        fileManager: FileManager = .default
    ) -> URL? {
        if let configuredURL = configuredExecutableURL(environment: environment) {
            return configuredURL
        }

        if let bundledURL = bundledExecutableURL(bundle: bundle, fileManager: fileManager) {
            return bundledURL
        }

        return developmentExecutableURL(fileManager: fileManager)
    }

    static func configuredExecutableURL(environment: [String: String]) -> URL? {
        for key in ["JARVIS_MAC_CORE_EXECUTABLE", "JARVIS_CORE_EXECUTABLE", "JARVIS_CLI_EXECUTABLE"] {
            if let value = environment[key], !value.isEmpty {
                return URL(fileURLWithPath: value)
            }
        }

        return nil
    }

    static func bundledExecutableURL(
        bundle: Bundle,
        fileManager: FileManager = .default
    ) -> URL? {
        var roots: [URL] = []

        if let resourceURL = bundle.resourceURL {
            roots.append(resourceURL.appending(path: "bin", directoryHint: .isDirectory))
            roots.append(resourceURL)
        }

        if let executableDirectory = bundle.executableURL?.deletingLastPathComponent() {
            roots.append(executableDirectory)
        }

        return firstExecutableURL(
            named: ["jarvis-cli", "jarvis", "jarvis-core"],
            in: roots,
            fileManager: fileManager
        )
    }

    static func developmentExecutableURL(fileManager: FileManager = .default) -> URL? {
        let current = URL(fileURLWithPath: fileManager.currentDirectoryPath)
        let candidates = [
            current.appending(path: "target/debug/jarvis-cli"),
            current.appending(path: "target/debug/jarvis"),
            current.appending(path: "../../target/debug/jarvis-cli").standardizedFileURL,
            current.appending(path: "../../target/debug/jarvis").standardizedFileURL,
            current.appending(path: "../../../target/debug/jarvis-cli").standardizedFileURL,
            current.appending(path: "../../../target/debug/jarvis").standardizedFileURL
        ]
        return candidates.first { fileManager.isExecutableFile(atPath: $0.path) }
    }

    static func defaultBindAddress(environment: [String: String]) -> String {
        environment["JARVIS_MAC_CORE_BIND_ADDRESS"].flatMap { $0.isEmpty ? nil : $0 } ?? "127.0.0.1:7787"
    }

    static func defaultEndpoint(bindAddress: String, environment: [String: String]) -> JarvisEndpoint {
        if let value = environment["JARVIS_MAC_CORE_ENDPOINT"], !value.isEmpty,
           let url = URL(string: value)
        {
            return JarvisEndpoint(baseURL: url)
        }

        return JarvisEndpoint(baseURL: URL(string: "http://\(bindAddress)")!)
    }

    static func firstExecutableURL(
        named names: [String],
        in roots: [URL],
        fileManager: FileManager = .default
    ) -> URL? {
        for root in roots {
            for name in names {
                let candidate = root.appending(path: name)
                if fileManager.isExecutableFile(atPath: candidate.path) {
                    return candidate
                }
            }
        }

        return nil
    }

    public static func defaultDatabaseURL(fileManager: FileManager = .default) -> URL? {
        if let configuredPath = ProcessInfo.processInfo.environment["JARVIS_MAC_CORE_DATABASE"], !configuredPath.isEmpty {
            return URL(fileURLWithPath: configuredPath)
        }

        guard let support = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            return nil
        }

        return support
            .appending(path: "Jarvis", directoryHint: .isDirectory)
            .appending(path: "jarvis.sqlite")
    }
}

public protocol JarvisCoreProcess: AnyObject, Sendable {
    var isRunning: Bool { get }
    func terminate()
}

public protocol JarvisCoreProcessLaunching: Sendable {
    func launch(
        executableURL: URL,
        arguments: [String],
        environment: [String: String]
    ) throws -> any JarvisCoreProcess
}

public struct FoundationJarvisCoreProcessLauncher: JarvisCoreProcessLaunching {
    public init() {}

    public func launch(
        executableURL: URL,
        arguments: [String],
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) throws -> any JarvisCoreProcess {
        let process = Process()
        process.executableURL = executableURL
        process.arguments = arguments
        process.environment = environment
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        try process.run()
        return FoundationJarvisCoreProcess(process: process)
    }
}

private final class FoundationJarvisCoreProcess: JarvisCoreProcess, @unchecked Sendable {
    private let process: Process

    init(process: Process) {
        self.process = process
    }

    var isRunning: Bool {
        process.isRunning
    }

    func terminate() {
        guard process.isRunning else { return }
        process.terminate()
    }
}

@MainActor
public final class JarvisCoreSupervisor: ObservableObject {
    @Published public private(set) var mode: JarvisCoreMode
    @Published public private(set) var lastHealth: JarvisHealth?

    public let configuration: JarvisCoreSupervisorConfiguration
    private let client: any JarvisCoreClient
    private let processLauncher: any JarvisCoreProcessLaunching
    private let credentialProvider: JarvisCoreCredentialProvider
    private var process: (any JarvisCoreProcess)?

    public init(
        configuration: JarvisCoreSupervisorConfiguration = JarvisCoreSupervisorConfiguration(),
        client: any JarvisCoreClient = JarvisIPCClient(),
        processLauncher: any JarvisCoreProcessLaunching = FoundationJarvisCoreProcessLauncher(),
        credentialProvider: JarvisCoreCredentialProvider = JarvisCoreCredentialProvider()
    ) {
        self.configuration = configuration
        self.client = client
        self.processLauncher = processLauncher
        self.credentialProvider = credentialProvider
        self.mode = .stopped
        self.lastHealth = nil
    }

    deinit {
        process?.terminate()
    }

    public var isAvailable: Bool {
        if case .available = mode {
            return true
        }
        return false
    }

    public var smokeSnapshot: JarvisCoreSupervisorSmokeSnapshot {
        JarvisCoreSupervisorSmokeSnapshot(
            mode: mode,
            endpoint: configuration.endpoint.baseURL,
            executablePath: configuration.executableURL?.path,
            databasePath: configuration.databaseURL?.path,
            launchArguments: configuration.launchArguments,
            lastHealthStatus: lastHealth?.status,
            lastHealthRuntime: lastHealth?.commandRuntime
        )
    }

    public func start(
        environmentOverrides: [String: String] = [:],
        requireMatchingConfiguration: Bool = false
    ) async {
        mode = .starting

        if await refreshHealth() {
            if requireMatchingConfiguration,
               let lastHealth,
               !Self.health(lastHealth, matches: environmentOverrides)
            {
                mode = .degraded(
                    reason: "A different Jarvis core is already running at \(configuration.endpoint.baseURL.absoluteString). Stop that process before restarting with the selected model."
                )
            } else {
                mode = .available
            }
            return
        }

        if process?.isRunning == true {
            mode = .degraded(
                reason: "The previous app-supervised Jarvis core is still running but unavailable. Wait for it to exit before starting another core."
            )
            return
        }

        guard let executableURL = configuration.executableURL else {
            mode = .degraded(reason: "jarvis core executable is not bundled or configured")
            return
        }

        do {
            try createDatabaseDirectoryIfNeeded()
            var environment = credentialProvider.launchEnvironment(base: ProcessInfo.processInfo.environment)
            environment.merge(environmentOverrides) { _, override in override }
            process = try processLauncher.launch(
                executableURL: executableURL,
                arguments: configuration.launchArguments,
                environment: environment
            )
            try await waitUntilHealthy(
                environmentOverrides: environmentOverrides,
                requireMatchingConfiguration: requireMatchingConfiguration
            )
            mode = .available
        } catch {
            process?.terminate()
            process = nil
            mode = .degraded(reason: String(describing: error))
        }
    }

    @discardableResult
    public func stop() async -> Bool {
        let stoppingProcess = process
        stoppingProcess?.terminate()
        if let stoppingProcess {
            let deadline = Date().addingTimeInterval(configuration.startupTimeoutSeconds)
            while stoppingProcess.isRunning, Date() < deadline {
                try? await Task.sleep(nanoseconds: configuration.healthPollIntervalNanoseconds)
            }
            if stoppingProcess.isRunning {
                mode = .degraded(reason: "jarvis core did not exit before the shutdown timeout")
                return false
            }
        }
        process = nil
        lastHealth = nil
        mode = .stopped
        return true
    }

    @discardableResult
    public func refreshHealth() async -> Bool {
        do {
            lastHealth = try await client.health()
            mode = .available
            return true
        } catch {
            lastHealth = nil
            if process?.isRunning == true {
                mode = .degraded(reason: "jarvis core is running but health is unavailable: \(error)")
            }
            return false
        }
    }

    private func waitUntilHealthy(
        environmentOverrides: [String: String],
        requireMatchingConfiguration: Bool
    ) async throws {
        let deadline = Date().addingTimeInterval(configuration.startupTimeoutSeconds)
        repeat {
            if await refreshHealth(),
               !requireMatchingConfiguration || lastHealth.map({ Self.health($0, matches: environmentOverrides) }) == true
            {
                return
            }
            try await Task.sleep(nanoseconds: configuration.healthPollIntervalNanoseconds)
        } while Date() < deadline

        throw JarvisCoreSupervisorError.healthCheckTimedOut
    }

    private func createDatabaseDirectoryIfNeeded() throws {
        guard let databaseURL = configuration.databaseURL else { return }
        let directory = databaseURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    private static func health(_ health: JarvisHealth, matches environment: [String: String]) -> Bool {
        if environment["JARVIS_CHATGPT_ENABLED"] == "true" {
            let expectedModel = environment["JARVIS_CHATGPT_MODEL"]?.trimmingCharacters(in: .whitespacesAndNewlines)
            let expectedAuthMode = (environment["JARVIS_CHATGPT_AUTH"] ?? environment["JARVIS_CHATGPT_AUTH_MODE"])?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return health.chatgptEnabled
                && health.chatgptRequiresApproval
                && (expectedAuthMode?.isEmpty != false || health.chatgptAuthMode == expectedAuthMode)
                && (expectedModel?.isEmpty != false || health.chatgptModel == expectedModel)
        }

        if environment["JARVIS_CHATGPT_ENABLED"] == "false", health.chatgptEnabled {
            return false
        }

        if let expectedProvider = environment["JARVIS_LOCAL_MODEL_PROVIDER"]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !expectedProvider.isEmpty,
           health.localModelProvider != expectedProvider
        {
            return false
        }

        if let expectedModel = environment["JARVIS_LOCAL_MODEL"]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !expectedModel.isEmpty,
           health.localModel != expectedModel
        {
            return false
        }

        return true
    }
}

public enum JarvisCoreSupervisorError: Error, Equatable {
    case healthCheckTimedOut
}
