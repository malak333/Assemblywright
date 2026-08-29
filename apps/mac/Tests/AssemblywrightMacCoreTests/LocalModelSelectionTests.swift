import Darwin
import Foundation
import Testing
@testable import AssemblywrightMacCore

@Suite("Local model selection")
struct LocalModelSelectionTests {
    @Test("Lost POST response reconciles target while explicit rejection never reconciles")
    func ambiguityAndExplicitRejection() async throws {
        let identity = TestModelIdentity(profile: oldModelProfile())
        let projection = targetProjectionData()
        let ambiguous = TestModelConnector(outcomes: [
            .session([.failure]),
            .session([.response(.init(status: 200, body: projection))])
        ])
        let outcome = try await AssemblywrightMacLocalModelSelectionControl.performIntent(
            intentData: try modelIntentData(),
            identityStore: identity,
            connector: ambiguous
        )
        #expect(outcome == .reconciledProjection(
            try AssemblywrightMacLocalModelSelectionProjection.decodeStrict(projection)
        ))
        #expect(identity.installCount == 1)
        #expect(await ambiguous.connectCount() == 2)

        let rejectedIdentity = TestModelIdentity(profile: oldModelProfile())
        let rejected = TestModelConnector(outcomes: [
            .session([.response(.init(
                status: 409,
                body: Data(#"{"error":"local_model_selection_rejected"}"#.utf8)
            ))])
        ])
        let rejectedOutcome = try await AssemblywrightMacLocalModelSelectionControl.performIntent(
            intentData: try modelIntentData(),
            identityStore: rejectedIdentity,
            connector: rejected
        )
        let rejectedData = try rejectedOutcome.commandData
        #expect(
            try AssemblywrightMacLocalModelSelectionControl.validateCommandData(
                rejectedData,
                intentData: modelIntentData()
            ) == .terminalRejection(errorCode: "local_model_selection_rejected")
        )
        #expect(rejectedIdentity.installCount == 0)
        #expect(await rejected.connectCount() == 1)

        let serverErrorIdentity = TestModelIdentity(profile: oldModelProfile())
        let serverError = TestModelConnector(outcomes: [
            .session([.response(.init(
                status: 500,
                body: Data(#"{"error":"internal_error"}"#.utf8)
            ))]),
            .session([.response(.init(status: 200, body: targetProjectionData()))])
        ])
        let serverErrorOutcome = try await AssemblywrightMacLocalModelSelectionControl
            .performIntent(
                intentData: try modelIntentData(),
                identityStore: serverErrorIdentity,
                connector: serverError
            )
        #expect(serverErrorOutcome == .reconciledProjection(
            try AssemblywrightMacLocalModelSelectionProjection.decodeStrict(
                targetProjectionData()
            )
        ))
        #expect(await serverError.connectCount() == 2)
    }

    @Test("Install failure resumes through target proof and pre-submit failure retries once after old proof")
    func resumeRecoveryModes() async throws {
        let identity = TestModelIdentity(profile: oldModelProfile(), failInstalls: 1)
        let initial = TestModelConnector(outcomes: [
            .session([.response(.init(status: 200, body: targetReceiptData()))])
        ])
        await #expect(throws: TestModelError.self) {
            try await AssemblywrightMacLocalModelSelectionControl.performIntent(
                intentData: try modelIntentData(),
                identityStore: identity,
                connector: initial
            )
        }
        let resume = TestModelConnector(outcomes: [
            .session([.response(.init(status: 200, body: targetProjectionData()))])
        ])
        _ = try await AssemblywrightMacLocalModelSelectionControl.reconcileIntent(
            intentData: try modelIntentData(),
            identityStore: identity,
            connector: resume
        )
        #expect(identity.installCount == 1)
        #expect(identity.profile.registryRevision == 2)

        let oldIdentity = TestModelIdentity(profile: oldModelProfile())
        let oldProofThenRetry = TestModelConnector(outcomes: [
            .failure,
            .session([
                .response(.init(status: 200, body: oldProjectionData())),
                .response(.init(status: 200, body: targetReceiptData()))
            ])
        ])
        _ = try await AssemblywrightMacLocalModelSelectionControl.reconcileIntent(
            intentData: try modelIntentData(),
            identityStore: oldIdentity,
            connector: oldProofThenRetry
        )
        #expect(oldIdentity.installCount == 1)
        #expect(await oldProofThenRetry.connectCount() == 2)
    }

    @Test("Swift model ID validation matches the Rust path-free contract")
    func modelIDParity() throws {
        for valid in ["Qwen3-8B", "mlx-community/Qwen3-8B-4bit", "org//model"] {
            let intent = AssemblywrightMacLocalModelSelectionIntent(
                deviceID: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                expectedRegistryRevision: 1,
                expectedDesignationRevision: 1,
                expectedEmergencyPauseRevision: 0,
                modelID: valid
            )
            #expect(throws: Never.self) { try intent.validate() }
        }
        for invalid in [
            "", "/private/model", "file:model", "org/../model", "org/./model", "org\\model",
            "model with space", "model\twith-tab", "model\nwith-newline", "org/Qwen-é"
        ] {
            let intent = AssemblywrightMacLocalModelSelectionIntent(
                deviceID: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                expectedRegistryRevision: 1,
                expectedDesignationRevision: 1,
                expectedEmergencyPauseRevision: 0,
                modelID: invalid
            )
            #expect(throws: AssemblywrightMacLocalModelSelectionError.self) {
                try intent.validate()
            }
        }
    }

    @Test("Request and command output decoding reject extra, duplicate, and revision drift")
    func strictCommandContracts() throws {
        let intent = AssemblywrightMacLocalModelSelectionIntent(
            deviceID: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            expectedRegistryRevision: 7,
            expectedDesignationRevision: 4,
            expectedEmergencyPauseRevision: 2,
            modelID: "target-model"
        )
        let intentData = try intent.encodeStrict()
        let projection = Data(
            #"{"schema_version":1,"device_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","device_name":"owner-bridge","registry_revision":8,"designation_revision":5,"emergency_pause_revision":2,"emergency_paused":false,"model_id":"target-model"}"#.utf8
        )
        let result = try AssemblywrightMacLocalModelSelectionControl.validateCommandData(
            projection,
            intentData: intentData
        )
        let binding = try #require({
            if case let .selected(binding) = result { return binding }
            return nil
        }())
        #expect(binding.reconciled)
        #expect(binding.registryRevision == 8)

        let extra = Data(
            #"{"schema_version":1,"device_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","device_name":"owner-bridge","registry_revision":8,"designation_revision":5,"emergency_pause_revision":2,"emergency_paused":false,"model_id":"target-model","path":"/private/model"}"#.utf8
        )
        #expect(throws: AssemblywrightMacLocalModelSelectionError.self) {
            try AssemblywrightMacLocalModelSelectionControl.validateCommandData(
                extra,
                intentData: intentData
            )
        }
        let duplicate = Data(
            #"{"schema_version":1,"device_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","device_name":"owner-bridge","registry_revision":8,"designation_revision":5,"designation_revision":5,"emergency_pause_revision":2,"emergency_paused":false,"model_id":"target-model"}"#.utf8
        )
        #expect(throws: AssemblywrightMacLocalModelSelectionError.self) {
            try AssemblywrightMacLocalModelSelectionControl.validateCommandData(
                duplicate,
                intentData: intentData
            )
        }

        let terminal = terminalRejectionData(
            registryRevision: 7,
            designationRevision: 4,
            emergencyPauseRevision: 2
        )
        #expect(
            try AssemblywrightMacLocalModelSelectionControl.validateCommandData(
                terminal,
                intentData: intentData
            ) == .terminalRejection(errorCode: "local_model_selection_rejected")
        )
        var driftedTerminal = try #require(
            try JSONSerialization.jsonObject(with: terminal) as? [String: Any]
        )
        driftedTerminal["model_id"] = "different-model"
        #expect(throws: AssemblywrightMacLocalModelSelectionError.self) {
            try AssemblywrightMacLocalModelSelectionControl.validateCommandData(
                try JSONSerialization.data(withJSONObject: driftedTerminal),
                intentData: intentData
            )
        }
        driftedTerminal["model_id"] = "target-model"
        driftedTerminal["local_path"] = "/private/model"
        #expect(throws: AssemblywrightMacLocalModelSelectionError.self) {
            try AssemblywrightMacLocalModelSelectionControl.validateCommandData(
                try JSONSerialization.data(withJSONObject: driftedTerminal),
                intentData: intentData
            )
        }
    }

    @Test("Owner-private store durably publishes at mode 0600 and rejects writable targets")
    func ownerPrivateStoreAndPaths() throws {
        let root = URL(fileURLWithPath: try temporaryDirectory())
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("mlx_lm.generate")
        FileManager.default.createFile(atPath: executable.path, contents: Data("#!/bin/sh\n".utf8))
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
        let models = root.appendingPathComponent("models", isDirectory: true)
        try FileManager.default.createDirectory(at: models, withIntermediateDirectories: false)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: models.path)
        let configuration = AssemblywrightMacLocalModelConfiguration(
            modelID: "target-model",
            executablePath: executable.path,
            modelDirectoryPath: models.path,
            registryRevision: 2
        )
        try configuration.validateLocalPaths()
        let store = AssemblywrightMacLocalModelSelectionStore(
            fileURL: root.appendingPathComponent("private/state.json")
        )
        let state = AssemblywrightMacLocalModelSelectionState(active: configuration, pending: nil)
        do { try store.save(state) } catch { Issue.record("save failed: \(error)"); return }
        var metadata = stat()
        #expect(lstat(store.fileURL.path, &metadata) == 0)
        #expect(metadata.st_mode & 0o777 == 0o600)
        let reopenedStore = AssemblywrightMacLocalModelSelectionStore(fileURL: store.fileURL)
        do { #expect(try reopenedStore.load() == state) } catch {
            let raw = (try? String(contentsOf: store.fileURL, encoding: .utf8)) ?? "unreadable"
            Issue.record("load failed: \(error) raw=\(raw)"); return
        }

        let wrongExecutable = root.appendingPathComponent("python3")
        FileManager.default.createFile(
            atPath: wrongExecutable.path,
            contents: Data("#!/bin/sh\n".utf8)
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: wrongExecutable.path
        )
        let wrongExecutableConfiguration = AssemblywrightMacLocalModelConfiguration(
            modelID: "target-model",
            executablePath: wrongExecutable.path,
            modelDirectoryPath: models.path,
            registryRevision: 2
        )
        #expect(throws: AssemblywrightMacLocalModelSelectionError.self) {
            try wrongExecutableConfiguration.validateLocalPaths()
        }

        try FileManager.default.setAttributes([.posixPermissions: 0o722], ofItemAtPath: models.path)
        #expect(throws: AssemblywrightMacLocalModelSelectionError.self) {
            try configuration.validateLocalPaths()
        }
    }
}

