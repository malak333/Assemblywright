import Darwin
import Foundation
import Security

public enum AssemblywrightIPCPeerIdentityProfile: String, Equatable, Sendable {
    case adhocExact = "adhoc_exact"
    case developerIDSameTeamHardenedRuntime = "developer_id_hardened"
}

public struct AssemblywrightIPCPeerIdentityPolicy: Equatable, Sendable {
    public static let kind = "unix_socket_peer_identity_v1"
    public static let appIdentifier = "com.nobiletechnology.assemblywright"
    public static let coreIdentifier = "com.nobiletechnology.assemblywright.core"

    public let profile: AssemblywrightIPCPeerIdentityProfile
    public let peerCodeRequirement: String
    public let coreCodeRequirement: String
    public let expectedCoreCDHash: Data
    public let expectedCoreExecutableURL: URL

    public init(
        profile: AssemblywrightIPCPeerIdentityProfile,
        peerCodeRequirement: String,
        coreCodeRequirement: String,
        expectedCoreCDHash: Data,
        expectedCoreExecutableURL: URL
    ) {
        self.profile = profile
        self.peerCodeRequirement = peerCodeRequirement
        self.coreCodeRequirement = coreCodeRequirement
        self.expectedCoreCDHash = expectedCoreCDHash
        self.expectedCoreExecutableURL = expectedCoreExecutableURL.standardizedFileURL
    }
}

public enum AssemblywrightIPCPeerIdentityError: Error, Equatable, Sendable {
    case unavailable
    case invalidIdentifier
    case invalidProfile
    case invalidRequirement
    case invalidSignature
    case hardenedRuntimeRequired
    case peerTokenUnavailable
    case peerCodeUnavailable
    case peerCodeInvalid
    case peerExecutableMismatch
    case peerCDHashMismatch
}

public protocol AssemblywrightIPCPeerIdentityPolicyProviding: Sendable {
    func policy(forCoreExecutable executableURL: URL) throws -> AssemblywrightIPCPeerIdentityPolicy
}

public protocol AssemblywrightUnixPeerIdentityVerifying: Sendable {
    func verifyPeer(
        on socketDescriptor: Int32,
        policy: AssemblywrightIPCPeerIdentityPolicy
    ) throws
}

public struct SecurityAssemblywrightIPCPeerIdentityPolicyProvider: AssemblywrightIPCPeerIdentityPolicyProviding {
    private static let adhocFlag: UInt32 = 0x0002
    private static let hardenedRuntimeFlag: UInt32 = 0x10000
    private static let maximumRequirementBytes = 4 * 1024

    public init() {}

    public func policy(forCoreExecutable executableURL: URL) throws -> AssemblywrightIPCPeerIdentityPolicy {
        guard executableURL.isFileURL, executableURL.path.hasPrefix("/") else {
            throw AssemblywrightIPCPeerIdentityError.unavailable
        }
        let app = try Self.copySelfCode()
        let appStatic = try Self.copyStaticCode(from: app)
        let core = try Self.copyStaticCode(at: executableURL)
        guard SecCodeCheckValidity(
            app,
            SecCSFlags(rawValue: kSecCSStrictValidate),
            nil
        ) == errSecSuccess,
        SecStaticCodeCheckValidity(
            core,
            SecCSFlags(rawValue: kSecCSStrictValidate),
            nil
        ) == errSecSuccess else {
            throw AssemblywrightIPCPeerIdentityError.invalidSignature
        }
        let appInfo = try Self.signingInformation(for: appStatic)
        let coreInfo = try Self.signingInformation(for: core)
        let appIdentity = try Self.identity(from: appInfo)
        let coreIdentity = try Self.identity(from: coreInfo)

        guard appIdentity.identifier == AssemblywrightIPCPeerIdentityPolicy.appIdentifier,
              coreIdentity.identifier == AssemblywrightIPCPeerIdentityPolicy.coreIdentifier else {
            throw AssemblywrightIPCPeerIdentityError.invalidIdentifier
        }

        let profile: AssemblywrightIPCPeerIdentityProfile
        let appRequirement: String
        let coreRequirement: String
        if appIdentity.isAdhoc, coreIdentity.isAdhoc,
           appIdentity.teamIdentifier == nil, coreIdentity.teamIdentifier == nil {
            profile = .adhocExact
            appRequirement = Self.exactAdhocRequirement(for: appIdentity)
            coreRequirement = Self.exactAdhocRequirement(for: coreIdentity)
        } else if !appIdentity.isAdhoc, !coreIdentity.isAdhoc,
                  let appTeam = appIdentity.teamIdentifier,
                  !appTeam.isEmpty,
                  coreIdentity.teamIdentifier == appTeam {
            guard appIdentity.hasHardenedRuntime, coreIdentity.hasHardenedRuntime else {
                throw AssemblywrightIPCPeerIdentityError.hardenedRuntimeRequired
            }
            profile = .developerIDSameTeamHardenedRuntime
            appRequirement = Self.developerIDRequirement(
                identifier: appIdentity.identifier,
                teamIdentifier: appTeam
            )
            coreRequirement = Self.developerIDRequirement(
                identifier: coreIdentity.identifier,
                teamIdentifier: appTeam
            )
        } else {
            throw AssemblywrightIPCPeerIdentityError.invalidProfile
        }

        try Self.validateRequirement(appRequirement)
        try Self.validateRequirement(coreRequirement)
        try Self.validate(dynamicCode: app, requirement: appRequirement)
        try Self.validate(code: core, requirement: coreRequirement, checkAllArchitectures: false)

        return AssemblywrightIPCPeerIdentityPolicy(
            profile: profile,
            peerCodeRequirement: appRequirement,
            coreCodeRequirement: coreRequirement,
            expectedCoreCDHash: coreIdentity.cdHash,
            expectedCoreExecutableURL: executableURL
        )
    }

