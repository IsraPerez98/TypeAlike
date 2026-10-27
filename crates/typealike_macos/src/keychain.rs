//! Keeps the key of the earlier writing as a generic password in the login keychain.

use std::io;
use std::ptr::{self, NonNull};

use objc2_core_foundation::{CFBoolean, CFData, CFDictionary, CFRetained, CFString, CFType};
use objc2_security::{
    SecItemAdd, SecItemCopyMatching, errSecItemNotFound, errSecSuccess, kSecAttrAccount, kSecAttrService,
    kSecClass, kSecClassGenericPassword, kSecMatchLimit, kSecMatchLimitOne, kSecReturnData, kSecValueData,
};
use typealike_core::history::{HistoryKey, KeyVault};

const SERVICE: &str = "io.github.israperez98.typealike";
const ACCOUNT: &str = "history-key";

/// The login keychain, which keeps the key of the earlier writing.
#[derive(Debug, Default)]
pub struct Keychain;

impl KeyVault for Keychain {
    fn load(&mut self) -> io::Result<Option<HistoryKey>> {
        let service = CFString::from_str(SERVICE);
        let account = CFString::from_str(ACCOUNT);
        // SAFETY: reads immutable system constants.
        let (keys, class, limit) = unsafe {
            (
                [
                    kSecClass,
                    kSecAttrService,
                    kSecAttrAccount,
                    kSecReturnData,
                    kSecMatchLimit,
                ],
                kSecClassGenericPassword,
                kSecMatchLimitOne,
            )
        };
        let values: [&CFType; 5] = [class, &service, &account, CFBoolean::new(true), limit];
        let query = CFDictionary::from_slices(&keys, &values);
        let mut found: *const CFType = ptr::null();
        // SAFETY: the query maps CFString keys to the values of a generic password search, and
        // `found` is a valid place for the result.
        let status = unsafe { SecItemCopyMatching(query.as_opaque(), &raw mut found) };
        if status == errSecItemNotFound {
            return Ok(None);
        }
        if status != errSecSuccess {
            return Err(keychain_error(status));
        }
        // SAFETY: on success, a copy call gives a reference that the caller owns.
        let found = NonNull::new(found.cast_mut()).map(|found| unsafe { CFRetained::from_raw(found) });
        let bytes = found
            .and_then(|found| found.downcast::<CFData>().ok())
            .map(|data| data.to_vec())
            .ok_or_else(|| io::Error::other("the keychain gave no key data"))?;
        let key = <[u8; 32]>::try_from(bytes)
            .map_err(|_bytes| io::Error::other("the keychain key has a wrong length"))?;
        Ok(Some(HistoryKey(key)))
    }

    fn store(&mut self, key: &HistoryKey) -> io::Result<()> {
        let service = CFString::from_str(SERVICE);
        let account = CFString::from_str(ACCOUNT);
        let data = CFData::from_bytes(&key.0);
        // SAFETY: reads immutable system constants.
        let (keys, class) = unsafe {
            (
                [kSecClass, kSecAttrService, kSecAttrAccount, kSecValueData],
                kSecClassGenericPassword,
            )
        };
        let values: [&CFType; 4] = [class, &service, &account, &data];
        let attributes = CFDictionary::from_slices(&keys, &values);
        // SAFETY: the attributes map CFString keys to the values of a generic password, and the
        // call gives no result.
        let status = unsafe { SecItemAdd(attributes.as_opaque(), ptr::null_mut()) };
        if status == errSecSuccess {
            Ok(())
        } else {
            Err(keychain_error(status))
        }
    }
}

fn keychain_error(status: i32) -> io::Error {
    io::Error::other(format!("the keychain failed with status {status}"))
}
