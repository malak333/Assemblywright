import Combine
import Darwin
import Foundation
import Security

public enum AssemblywrightDeveloperBridgeAppPhase: String, Equatable, Sendable {
    case disabled
    case starting
    case connected
    case masterOffline = "master_offline"
    case maintenance
    case paused
    case stopped
}

public struct AssemblywrightDeveloperBridgeAppStatus: Equatable, Sendable {
    public let phase: AssemblywrightDeveloperBridgeAppPhase
    public let masterEndpoint: String?
    public let connectionEpoch: UInt64?
    public let featureConveyor: AssemblywrightMacFeatureConveyorStatus?
    public let errorCode: String?

    public init(
        phase: AssemblywrightDeveloperBridgeAppPhase,
        masterEndpoint: String? = nil,
        connectionEpoch: UInt64? = nil,
        featureConveyor: AssemblywrightMacFeatureConveyorStatus? = nil,
        errorCode: String? = nil
    ) {
        self.phase = phase
        self.masterEndpoint = masterEndpoint
        self.connectionEpoch = connectionEpoch
        self.featureConveyor = featureConveyor
        self.errorCode = errorCode
    }

    public static let disabled = Self(phase: .disabled)
}

public struct AssemblywrightDeveloperBridgeProcessConfiguration: Equatable, Sendable {
    public static let executableEnvironmentKey = "ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_EXECUTABLE"
    public static let teamIdentifierEnvironmentKey =
        "ASSEMBLYWRIGHT_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER"
    public static let agentExecutableEnvironmentKey =
        "ASSEMBLYWRIGHT_MAC_DEVELOPER_AGENT_EXECUTABLE"
    public static let agentDataDirectoryEnvironmentKey =
        "ASSEMBLYWRIGHT_MAC_DEVELOPER_AGENT_DATA_DIR"
    public static let fixtureJobsEnabledEnvironmentKey =
        "ASSEMBLYWRIGHT_MAC_DEVELOPER_FIXTURE_JOBS_ENABLED"
    public static let mlxJobsEnabledEnvironmentKey =
        "ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_JOBS_ENABLED"
    public static let localCodingSnapshotsEnabledEnvironmentKey =
        "ASSEMBLYWRIGHT_MAC_DEVELOPER_LOCAL_CODING_SNAPSHOTS_ENABLED"
    public static let mlxExecutableEnvironmentKey =
        "ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_EXECUTABLE"
    public static let mlxModelDirectoryEnvironmentKey =
        "ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_DIR"
    public static let mlxModelIDEnvironmentKey =
        "ASSEMBLYWRIGHT_MAC_DEVELOPER_MLX_MODEL_ID"
    public let executableURL: URL?
    public let expectedTeamIdentifier: String?
    public let eventRelayConfiguration: AssemblywrightMacDeveloperEventRelayConfiguration?