    private struct CodeIdentity {
        let identifier: String
        let teamIdentifier: String?
        let cdHash: Data
        let flags: UInt32

        var isAdhoc: Bool { flags & adhocFlag != 0 }
        var hasHardenedRuntime: Bool { flags & hardenedRuntimeFlag != 0 }
    }

    private static func copySelfCode() throws -> SecCode {
        var code: SecCode?
        guard SecCodeCopySelf([], &code) == errSecSuccess, let code else {
            throw AssemblywrightIPCPeerIdentityError.unavailable
        }
        return code
    }

    private static func copyStaticCode(at url: URL) throws -> SecStaticCode {
        var code: SecStaticCode?
        guard SecStaticCodeCreateWithPath(url as CFURL, [], &code) == errSecSuccess,
              let code else {
            throw AssemblywrightIPCPeerIdentityError.unavailable
        }
        return code
    }

    private static func copyStaticCode(from code: SecCode) throws -> SecStaticCode {
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess,
              let staticCode else {
            throw AssemblywrightIPCPeerIdentityError.unavailable
        }
        return staticCode
    }

    private static func signingInformation(for code: SecStaticCode) throws -> [String: Any] {
        var information: CFDictionary?
        guard SecCodeCopySigningInformation(
            code,
            SecCSFlags(rawValue: kSecCSSigningInformation),
            &information
        ) == errSecSuccess,
        let information = information as? [String: Any] else {
            throw AssemblywrightIPCPeerIdentityError.unavailable
        }
        return information
    }

    private static func identity(from information: [String: Any]) throws -> CodeIdentity {
        guard let identifier = information[kSecCodeInfoIdentifier as String] as? String,
              !identifier.isEmpty,
              identifier.utf8.count <= 256,
              let cdHash = information[kSecCodeInfoUnique as String] as? Data,
              !cdHash.isEmpty,
              cdHash.count <= 64,
              let flagsNumber = information[kSecCodeInfoFlags as String] as? NSNumber else {
            throw AssemblywrightIPCPeerIdentityError.invalidSignature
        }
        let team = information[kSecCodeInfoTeamIdentifier as String] as? String
        guard team?.utf8.count ?? 0 <= 64 else {
            throw AssemblywrightIPCPeerIdentityError.invalidSignature
        }
        return CodeIdentity(
            identifier: identifier,
            teamIdentifier: team,
            cdHash: cdHash,
            flags: flagsNumber.uint32Value
        )
    }

    private static func exactAdhocRequirement(for identity: CodeIdentity) -> String {
        "identifier \"\(identity.identifier)\" and cdhash H\"\(identity.cdHash.hexString)\""
    }

    private static func developerIDRequirement(
        identifier: String,
        teamIdentifier: String
    ) -> String {
        "anchor apple generic and identifier \"\(identifier)\" and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = \"\(teamIdentifier)\""
    }

    private static func validateRequirement(_ text: String) throws {
        guard !text.isEmpty, text.utf8.count <= maximumRequirementBytes else {
            throw AssemblywrightIPCPeerIdentityError.invalidRequirement
        }
        _ = try requirement(from: text)
    }

    fileprivate static func requirement(from text: String) throws -> SecRequirement {
        var requirement: SecRequirement?
        guard SecRequirementCreateWithString(text as CFString, [], &requirement) == errSecSuccess,
              let requirement else {
            throw AssemblywrightIPCPeerIdentityError.invalidRequirement
        }
        return requirement
    }

