import Combine
import Darwin
import Foundation
import Security

public enum JarvisDeveloperBridgeAppPhase: String, Equatable, Sendable {
    case disabled
    case starting
    case connected
    case masterOffline = "master_offline"
    case maintenance
    case stopped
}

public struct JarvisDeveloperBridgeAppStatus: Equatable, Sendable {
    public let phase: JarvisDeveloperBridgeAppPhase
    public let masterEndpoint: String?
    public let connectionEpoch: UInt64?
    public let errorCode: String?

    public init(
        phase: JarvisDeveloperBridgeAppPhase,
        masterEndpoint: String? = nil,
        connectionEpoch: UInt64? = nil,
        errorCode: String? = nil
    ) {
        self.phase = phase
        self.masterEndpoint = masterEndpoint
        self.connectionEpoch = connectionEpoch
        self.errorCode = errorCode
    }

    public static let disabled = Self(phase: .disabled)
}

public struct JarvisDeveloperBridgeProcessConfiguration: Equatable, Sendable {
    public static let executableEnvironmentKey = "JARVIS_MAC_DEVELOPER_BRIDGE_EXECUTABLE"
    public static let teamIdentifierEnvironmentKey =
        "JARVIS_MAC_DEVELOPER_BRIDGE_TEAM_IDENTIFIER"
    public let executableURL: URL?
    public let expectedTeamIdentifier: String?

    public init(environment: [String: String] = ProcessInfo.processInfo.environment) {
        guard let value = environment[Self.executableEnvironmentKey], !value.isEmpty,
              let teamIdentifier = environment[Self.teamIdentifierEnvironmentKey],
              Self.isValidTeamIdentifier(teamIdentifier) else {
            executableURL = nil
            expectedTeamIdentifier = nil
            return
        }
        executableURL = URL(fileURLWithPath: value)
        expectedTeamIdentifier = teamIdentifier
    }

    private static func isValidTeamIdentifier(_ value: String) -> Bool {
        value.utf8.count == 10 && value.utf8.allSatisfy({
            (0x41 ... 0x5a).contains($0) || (0x30 ... 0x39).contains($0)
        })
    }
}

public enum JarvisDeveloperBridgeProcessError: Error, Equatable, Sendable {
    case invalidExecutablePath
    case invalidExecutableSignature
    case launchFailed
    case teardownFailed
    case outputTooLarge
    case invalidSnapshot
    case helperExited
}

