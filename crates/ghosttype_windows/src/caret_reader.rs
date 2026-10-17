//! Reads the caret on a thread of its own: the system caret when the focused app has one, else the
//! caret that UI Automation (UIA) reports. UIA can wait long for a busy app, so the thread of the
//! hooks never reads it. What shows at the caret waits for the newest read.

use std::sync::mpsc;
use std::thread;

use ghosttype_core::app::BadgeView;
use windows::core::Result;

use crate::focus::{self, Caret};
use crate::mailbox::Mailbox;
use crate::uia::UiaCaret;

const THREAD_NAME: &str = "caret reader";

/// Identifies one read of the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ReadId(u64);

/// The answer to one read of the caret.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CaretRead {
    pub(crate) id: ReadId,
    /// The caret of the focused control, or `None` when the control shows no caret that GhostType
    /// can find.
    pub(crate) caret: Option<Caret>,
}

/// The thread that reads the caret. Drop stops the thread after its current read and does not
/// wait for it, because a read can wait long for a busy app.
#[derive(Debug)]
pub(crate) struct CaretReader {
    reads: mpsc::Sender<ReadId>,
}

impl CaretReader {
    /// Starts the thread, which sends the answer to each read to `answers`.
    pub(crate) fn start(answers: Mailbox<CaretRead>) -> Result<Self> {
        let (reads, requests) = mpsc::channel();
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || answer_reads(&requests, &answers))?;
        Ok(Self { reads })
    }

    /// Asks for the caret as it is now. The answer has the id `read`.
    pub(crate) fn read(&self, read: ReadId) {
        if self.reads.send(read).is_err() {
            tracing::warn!("the caret reader stopped");
        }
    }
}

/// Answers the reads until the reader drops. Of the reads that wait together, only the newest
/// gets an answer.
fn answer_reads(reads: &mpsc::Receiver<ReadId>, answers: &Mailbox<CaretRead>) {
    let uia = UiaCaret::connect()
        .inspect_err(|error| tracing::warn!(%error, "UI Automation is unavailable"))
        .ok();
    while let Ok(oldest) = reads.recv() {
        let id = reads.try_iter().last().unwrap_or(oldest);
        let caret = focus::caret().or_else(|| uia.as_ref()?.read());
        answers.send(CaretRead { id, caret });
    }
}

/// What shows at the caret after a read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CaretView {
    pub(crate) ghost: Option<String>,
    pub(crate) badge: Option<BadgeView>,
}

/// The view that waits for the newest read of the caret. The answer to an older read is stale.
#[derive(Debug, Default)]
pub(crate) struct PendingView {
    newest: ReadId,
    view: Option<CaretView>,
}

impl PendingView {
    /// Makes `view` wait for a new read, and returns the id of that read.
    pub(crate) fn wait(&mut self, view: CaretView) -> ReadId {
        self.newest = ReadId(self.newest.0.wrapping_add(1));
        self.view = Some(view);
        self.newest
    }

    /// Removes the ghost text from the waiting view.
    pub(crate) fn forget_ghost(&mut self) {
        if let Some(view) = &mut self.view {
            view.ghost = None;
        }
    }

    /// Removes the badge from the waiting view.
    pub(crate) fn forget_badge(&mut self) {
        if let Some(view) = &mut self.view {
            view.badge = None;
        }
    }

    /// Returns the waiting view when `read` is the newest read, one time only.
    pub(crate) fn take(&mut self, read: ReadId) -> Option<CaretView> {
        if read == self.newest {
            self.view.take()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use ghosttype_core::app::{Status, TrayCommand};

    use super::*;

    const BADGE: BadgeView = BadgeView {
        status: Status::Active,
        label: "Pause suggestions",
        command: TrayCommand::TogglePause,
    };

    fn ghost_and_badge() -> CaretView {
        CaretView {
            ghost: Some("world".to_owned()),
            badge: Some(BADGE),
        }
    }

    #[test]
    fn newest_read_gives_waiting_view() {
        let mut pending = PendingView::default();
        let read = pending.wait(ghost_and_badge());

        let view = pending.take(read);

        assert_eq!(view, Some(ghost_and_badge()));
    }

    #[test]
    fn stale_read_gives_no_view() {
        let mut pending = PendingView::default();
        let stale = pending.wait(ghost_and_badge());
        pending.wait(ghost_and_badge());

        let view = pending.take(stale);

        assert_eq!(view, None);
    }

    #[test]
    fn view_is_given_one_time() {
        let mut pending = PendingView::default();
        let read = pending.wait(ghost_and_badge());
        pending.take(read);

        let view = pending.take(read);

        assert_eq!(view, None);
    }

    #[test]
    fn forgotten_ghost_does_not_show_after_read() {
        let mut pending = PendingView::default();
        let read = pending.wait(ghost_and_badge());
        pending.forget_ghost();

        let view = pending.take(read);

        assert_eq!(
            view,
            Some(CaretView {
                ghost: None,
                badge: Some(BADGE),
            })
        );
    }

    #[test]
    fn forgotten_badge_does_not_show_after_read() {
        let mut pending = PendingView::default();
        let read = pending.wait(ghost_and_badge());
        pending.forget_badge();

        let view = pending.take(read);

        assert_eq!(
            view,
            Some(CaretView {
                ghost: Some("world".to_owned()),
                badge: None,
            })
        );
    }
}
