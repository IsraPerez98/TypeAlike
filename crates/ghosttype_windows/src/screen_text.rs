//! Reads the text near the focused field on a thread of its own, with a priority below the normal
//! one, so that the model and the apps of the user get the CPU first: the text elements of the UI
//! Automation (UIA) tree of the foreground window, or the text that OCR recognizes in a capture of
//! the window (`ocr`). UIA can take long to read a large window, so the caret reader never does it.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use ghosttype_core::desktop::{ScreenRead, ScreenReadId, ScreenSource};
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
use windows::core::Result;

use crate::mailbox::Mailbox;
use crate::ocr::Ocr;
use crate::uia::UiaCaret;

const THREAD_NAME: &str = "screen text reader";
const CONNECTION_TIMEOUT: Duration = Duration::from_millis(500);
/// A read of a large browser window can take more than one second.
const TRANSACTION_TIMEOUT: Duration = Duration::from_secs(3);

/// The thread that reads the screen text. Drop stops the thread after its current read and does
/// not wait for it, because a read can wait long for a busy app.
#[derive(Debug)]
pub(crate) struct ScreenTextReader {
    requests: mpsc::Sender<ScreenRequest>,
}

/// A read that core asked for.
#[derive(Debug)]
struct ScreenRequest {
    read: ScreenReadId,
    source: ScreenSource,
    requested: Instant,
}

/// The answer to a read, and when the reader sent it.
#[derive(Debug)]
pub(crate) struct ScreenAnswer {
    pub(crate) read: ScreenRead,
    pub(crate) sent: Instant,
}

impl ScreenTextReader {
    /// Starts the thread, which sends the answer to each read to `answers`.
    pub(crate) fn start(answers: Mailbox<ScreenAnswer>) -> Result<Self> {
        let (requests, received) = mpsc::channel();
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || answer_reads(&received, &answers))?;
        Ok(Self { requests })
    }

    /// Asks for the text near the focused field from `source`, as it is now. The answer has the
    /// id `read`.
    pub(crate) fn read(&self, read: ScreenReadId, source: ScreenSource) {
        tracing::debug!(?read, ?source, "screen read requested");
        let request = ScreenRequest {
            read,
            source,
            requested: Instant::now(),
        };
        if self.requests.send(request).is_err() {
            tracing::warn!("the screen text reader stopped");
        }
    }
}

/// Answers the reads until the reader drops. Of the reads that wait together, only the newest
/// gets an answer, because core uses only the newest.
fn answer_reads(requests: &mpsc::Receiver<ScreenRequest>, answers: &Mailbox<ScreenAnswer>) {
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
    let mut ocr = Ocr::default();
    while let Ok(oldest) = requests.recv() {
        let newest = requests.try_iter().last().unwrap_or(oldest);
        let started = Instant::now();
        let text = uia.as_ref().and_then(|uia| match newest.source {
            ScreenSource::Accessibility => uia.screen_text(),
            ScreenSource::Ocr(_) => ocr.read(uia),
        });
        tracing::debug!(
            read = ?newest.read,
            source = ?newest.source,
            waited = ?started - newest.requested,
            duration = ?started.elapsed(),
            lines = text.as_ref().map(|text| text.lines.len()),
            "screen read answered"
        );
        answers.send(ScreenAnswer {
            read: ScreenRead {
                id: newest.read,
                text,
            },
            sent: Instant::now(),
        });
    }
}
