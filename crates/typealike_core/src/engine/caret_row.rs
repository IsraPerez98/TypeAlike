//! The row of the caret on the screen, in a field that shows its text only on the screen and gives
//! no caret, such as the terminal of Ghostty or the window of Herdr. The mirror shows below that
//! row. The reads of the row are OCR reads of their own, whose text never goes into a prompt.

use crate::desktop::{ScreenRead, ScreenReadId, ScreenRect, ScreenText};
use crate::screen::{self, ShownTyping};

use super::Engine;
use super::session::{FieldTextAccess, Session};

/// The letters and digits that the user can type on the line of the caret after the start of a
/// row read, before its row gets another read, because the typed line can wrap onto the next row.
// ponytail: a fixed letter budget, a wrap check from the width of the row if the reads cost too much
const MAX_LETTERS_AFTER_ROW_READ: usize = 20;
/// The letters and digits typed after a row read that found no row, before the next row read. An
/// OCR often gives no line for a prompt mark with only one or two typed letters.
const LETTERS_BEFORE_ROW_RETRY: usize = 4;
/// The last letters and digits of the typed line at the start of a row read that the screen can
/// still miss, because the app writes the keys back after the read starts.
const UNECHOED_LETTERS: usize = 2;

/// The answer of the newest read of the row of the caret in a session.
pub(super) struct CaretRowRead {
    text: Option<ScreenText>,
    /// The letters and digits of the line of the caret when the read started.
    typed_letters: usize,
}

impl CaretRowRead {
    /// The row of the caret in `field` that this read shows, for the text `typed` before the caret.
    fn row(&self, typed: &str, field: Option<ScreenRect>) -> Option<ScreenRect> {
        let text = self.text.as_ref().filter(|text| Some(text.field) == field)?;
        let shown = ShownTyping::Start(self.typed_letters.saturating_sub(UNECHOED_LETTERS));
        screen::caret_row(text, typed, shown)
    }

    /// Whether the row of the caret needs another read for the text `typed` before the caret in
    /// `field`: the typed line grew too far past a row that this read found, or the user typed a
    /// few letters after a read that found no row.
    fn is_stale(&self, typed: &str, field: Option<ScreenRect>) -> bool {
        let typed_since = typed_letters(typed).abs_diff(self.typed_letters);
        match self.row(typed, field) {
            Some(_) => typed_since > MAX_LETTERS_AFTER_ROW_READ,
            None => typed_since >= LETTERS_BEFORE_ROW_RETRY,
        }
    }
}

impl Engine {
    /// The row of the caret on the screen in the field of the session, when the field gives no text
    /// but shows text that the platform cannot read: the row that the newest row read shows, else
    /// the row that the newest read of the screen shows ([`screen::caret_row`]). Only a read of
    /// `field`, the frame of the field now, counts, because the window moved since another read.
    pub(crate) fn caret_row_on_screen(&self, field: Option<ScreenRect>) -> Option<ScreenRect> {
        let session = self.session_with_unreadable_text()?;
        let typed = &session.text_before;
        let row_of_row_read = session
            .caret_row_read
            .as_ref()
            .and_then(|read| read.row(typed, field));
        row_of_row_read.or_else(|| {
            let screen = self
                .screen_reads
                .text()
                .filter(|screen| Some(screen.field) == field)?;
            screen::caret_row(screen, typed, ShownTyping::WholeLine)
        })
    }

    /// Whether the session needs a read of the row of the caret in `field`, the frame of the field
    /// now: its field gives no text but shows text that the platform cannot read, it has no row
    /// read or a stale one, and no other read of the screen runs. The platform answers only the
    /// newest of the reads that wait together, so a row read never waits beside another read.
    pub(crate) fn is_caret_row_read_due(&self, field: Option<ScreenRect>) -> bool {
        let Some(session) = self.session_with_unreadable_text() else {
            return false;
        };
        if self.screen_reads.is_reading() || self.screen_reads.is_row_reading() {
            return false;
        }
        session
            .caret_row_read
            .as_ref()
            .is_none_or(|read| read.is_stale(&session.text_before, field))
    }

    /// Starts a read of the row of the caret in `field` when [`Engine::is_caret_row_read_due`],
    /// and returns its id.
    pub(crate) fn start_caret_row_read(&mut self, field: Option<ScreenRect>) -> Option<ScreenReadId> {
        if !self.is_caret_row_read_due(field) {
            return None;
        }
        let typed = typed_letters(&self.session.as_ref()?.text_before);
        Some(self.screen_reads.start_row_read(typed))
    }

    /// Whether `read` is the read of the row of the caret that runs.
    pub(crate) fn is_caret_row_read(&self, read: ScreenReadId) -> bool {
        self.screen_reads.is_row_read(read)
    }

    /// Keeps the answer of the row read `read` for the session.
    pub(crate) fn on_caret_row_read(&mut self, read: ScreenRead) {
        let Some(typed_letters) = self.screen_reads.answer_row_read(read.id) else {
            return;
        };
        if let Some(session) = &mut self.session {
            session.caret_row_read = Some(CaretRowRead {
                text: read.text,
                typed_letters,
            });
        }
    }

    fn session_with_unreadable_text(&self) -> Option<&Session> {
        self.session
            .as_ref()
            .filter(|session| session.field.text == FieldTextAccess::Unreadable)
    }
}

/// The count of the letters and digits of the line of `typed` where the caret is.
fn typed_letters(typed: &str) -> usize {
    screen::typed_line_letters(typed).chars().count()
}
