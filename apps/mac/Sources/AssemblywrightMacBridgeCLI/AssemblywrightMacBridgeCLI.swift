import Darwin
import Foundation
import AssemblywrightMacCore

private enum BridgeCLIError: Error, CustomStringConvertible {
    case usage
    case inputTooLarge
    case notEnrolled
    case invalidHealth

    var description: String {
        switch self {
        case .usage:
            "Usage: assemblywright-mac-bridge enrollment prepare|install [--identity-profile fixture|local-coding] | enrollment rotate prepare|install --confirm | enrollment rebind prepare|stage|promote|cancel --confirm | enrollment remove --confirm --identity-profile fixture|local-coding | local-model select|reconcile --confirm | assembly-line project-draft|feature-draft|frozen-specification|approve-project|approve-feature|auto-run --confirm | feature-conveyor approve-and-enqueue|activation|orchestration pause|orchestration resume|cancel-active-feature|abandon-and-advance --confirm | status|connect [--identity-profile fixture|local-coding] | monitor|relay [--identity-profile fixture|local-coding] [--samples COUNT] [--interval-ms MILLISECONDS] [--reconnect-between-samples]"
        case .inputTooLarge:
            "Input exceeds this command's fixed document limit."
        case .notEnrolled:
            "No installed Developer Mode bridge identity is available in Keychain."
        case .invalidHealth:
            "The authenticated Windows master returned an invalid health response."
        }
    }
}

private enum AssemblyLineCLIOutcome: Error {
    case rejectedBeforeEffect
    case outcomeUnknown
}

@main
private struct AssemblywrightMacBridgeCLI {
    static func main() async {
        do {
            try await run(arguments: Array(CommandLine.arguments.dropFirst()))
        } catch AssemblyLineCLIOutcome.rejectedBeforeEffect {
            FileHandle.standardError.write(
                Data("assemblywright-mac-bridge: assembly-line request rejected\n".utf8)
            )
            Darwin.exit(AssemblywrightMacAssemblyLineHelperExitStatus.rejectedBeforeEffect)
        } catch AssemblyLineCLIOutcome.outcomeUnknown {
            FileHandle.standardError.write(
                Data("assemblywright-mac-bridge: assembly-line outcome unknown\n".utf8)
            )
            Darwin.exit(AssemblywrightMacAssemblyLineHelperExitStatus.outcomeUnknown)
        } catch {
            FileHandle.standardError.write(Data("assemblywright-mac-bridge: \(error)\n".utf8))
            Darwin.exit(1)
        }
    }

