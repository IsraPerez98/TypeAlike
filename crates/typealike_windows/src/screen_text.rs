//! Reads the text near the focused field on a thread of its own, with a priority below the normal
//! one, so that the model and the apps of the user get the CPU first: the text elements of the UI
//! Automation (UIA) tree of the foreground window, or the text that OCR recognizes in a capture of
//! the window (`ocr`). UIA can take long to read a large window, so the caret reader never does it.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use typealike_core::desktop::{ScreenRead, ScreenReadId, ScreenSource, Surroundings};
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
use windows::core::Result;

use crate::mailbox::Mailbox;
use crate::ocr::Ocr;
use crate::uia::UiaConnection;

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

/// What a read of the screen does with the pixels of the focused field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldPixels {
    /// The field gives its text through UIA. The OCR leaves out its pixels, so that the typed text
    /// and the caret in the field do not change the OCR.
    LeftOut,
    /// The field shows its text only on the screen, as the window of Herdr. The OCR reads its
    /// pixels, because they hold its text. When the field gives no caret, the read also looks for
    /// the block cursor in them.
    Read,
}

impl FieldPixels {
    /// What a read does with the pixels of the focused field of `surroundings`.
    pub(crate) fn of(surroundings: &Surroundings) -> Self {
        if surroundings.shows_text_only_on_screen {
            Self::Read
        } else {
            Self::LeftOut
        }
    }
}

/// A read that core asked for.
#[derive(Debug)]
struct ScreenRequest {
    read: ScreenReadId,
    source: ScreenSource,
    field_pixels: FieldPixels,
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

    /// Asks for the text near the focused field from `source`, as it is now. The read does with
    /// the pixels of the field what `field_pixels` tells. The answer has the id `read`.
    pub(crate) fn read(&self, read: ScreenReadId, source: ScreenSource, field_pixels: FieldPixels) {
        tracing::debug!(?read, ?source, ?field_pixels, "screen read requested");
        let request = ScreenRequest {
            read,
            source,
            field_pixels,
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
    let mut connection = UiaConnection::connect(CONNECTION_TIMEOUT, TRANSACTION_TIMEOUT);
    let mut ocr = Ocr::default();
    while let Ok(oldest) = requests.recv() {
        let newest = requests.try_iter().last().unwrap_or(oldest);
        let started = Instant::now();
        let (text, cursor) = connection
            .connected()
            .map_or((None, None), |uia| match newest.source {
                ScreenSource::Accessibility => (uia.screen_text(), None),
                ScreenSource::Ocr(_) => ocr.read(uia, newest.field_pixels),
                ScreenSource::Cursor => (None, ocr.look_at_cursor(uia, newest.field_pixels)),
            });
        tracing::debug!(
            read = ?newest.read,
            source = ?newest.source,
            waited = ?started - newest.requested,
            duration = ?started.elapsed(),
            lines = text.as_ref().map(|text| text.lines.len()),
            blocks = cursor.as_ref().map(|look| look.blocks.len()),
            "screen read answered"
        );
        let read = ScreenRead {
            id: newest.read,
            text,
            cursor,
        };
        #[cfg(feature = "suggestion-log")]
        crate::suggestion_log::screen_read(
            &read,
            newest.source,
            started - newest.requested,
            started.elapsed(),
        );
        answers.send(ScreenAnswer {
            read,
            sent: Instant::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_that_shows_its_text_only_on_the_screen_gives_its_pixels_to_the_read() {
        let surroundings = Surroundings {
            shows_text_only_on_screen: true,
            has_unreadable_text: true,
            ..Surroundings::default()
        };

        let pixels = FieldPixels::of(&surroundings);

        assert_eq!(pixels, FieldPixels::Read);
    }

    #[test]
    fn other_field_leaves_its_pixels_out_of_the_read() {
        let surroundings = Surroundings {
            has_unreadable_text: true,
            ..Surroundings::default()
        };

        let pixels = FieldPixels::of(&surroundings);

        assert_eq!(pixels, FieldPixels::LeftOut);
    }
}
