import Darwin
import Foundation

public enum AssemblywrightMacLocalModelSelectionError: Error, Equatable, Sendable {
    case invalidSelection
    case invalidLocalPath
    case unavailable
    case rejected
    case reconciliationRequired
    case unsafeStore
}

public protocol AssemblywrightMacLocalModelIdentityStoring: Sendable {
    func loadInstalledProfile() throws -> AssemblywrightMacBridgeProfile?
    func installLocalModelSelection(
        modelID: String,
        expectedRegistryRevision: UInt64,
        registryRevision: UInt64
    ) throws -> AssemblywrightMacBridgeProfile
}

extension KeychainAssemblywrightMacBridgeIdentityStore:
    AssemblywrightMacLocalModelIdentityStoring {}

public struct AssemblywrightMacLocalModelConfiguration: Codable, Equatable, Sendable {
    public let modelID: String
    public let executablePath: String
    public let modelDirectoryPath: String
    public let registryRevision: UInt64

    public init(
        modelID: String,
        executablePath: String,
        modelDirectoryPath: String,
        registryRevision: UInt64
    ) {
        self.modelID = modelID
        self.executablePath = executablePath
        self.modelDirectoryPath = modelDirectoryPath
        self.registryRevision = registryRevision
    }

    public func validateLocalPaths() throws {
        guard !modelID.isEmpty, modelID.utf8.count <= 128,
              validLocalModelID(modelID),
              URL(fileURLWithPath: executablePath).lastPathComponent == "mlx_lm.generate",
              executablePath.utf8.count <= 4 * 1_024,
              modelDirectoryPath.utf8.count <= 4 * 1_024 else {
            throw AssemblywrightMacLocalModelSelectionError.invalidSelection
        }
        try Self.validate(path: executablePath, kind: S_IFREG, executable: true)
        try Self.validate(path: modelDirectoryPath, kind: S_IFDIR, executable: false)
    }

    private static func validate(path: String, kind: mode_t, executable: Bool) throws {
        guard path.hasPrefix("/"), !path.contains("\0"),
              !path.split(separator: "/").contains("..") else {
            throw AssemblywrightMacLocalModelSelectionError.invalidLocalPath
        }
        let url = URL(fileURLWithPath: path)
        let canonical = url.resolvingSymlinksInPath().standardizedFileURL.path
        guard canonical == url.standardizedFileURL.path else {
            throw AssemblywrightMacLocalModelSelectionError.invalidLocalPath
        }
        var metadata = stat()
        guard lstat(path, &metadata) == 0,
              metadata.st_mode & S_IFMT == kind,
              metadata.st_uid == getuid(),
              metadata.st_mode & 0o022 == 0,
              !executable || access(path, X_OK) == 0 else {
            throw AssemblywrightMacLocalModelSelectionError.invalidLocalPath
        }
    }

    func relayConfiguration(
        replacing current: AssemblywrightMacDeveloperEventRelayConfiguration
    ) throws -> AssemblywrightMacDeveloperEventRelayConfiguration {
        try validateLocalPaths()
        let selected = AssemblywrightMacDeveloperEventRelayConfiguration(
            agentExecutableURL: current.agentExecutableURL,
            agentDataDirectoryURL: current.agentDataDirectoryURL,
            mlxJobsEnabled: true,
            mlxExecutableURL: URL(fileURLWithPath: executablePath),
            mlxModelDirectoryURL: URL(fileURLWithPath: modelDirectoryPath, isDirectory: true),
            mlxModelID: modelID
        )
        try selected.validatePaths()
        return selected
    }
}

public struct AssemblywrightMacPendingLocalModelSelection: Codable, Equatable, Sendable {
    public let configuration: AssemblywrightMacLocalModelConfiguration
    public let requestData: Data

    public init(configuration: AssemblywrightMacLocalModelConfiguration, requestData: Data) {
        self.configuration = configuration
        self.requestData = requestData
    }

    fileprivate func validatePersistedBinding() throws {
        let intent: AssemblywrightMacLocalModelSelectionIntent
        let canonicalRequest: Data
        do {
            intent = try AssemblywrightMacLocalModelSelectionIntent.decodeStrict(requestData)
            try intent.validate()
            canonicalRequest = try intent.encodeStrict()
            try configuration.validateLocalPaths()
        } catch {
            throw AssemblywrightMacLocalModelSelectionError.unsafeStore
        }
        guard configuration.registryRevision == 0,
              configuration.modelID == intent.modelID,
              requestData == canonicalRequest else {
            throw AssemblywrightMacLocalModelSelectionError.unsafeStore
        }
    }
}

public struct AssemblywrightMacLocalModelSelectionState: Codable, Equatable, Sendable {
    public let schemaVersion: UInt16
    public let active: AssemblywrightMacLocalModelConfiguration?
    public let pending: AssemblywrightMacPendingLocalModelSelection?

    public init(
        active: AssemblywrightMacLocalModelConfiguration?,
        pending: AssemblywrightMacPendingLocalModelSelection?
    ) {
        schemaVersion = 1
        self.active = active
        self.pending = pending
    }

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case active, pending
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(schemaVersion, forKey: .schemaVersion)
        try container.encode(active, forKey: .active)
        try container.encode(pending, forKey: .pending)
    }
}

