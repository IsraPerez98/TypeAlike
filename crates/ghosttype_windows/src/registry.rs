//! Reads and changes values in the registry of the current user.

use std::ffi::OsStr;
use std::iter;
use std::os::windows::ffi::OsStrExt;

use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_ROUTINE_FLAGS, REG_SZ, RRF_RT_ANY, RegDeleteKeyValueW, RegGetValueW,
    RegSetKeyValueW,
};
use windows::core::{PCWSTR, Result};

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
