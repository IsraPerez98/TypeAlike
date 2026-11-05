//! Builds what the model reads before the text, with the screen text and the recent copy.

use std::time::Duration;

use crate::desktop::{Desktop, ScreenRead, ScreenReadId, ScreenSource, ScreenText, Surroundings};
use crate::history;
use crate::prompt::{self, Example, ScreenPart, Writer};
use crate::settings::{ClipboardAccess, OcrLevel, ReplyWords, ScreenTextSettings};

use super::{Engine, read_surroundings};

/// The age after which a copy has no relation to the text that the user types.
pub(super) const MAX_CLIPBOARD_AGE: Duration = Duration::from_mins(5);

/// What the engine knows of the text near the field on the screen.
#[derive(Default)]
enum Screen {
    #[default]
    Unknown,
    /// The platform reads the screen from `source`. A session that starts now gets the text when
    /// the answer comes. An OCR read keeps the text of the read of the tree that it replaces, for
    /// an answer without lines.
    Reading {
        id: ScreenReadId,
        source: ScreenSource,
        tree_text: Option<ScreenText>,
    },
    /// The newest read, for the next session.
    Read(ScreenText),
}

/// The reads of the screen that the engine starts, and the text of the newest answer.
#[derive(Default)]
pub(super) struct ScreenReads {
    screen: Screen,
    /// The id of the newest read that the engine started.
    newest: ScreenReadId,
}

impl ScreenReads {
    fn is_reading(&self) -> bool {
        matches!(self.screen, Screen::Reading { .. })
    }

    fn start(&mut self, source: ScreenSource) -> ScreenReadId {
        self.newest = self.newest.next();
        self.screen = Screen::Reading {
            id: self.newest,
            source,
            tree_text: None,
        };
        self.newest
    }

    /// Starts an OCR read at `level` in place of `read`, when `read` is the newest read and reads
    /// the accessibility tree, and returns its id. `tree_text` is the answer of `read`.
    fn start_ocr_after(
        &mut self,
        read: ScreenReadId,
        tree_text: Option<ScreenText>,
        level: OcrLevel,
    ) -> Option<ScreenReadId> {
        let is_newest_tree_read = matches!(
            self.screen,
            Screen::Reading { id, source: ScreenSource::Accessibility, .. } if id == read
        );
        is_newest_tree_read.then(|| {
            self.newest = self.newest.next();
            self.screen = Screen::Reading {
                id: self.newest,
                source: ScreenSource::Ocr(level),
                tree_text,
            };
            self.newest
        })
    }

    /// Keeps the text of `read` when it answers the newest read, and returns whether it does. An
    /// OCR answer without lines keeps the text of the read of the tree before it.
    fn answer(&mut self, read: ScreenRead) -> bool {
        let Screen::Reading { id, tree_text, .. } = &mut self.screen else {
            return false;
        };
        if *id != read.id {
            return false;
        }
        let has_lines = read.text.as_ref().is_some_and(|text| !text.lines.is_empty());
        let text = if has_lines {
            read.text
        } else {
            tree_text.take().or(read.text)
        };
        self.screen = text.map_or(Screen::Unknown, Screen::Read);
        true
    }

    /// The text of the newest answer, if it has text.
    fn text(&self) -> Option<&ScreenText> {
        match &self.screen {
            Screen::Read(text) => Some(text),
            Screen::Unknown | Screen::Reading { .. } => None,
        }
    }

    fn forget(&mut self) {
        self.screen = Screen::Unknown;
    }
}

impl Engine {
    /// Starts a read of the accessibility tree of the screen, and returns its id. A session that
    /// has its final screen text or waits for a read starts none.
    pub(crate) fn start_screen_read(&mut self) -> Option<ScreenReadId> {
        if let Some(session) = &self.session {
            let is_reading = self.screen_reads.is_reading();
            if session.has_screen_read || is_reading {
                return None;
            }
        }
        Some(self.screen_reads.start(ScreenSource::Accessibility))
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
            .field
            .app
            .as_ref()
            .is_some_and(|app| screen_text.allows(&app.app_id));
        if !session.field.is_text_field || !is_allowed {
            return None;
        }
        self.start_screen_read()
    }