public struct AssemblywrightMacLocalModelSelectionStore: Sendable {
    public let fileURL: URL

    public init(fileURL: URL = Self.defaultURL()) {
        self.fileURL = fileURL.standardizedFileURL
    }

    public static func defaultURL() -> URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/Assemblywright", isDirectory: true)
            .appendingPathComponent("local-model-selection-v1.json")
    }

    public func load() throws -> AssemblywrightMacLocalModelSelectionState? {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return nil }
        var metadata = stat()
        guard lstat(fileURL.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFREG,
              metadata.st_uid == getuid(), metadata.st_mode & 0o077 == 0,
              metadata.st_size >= 0, metadata.st_size <= 16 * 1_024 else {
            throw AssemblywrightMacLocalModelSelectionError.unsafeStore
        }
        let data = try Data(contentsOf: fileURL, options: [.mappedIfSafe])
        var duplicateScanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
        guard data.count <= 16 * 1_024,
              (try? duplicateScanner.validateNoDuplicateObjectKeysRecursively()) != nil,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys) == ["schema_version", "active", "pending"],
              Self.hasExactNestedShape(object),
              let state = try? JSONDecoder().decode(
                AssemblywrightMacLocalModelSelectionState.self,
                from: data
              ), state.schemaVersion == 1 else {
            throw AssemblywrightMacLocalModelSelectionError.unsafeStore
        }
        try Self.validate(state)
        return state
    }

    public func save(_ state: AssemblywrightMacLocalModelSelectionState) throws {
        try Self.validate(state)
        let directory = fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
        var directoryMetadata = stat()
        guard lstat(directory.path, &directoryMetadata) == 0,
              directoryMetadata.st_mode & S_IFMT == S_IFDIR,
              directoryMetadata.st_uid == getuid(),
              directoryMetadata.st_mode & 0o077 == 0 else {
            throw AssemblywrightMacLocalModelSelectionError.unsafeStore
        }
        let directoryDescriptor = open(directory.path, O_RDONLY | O_DIRECTORY | O_CLOEXEC)
        guard directoryDescriptor >= 0 else {
            throw AssemblywrightMacLocalModelSelectionError.unsafeStore
        }
        defer { _ = close(directoryDescriptor) }
        var openedDirectoryMetadata = stat()
        guard fstat(directoryDescriptor, &openedDirectoryMetadata) == 0,
              openedDirectoryMetadata.st_dev == directoryMetadata.st_dev,
              openedDirectoryMetadata.st_ino == directoryMetadata.st_ino else {
            throw AssemblywrightMacLocalModelSelectionError.unsafeStore
        }
        let data = try JSONEncoder.sorted.encode(state)
        guard data.count <= 16 * 1_024 else {
            throw AssemblywrightMacLocalModelSelectionError.unsafeStore
        }
        let temporary = directory.appendingPathComponent(
            ".local-model-selection-\(UUID().uuidString.lowercased()).tmp"
        )
        let descriptor = open(temporary.path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0o600)
        guard descriptor >= 0 else {
            throw AssemblywrightMacLocalModelSelectionError.unsafeStore
        }
        var published = false
        defer {
            _ = close(descriptor)
            if !published { _ = unlink(temporary.path) }
        }
        let written = data.withUnsafeBytes { bytes in
            Darwin.write(descriptor, bytes.baseAddress, bytes.count)
        }
        guard written == data.count, fsync(descriptor) == 0,
              rename(temporary.path, fileURL.path) == 0,
              fsync(directoryDescriptor) == 0 else {
            throw AssemblywrightMacLocalModelSelectionError.unsafeStore
        }
        published = true
    }

    private static func validate(_ state: AssemblywrightMacLocalModelSelectionState) throws {
        guard state.schemaVersion == 1 else {
            throw AssemblywrightMacLocalModelSelectionError.unsafeStore
        }
        if let active = state.active {
            guard active.registryRevision > 0 else {
                throw AssemblywrightMacLocalModelSelectionError.unsafeStore
            }
            do {
                try active.validateLocalPaths()
            } catch {
                throw AssemblywrightMacLocalModelSelectionError.unsafeStore
            }
        }
        try state.pending?.validatePersistedBinding()
    }

    private static func hasExactNestedShape(_ object: [String: Any]) -> Bool {
        let configurationKeys: Set<String> = [
            "modelID", "executablePath", "modelDirectoryPath", "registryRevision"
        ]
        if !(object["active"] is NSNull) {
            guard let active = object["active"] as? [String: Any],
                  Set(active.keys) == configurationKeys else { return false }
        }
        if !(object["pending"] is NSNull) {
            guard let pending = object["pending"] as? [String: Any],
                  Set(pending.keys) == ["configuration", "requestData"],
                  let configuration = pending["configuration"] as? [String: Any],
                  Set(configuration.keys) == configurationKeys else { return false }
        }
        return true
    }
}

private extension JSONEncoder {
    static var sorted: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }
}

