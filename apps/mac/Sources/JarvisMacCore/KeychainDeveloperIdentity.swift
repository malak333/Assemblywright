import CryptoKit
import Foundation
import Security

public struct KeychainJarvisMacBridgeIdentityStore: JarvisMacBridgeIdentityStore, Sendable {
    private static let service = "com.nobiletechnology.jarvis.developer-bridge"
    private static let stagedAccount = "enrollment-staged-v1"
    private static let installedAccount = "identity-installed-v1"
    private static let keyTag = Data("com.nobiletechnology.jarvis.developer-bridge.p256-v1".utf8)
    private static let lock = NSLock()

    public init() {}

    public func stageIdentity(for invitation: JarvisMacEnrollmentInvitation) throws -> JarvisMacEnrollmentCSR {
        try Self.lock.withLock {
            if let installed: InstalledRecord = try readRecord(account: Self.installedAccount) {
                guard installed.profile.deviceID == invitation.deviceID else {
                    throw JarvisMacDeveloperBridgeError.bindingMismatch
                }
                throw JarvisMacDeveloperBridgeError.identityUnavailable
            }
            if let staged: StagedRecord = try readRecord(account: Self.stagedAccount) {
                guard staged.invitation == invitation else {
                    throw JarvisMacDeveloperBridgeError.bindingMismatch
                }
                let key = try loadPrivateKey()
                return try makeCSR(invitation: invitation, privateKey: key)
            }
            guard try findPrivateKey() == nil else {
                throw JarvisMacDeveloperBridgeError.identityUnavailable
            }
            let key = try createSecureEnclaveKey()
            do {
                let reply = try makeCSR(invitation: invitation, privateKey: key)
                let record = StagedRecord(invitation: invitation)
                try saveRecord(record, account: Self.stagedAccount)
                return reply
            } catch {
                try? deletePrivateKey()
                throw error
            }
        }
    }

    public func loadStagedInvitation() throws -> JarvisMacEnrollmentInvitation? {
        try Self.lock.withLock {
            let staged: StagedRecord? = try readRecord(account: Self.stagedAccount)
            return staged?.invitation
        }
    }

    public func install(
        _ receipt: JarvisMacIssuedDeviceCertificate,
        for invitation: JarvisMacEnrollmentInvitation
    ) throws -> JarvisMacBridgeProfile {
        try Self.lock.withLock {
            let staged: StagedRecord? = try readRecord(account: Self.stagedAccount)
            guard staged?.invitation == invitation else {
                throw JarvisMacDeveloperBridgeError.noStagedEnrollment
            }
            let privateKey = try loadPrivateKey()
            let leafDER = try decodePEM(receipt.certificatePEM, label: "CERTIFICATE")
            let caDER = try decodePEM(receipt.caCertificatePEM, label: "CERTIFICATE")
            guard hex(SHA256.hash(data: leafDER)) == receipt.certificateSHA256.lowercased(),
                  hex(SHA256.hash(data: caDER)) == invitation.caFingerprintSHA256.lowercased(),
                  let leaf = SecCertificateCreateWithData(nil, leafDER as CFData),
                  let ca = SecCertificateCreateWithData(nil, caDER as CFData) else {
                throw JarvisMacDeveloperBridgeError.certificateInvalid
            }
            try validateCertificate(
                leaf: leaf,
                leafDER: leafDER,
                ca: ca,
                privateKey: privateKey,
                expectedDeviceID: invitation.deviceID,
                expectedDeviceName: invitation.deviceName,
                expectedSerialHex: receipt.serialHex,
                expectedNotAfterMilliseconds: receipt.notAfterMilliseconds
            )
            let profile = JarvisMacBridgeProfile(
                deviceID: receipt.deviceID,
                deviceName: receipt.deviceName,
                role: receipt.role,
                registryRevision: receipt.registryRevision,
                capabilities: invitation.capabilities,
                masterEndpoint: invitation.masterEndpoint,
                certificateNotAfterMilliseconds: receipt.notAfterMilliseconds
            )
            let record = InstalledRecord(
                profile: profile,
                certificatePEM: receipt.certificatePEM,
                caCertificatePEM: receipt.caCertificatePEM,
                caFingerprintSHA256: invitation.caFingerprintSHA256.lowercased()
            )
            try saveRecord(record, account: Self.installedAccount)
            do {
                try deleteRecord(account: Self.stagedAccount)
            } catch {
                try? deleteRecord(account: Self.installedAccount)
                throw error
            }
            return profile
        }
    }

