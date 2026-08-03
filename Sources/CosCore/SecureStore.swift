import Foundation
import Security

public enum SecureStoreError: LocalizedError {
    case unexpectedStatus(OSStatus)
    case invalidData

    public var errorDescription: String? {
        switch self {
        case .unexpectedStatus(let status): "Keychain returned status \(status)."
        case .invalidData: "The Keychain value is not valid UTF-8."
        }
    }
}

public struct SecureStore: Sendable {
    public let service: String

    public init(service: String = "codes.ssh.cos") {
        self.service = service
    }

    public func set(_ secret: String, account: String) throws {
        let data = Data(secret.utf8)
        let base: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let updateStatus = SecItemUpdate(
            base as CFDictionary,
            [kSecValueData as String: data] as CFDictionary
        )

        if updateStatus == errSecItemNotFound {
            var add = base
            add[kSecValueData as String] = data
            add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            let status = SecItemAdd(add as CFDictionary, nil)
            guard status == errSecSuccess else { throw SecureStoreError.unexpectedStatus(status) }
        } else if updateStatus != errSecSuccess {
            throw SecureStoreError.unexpectedStatus(updateStatus)
        }
    }

    public func get(account: String) throws -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else { throw SecureStoreError.unexpectedStatus(status) }
        guard let data = result as? Data, let value = String(data: data, encoding: .utf8) else {
            throw SecureStoreError.invalidData
        }
        return value
    }

    public func remove(account: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw SecureStoreError.unexpectedStatus(status)
        }
    }
}
