//! Reads the text near the focused field on a thread of its own, with a priority below the normal
//! one, so that the model and the apps of the user get the CPU first. UI Automation (UIA) can take
//! long to read a large window, so the caret reader never does it.

use std::iter;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ghosttype_core::desktop::{ScreenRead, ScreenReadId};
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
use windows::core::Result;

use crate::mailbox::Mailbox;
use crate::uia::UiaCaret;

const THREAD_NAME: &str = "screen text reader";
const CONNECTION_TIMEOUT: Duration = Duration::from_millis(500);
/// A read of a large browser window can take more than one second.
const TRANSACTION_TIMEOUT: Duration = Duration::from_secs(3);

/// The thread that reads the screen text. Drop stops the thread after its current read and does
/// not wait for it, because a read can wait long for a busy app.
#[derive(Debug)]
pub(crate) struct ScreenTextReader {
    requests: mpsc::Sender<ScreenReadId>,
}

impl ScreenTextReader {
    /// Starts the thread, which sends the answer to each read to `answers`.
    pub(crate) fn start(answers: Mailbox<ScreenRead>) -> Result<Self> {
        let (requests, received) = mpsc::channel();
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || answer_reads(&received, &answers))?;
        Ok(Self { requests })
    }

    /// Asks for the text near the focused field as it is now. The answer has the id `read`.
    pub(crate) fn read(&self, read: ScreenReadId) {
        if self.requests.send(read).is_err() {
            tracing::warn!("the screen text reader stopped");
        }
    }
}

/// Answers the reads until the reader drops. Of the reads that wait together, only the newest
/// gets an answer, because core uses only the newest.
fn answer_reads(requests: &mpsc::Receiver<ScreenReadId>, answers: &Mailbox<ScreenRead>) {
    // SAFETY: plain calls on the pseudo handle of this thread.
    if let Err(error) = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) } {
        tracing::warn!(%error, "failed to lower the priority of the screen text reader");
    }
    let uia = UiaCaret::connect()
        .inspect_err(|error| tracing::warn!(%error, "UI Automation is unavailable"))
        .ok();
    if let Some(uia) = &uia {
        uia.limit_waits(CONNECTION_TIMEOUT, TRANSACTION_TIMEOUT);
    }
    while let Ok(oldest) = requests.recv() {
        let newest = iter::once(oldest)
            .chain(requests.try_iter())
            .last()
            .unwrap_or(oldest);
        let text = uia.as_ref().and_then(UiaCaret::screen_text);
        answers.send(ScreenRead { id: newest, text });
    }
}