public struct AssemblywrightMacLocalModelSelectionRequest: Codable, Equatable, Sendable {
    public let schemaVersion: UInt16
    public let deviceID: String
    public let expectedRegistryRevision: UInt64
    public let expectedDesignationRevision: UInt64
    public let expectedEmergencyPauseRevision: UInt64
    public let modelID: String

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case deviceID = "device_id"
        case expectedRegistryRevision = "expected_registry_revision"
        case expectedDesignationRevision = "expected_designation_revision"
        case expectedEmergencyPauseRevision = "expected_emergency_pause_revision"
        case modelID = "model_id"
    }

    public static func decodeStrict(_ data: Data) throws -> Self {
        try decodeStrictObject(
            data,
            keys: [
                "schema_version", "device_id", "expected_registry_revision",
                "expected_designation_revision", "expected_emergency_pause_revision", "model_id"
            ]
        )
    }

    public func validate() throws {
        guard schemaVersion == 1, UUID(uuidString: deviceID) != nil,
              expectedRegistryRevision > 0, expectedDesignationRevision > 0,
              !modelID.isEmpty, modelID.utf8.count <= 128,
              validLocalModelID(modelID) else {
            throw AssemblywrightMacLocalModelSelectionError.invalidSelection
        }
    }
}

/// App-to-signed-helper intent frozen from the authenticated supervisor view.
/// The helper requires these device and revision bindings to match its exact
/// installed profile before it constructs the path-free Windows request.
public struct AssemblywrightMacLocalModelSelectionIntent: Codable, Equatable, Sendable {
    public let schemaVersion: UInt16
    public let deviceID: String
    public let expectedRegistryRevision: UInt64
    public let expectedDesignationRevision: UInt64
    public let expectedEmergencyPauseRevision: UInt64
    public let modelID: String

    public init(
        deviceID: String,
        expectedRegistryRevision: UInt64,
        expectedDesignationRevision: UInt64,
        expectedEmergencyPauseRevision: UInt64,
        modelID: String
    ) {
        schemaVersion = 1
        self.deviceID = deviceID
        self.expectedRegistryRevision = expectedRegistryRevision
        self.expectedDesignationRevision = expectedDesignationRevision
        self.expectedEmergencyPauseRevision = expectedEmergencyPauseRevision
        self.modelID = modelID
    }

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case deviceID = "device_id"
        case expectedRegistryRevision = "expected_registry_revision"
        case expectedDesignationRevision = "expected_designation_revision"
        case expectedEmergencyPauseRevision = "expected_emergency_pause_revision"
        case modelID = "model_id"
    }

    public static func decodeStrict(_ data: Data) throws -> Self {
        try decodeStrictObject(
            data,
            keys: [
                "schema_version", "device_id", "expected_registry_revision",
                "expected_designation_revision",
                "expected_emergency_pause_revision", "model_id"
            ]
        )
    }

    public func validate() throws {
        guard schemaVersion == 1, UUID(uuidString: deviceID) != nil,
              expectedRegistryRevision > 0, expectedDesignationRevision > 0,
              validLocalModelID(modelID) else {
            throw AssemblywrightMacLocalModelSelectionError.invalidSelection
        }
    }

    public func encodeStrict() throws -> Data {
        try validate()
        return try JSONEncoder.sorted.encode(self)
    }
}

public struct AssemblywrightMacLocalModelSelectionReceipt: Codable, Equatable, Sendable {
    public let schemaVersion: UInt16
    public let deviceID: String
    public let registryRevision: UInt64
    public let designationRevision: UInt64
    public let emergencyPauseRevision: UInt64
    public let modelID: String
    public let selectedAtMilliseconds: UInt64
    public let status: String

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case deviceID = "device_id"
        case registryRevision = "registry_revision"
        case designationRevision = "designation_revision"
        case emergencyPauseRevision = "emergency_pause_revision"
        case modelID = "model_id"
        case selectedAtMilliseconds = "selected_at_ms"
        case status
    }

    static func decodeStrict(_ data: Data) throws -> Self {
        let projection: Self = try decodeStrictObject(
            data,
            keys: [
                "schema_version", "device_id", "registry_revision", "designation_revision",
                "emergency_pause_revision", "model_id", "selected_at_ms", "status"
            ]
        )
        try projection.validate()
        return projection
    }

    func validate() throws {
        guard schemaVersion == 1,
              UUID(uuidString: deviceID)?.uuidString.lowercased() == deviceID.lowercased(),
              registryRevision > 0, designationRevision > 0,
              validLocalModelID(modelID), selectedAtMilliseconds > 0,
              status == "selected" else {
            throw AssemblywrightMacLocalModelSelectionError.invalidSelection
        }
    }
}

public struct AssemblywrightMacLocalModelSelectionProjection: Codable, Equatable, Sendable {
    public let schemaVersion: UInt16
    public let deviceID: String
    public let deviceName: String
    public let registryRevision: UInt64
    public let designationRevision: UInt64
    public let emergencyPauseRevision: UInt64
    public let emergencyPaused: Bool
    public let modelID: String