@MainActor
@Suite("Local model selection lifecycle")
struct LocalModelSelectionLifecycleTests {
    @Test("Persisted pending selection blocks supervision until separately resumed")
    func pendingBlocksAndExactResumePromotes() async throws {
        let root = URL(fileURLWithPath: try temporaryDirectory())
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("mlx_lm.generate")
        FileManager.default.createFile(atPath: executable.path, contents: Data("#!/bin/sh\n".utf8))
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
        let models = root.appendingPathComponent("models", isDirectory: true)
        try FileManager.default.createDirectory(
            at: models,
            withIntermediateDirectories: false
        )
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: models.path)
        let intent = try AssemblywrightMacLocalModelSelectionIntent(
            deviceID: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            expectedRegistryRevision: 1,
            expectedDesignationRevision: 1,
            expectedEmergencyPauseRevision: 0,
            modelID: "target-model"
        ).encodeStrict()
        let pending = AssemblywrightMacPendingLocalModelSelection(
            configuration: .init(
                modelID: "target-model",
                executablePath: executable.path,
                modelDirectoryPath: models.path,
                registryRevision: 0
            ),
            requestData: intent
        )
        let store = AssemblywrightMacLocalModelSelectionStore(
            fileURL: root.appendingPathComponent("private/state.json")
        )
        try store.save(.init(active: nil, pending: pending))
        let launcher = LocalModelFakeLauncher(output: Data(
            #"{"schema_version":1,"device_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","device_name":"owner-bridge","registry_revision":2,"designation_revision":2,"emergency_pause_revision":0,"emergency_paused":false,"model_id":"target-model"}"#.utf8
        ))
        let configuration = AssemblywrightDeveloperBridgeProcessConfiguration(environment: [
            AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/bin/true",
            AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ",
            AssemblywrightDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey: "/bin/true",
            AssemblywrightDeveloperBridgeProcessConfiguration.agentDataDirectoryEnvironmentKey: root.path,
            AssemblywrightDeveloperBridgeProcessConfiguration.mlxJobsEnabledEnvironmentKey: "true",
            AssemblywrightDeveloperBridgeProcessConfiguration.mlxExecutableEnvironmentKey: executable.path,
            AssemblywrightDeveloperBridgeProcessConfiguration.mlxModelDirectoryEnvironmentKey: models.path,
            AssemblywrightDeveloperBridgeProcessConfiguration.mlxModelIDEnvironmentKey: "old-model"
        ])
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: configuration,
            validator: LocalModelFakeValidator(),
            launcher: launcher,
            localModelSelectionStore: store
        )
        lifecycle.start()
        #expect(await launcher.launchCount() == 0)
        #expect(lifecycle.status.errorCode == "local_model_reconciliation_required")
        await lifecycle.resumePendingLocalModelSelection()
        #expect(lifecycle.localModelSelectionState.pending == nil)
        #expect(lifecycle.localModelSelectionState.active?.modelID == "target-model")
        #expect(await launcher.commandArguments() == ["local-model", "reconcile", "--confirm"])
        #expect(await launcher.commandInput() == intent)
        await lifecycle.stop()
    }

    @Test("Unsafe, malformed, oversized, and path-invalid stores block lifecycle start")
    func invalidStoreBlocksStart() async throws {
        let root = URL(fileURLWithPath: try temporaryDirectory())
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("mlx_lm.generate")
        FileManager.default.createFile(atPath: executable.path, contents: Data("#!/bin/sh\n".utf8))
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
        let models = root.appendingPathComponent("models", isDirectory: true)
        try FileManager.default.createDirectory(at: models, withIntermediateDirectories: false)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: models.path)
        let emptyState = try JSONEncoder().encode(
            AssemblywrightMacLocalModelSelectionState(active: nil, pending: nil)
        )
        let invalidPathState = try JSONEncoder().encode(
            AssemblywrightMacLocalModelSelectionState(
                active: .init(
                    modelID: "target-model",
                    executablePath: root.appendingPathComponent("missing/mlx_lm.generate").path,
                    modelDirectoryPath: models.path,
                    registryRevision: 2
                ),
                pending: nil
            )
        )
        let cases: [(Data, NSNumber)] = [
            (emptyState, 0o644),
            (Data("not-json".utf8), 0o600),
            (Data(repeating: 0x20, count: 16 * 1_024 + 1), 0o600),
            (invalidPathState, 0o600)
        ]
        for (index, invalid) in cases.enumerated() {
            let stateURL = root.appendingPathComponent("invalid-\(index)/state.json")
            try FileManager.default.createDirectory(
                at: stateURL.deletingLastPathComponent(),
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            FileManager.default.createFile(atPath: stateURL.path, contents: invalid.0)
            try FileManager.default.setAttributes(
                [.posixPermissions: invalid.1],
                ofItemAtPath: stateURL.path
            )
            let launcher = LocalModelFakeLauncher(output: targetProjectionData())
            let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
                configuration: localModelLifecycleConfiguration(
                    root: root,
                    executable: executable,
                    models: models
                ),
                validator: LocalModelFakeValidator(),
                launcher: launcher,
                localModelSelectionStore: .init(fileURL: stateURL)
            )
            lifecycle.start()
            #expect(lifecycle.status.phase == .masterOffline)
            #expect(lifecycle.status.errorCode == "local_model_selection_store_invalid")
            #expect(lifecycle.localModelSelectionErrorCode
                == "local_model_selection_store_invalid")
            #expect(await launcher.launchCount() == 0)
        }
    }

    @Test("Post-authority relay validation cannot persist false completion")
    func relayValidationPrecedesPromotionPersistence() async throws {
        let root = URL(fileURLWithPath: try temporaryDirectory())
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("mlx_lm.generate")
        FileManager.default.createFile(atPath: executable.path, contents: Data("#!/bin/sh\n".utf8))
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
        let models = root.appendingPathComponent("models", isDirectory: true)
        try FileManager.default.createDirectory(at: models, withIntermediateDirectories: false)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: models.path)
        let pending = AssemblywrightMacPendingLocalModelSelection(
            configuration: .init(
                modelID: "target-model",
                executablePath: executable.path,
                modelDirectoryPath: models.path,
                registryRevision: 0
            ),
            requestData: try modelIntentData()
        )
        let store = AssemblywrightMacLocalModelSelectionStore(
            fileURL: root.appendingPathComponent("private/state.json")
        )
        try store.save(.init(active: nil, pending: pending))
        let launcher = LocalModelFakeLauncher(output: targetProjectionData())
        let lifecycle = AssemblywrightDeveloperBridgeProcessLifecycle(
            configuration: localModelLifecycleConfiguration(
                root: root,
                executable: executable,
                models: models
            ),
            validator: LocalModelFakeValidator(),
            launcher: launcher,
            localModelSelectionStore: store
        )
        try FileManager.default.setAttributes([.posixPermissions: 0o722], ofItemAtPath: models.path)

        await lifecycle.resumePendingLocalModelSelection()
        #expect(lifecycle.localModelSelectionState.pending == pending)
        #expect(lifecycle.localModelSelectionState.active == nil)
        #expect(lifecycle.localModelSelectionErrorCode == "local_model_reconciliation_required")
        #expect(await launcher.commandArguments()
            == AssemblywrightDeveloperBridgeProcessLifecycle.localModelReconciliationArguments)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: models.path)
        #expect(try store.load()?.pending == pending)
        #expect(try store.load()?.active == nil)
    }
}