    public init(environment: [String: String] = ProcessInfo.processInfo.environment) {
        guard let value = environment[Self.executableEnvironmentKey], !value.isEmpty,
              let teamIdentifier = environment[Self.teamIdentifierEnvironmentKey],
              Self.isValidTeamIdentifier(teamIdentifier) else {
            executableURL = nil
            expectedTeamIdentifier = nil
            eventRelayConfiguration = nil
            return
        }
        let agentExecutable = environment[Self.agentExecutableEnvironmentKey]
        let agentDataDirectory = environment[Self.agentDataDirectoryEnvironmentKey]
        let fixtureJobsValue = environment[Self.fixtureJobsEnabledEnvironmentKey]
        let mlxJobsValue = environment[Self.mlxJobsEnabledEnvironmentKey]
        let localCodingSnapshotsValue =
            environment[Self.localCodingSnapshotsEnabledEnvironmentKey]
        guard Self.isValidBooleanOptIn(fixtureJobsValue),
              Self.isValidBooleanOptIn(mlxJobsValue),
              Self.isValidBooleanOptIn(localCodingSnapshotsValue) else {
            executableURL = nil
            expectedTeamIdentifier = nil
            eventRelayConfiguration = nil
            return
        }
        let fixtureJobsEnabled = fixtureJobsValue == "true"
        let mlxJobsEnabled = mlxJobsValue == "true"
        let localCodingSnapshotsEnabled = localCodingSnapshotsValue == "true"
        guard [fixtureJobsEnabled, mlxJobsEnabled, localCodingSnapshotsEnabled]
            .filter({ $0 }).count <= 1 else {
            executableURL = nil
            expectedTeamIdentifier = nil
            eventRelayConfiguration = nil
            return
        }
        let mlxExecutable = environment[Self.mlxExecutableEnvironmentKey]
        let mlxModelDirectory = environment[Self.mlxModelDirectoryEnvironmentKey]
        let mlxModelID = environment[Self.mlxModelIDEnvironmentKey]
        guard mlxJobsEnabled
                ? mlxExecutable != nil && mlxModelDirectory != nil && mlxModelID != nil
                : mlxExecutable == nil && mlxModelDirectory == nil && mlxModelID == nil else {
            executableURL = nil
            expectedTeamIdentifier = nil
            eventRelayConfiguration = nil
            return
        }
        if mlxJobsEnabled {
            guard let mlxExecutable, Self.isValidAbsolutePath(mlxExecutable),
                  let mlxModelDirectory, Self.isValidAbsolutePath(mlxModelDirectory) else {
                executableURL = nil
                expectedTeamIdentifier = nil
                eventRelayConfiguration = nil
                return
            }
        }
        guard (agentExecutable == nil) == (agentDataDirectory == nil) else {
            executableURL = nil
            expectedTeamIdentifier = nil
            eventRelayConfiguration = nil
            return
        }
        let relayConfiguration: AssemblywrightMacDeveloperEventRelayConfiguration?
        if let agentExecutable, !agentExecutable.isEmpty,
           let agentDataDirectory, !agentDataDirectory.isEmpty {
            let relay = AssemblywrightMacDeveloperEventRelayConfiguration(
                agentExecutableURL: URL(fileURLWithPath: agentExecutable),
                agentDataDirectoryURL: URL(
                    fileURLWithPath: agentDataDirectory,
                    isDirectory: true
                ),
                fixtureJobsEnabled: fixtureJobsEnabled,
                mlxJobsEnabled: mlxJobsEnabled,
                localCodingSnapshotsEnabled: localCodingSnapshotsEnabled,
                mlxExecutableURL: mlxExecutable.map(URL.init(fileURLWithPath:)),
                mlxModelDirectoryURL: mlxModelDirectory.map {
                    URL(fileURLWithPath: $0, isDirectory: true)
                },
                mlxModelID: mlxModelID
            )
            guard (try? relay.validatePaths()) != nil else {
                executableURL = nil
                expectedTeamIdentifier = nil
                eventRelayConfiguration = nil
                return
            }
            relayConfiguration = relay
        } else {
            guard !fixtureJobsEnabled, !mlxJobsEnabled,
                  !localCodingSnapshotsEnabled else {
                executableURL = nil
                expectedTeamIdentifier = nil
                eventRelayConfiguration = nil
                return
            }
            relayConfiguration = nil
        }
        executableURL = URL(fileURLWithPath: value)
        expectedTeamIdentifier = teamIdentifier
        eventRelayConfiguration = relayConfiguration
    }

    private static func isValidTeamIdentifier(_ value: String) -> Bool {
        value.utf8.count == 10 && value.utf8.allSatisfy({
            (0x41 ... 0x5a).contains($0) || (0x30 ... 0x39).contains($0)
        })
    }

    private static func isValidBooleanOptIn(_ value: String?) -> Bool {
        value == nil || value == "false" || value == "true"
    }

    private static func isValidAbsolutePath(_ value: String) -> Bool {
        value.hasPrefix("/")
            && !value.contains("\0")
            && !value.split(separator: "/").contains("..")
            && value.utf8.count <= 4 * 1_024
    }
}

public enum AssemblywrightDeveloperBridgeProcessError: Error, Equatable, Sendable {
    case invalidExecutablePath
    case invalidExecutableSignature
    case launchFailed
    case teardownFailed
    case outputTooLarge
    case invalidSnapshot
    case helperExited
}

public struct AssemblywrightDeveloperBridgeValidatedExecutable: Equatable, Sendable {
    public let executableURL: URL
    public let teamIdentifier: String
    public let codeRequirement: String
    public let cdHash: Data

    public init(
        executableURL: URL,
        teamIdentifier: String,
        codeRequirement: String,
        cdHash: Data
    ) {
        self.executableURL = executableURL
        self.teamIdentifier = teamIdentifier
        self.codeRequirement = codeRequirement
        self.cdHash = cdHash
    }
}

