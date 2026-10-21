//! Builds what the model reads before the text, with the screen text and the recent copy.

use std::time::Duration;

use crate::desktop::{Desktop, ScreenRead, ScreenReadId, ScreenSource, ScreenText, Surroundings};
use crate::history;
use crate::prompt::{self, Writer};
use crate::settings::{ClipboardAccess, OcrLevel, ScreenTextSettings};

use super::Engine;

/// The age after which a copy has no relation to the text that the user types.
pub(super) const MAX_CLIPBOARD_AGE: Duration = Duration::from_mins(5);

/// What the engine knows of the text near the field on the screen.
#[derive(Default)]
pub(super) enum Screen {
    #[default]
    Unknown,
    /// The platform reads the screen from `source`. A session that starts now gets the text when
    /// the answer comes.
    Reading { id: ScreenReadId, source: ScreenSource },
    /// The newest read, for the next session.
    Read(ScreenText),
}

impl Engine {
    /// Starts a read of the accessibility tree of the screen, and returns its id. A session that
    /// has its final screen text or waits for a read starts none.
    pub(crate) fn start_screen_read(&mut self) -> Option<ScreenReadId> {
        if let Some(session) = &self.session {
            let is_reading = matches!(self.screen, Screen::Reading { .. });
            if session.has_screen_read || is_reading {
                return None;
            }
        }
        Some(self.start_read(ScreenSource::Accessibility))
    }

    /// Starts a read of the accessibility tree for the session, when the session is in a text
    /// field of an app that `screen_text` allows, and returns its id. A session that has its
    /// final screen text or waits for a read starts none.
    pub(crate) fn start_session_screen_read(
        &mut self,
        screen_text: &ScreenTextSettings,
    ) -> Option<ScreenReadId> {
        let session = self.session.as_ref()?;
        let is_allowed = session
            .app
            .as_ref()
            .is_some_and(|app| screen_text.allows(&app.app_id));
        if !session.is_text_field || !is_allowed {
            return None;
        }
        self.start_screen_read()
    }

    /// Starts an OCR read at `level` in place of `read`, when `read` is the newest read and reads
    /// the accessibility tree, and returns its id.
    pub(crate) fn start_ocr_after(&mut self, read: ScreenReadId, level: OcrLevel) -> Option<ScreenReadId> {
        let is_newest_tree_read = matches!(
            self.screen,
            Screen::Reading { id, source: ScreenSource::Accessibility } if id == read
        );
        is_newest_tree_read.then(|| self.start_read(ScreenSource::Ocr(level)))
    }

    fn start_read(&mut self, source: ScreenSource) -> ScreenReadId {
        self.newest_screen_read = self.newest_screen_read.next();
        self.screen = Screen::Reading {
            id: self.newest_screen_read,
            source,
        };
        self.newest_screen_read
    }

    /// Keeps the text of `read` when it answers the newest read, and returns whether it does. A
    /// session without its final screen text gets the new text, with what else surrounds the caret
    /// in `desktop` now, when its preface changes. The session then has its final screen text. The
    /// next session also gets the new text.
    pub(crate) fn on_screen_read(&mut self, read: ScreenRead, desktop: &dyn Desktop) -> bool {
        if !matches!(self.screen, Screen::Reading { id, .. } if id == read.id) {
            return false;
        }
        self.screen = read.text.map_or(Screen::Unknown, Screen::Read);
        self.add_screen_to_session(desktop);
        true
    }

    fn add_screen_to_session(&mut self, desktop: &dyn Desktop) {
        let is_waiting = self
            .session
            .as_ref()
            .is_some_and(|session| !session.has_screen_read);
        let has_text = matches!(self.screen, Screen::Read(_));
        let preface = (is_waiting && has_text).then(|| self.preface(&desktop.surroundings(), desktop));
        let Some(session) = &mut self.session else {
            return;
        };
        session.has_screen_read = true;
        if let Some(preface) = preface.filter(|preface| *preface != session.preface) {
            session.use_preface(preface);
        }
    }

    /// Forgets the screen text and the read that runs, for when another field can have the focus.
    pub(crate) fn forget_screen(&mut self) {
        self.screen = Screen::Unknown;
    }

    /// Returns what the model reads before the text of the field in `surroundings`.
    pub(super) fn preface(&self, surroundings: &Surroundings, desktop: &dyn Desktop) -> String {
        let recent_copy = match self.clipboard_access {
            ClipboardAccess::Allowed => desktop
                .copied_text()
                .filter(|copied| copied.age.is_some_and(|age| age < MAX_CLIPBOARD_AGE)),
            ClipboardAccess::Denied => None,
        };
        let screen = match &self.screen {
            Screen::Read(text) => Some(text),
            Screen::Unknown | Screen::Reading { .. } => None,
        };
        let texts = history::session_texts(&self.history, surroundings, &self.personalization);
        let examples = history::examples(&texts, surroundings, screen, self.matching_examples);
        let writer = Writer {
            notes: &self.writer_notes.0,
            name: &self.writer_name.0,
            examples,
        };
        prompt::preface(
            &writer,
            surroundings,
            recent_copy.map(|copied| copied.text).as_deref(),
            screen,
        )
    }
}