private struct LocalModelFakeValidator: AssemblywrightDeveloperBridgeExecutableValidating {
    func validate(
        executableURL: URL,
        expectedTeamIdentifier: String
    ) throws -> AssemblywrightDeveloperBridgeValidatedExecutable {
        .init(
            executableURL: executableURL,
            teamIdentifier: expectedTeamIdentifier,
            codeRequirement: "test",
            cdHash: Data([1])
        )
    }
}

private enum TestModelError: Error { case injected }

private final class TestModelIdentity: AssemblywrightMacLocalModelIdentityStoring, @unchecked Sendable {
    private let lock = NSLock()
    private var stored: AssemblywrightMacBridgeProfile
    private var remainingFailures: Int
    private(set) var installCount = 0

    init(profile: AssemblywrightMacBridgeProfile, failInstalls: Int = 0) {
        stored = profile
        remainingFailures = failInstalls
    }

    var profile: AssemblywrightMacBridgeProfile { lock.withLock { stored } }

    func loadInstalledProfile() throws -> AssemblywrightMacBridgeProfile? {
        lock.withLock { stored }
    }

    func installLocalModelSelection(
        modelID: String,
        expectedRegistryRevision: UInt64,
        registryRevision: UInt64
    ) throws -> AssemblywrightMacBridgeProfile {
        try lock.withLock {
            if remainingFailures > 0 {
                remainingFailures -= 1
                throw TestModelError.injected
            }
            guard stored.registryRevision == expectedRegistryRevision else {
                throw TestModelError.injected
            }
            let old = stored.capabilities[0]
            stored = AssemblywrightMacBridgeProfile(
                deviceID: stored.deviceID,
                deviceName: stored.deviceName,
                role: stored.role,
                registryRevision: registryRevision,
                capabilities: [.init(
                    id: old.id, kind: old.kind, provider: old.provider, model: modelID,
                    maxContextBytes: old.maxContextBytes, maxResultBytes: old.maxResultBytes
                )],
                masterEndpoint: stored.masterEndpoint,
                certificateNotAfterMilliseconds: stored.certificateNotAfterMilliseconds
            )
            installCount += 1
            return stored
        }
    }
}

