import Combine
import CoreFoundation
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

public enum AssemblywrightDeveloperBridgeConfigurationState: Equatable, Sendable {
    case notConfigured
    case configured
    case invalidStore
}

public struct AssemblywrightDeveloperBridgeEnrollmentStatus: Equatable, Sendable {
    public let installed: Bool
    public let deviceID: String?
    public let deviceName: String?
    public let masterEndpoint: String?
    public let registryRevision: UInt64?
    public let certificateNotAfterMilliseconds: UInt64?

    public init(
        installed: Bool,
        deviceID: String? = nil,
        deviceName: String? = nil,
        masterEndpoint: String? = nil,
        registryRevision: UInt64? = nil,
        certificateNotAfterMilliseconds: UInt64? = nil
    ) {
        self.installed = installed
        self.deviceID = deviceID
        self.deviceName = deviceName
        self.masterEndpoint = masterEndpoint
        self.registryRevision = registryRevision
        self.certificateNotAfterMilliseconds = certificateNotAfterMilliseconds
    }
}

public struct AssemblywrightDeveloperBridgeAppStatus: Equatable, Sendable {
    public let phase: AssemblywrightDeveloperBridgeAppPhase
    public let deviceID: String?
    public let masterEndpoint: String?
    public let connectionEpoch: UInt64?
    public let featureConveyor: AssemblywrightMacFeatureConveyorStatus?
    public let ownerControl: AssemblywrightMacFeatureConveyorOwnerControlProjection?
    public let assemblyLine: AssemblywrightMacAssemblyLineOwnerProjection?
    public let localModelSelection: AssemblywrightMacLocalModelSelectionProjection?
    public let errorCode: String?