    private static func run(arguments: [String]) async throws {
        let parsed = try identityProfileArguments(arguments)
        let identityStore = KeychainAssemblywrightMacBridgeIdentityStore(
            identityProfile: parsed.profile
        )
        let coordinator = AssemblywrightMacEnrollmentCoordinator(
            identityStore: identityStore,
            identityProfile: parsed.profile
        )
        switch parsed.arguments {
        case ["enrollment", "prepare"]:
            let invitation = try readBoundedStdin(
                maximum: AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes
            )
            try writeStdout(coordinator.prepare(invitationData: invitation))
        case ["enrollment", "install"]:
            let receipt = try readBoundedStdin(
                maximum: AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes
            )
            let profile = try coordinator.install(issuedReceiptData: receipt)
            try writeJSON([
                "status": "enrollment_installed",
                "device_id": profile.deviceID,
                "device_name": profile.deviceName,
                "master_endpoint": profile.masterEndpoint,
                "registry_revision": profile.registryRevision,
                "certificate_not_after_ms": profile.certificateNotAfterMilliseconds
            ])
        case ["enrollment", "rotate", "prepare", "--confirm"]
            where parsed.profile == .standard:
            let invitation = try readBoundedStdin(
                maximum: AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes
            )
            try writeStdout(coordinator.prepareRotation(invitationData: invitation))
        case ["enrollment", "rotate", "install", "--confirm"]
            where parsed.profile == .standard:
            let receipt = try readBoundedStdin(
                maximum: AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes
            )
            let profile = try coordinator.installRotation(issuedReceiptData: receipt)
            try writeJSON([
                "status": "certificate_rotation_installed",
                "device_id": profile.deviceID,
                "device_name": profile.deviceName,
                "master_endpoint": profile.masterEndpoint,
                "registry_revision": profile.registryRevision,
                "certificate_not_after_ms": profile.certificateNotAfterMilliseconds
            ])
        case ["enrollment", "rebind", "prepare", "--confirm"]
            where parsed.profile == .standard:
            let invitation = try readBoundedStdin(
                maximum: AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes
            )
            try writeStdout(coordinator.prepareCapabilityRebind(invitationData: invitation))
        case ["enrollment", "rebind", "stage", "--confirm"]
            where parsed.profile == .standard:
            let receipt = try readBoundedStdin(
                maximum: AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes
            )
            try writeStdout(coordinator.stageCapabilityRebind(issuedReceiptData: receipt))
        case ["enrollment", "rebind", "promote", "--confirm"]
            where parsed.profile == .standard:
            let activation = try readBoundedStdin(
                maximum: AssemblywrightMacEnrollmentCoordinator.maximumDocumentBytes
            )
            let profile = try coordinator.promoteCapabilityRebind(activationData: activation)
            try writeJSON([
                "status": "capability_rebind_promoted",
                "device_id": profile.deviceID,
                "device_name": profile.deviceName,
                "master_endpoint": profile.masterEndpoint,
                "registry_revision": profile.registryRevision,
                "certificate_not_after_ms": profile.certificateNotAfterMilliseconds
            ])
        case ["enrollment", "rebind", "cancel", "--confirm"]
            where parsed.profile == .standard:
            try coordinator.cancelCapabilityRebind()
            try writeJSON(["status": "capability_rebind_stage_removed"])
        case ["enrollment", "remove", "--confirm"]
            where parsed.profile == .fixtureReasoning || parsed.profile == .localCoding:
            try identityStore.removeIsolatedIdentity()
            let status = parsed.profile == .fixtureReasoning
                ? "fixture_identity_removed" : "local_coding_identity_removed"
            try writeJSON([
                "status": status,
                "identity_profile": parsed.profile.rawValue
            ])
        case ["status"]:
            guard let profile = try coordinator.status() else {
                try writeJSON(["status": "not_enrolled"])
                return
            }
            try writeJSON([
                "status": "enrolled",
                "device_id": profile.deviceID,
                "device_name": profile.deviceName,
                "master_endpoint": profile.masterEndpoint,
                "registry_revision": profile.registryRevision,
                "certificate_not_after_ms": profile.certificateNotAfterMilliseconds
            ])
        case ["connect"]:
            guard let profile = try coordinator.status() else { throw BridgeCLIError.notEnrolled }
            let session = try await AssemblywrightMacMTLSBridgeTransport(
                factory: NetworkAssemblywrightMacTLSChannelFactory(identityStore: identityStore)
            ).connect(profile: profile)
            do {
                let health = try await session.send(
                    AssemblywrightMacBridgeHTTPRequest(method: "GET", path: "/health")
                )
                guard health.status == 200,
                      let healthObject = try JSONSerialization.jsonObject(with: health.body)
                        as? [String: Any],
                      let masterStatus = healthObject["status"] as? String,
                      let masterMode = healthObject["mode"] as? String,
                      masterMode == "developer_remote_master",
                      let maintenanceActive = healthObject["maintenance_active"] as? Bool,
                      let emergencyPaused = healthObject["emergency_paused"] as? Bool,
                      let protocolVersion = healthObject["protocol_version"] as? Int,
                      let schemaVersion = healthObject["schema_version"] as? Int,
                      masterStatus == (
                        maintenanceActive ? "maintenance" : emergencyPaused ? "paused" : "ok"
                      ) else {
                    throw BridgeCLIError.invalidHealth
                }
                try writeJSON([
                    "status": "authenticated",
                    "device_id": profile.deviceID,
                    "master_endpoint": profile.masterEndpoint,
                    "connection_epoch": session.connectionEpoch,
                    "master_status": masterStatus,
                    "master_mode": masterMode,
                    "maintenance_active": maintenanceActive,
                    "emergency_paused": emergencyPaused,
                    "protocol_version": protocolVersion,
                    "schema_version": schemaVersion
                ])
                await session.cancel()
            } catch {
                await session.cancel()
                throw error
            }
        case ["feature-conveyor", "approve-and-enqueue", "--confirm"]
            where parsed.profile == .standard:
            guard let profile = try coordinator.status() else { throw BridgeCLIError.notEnrolled }
            let request = try readBoundedStdin(
                maximum: AssemblywrightMacFeatureConveyorOwnerControl.maximumRequestBytes
            )
            let session = try await AssemblywrightMacMTLSBridgeTransport(
                factory: NetworkAssemblywrightMacTLSChannelFactory(identityStore: identityStore)
            ).connect(profile: profile)
            let receipt = try await AssemblywrightMacFeatureConveyorOwnerControl
                .approveAndEnqueue(requestData: request, using: session)
            try writeEncodableJSON(receipt)
        case ["local-model", "select", "--confirm"] where parsed.profile == .standard:
            let request = try readBoundedStdin(
                maximum: AssemblywrightMacLocalModelSelectionControl.maximumFrameBytes
            )
            let outcome = try await AssemblywrightMacLocalModelSelectionControl.performIntent(
                intentData: request,
                identityStore: identityStore,
                connector: AssemblywrightMacDefaultBridgeConnector(
                    transport: AssemblywrightMacMTLSBridgeTransport(
                        factory: NetworkAssemblywrightMacTLSChannelFactory(
                            identityStore: identityStore
                        )
                    )
                )
            )
            try writeStdout(outcome.commandData)
        case ["local-model", "reconcile", "--confirm"] where parsed.profile == .standard:
            let intent = try readBoundedStdin(
                maximum: AssemblywrightMacLocalModelSelectionControl.maximumFrameBytes
            )
            let outcome = try await AssemblywrightMacLocalModelSelectionControl.reconcileIntent(
                intentData: intent,
                identityStore: identityStore,
                connector: AssemblywrightMacDefaultBridgeConnector(
                    transport: AssemblywrightMacMTLSBridgeTransport(
                        factory: NetworkAssemblywrightMacTLSChannelFactory(
                            identityStore: identityStore
                        )
                    )
                )
            )
            try writeStdout(outcome.commandData)
        case let arguments where parsed.profile == .standard
            && assemblyLinePlanningAction(arguments) != nil:
            guard let action = assemblyLinePlanningAction(arguments) else {
                throw BridgeCLIError.usage
            }
            let receipt: Data
            do {
                guard let profile = try coordinator.status() else {
                    throw BridgeCLIError.notEnrolled
                }
                let request = try readBoundedStdin(
                    maximum: AssemblywrightMacAssemblyLineOwnerControl.maximumRequestBytes
                )
                let session = try await AssemblywrightMacMTLSBridgeTransport(
                    factory: NetworkAssemblywrightMacTLSChannelFactory(identityStore: identityStore)
                ).connect(profile: profile)
                receipt = try await AssemblywrightMacAssemblyLineOwnerControl.perform(
                    action: action,
                    requestData: request,
                    using: session
                )
            } catch let error as AssemblywrightMacAssemblyLineError {
                switch error {
                case .outcomeUnknown, .ambiguous, .invalidReceipt:
                    throw AssemblyLineCLIOutcome.outcomeUnknown
                case .invalidRequest, .requestTooLarge, .invalidProjection, .rejected:
                    throw AssemblyLineCLIOutcome.rejectedBeforeEffect
                }
            } catch {
                throw AssemblyLineCLIOutcome.rejectedBeforeEffect
            }
            do {
                try writeStdout(receipt)
            } catch {
                throw AssemblyLineCLIOutcome.outcomeUnknown
            }
        case let arguments where parsed.profile == .standard && ownerControlAction(arguments) != nil:
            guard let action = ownerControlAction(arguments) else { throw BridgeCLIError.usage }
            guard let profile = try coordinator.status() else { throw BridgeCLIError.notEnrolled }
            let request = try readBoundedStdin(
                maximum: AssemblywrightMacFeatureConveyorActivationControl.maximumFrameBytes
            )
            let session = try await AssemblywrightMacMTLSBridgeTransport(
                factory: NetworkAssemblywrightMacTLSChannelFactory(identityStore: identityStore)
            ).connect(profile: profile)
            let receipt = try await AssemblywrightMacFeatureConveyorActivationControl.perform(
                action: action, requestData: request, using: session
            )
            try writeStdout(receipt)
        case let arguments where arguments.first == "monitor":
            guard let profile = try coordinator.status() else { throw BridgeCLIError.notEnrolled }
            let options = try monitorOptions(Array(arguments.dropFirst()))
            try await runMonitor(
                profile: profile,
                options: options,
                eventRelay: nil,
                identityStore: identityStore
            )
        case let arguments where arguments.first == "relay":
            guard let profile = try coordinator.status() else { throw BridgeCLIError.notEnrolled }
            let configuration = try AssemblywrightMacDeveloperEventRelayConfiguration
                .decodeStartupDocument(
                    readBoundedStdin(
                        maximum: AssemblywrightMacDeveloperEventRelayConfiguration.maximumDocumentBytes
                    )
            )
            let options = try monitorOptions(Array(arguments.dropFirst()))
            let relay = AssemblywrightMacDeveloperEventRelay(
                configuration: configuration,
                deviceID: configuration.fixtureJobsEnabled || configuration.mlxJobsEnabled
                    || configuration.localCodingSnapshotsEnabled
                    ? UUID(uuidString: profile.deviceID)
                    : nil
            )
            try await runMonitor(
                profile: profile,
                options: options,
                eventRelay: relay,
                identityStore: identityStore
            )
        default:
            throw BridgeCLIError.usage
        }
    }