public protocol AssemblywrightDeveloperBridgeExecutableValidating: Sendable {
    func validate(
        executableURL: URL,
        expectedTeamIdentifier: String
    ) throws -> AssemblywrightDeveloperBridgeValidatedExecutable
}

public struct SecurityAssemblywrightDeveloperBridgeExecutableValidator:
    AssemblywrightDeveloperBridgeExecutableValidating, Sendable
{
    public static let helperIdentifier = "com.nobiletechnology.assemblywright.developer-bridge.cli"
    private static let maximumPathBytes = 4 * 1_024

    public init() {}

    public func validate(
        executableURL: URL,
        expectedTeamIdentifier: String
    ) throws -> AssemblywrightDeveloperBridgeValidatedExecutable {
        let standardized = executableURL.standardizedFileURL
        guard standardized.isFileURL,
              standardized.path.hasPrefix("/"),
              !standardized.path.contains("\0"),
              standardized.path.utf8.count <= Self.maximumPathBytes else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidExecutablePath
        }
        var metadata = stat()
        guard lstat(standardized.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFREG,
              access(standardized.path, X_OK) == 0 else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidExecutablePath
        }

        var code: SecStaticCode?
        guard SecStaticCodeCreateWithPath(standardized as CFURL, [], &code) == errSecSuccess,
              let code else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidExecutableSignature
        }
        var rawInformation: CFDictionary?
        guard SecCodeCopySigningInformation(
            code,
            SecCSFlags(rawValue: kSecCSSigningInformation),
            &rawInformation
        ) == errSecSuccess,
        let information = rawInformation as? [String: Any],
        information[kSecCodeInfoIdentifier as String] as? String == Self.helperIdentifier,
        let teamIdentifier = information[kSecCodeInfoTeamIdentifier as String] as? String,
        teamIdentifier == expectedTeamIdentifier,
        let cdHash = information[kSecCodeInfoUnique as String] as? Data,
        !cdHash.isEmpty,
        cdHash.count <= 64,
        let executable = information[kSecCodeInfoMainExecutable as String] as? URL,
        executable.standardizedFileURL.path == standardized.path,
        let entitlements = information[kSecCodeInfoEntitlementsDict as String] as? [String: Any],
        entitlements["com.apple.developer.team-identifier"] as? String == teamIdentifier,
        entitlements["com.apple.application-identifier"] as? String
            == "\(teamIdentifier).\(Self.helperIdentifier)",
        let accessGroups = entitlements["keychain-access-groups"] as? [String],
        accessGroups == ["\(teamIdentifier).\(Self.helperIdentifier)"] else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidExecutableSignature
        }
        let requirementText =
            "anchor apple generic and identifier \"\(Self.helperIdentifier)\" "
            + "and certificate leaf[subject.OU] = \"\(teamIdentifier)\""
        var requirement: SecRequirement?
        guard SecRequirementCreateWithString(
            requirementText as CFString,
            [],
            &requirement
        ) == errSecSuccess,
        let requirement,
        SecStaticCodeCheckValidity(
            code,
            SecCSFlags(rawValue: kSecCSStrictValidate),
            requirement
        ) == errSecSuccess else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidExecutableSignature
        }
        return AssemblywrightDeveloperBridgeValidatedExecutable(
            executableURL: standardized,
            teamIdentifier: teamIdentifier,
            codeRequirement: requirementText,
            cdHash: cdHash
        )
    }
}

public protocol AssemblywrightDeveloperBridgeRunningProcessValidating: Sendable {
    func validate(
        processIdentifier: Int32,
        expected: AssemblywrightDeveloperBridgeValidatedExecutable
    ) throws
}