public struct JarvisDeveloperBridgeValidatedExecutable: Equatable, Sendable {
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

public protocol JarvisDeveloperBridgeExecutableValidating: Sendable {
    func validate(
        executableURL: URL,
        expectedTeamIdentifier: String
    ) throws -> JarvisDeveloperBridgeValidatedExecutable
}

public struct SecurityJarvisDeveloperBridgeExecutableValidator:
    JarvisDeveloperBridgeExecutableValidating, Sendable
{
    public static let helperIdentifier = "com.nobiletechnology.jarvis.developer-bridge.cli"
    private static let maximumPathBytes = 4 * 1_024

    public init() {}

    public func validate(
        executableURL: URL,
        expectedTeamIdentifier: String
    ) throws -> JarvisDeveloperBridgeValidatedExecutable {
        let standardized = executableURL.standardizedFileURL
        guard standardized.isFileURL,
              standardized.path.hasPrefix("/"),
              !standardized.path.contains("\0"),
              standardized.path.utf8.count <= Self.maximumPathBytes else {
            throw JarvisDeveloperBridgeProcessError.invalidExecutablePath
        }
        var metadata = stat()
        guard lstat(standardized.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFREG,
              access(standardized.path, X_OK) == 0 else {
            throw JarvisDeveloperBridgeProcessError.invalidExecutablePath
        }

        var code: SecStaticCode?
        guard SecStaticCodeCreateWithPath(standardized as CFURL, [], &code) == errSecSuccess,
              let code else {
            throw JarvisDeveloperBridgeProcessError.invalidExecutableSignature
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
            throw JarvisDeveloperBridgeProcessError.invalidExecutableSignature
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
            throw JarvisDeveloperBridgeProcessError.invalidExecutableSignature
        }
        return JarvisDeveloperBridgeValidatedExecutable(
            executableURL: standardized,
            teamIdentifier: teamIdentifier,
            codeRequirement: requirementText,
            cdHash: cdHash
        )
    }
}

public protocol JarvisDeveloperBridgeRunningProcessValidating: Sendable {
    func validate(
        processIdentifier: Int32,
        expected: JarvisDeveloperBridgeValidatedExecutable
    ) throws
}

public struct SecurityJarvisDeveloperBridgeRunningProcessValidator:
    JarvisDeveloperBridgeRunningProcessValidating, Sendable
{
    public init() {}

    public func validate(
        processIdentifier: Int32,
        expected: JarvisDeveloperBridgeValidatedExecutable
    ) throws {
        let attributes = [
            kSecGuestAttributePid as String: NSNumber(value: processIdentifier)
        ] as CFDictionary
        var runningCode: SecCode?
        guard SecCodeCopyGuestWithAttributes(nil, attributes, [], &runningCode) == errSecSuccess,
              let runningCode else {
            throw JarvisDeveloperBridgeProcessError.invalidExecutableSignature
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
            throw JarvisDeveloperBridgeProcessError.invalidExecutableSignature
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
            throw JarvisDeveloperBridgeProcessError.invalidExecutableSignature
        }
    }
}

public protocol JarvisDeveloperBridgeProcessSession: Sendable {
    var outputLines: AsyncThrowingStream<Data, Error> { get }
    func stop() async throws
}

public protocol JarvisDeveloperBridgeProcessLaunching: Sendable {
    func launch(
        executable: JarvisDeveloperBridgeValidatedExecutable
    ) async throws -> any JarvisDeveloperBridgeProcessSession
}

public struct FoundationJarvisDeveloperBridgeProcessLauncher:
    JarvisDeveloperBridgeProcessLaunching, Sendable
{
    private let runningProcessValidator: any JarvisDeveloperBridgeRunningProcessValidating

    public init(
        runningProcessValidator: any JarvisDeveloperBridgeRunningProcessValidating =
            SecurityJarvisDeveloperBridgeRunningProcessValidator()
    ) {
        self.runningProcessValidator = runningProcessValidator
    }

    public func launch(
        executable: JarvisDeveloperBridgeValidatedExecutable
    ) async throws -> any JarvisDeveloperBridgeProcessSession {
        try FoundationJarvisDeveloperBridgeProcessSession(
            executable: executable,
            runningProcessValidator: runningProcessValidator
        )
    }
}

private actor FoundationJarvisDeveloperBridgeProcessSession:
    JarvisDeveloperBridgeProcessSession
{
    private let process: Process
    private let pipe: Pipe
    private let reader: Task<Void, Never>
    nonisolated let outputLines: AsyncThrowingStream<Data, Error>
    private var stopped = false

    init(
        executable: JarvisDeveloperBridgeValidatedExecutable,
        runningProcessValidator: any JarvisDeveloperBridgeRunningProcessValidating
    ) throws {
        let process = Process()
        let pipe = Pipe()
        var continuation: AsyncThrowingStream<Data, Error>.Continuation!
        outputLines = AsyncThrowingStream(bufferingPolicy: .bufferingOldest(1)) {
            continuation = $0
        }
        self.process = process
        self.pipe = pipe

        process.executableURL = executable.executableURL
        process.arguments = ["monitor"]
        process.environment = [:]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            throw JarvisDeveloperBridgeProcessError.launchFailed
        }

        do {
            try runningProcessValidator.validate(
                processIdentifier: process.processIdentifier,
                expected: executable
            )
        } catch {
            try? pipe.fileHandleForReading.close()
            guard Self.killAndReapRejectedProcess(process) else {
                throw JarvisDeveloperBridgeProcessError.teardownFailed
            }
            throw JarvisDeveloperBridgeProcessError.invalidExecutableSignature
        }

        reader = Task.detached {
            var pending = Data()
            while !Task.isCancelled {
                let chunk = pipe.fileHandleForReading.availableData
                if chunk.isEmpty { break }
                pending.append(chunk)
                if pending.count > JarvisDeveloperBridgeProcessLifecycle.maximumBufferedBytes {
                    continuation.finish(throwing: JarvisDeveloperBridgeProcessError.outputTooLarge)
                    return
                }
                while let newline = pending.firstIndex(of: 0x0a) {
                    var line = pending.prefix(upTo: newline)
                    pending.removeSubrange(...newline)
                    if line.last == 0x0d { line = line.dropLast() }
                    guard !line.isEmpty,
                          line.count <= JarvisDeveloperBridgeProcessLifecycle.maximumLineBytes else {
                        continuation.finish(throwing: JarvisDeveloperBridgeProcessError.invalidSnapshot)
                        return
                    }
                    switch continuation.yield(Data(line)) {
                    case .enqueued:
                        break
                    case .dropped:
                        continuation.finish(
                            throwing: JarvisDeveloperBridgeProcessError.outputTooLarge
                        )
                        return
                    case .terminated:
                        return
                    @unknown default:
                        continuation.finish(
                            throwing: JarvisDeveloperBridgeProcessError.invalidSnapshot
                        )
                        return
                    }
                }
            }
            guard Task.isCancelled else {
                continuation.finish(throwing: JarvisDeveloperBridgeProcessError.helperExited)
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
                throw JarvisDeveloperBridgeProcessError.teardownFailed
            }
            await waitForProcessExit(until: .now + .seconds(1))
        }
        guard !process.isRunning else {
            throw JarvisDeveloperBridgeProcessError.teardownFailed
        }
        process.waitUntilExit()
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
        guard !process.isRunning else { return false }
        process.waitUntilExit()
        return true
    }

}

@MainActor
public final class JarvisDeveloperBridgeProcessLifecycle: ObservableObject {
    nonisolated public static let maximumLineBytes = 16 * 1_024
    nonisolated public static let maximumBufferedBytes = maximumLineBytes + 1
    nonisolated public static let proofBoundary =
        "Read-only Developer Mode bridge health. This does not enable distributed commands, jobs, models, repositories, Codex, or Git authority."