    /// Starts an OCR read at `level` in place of `read`, when `read` is the newest read and reads
    /// the accessibility tree, and returns its id. When the OCR gives no lines, the screen text is
    /// `tree_text`, the answer of `read`.
    pub(crate) fn start_ocr_after(
        &mut self,
        read: ScreenReadId,
        tree_text: Option<ScreenText>,
        level: OcrLevel,
    ) -> Option<ScreenReadId> {
        self.screen_reads.start_ocr_after(read, tree_text, level)
    }

    /// Keeps the text of `read` when it answers the newest read, and returns whether it does. A
    /// session without its final screen text gets the new text, with what else surrounds the caret
    /// in `desktop` now, when its preface changes. The session then has its final screen text. The
    /// next session also gets the new text.
    pub(crate) fn on_screen_read(&mut self, read: ScreenRead, desktop: &dyn Desktop) -> bool {
        if !self.screen_reads.answer(read) {
            return false;
        }
        self.add_screen_to_session(desktop);
        true
    }

    fn add_screen_to_session(&mut self, desktop: &dyn Desktop) {
        let is_waiting = self
            .session
            .as_ref()
            .is_some_and(|session| !session.has_screen_read);
        let has_text = self.screen_reads.text().is_some();
        let update = (is_waiting && has_text).then(|| {
            let surroundings = read_surroundings(desktop);
            (
                self.build_preface(&surroundings, desktop),
                self.reply_words(&surroundings),
            )
        });
        let Some(session) = &mut self.session else {
            return;
        };
        session.has_screen_read = true;
        let Some((preface, reply_to)) = update else {
            return;
        };
        session.reply_to = reply_to;
        if preface != session.preface {
            session.use_preface(preface);
        }
    }

    /// Returns the words of the message that the user answers in the field of `surroundings`, from
    /// the screen text of the newest read. It has none while the reply words are off.
    pub(super) fn reply_words(&self, surroundings: &Surroundings) -> Vec<String> {
        let screen = self
            .screen_reads
            .text()
            .filter(|_| self.personalization.reply_words == ReplyWords::On);
        screen
            .map(|screen| history::reply_words(screen, surroundings, &self.history))
            .unwrap_or_default()
    }

    /// Lets the session read the screen once more, for when a scroll moved the text on the screen.
    pub(crate) fn read_screen_again(&mut self) {
        if let Some(session) = &mut self.session {
            session.has_screen_read = false;
        }
    }

    /// Forgets the screen text and the read that runs, for when another field can have the focus.
    pub(crate) fn forget_screen(&mut self) {
        self.screen_reads.forget();
    }

    /// Returns what the model reads before the text of the field in `surroundings`.
    pub(super) fn build_preface(&self, surroundings: &Surroundings, desktop: &dyn Desktop) -> String {
        let recent_copy = match self.clipboard_access {
            ClipboardAccess::Allowed => desktop
                .copied_text()
                .filter(|copied| copied.age.is_some_and(|age| age < MAX_CLIPBOARD_AGE)),
            ClipboardAccess::Denied => None,
        };
        let screen = self.screen_reads.text();
        let texts = history::session_texts(&self.history, surroundings, &self.personalization);
        let now = desktop.now();
        let examples = history::examples(&texts, surroundings, screen, self.matching_examples)
            .into_iter()
            .map(|(text, written_at)| Example {
                text,
                days_ago: now
                    .zip(written_at)
                    .map(|(now, written_at)| now.day().saturating_sub(written_at.day())),
            })
            .collect();
        let writer = Writer {
            notes: &self.writer_notes.0,
            name: &self.writer_name.0,
            examples,
        };
        prompt::preface(
            &writer,
            surroundings,
            recent_copy.map(|copied| copied.text).as_deref(),
            screen.map(|text| ScreenPart {
                text,
                max_tokens: self.screen_tokens,
            }),
        )
    }
}
