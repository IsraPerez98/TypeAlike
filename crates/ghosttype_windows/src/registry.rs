//! Reads values in the registry of the current user.

use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_ROUTINE_FLAGS, RegGetValueW};
use windows::core::PCWSTR;

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