    public func loadInstalledProfile() throws -> JarvisMacBridgeProfile? {
        try Self.lock.withLock {
            let record: InstalledRecord? = try readRecord(account: Self.installedAccount)
            return record?.profile
        }
    }

    func loadTLSIdentityMaterial() throws -> JarvisMacTLSIdentityMaterial {
        try Self.lock.withLock {
            guard let record: InstalledRecord = try readRecord(account: Self.installedAccount) else {
                throw JarvisMacDeveloperBridgeError.identityUnavailable
            }
            let privateKey = try loadPrivateKey()
            let leafDER = try decodePEM(record.certificatePEM, label: "CERTIFICATE")
            let caDER = try decodePEM(record.caCertificatePEM, label: "CERTIFICATE")
            guard hex(SHA256.hash(data: caDER)) == record.caFingerprintSHA256,
                  let leaf = SecCertificateCreateWithData(nil, leafDER as CFData),
                  let ca = SecCertificateCreateWithData(nil, caDER as CFData),
                  let identity = SecIdentityCreate(nil, leaf, privateKey) else {
                throw JarvisMacDeveloperBridgeError.certificateInvalid
            }
            try validateCertificate(
                leaf: leaf,
                leafDER: leafDER,
                ca: ca,
                privateKey: privateKey,
                expectedDeviceID: record.profile.deviceID,
                expectedDeviceName: record.profile.deviceName,
                expectedSerialHex: nil,
                expectedNotAfterMilliseconds: record.profile.certificateNotAfterMilliseconds
            )
            return JarvisMacTLSIdentityMaterial(identity: identity, caCertificate: ca)
        }
    }

    private func createSecureEnclaveKey() throws -> SecKey {
        var accessError: Unmanaged<CFError>?
        guard let access = SecAccessControlCreateWithFlags(
            nil,
            kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            [.privateKeyUsage],
            &accessError
        ) else {
            throw JarvisMacDeveloperBridgeError.identityUnavailable
        }
        let attributes: [String: Any] = [
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrKeySizeInBits as String: 256,
            kSecAttrTokenID as String: kSecAttrTokenIDSecureEnclave,
            kSecUseDataProtectionKeychain as String: true,
            kSecPrivateKeyAttrs as String: [
                kSecAttrIsPermanent as String: true,
                kSecAttrApplicationTag as String: Self.keyTag,
                kSecAttrAccessControl as String: access
            ]
        ]
        var error: Unmanaged<CFError>?
        guard let key = SecKeyCreateRandomKey(attributes as CFDictionary, &error) else {
            throw JarvisMacDeveloperBridgeError.identityUnavailable
        }
        return key
    }

