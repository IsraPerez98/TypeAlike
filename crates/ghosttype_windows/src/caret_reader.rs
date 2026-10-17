//! Reads the caret and the text around it on a thread of its own. The caret is the system caret
//! when the focused app has one, else the caret that UI Automation (UIA) reports. The text comes
//! from UIA, else from a classic edit control. UIA can wait long for a busy app, so the thread of
//! the hooks never reads it. What shows at the caret waits for the newest read of the caret. The
//! hooks use the newest read of the text while no input changed the field.

use std::iter;
use std::sync::mpsc;
use std::thread;

use ghosttype_core::app::BadgeView;
use ghosttype_core::desktop::FieldText;
use windows::core::Result;

use crate::focus::{self, Caret, WindowId};
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

/// Identifies one read of the field text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct FieldReadId(u64);

/// The answer to one read of the field text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldRead {
    pub(crate) id: FieldReadId,
    /// The foreground window when the read started.
    pub(crate) window: WindowId,
    /// The text around the caret of the focused field, or `None` when GhostType cannot read it.
    pub(crate) text: Option<FieldText>,
    /// The label of the focused field, or `None` when it has none.
    pub(crate) label: Option<String>,
}

/// A read that the shell asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Request {
    Caret(ReadId),
    FieldText(FieldReadId),
}

/// The thread that reads the caret and the field text. Drop stops the thread after its current
/// read and does not wait for it, because a read can wait long for a busy app.
#[derive(Debug)]
pub(crate) struct CaretReader {
    requests: mpsc::Sender<Request>,
}

impl CaretReader {
    /// Starts the thread, which sends the answer to each read of the caret to `carets`, and to
    /// each read of the field text to `field_texts`.
    pub(crate) fn start(carets: Mailbox<CaretRead>, field_texts: Mailbox<FieldRead>) -> Result<Self> {
        let (requests, received) = mpsc::channel();
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || answer_reads(&received, &carets, &field_texts))?;
        Ok(Self { requests })
    }

    /// Asks for the caret as it is now. The answer has the id `read`.
    pub(crate) fn read(&self, read: ReadId) {
        self.ask(Request::Caret(read));
    }

    /// Asks for the text around the caret of the focused field as it is now. The answer has the
    /// id `read`.
    pub(crate) fn read_field_text(&self, read: FieldReadId) {
        self.ask(Request::FieldText(read));
    }

    fn ask(&self, request: Request) {
        if self.requests.send(request).is_err() {
            tracing::warn!("the caret reader stopped");
        }
    }
}

/// Answers the reads until the reader drops. Of the reads of one kind that wait together, only
/// the newest gets an answer. The caret goes first, because the ghost text waits for it.
fn answer_reads(
    requests: &mpsc::Receiver<Request>,
    carets: &Mailbox<CaretRead>,
    field_texts: &Mailbox<FieldRead>,
) {
    let uia = UiaCaret::connect()
        .inspect_err(|error| tracing::warn!(%error, "UI Automation is unavailable"))
        .ok();
    while let Ok(oldest) = requests.recv() {
        let newest = NewestReads::of(iter::once(oldest).chain(requests.try_iter()));
        if let Some(id) = newest.caret {
            let caret = focus::caret().or_else(|| uia.as_ref()?.read());
            carets.send(CaretRead { id, caret });
        }
        if let Some(id) = newest.field_text {
            let window = focus::foreground_window();
            let text = uia
                .as_ref()
                .and_then(UiaCaret::field_text)
                .or_else(focus::edit_field_text);
            let label = uia.as_ref().and_then(UiaCaret::field_label);
            field_texts.send(FieldRead {
                id,
                window,
                text,
                label,
            });
        }
    }
}

/// The newest read of each kind among the reads that wait together.
#[derive(Debug, Default, PartialEq, Eq)]
struct NewestReads {
    caret: Option<ReadId>,
    field_text: Option<FieldReadId>,
}

impl NewestReads {
    /// Picks the newest read of each kind from `requests`, which come oldest first.
    fn of(requests: impl IntoIterator<Item = Request>) -> Self {
        requests
            .into_iter()
            .fold(Self::default(), |newest, request| match request {
                Request::Caret(id) => Self {
                    caret: Some(id),
                    ..newest
                },
                Request::FieldText(id) => Self {
                    field_text: Some(id),
                    ..newest
                },
            })
    }
}

/// The newest read of the field text and label, kept while no input or focus change can have
/// changed the field. The hooks use it, because they cannot wait for UIA.
#[derive(Debug, Default)]
pub(crate) struct KeptFieldText {
    newest: FieldReadId,
    kept: Option<FieldRead>,
}

impl KeptFieldText {
    /// Forgets the kept text, because an input or a focus change can change the field now. The
    /// answers to the reads asked for before are stale.
    pub(crate) fn forget(&mut self) {
        self.newest = FieldReadId(self.newest.0.wrapping_add(1));
        self.kept = None;
    }

    /// The id for a read of the field as it is now.
    pub(crate) fn newest(&self) -> FieldReadId {
        self.newest
    }

    /// Keeps `read`, unless an input or a focus change came after it was asked for.
    pub(crate) fn keep(&mut self, read: FieldRead) {
        if read.id == self.newest {
            self.kept = Some(read);
        }
    }

    /// Returns the kept read when it is the read of a field in `window`.
    pub(crate) fn read_in(&self, window: WindowId) -> Option<&FieldRead> {
        self.kept.as_ref().filter(|read| read.window == window)
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
    use std::ptr;

    use ghosttype_core::app::{Status, TrayCommand};
    use windows::Win32::Foundation::HWND;

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

    fn window(number: usize) -> WindowId {
        HWND(ptr::without_provenance_mut(number)).into()
    }

    fn field_read(id: FieldReadId, window: WindowId) -> FieldRead {
        FieldRead {
            id,
            window,
            text: Some(FieldText {
                before_caret: "Dear Ana,".to_owned(),
                after_caret: String::new(),
            }),
            label: Some("Message".to_owned()),
        }
    }

    #[test]
    fn read_after_last_input_gives_its_text() {
        let mut kept = KeptFieldText::default();
        kept.forget();
        let read = field_read(kept.newest(), window(1));

        kept.keep(read.clone());

        assert_eq!(kept.read_in(window(1)), Some(&read));
    }

    #[test]
    fn read_asked_for_before_an_input_is_dropped() {
        let mut kept = KeptFieldText::default();
        let stale = field_read(kept.newest(), window(1));
        kept.forget();

        kept.keep(stale);

        assert_eq!(kept.read_in(window(1)), None);
    }

    #[test]
    fn input_forgets_kept_text() {
        let mut kept = KeptFieldText::default();
        kept.keep(field_read(kept.newest(), window(1)));

        kept.forget();

        assert_eq!(kept.read_in(window(1)), None);
    }

    #[test]
    fn text_of_another_window_is_not_used() {
        let mut kept = KeptFieldText::default();
        kept.keep(field_read(kept.newest(), window(1)));

        let read = kept.read_in(window(2));

        assert_eq!(read, None);
    }

    #[test]
    fn newest_read_of_each_kind_is_answered() {
        let requests = [
            Request::Caret(ReadId(1)),
            Request::FieldText(FieldReadId(1)),
            Request::Caret(ReadId(2)),
        ];

        let newest = NewestReads::of(requests);

        assert_eq!(
            newest,
            NewestReads {
                caret: Some(ReadId(2)),
                field_text: Some(FieldReadId(1)),
            }
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