    private static func ownerControlAction(
        _ arguments: [String]
    ) -> AssemblywrightMacOwnerControlAction? {
        switch arguments {
        case ["feature-conveyor", "activation", "--confirm"]: .activation
        case ["feature-conveyor", "orchestration", "pause", "--confirm"]: .pause
        case ["feature-conveyor", "orchestration", "resume", "--confirm"]: .resume
        case ["feature-conveyor", "cancel-active-feature", "--confirm"]: .cancelActiveFeature
        case ["feature-conveyor", "abandon-and-advance", "--confirm"]: .abandonAndAdvance
        default: nil
        }
    }

    private static func assemblyLinePlanningAction(
        _ arguments: [String]
    ) -> AssemblywrightMacAssemblyLinePlanningAction? {
        AssemblywrightMacAssemblyLinePlanningAction.allCases.first(where: {
            $0.helperArguments == arguments
        })
    }

    private static func identityProfileArguments(
        _ arguments: [String]
    ) throws -> (arguments: [String], profile: AssemblywrightMacBridgeIdentityProfile) {
        var remaining: [String] = []
        var profile = AssemblywrightMacBridgeIdentityProfile.standard
        var selected = false
        var index = 0
        while index < arguments.count {
            if arguments[index] == "--identity-profile" {
                guard !selected, index + 1 < arguments.count,
                      let selectedProfile = AssemblywrightMacBridgeIdentityProfile(
                        selector: arguments[index + 1]
                      ), selectedProfile != .standard else {
                    throw BridgeCLIError.usage
                }
                selected = true
                profile = selectedProfile
                index += 2
            } else {
                remaining.append(arguments[index])
                index += 1
            }
        }
        return (remaining, profile)
    }