    fileprivate static func validate(
        code: SecStaticCode,
        requirement text: String,
        checkAllArchitectures: Bool
    ) throws {
        let requirement = try requirement(from: text)
        var rawFlags = kSecCSStrictValidate
        if checkAllArchitectures { rawFlags |= kSecCSCheckAllArchitectures }
        guard SecStaticCodeCheckValidity(
            code,
            SecCSFlags(rawValue: rawFlags),
            requirement
        ) == errSecSuccess else {
            throw AssemblywrightIPCPeerIdentityError.invalidSignature
        }
    }


    fileprivate static func validate(dynamicCode code: SecCode, requirement text: String) throws {
        let requirement = try requirement(from: text)
        guard SecCodeCheckValidity(
            code,
            SecCSFlags(rawValue: kSecCSStrictValidate),
            requirement
        ) == errSecSuccess else {
            throw AssemblywrightIPCPeerIdentityError.invalidSignature
        }
    }

    fileprivate static func staticCode(from dynamicCode: SecCode) throws -> SecStaticCode {
        try copyStaticCode(from: dynamicCode)
    }
}

public struct SecurityAssemblywrightUnixPeerIdentityVerifier: AssemblywrightUnixPeerIdentityVerifying {
    private static let hardenedRuntimeFlag: UInt32 = 0x10000

    public init() {}

    public func verifyPeer(
        on socketDescriptor: Int32,
        policy: AssemblywrightIPCPeerIdentityPolicy
    ) throws {
        var token = audit_token_t()
        var tokenLength = socklen_t(MemoryLayout.size(ofValue: token))
        guard Darwin.getsockopt(
            socketDescriptor,
            SOL_LOCAL,
            LOCAL_PEERTOKEN,
            &token,
            &tokenLength
        ) == 0,
        tokenLength == MemoryLayout.size(ofValue: token) else {
            throw AssemblywrightIPCPeerIdentityError.peerTokenUnavailable
        }

        let tokenData = withUnsafeBytes(of: &token) { Data($0) }
        let attributes = [kSecGuestAttributeAudit as String: tokenData] as CFDictionary
        var peerCode: SecCode?
        guard SecCodeCopyGuestWithAttributes(nil, attributes, [], &peerCode) == errSecSuccess,
              let peerCode else {
            throw AssemblywrightIPCPeerIdentityError.peerCodeUnavailable
        }
        let requirement = try SecurityAssemblywrightIPCPeerIdentityPolicyProvider.requirement(
            from: policy.coreCodeRequirement
        )
        guard SecCodeCheckValidity(
            peerCode,
            SecCSFlags(rawValue: kSecCSStrictValidate),
            requirement
        ) == errSecSuccess else {
            throw AssemblywrightIPCPeerIdentityError.peerCodeInvalid
        }

        let peerStaticCode = try SecurityAssemblywrightIPCPeerIdentityPolicyProvider.staticCode(
            from: peerCode
        )
        var rawInformation: CFDictionary?
        let flags = SecCSFlags(rawValue: kSecCSSigningInformation)
        guard SecCodeCopySigningInformation(peerStaticCode, flags, &rawInformation) == errSecSuccess,
              let information = rawInformation as? [String: Any],
              let executableURL = information[kSecCodeInfoMainExecutable as String] as? URL,
              let cdHash = information[kSecCodeInfoUnique as String] as? Data,
              let signatureFlags = information[kSecCodeInfoFlags as String] as? NSNumber else {
            throw AssemblywrightIPCPeerIdentityError.peerCodeInvalid
        }
        guard executableURL.standardizedFileURL.path
                == policy.expectedCoreExecutableURL.standardizedFileURL.path else {
            throw AssemblywrightIPCPeerIdentityError.peerExecutableMismatch
        }
        guard cdHash.constantTimeEquals(policy.expectedCoreCDHash) else {
            throw AssemblywrightIPCPeerIdentityError.peerCDHashMismatch
        }
        if policy.profile == .developerIDSameTeamHardenedRuntime,
           signatureFlags.uint32Value & Self.hardenedRuntimeFlag == 0 {
            throw AssemblywrightIPCPeerIdentityError.hardenedRuntimeRequired
        }
    }
}

private extension Data {
    var hexString: String {
        map { String(format: "%02x", $0) }.joined()
    }

    func constantTimeEquals(_ other: Data) -> Bool {
        guard count == other.count else { return false }
        var difference: UInt8 = 0
        for (left, right) in zip(self, other) {
            difference |= left ^ right
        }
        return difference == 0
    }
}