    @Published public private(set) var status: JarvisDeveloperBridgeAppStatus

    private let configuration: JarvisDeveloperBridgeProcessConfiguration
    private let validator: any JarvisDeveloperBridgeExecutableValidating
    private let launcher: any JarvisDeveloperBridgeProcessLaunching
    private var task: Task<Void, Never>?
    private var session: (any JarvisDeveloperBridgeProcessSession)?

    public init(
        configuration: JarvisDeveloperBridgeProcessConfiguration = .init(),
        validator: any JarvisDeveloperBridgeExecutableValidating =
            SecurityJarvisDeveloperBridgeExecutableValidator(),
        launcher: any JarvisDeveloperBridgeProcessLaunching =
            FoundationJarvisDeveloperBridgeProcessLauncher()
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
                let launched = try await launcher.launch(executable: validated)
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

    nonisolated public static func status(from line: Data) throws -> JarvisDeveloperBridgeAppStatus {
        let snapshot = try JarvisMacBridgeSupervisorSnapshot.decodeStrict(line)
        switch snapshot.phase {
        case .authenticated:
            return JarvisDeveloperBridgeAppStatus(
                phase: snapshot.maintenanceActive == true ? .maintenance : .connected,
                masterEndpoint: snapshot.masterEndpoint,
                connectionEpoch: snapshot.connectionEpoch
            )
        case .backingOff:
            return JarvisDeveloperBridgeAppStatus(
                phase: .masterOffline,
                errorCode: snapshot.errorCode
            )
        case .stopped:
            return JarvisDeveloperBridgeAppStatus(phase: .stopped)
        }
    }

    private func launchedSessionStop() async throws {
        guard let launched = session else { return }
        try await launched.stop()
        session = nil
    }

    private static func errorCode(for error: Error) -> String {
        switch error as? JarvisDeveloperBridgeProcessError {
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