    public init(
        phase: AssemblywrightDeveloperBridgeAppPhase,
        deviceID: String? = nil,
        masterEndpoint: String? = nil,
        connectionEpoch: UInt64? = nil,
        featureConveyor: AssemblywrightMacFeatureConveyorStatus? = nil,
        ownerControl: AssemblywrightMacFeatureConveyorOwnerControlProjection? = nil,
        assemblyLine: AssemblywrightMacAssemblyLineOwnerProjection? = nil,
        localModelSelection: AssemblywrightMacLocalModelSelectionProjection? = nil,
        errorCode: String? = nil
    ) {
        self.phase = phase
        self.deviceID = deviceID
        self.masterEndpoint = masterEndpoint
        self.connectionEpoch = connectionEpoch
        self.featureConveyor = featureConveyor
        self.ownerControl = ownerControl
        self.assemblyLine = assemblyLine
        self.localModelSelection = localModelSelection
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

    public init(
        executableURL: URL,
        expectedTeamIdentifier: String,
        eventRelayConfiguration: AssemblywrightMacDeveloperEventRelayConfiguration? = nil
    ) {
        guard Self.isValidTeamIdentifier(expectedTeamIdentifier) else {
            self.executableURL = nil
            self.expectedTeamIdentifier = nil
            self.eventRelayConfiguration = nil
            return
        }
        self.executableURL = executableURL.standardizedFileURL
        self.expectedTeamIdentifier = expectedTeamIdentifier
        self.eventRelayConfiguration = eventRelayConfiguration
    }

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

    static func isValidTeamIdentifier(_ value: String) -> Bool {
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
    case commandRejectedBeforeEffect
    case commandNotSubmitted
    case commandOutcomeUnknown
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
    func runCommand(
        executable: AssemblywrightDeveloperBridgeValidatedExecutable,
        arguments: [String],
        input: Data
    ) async throws -> Data
}

public extension AssemblywrightDeveloperBridgeProcessLaunching {
    func runCommand(
        executable _: AssemblywrightDeveloperBridgeValidatedExecutable,
        arguments _: [String],
        input _: Data
    ) async throws -> Data { throw AssemblywrightDeveloperBridgeProcessError.launchFailed }
}

public struct FoundationAssemblywrightDeveloperBridgeProcessLauncher:
    AssemblywrightDeveloperBridgeProcessLaunching, Sendable
{
    private let runningProcessValidator: any AssemblywrightDeveloperBridgeRunningProcessValidating
    private let ownerCommandTimeout: Duration

    public init(
        runningProcessValidator: any AssemblywrightDeveloperBridgeRunningProcessValidating =
            SecurityAssemblywrightDeveloperBridgeRunningProcessValidator(),
        ownerCommandTimeout: Duration = .seconds(30)
    ) {
        self.runningProcessValidator = runningProcessValidator
        self.ownerCommandTimeout = ownerCommandTimeout
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


    public func runCommand(
        executable: AssemblywrightDeveloperBridgeValidatedExecutable,
        arguments: [String],
        input: Data
    ) async throws -> Data {
        let ownerActions = AssemblywrightMacOwnerControlAction.allCases.map(
            AssemblywrightDeveloperBridgeProcessLifecycle.helperArguments(for:)
        )
        let approvedFeatureEnqueue =
            arguments == AssemblywrightDeveloperBridgeProcessLifecycle
                .approvedFeatureEnqueueArguments
        let localModelSelection = arguments
            == AssemblywrightDeveloperBridgeProcessLifecycle.localModelSelectionArguments
            || arguments
                == AssemblywrightDeveloperBridgeProcessLifecycle
                    .localModelReconciliationArguments
        let assemblyLinePlanningAction = AssemblywrightMacAssemblyLinePlanningAction.allCases
            .first(where: { $0.helperArguments == arguments })
        let assemblyLinePlanning = assemblyLinePlanningAction != nil
        let setupCommand = AssemblywrightDeveloperBridgeProcessLifecycle
            .setupHelperArguments.contains(arguments)
        let setupStatus = arguments
            == AssemblywrightDeveloperBridgeProcessLifecycle.enrollmentStatusArguments
        let effectfulSetupInstall = arguments
            == AssemblywrightDeveloperBridgeProcessLifecycle.enrollmentInstallArguments
            || arguments
                == AssemblywrightDeveloperBridgeProcessLifecycle.rotationInstallArguments
        let inputLimit = approvedFeatureEnqueue
            ? AssemblywrightMacFeatureConveyorApprovedFeatureDraft.maximumRequestBytes
            : localModelSelection
                ? AssemblywrightMacLocalModelSelectionControl.maximumFrameBytes
                : assemblyLinePlanning
                    ? AssemblywrightMacAssemblyLineOwnerControl.maximumRequestBytes
                    : setupCommand
                        ? AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes
                        : 8 * 1_024
        let outputLimit = approvedFeatureEnqueue
            ? AssemblywrightMacFeatureConveyorOwnerControl.maximumReceiptBytes
            : assemblyLinePlanning
                ? AssemblywrightMacAssemblyLineOwnerControl.maximumResponseBytes
                : setupCommand
                    ? AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes
                    : 8 * 1_024
        guard (ownerActions.contains(arguments) || approvedFeatureEnqueue || localModelSelection
                || assemblyLinePlanning || setupCommand),
              input.count <= inputLimit,
              setupStatus ? input.isEmpty : !input.isEmpty else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        let process = Process()
        let inputPipe = Pipe()
        let outputPipe = Pipe()
        process.executableURL = executable.executableURL
        process.arguments = arguments
        process.environment = [:]
        process.standardInput = inputPipe
        process.standardOutput = outputPipe
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { throw AssemblywrightDeveloperBridgeProcessError.launchFailed }
        let commandDeadline = ContinuousClock.now + ownerCommandTimeout
        let timeoutMilliseconds = max(
            Int(ownerCommandTimeout.components.seconds * 1_000)
                + Int(ownerCommandTimeout.components.attoseconds / 1_000_000_000_000_000),
            1
        )
        let timeout = DispatchWorkItem { Self.terminateThenKill(process) }
        DispatchQueue.global(qos: .utility).asyncAfter(
            deadline: .now() + .milliseconds(timeoutMilliseconds),
            execute: timeout
        )
        var inputDelivered = false
        do {
            try runningProcessValidator.validate(processIdentifier: process.processIdentifier, expected: executable)
            try inputPipe.fileHandleForWriting.write(contentsOf: input)
            try inputPipe.fileHandleForWriting.close()
            inputDelivered = true
            let output = try await withTaskCancellationHandler {
                var result = Data()
                while true {
                    let remaining = outputLimit + 2 - result.count
                    guard remaining > 0 else { throw AssemblywrightDeveloperBridgeProcessError.outputTooLarge }
                    let chunk = try outputPipe.fileHandleForReading.read(upToCount: min(4 * 1_024, remaining)) ?? Data()
                    if chunk.isEmpty { break }
                    result.append(chunk)
                }
                return result
            } onCancel: {
                Self.terminateThenKill(process)
            }
            let reaped = await Self.waitForProcessExit(
                process,
                until: commandDeadline + .seconds(2)
            )
            timeout.cancel()
            guard reaped else {
                _ = Self.signal(process, SIGKILL)
                guard await Self.waitForProcessExit(
                    process,
                    until: ContinuousClock.now + .seconds(1)
                ) else {
                    throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
                }
                throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
            }
            try Task.checkCancellation()
            if assemblyLinePlanning, process.terminationStatus != 0 {
                guard output.isEmpty else {
                    throw AssemblywrightDeveloperBridgeProcessError.commandOutcomeUnknown
                }
                switch process.terminationStatus {
                case AssemblywrightMacAssemblyLineHelperExitStatus.rejectedBeforeEffect:
                    throw AssemblywrightDeveloperBridgeProcessError.commandRejectedBeforeEffect
                case AssemblywrightMacAssemblyLineHelperExitStatus.outcomeUnknown:
                    throw AssemblywrightDeveloperBridgeProcessError.commandOutcomeUnknown
                default:
                    throw AssemblywrightDeveloperBridgeProcessError.commandOutcomeUnknown
                }
            }
            guard process.terminationStatus == 0, !output.isEmpty,
                  output.count <= outputLimit + 1,
                  output.last == 0x0a else { throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot }
            let line = output.dropLast()
            guard !line.isEmpty, !line.contains(0x0a) else { throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot }
            let response = Data(line)
            if let assemblyLinePlanningAction {
                try AssemblywrightMacAssemblyLineOwnerControl.validateHelperOutput(
                    action: assemblyLinePlanningAction,
                    requestData: input,
                    responseData: response
                )
            }
            return response
        } catch {
            timeout.cancel()
            try? inputPipe.fileHandleForWriting.close()
            guard await Self.terminateKillAndReap(process) else {
                throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
            }
            if assemblyLinePlanning, inputDelivered,
               error as? AssemblywrightDeveloperBridgeProcessError
                != .commandRejectedBeforeEffect {
                throw AssemblywrightDeveloperBridgeProcessError.commandOutcomeUnknown
            }
            if effectfulSetupInstall, inputDelivered {
                throw AssemblywrightDeveloperBridgeProcessError.commandOutcomeUnknown
            }
            if assemblyLinePlanning, !inputDelivered,
               error as? AssemblywrightDeveloperBridgeProcessError
                != .invalidExecutableSignature {
                throw AssemblywrightDeveloperBridgeProcessError.commandNotSubmitted
            }
            throw error
        }
    }

    private static func terminateThenKill(_ process: Process) {
        guard process.isRunning else { return }
        process.terminate()
        DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + .milliseconds(500)) {
            _ = signal(process, SIGKILL)
        }
    }

    private static func terminateKillAndReap(_ process: Process) async -> Bool {
        guard process.isRunning else { return true }
        process.terminate()
        if await waitForProcessExit(
            process,
            until: ContinuousClock.now + .milliseconds(500)
        ) {
            return true
        }
        _ = signal(process, SIGKILL)
        return await waitForProcessExit(
            process,
            until: ContinuousClock.now + .seconds(1)
        )
    }

    private static func signal(_ process: Process, _ signal: Int32) -> Bool {
        guard process.isRunning else { return true }
        let result = Darwin.kill(process.processIdentifier, signal)
        return result == 0 || errno == ESRCH
    }

    private static func waitForProcessExit(
        _ process: Process,
        until deadline: ContinuousClock.Instant
    ) async -> Bool {
        while process.isRunning, ContinuousClock.now < deadline {
            try? await Task.sleep(for: .milliseconds(20))
        }
        return !process.isRunning
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
    nonisolated public static let maximumLegacySnapshotBytes = 96 * 1_024
    nonisolated public static let maximumAssemblyLineSnapshotBytes =
        AssemblywrightMacAssemblyLineOwnerProjection.maximumBytes
    nonisolated public static let maximumLineBytes =
        maximumLegacySnapshotBytes + maximumAssemblyLineSnapshotBytes
    nonisolated public static let maximumBufferedBytes = maximumLineBytes + 1
    nonisolated public static let proofBoundary =
        "Read-only observation shows bounded Windows-authoritative Feature Conveyor and activation readiness. Authoring and owner actions are separate, explicitly confirmed signed-helper operations; observation does not enable authority and stores no token, command, path, raw evidence, credential, or provider output."
    nonisolated public static let enrollmentStatusArguments = ["status"]
    nonisolated public static let enrollmentPrepareArguments = ["enrollment", "prepare"]
    nonisolated public static let enrollmentInstallArguments = ["enrollment", "install"]
    nonisolated public static let rotationPrepareArguments = [
        "enrollment", "rotate", "prepare", "--confirm"
    ]
    nonisolated public static let rotationInstallArguments = [
        "enrollment", "rotate", "install", "--confirm"
    ]
    nonisolated static let setupHelperArguments = [
        enrollmentStatusArguments,
        enrollmentPrepareArguments,
        enrollmentInstallArguments,
        rotationPrepareArguments,
        rotationInstallArguments
    ]

    @Published public private(set) var status: AssemblywrightDeveloperBridgeAppStatus
    @Published public private(set) var ownerActionInProgress = false
    @Published public private(set) var ownerActionErrorCode: String?
    @Published public private(set) var approvedFeatureReceipt:
        AssemblywrightMacFeatureConveyorApprovedFeatureReceipt?
    @Published public private(set) var pendingApprovedFeatureRecovery:
        AssemblywrightMacApprovedFeaturePendingRecovery?
    @Published public private(set) var pendingAssemblyLinePlanningAction:
        AssemblywrightMacAssemblyLinePlanningAction?
    @Published public private(set) var localModelSelectionState:
        AssemblywrightMacLocalModelSelectionState
    @Published public private(set) var localModelSelectionErrorCode: String?
    @Published public private(set) var bridgeConfigurationState:
        AssemblywrightDeveloperBridgeConfigurationState
    @Published public private(set) var setupActionErrorCode: String?

    private var configuration: AssemblywrightDeveloperBridgeProcessConfiguration
    private let configurationStore: AssemblywrightDeveloperBridgeConfigurationStore
    private var configurationStoreLoadFailed: Bool
    private let validator: any AssemblywrightDeveloperBridgeExecutableValidating
    private let launcher: any AssemblywrightDeveloperBridgeProcessLaunching
    private let localModelSelectionStore: AssemblywrightMacLocalModelSelectionStore
    private let localModelSelectionStoreLoadFailed: Bool
    private let assemblyLinePendingMutationStore:
        AssemblywrightMacAssemblyLinePendingMutationStore
    private let assemblyLinePendingMutationStoreLoadFailed: Bool
    private var activeEventRelayConfiguration: AssemblywrightMacDeveloperEventRelayConfiguration?
    private var task: Task<Void, Never>?
    private var session: (any AssemblywrightDeveloperBridgeProcessSession)?
    private var pendingAssemblyLinePlanningMutation:
        AssemblywrightMacPendingAssemblyLinePlanningMutation?

    public init(
        configuration explicitConfiguration: AssemblywrightDeveloperBridgeProcessConfiguration? = nil,
        configurationStore: AssemblywrightDeveloperBridgeConfigurationStore = .init(),
        environment: [String: String]? = nil,
        validator: any AssemblywrightDeveloperBridgeExecutableValidating =
            SecurityAssemblywrightDeveloperBridgeExecutableValidator(),
        launcher: any AssemblywrightDeveloperBridgeProcessLaunching =
            FoundationAssemblywrightDeveloperBridgeProcessLauncher(),
        localModelSelectionStore: AssemblywrightMacLocalModelSelectionStore = .init(),
        assemblyLinePendingMutationStore:
            AssemblywrightMacAssemblyLinePendingMutationStore = .init()
    ) {
        self.configurationStore = configurationStore
        let configuration: AssemblywrightDeveloperBridgeProcessConfiguration
        let configurationState: AssemblywrightDeveloperBridgeConfigurationState
        var configurationLoadFailed = false
        if let explicitConfiguration {
            configuration = explicitConfiguration
            configurationState = explicitConfiguration.executableURL == nil
                ? .notConfigured : .configured
        } else {
            do {
                if let stored = try configurationStore.load() {
                    configuration = stored.processConfiguration
                    configurationState = .configured
                } else if let environment {
                    configuration = .init(environment: environment)
                    configurationState = configuration.executableURL == nil
                        ? .notConfigured : .configured
                } else {
                    configuration = .init(environment: [:])
                    configurationState = .notConfigured
                }
            } catch {
                configuration = .init(environment: [:])
                configurationState = .invalidStore
                configurationLoadFailed = true
            }
        }
        self.configuration = configuration
        bridgeConfigurationState = configurationState
        configurationStoreLoadFailed = configurationLoadFailed
        setupActionErrorCode = configurationLoadFailed
            ? "developer_bridge_configuration_store_invalid" : nil
        self.validator = validator
        self.launcher = launcher
        self.localModelSelectionStore = localModelSelectionStore
        self.assemblyLinePendingMutationStore = assemblyLinePendingMutationStore
        let loadedSelection: AssemblywrightMacLocalModelSelectionState
        var selectionStoreLoadFailed = false
        do {
            loadedSelection = try localModelSelectionStore.load()
                ?? AssemblywrightMacLocalModelSelectionState(active: nil, pending: nil)
        } catch {
            loadedSelection = AssemblywrightMacLocalModelSelectionState(active: nil, pending: nil)
            selectionStoreLoadFailed = true
        }
        localModelSelectionState = loadedSelection
        var loadedAssemblyLinePendingMutation:
            AssemblywrightMacPendingAssemblyLinePlanningMutation?
        var assemblyLineStoreLoadFailed = false
        do {
            loadedAssemblyLinePendingMutation = try assemblyLinePendingMutationStore.load()
        } catch {
            loadedAssemblyLinePendingMutation = nil
            assemblyLineStoreLoadFailed = true
        }
        pendingAssemblyLinePlanningMutation = loadedAssemblyLinePendingMutation
        pendingAssemblyLinePlanningAction = loadedAssemblyLinePendingMutation?.action
        assemblyLinePendingMutationStoreLoadFailed = assemblyLineStoreLoadFailed
        var relayConfiguration = configuration.eventRelayConfiguration
        if !selectionStoreLoadFailed,
           let active = loadedSelection.active, let current = relayConfiguration {
            do {
                relayConfiguration = try active.relayConfiguration(replacing: current)
            } catch {
                selectionStoreLoadFailed = true
            }
        }
        activeEventRelayConfiguration = relayConfiguration
        localModelSelectionStoreLoadFailed = selectionStoreLoadFailed
        localModelSelectionErrorCode = selectionStoreLoadFailed
            ? "local_model_selection_store_invalid"
            : loadedSelection.pending == nil ? nil : "local_model_reconciliation_required"
        ownerActionErrorCode = assemblyLineStoreLoadFailed
            ? "assembly_line_pending_store_invalid"
            : loadedAssemblyLinePendingMutation == nil
                ? nil : "assembly_line_reconciliation_required"
        if configurationLoadFailed {
            status = .init(
                phase: .masterOffline,
                errorCode: "developer_bridge_configuration_store_invalid"
            )
        } else if selectionStoreLoadFailed {
            status = .init(
                phase: .masterOffline,
                errorCode: "local_model_selection_store_invalid"
            )
        } else if assemblyLineStoreLoadFailed {
            status = .init(
                phase: .masterOffline,
                errorCode: "assembly_line_pending_store_invalid"
            )
        } else if configuration.executableURL == nil {
            status = .disabled
        } else if loadedSelection.pending != nil {
            status = .init(
                phase: .masterOffline,
                errorCode: "local_model_reconciliation_required"
            )
        } else {
            status = .init(phase: .starting)
        }
    }

    public func start() {
        guard task == nil, session == nil else { return }
        guard !configurationStoreLoadFailed else {
            status = .init(
                phase: .masterOffline,
                errorCode: "developer_bridge_configuration_store_invalid"
            )
            return
        }
        guard !localModelSelectionStoreLoadFailed else {
            status = .init(
                phase: .masterOffline,
                errorCode: "local_model_selection_store_invalid"
            )
            return
        }
        guard !assemblyLinePendingMutationStoreLoadFailed else {
            status = .init(
                phase: .masterOffline,
                errorCode: "assembly_line_pending_store_invalid"
            )
            return
        }
        guard localModelSelectionState.pending == nil else {
            status = .init(
                phase: .masterOffline,
                errorCode: "local_model_reconciliation_required"
            )
            return
        }
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
                    eventRelayConfiguration: activeEventRelayConfiguration
                )
                session = launched
                for try await line in launched.outputLines {
                    if Task.isCancelled { break }
                    status = try Self.status(
                        from: line,
                        localCodingSnapshotsEnabled:
                            configuration.eventRelayConfiguration?
                                .localCodingSnapshotsEnabled == true
                                || activeEventRelayConfiguration?
                                .localCodingSnapshotsEnabled == true
                    )
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

    @discardableResult
    public func configureBridge(
        helperURL: URL,
        expectedTeamIdentifier: String
    ) async -> Bool {
        guard !ownerActionInProgress else { return false }
        ownerActionInProgress = true
        setupActionErrorCode = nil
        defer { ownerActionInProgress = false }
        do {
            let stored = try AssemblywrightDeveloperBridgeStoredConfiguration(
                helperPath: helperURL.standardizedFileURL.path,
                teamIdentifier: expectedTeamIdentifier
            )
            _ = try validator.validate(
                executableURL: URL(fileURLWithPath: stored.helperPath),
                expectedTeamIdentifier: stored.teamIdentifier
            )
            await stop()
            guard status.errorCode == nil else {
                throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
            }
            try configurationStore.save(stored)
            configuration = stored.processConfiguration
            activeEventRelayConfiguration = configuration.eventRelayConfiguration
            configurationStoreLoadFailed = false
            bridgeConfigurationState = .configured
            status = .init(phase: .starting)
            start()
            return true
        } catch {
            setupActionErrorCode = Self.setupErrorCode(for: error)
            if configurationStoreLoadFailed {
                bridgeConfigurationState = .invalidStore
                status = .init(
                    phase: .masterOffline,
                    errorCode: "developer_bridge_configuration_store_invalid"
                )
            } else if configuration.executableURL == nil {
                bridgeConfigurationState = .notConfigured
                status = .disabled
            } else {
                start()
            }
            return false
        }
    }

    public func retryBridgeConnection() async {
        guard !ownerActionInProgress else { return }
        guard !configurationStoreLoadFailed, configuration.executableURL != nil else {
            if configurationStoreLoadFailed {
                setupActionErrorCode = "developer_bridge_configuration_store_invalid"
            }
            return
        }
        ownerActionInProgress = true
        defer { ownerActionInProgress = false }
        setupActionErrorCode = nil
        await stop()
        guard status.errorCode == nil else {
            setupActionErrorCode = "helper_teardown_failed"
            return
        }
        status = .init(phase: .starting)
        start()
    }

    public func enrollmentStatus() async -> AssemblywrightDeveloperBridgeEnrollmentStatus? {
        guard let output = await performSetupHelperCommand(
            arguments: Self.enrollmentStatusArguments,
            input: Data(),
            ambiguityCode: "developer_bridge_status_unavailable"
        ) else { return nil }
        do {
            return try Self.decodeEnrollmentStatus(output, expectedStatus: nil)
        } catch {
            setupActionErrorCode = "invalid_helper_setup_response"
            return nil
        }
    }

    public func prepareEnrollment(invitationData: Data) async -> Data? {
        await prepareEnrollmentDocument(
            arguments: Self.enrollmentPrepareArguments,
            invitationData: invitationData
        )
    }

    public func installEnrollment(
        receiptData: Data
    ) async -> AssemblywrightDeveloperBridgeEnrollmentStatus? {
        await installEnrollmentDocument(
            arguments: Self.enrollmentInstallArguments,
            receiptData: receiptData,
            expectedStatus: "enrollment_installed"
        )
    }

    public func prepareCertificateRotation(invitationData: Data) async -> Data? {
        await prepareEnrollmentDocument(
            arguments: Self.rotationPrepareArguments,
            invitationData: invitationData
        )
    }

    public func installCertificateRotation(
        receiptData: Data
    ) async -> AssemblywrightDeveloperBridgeEnrollmentStatus? {
        await installEnrollmentDocument(
            arguments: Self.rotationInstallArguments,
            receiptData: receiptData,
            expectedStatus: "certificate_rotation_installed"
        )
    }

    public func performOwnerAction(
        _ action: AssemblywrightMacOwnerControlAction,
        safeReconciliationSHA256: [UInt8]? = nil,
        merged: Bool = false,
        verifiedHealthyMainSHA256: [UInt8]? = nil
    ) async {
        guard !ownerActionInProgress, let projection = status.ownerControl,
              let executableURL = configuration.executableURL,
              let expectedTeamIdentifier = configuration.expectedTeamIdentifier else { return }
        ownerActionInProgress = true
        ownerActionErrorCode = nil
        defer { ownerActionInProgress = false }
        let request: Data
        do {
            switch action {
            case .activation:
                request = try AssemblywrightMacFeatureConveyorActivationControl.activationRequest(from: projection)
            case .pause, .resume:
                request = try AssemblywrightMacFeatureConveyorActivationControl.orchestrationRequest(from: projection, action: action)
            case .cancelActiveFeature:
                request = try AssemblywrightMacFeatureConveyorActivationControl.cancelRequest(from: projection)
            case .abandonAndAdvance:
                guard let safeReconciliationSHA256 else { throw ControlError.invalidRequest }
                request = try AssemblywrightMacFeatureConveyorActivationControl.abandonRequest(
                    from: projection,
                    safeReconciliationSHA256: safeReconciliationSHA256, merged: merged,
                    verifiedHealthyMainSHA256: verifiedHealthyMainSHA256
                )
            }
            await stop()
            let validated = try validator.validate(executableURL: executableURL, expectedTeamIdentifier: expectedTeamIdentifier)
            let receipt = try await launcher.runCommand(
                executable: validated, arguments: Self.helperArguments(for: action), input: request
            )
            try AssemblywrightMacFeatureConveyorActivationControl.validateCommandReceipt(
                receipt, requestData: request, action: action
            )
            status = .init(phase: .starting)
            start()
        } catch {
            ownerActionErrorCode = error is ControlError ? "owner_action_rejected" : Self.errorCode(for: error)
            status = .init(phase: .masterOffline, errorCode: ownerActionErrorCode)
            start()
        }
    }

    public func performApprovedFeatureEnqueue(
        _ preparedRequest: AssemblywrightMacApprovedFeaturePreparedRequest
    ) async {
        guard !ownerActionInProgress else { return }
        guard pendingApprovedFeatureRecovery == nil else {
            ownerActionErrorCode = "approved_feature_reconciliation_required"
            return
        }
        guard approvedFeatureCommandConfiguration() != nil else {
            approvedFeatureReceipt = nil
            ownerActionErrorCode = "approved_feature_enqueue_rejected"
            return
        }
        ownerActionInProgress = true
        ownerActionErrorCode = nil
        approvedFeatureReceipt = nil
        defer { ownerActionInProgress = false }
        do {
            let recovery = AssemblywrightMacApprovedFeaturePendingRecovery(
                preparedRequest: preparedRequest
            )
            pendingApprovedFeatureRecovery = recovery
            approvedFeatureReceipt = try await executeApprovedFeatureRequest(recovery)
            pendingApprovedFeatureRecovery = nil
            status = .init(phase: .starting)
            start()
        } catch {
            ownerActionErrorCode = pendingApprovedFeatureRecovery == nil
                ? "approved_feature_enqueue_rejected"
                : "approved_feature_reconciliation_required"
            status = .init(phase: .masterOffline, errorCode: ownerActionErrorCode)
            start()
        }
    }

    @discardableResult
    public func performAssemblyLinePlanningAction(
        _ action: AssemblywrightMacAssemblyLinePlanningAction,
        requestData: Data
    ) async -> Data? {
        guard !ownerActionInProgress, pendingAssemblyLinePlanningMutation == nil,
              status.phase == .connected,
              let projection = status.assemblyLine,
              let (executableURL, expectedTeamIdentifier) =
                approvedFeatureCommandConfiguration() else {
            ownerActionErrorCode = pendingAssemblyLinePlanningMutation == nil
                ? "assembly_line_action_unavailable"
                : "assembly_line_reconciliation_required"
            return nil
        }
        ownerActionInProgress = true
        ownerActionErrorCode = nil
        defer { ownerActionInProgress = false }
        do {
            try AssemblywrightMacAssemblyLineOwnerControl.validateRequest(
                action: action,
                requestData: requestData,
                against: projection
            )
            await stop()
            guard status.errorCode == nil else {
                throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
            }
            let validated = try validator.validate(
                executableURL: executableURL,
                expectedTeamIdentifier: expectedTeamIdentifier
            )
            let pending = try AssemblywrightMacPendingAssemblyLinePlanningMutation(
                action: action,
                requestData: requestData
            )
            try assemblyLinePendingMutationStore.save(pending)
            pendingAssemblyLinePlanningMutation = pending
            pendingAssemblyLinePlanningAction = action
            let response = try await launcher.runCommand(
                executable: validated,
                arguments: action.helperArguments,
                input: pending.requestData
            )
            try AssemblywrightMacAssemblyLineOwnerControl.validateHelperOutput(
                action: action,
                requestData: pending.requestData,
                responseData: response
            )
            try clearPendingAssemblyLinePlanningMutation()
            let finalResponse: Data
            if action == .projectApproval {
                let repository = try JSONDecoder().decode(
                    AssemblywrightMacRepositoryCreationProjection.self,
                    from: response
                )
                finalResponse = try await performRepositoryCreationAfterApproval(
                    repositoryID: repository.repository.repositoryID,
                    executable: validated
                )
            } else {
                finalResponse = response
            }
            status = .init(phase: .starting)
            start()
            return finalResponse
        } catch {
            if pendingAssemblyLinePlanningMutation != nil,
               Self.isKnownPreEffectAssemblyLineFailure(error) {
                do {
                    try clearPendingAssemblyLinePlanningMutation()
                } catch {
                    ownerActionErrorCode = "assembly_line_pending_store_invalid"
                    status = .init(phase: .masterOffline, errorCode: ownerActionErrorCode)
                    start()
                    return nil
                }
            }
            ownerActionErrorCode = pendingAssemblyLinePlanningMutation == nil
                ? error is AssemblywrightMacAssemblyLinePendingStoreError
                    ? "assembly_line_pending_store_invalid"
                    : error is AssemblywrightMacAssemblyLineError
                        ? "assembly_line_action_rejected" : Self.errorCode(for: error)
                : "assembly_line_reconciliation_required"
            status = .init(phase: .masterOffline, errorCode: ownerActionErrorCode)
            start()
            return nil
        }
    }

    public func reconcilePendingAssemblyLinePlanningMutation() async {
        guard !ownerActionInProgress,
              let pending = pendingAssemblyLinePlanningMutation,
              let (executableURL, expectedTeamIdentifier) =
                approvedFeatureCommandConfiguration() else {
            if pendingAssemblyLinePlanningMutation != nil {
                ownerActionErrorCode = "assembly_line_reconciliation_required"
            }
            return
        }
        ownerActionInProgress = true
        ownerActionErrorCode = nil
        defer { ownerActionInProgress = false }
        do {
            await stop()
            guard status.errorCode == nil else {
                throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
            }
            let validated = try validator.validate(
                executableURL: executableURL,
                expectedTeamIdentifier: expectedTeamIdentifier
            )
            let response = try await launcher.runCommand(
                executable: validated,
                arguments: pending.action.helperArguments,
                input: pending.requestData
            )
            try AssemblywrightMacAssemblyLineOwnerControl.validateHelperOutput(
                action: pending.action,
                requestData: pending.requestData,
                responseData: response
            )
            try clearPendingAssemblyLinePlanningMutation()
            if pending.action == .projectApproval {
                let repository = try JSONDecoder().decode(
                    AssemblywrightMacRepositoryCreationProjection.self,
                    from: response
                )
                _ = try await performRepositoryCreationAfterApproval(
                    repositoryID: repository.repository.repositoryID,
                    executable: validated
                )
            }
            status = .init(phase: .starting)
            start()
        } catch {
            ownerActionErrorCode = "assembly_line_reconciliation_required"
            status = .init(phase: .masterOffline, errorCode: ownerActionErrorCode)
            start()
        }
    }

    private func performRepositoryCreationAfterApproval(
        repositoryID: UUID,
        executable: AssemblywrightDeveloperBridgeValidatedExecutable
    ) async throws -> Data {
        let request = try AssemblywrightMacAssemblyLineOwnerControl.repositoryCreationRequest(
            repositoryID: repositoryID
        )
        let pending = try AssemblywrightMacPendingAssemblyLinePlanningMutation(
            action: .repositoryCreation,
            requestData: request
        )
        try assemblyLinePendingMutationStore.save(pending)
        pendingAssemblyLinePlanningMutation = pending
        pendingAssemblyLinePlanningAction = .repositoryCreation
        let response = try await launcher.runCommand(
            executable: executable,
            arguments: AssemblywrightMacAssemblyLinePlanningAction.repositoryCreation.helperArguments,
            input: request
        )
        try AssemblywrightMacAssemblyLineOwnerControl.validateHelperOutput(
            action: .repositoryCreation,
            requestData: request,
            responseData: response
        )
        try clearPendingAssemblyLinePlanningMutation()
        return response
    }

    private func clearPendingAssemblyLinePlanningMutation() throws {
        try assemblyLinePendingMutationStore.clear()
        pendingAssemblyLinePlanningMutation = nil
        pendingAssemblyLinePlanningAction = nil
    }

    private static func isKnownPreEffectAssemblyLineFailure(_ error: Error) -> Bool {
        return switch error as? AssemblywrightDeveloperBridgeProcessError {
        case .commandRejectedBeforeEffect, .commandNotSubmitted, .launchFailed,
            .invalidExecutableSignature:
            true
        default:
            false
        }
    }

    public func selectLocalModel(
        modelID: String,
        executablePath: String,
        modelDirectoryPath: String
    ) async {
        guard !ownerActionInProgress,
              localModelSelectionState.pending == nil,
              status.phase == .connected,
              let ownerControl = status.ownerControl,
              let currentSelection = status.localModelSelection,
              let currentRelay = activeEventRelayConfiguration,
              currentRelay.mlxJobsEnabled else {
            localModelSelectionErrorCode = "local_model_selection_unavailable"
            return
        }
        let configuration = AssemblywrightMacLocalModelConfiguration(
            modelID: modelID,
            executablePath: executablePath,
            modelDirectoryPath: modelDirectoryPath,
            registryRevision: 0
        )
        do {
            try configuration.validateLocalPaths()
            guard currentSelection.modelID != modelID,
                  currentSelection.designationRevision
                    == ownerControl.ownerControlDesignationRevision,
                  currentSelection.emergencyPauseRevision
                    == ownerControl.emergencyPauseRevision else {
                throw AssemblywrightMacLocalModelSelectionError.invalidSelection
            }
            let intentData = try AssemblywrightMacLocalModelSelectionIntent(
                deviceID: currentSelection.deviceID,
                expectedRegistryRevision: currentSelection.registryRevision,
                expectedDesignationRevision: ownerControl.ownerControlDesignationRevision,
                expectedEmergencyPauseRevision: ownerControl.emergencyPauseRevision,
                modelID: modelID
            ).encodeStrict()
            let pending = AssemblywrightMacPendingLocalModelSelection(
                configuration: configuration,
                requestData: intentData
            )
            let state = AssemblywrightMacLocalModelSelectionState(
                active: localModelSelectionState.active,
                pending: pending
            )
            try localModelSelectionStore.save(state)
            localModelSelectionState = state
            await executePendingLocalModelSelection(pending, reconcileOnly: false)
        } catch {
            localModelSelectionErrorCode = "local_model_selection_rejected"
        }
    }

    public func resumePendingLocalModelSelection() async {
        guard !ownerActionInProgress,
              let pending = localModelSelectionState.pending else { return }
        await executePendingLocalModelSelection(pending, reconcileOnly: true)
    }

    private func executePendingLocalModelSelection(
        _ pending: AssemblywrightMacPendingLocalModelSelection,
        reconcileOnly: Bool
    ) async {
        guard let executableURL = configuration.executableURL,
              let expectedTeamIdentifier = configuration.expectedTeamIdentifier,
              let currentRelay = activeEventRelayConfiguration else {
            localModelSelectionErrorCode = "local_model_reconciliation_required"
            return
        }
        ownerActionInProgress = true
        localModelSelectionErrorCode = nil
        defer { ownerActionInProgress = false }
        do {
            await stop()
            let validated = try validator.validate(
                executableURL: executableURL,
                expectedTeamIdentifier: expectedTeamIdentifier
            )
            let output = try await launcher.runCommand(
                executable: validated,
                arguments: reconcileOnly
                    ? Self.localModelReconciliationArguments
                    : Self.localModelSelectionArguments,
                input: pending.requestData
            )
            let result = try AssemblywrightMacLocalModelSelectionControl.validateCommandData(
                output,
                intentData: pending.requestData
            )
            switch result {
            case let .selected(binding):
                let active = AssemblywrightMacLocalModelConfiguration(
                    modelID: binding.modelID,
                    executablePath: pending.configuration.executablePath,
                    modelDirectoryPath: pending.configuration.modelDirectoryPath,
                    registryRevision: binding.registryRevision
                )
                let nextRelay = try active.relayConfiguration(replacing: currentRelay)
                let state = AssemblywrightMacLocalModelSelectionState(active: active, pending: nil)
                try localModelSelectionStore.save(state)
                activeEventRelayConfiguration = nextRelay
                localModelSelectionState = state
                status = .init(phase: .starting)
                start()
            case let .terminalRejection(errorCode):
                let state = AssemblywrightMacLocalModelSelectionState(
                    active: localModelSelectionState.active,
                    pending: nil
                )
                try localModelSelectionStore.save(state)
                localModelSelectionState = state
                localModelSelectionErrorCode = errorCode
                status = .init(phase: .starting)
                start()
            }
        } catch {
            localModelSelectionErrorCode = "local_model_reconciliation_required"
            status = .init(
                phase: .masterOffline,
                errorCode: "local_model_reconciliation_required"
            )
        }
    }

    public func reconcilePendingApprovedFeatureEnqueue() async {
        guard !ownerActionInProgress,
              let recovery = pendingApprovedFeatureRecovery else { return }
        guard approvedFeatureCommandConfiguration() != nil else {
            ownerActionErrorCode = "approved_feature_reconciliation_required"
            return
        }
        ownerActionInProgress = true
        ownerActionErrorCode = nil
        approvedFeatureReceipt = nil
        defer { ownerActionInProgress = false }
        do {
            approvedFeatureReceipt = try await executeApprovedFeatureRequest(recovery)
            pendingApprovedFeatureRecovery = nil
            status = .init(phase: .starting)
            start()
        } catch {
            ownerActionErrorCode = "approved_feature_reconciliation_required"
            status = .init(phase: .masterOffline, errorCode: ownerActionErrorCode)
            start()
        }
    }

    nonisolated public static let approvedFeatureEnqueueArguments = [
        "feature-conveyor", "approve-and-enqueue", "--confirm"
    ]
    nonisolated public static let localModelSelectionArguments = [
        "local-model", "select", "--confirm"
    ]
    nonisolated public static let localModelReconciliationArguments = [
        "local-model", "reconcile", "--confirm"
    ]

    nonisolated public static func helperArguments(for action: AssemblywrightMacOwnerControlAction) -> [String] {
        switch action {
        case .activation: ["feature-conveyor", "activation", "--confirm"]
        case .pause: ["feature-conveyor", "orchestration", "pause", "--confirm"]
        case .resume: ["feature-conveyor", "orchestration", "resume", "--confirm"]
        case .cancelActiveFeature: ["feature-conveyor", "cancel-active-feature", "--confirm"]
        case .abandonAndAdvance: ["feature-conveyor", "abandon-and-advance", "--confirm"]
        }
    }

    nonisolated public static func status(
        from line: Data,
        localCodingSnapshotsEnabled: Bool = false
    ) throws -> AssemblywrightDeveloperBridgeAppStatus {
        let snapshot = try AssemblywrightMacBridgeSupervisorSnapshot.decodeStrict(
            line,
            localCodingSnapshotsEnabled: localCodingSnapshotsEnabled
        )
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
                deviceID: snapshot.deviceID.lowercased(),
                masterEndpoint: snapshot.masterEndpoint,
                connectionEpoch: snapshot.connectionEpoch,
                featureConveyor: snapshot.featureConveyor,
                ownerControl: snapshot.ownerControl,
                assemblyLine: snapshot.assemblyLine,
                localModelSelection: snapshot.localModelSelection
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

    private func performSetupHelperCommand(
        arguments: [String],
        input: Data,
        ambiguityCode: String
    ) async -> Data? {
        guard !ownerActionInProgress else { return nil }
        guard Self.setupHelperArguments.contains(arguments),
              !configurationStoreLoadFailed,
              let executableURL = configuration.executableURL,
              let expectedTeamIdentifier = configuration.expectedTeamIdentifier else {
            setupActionErrorCode = configurationStoreLoadFailed
                ? "developer_bridge_configuration_store_invalid"
                : "developer_bridge_not_configured"
            return nil
        }
        ownerActionInProgress = true
        setupActionErrorCode = nil
        defer { ownerActionInProgress = false }
        do {
            await stop()
            guard status.errorCode == nil else {
                throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
            }
            let validated = try validator.validate(
                executableURL: executableURL,
                expectedTeamIdentifier: expectedTeamIdentifier
            )
            let output = try await launcher.runCommand(
                executable: validated,
                arguments: arguments,
                input: input
            )
            status = .init(phase: .starting)
            start()
            return output
        } catch {
            setupActionErrorCode = Task.isCancelled
                || error as? AssemblywrightDeveloperBridgeProcessError == .commandOutcomeUnknown
                ? ambiguityCode : Self.setupErrorCode(for: error)
            status = .init(
                phase: .masterOffline,
                errorCode: setupActionErrorCode
            )
            start()
            return nil
        }
    }

    private func prepareEnrollmentDocument(
        arguments: [String],
        invitationData: Data
    ) async -> Data? {
        guard !invitationData.isEmpty,
              invitationData.count <= AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes
        else {
            setupActionErrorCode = "invalid_enrollment_document"
            return nil
        }
        guard let output = await performSetupHelperCommand(
            arguments: arguments,
            input: invitationData,
            ambiguityCode: "enrollment_prepare_recovery_required"
        ) else { return nil }
        do {
            try Self.validateEnrollmentCSR(output, matchingInvitation: invitationData)
            return output
        } catch {
            setupActionErrorCode = "invalid_helper_setup_response"
            return nil
        }
    }

    private func installEnrollmentDocument(
        arguments: [String],
        receiptData: Data,
        expectedStatus: String
    ) async -> AssemblywrightDeveloperBridgeEnrollmentStatus? {
        guard !receiptData.isEmpty,
              receiptData.count <= AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes
        else {
            setupActionErrorCode = "invalid_enrollment_document"
            return nil
        }
        guard let output = await performSetupHelperCommand(
            arguments: arguments,
            input: receiptData,
            ambiguityCode: "enrollment_install_recovery_required"
        ) else { return nil }
        do {
            return try Self.decodeEnrollmentStatus(output, expectedStatus: expectedStatus)
        } catch {
            setupActionErrorCode = "enrollment_install_recovery_required"
            return nil
        }
    }

    private static func validateEnrollmentCSR(
        _ data: Data,
        matchingInvitation invitationData: Data
    ) throws {
        let invitation = try exactJSONObject(
            invitationData,
            keys: [
                "schema_version", "status", "grant_id", "device_id", "device_name", "role",
                "registry_revision", "expires_at_ms", "capabilities", "master_endpoint",
                "ca_fingerprint_sha256"
            ]
        )
        let output = try exactJSONObject(
            data,
            keys: ["schema_version", "status", "grant_id", "device_id", "csr_pem"]
        )
        guard let schema = output["schema_version"] as? NSNumber,
              CFGetTypeID(schema) != CFBooleanGetTypeID(),
              schema.uint16Value == 1, schema.doubleValue == 1,
              output["status"] as? String == "enrollment_csr_ready",
              let grantID = output["grant_id"] as? String,
              let deviceID = output["device_id"] as? String,
              canonicalUUID(grantID), canonicalUUID(deviceID),
              grantID == invitation["grant_id"] as? String,
              deviceID == invitation["device_id"] as? String,
              let csr = output["csr_pem"] as? String,
              csr.utf8.count <= 32 * 1_024,
              !csr.contains("\0"),
              csr.hasPrefix("-----BEGIN CERTIFICATE REQUEST-----\n"),
              csr.hasSuffix("-----END CERTIFICATE REQUEST-----\n")
                || csr.hasSuffix("-----END CERTIFICATE REQUEST-----") else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
    }

    private static func decodeEnrollmentStatus(
        _ data: Data,
        expectedStatus: String?
    ) throws -> AssemblywrightDeveloperBridgeEnrollmentStatus {
        let preliminary = try exactJSONObject(data, keys: nil)
        guard let status = preliminary["status"] as? String else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        if status == "not_enrolled", expectedStatus == nil {
            guard Set(preliminary.keys) == ["status"] else {
                throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
            }
            return .init(installed: false)
        }
        let allowedStatuses = expectedStatus.map { [$0] } ?? ["enrolled"]
        guard allowedStatuses.contains(status),
              Set(preliminary.keys) == [
                "status", "device_id", "device_name", "master_endpoint",
                "registry_revision", "certificate_not_after_ms"
              ],
              let deviceID = preliminary["device_id"] as? String,
              canonicalUUID(deviceID),
              let deviceName = preliminary["device_name"] as? String,
              !deviceName.isEmpty, deviceName.utf8.count <= 128,
              deviceName.utf8.allSatisfy({ $0 >= 0x20 && $0 <= 0x7e }),
              let endpoint = preliminary["master_endpoint"] as? String,
              validStoredEndpoint(endpoint),
              let revisionNumber = preliminary["registry_revision"] as? NSNumber,
              CFGetTypeID(revisionNumber) != CFBooleanGetTypeID(),
              revisionNumber.doubleValue.rounded(.towardZero) == revisionNumber.doubleValue,
              revisionNumber.int64Value > 0,
              let expiryNumber = preliminary["certificate_not_after_ms"] as? NSNumber,
              CFGetTypeID(expiryNumber) != CFBooleanGetTypeID(),
              expiryNumber.doubleValue.rounded(.towardZero) == expiryNumber.doubleValue,
              expiryNumber.int64Value > 0 else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        return .init(
            installed: true,
            deviceID: deviceID,
            deviceName: deviceName,
            masterEndpoint: endpoint,
            registryRevision: revisionNumber.uint64Value,
            certificateNotAfterMilliseconds: expiryNumber.uint64Value
        )
    }

    private static func exactJSONObject(
        _ data: Data,
        keys: Set<String>?
    ) throws -> [String: Any] {
        guard data.count <= AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
        try scanner.validateNoDuplicateObjectKeysRecursively()
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              keys.map({ Set(object.keys) == $0 }) ?? true else {
            throw AssemblywrightDeveloperBridgeProcessError.invalidSnapshot
        }
        return object
    }

    private static func canonicalUUID(_ value: String) -> Bool {
        guard let uuid = UUID(uuidString: value) else { return false }
        return uuid.uuidString.lowercased() == value
    }

    private static func validStoredEndpoint(_ value: String) -> Bool {
        !value.isEmpty && value.utf8.count <= 255 && value.contains(":")
            && value.utf8.allSatisfy({ $0 >= 0x21 && $0 <= 0x7e })
    }

    private func approvedFeatureCommandConfiguration() -> (URL, String)? {
        guard configuration.eventRelayConfiguration.map({
            !$0.fixtureJobsEnabled && !$0.mlxJobsEnabled
                && !$0.localCodingSnapshotsEnabled
        }) ?? true,
        let executableURL = configuration.executableURL,
        let expectedTeamIdentifier = configuration.expectedTeamIdentifier else { return nil }
        return (executableURL, expectedTeamIdentifier)
    }

    private func executeApprovedFeatureRequest(
        _ recovery: AssemblywrightMacApprovedFeaturePendingRecovery
    ) async throws -> AssemblywrightMacFeatureConveyorApprovedFeatureReceipt {
        guard let (executableURL, expectedTeamIdentifier) =
            approvedFeatureCommandConfiguration() else {
            throw AssemblywrightMacApprovedFeatureAuthoringError.invalidAuthenticatedStatus
        }
        await stop()
        guard status.errorCode == nil else {
            throw AssemblywrightDeveloperBridgeProcessError.teardownFailed
        }
        let validated = try validator.validate(
            executableURL: executableURL,
            expectedTeamIdentifier: expectedTeamIdentifier
        )
        let receiptData = try await launcher.runCommand(
            executable: validated,
            arguments: Self.approvedFeatureEnqueueArguments,
            input: recovery.requestData
        )
        return try AssemblywrightMacFeatureConveyorApprovedFeatureDraft
            .validateCommandReceipt(receiptData, requestData: recovery.requestData)
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
        case .commandRejectedBeforeEffect: "assembly_line_action_rejected"
        case .commandNotSubmitted: "assembly_line_action_rejected"
        case .commandOutcomeUnknown: "assembly_line_reconciliation_required"
        case nil: "helper_unavailable"
        }
    }

    private static func setupErrorCode(for error: Error) -> String {
        if error is AssemblywrightDeveloperBridgeConfigurationStoreError {
            return "developer_bridge_configuration_rejected"
        }
        return switch error as? AssemblywrightDeveloperBridgeProcessError {
        case .invalidExecutablePath: "invalid_helper_path"
        case .invalidExecutableSignature: "invalid_helper_signature"
        case .launchFailed: "helper_launch_failed"
        case .teardownFailed: "helper_teardown_failed"
        case .outputTooLarge: "helper_output_too_large"
        case .invalidSnapshot: "invalid_helper_setup_response"
        case .helperExited: "helper_exited"
        case .commandRejectedBeforeEffect, .commandNotSubmitted:
            "developer_bridge_setup_rejected"
        case .commandOutcomeUnknown: "developer_bridge_setup_recovery_required"
        case nil: "developer_bridge_setup_unavailable"
        }
    }
}
