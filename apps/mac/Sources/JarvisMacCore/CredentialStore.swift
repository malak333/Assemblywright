import Foundation

#if canImport(Security)
import Security
#endif

public enum JarvisCredentialKey: String, CaseIterable, Sendable {
    case openAIAPIKey = "openai-api-key"

    public var environmentKey: String {
        switch self {
        case .openAIAPIKey:
            return "JARVIS_OPENAI_API_KEY"
        }
    }
}

public enum JarvisCredentialStoreError: Error, Equatable, Sendable, CustomStringConvertible {
    case unavailable(String)
    case invalidData(String)
    case keychainStatus(String, Int32)

    public var description: String {
        switch self {
        case let .unavailable(reason):
            return "Credential store unavailable: \(reason)"
        case let .invalidData(reason):
            return "Credential data is invalid: \(reason)"
        case let .keychainStatus(operation, status):
            return "Keychain \(operation) failed with status \(status)."
        }
    }
}

public protocol JarvisCredentialStore: Sendable {
    func readCredential(_ key: JarvisCredentialKey) throws -> String?
    func saveCredential(_ value: String, for key: JarvisCredentialKey) throws
    func deleteCredential(_ key: JarvisCredentialKey) throws
}

public struct JarvisCoreCredentialProvider: Sendable {
    public var store: any JarvisCredentialStore

    public init(store: any JarvisCredentialStore = KeychainJarvisCredentialStore()) {
        self.store = store
    }

    public func launchEnvironment(base: [String: String]) -> [String: String] {
        var environment = base
        for key in JarvisCredentialKey.allCases {
            guard shouldLoad(key, into: environment) else {
                continue
            }
            guard environment[key.environmentKey, default: ""].isEmpty else {
                continue
            }
            guard let credential = try? store.readCredential(key), !credential.isEmpty else {
                continue
            }
            environment[key.environmentKey] = credential
        }
        return environment
    }

    private func shouldLoad(_ key: JarvisCredentialKey, into environment: [String: String]) -> Bool {
        switch key {
        case .openAIAPIKey:
            guard environment["JARVIS_CHATGPT_ENABLED"] == "true" else {
                return false
            }
            let authMode = (environment["JARVIS_CHATGPT_AUTH"]
                ?? environment["JARVIS_CHATGPT_AUTH_MODE"]
                ?? "api_key")
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased()
            return authMode != "codex_account"
        }
    }
}

#if canImport(Security)
public struct KeychainJarvisCredentialStore: JarvisCredentialStore {
    public var service: String

    public init(service: String = "com.nobiletechnology.jarvis.credentials") {
        self.service = service
    }

    public func readCredential(_ key: JarvisCredentialKey) throws -> String? {
        var query = baseQuery(for: key)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound {
            return nil
        }
        guard status == errSecSuccess else {
            throw JarvisCredentialStoreError.keychainStatus("read", status)
        }
        guard let data = item as? Data else {
            throw JarvisCredentialStoreError.invalidData("Keychain item was not data.")
        }
        guard let value = String(data: data, encoding: .utf8) else {
            throw JarvisCredentialStoreError.invalidData("Keychain item was not UTF-8.")
        }
        return value
    }

    public func saveCredential(_ value: String, for key: JarvisCredentialKey) throws {
        let data = Data(value.utf8)
        let query = baseQuery(for: key)
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        ]

        let updateStatus = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if updateStatus == errSecSuccess {
            return
        }
        guard updateStatus == errSecItemNotFound else {
            throw JarvisCredentialStoreError.keychainStatus("update", updateStatus)
        }

        var addQuery = query
        addQuery.merge(attributes) { _, new in new }
        let addStatus = SecItemAdd(addQuery as CFDictionary, nil)
        guard addStatus == errSecSuccess else {
            throw JarvisCredentialStoreError.keychainStatus("save", addStatus)
        }
    }

    public func deleteCredential(_ key: JarvisCredentialKey) throws {
        let status = SecItemDelete(baseQuery(for: key) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw JarvisCredentialStoreError.keychainStatus("delete", status)
        }
    }

    private func baseQuery(for key: JarvisCredentialKey) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key.rawValue
        ]
    }
}
#else
public struct KeychainJarvisCredentialStore: JarvisCredentialStore {
    public init(service _: String = "com.nobiletechnology.jarvis.credentials") {}

    public func readCredential(_: JarvisCredentialKey) throws -> String? {
        throw JarvisCredentialStoreError.unavailable("Security framework is not available in this build.")
    }

    public func saveCredential(_: String, for _: JarvisCredentialKey) throws {
        throw JarvisCredentialStoreError.unavailable("Security framework is not available in this build.")
    }

    public func deleteCredential(_: JarvisCredentialKey) throws {
        throw JarvisCredentialStoreError.unavailable("Security framework is not available in this build.")
    }
}
#endif