public struct SecurityAssemblywrightDeveloperBridgeRunningProcessValidator:
    AssemblywrightDeveloperBridgeRunningProcessValidating, Sendable
{
    public init() {}

    public func validate(
        processIdentifier: Int32,
        expected: AssemblywrightDeveloperBridgeValidatedExecutable
    ) throws {
        let attributes = [
            kSecGuestAttributePid as String: NSNumber(value: processIdentifier)
        ] as CFDictionary
        var runningCode: SecCode?
        guard SecCodeCopyGuestWithAttributes(nil, attributes, [], &runningCode) == errSecSuccess,
              let runningCode else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidExecutableSignature
        }
        var requirement: SecRequirement?
        guard SecRequirementCreateWithString(
            expected.codeRequirement as CFString,
            [],
            &requirement
        ) == errSecSuccess,
        let requirement,
        SecCodeCheckValidity(
            runningCode,
            SecCSFlags(rawValue: kSecCSStrictValidate),
            requirement
        ) == errSecSuccess else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidExecutableSignature
        }
        var staticCode: SecStaticCode?
        var rawInformation: CFDictionary?
        guard SecCodeCopyStaticCode(runningCode, [], &staticCode) == errSecSuccess,
              let staticCode,
              SecCodeCopySigningInformation(
                  staticCode,
                  SecCSFlags(rawValue: kSecCSSigningInformation),
                  &rawInformation
              ) == errSecSuccess,
              let information = rawInformation as? [String: Any],
              information[kSecCodeInfoTeamIdentifier as String] as? String
                  == expected.teamIdentifier,
              information[kSecCodeInfoUnique as String] as? Data == expected.cdHash,
              let runningExecutable = information[kSecCodeInfoMainExecutable as String] as? URL,
              runningExecutable.standardizedFileURL.path
                  == expected.executableURL.standardizedFileURL.path else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidExecutableSignature
        }
    }
}

public protocol AssemblywrightDeveloperBridgeProcessSession: Sendable {
    var outputLines: AsyncThrowingStream<Data, Error> { get }
    func stop() async throws
}

public protocol AssemblywrightDeveloperBridgeProcessLaunching: Sendable {
    func launch(
        executable: AssemblywrightDeveloperBridgeValidatedExecutable,
        eventRelayConfiguration: AssemblywrightMacDeveloperEventRelayConfiguration?
    ) async throws -> any AssemblywrightDeveloperBridgeProcessSession
}

public struct FoundationAssemblywrightDeveloperBridgeProcessLauncher:
    AssemblywrightDeveloperBridgeProcessLaunching, Sendable
{
    private let runningProcessValidator: any AssemblywrightDeveloperBridgeRunningProcessValidating

    public init(
        runningProcessValidator: any AssemblywrightDeveloperBridgeRunningProcessValidating =
            SecurityAssemblywrightDeveloperBridgeRunningProcessValidator()
    ) {
        self.runningProcessValidator = runningProcessValidator
    }

    public func launch(
        executable: AssemblywrightDeveloperBridgeValidatedExecutable,
        eventRelayConfiguration: AssemblywrightMacDeveloperEventRelayConfiguration?
    ) async throws -> any AssemblywrightDeveloperBridgeProcessSession {
        try FoundationAssemblywrightDeveloperBridgeProcessSession(
            executable: executable,
            eventRelayConfiguration: eventRelayConfiguration,
            runningProcessValidator: runningProcessValidator
        )
    }

    static func helperArguments(
        eventRelayConfiguration: AssemblywrightMacDeveloperEventRelayConfiguration?
    ) -> [String] {
        if let eventRelayConfiguration, eventRelayConfiguration.fixtureJobsEnabled {
            return ["relay", "--identity-profile", "fixture"]
        }
        if let eventRelayConfiguration, eventRelayConfiguration.localCodingSnapshotsEnabled {
            return ["relay", "--identity-profile", "local-coding"]
        }
        return [eventRelayConfiguration == nil ? "monitor" : "relay"]
    }
}