    public init(
        schemaVersion: UInt16,
        deviceID: String,
        deviceName: String,
        registryRevision: UInt64,
        designationRevision: UInt64,
        emergencyPauseRevision: UInt64,
        emergencyPaused: Bool,
        modelID: String
    ) {
        self.schemaVersion = schemaVersion
        self.deviceID = deviceID
        self.deviceName = deviceName
        self.registryRevision = registryRevision
        self.designationRevision = designationRevision
        self.emergencyPauseRevision = emergencyPauseRevision
        self.emergencyPaused = emergencyPaused
        self.modelID = modelID
    }

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case deviceID = "device_id"
        case deviceName = "device_name"
        case registryRevision = "registry_revision"
        case designationRevision = "designation_revision"
        case emergencyPauseRevision = "emergency_pause_revision"
        case emergencyPaused = "emergency_paused"
        case modelID = "model_id"
    }

    static func decodeStrict(_ data: Data) throws -> Self {
        let projection: Self = try decodeStrictObject(
            data,
            keys: [
                "schema_version", "device_id", "device_name", "registry_revision",
                "designation_revision", "emergency_pause_revision", "emergency_paused", "model_id"
            ]
        )
        try projection.validate()
        return projection
    }

    func validate() throws {
        guard schemaVersion == 1,
              UUID(uuidString: deviceID)?.uuidString.lowercased() == deviceID.lowercased(),
              !deviceName.isEmpty, deviceName.utf8.count <= 128,
              registryRevision > 0, designationRevision > 0,
              validLocalModelID(modelID) else {
            throw AssemblywrightMacLocalModelSelectionError.invalidSelection
        }
    }
}

/// Fixed, path-free helper evidence that an authenticated Windows endpoint
/// explicitly rejected the frozen selection request before any local promotion.
public struct AssemblywrightMacLocalModelSelectionTerminalRejection:
    Codable, Equatable, Sendable
{
    public let schemaVersion: UInt16
    public let deviceID: String
    public let expectedRegistryRevision: UInt64
    public let expectedDesignationRevision: UInt64
    public let expectedEmergencyPauseRevision: UInt64
    public let modelID: String
    public let status: String
    public let errorCode: String

    init(request: AssemblywrightMacLocalModelSelectionRequest, errorCode: String) {
        schemaVersion = 1
        deviceID = request.deviceID
        expectedRegistryRevision = request.expectedRegistryRevision
        expectedDesignationRevision = request.expectedDesignationRevision
        expectedEmergencyPauseRevision = request.expectedEmergencyPauseRevision
        modelID = request.modelID
        status = "rejected"
        self.errorCode = errorCode
    }

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case deviceID = "device_id"
        case expectedRegistryRevision = "expected_registry_revision"
        case expectedDesignationRevision = "expected_designation_revision"
        case expectedEmergencyPauseRevision = "expected_emergency_pause_revision"
        case modelID = "model_id"
        case status
        case errorCode = "error_code"
    }

    static func decodeStrict(_ data: Data) throws -> Self {
        let decoded: Self = try decodeStrictObject(data, keys: [
            "schema_version", "device_id", "expected_registry_revision",
            "expected_designation_revision", "expected_emergency_pause_revision",
            "model_id", "status", "error_code"
        ])
        let intent = AssemblywrightMacLocalModelSelectionIntent(
            deviceID: decoded.deviceID,
            expectedRegistryRevision: decoded.expectedRegistryRevision,
            expectedDesignationRevision: decoded.expectedDesignationRevision,
            expectedEmergencyPauseRevision: decoded.expectedEmergencyPauseRevision,
            modelID: decoded.modelID
        )
        guard decoded.schemaVersion == 1, decoded.status == "rejected",
              [
                  "local_model_selection_rejected",
                  "local_model_selection_request_rejected",
                  "unauthorized"
              ].contains(decoded.errorCode) else {
            throw AssemblywrightMacLocalModelSelectionError.invalidSelection
        }
        try intent.validate()
        return decoded
    }
}

public enum AssemblywrightMacLocalModelSelectionOutcome: Equatable, Sendable {
    case authoritativeReceipt(AssemblywrightMacLocalModelSelectionReceipt)
    case reconciledProjection(AssemblywrightMacLocalModelSelectionProjection)
    case terminalRejection(AssemblywrightMacLocalModelSelectionTerminalRejection)

    public var commandData: Data {
        get throws {
            switch self {
            case let .authoritativeReceipt(receipt): try JSONEncoder.sorted.encode(receipt)
            case let .reconciledProjection(projection): try JSONEncoder.sorted.encode(projection)
            case let .terminalRejection(rejection): try JSONEncoder.sorted.encode(rejection)
            }
        }
    }
}

public struct AssemblywrightMacLocalModelSelectionCommandBinding: Equatable, Sendable {
    public let registryRevision: UInt64
    public let designationRevision: UInt64
    public let emergencyPauseRevision: UInt64
    public let modelID: String
    public let reconciled: Bool
}

