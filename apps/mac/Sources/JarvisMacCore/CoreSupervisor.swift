import Foundation
import Darwin

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
        environment: [String: String],
        standardInput: Data?
    ) async throws -> any JarvisCoreProcess
}

public struct FoundationJarvisCoreProcessLauncher: JarvisCoreProcessLaunching {
    public var standardInputWriteTimeoutNanoseconds: UInt64
    private let standardInputWriter: @Sendable (FileHandle, Data) -> Bool

    public init(
        standardInputWriteTimeoutNanoseconds: UInt64 = 2_000_000_000,
        standardInputWriter: @escaping @Sendable (FileHandle, Data) -> Bool = { handle, data in
            do {
                try handle.write(contentsOf: data)
                return true
            } catch {
                return false
            }
        }
    ) {
        self.standardInputWriteTimeoutNanoseconds = standardInputWriteTimeoutNanoseconds
        self.standardInputWriter = standardInputWriter
    }

    public func launch(
        executableURL: URL,
        arguments: [String],
        environment: [String: String] = ProcessInfo.processInfo.environment,
        standardInput: Data? = nil
    ) async throws -> any JarvisCoreProcess {
        if let standardInput, standardInput.count > jarvisWorkspaceRootStartupEnvelopeMaximumBytes {
            throw JarvisCoreSupervisorError.startupConfigurationTooLarge
        }
        let process = Process()
        process.executableURL = executableURL
        process.arguments = arguments
        process.environment = environment
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        let inputPipe = standardInput.map { _ in Pipe() }
        process.standardInput = inputPipe
        try process.run()
        if let standardInput, let inputPipe {
            let handle = inputPipe.fileHandleForWriting
            let writer = standardInputWriter
            let writeTask = Task.detached(priority: .userInitiated) {
                writer(handle, standardInput)
            }
            enum DeliveryOutcome: Equatable, Sendable {
                case written
                case failed
                case timedOut
            }
            let timeout = standardInputWriteTimeoutNanoseconds
            let outcome = await withTaskGroup(of: DeliveryOutcome.self) { group in
                group.addTask {
                    await writeTask.value ? .written : .failed
                }
                group.addTask {
                    do {
                        try await Task.sleep(nanoseconds: timeout)
                        return .timedOut
                    } catch {
                        return .failed
                    }
                }
                let first = await group.next() ?? .failed
                if first == .timedOut {
                    _ = Darwin.kill(process.processIdentifier, SIGKILL)
                    try? handle.close()
                }
                group.cancelAll()
                return first
            }
            try? handle.close()
            guard outcome == .written else {
                if process.isRunning {
                    _ = Darwin.kill(process.processIdentifier, SIGKILL)
                }
                process.waitUntilExit()
                if outcome == .timedOut {
                    throw JarvisCoreSupervisorError.startupConfigurationWriteTimedOut
                }
                throw JarvisCoreSupervisorError.startupConfigurationWriteFailed
            }
        }
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
    private let workspaceRootProvider: any JarvisWorkspaceRootGrantProviding
    private let ipcAuthorization: JarvisIPCSessionAuthorization
    private var process: (any JarvisCoreProcess)?
    private var processMonitorTask: Task<Void, Never>?
    private var activeWorkspaceRootLease: JarvisWorkspaceRootAccessLease?
    private var activeIPCAuthorizationGeneration: UInt64?
    private var pendingTrustedWakeBootstrap: Data?
    private var pendingTrustedWakeKeyControl: Data?
    private var activeEnvironmentOverrides: [String: String]
    private var trustedWakeProvisionInFlight: Bool
    private var lifecycleOperationInProgress: Bool

    public init(
        configuration: JarvisCoreSupervisorConfiguration = JarvisCoreSupervisorConfiguration(),
        client: any JarvisCoreClient = JarvisIPCClient(),
        processLauncher: any JarvisCoreProcessLaunching = FoundationJarvisCoreProcessLauncher(),
        credentialProvider: JarvisCoreCredentialProvider = JarvisCoreCredentialProvider(),
        workspaceRootProvider: any JarvisWorkspaceRootGrantProviding = JarvisWorkspaceRootBookmarkCoordinator(),
        ipcAuthorization: JarvisIPCSessionAuthorization = JarvisIPCSessionAuthorization()
    ) {
        self.configuration = configuration
        self.client = client
        self.processLauncher = processLauncher
        self.credentialProvider = credentialProvider
        self.workspaceRootProvider = workspaceRootProvider
        self.ipcAuthorization = ipcAuthorization
        self.mode = .stopped
        self.lastHealth = nil
        self.activeWorkspaceRootLease = nil
        self.activeIPCAuthorizationGeneration = nil
        self.processMonitorTask = nil
        self.pendingTrustedWakeBootstrap = nil
        self.pendingTrustedWakeKeyControl = nil
        self.activeEnvironmentOverrides = [:]
        self.trustedWakeProvisionInFlight = false
        self.lifecycleOperationInProgress = false
    }

    deinit {
        processMonitorTask?.cancel()
        process?.terminate()
        activeWorkspaceRootLease?.release()
        if let generation = activeIPCAuthorizationGeneration {
            ipcAuthorization.clear(generation: generation)
        }
    }

    public var isAvailable: Bool {
        if case .available = mode {
            return true
        }
        return false
    }

    public var isSupervisingCoreProcess: Bool {
        process?.isRunning == true
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
        guard beginLifecycleOperation() else { return }
        defer { endLifecycleOperation() }
        await startInternal(
            environmentOverrides: environmentOverrides,
            requireMatchingConfiguration: requireMatchingConfiguration,
            skipExistingHealthCheck: false
        )
    }

    private func startInternal(
        environmentOverrides: [String: String],
        requireMatchingConfiguration: Bool,
        skipExistingHealthCheck: Bool
    ) async {
        mode = .starting

        if !skipExistingHealthCheck, await refreshHealth() {
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
        activeWorkspaceRootLease?.release()
        activeWorkspaceRootLease = nil
        if let generation = activeIPCAuthorizationGeneration {
            ipcAuthorization.clear(generation: generation)
            activeIPCAuthorizationGeneration = nil
        }

        guard let executableURL = configuration.executableURL else {
            mode = .degraded(reason: "jarvis core executable is not bundled or configured")
            return
        }

        var launchAuthorization: JarvisIPCLaunchAuthorization?
        do {
            try createDatabaseDirectoryIfNeeded()
            var environment = credentialProvider.launchEnvironment(base: ProcessInfo.processInfo.environment)
            environment.merge(environmentOverrides) { _, override in override }
            environment.removeValue(forKey: "JARVIS_IPC_TOKEN_FILE")
            environment.removeValue(forKey: JarvisIPCCLIHandoffConfiguration.enableEnvironmentKey)
            environment.removeValue(forKey: JarvisIPCCLIHandoffConfiguration.fileEnvironmentKey)
            let trustedWakeBootstrap = pendingTrustedWakeBootstrap
            let trustedWakeKeyControl = pendingTrustedWakeKeyControl
            var launchArguments = configuration.launchArguments
            if ipcAuthorization.mode == .appSupervised {
                guard JarvisIPCSessionAuthorization.isStrictLoopbackEndpoint(configuration.endpoint.baseURL),
                      let bindURL = URL(string: "http://\(configuration.bindAddress)"),
                      JarvisIPCSessionAuthorization.isStrictLoopbackEndpoint(bindURL) else {
                    throw JarvisIPCAuthorizationError.nonLoopbackEndpoint
                }
            }
            let workspaceRootLease = try workspaceRootProvider.acquireForCoreLaunch()
            launchAuthorization = try ipcAuthorization.rotateForLaunch()
            let startupInput: Data
            do {
                startupInput = try Self.startupConfigurationEnvelope(
                    roots: workspaceRootLease.roots,
                    trustedWakeBootstrap: trustedWakeBootstrap,
                    trustedWakeKeyControl: trustedWakeKeyControl,
                    ipcAuthorization: launchAuthorization
                )
            } catch {
                workspaceRootLease.release()
                if let generation = launchAuthorization?.generation {
                    ipcAuthorization.clear(generation: generation)
                }
                throw error
            }
            if !workspaceRootLease.roots.isEmpty
                || trustedWakeBootstrap != nil
                || trustedWakeKeyControl != nil
                || launchAuthorization != nil
            {
                launchArguments.append("--startup-config-stdin")
            }
            process = try await processLauncher.launch(
                executableURL: executableURL,
                arguments: launchArguments,
                environment: environment,
                standardInput: startupInput.isEmpty ? nil : startupInput
            )
            activeWorkspaceRootLease = workspaceRootLease
            activeIPCAuthorizationGeneration = launchAuthorization?.generation
            try await waitUntilHealthy(
                environmentOverrides: environmentOverrides,
                requireMatchingConfiguration: requireMatchingConfiguration
            )
            activeEnvironmentOverrides = environmentOverrides
            mode = .available
            if let process {
                startProcessMonitor(for: process)
            }
        } catch {
            processMonitorTask?.cancel()
            processMonitorTask = nil
            process?.terminate()
            process = nil
            activeWorkspaceRootLease?.release()
            activeWorkspaceRootLease = nil
            if let generation = launchAuthorization?.generation ?? activeIPCAuthorizationGeneration {
                ipcAuthorization.clear(generation: generation)
            }
            activeIPCAuthorizationGeneration = nil
            mode = .degraded(reason: String(describing: error))
        }
    }

    public func provisionTrustedWake(
        using provider: any TrustedWakeBootstrapProviding = TrustedWakeBootstrapProvider()
    ) async throws {
        guard !trustedWakeProvisionInFlight else {
            throw JarvisCoreSupervisorError.trustedWakeProvisionInProgress
        }
        trustedWakeProvisionInFlight = true
        defer { trustedWakeProvisionInFlight = false }

        guard isAvailable, let originalProcess = process, originalProcess.isRunning else {
            throw JarvisCoreSupervisorError.trustedWakeCoreNotAppSupervised
        }
        let environmentOverrides = activeEnvironmentOverrides

        let bootstrap: Data?
        do {
            bootstrap = try await Task.detached(priority: .userInitiated) {
                try provider.bootstrapData()
            }.value
        } catch {
            throw JarvisCoreSupervisorError.trustedWakeBootstrapPreparationFailed
        }
        guard let bootstrap, !bootstrap.isEmpty else {
            throw JarvisCoreSupervisorError.trustedWakeBootstrapUnavailable
        }
        guard bootstrap.count <= jarvisTrustedWakeStartupDocumentMaximumBytes else {
            throw JarvisCoreSupervisorError.trustedWakeBootstrapTooLarge
        }
        guard isAvailable,
              let currentProcess = process,
              currentProcess === originalProcess,
              originalProcess.isRunning else {
            throw JarvisCoreSupervisorError.trustedWakeCoreChangedDuringPreparation
        }
        guard beginLifecycleOperation() else {
            throw JarvisCoreSupervisorError.trustedWakeLifecycleBusy
        }
        defer { endLifecycleOperation() }
        guard isAvailable,
              let lockedProcess = process,
              lockedProcess === originalProcess,
              originalProcess.isRunning else {
            throw JarvisCoreSupervisorError.trustedWakeCoreChangedDuringPreparation
        }

        guard await stopInternal() else {
            throw JarvisCoreSupervisorError.trustedWakeStopFailed
        }

        pendingTrustedWakeBootstrap = bootstrap
        defer { pendingTrustedWakeBootstrap = nil }
        await startInternal(
            environmentOverrides: environmentOverrides,
            requireMatchingConfiguration: false,
            skipExistingHealthCheck: true
        )
        guard isAvailable else {
            throw JarvisCoreSupervisorError.trustedWakeRestartFailed
        }
    }

    public func installTrustedWakeKeyControl(
        using provider: any TrustedWakeKeyControlInstallProviding = TrustedWakeKeyRing()
    ) async throws {
        guard !trustedWakeProvisionInFlight else {
            throw JarvisCoreSupervisorError.trustedWakeProvisionInProgress
        }
        trustedWakeProvisionInFlight = true
        defer { trustedWakeProvisionInFlight = false }

        guard isAvailable, let originalProcess = process, originalProcess.isRunning else {
            throw JarvisCoreSupervisorError.trustedWakeCoreNotAppSupervised
        }
        let environmentOverrides = activeEnvironmentOverrides
        let minimumValiditySeconds = configuration.startupTimeoutSeconds + 5
        let document: Data?
        do {
            document = try await Task.detached(priority: .userInitiated) {
                try provider.installData(
                    minimumValiditySeconds: minimumValiditySeconds
                )
            }.value
        } catch {
            throw JarvisCoreSupervisorError.trustedWakeKeyControlPreparationFailed
        }
        guard let document, !document.isEmpty else {
            throw JarvisCoreSupervisorError.trustedWakeKeyControlUnavailable
        }
        guard document.count <= jarvisTrustedWakeStartupDocumentMaximumBytes else {
            throw JarvisCoreSupervisorError.trustedWakeBootstrapTooLarge
        }
        guard isAvailable,
              let currentProcess = process,
              currentProcess === originalProcess,
              originalProcess.isRunning else {
            throw JarvisCoreSupervisorError.trustedWakeCoreChangedDuringPreparation
        }
        guard beginLifecycleOperation() else {
            throw JarvisCoreSupervisorError.trustedWakeLifecycleBusy
        }
        defer { endLifecycleOperation() }
        guard isAvailable,
              let lockedProcess = process,
              lockedProcess === originalProcess,
              originalProcess.isRunning else {
            throw JarvisCoreSupervisorError.trustedWakeCoreChangedDuringPreparation
        }
        guard await stopInternal() else {
            throw JarvisCoreSupervisorError.trustedWakeStopFailed
        }

        pendingTrustedWakeKeyControl = document
        defer { pendingTrustedWakeKeyControl = nil }
        await startInternal(
            environmentOverrides: environmentOverrides,
            requireMatchingConfiguration: false,
            skipExistingHealthCheck: true
        )
        guard isAvailable else {
            throw JarvisCoreSupervisorError.trustedWakeRestartFailed
        }
    }

    @discardableResult
    public func stop() async -> Bool {
        guard beginLifecycleOperation() else { return false }
        defer { endLifecycleOperation() }
        return await stopInternal()
    }

    private func beginLifecycleOperation() -> Bool {
        guard !lifecycleOperationInProgress else { return false }
        lifecycleOperationInProgress = true
        return true
    }

    private func endLifecycleOperation() {
        lifecycleOperationInProgress = false
    }

    private func stopInternal() async -> Bool {
        processMonitorTask?.cancel()
        processMonitorTask = nil
        let stoppingProcess = process
        stoppingProcess?.terminate()
        if let stoppingProcess {
            let deadline = Date().addingTimeInterval(configuration.startupTimeoutSeconds)
            while stoppingProcess.isRunning, Date() < deadline {
                try? await Task.sleep(nanoseconds: configuration.healthPollIntervalNanoseconds)
            }
            if stoppingProcess.isRunning {
                if process === stoppingProcess {
                    mode = .degraded(reason: "jarvis core did not exit before the shutdown timeout")
                }
                return false
            }
        }
        guard process === stoppingProcess else { return false }
        process = nil
        activeWorkspaceRootLease?.release()
        activeWorkspaceRootLease = nil
        if let generation = activeIPCAuthorizationGeneration {
            ipcAuthorization.clear(generation: generation)
            activeIPCAuthorizationGeneration = nil
        }
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
            if let process, !process.isRunning {
                handleUnexpectedProcessExit(process)
            } else if process?.isRunning == true {
                mode = .degraded(reason: "jarvis core is running but health is unavailable: \(error)")
            }
            return false
        }
    }

    private func startProcessMonitor(for monitoredProcess: any JarvisCoreProcess) {
        processMonitorTask?.cancel()
        let interval = configuration.healthPollIntervalNanoseconds
        processMonitorTask = Task { @MainActor [weak self] in
            while !Task.isCancelled, monitoredProcess.isRunning {
                try? await Task.sleep(nanoseconds: interval)
            }
            guard !Task.isCancelled else { return }
            self?.handleUnexpectedProcessExit(monitoredProcess)
        }
    }

    private func handleUnexpectedProcessExit(_ exitedProcess: any JarvisCoreProcess) {
        guard process === exitedProcess else { return }
        processMonitorTask?.cancel()
        processMonitorTask = nil
        process = nil
        activeWorkspaceRootLease?.release()
        activeWorkspaceRootLease = nil
        if let generation = activeIPCAuthorizationGeneration {
            ipcAuthorization.clear(generation: generation)
            activeIPCAuthorizationGeneration = nil
        }
        lastHealth = nil
        mode = .degraded(reason: "the app-supervised Jarvis core exited unexpectedly")
    }

    private func waitUntilHealthy(
        environmentOverrides: [String: String],
        requireMatchingConfiguration: Bool
    ) async throws {
        let deadline = Date().addingTimeInterval(configuration.startupTimeoutSeconds)
        repeat {
            guard process?.isRunning == true else {
                throw JarvisCoreSupervisorError.launchedProcessExited
            }
            if await refreshHealth(),
               process?.isRunning == true,
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

    private static func startupConfigurationEnvelope(
        roots: [JarvisWorkspaceRootLaunchRoot],
        trustedWakeBootstrap: Data?,
        trustedWakeKeyControl: Data?,
        ipcAuthorization: JarvisIPCLaunchAuthorization?
    ) throws -> Data {
        guard trustedWakeBootstrap == nil || trustedWakeKeyControl == nil else {
            throw JarvisCoreSupervisorError.startupConfigurationInvalid
        }
        guard !roots.isEmpty
            || trustedWakeBootstrap != nil
            || trustedWakeKeyControl != nil
            || ipcAuthorization != nil else {
            return Data()
        }

        var envelope: [String: Any] = [
            "version": 1,
            "workspace_roots": roots.map { ["id": $0.id, "path": $0.path] }
        ]
        if let document = trustedWakeBootstrap ?? trustedWakeKeyControl {
            guard document.count <= jarvisTrustedWakeStartupDocumentMaximumBytes else {
                throw JarvisCoreSupervisorError.trustedWakeBootstrapTooLarge
            }
            let value: Any
            do {
                value = try JSONSerialization.jsonObject(with: document, options: [.fragmentsAllowed])
            } catch {
                throw JarvisCoreSupervisorError.startupConfigurationInvalid
            }
            envelope["trusted_wake"] = [
                "kind": trustedWakeBootstrap == nil ? "key_control" : "bootstrap",
                "document": value
            ]
        }
        if let ipcAuthorization {
            envelope["ipc_auth"] = [
                "scheme": "bearer",
                "token": ipcAuthorization.token,
                "generation": ipcAuthorization.generation
            ]
        }
        let data = try JSONSerialization.data(withJSONObject: envelope, options: [.sortedKeys])
        guard data.count <= jarvisWorkspaceRootStartupEnvelopeMaximumBytes else {
            throw JarvisCoreSupervisorError.startupConfigurationTooLarge
        }
        return data
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
    case launchedProcessExited
    case startupConfigurationInvalid
    case startupConfigurationTooLarge
    case startupConfigurationWriteFailed
    case startupConfigurationWriteTimedOut
    case trustedWakeCoreNotAppSupervised
    case trustedWakeProvisionInProgress
    case trustedWakeCoreChangedDuringPreparation
    case trustedWakeLifecycleBusy
    case trustedWakeBootstrapPreparationFailed
    case trustedWakeBootstrapUnavailable
    case trustedWakeBootstrapTooLarge
    case trustedWakeKeyControlPreparationFailed
    case trustedWakeKeyControlUnavailable
    case trustedWakeStopFailed
    case trustedWakeRestartFailed
}