private actor FoundationAssemblywrightDeveloperBridgeProcessSession:
    AssemblywrightDeveloperBridgeProcessSession
{
    private let process: Process
    private let pipe: Pipe
    private let reader: Task<Void, Never>
    nonisolated let outputLines: AsyncThrowingStream<Data, Error>
    private var stopped = false

    init(
        executable: AssemblywrightDeveloperBridgeValidatedExecutable,
        eventRelayConfiguration: AssemblywrightMacDeveloperEventRelayConfiguration?,
        runningProcessValidator: any AssemblywrightDeveloperBridgeRunningProcessValidating
    ) throws {
        let process = Process()
        let pipe = Pipe()
        let input = eventRelayConfiguration == nil ? nil : Pipe()
        var continuation: AsyncThrowingStream<Data, Error>.Continuation!
        outputLines = AsyncThrowingStream(bufferingPolicy: .bufferingOldest(1)) {
            continuation = $0
        }
        self.process = process
        self.pipe = pipe

        process.executableURL = executable.executableURL
        process.arguments = FoundationAssemblywrightDeveloperBridgeProcessLauncher.helperArguments(
            eventRelayConfiguration: eventRelayConfiguration
        )
        process.environment = [:]
        process.standardInput = input ?? FileHandle.nullDevice
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            throw AssemblywrightDeveloperBridgeProcessError.launchFailed
        }

        do {
            try runningProcessValidator.validate(
                processIdentifier: process.processIdentifier,
                expected: executable
            )
            if let eventRelayConfiguration, let input {
                let document = try eventRelayConfiguration.encodeStartupDocument()
                try input.fileHandleForWriting.write(contentsOf: document)
                try input.fileHandleForWriting.close()
            }
        } catch {
            try? input?.fileHandleForWriting.close()
            try? pipe.fileHandleForReading.close()
            guard Self.killAndReapRejectedProcess(process) else {
                throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
            }
            throw AssemblywrightDeveloperBridgeProcessError.invalidExecutableSignature
        }

        reader = Task.detached {
            var pending = Data()
            while !Task.isCancelled {
                let chunk = pipe.fileHandleForReading.availableData
                if chunk.isEmpty { break }
                pending.append(chunk)
                if pending.count > AssemblywrightDeveloperBridgeProcessLifecycle.maximumBufferedBytes {
                    continuation.finish(throwing: AssemblywrightDeveloperBridgeProcessError.outputTooLarge)
                    return
                }
                while let newline = pending.firstIndex(of: 0x0a) {
                    var line = pending.prefix(upTo: newline)
                    pending.removeSubrange(...newline)
                    if line.last == 0x0d { line = line.dropLast() }
                    guard !line.isEmpty,
                          line.count <= AssemblywrightDeveloperBridgeProcessLifecycle.maximumLineBytes else {
                        continuation.finish(throwing: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot)
                        return
                    }
                    switch continuation.yield(Data(line)) {
                    case .enqueued:
                        break
                    case .dropped:
                        continuation.finish(
                            throwing: AssemblywrightDeveloperBridgeProcessError.outputTooLarge
                        )
                        return
                    case .terminated:
                        return
                    @unknown default:
                        continuation.finish(
                            throwing: AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
                        )
                        return
                    }
                }
            }
            guard Task.isCancelled else {
                continuation.finish(throwing: AssemblywrightDeveloperBridgeProcessError.helperExited)
                return
            }
            continuation.finish()
        }
    }

    func stop() async throws {
        guard !stopped else { return }
        reader.cancel()
        if process.isRunning {
            process.terminate()
        }
        try? pipe.fileHandleForReading.close()
        await waitForProcessExit(until: .now + .milliseconds(500))
        if process.isRunning {
            let result = Darwin.kill(process.processIdentifier, SIGKILL)
            guard result == 0 || errno == ESRCH else {
                throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
            }
            await waitForProcessExit(until: .now + .seconds(1))
        }
        guard !process.isRunning else {
            throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
        }
        await reader.value
        stopped = true
    }

    private func waitForProcessExit(until deadline: ContinuousClock.Instant) async {
        while process.isRunning, ContinuousClock.now < deadline {
            try? await Task.sleep(for: .milliseconds(20))
        }
    }

    private nonisolated static func killAndReapRejectedProcess(_ process: Process) -> Bool {
        if process.isRunning {
            let result = Darwin.kill(process.processIdentifier, SIGKILL)
            guard result == 0 || errno == ESRCH else { return false }
        }
        let deadline = ContinuousClock.now + .seconds(1)
        while process.isRunning, ContinuousClock.now < deadline {
            usleep(20_000)
        }
        return !process.isRunning
    }

}

@MainActor
public final class AssemblywrightDeveloperBridgeProcessLifecycle: ObservableObject {
    nonisolated public static let maximumLineBytes = 96 * 1_024
    nonisolated public static let maximumBufferedBytes = maximumLineBytes + 1
    nonisolated public static let proofBoundary =
        "Read-only Developer Mode health, Feature Conveyor observation, and metadata relay, with a separately explicit Public synthetic fixture-job diagnostic. Guidance is display-only and does not enable models, tools, files, repositories, Codex, Git, or owner-action authority."

    @Published public private(set) var status: AssemblywrightDeveloperBridgeAppStatus

