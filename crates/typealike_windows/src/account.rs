//! The account of the user who runs TypeAlike.

use typealike_core::settings::WriterName;
use windows::Win32::NetworkManagement::NetManagement::{
    NERR_Success, NetApiBufferFree, NetUserGetInfo, USER_INFO_10,
};
use windows::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
use windows::core::{HSTRING, PCWSTR, PWSTR};

use crate::utf16::whole_chars;

const USER_INFO_LEVEL_10: u32 = 10;

/// The full name of the user account, such as "Sara Novak", for the name of the writer. It is
/// empty when the account has no full name.
#[must_use]
pub fn account_name() -> WriterName {
    let name = domain_display_name().or_else(local_full_name).unwrap_or_default();
    WriterName(name.trim().to_owned())
}

/// The display name of a domain account. Windows gives it only for an account in a domain.
fn domain_display_name() -> Option<String> {
    let mut length = 0;
    // SAFETY: with no buffer, the call fails and only writes the length that the name needs.
    unsafe { GetUserNameExW(NameDisplay, None, &raw mut length) };
    if length == 0 {
        return None;
    }
    let mut buffer = vec![0_u16; usize::try_from(length).expect("a u32 fits in usize")];
    // SAFETY: the buffer holds `length` units, the length that the first call asked for.
    let is_written =
        unsafe { GetUserNameExW(NameDisplay, Some(PWSTR(buffer.as_mut_ptr())), &raw mut length) };
    let written = usize::try_from(length).expect("a u32 fits in usize");
    is_written.then(|| whole_chars(&buffer[..written.min(buffer.len())]))
}

/// The full name of a local account. The local account of a Microsoft account has the name of
/// that account.
fn local_full_name() -> Option<String> {
    let user_name = HSTRING::from(std::env::var_os("USERNAME")?);
    let mut info: *mut u8 = std::ptr::null_mut();
    // SAFETY: on success, Windows writes a pointer to a USER_INFO_10 that the free below releases.
    let status = unsafe { NetUserGetInfo(PCWSTR::null(), &user_name, USER_INFO_LEVEL_10, &raw mut info) };
    if status != NERR_Success || info.is_null() {
        return None;
    }
    // SAFETY: level 10 gives a USER_INFO_10, and its strings live until the free.
    let full_name = unsafe {
        let full_name = info.cast::<USER_INFO_10>().read_unaligned().usri10_full_name;
        if full_name.is_null() {
            None
        } else {
            full_name.to_string().ok()
        }
    };
    // SAFETY: NetUserGetInfo allocated the buffer, and nothing reads it after the free.
    unsafe { NetApiBufferFree(Some(info.cast_const().cast())) };
    full_name
}