    private func findPrivateKey() throws -> SecKey? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrApplicationTag as String: Self.keyTag,
            kSecReturnRef as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
            kSecUseDataProtectionKeychain as String: true
        ]
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let key = result as! SecKey? else {
            throw JarvisMacDeveloperBridgeError.keychainFailure(status)
        }
        return key
    }

    private func loadPrivateKey() throws -> SecKey {
        guard let key = try findPrivateKey() else {
            throw JarvisMacDeveloperBridgeError.identityUnavailable
        }
        return key
    }

    private func deletePrivateKey() throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrApplicationTag as String: Self.keyTag,
            kSecUseDataProtectionKeychain as String: true
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw JarvisMacDeveloperBridgeError.keychainFailure(status)
        }
    }

    private func makeCSR(
        invitation: JarvisMacEnrollmentInvitation,
        privateKey: SecKey
    ) throws -> JarvisMacEnrollmentCSR {
        guard let publicKey = SecKeyCopyPublicKey(privateKey),
              let publicBytes = SecKeyCopyExternalRepresentation(publicKey, nil) as Data?,
              publicBytes.count == 65, publicBytes.first == 0x04 else {
            throw JarvisMacDeveloperBridgeError.identityUnavailable
        }
        let requestInfo = certificationRequestInfo(
            commonName: invitation.deviceName,
            publicKeyX963: publicBytes
        )
        var signatureError: Unmanaged<CFError>?
        guard let signature = SecKeyCreateSignature(
            privateKey,
            .ecdsaSignatureMessageX962SHA256,
            requestInfo as CFData,
            &signatureError
        ) as Data? else {
            throw JarvisMacDeveloperBridgeError.identityUnavailable
        }
        let signatureAlgorithm = derSequence(derOID([0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02]))
        let csr = derSequence(requestInfo + signatureAlgorithm + derBitString(signature))
        let pem = pemEncode(csr, label: "CERTIFICATE REQUEST")
        guard pem.utf8.count <= JarvisMacEnrollmentCoordinator.maximumDocumentBytes else {
            throw JarvisMacDeveloperBridgeError.documentTooLarge
        }
        return JarvisMacEnrollmentCSR(
            schemaVersion: 1,
            status: "enrollment_csr_ready",
            grantID: invitation.grantID,
            deviceID: invitation.deviceID,
            csrPEM: pem
        )
    }

    private func certificationRequestInfo(commonName: String, publicKeyX963: Data) -> Data {
        let commonNameOID = derOID([0x55, 0x04, 0x03])
        let commonNameValue = derUTF8String(Data(commonName.utf8))
        let subject = derSequence(derSet(derSequence(commonNameOID + commonNameValue)))
        let ecPublicKeyOID = derOID([0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01])
        let p256OID = derOID([0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07])
        let subjectPublicKeyInfo = derSequence(
            derSequence(ecPublicKeyOID + p256OID) + derBitString(publicKeyX963)
        )
        return derSequence(derIntegerZero() + subject + subjectPublicKeyInfo + Data([0xa0, 0x00]))
    }

    private func validateCertificate(
        leaf: SecCertificate,
        leafDER: Data,
        ca: SecCertificate,
        privateKey: SecKey,
        expectedDeviceID: String,
        expectedDeviceName: String,
        expectedSerialHex: String?,
        expectedNotAfterMilliseconds: UInt64
    ) throws {
        var commonName: CFString?
        let commonNameStatus = SecCertificateCopyCommonName(leaf, &commonName)
        let serial = SecCertificateCopySerialNumberData(leaf, nil) as Data?
        let actualNotAfter = certificateNotAfterMilliseconds(leaf)
        let serialMatches = expectedSerialHex.map { expected in
            serial.map { hex($0) == expected.lowercased() } == true
        } ?? true
        let expiryMatches = actualNotAfter.map { actual in
            actual <= expectedNotAfterMilliseconds
                && expectedNotAfterMilliseconds - actual < 1_000
        } == true
        guard let leafPublicKey = SecCertificateCopyKey(leaf),
              let privatePublicKey = SecKeyCopyPublicKey(privateKey),
              let leafBytes = SecKeyCopyExternalRepresentation(leafPublicKey, nil) as Data?,
              let keyBytes = SecKeyCopyExternalRepresentation(privatePublicKey, nil) as Data?,
              leafBytes == keyBytes,
              SecIdentityCreate(nil, leaf, privateKey) != nil,
              certificateHasExactDeviceSAN(leafDER, deviceID: expectedDeviceID),
              certificateHasClientAuthenticationUsage(leafDER),
              commonNameStatus == errSecSuccess,
              (commonName as String?) == expectedDeviceName,
              serialMatches,
              expiryMatches else {
            throw JarvisMacDeveloperBridgeError.certificateInvalid
        }
        var trust: SecTrust?
        guard SecTrustCreateWithCertificates(
            [leaf] as CFArray,
            SecPolicyCreateBasicX509(),
            &trust
        ) == errSecSuccess, let trust else {
            throw JarvisMacDeveloperBridgeError.certificateInvalid
        }
        SecTrustSetAnchorCertificates(trust, [ca] as CFArray)
        SecTrustSetAnchorCertificatesOnly(trust, true)
        guard SecTrustEvaluateWithError(trust, nil) else {
            throw JarvisMacDeveloperBridgeError.certificateInvalid
        }
    }

    private func readRecord<T: Decodable>(account: String) throws -> T? {
        var query = baseRecordQuery(account: account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data,
              let record = try? JSONDecoder().decode(T.self, from: data) else {
            if status != errSecSuccess {
                throw JarvisMacDeveloperBridgeError.keychainFailure(status)
            }
            throw JarvisMacDeveloperBridgeError.identityUnavailable
        }
        return record
    }

    private func saveRecord<T: Encodable>(_ record: T, account: String) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(record)
        guard data.count <= JarvisMacEnrollmentCoordinator.maximumDocumentBytes else {
            throw JarvisMacDeveloperBridgeError.documentTooLarge
        }
        let query = baseRecordQuery(account: account)
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        ]
        let update = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if update == errSecSuccess { return }
        guard update == errSecItemNotFound else {
            throw JarvisMacDeveloperBridgeError.keychainFailure(update)
        }
        var add = query
        add.merge(attributes) { _, new in new }
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw JarvisMacDeveloperBridgeError.keychainFailure(status)
        }
    }

    private func deleteRecord(account: String) throws {
        let status = SecItemDelete(baseRecordQuery(account: account) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw JarvisMacDeveloperBridgeError.keychainFailure(status)
        }
    }

    private func baseRecordQuery(account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Self.service,
            kSecAttrAccount as String: account,
            kSecUseDataProtectionKeychain as String: true
        ]
    }
}

