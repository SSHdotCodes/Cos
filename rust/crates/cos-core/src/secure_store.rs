use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::string::CFString;
use core_foundation_sys::base::CFTypeRef;
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::string::CFStringRef;
use std::fmt;

use crate::cf::dictionary_from_raw_pairs;

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecItemAdd(attributes: CFDictionaryRef, result: *mut CFTypeRef) -> i32;
    fn SecItemCopyMatching(query: CFDictionaryRef, result: *mut CFTypeRef) -> i32;
    fn SecItemUpdate(query: CFDictionaryRef, attributes_to_update: CFDictionaryRef) -> i32;
    fn SecItemDelete(query: CFDictionaryRef) -> i32;

    static kSecClass: CFStringRef;
    static kSecClassGenericPassword: CFStringRef;
    static kSecAttrService: CFStringRef;
    static kSecAttrAccount: CFStringRef;
    static kSecValueData: CFStringRef;
    static kSecReturnData: CFStringRef;
    static kSecMatchLimit: CFStringRef;
    static kSecMatchLimitOne: CFStringRef;
    static kSecAttrAccessible: CFStringRef;
    static kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly: CFStringRef;
}

#[derive(Debug)]
pub enum SecureStoreError {
    UnexpectedStatus(i32),
    InvalidData,
}

impl fmt::Display for SecureStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedStatus(status) => write!(f, "Keychain returned status {status}."),
            Self::InvalidData => write!(f, "The Keychain value is not valid UTF-8."),
        }
    }
}

impl std::error::Error for SecureStoreError {}

pub const DEFAULT_SERVICE: &str = "codes.ssh.cos";

#[derive(Debug, Clone)]
pub struct SecureStore {
    pub service: String,
}

impl Default for SecureStore {
    fn default() -> Self {
        Self { service: DEFAULT_SERVICE.to_string() }
    }
}

const ERR_SEC_SUCCESS: i32 = 0;
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

impl SecureStore {
    pub fn set(&self, secret: &str, account: &str) -> Result<(), SecureStoreError> {
        unsafe {
            let service = CFString::new(&self.service);
            let account = CFString::new(account);
            let data = CFData::from_buffer(secret.as_bytes());

            let base = dictionary_from_raw_pairs(&[
                (kSecClass as CFTypeRef, kSecClassGenericPassword as CFTypeRef),
                (kSecAttrService as CFTypeRef, service.as_concrete_TypeRef() as CFTypeRef),
                (kSecAttrAccount as CFTypeRef, account.as_concrete_TypeRef() as CFTypeRef),
            ]);
            let update = dictionary_from_raw_pairs(&[(
                kSecValueData as CFTypeRef,
                data.as_concrete_TypeRef() as CFTypeRef,
            )]);
            let update_status = SecItemUpdate(
                base.as_concrete_TypeRef(),
                update.as_concrete_TypeRef(),
            );

            if update_status == ERR_SEC_ITEM_NOT_FOUND {
                let add = dictionary_from_raw_pairs(&[
                    (kSecClass as CFTypeRef, kSecClassGenericPassword as CFTypeRef),
                    (kSecAttrService as CFTypeRef, service.as_concrete_TypeRef() as CFTypeRef),
                    (kSecAttrAccount as CFTypeRef, account.as_concrete_TypeRef() as CFTypeRef),
                    (kSecValueData as CFTypeRef, data.as_concrete_TypeRef() as CFTypeRef),
                    (
                        kSecAttrAccessible as CFTypeRef,
                        kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly as CFTypeRef,
                    ),
                ]);
                let status = SecItemAdd(add.as_concrete_TypeRef(), std::ptr::null_mut());
                if status != ERR_SEC_SUCCESS {
                    return Err(SecureStoreError::UnexpectedStatus(status));
                }
            } else if update_status != ERR_SEC_SUCCESS {
                return Err(SecureStoreError::UnexpectedStatus(update_status));
            }
            Ok(())
        }
    }

    pub fn get(&self, account: &str) -> Result<Option<String>, SecureStoreError> {
        unsafe {
            let service = CFString::new(&self.service);
            let account = CFString::new(account);
            let query = dictionary_from_raw_pairs(&[
                (kSecClass as CFTypeRef, kSecClassGenericPassword as CFTypeRef),
                (kSecAttrService as CFTypeRef, service.as_concrete_TypeRef() as CFTypeRef),
                (kSecAttrAccount as CFTypeRef, account.as_concrete_TypeRef() as CFTypeRef),
                (
                    kSecReturnData as CFTypeRef,
                    CFBoolean::true_value().as_concrete_TypeRef() as CFTypeRef,
                ),
                (kSecMatchLimit as CFTypeRef, kSecMatchLimitOne as CFTypeRef),
            ]);
            let mut result: CFTypeRef = std::ptr::null();
            let status = SecItemCopyMatching(query.as_concrete_TypeRef(), &mut result);
            if status == ERR_SEC_ITEM_NOT_FOUND {
                return Ok(None);
            }
            if status != ERR_SEC_SUCCESS {
                return Err(SecureStoreError::UnexpectedStatus(status));
            }
            if result.is_null() {
                return Ok(None);
            }
            let value = CFType::wrap_under_create_rule(result);
            let Some(data) = value.downcast::<CFData>() else {
                return Err(SecureStoreError::InvalidData);
            };
            String::from_utf8(data.bytes().to_vec())
                .map(Some)
                .map_err(|_| SecureStoreError::InvalidData)
        }
    }

    pub fn remove(&self, account: &str) -> Result<(), SecureStoreError> {
        unsafe {
            let service = CFString::new(&self.service);
            let account = CFString::new(account);
            let query = dictionary_from_raw_pairs(&[
                (kSecClass as CFTypeRef, kSecClassGenericPassword as CFTypeRef),
                (kSecAttrService as CFTypeRef, service.as_concrete_TypeRef() as CFTypeRef),
                (kSecAttrAccount as CFTypeRef, account.as_concrete_TypeRef() as CFTypeRef),
            ]);
            let status = SecItemDelete(query.as_concrete_TypeRef());
            if status != ERR_SEC_SUCCESS && status != ERR_SEC_ITEM_NOT_FOUND {
                return Err(SecureStoreError::UnexpectedStatus(status));
            }
            Ok(())
        }
    }
}
