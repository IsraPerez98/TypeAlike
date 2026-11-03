//! Opens a web page in the default browser of the user.

use std::thread;

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

use crate::uia::ComApartment;

/// The highest value that `ShellExecuteW` gives back for a failure.
const HIGHEST_FAILURE_CODE: usize = 32;

/// Opens `url` in the default browser, on a thread of its own, so that the start of the browser
/// never delays the hooks of the main thread.
pub(crate) fn open(url: &str) {
    let url = HSTRING::from(url);
    thread::spawn(move || {
        // A shell extension that opens the address can need COM.
        let _apartment = ComApartment::join()
            .inspect_err(|error| tracing::warn!(%error, "failed to join COM to open a web page"))
            .ok();
        // SAFETY: each string is nul-terminated and lives during the call, and no window owns it.
        let instance = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                &url,
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        let code = instance.0.addr();
        if code <= HIGHEST_FAILURE_CODE {
            tracing::warn!(code, "failed to open a web page");
        }
    });
}
