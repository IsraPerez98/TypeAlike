//! The text that the user copied. Reading the clipboard can wait on the app that copied the text,
//! so a thread of its own reads it after each change, and the thread of the hooks never reads it.

use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use typealike_core::desktop::CopiedText;
use typealike_core::settings::ClipboardAccess;
use windows::Win32::Foundation::{HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, RemoveClipboardFormatListener,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::core::{PCWSTR, Result, w};

use crate::mailbox::Mailbox;
use crate::utf16::until_nul;

const THREAD_NAME: &str = "clipboard reader";
/// `CF_UNICODETEXT`: text in UTF-16.
const UNICODE_TEXT_FORMAT: u32 = 13;
/// The formats that password managers and other apps add to text that no other app may read or
/// keep.
const PRIVATE_FORMATS: [PCWSTR; 2] = [
    w!("ExcludeClipboardContentFromMonitorProcessing"),
    w!("Clipboard Viewer Ignore"),
];

/// Identifies one read of the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ClipboardReadId(u64);

/// The answer to one read of the clipboard.
#[derive(Debug)]
pub(crate) struct ClipboardRead {
    id: ClipboardReadId,
    /// The text on the clipboard, or `None` when it holds no text that TypeAlike may read.
    text: Option<String>,
}

/// Keeps the copied text while the user allows it. Its window gets `WM_CLIPBOARDUPDATE` after each
/// copy, and the reader thread reads the clipboard after it.
#[derive(Debug)]
pub(crate) struct ClipboardWatch {
    window: HWND,
    is_listening: bool,
    access: ClipboardAccess,
    reads: mpsc::Sender<ClipboardReadId>,
    newest: ClipboardReadId,
    /// The text of the answer to the newest read.
    copied_text: Option<String>,
    /// When the last copy came, or `None` while the clipboard holds a copy from before the start.
    copied_at: Option<Instant>,
}

impl ClipboardWatch {
    /// Starts the reader thread, which sends the answer to each read to `answers`. Then starts to
    /// send `WM_CLIPBOARDUPDATE` to `window`, a window of this thread, and reads the clipboard when
    /// `access` allows it.
    pub(crate) fn start(
        window: HWND,
        access: ClipboardAccess,
        answers: Mailbox<ClipboardRead>,
    ) -> Result<Self> {
        let (reads, requests) = mpsc::channel();
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || answer_reads(&requests, &answers))?;
        // SAFETY: the window is a live window of this thread, and drop removes the listener.
        let listened = unsafe { AddClipboardFormatListener(window) };
        if let Err(error) = &listened {
            tracing::warn!(%error, "failed to watch the clipboard");
        }
        let mut watch = Self {
            window,
            is_listening: listened.is_ok(),
            access,
            reads,
            newest: ClipboardReadId::default(),
            copied_text: None,
            copied_at: None,
        };
        watch.read();
        Ok(watch)
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

    /// Keeps the text of `read` when it answers the newest read. A copy or a change of the access
    /// after the read makes it stale.
    pub(crate) fn keep(&mut self, read: ClipboardRead) {
        if read.id == self.newest {
            self.copied_text = read.text;
        }
    }

    pub(crate) fn copied_text(&self) -> Option<CopiedText> {
        Some(CopiedText {
            text: self.copied_text.clone()?,
            age: self.copied_at.map(|copied_at| copied_at.elapsed()),
        })
    }

    /// Forgets the copied text until the answer to a new read, and asks the reader thread for that
    /// read when the access allows it.
    fn read(&mut self) {
        self.newest = ClipboardReadId(self.newest.0.wrapping_add(1));
        self.copied_text = None;
        if self.access == ClipboardAccess::Allowed && self.reads.send(self.newest).is_err() {
            tracing::warn!("the clipboard reader stopped");
        }
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

/// Answers the reads until the watch drops. Of the reads that wait together, only the newest gets
/// an answer, because the watch keeps only the newest.
fn answer_reads(requests: &mpsc::Receiver<ClipboardReadId>, answers: &Mailbox<ClipboardRead>) {
    while let Ok(oldest) = requests.recv() {
        let id = requests.try_iter().last().unwrap_or(oldest);
        answers.send(ClipboardRead {
            id,
            text: read_text(),
        });
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

fn read_text() -> Option<String> {
    if is_private() {
        return None;
    }
    // SAFETY: plain call. A read needs no window, and the guard closes the clipboard on this thread.
    unsafe { OpenClipboard(None) }.ok()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn watch_with(access: ClipboardAccess) -> (ClipboardWatch, mpsc::Receiver<ClipboardReadId>) {
        let (reads, requests) = mpsc::channel();
        let watch = ClipboardWatch {
            window: HWND::default(),
            is_listening: false,
            access,
            reads,
            newest: ClipboardReadId::default(),
            copied_text: None,
            copied_at: None,
        };
        (watch, requests)
    }

    fn answer(id: ClipboardReadId) -> ClipboardRead {
        ClipboardRead {
            id,
            text: Some("Dear Ana,".to_owned()),
        }
    }

    #[test]
    fn answer_to_the_read_after_a_copy_gives_the_copied_text() {
        let (mut watch, requests) = watch_with(ClipboardAccess::Allowed);
        watch.on_update();
        let read = requests.try_recv().expect("the copy asks for a read");

        watch.keep(answer(read));

        let text = watch.copied_text().map(|copied| copied.text);
        assert_eq!(text.as_deref(), Some("Dear Ana,"));
    }

    #[test]
    fn answer_to_a_read_before_a_newer_copy_is_dropped() {
        let (mut watch, requests) = watch_with(ClipboardAccess::Allowed);
        watch.on_update();
        let stale = requests.try_recv().expect("the copy asks for a read");
        watch.on_update();

        watch.keep(answer(stale));

        assert_eq!(watch.copied_text(), None);
    }

    #[test]
    fn answer_to_a_read_before_the_access_is_denied_is_dropped() {
        let (mut watch, requests) = watch_with(ClipboardAccess::Allowed);
        watch.on_update();
        let stale = requests.try_recv().expect("the copy asks for a read");
        watch.use_access(ClipboardAccess::Denied);

        watch.keep(answer(stale));

        assert_eq!(watch.copied_text(), None);
    }

    #[test]
    fn copy_while_the_access_is_denied_asks_for_no_read() {
        let (mut watch, requests) = watch_with(ClipboardAccess::Denied);

        watch.on_update();

        assert!(requests.try_recv().is_err());
    }
}