    private let configuration: AssemblywrightDeveloperBridgeProcessConfiguration
    private let validator: any AssemblywrightDeveloperBridgeExecutableValidating
    private let launcher: any AssemblywrightDeveloperBridgeProcessLaunching
    private var task: Task<Void, Never>?
    private var session: (any AssemblywrightDeveloperBridgeProcessSession)?

    public init(
        configuration: AssemblywrightDeveloperBridgeProcessConfiguration = .init(),
        validator: any AssemblywrightDeveloperBridgeExecutableValidating =
            SecurityAssemblywrightDeveloperBridgeExecutableValidator(),
        launcher: any AssemblywrightDeveloperBridgeProcessLaunching =
            FoundationAssemblywrightDeveloperBridgeProcessLauncher()
    ) {
        self.configuration = configuration
        self.validator = validator
        self.launcher = launcher
        status = configuration.executableURL == nil ? .disabled : .init(phase: .starting)
    }

    public func start() {
        guard task == nil, session == nil else { return }
        guard let executableURL = configuration.executableURL,
              let expectedTeamIdentifier = configuration.expectedTeamIdentifier else {
            status = .disabled
            return
        }
        status = .init(phase: .starting)
        task = Task { [weak self] in
            guard let self else { return }
            do {
                let validated = try validator.validate(
                    executableURL: executableURL,
                    expectedTeamIdentifier: expectedTeamIdentifier
                )
                let launched = try await launcher.launch(
                    executable: validated,
                    eventRelayConfiguration: configuration.eventRelayConfiguration
                )
                session = launched
                for try await line in launched.outputLines {
                    if Task.isCancelled { break }
                    status = try Self.status(from: line)
                }
                if !Task.isCancelled {
                    status = .init(phase: .masterOffline, errorCode: "helper_exited")
                }
            } catch {
                if !Task.isCancelled {
                    status = .init(
                        phase: .masterOffline,
                        errorCode: Self.errorCode(for: error)
                    )
                }
            }
            do {
                try await launchedSessionStop()
            } catch {
                status = .init(
                    phase: .masterOffline,
                    errorCode: Self.errorCode(for: error)
                )
            }
            task = nil
        }
    }

    public func stop() async {
        let running = task
        running?.cancel()
        var stopError: Error?
        do {
            try await launchedSessionStop()
        } catch {
            stopError = error
        }
        if let running { await running.value }
        task = nil
        if let stopError {
            status = .init(
                phase: .masterOffline,
                errorCode: Self.errorCode(for: stopError)
            )
        } else if configuration.executableURL != nil {
            status = .init(phase: .stopped)
        } else {
            status = .disabled
        }
    }

    public func superviseUntilCancelled() async {
        start()
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(3_600))
        }
        await stop()
    }

    nonisolated public static func status(from line: Data) throws -> AssemblywrightDeveloperBridgeAppStatus {
        let snapshot = try AssemblywrightMacBridgeSupervisorSnapshot.decodeStrict(line)
        switch snapshot.phase {
        case .authenticated:
            let phase: AssemblywrightDeveloperBridgeAppPhase
            if snapshot.maintenanceActive == true {
                phase = .maintenance
            } else if snapshot.emergencyPaused == true {
                phase = .paused
            } else {
                phase = .connected
            }
            return AssemblywrightDeveloperBridgeAppStatus(
                phase: phase,
                masterEndpoint: snapshot.masterEndpoint,
                connectionEpoch: snapshot.connectionEpoch,
                featureConveyor: snapshot.featureConveyor
            )
        case .backingOff:
            return AssemblywrightDeveloperBridgeAppStatus(
                phase: .masterOffline,
                errorCode: snapshot.errorCode
            )
        case .stopped:
            return AssemblywrightDeveloperBridgeAppStatus(phase: .stopped)
        }
    }

    private func launchedSessionStop() async throws {
        guard let launched = session else { return }
        try await launched.stop()
        session = nil
    }

    private static func errorCode(for error: Error) -> String {
        switch error as? AssemblywrightDeveloperBridgeProcessError {
        case .invalidExecutablePath: "invalid_helper_path"
        case .invalidExecutableSignature: "invalid_helper_signature"
        case .launchFailed: "helper_launch_failed"
        case .teardownFailed: "helper_teardown_failed"
        case .outputTooLarge: "helper_output_too_large"
        case .invalidSnapshot: "invalid_helper_snapshot"
        case .helperExited: "helper_exited"
        case nil: "helper_unavailable"
        }
    }
}