struct JarvisMacTLSIdentityMaterial: @unchecked Sendable {
    let identity: SecIdentity
    let caCertificate: SecCertificate
}

private struct StagedRecord: Codable {
    let invitation: JarvisMacEnrollmentInvitation
}

private struct InstalledRecord: Codable {
    let profile: JarvisMacBridgeProfile
    let certificatePEM: String
    let caCertificatePEM: String
    let caFingerprintSHA256: String
}

private func decodePEM(_ value: String, label: String) throws -> Data {
    let header = "-----BEGIN \(label)-----"
    let footer = "-----END \(label)-----"
    guard value.hasPrefix(header), value.contains(footer) else {
        throw JarvisMacDeveloperBridgeError.certificateInvalid
    }
    let base64 = value
        .replacingOccurrences(of: header, with: "")
        .replacingOccurrences(of: footer, with: "")
        .components(separatedBy: .whitespacesAndNewlines)
        .joined()
    guard let data = Data(base64Encoded: base64), !data.isEmpty else {
        throw JarvisMacDeveloperBridgeError.certificateInvalid
    }
    return data
}

private func pemEncode(_ data: Data, label: String) -> String {
    let base64 = data.base64EncodedString()
    let lines = stride(from: 0, to: base64.count, by: 64).map { offset -> String in
        let start = base64.index(base64.startIndex, offsetBy: offset)
        let end = base64.index(start, offsetBy: min(64, base64.distance(from: start, to: base64.endIndex)))
        return String(base64[start..<end])
    }
    return "-----BEGIN \(label)-----\n\(lines.joined(separator: "\n"))\n-----END \(label)-----\n"
}

private func derSequence(_ body: Data) -> Data { der(tag: 0x30, body) }
private func derSet(_ body: Data) -> Data { der(tag: 0x31, body) }
private func derOID(_ body: [UInt8]) -> Data { der(tag: 0x06, Data(body)) }
private func derUTF8String(_ body: Data) -> Data { der(tag: 0x0c, body) }
private func derIntegerZero() -> Data { Data([0x02, 0x01, 0x00]) }
private func derBitString(_ body: Data) -> Data { der(tag: 0x03, Data([0]) + body) }

private func der(tag: UInt8, _ body: Data) -> Data {
    Data([tag]) + derLength(body.count) + body
}

private func derLength(_ count: Int) -> Data {
    if count < 128 { return Data([UInt8(count)]) }
    var value = count
    var bytes: [UInt8] = []
    while value > 0 {
        bytes.insert(UInt8(value & 0xff), at: 0)
        value >>= 8
    }
    return Data([0x80 | UInt8(bytes.count)] + bytes)
}