public enum AssemblywrightMacLocalModelSelectionCommandResult: Equatable, Sendable {
    case selected(AssemblywrightMacLocalModelSelectionCommandBinding)
    case terminalRejection(errorCode: String)
}

public enum AssemblywrightMacLocalModelSelectionControl {
    public static let path = "/v1/distributed/local-model/selection"
    public static let maximumFrameBytes = 8 * 1_024

    static func reconciliationTarget(
        installedProfile: AssemblywrightMacBridgeProfile,
        requestedModelID: String
    ) throws -> AssemblywrightMacBridgeProfile {
        try validateMLX(installedProfile)
        guard installedProfile.registryRevision < UInt64.max,
              validLocalModelID(requestedModelID),
              installedProfile.capabilities[0].model != requestedModelID else {
            throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
        }
        let old = installedProfile.capabilities[0]
        return AssemblywrightMacBridgeProfile(
            deviceID: installedProfile.deviceID,
            deviceName: installedProfile.deviceName,
            role: installedProfile.role,
            registryRevision: installedProfile.registryRevision + 1,
            capabilities: [AssemblywrightMacBridgeCapability(
                id: old.id, kind: old.kind, provider: old.provider, model: requestedModelID,
                maxContextBytes: old.maxContextBytes, maxResultBytes: old.maxResultBytes
            )],
            masterEndpoint: installedProfile.masterEndpoint,
            certificateNotAfterMilliseconds: installedProfile.certificateNotAfterMilliseconds
        )
    }

    public static func request(
        profile: AssemblywrightMacBridgeProfile,
        designationRevision: UInt64,
        emergencyPauseRevision: UInt64,
        modelID: String
    ) throws -> Data {
        let request = AssemblywrightMacLocalModelSelectionRequest(
            schemaVersion: 1,
            deviceID: profile.deviceID,
            expectedRegistryRevision: profile.registryRevision,
            expectedDesignationRevision: designationRevision,
            expectedEmergencyPauseRevision: emergencyPauseRevision,
            modelID: modelID
        )
        try request.validate()
        try validateMLX(profile)
        guard profile.capabilities[0].model != modelID else {
            throw AssemblywrightMacLocalModelSelectionError.invalidSelection
        }
        return try JSONEncoder.sorted.encode(request)
    }

    public static func perform(
        requestData: Data,
        identityStore: any AssemblywrightMacLocalModelIdentityStoring,
        connector: any AssemblywrightMacBridgeConnecting = AssemblywrightMacDefaultBridgeConnector()
    ) async throws -> AssemblywrightMacLocalModelSelectionOutcome {
        let request = try AssemblywrightMacLocalModelSelectionRequest.decodeStrict(requestData)
        try request.validate()
        guard let profile = try identityStore.loadInstalledProfile(),
              profile.deviceID.lowercased() == request.deviceID.lowercased(),
              profile.registryRevision == request.expectedRegistryRevision else {
            throw AssemblywrightMacLocalModelSelectionError.rejected
        }
        try validateMLX(profile)
        let session = try await connector.connect(profile: profile)
        let response: AssemblywrightMacBridgeHTTPResponse
        do {
            response = try await session.send(
                AssemblywrightMacBridgeHTTPRequest(method: "POST", path: path, body: requestData)
            )
        } catch {
            return try await reconcile(
                request: request,
                profile: profile,
                identityStore: identityStore,
                connector: connector
            )
        }
        if response.status != 200 {
            if let errorCode = terminalRejectionErrorCode(response) {
                return .terminalRejection(.init(request: request, errorCode: errorCode))
            }
            return try await reconcile(
                request: request,
                profile: profile,
                identityStore: identityStore,
                connector: connector
            )
        }
        guard response.body.count <= maximumFrameBytes else {
            throw AssemblywrightMacLocalModelSelectionError.rejected
        }
        let receipt = try AssemblywrightMacLocalModelSelectionReceipt.decodeStrict(response.body)
        try validate(receipt: receipt, request: request)
        _ = try identityStore.installLocalModelSelection(
            modelID: request.modelID,
            expectedRegistryRevision: request.expectedRegistryRevision,
            registryRevision: receipt.registryRevision
        )
        return .authoritativeReceipt(receipt)
    }

    public static func performIntent(
        intentData: Data,
        identityStore: any AssemblywrightMacLocalModelIdentityStoring,
        connector: any AssemblywrightMacBridgeConnecting = AssemblywrightMacDefaultBridgeConnector()
    ) async throws -> AssemblywrightMacLocalModelSelectionOutcome {
        let intent = try AssemblywrightMacLocalModelSelectionIntent.decodeStrict(intentData)
        try intent.validate()
        guard let profile = try identityStore.loadInstalledProfile() else {
            throw AssemblywrightMacLocalModelSelectionError.unavailable
        }
        let requestData = try request(
            profile: profile,
            designationRevision: intent.expectedDesignationRevision,
            emergencyPauseRevision: intent.expectedEmergencyPauseRevision,
            modelID: intent.modelID
        )
        guard profile.deviceID.lowercased() == intent.deviceID.lowercased(),
              profile.registryRevision == intent.expectedRegistryRevision else {
            throw AssemblywrightMacLocalModelSelectionError.rejected
        }
        return try await perform(
            requestData: requestData,
            identityStore: identityStore,
            connector: connector
        )
    }

