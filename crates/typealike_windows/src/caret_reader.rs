//! Reads the caret and the focused field on a thread of its own. The caret is the system caret
//! when the focused app has one, else the caret of its caret object, else the caret that UI
//! Automation (UIA) reports. Each read of the caret also gives the frame of the focused field. The
//! text comes from UIA, else from a classic edit control. UIA can wait long for a busy app, so the
//! thread of the hooks never reads it. The shell keeps the caret of the newest read, and core
//! decides what shows there. The hooks use the newest read of the text while no input changed the
//! field, and treat the focused field as a password field until a read after the last move of the
//! focus tells otherwise.
//!
//! Each request of this thread to an app has a time limit, so that one app that does not answer
//! cannot stop the reads in all apps. A read of the field that fails, also on a timeout, tells
//! nothing about the field: the field keeps what the reads after the last move of the focus told.

use std::iter;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use typealike_core::controller::CaretRead;
use typealike_core::desktop::{AfterCaret, FieldText};
use windows::Win32::Foundation::RECT;
use windows::core::Result;

use crate::caret::{self, Caret};
use crate::mailbox::Mailbox;
use crate::uia::{FocusedCaret, ShownField, Uia, UiaConnection, UiaField};
use crate::window::WindowId;
use crate::{display, focus};

const THREAD_NAME: &str = "caret reader";
/// The longest wait for an app to give an element, such as the focused element.
const CONNECTION_TIMEOUT: Duration = Duration::from_millis(500);
/// The longest wait for an app to answer one request about an element, such as a property. The
/// move of the text range by thousands of characters can take seconds in a WPF text box.
const TRANSACTION_TIMEOUT: Duration = Duration::from_secs(3);

/// Identifies one read of the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ReadId(u64);

/// The answer to one read of the caret.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CaretAnswer {
    pub(crate) id: ReadId,
    /// The caret of the focused control, or `None` when the control shows no caret that TypeAlike
    /// can find.
    pub(crate) caret: Option<Caret>,
    /// The frame of the focused field in physical screen pixels, without the part outside the
    /// client area of the focused window, or `None` when TypeAlike cannot read it.
    pub(crate) field: Option<RECT>,
    /// What follows the caret at this read.
    pub(crate) after: AfterCaret,
}

/// Identifies one read of the field text. A later read has a higher id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(crate) struct FieldReadId(u64);

/// The answer to one read of the field text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldRead {
    pub(crate) id: FieldReadId,
    /// The foreground window when the read started.
    pub(crate) window: WindowId,
    /// What the read tells about the focused field.
    pub(crate) field: FieldAnswer,
}

/// What a read tells about the focused field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FieldAnswer {
    /// The field shows its text, and tells this about itself.
    Shown(ShownField),
    /// The field hides its text, as a password field does.
    Hidden,
    /// The read cannot tell whether the field hides its text, because UIA gives no focused field,
    /// for example after a request that failed or did not finish in time.
    Unknown,
}

impl FieldAnswer {
    /// The answer when TypeAlike has no connection to UIA: a shown field with `edit_text`, the
    /// text of a classic edit control, so that TypeAlike also works without UIA.
    fn without_uia(edit_text: Option<FieldText>) -> Self {
        Self::Shown(ShownField {
            text: edit_text,
            ..ShownField::default()
        })
    }

    /// The answer for `field`, what UIA tells about the focused field, or `None` when UIA gives
    /// no focused field. A shown field without text from UIA gets the text that `edit_text` reads
    /// from a classic edit control.
    fn from_uia(field: Option<UiaField>, edit_text: impl FnOnce() -> Option<FieldText>) -> Self {
        match field {
            Some(UiaField::Shown(field)) => Self::Shown(ShownField {
                text: field.text.or_else(edit_text),
                ..field
            }),
            Some(UiaField::Password) => Self::Hidden,
            None => Self::Unknown,
        }
    }