private enum TestConnectOutcome: Sendable {
    case failure
    case session([TestSessionOutcome])
}

private enum TestSessionOutcome: Sendable {
    case failure
    case response(AssemblywrightMacBridgeHTTPResponse)
}

private actor TestModelConnector: AssemblywrightMacBridgeConnecting {
    private var outcomes: [TestConnectOutcome]
    private var profiles: [AssemblywrightMacBridgeProfile] = []

    init(outcomes: [TestConnectOutcome]) { self.outcomes = outcomes }

    func connect(profile: AssemblywrightMacBridgeProfile) async throws
        -> any AssemblywrightMacBridgeSession {
        profiles.append(profile)
        guard !outcomes.isEmpty else { throw TestModelError.injected }
        switch outcomes.removeFirst() {
        case .failure: throw TestModelError.injected
        case let .session(session): return TestModelSession(outcomes: session)
        }
    }

    func connectCount() -> Int { profiles.count }
}

private actor TestModelSession: AssemblywrightMacBridgeSession {
    nonisolated let connectionEpoch: UInt64 = 1
    private var outcomes: [TestSessionOutcome]
    init(outcomes: [TestSessionOutcome]) { self.outcomes = outcomes }

    func send(_ request: AssemblywrightMacBridgeHTTPRequest) async throws
        -> AssemblywrightMacBridgeHTTPResponse {
        guard !outcomes.isEmpty else { throw TestModelError.injected }
        switch outcomes.removeFirst() {
        case .failure: throw TestModelError.injected
        case let .response(response): return response
        }
    }
    func cancel() async {}
}