    private static func monitorOptions(_ arguments: [String]) throws -> (
        samples: Int?, intervalMilliseconds: UInt64, reconnectBetweenSamples: Bool
    ) {
        var samples: Int?
        var intervalMilliseconds = AssemblywrightMacBridgeSupervisor.normalPollDelayMilliseconds
        var reconnectBetweenSamples = false
        var index = 0
        while index < arguments.count {
            if arguments[index] == "--reconnect-between-samples" {
                guard !reconnectBetweenSamples else { throw BridgeCLIError.usage }
                reconnectBetweenSamples = true
                index += 1
                continue
            }
            guard index + 1 < arguments.count else { throw BridgeCLIError.usage }
            let value = arguments[index + 1]
            switch arguments[index] {
            case "--samples":
                guard samples == nil, let parsed = Int(value), (1 ... 10_000).contains(parsed) else {
                    throw BridgeCLIError.usage
                }
                samples = parsed
            case "--interval-ms":
                guard let parsed = UInt64(value), (100 ... 60_000).contains(parsed) else {
                    throw BridgeCLIError.usage
                }
                intervalMilliseconds = parsed
            default:
                throw BridgeCLIError.usage
            }
            index += 2
        }
        guard !reconnectBetweenSamples || (samples ?? 0) >= 2 else {
            throw BridgeCLIError.usage
        }
        return (samples, intervalMilliseconds, reconnectBetweenSamples)
    }