    /// The field when the answer tells that it shows its text.
    pub(crate) fn shown(&self) -> Option<&ShownField> {
        match self {
            Self::Shown(field) => Some(field),
            Self::Hidden | Self::Unknown => None,
        }
    }
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
    pub(crate) fn start(carets: Mailbox<CaretAnswer>, field_texts: Mailbox<FieldRead>) -> Result<Self> {
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
    carets: &Mailbox<CaretAnswer>,
    field_texts: &Mailbox<FieldRead>,
) {
    let mut connection = UiaConnection::connect(CONNECTION_TIMEOUT, TRANSACTION_TIMEOUT);
    while let Ok(oldest) = requests.recv() {
        let newest = NewestReads::of(iter::once(oldest).chain(requests.try_iter()));
        let uia = connection.connected();
        if let Some(id) = newest.caret {
            carets.send(read_caret(id, uia));
        }
        if let Some(id) = newest.field_text {
            let window = focus::foreground_window();
            let field = read_field(uia);
            field_texts.send(FieldRead { id, window, field });
        }
    }
}

/// Reads the caret through `uia`, else the system caret. The frame of the focused field is the part
/// of the frame that UIA gives inside the client area of the focused window, else that client area
/// ([`display::visible_field_frame`]).
fn read_caret(id: ReadId, uia: Option<&Uia>) -> CaretAnswer {
    let started = Instant::now();
    let focused = uia.map_or_else(
        || FocusedCaret {
            caret: caret::system_caret(),
            frame: None,
            after: AfterCaret::Unknown,
        },
        Uia::caret,
    );
    let client_area = focus::focused_client_area();
    let field = display::visible_field_frame(focused.frame, client_area);
    tracing::debug!(
        duration = ?started.elapsed(),
        caret = ?focused.caret.as_ref().map(Caret::screen_rect),
        uia_frame = ?focused.frame,
        ?client_area,
        ?field,
        after = ?focused.after,
        "read the caret"
    );
    CaretAnswer {
        id,
        caret: focused.caret,
        field,
        after: focused.after,
    }
}

/// Reads the focused field through `uia`, and its text from a classic edit control when UIA does
/// not give it. Without a connection to UIA, the field counts as a field that shows its text.
fn read_field(uia: Option<&Uia>) -> FieldAnswer {
    let Some(uia) = uia else {
        return FieldAnswer::without_uia(focus::edit_field_text());
    };
    let started = Instant::now();
    let answer = FieldAnswer::from_uia(uia.field(), focus::edit_field_text);
    if answer == FieldAnswer::Unknown {
        tracing::debug!(duration = ?started.elapsed(), "UI Automation gave no focused field");
    }
    answer
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

/// The newest read of the field text and labels, kept while no input or focus change can have
/// changed the field, and whether the focused field hides its text. The hooks use it, because
/// they cannot wait for UIA.
#[derive(Debug, Default)]
pub(crate) struct KeptFieldText {
    newest: FieldReadId,
    kept: Option<FieldRead>,
    /// The first read asked for after the last move of the focus.
    first_after_focus_move: FieldReadId,
    visibility: TextVisibility,
}

/// What the reads tell about the text of the focused field since the last move of the focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum TextVisibility {
    /// No read asked for after the move told yet.
    #[default]
    Unknown,
    Shown,
    Hidden,
}

impl KeptFieldText {
    /// Forgets the kept text, because an input can change the field now. The answers to the reads
    /// asked for before are stale. Whether the field hides its text stays, because an input that
    /// moves the focus, such as Tab, also comes as a move of the focus.
    pub(crate) fn forget_text(&mut self) {
        self.newest = FieldReadId(self.newest.0.wrapping_add(1));
        self.kept = None;
    }

    /// Forgets the kept text and whether the focused field hides its text, because a click or a
    /// move of the focus can put the focus in another field now.
    pub(crate) fn forget_focused_field(&mut self) {
        self.forget_text();
        self.first_after_focus_move = self.newest;
        self.visibility = TextVisibility::Unknown;
    }

    /// The id for a read of the field as it is now.
    pub(crate) fn newest(&self) -> FieldReadId {
        self.newest
    }