private func oldModelProfile() -> AssemblywrightMacBridgeProfile {
    .init(
        deviceID: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        deviceName: "owner-bridge",
        role: "mac_bridge",
        registryRevision: 1,
        capabilities: [.init(
            id: "mlx.reasoning", kind: "local_inference", provider: "mlx",
            model: "old-model", maxContextBytes: 32 * 1_024, maxResultBytes: 32 * 1_024
        )],
        masterEndpoint: "100.64.0.1:7792",
        certificateNotAfterMilliseconds: 4_102_444_800_000
    )
}

private func modelIntentData() throws -> Data {
    try AssemblywrightMacLocalModelSelectionIntent(
        deviceID: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        expectedRegistryRevision: 1,
        expectedDesignationRevision: 1,
        expectedEmergencyPauseRevision: 0,
        modelID: "target-model"
    ).encodeStrict()
}

private func targetReceiptData() -> Data {
    Data(#"{"schema_version":1,"device_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","registry_revision":2,"designation_revision":2,"emergency_pause_revision":0,"model_id":"target-model","selected_at_ms":50,"status":"selected"}"#.utf8)
}

private func terminalRejectionData(
    registryRevision: UInt64 = 1,
    designationRevision: UInt64 = 1,
    emergencyPauseRevision: UInt64 = 0
) -> Data {
    let object: [String: Any] = [
        "schema_version": 1,
        "device_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "expected_registry_revision": registryRevision,
        "expected_designation_revision": designationRevision,
        "expected_emergency_pause_revision": emergencyPauseRevision,
        "model_id": "target-model",
        "status": "rejected",
        "error_code": "local_model_selection_rejected"
    ]
    return try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

private func targetProjectionData() -> Data {
    Data(#"{"schema_version":1,"device_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","device_name":"owner-bridge","registry_revision":2,"designation_revision":2,"emergency_pause_revision":0,"emergency_paused":false,"model_id":"target-model"}"#.utf8)
}

private func oldProjectionData() -> Data {
    Data(#"{"schema_version":1,"device_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","device_name":"owner-bridge","registry_revision":1,"designation_revision":1,"emergency_pause_revision":0,"emergency_paused":false,"model_id":"old-model"}"#.utf8)
}

private actor LocalModelFakeLauncher: AssemblywrightDeveloperBridgeProcessLaunching {
    private let output: Data
    private var launches = 0
    private var arguments: [String]?
    private var input: Data?

    init(output: Data) { self.output = output }

    func launch(
        executable _: AssemblywrightDeveloperBridgeValidatedExecutable,
        eventRelayConfiguration _: AssemblywrightMacDeveloperEventRelayConfiguration?
    ) async throws -> any AssemblywrightDeveloperBridgeProcessSession {
        launches += 1
        return LocalModelFakeSession()
    }

    func runCommand(
        executable _: AssemblywrightDeveloperBridgeValidatedExecutable,
        arguments: [String],
        input: Data
    ) async throws -> Data {
        self.arguments = arguments
        self.input = input
        return output
    }

    func launchCount() -> Int { launches }
    func commandArguments() -> [String]? { arguments }
    func commandInput() -> Data? { input }
}

private struct LocalModelFakeSession: AssemblywrightDeveloperBridgeProcessSession {
    let outputLines = AsyncThrowingStream<Data, Error> { _ in }
    func stop() async throws {}
}

private func localModelLifecycleConfiguration(
    root: URL,
    executable: URL,
    models: URL
) -> AssemblywrightDeveloperBridgeProcessConfiguration {
    AssemblywrightDeveloperBridgeProcessConfiguration(environment: [
        AssemblywrightDeveloperBridgeProcessConfiguration.executableEnvironmentKey: "/bin/true",
        AssemblywrightDeveloperBridgeProcessConfiguration.teamIdentifierEnvironmentKey: "ABCDEFGHIJ",
        AssemblywrightDeveloperBridgeProcessConfiguration.agentExecutableEnvironmentKey: executable.path,
        AssemblywrightDeveloperBridgeProcessConfiguration.agentDataDirectoryEnvironmentKey: root.path,
        AssemblywrightDeveloperBridgeProcessConfiguration.mlxJobsEnabledEnvironmentKey: "true",
        AssemblywrightDeveloperBridgeProcessConfiguration.mlxExecutableEnvironmentKey: executable.path,
        AssemblywrightDeveloperBridgeProcessConfiguration.mlxModelDirectoryEnvironmentKey: models.path,
        AssemblywrightDeveloperBridgeProcessConfiguration.mlxModelIDEnvironmentKey: "old-model"
    ])
}

private func temporaryDirectory() throws -> String {
    var template = Array("/tmp/assemblywright-model-selection.XXXXXX".utf8CString)
    guard let path = mkdtemp(&template) else { throw CocoaError(.fileWriteUnknown) }
    let value = String(cString: path)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: value)
    return URL(fileURLWithPath: value).resolvingSymlinksInPath().path
}