    private static func runMonitor(
        profile: AssemblywrightMacBridgeProfile,
        options: (samples: Int?, intervalMilliseconds: UInt64, reconnectBetweenSamples: Bool),
        eventRelay: (any AssemblywrightMacBridgeEventRelaying)?,
        identityStore: KeychainAssemblywrightMacBridgeIdentityStore
    ) async throws {
        let supervisor = AssemblywrightMacBridgeSupervisor(
            profile: profile,
            connector: AssemblywrightMacDefaultBridgeConnector(
                transport: AssemblywrightMacMTLSBridgeTransport(
                    factory: NetworkAssemblywrightMacTLSChannelFactory(identityStore: identityStore)
                )
            ),
            eventRelay: eventRelay
        )
        var completedSamples = 0
        do {
            while options.samples.map({ completedSamples < $0 }) ?? true {
                let snapshot = await supervisor.sample()
                try writeEncodableJSON(snapshot)
                completedSamples += 1
                if let samples = options.samples, completedSamples >= samples { break }
                if options.reconnectBetweenSamples, snapshot.phase == .authenticated {
                    await supervisor.reconnectBeforeNextSample()
                }
                let delay = snapshot.phase == .authenticated
                    ? options.intervalMilliseconds
                    : snapshot.nextDelayMilliseconds
                try await Task.sleep(for: .milliseconds(delay))
            }
        } catch {
            await supervisor.stop()
            try? await eventRelay?.stop()
            throw error
        }
        await supervisor.stop()
        try await eventRelay?.stop()
    }

    private static func readBoundedStdin(maximum: Int) throws -> Data {
        var input = Data()
        while true {
            let remaining = maximum + 1 - input.count
            guard remaining > 0 else { throw BridgeCLIError.inputTooLarge }
            let chunk = FileHandle.standardInput.readData(ofLength: min(64 * 1_024, remaining))
            if chunk.isEmpty { break }
            input.append(chunk)
        }
        guard input.count <= maximum else { throw BridgeCLIError.inputTooLarge }
        return input
    }

    private static func writeStdout(_ data: Data) throws {
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data("\n".utf8))
    }

    private static func writeJSON(_ object: [String: Any]) throws {
        let data = try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        try writeStdout(data)
    }

    private static func writeEncodableJSON(_ value: some Encodable) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        try writeStdout(encoder.encode(value))
    }
}