    public static func reconcileIntent(
        intentData: Data,
        identityStore: any AssemblywrightMacLocalModelIdentityStoring,
        connector: any AssemblywrightMacBridgeConnecting = AssemblywrightMacDefaultBridgeConnector()
    ) async throws -> AssemblywrightMacLocalModelSelectionOutcome {
        let intent = try AssemblywrightMacLocalModelSelectionIntent.decodeStrict(intentData)
        try intent.validate()
        guard intent.expectedRegistryRevision < UInt64.max,
              intent.expectedDesignationRevision < UInt64.max else {
            throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
        }
        guard let installed = try identityStore.loadInstalledProfile(),
              installed.deviceID.lowercased() == intent.deviceID.lowercased() else {
            throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
        }
        if installed.registryRevision == intent.expectedRegistryRevision + 1,
           installed.capabilities.count == 1,
           installed.capabilities[0].model == intent.modelID {
            let session = try await connector.connect(profile: installed)
            let response = try await session.send(
                AssemblywrightMacBridgeHTTPRequest(method: "GET", path: path)
            )
            guard response.status == 200 else {
                throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
            }
            let projection = try AssemblywrightMacLocalModelSelectionProjection.decodeStrict(
                response.body
            )
            try validateProjection(projection, intent: intent, deviceName: installed.deviceName)
            return .reconciledProjection(projection)
        }
        guard installed.registryRevision == intent.expectedRegistryRevision else {
            throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
        }
        let request = AssemblywrightMacLocalModelSelectionRequest(
            schemaVersion: 1,
            deviceID: intent.deviceID,
            expectedRegistryRevision: intent.expectedRegistryRevision,
            expectedDesignationRevision: intent.expectedDesignationRevision,
            expectedEmergencyPauseRevision: intent.expectedEmergencyPauseRevision,
            modelID: intent.modelID
        )
        do {
            return try await reconcileTarget(
                request: request,
                profile: installed,
                identityStore: identityStore,
                connector: connector
            )
        } catch is AssemblywrightMacAuthoritativeTargetReconciliationError {
            // A target-profile application session proves that the old
            // registration is stale. Pause or authority churn must remain
            // pending and must never trigger an old-profile authentication.
            throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
        } catch {
            // Separately confirmed recovery may retry the original POST once,
            // but only after Windows proves it still has the exact old binding.
            let oldSession = try await connector.connect(profile: installed)
            let oldResponse = try await oldSession.send(
                AssemblywrightMacBridgeHTTPRequest(method: "GET", path: path)
            )
            guard oldResponse.status == 200 else {
                throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
            }
            let oldProjection = try AssemblywrightMacLocalModelSelectionProjection.decodeStrict(
                oldResponse.body
            )
            guard oldProjection.schemaVersion == 1,
                  oldProjection.deviceID.lowercased() == intent.deviceID.lowercased(),
                  oldProjection.deviceName == installed.deviceName,
                  oldProjection.registryRevision == intent.expectedRegistryRevision,
                  oldProjection.designationRevision == intent.expectedDesignationRevision,
                  oldProjection.emergencyPauseRevision
                    == intent.expectedEmergencyPauseRevision,
                  !oldProjection.emergencyPaused,
                  oldProjection.modelID == installed.capabilities[0].model else {
                throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
            }
            let requestData = try JSONEncoder.sorted.encode(request)
            let selectedResponse: AssemblywrightMacBridgeHTTPResponse
            do {
                selectedResponse = try await oldSession.send(
                    AssemblywrightMacBridgeHTTPRequest(
                        method: "POST", path: path, body: requestData
                    )
                )
            } catch {
                return try await reconcile(
                    request: request,
                    profile: installed,
                    identityStore: identityStore,
                    connector: connector
                )
            }
            if selectedResponse.status != 200 {
                if let errorCode = terminalRejectionErrorCode(selectedResponse) {
                    return .terminalRejection(.init(request: request, errorCode: errorCode))
                }
                return try await reconcile(
                    request: request,
                    profile: installed,
                    identityStore: identityStore,
                    connector: connector
                )
            }
            guard selectedResponse.body.count <= maximumFrameBytes else {
                throw AssemblywrightMacLocalModelSelectionError.rejected
            }
            let receipt = try AssemblywrightMacLocalModelSelectionReceipt.decodeStrict(
                selectedResponse.body
            )
            try validate(receipt: receipt, request: request)
            _ = try identityStore.installLocalModelSelection(
                modelID: request.modelID,
                expectedRegistryRevision: request.expectedRegistryRevision,
                registryRevision: receipt.registryRevision
            )
            return .authoritativeReceipt(receipt)
        }
    }