    /// Keeps `read`, unless an input or a focus change came after it was asked for. A read asked
    /// for after the last move of the focus tells whether the field hides its text, also when an
    /// input came after it. An unknown answer does not change it: a field stays excluded until a
    /// read tells that it shows its text, and an input cannot make a shown field a password field.
    pub(crate) fn keep(&mut self, read: FieldRead) {
        if read.id >= self.first_after_focus_move {
            self.visibility = match read.field {
                FieldAnswer::Shown(_) => TextVisibility::Shown,
                FieldAnswer::Hidden => TextVisibility::Hidden,
                FieldAnswer::Unknown => self.visibility,
            };
        }
        if read.id == self.newest {
            self.kept = Some(read);
        }
    }

    /// Whether the focused field can hide its text: a read tells that it does, or no read since
    /// the last move of the focus tells yet.
    pub(crate) fn may_hide_text(&self) -> bool {
        self.visibility != TextVisibility::Shown
    }

    /// Returns the kept read when it is the read of a field in `window`.
    pub(crate) fn read_in(&self, window: WindowId) -> Option<&FieldRead> {
        self.kept.as_ref().filter(|read| read.window == window)
    }
}

/// The newest read of the caret, and the caret of its answer, where the ghost text and the badge
/// show. The answer to an older read is stale.
#[derive(Debug, Default)]
pub(crate) struct NewestCaret {
    newest: ReadId,
    caret: Option<Caret>,
}

impl NewestCaret {
    /// Makes a new read the newest, and returns its id.
    pub(crate) fn next_read(&mut self) -> ReadId {
        self.newest = ReadId(self.newest.0.wrapping_add(1));
        self.newest
    }

    /// Keeps the caret of `answer` when it answers the newest read, and returns the look at the
    /// caret that core gets. `None` for a stale answer.
    pub(crate) fn keep(&mut self, answer: CaretAnswer) -> Option<CaretRead> {
        if answer.id != self.newest {
            return None;
        }
        let read = CaretRead {
            caret: answer.caret.as_ref().map(Caret::screen_rect),
            field: answer.field.map(display::screen_rect),
            after: answer.after,
        };
        self.caret = answer.caret;
        Some(read)
    }

