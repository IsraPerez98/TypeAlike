//! Reads and changes values in the registry of the current user.

use std::ffi::OsStr;
use std::iter;
use std::os::windows::ffi::OsStrExt;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, HANDLE};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME, REG_ROUTINE_FLAGS,
    REG_SZ, RRF_RT_ANY, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegEnumKeyExW, RegGetValueW,
    RegNotifyChangeKeyValue, RegOpenKeyExW, RegSetKeyValueW,
};
use windows::core::{Owned, PCWSTR, PWSTR, Result};

use crate::utf16;

/// The longest name of a key, in UTF-16 units, without its nul.
const MAX_KEY_NAME_UNITS: usize = 255;
/// The longest text value that [`OpenKey::subkey_text`] reads, in UTF-16 units.
const MAX_TEXT_UNITS: usize = 1024;

/// Reads the value `name` of `kind` in `key` into `buffer`. Returns how many bytes it holds.
pub(crate) fn read_value(
    key: PCWSTR,
    name: PCWSTR,
    kind: REG_ROUTINE_FLAGS,
    buffer: &mut [u8],
) -> Option<usize> {
    let mut size: u32 = buffer.len().try_into().ok()?;
    // SAFETY: the output buffer is a live slice of exactly the size passed.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            name,
            kind,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&raw mut size),
        )
    };
    status.is_ok().then(|| usize::try_from(size).ok()).flatten()
}

/// Whether `key` holds the value `name`, of any type.
pub(crate) fn has_value(key: PCWSTR, name: PCWSTR) -> bool {
    // SAFETY: plain call with nul-terminated names and no output buffers.
    unsafe { RegGetValueW(HKEY_CURRENT_USER, key, name, RRF_RT_ANY, None, None, None) }.is_ok()
}

/// Sets the value `name` in `key` to the string `text`. Makes `key` when it is missing.
pub(crate) fn write_string(key: PCWSTR, name: PCWSTR, text: &OsStr) -> Result<()> {
    let wide: Vec<u16> = text.encode_wide().chain(iter::once(0)).collect();
    let size = u32::try_from(size_of_val(wide.as_slice())).expect("a registry string is shorter than 4 GiB");
    // SAFETY: the data is a live, nul-terminated UTF-16 buffer of exactly the size passed.
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key,
            name,
            REG_SZ.0,
            Some(wide.as_ptr().cast()),
            size,
        )
    }
    .ok()
}

/// Removes the value `name` from `key`. A missing value or key is not a failure.
pub(crate) fn delete_value(key: PCWSTR, name: PCWSTR) -> Result<()> {
    // SAFETY: plain call with nul-terminated names.
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key, name) };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    status.ok()
}

/// A key of the registry of the current user, open for reads and change notifications. Drop
/// closes it.
#[derive(Debug)]
pub(crate) struct OpenKey {
    key: Owned<HKEY>,
}

impl OpenKey {
    /// Opens `path` under the key of the current user.
    pub(crate) fn open(path: PCWSTR) -> Result<Self> {
        let mut key = HKEY::default();
        // SAFETY: the output pointer refers to a local, and the path is nul-terminated.
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path, None, KEY_READ, &raw mut key) }.ok()?;
        // SAFETY: RegOpenKeyExW returned a valid key that nothing else owns.
        let key = unsafe { Owned::new(key) };
        Ok(Self { key })
    }

    /// The names of the subkeys, each with a nul at its end.
    pub(crate) fn subkey_names(&self) -> Vec<Vec<u16>> {
        (0..).map_while(|index| self.subkey_name(index)).collect()
    }

    fn subkey_name(&self, index: u32) -> Option<Vec<u16>> {
        let mut units = [0_u16; MAX_KEY_NAME_UNITS + 1];
        let mut length = u32::try_from(units.len()).ok()?;
        // SAFETY: the buffer and its length are locals of the length passed.
        unsafe {
            RegEnumKeyExW(
                *self.key,
                index,
                Some(PWSTR(units.as_mut_ptr())),
                &raw mut length,
                None,
                None,
                None,
                None,
            )
        }
        .ok()
        .ok()?;
        let name = units.get(..usize::try_from(length).ok()?)?;
        Some(name.iter().copied().chain(iter::once(0)).collect())
    }

    /// Reads the number value `name` of the subkey `subkey`.
    pub(crate) fn subkey_number(&self, subkey: PCWSTR, name: PCWSTR) -> Option<u32> {
        let mut number = 0_u32;
        let mut size = u32::try_from(size_of::<u32>()).ok()?;
        // SAFETY: the output buffer is a local of exactly the size passed.
        unsafe {
            RegGetValueW(
                *self.key,
                subkey,
                name,
                RRF_RT_REG_DWORD,
                None,
                Some((&raw mut number).cast()),
                Some(&raw mut size),
            )
        }
        .ok()
        .ok()?;
        Some(number)
    }

    /// Reads the text value `name` of the subkey `subkey`. A text of more than 1024 UTF-16 units
    /// gives `None`.
    pub(crate) fn subkey_text(&self, subkey: PCWSTR, name: PCWSTR) -> Option<String> {
        let mut units = [0_u16; MAX_TEXT_UNITS];
        let mut size = u32::try_from(size_of_val(&units)).ok()?;
        // SAFETY: the output buffer is a local of exactly the size passed.
        unsafe {
            RegGetValueW(
                *self.key,
                subkey,
                name,
                RRF_RT_REG_SZ,
                None,
                Some(units.as_mut_ptr().cast()),
                Some(&raw mut size),
            )
        }
        .ok()
        .ok()?;
        let text = utf16::until_nul(&units);
        (!text.is_empty()).then(|| String::from_utf16_lossy(text))
    }

    /// Signals `event` one time at the next change of the key, of its values or of its subkeys.
    pub(crate) fn notify_change(&self, event: HANDLE) -> Result<()> {
        // SAFETY: the key and the event are open. An asynchronous notification ends with the
        // thread that asked for it, and this thread keeps `event` open while it lives.
        unsafe {
            RegNotifyChangeKeyValue(
                *self.key,
                true,
                REG_NOTIFY_CHANGE_NAME | REG_NOTIFY_CHANGE_LAST_SET,
                Some(event),
                true,
            )
        }
        .ok()
    }
}