    public static func validateCommandData(
        _ data: Data,
        intentData: Data
    ) throws -> AssemblywrightMacLocalModelSelectionCommandResult {
        let intent = try AssemblywrightMacLocalModelSelectionIntent.decodeStrict(intentData)
        try intent.validate()
        guard intent.expectedRegistryRevision < UInt64.max,
              intent.expectedDesignationRevision < UInt64.max,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw AssemblywrightMacLocalModelSelectionError.rejected
        }
        if Set(object.keys) == [
            "schema_version", "device_id", "expected_registry_revision",
            "expected_designation_revision", "expected_emergency_pause_revision",
            "model_id", "status", "error_code"
        ] {
            let rejection = try AssemblywrightMacLocalModelSelectionTerminalRejection.decodeStrict(
                data
            )
            guard rejection.deviceID.lowercased() == intent.deviceID.lowercased(),
                  rejection.expectedRegistryRevision == intent.expectedRegistryRevision,
                  rejection.expectedDesignationRevision == intent.expectedDesignationRevision,
                  rejection.expectedEmergencyPauseRevision
                    == intent.expectedEmergencyPauseRevision,
                  rejection.modelID == intent.modelID else {
                throw AssemblywrightMacLocalModelSelectionError.rejected
            }
            return .terminalRejection(errorCode: rejection.errorCode)
        }
        if Set(object.keys) == [
            "schema_version", "device_id", "registry_revision", "designation_revision",
            "emergency_pause_revision", "model_id", "selected_at_ms", "status"
        ] {
            let receipt = try AssemblywrightMacLocalModelSelectionReceipt.decodeStrict(data)
            guard receipt.schemaVersion == 1, receipt.status == "selected",
                  receipt.deviceID.lowercased() == intent.deviceID.lowercased(),
                  receipt.registryRevision == intent.expectedRegistryRevision + 1,
                  receipt.designationRevision == intent.expectedDesignationRevision + 1,
                  receipt.emergencyPauseRevision == intent.expectedEmergencyPauseRevision,
                  receipt.modelID == intent.modelID, receipt.selectedAtMilliseconds > 0 else {
                throw AssemblywrightMacLocalModelSelectionError.rejected
            }
            return .selected(.init(
                registryRevision: receipt.registryRevision,
                designationRevision: receipt.designationRevision,
                emergencyPauseRevision: receipt.emergencyPauseRevision,
                modelID: receipt.modelID,
                reconciled: false
            ))
        }
        let projection = try AssemblywrightMacLocalModelSelectionProjection.decodeStrict(data)
        guard projection.schemaVersion == 1,
              projection.deviceID.lowercased() == intent.deviceID.lowercased(),
              projection.registryRevision == intent.expectedRegistryRevision + 1,
              projection.designationRevision >= intent.expectedDesignationRevision + 1,
              projection.emergencyPauseRevision >= intent.expectedEmergencyPauseRevision,
              !projection.emergencyPaused, projection.modelID == intent.modelID else {
            throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
        }
        return .selected(.init(
            registryRevision: projection.registryRevision,
            designationRevision: projection.designationRevision,
            emergencyPauseRevision: projection.emergencyPauseRevision,
            modelID: projection.modelID,
            reconciled: true
        ))
    }

    private static func reconcile(
        request: AssemblywrightMacLocalModelSelectionRequest,
        profile: AssemblywrightMacBridgeProfile,
        identityStore: any AssemblywrightMacLocalModelIdentityStoring,
        connector: any AssemblywrightMacBridgeConnecting
    ) async throws -> AssemblywrightMacLocalModelSelectionOutcome {
        do {
            return try await reconcileTarget(
                request: request,
                profile: profile,
                identityStore: identityStore,
                connector: connector
            )
        } catch is AssemblywrightMacAuthoritativeTargetReconciliationError {
            throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
        }
    }

    private static func reconcileTarget(
        request: AssemblywrightMacLocalModelSelectionRequest,
        profile: AssemblywrightMacBridgeProfile,
        identityStore: any AssemblywrightMacLocalModelIdentityStoring,
        connector: any AssemblywrightMacBridgeConnecting
    ) async throws -> AssemblywrightMacLocalModelSelectionOutcome {
        guard request.expectedRegistryRevision < UInt64.max,
              request.expectedDesignationRevision < UInt64.max else {
            throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
        }
        let target = try reconciliationTarget(
            installedProfile: profile,
            requestedModelID: request.modelID
        )
        let session = try await connector.connectForLocalModelReconciliation(
            profile: target,
            installedProfile: profile,
            requestedModelID: request.modelID
        )
        do {
            let response = try await session.send(
                AssemblywrightMacBridgeHTTPRequest(method: "GET", path: path)
            )
            guard response.status == 200, response.body.count <= maximumFrameBytes else {
                throw AssemblywrightMacAuthoritativeTargetReconciliationError.unsafeState
            }
            let projection = try AssemblywrightMacLocalModelSelectionProjection.decodeStrict(
                response.body
            )
            guard projection.schemaVersion == 1,
                  projection.deviceID.lowercased() == request.deviceID.lowercased(),
                  projection.deviceName == profile.deviceName,
                  projection.registryRevision == request.expectedRegistryRevision + 1,
                  projection.designationRevision >= request.expectedDesignationRevision + 1,
                  projection.emergencyPauseRevision
                    >= request.expectedEmergencyPauseRevision,
                  !projection.emergencyPaused,
                  projection.modelID == request.modelID else {
                throw AssemblywrightMacAuthoritativeTargetReconciliationError.unsafeState
            }
            _ = try identityStore.installLocalModelSelection(
                modelID: request.modelID,
                expectedRegistryRevision: request.expectedRegistryRevision,
                registryRevision: projection.registryRevision
            )
            return .reconciledProjection(projection)
        } catch is AssemblywrightMacAuthoritativeTargetReconciliationError {
            throw AssemblywrightMacAuthoritativeTargetReconciliationError.unsafeState
        } catch {
            // Successful target-profile connection means Windows accepted the
            // committed target registration. Never probe the stale old profile
            // after any later response, pause, validation, or install failure.
            throw AssemblywrightMacAuthoritativeTargetReconciliationError.unsafeState
        }
    }

    private static func validateProjection(
        _ projection: AssemblywrightMacLocalModelSelectionProjection,
        intent: AssemblywrightMacLocalModelSelectionIntent,
        deviceName: String
    ) throws {
        guard intent.expectedRegistryRevision < UInt64.max,
              intent.expectedDesignationRevision < UInt64.max,
              projection.schemaVersion == 1,
              projection.deviceID.lowercased() == intent.deviceID.lowercased(),
              projection.deviceName == deviceName,
              projection.registryRevision == intent.expectedRegistryRevision + 1,
              projection.designationRevision >= intent.expectedDesignationRevision + 1,
              projection.emergencyPauseRevision >= intent.expectedEmergencyPauseRevision,
              !projection.emergencyPaused, projection.modelID == intent.modelID else {
            throw AssemblywrightMacLocalModelSelectionError.reconciliationRequired
        }
    }

    private static func validate(
        receipt: AssemblywrightMacLocalModelSelectionReceipt,
        request: AssemblywrightMacLocalModelSelectionRequest
    ) throws {
        guard request.expectedRegistryRevision < UInt64.max,
              request.expectedDesignationRevision < UInt64.max,
              receipt.schemaVersion == 1, receipt.status == "selected",
              receipt.deviceID.lowercased() == request.deviceID.lowercased(),
              receipt.registryRevision == request.expectedRegistryRevision + 1,
              receipt.designationRevision == request.expectedDesignationRevision + 1,
              receipt.emergencyPauseRevision == request.expectedEmergencyPauseRevision,
              receipt.modelID == request.modelID, receipt.selectedAtMilliseconds > 0 else {
            throw AssemblywrightMacLocalModelSelectionError.rejected
        }
    }

    private static func validateMLX(_ profile: AssemblywrightMacBridgeProfile) throws {
        guard profile.role == "mac_bridge", profile.capabilities.count == 1,
              let capability = profile.capabilities.first,
              capability.id == "mlx.reasoning", capability.kind == "local_inference",
              capability.provider == "mlx" else {
            throw AssemblywrightMacLocalModelSelectionError.rejected
        }
    }

    private static func terminalRejectionErrorCode(
        _ response: AssemblywrightMacBridgeHTTPResponse
    ) -> String? {
        let expectedCode: String
        switch response.status {
        case 401: expectedCode = "unauthorized"
        case 409: expectedCode = "local_model_selection_rejected"
        case 422: expectedCode = "local_model_selection_request_rejected"
        default: return nil
        }
        guard response.body.count <= maximumFrameBytes,
              let body = try? AssemblywrightMacLocalModelSelectionHTTPError.decodeStrict(
                  response.body
              ), body.error == expectedCode else {
            return nil
        }
        return expectedCode
    }
}