private func hex<D: Sequence>(_ bytes: D) -> String where D.Element == UInt8 {
    bytes.map { String(format: "%02x", $0) }.joined()
}

private func certificateNotAfterMilliseconds(_ certificate: SecCertificate) -> UInt64? {
    guard let values = SecCertificateCopyValues(
        certificate,
        [kSecOIDX509V1ValidityNotAfter] as CFArray,
        nil
    ) as? [CFString: Any],
    let property = values[kSecOIDX509V1ValidityNotAfter] as? [CFString: Any],
    let date = property[kSecPropertyKeyValue] as? Date else { return nil }
    let milliseconds = date.timeIntervalSince1970 * 1_000
    guard milliseconds.isFinite, milliseconds >= 0, milliseconds <= Double(UInt64.max) else {
        return nil
    }
    return UInt64(milliseconds.rounded(.down))
}

private struct DERTLV {
    let tag: UInt8
    let body: Data
}

private func certificateHasExactDeviceSAN(_ certificate: Data, deviceID: String) -> Bool {
    guard let san = certificateExtension(certificate, oid: [0x55, 0x1d, 0x11]),
          let names = derChildren(san), names.count == 1, names[0].tag == 0x30,
          let generalNames = derChildren(names[0].body) else { return false }
    let jarvisURIs = generalNames.compactMap { name -> String? in
        guard name.tag == 0x86, let value = String(data: name.body, encoding: .ascii),
              value.hasPrefix("urn:jarvis:device:") else { return nil }
        return value
    }
    return jarvisURIs == ["urn:jarvis:device:\(deviceID.lowercased())"]
}

private func certificateHasClientAuthenticationUsage(_ certificate: Data) -> Bool {
    guard let usage = certificateExtension(certificate, oid: [0x55, 0x1d, 0x25]),
          let outer = derChildren(usage), outer.count == 1, outer[0].tag == 0x30,
          let purposes = derChildren(outer[0].body) else { return false }
    return purposes.contains {
        $0.tag == 0x06 && Array($0.body) == [0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x02]
    }
}

private func certificateExtension(_ certificate: Data, oid: [UInt8]) -> Data? {
    guard let root = derChildren(certificate), root.count == 1, root[0].tag == 0x30,
          let certificateParts = derChildren(root[0].body),
          let tbs = certificateParts.first, tbs.tag == 0x30,
          let tbsParts = derChildren(tbs.body),
          let extensions = tbsParts.first(where: { $0.tag == 0xa3 }),
          let extensionWrapper = derChildren(extensions.body), extensionWrapper.count == 1,
          extensionWrapper[0].tag == 0x30,
          let entries = derChildren(extensionWrapper[0].body) else { return nil }
    var matches: [Data] = []
    for entry in entries where entry.tag == 0x30 {
        guard let fields = derChildren(entry.body), fields.count == 2 || fields.count == 3,
              fields[0].tag == 0x06, Array(fields[0].body) == oid,
              let value = fields.last, value.tag == 0x04 else { continue }
        matches.append(value.body)
    }
    return matches.count == 1 ? matches[0] : nil
}

private func derChildren(_ data: Data) -> [DERTLV]? {
    var offset = 0
    var values: [DERTLV] = []
    while offset < data.count {
        guard offset + 2 <= data.count else { return nil }
        let tag = data[offset]
        offset += 1
        let firstLength = data[offset]
        offset += 1
        let length: Int
        if firstLength & 0x80 == 0 {
            length = Int(firstLength)
        } else {
            let count = Int(firstLength & 0x7f)
            guard count > 0, count <= MemoryLayout<Int>.size, offset + count <= data.count else {
                return nil
            }
            var decoded = 0
            for _ in 0..<count {
                guard decoded <= (Int.max >> 8) else { return nil }
                decoded = (decoded << 8) | Int(data[offset])
                offset += 1
            }
            length = decoded
        }
        guard length >= 0, offset <= data.count - length else { return nil }
        values.append(DERTLV(tag: tag, body: data.subdata(in: offset..<(offset + length))))
        offset += length
    }
    return values
}