    /// The caret of the newest answer, or `None` when that answer found no caret.
    pub(crate) fn caret(&self) -> Option<&Caret> {
        self.caret.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use typealike_core::desktop::{FieldLabels, ScreenRect};
    use windows::Win32::Foundation::{HWND, POINT};

    use super::*;

    fn caret_at(x: i32) -> Caret {
        Caret {
            origin: POINT { x, y: 40 },
            height: 20,
            font_face: None,
            text_color: None,
        }
    }

    fn caret_answer(id: ReadId, caret: Option<Caret>) -> CaretAnswer {
        CaretAnswer {
            id,
            caret,
            field: Some(RECT {
                left: 0,
                top: 30,
                right: 600,
                bottom: 70,
            }),
            after: AfterCaret::Unknown,
        }
    }

    #[test]
    fn answer_to_the_newest_read_gives_its_look_to_core() {
        let mut newest = NewestCaret::default();
        let read = newest.next_read();

        let look = newest.keep(caret_answer(read, Some(caret_at(102))));

        assert_eq!(
            look,
            Some(CaretRead {
                caret: Some(ScreenRect {
                    left: 102,
                    top: 40,
                    right: 102,
                    bottom: 60,
                }),
                field: Some(ScreenRect {
                    left: 0,
                    top: 30,
                    right: 600,
                    bottom: 70,
                }),
                after: AfterCaret::Unknown,
            })
        );
    }

    #[test]
    fn answer_to_the_newest_read_gives_what_follows_the_caret_to_core() {
        let mut newest = NewestCaret::default();
        let read = newest.next_read();
        let answer = CaretAnswer {
            after: AfterCaret::Selection,
            ..caret_answer(read, Some(caret_at(102)))
        };

        let look = newest.keep(answer);

        assert_eq!(look.map(|look| look.after), Some(AfterCaret::Selection));
    }

    #[test]
    fn answer_to_the_newest_read_keeps_its_caret() {
        let mut newest = NewestCaret::default();
        let read = newest.next_read();

        newest.keep(caret_answer(read, Some(caret_at(102))));

        assert_eq!(newest.caret(), Some(&caret_at(102)));
    }

    #[test]
    fn stale_answer_gives_no_look() {
        let mut newest = NewestCaret::default();
        let stale = newest.next_read();
        newest.next_read();

        let look = newest.keep(caret_answer(stale, Some(caret_at(102))));

        assert_eq!(look, None);
    }

    #[test]
    fn stale_answer_keeps_the_caret_of_the_newest_answer() {
        let mut newest = NewestCaret::default();
        let stale = newest.next_read();
        let read = newest.next_read();
        newest.keep(caret_answer(read, Some(caret_at(102))));

        newest.keep(caret_answer(stale, Some(caret_at(300))));

        assert_eq!(newest.caret(), Some(&caret_at(102)));
    }

    #[test]
    fn answer_without_a_caret_forgets_the_kept_caret() {
        let mut newest = NewestCaret::default();
        let first = newest.next_read();
        newest.keep(caret_answer(first, Some(caret_at(102))));
        let read = newest.next_read();

        newest.keep(caret_answer(read, None));

        assert_eq!(newest.caret(), None);
    }

    fn window(number: usize) -> WindowId {
        HWND(ptr::without_provenance_mut(number)).into()
    }

    fn text(before_caret: &str) -> FieldText {
        FieldText {
            before_caret: before_caret.to_owned(),
            after_caret: String::new(),
        }
    }

    fn message_field() -> ShownField {
        ShownField {
            text: Some(text("Dear Ana,")),
            labels: FieldLabels {
                name: Some("Message".to_owned()),
                ..FieldLabels::default()
            },
            domain: Some("mail.google.com".to_owned()),
            is_caret_line_hidden: false,
            is_search_field: false,
            has_unreadable_text: false,
            window_title: None,
        }
    }

    fn field_read(id: FieldReadId, window: WindowId) -> FieldRead {
        FieldRead {
            id,
            window,
            field: FieldAnswer::Shown(message_field()),
        }
    }

    fn password_read(id: FieldReadId, window: WindowId) -> FieldRead {
        FieldRead {
            id,
            window,
            field: FieldAnswer::Hidden,
        }
    }

    fn unknown_read(id: FieldReadId, window: WindowId) -> FieldRead {
        FieldRead {
            id,
            window,
            field: FieldAnswer::Unknown,
        }
    }

    #[test]
    fn field_without_uia_shows_the_text_of_its_edit_control() {
        let answer = FieldAnswer::without_uia(Some(text("Dear Ana,")));

        assert_eq!(
            answer,
            FieldAnswer::Shown(ShownField {
                text: Some(text("Dear Ana,")),
                ..ShownField::default()
            })
        );
    }

    #[test]
    fn uia_without_a_focused_field_gives_an_unknown_answer() {
        let answer = FieldAnswer::from_uia(None, || Some(text("Dear Ana,")));

        assert_eq!(answer, FieldAnswer::Unknown);
    }

    #[test]
    fn uia_password_field_gives_a_hidden_answer() {
        let answer = FieldAnswer::from_uia(Some(UiaField::Password), || Some(text("Dear Ana,")));

        assert_eq!(answer, FieldAnswer::Hidden);
    }

    #[test]
    fn shown_uia_field_without_text_takes_the_text_of_its_edit_control() {
        let field = ShownField {
            text: None,
            ..message_field()
        };

        let answer = FieldAnswer::from_uia(Some(UiaField::Shown(field)), || Some(text("Dear Ana,")));

        assert_eq!(answer, FieldAnswer::Shown(message_field()));
    }

    #[test]
    fn shown_uia_field_keeps_its_own_text() {
        let answer = FieldAnswer::from_uia(Some(UiaField::Shown(message_field())), || Some(text("Hello")));

        assert_eq!(answer, FieldAnswer::Shown(message_field()));
    }

    #[test]
    fn unknown_answer_after_a_focus_move_keeps_the_field_excluded() {
        let mut kept = KeptFieldText::default();
        kept.keep(field_read(kept.newest(), window(1)));
        kept.forget_focused_field();

        kept.keep(unknown_read(kept.newest(), window(1)));

        assert!(kept.may_hide_text());
    }

    #[test]
    fn unknown_answer_after_a_shown_answer_keeps_the_field_open() {
        let mut kept = KeptFieldText::default();
        kept.keep(field_read(kept.newest(), window(1)));
        kept.forget_text();

        kept.keep(unknown_read(kept.newest(), window(1)));

        assert!(!kept.may_hide_text());
    }

    #[test]
    fn unknown_answer_after_a_hidden_answer_keeps_the_field_excluded() {
        let mut kept = KeptFieldText::default();
        kept.keep(password_read(kept.newest(), window(1)));
        kept.forget_text();

        kept.keep(unknown_read(kept.newest(), window(1)));

        assert!(kept.may_hide_text());
    }

    #[test]
    fn unknown_answer_gives_no_shown_field() {
        let mut kept = KeptFieldText::default();

        kept.keep(unknown_read(kept.newest(), window(1)));

        let field = kept.read_in(window(1)).and_then(|read| read.field.shown());
        assert_eq!(field, None);
    }

    #[test]
    fn field_may_hide_its_text_before_the_first_read() {
        let kept = KeptFieldText::default();

        assert!(kept.may_hide_text());
    }

    #[test]
    fn read_of_a_shown_field_tells_that_it_shows_its_text() {
        let mut kept = KeptFieldText::default();

        kept.keep(field_read(FieldReadId::default(), window(1)));

        assert!(!kept.may_hide_text());
    }

    #[test]
    fn read_of_a_password_field_tells_that_it_hides_its_text() {
        let mut kept = KeptFieldText::default();

        kept.keep(password_read(FieldReadId::default(), window(1)));

        assert!(kept.may_hide_text());
    }

    #[test]
    fn input_keeps_that_the_last_read_found_a_shown_field() {
        let mut kept = KeptFieldText::default();
        kept.keep(field_read(FieldReadId::default(), window(1)));

        kept.forget_text();

        assert!(!kept.may_hide_text());
    }

    #[test]
    fn input_keeps_that_the_last_read_found_a_password_field() {
        let mut kept = KeptFieldText::default();
        kept.keep(password_read(FieldReadId::default(), window(1)));

        kept.forget_text();

        assert!(kept.may_hide_text());
    }

    #[test]
    fn focus_move_hides_the_field_until_the_next_read() {
        let mut kept = KeptFieldText::default();
        kept.keep(field_read(kept.newest(), window(1)));

        kept.forget_focused_field();

        assert!(kept.may_hide_text());
    }

    #[test]
    fn read_asked_for_before_a_focus_move_does_not_tell_about_the_field() {
        let mut kept = KeptFieldText::default();
        let stale = field_read(kept.newest(), window(1));
        kept.forget_focused_field();

        kept.keep(stale);

        assert!(kept.may_hide_text());
    }

    #[test]
    fn read_asked_for_after_a_focus_move_tells_about_the_field_after_an_input() {
        let mut kept = KeptFieldText::default();
        kept.forget_focused_field();
        let read = field_read(kept.newest(), window(1));
        kept.forget_text();

        kept.keep(read);

        assert!(!kept.may_hide_text());
    }

    #[test]
    fn read_after_last_input_gives_its_text() {
        let mut kept = KeptFieldText::default();
        kept.forget_text();
        let read = field_read(kept.newest(), window(1));

        kept.keep(read.clone());

        assert_eq!(kept.read_in(window(1)), Some(&read));
    }

    #[test]
    fn read_asked_for_before_an_input_is_dropped() {
        let mut kept = KeptFieldText::default();
        let stale = field_read(kept.newest(), window(1));
        kept.forget_text();

        kept.keep(stale);

        assert_eq!(kept.read_in(window(1)), None);
    }

    #[test]
    fn input_forgets_kept_text() {
        let mut kept = KeptFieldText::default();
        kept.keep(field_read(kept.newest(), window(1)));

        kept.forget_text();

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
}
