import Foundation
import Security

enum KeychainStore {
    private static let service = "io.github.mith-mmk.wml2viewer.smb"

    static func save(password: String, reference: String) throws {
        let data = Data(password.utf8)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: reference,
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
        ]
        let status = SecItemAdd(query as CFDictionary, nil)
        if status == errSecDuplicateItem {
            let lookup: [String: Any] = [
                kSecClass as String: kSecClassGenericPassword,
                kSecAttrService as String: service,
                kSecAttrAccount as String: reference,
            ]
            let updateStatus = SecItemUpdate(lookup as CFDictionary, [
                kSecValueData as String: data,
            ] as CFDictionary)
            guard updateStatus == errSecSuccess else { throw KeychainError(status: updateStatus) }
        } else if status != errSecSuccess {
            throw KeychainError(status: status)
        }
    }

    static func load(reference: String) throws -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: reference,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else {
            throw KeychainError(status: status)
        }
        return String(data: data, encoding: .utf8)
    }

    static func remove(reference: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: reference,
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw KeychainError(status: status)
        }
    }

    struct KeychainError: Error {
        let status: OSStatus
    }
}

@_cdecl("wml2viewer_ios_keychain_save_password")
func wml2viewer_ios_keychain_save_password(
    _ reference: UnsafePointer<CChar>?,
    _ password: UnsafePointer<CChar>?
) -> Int32 {
    guard let reference, let password else { return -1 }
    do {
        try KeychainStore.save(
            password: String(cString: password),
            reference: String(cString: reference)
        )
        return 0
    } catch {
        return -1
    }
}

@_cdecl("wml2viewer_ios_keychain_copy_password")
func wml2viewer_ios_keychain_copy_password(
    _ reference: UnsafePointer<CChar>?,
    _ output: UnsafeMutablePointer<CChar>?,
    _ capacity: Int
) -> Int32 {
    guard let reference, let output, capacity > 0 else { return -1 }
    do {
        guard let password = try KeychainStore.load(reference: String(cString: reference)) else {
            return 0
        }
        let bytes = Array(password.utf8)
        guard bytes.count + 1 <= capacity else { return -1 }
        bytes.withUnsafeBytes { rawBuffer in
            if let baseAddress = rawBuffer.baseAddress {
                memcpy(output, baseAddress, bytes.count)
            }
        }
        output[bytes.count] = 0
        return Int32(bytes.count)
    } catch {
        return -1
    }
}