private enum AssemblywrightMacAuthoritativeTargetReconciliationError: Error {
    case unsafeState
}

private struct AssemblywrightMacLocalModelSelectionHTTPError: Decodable {
    let error: String

    static func decodeStrict(_ data: Data) throws -> Self {
        try decodeStrictObject(data, keys: ["error"])
    }
}

func validLocalModelID(_ value: String) -> Bool {
    !value.isEmpty
        && value.utf8.count <= 128
        && value.utf8.allSatisfy { (0x21 ... 0x7e).contains($0) }
        && !value.hasPrefix("/")
        && !value.contains("\\")
        && !value.hasPrefix("file:")
        && !value.split(separator: "/", omittingEmptySubsequences: false).contains(where: {
            $0 == "." || $0 == ".."
        })
}

private func decodeStrictObject<T: Decodable>(
    _ data: Data,
    keys expected: Set<String>
) throws -> T {
    guard !data.isEmpty, data.count <= 8 * 1_024 else {
        throw AssemblywrightMacLocalModelSelectionError.invalidSelection
    }
    var scanner = AssemblywrightStrictJSONObjectKeyScanner(data: data)
    guard let keys = try? scanner.scanTopLevelKeys(), Set(keys).count == keys.count,
          Set(keys) == expected,
          let decoded = try? JSONDecoder().decode(T.self, from: data) else {
        throw AssemblywrightMacLocalModelSelectionError.invalidSelection
    }
    return decoded
}
