//! The text that the user copied. Reading the clipboard can wait on the app that copied the text,
//! so GhostType reads it when the clipboard changes, and never in the keyboard hook.

use std::time::Instant;

use ghosttype_core::desktop::CopiedText;
use ghosttype_core::settings::ClipboardAccess;
use windows::Win32::Foundation::{HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, RemoveClipboardFormatListener,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::core::{PCWSTR, w};

/// `CF_UNICODETEXT`: text in UTF-16.
const UNICODE_TEXT_FORMAT: u32 = 13;
/// The formats that password managers and other apps add to text that no other app may read or
/// keep.
const PRIVATE_FORMATS: [PCWSTR; 2] = [
    w!("ExcludeClipboardContentFromMonitorProcessing"),
    w!("Clipboard Viewer Ignore"),
];

/// Keeps the copied text while the user allows it. Its window gets `WM_CLIPBOARDUPDATE` after each
/// copy.
#[derive(Debug)]
pub(crate) struct ClipboardWatch {
    window: HWND,
    is_listening: bool,
    access: ClipboardAccess,
    copied_text: Option<String>,
    /// When the last copy came, or `None` while the clipboard holds a copy from before the start.
    copied_at: Option<Instant>,
}

impl ClipboardWatch {
    /// Starts to send `WM_CLIPBOARDUPDATE` to `window`, a window of this thread, and reads the
    /// clipboard when `access` allows it.
    pub(crate) fn start(window: HWND, access: ClipboardAccess) -> Self {
        // SAFETY: the window is a live window of this thread, and drop removes the listener.
        let listened = unsafe { AddClipboardFormatListener(window) };
        if let Err(error) = &listened {
            tracing::warn!(%error, "failed to watch the clipboard");
        }
        let mut watch = Self {
            window,
            is_listening: listened.is_ok(),
            access,
            copied_text: None,
            copied_at: None,
        };
        watch.read();
        watch
    }

    /// Notes the time of the copy, and reads the clipboard again when the access allows it. Call it
    /// after each `WM_CLIPBOARDUPDATE`.
    pub(crate) fn on_update(&mut self) {
        self.copied_at = Some(Instant::now());
        self.read();
    }

    /// Reads the clipboard, or forgets its text, when `access` differs from the access before.
    pub(crate) fn use_access(&mut self, access: ClipboardAccess) {
        if access != self.access {
            self.access = access;
            self.read();
        }
    }

    pub(crate) fn copied_text(&self) -> Option<CopiedText> {
        Some(CopiedText {
            text: self.copied_text.clone()?,
            age: self.copied_at.map(|copied_at| copied_at.elapsed()),
        })
    }

    fn read(&mut self) {
        self.copied_text = match self.access {
            ClipboardAccess::Allowed => read_text(self.window),
            ClipboardAccess::Denied => None,
        };
    }
}

impl Drop for ClipboardWatch {
    fn drop(&mut self) {
        if !self.is_listening {
            return;
        }
        // SAFETY: removes the listener that `start` added for this window.
        if let Err(error) = unsafe { RemoveClipboardFormatListener(self.window) } {
            tracing::warn!(%error, "failed to stop watching the clipboard");
        }
    }
}

/// Closes the clipboard on drop.
struct OpenClipboardGuard;

impl Drop for OpenClipboardGuard {
    fn drop(&mut self) {
        // Fails only when the clipboard is not open, which needs no handling.
        // SAFETY: closes the clipboard that `read_text` opened.
        let _ = unsafe { CloseClipboard() };
    }
}

fn read_text(window: HWND) -> Option<String> {
    if is_private() {
        return None;
    }
    // SAFETY: plain call. The guard closes the clipboard.
    unsafe { OpenClipboard(Some(window)) }.ok()?;
    let _open = OpenClipboardGuard;
    // SAFETY: the clipboard is open.
    let handle = unsafe { GetClipboardData(UNICODE_TEXT_FORMAT) }.ok()?;
    let memory = HGLOBAL(handle.0);
    // SAFETY: the clipboard owns the block and keeps it while the clipboard is open.
    let start = unsafe { GlobalLock(memory) }.cast::<u16>();
    if start.is_null() {
        return None;
    }
    // SAFETY: GlobalLock returned the start of a locked block of GlobalSize bytes.
    let units = unsafe { std::slice::from_raw_parts(start, GlobalSize(memory) / size_of::<u16>()) };
    let text = String::from_utf16_lossy(until_nul(units));
    // Fails also when the block is no longer locked, which is the expected outcome.
    // SAFETY: unlocks the block that GlobalLock locked.
    let _ = unsafe { GlobalUnlock(memory) };
    Some(text)
}

fn is_private() -> bool {
    PRIVATE_FORMATS.into_iter().any(|name| {
        // SAFETY: plain calls with a static, nul-terminated name.
        unsafe { IsClipboardFormatAvailable(RegisterClipboardFormatW(name)) }.is_ok()
    })
}

/// Returns the units of `units` before its first nul.
fn until_nul(units: &[u16]) -> &[u16] {
    units.split(|&unit| unit == 0).next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_ends_at_the_first_nul() {
        let units = [0x48, 0x69, 0, 0x78];

        assert_eq!(until_nul(&units), [0x48, 0x69]);
    }

    #[test]
    fn text_without_nul_is_whole() {
        let units = [0x48, 0x69];

        assert_eq!(until_nul(&units), [0x48, 0x69]);
    }
}
