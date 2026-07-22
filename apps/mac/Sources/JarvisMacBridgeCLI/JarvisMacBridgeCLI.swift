import Darwin
import Foundation
import JarvisMacCore

private enum BridgeCLIError: Error, CustomStringConvertible {
    case usage
    case inputTooLarge
    case notEnrolled
    case invalidHealth

    var description: String {
        switch self {
        case .usage:
            "Usage: jarvis-mac-bridge enrollment prepare|install | status | connect | monitor [--samples COUNT] [--interval-ms MILLISECONDS] [--reconnect-between-samples]"
        case .inputTooLarge:
            "Input exceeds the 64 KiB enrollment-document limit."
        case .notEnrolled:
            "No installed Developer Mode bridge identity is available in Keychain."
        case .invalidHealth:
            "The authenticated Windows master returned an invalid health response."
        }
    }
}

@main
private struct JarvisMacBridgeCLI {
    static func main() async {
        do {
            try await run(arguments: Array(CommandLine.arguments.dropFirst()))
        } catch {
            FileHandle.standardError.write(Data("jarvis-mac-bridge: \(error)\n".utf8))
            Darwin.exit(1)
        }
    }

    private static func run(arguments: [String]) async throws {
        let coordinator = JarvisMacEnrollmentCoordinator()
        switch arguments {
        case ["enrollment", "prepare"]:
            let invitation = try readBoundedStdin()
            try writeStdout(coordinator.prepare(invitationData: invitation))
        case ["enrollment", "install"]:
            let receipt = try readBoundedStdin()
            let profile = try coordinator.install(issuedReceiptData: receipt)
            try writeJSON([
                "status": "enrollment_installed",
                "device_id": profile.deviceID,
                "device_name": profile.deviceName,
                "master_endpoint": profile.masterEndpoint,
                "registry_revision": profile.registryRevision,
                "certificate_not_after_ms": profile.certificateNotAfterMilliseconds
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
            let session = try await JarvisMacMTLSBridgeTransport().connect(profile: profile)
            do {
                let health = try await session.send(
                    JarvisMacBridgeHTTPRequest(method: "GET", path: "/health")
                )
                guard health.status == 200,
                      let healthObject = try JSONSerialization.jsonObject(with: health.body)
                        as? [String: Any],
                      let masterStatus = healthObject["status"] as? String,
                      let masterMode = healthObject["mode"] as? String,
                      masterMode == "developer_remote_master",
                      let maintenanceActive = healthObject["maintenance_active"] as? Bool,
                      let protocolVersion = healthObject["protocol_version"] as? Int,
                      let schemaVersion = healthObject["schema_version"] as? Int else {
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
                    "protocol_version": protocolVersion,
                    "schema_version": schemaVersion
                ])
                await session.cancel()
            } catch {
                await session.cancel()
                throw error
            }
        case let arguments where arguments.first == "monitor":
            guard let profile = try coordinator.status() else { throw BridgeCLIError.notEnrolled }
            let options = try monitorOptions(Array(arguments.dropFirst()))
            let supervisor = JarvisMacBridgeSupervisor(profile: profile)
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
                throw error
            }
            await supervisor.stop()
        default:
            throw BridgeCLIError.usage
        }
    }

    private static func monitorOptions(_ arguments: [String]) throws -> (
        samples: Int?, intervalMilliseconds: UInt64, reconnectBetweenSamples: Bool
    ) {
        var samples: Int?
        var intervalMilliseconds = JarvisMacBridgeSupervisor.normalPollDelayMilliseconds
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

    private static func readBoundedStdin() throws -> Data {
        let maximum = JarvisMacEnrollmentCoordinator.maximumDocumentBytes
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
