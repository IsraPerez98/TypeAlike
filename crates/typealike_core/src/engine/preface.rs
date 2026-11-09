//! Builds what the model reads before the text, with the screen text and the recent copy.

use std::borrow::Cow;
use std::time::Duration;

use crate::desktop::{
    Desktop, OcrRead, ScreenRead, ScreenReadId, ScreenSource, ScreenText, Surroundings, Timestamp,
};
use crate::history;
use crate::prompt::{self, Example, Preface, ScreenPart, Writer};
use crate::screen::{ConversationLines, LineScope};
use crate::settings::{ClipboardAccess, ReplyWords, ScreenTextSettings};

use super::{Engine, read_surroundings};

/// The age after which a copy has no relation to the text that the user types.
pub(super) const MAX_CLIPBOARD_AGE: Duration = Duration::from_mins(5);
/// The estimated tokens of the text before the caret, about 8 words, after which a settle of the
/// input picks the examples of the session again, and again each time the text doubles. A new
/// pick decodes the prompt again from the examples on, so the picks get rarer as the text grows.
pub(super) const EXAMPLE_STEP_TOKENS: u32 = 16;

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

/// What the engine did with the answer to a read of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenAnswer {
    /// The answer is not for the newest read, so the engine dropped it.
    Dropped,
    /// The engine keeps the text of the answer.
    Kept,
    /// The engine keeps the text, which changed the preface of the session after the request of
    /// the suggestion that runs or shows. The user pressed no key after that request.
    ChangedSuggestionPrompt,
}

/// The reads of the screen that the engine starts, and the text of the newest answer.
#[derive(Default)]
pub(super) struct ScreenReads {
    screen: Screen,
    /// The id of the newest read that the engine started.
    newest: ScreenReadId,
    /// The lines above the field that the newest preface with screen text showed. The next preface
    /// in the field keeps them first while they are on the screen.
    shown_above: Vec<String>,
    /// The lines of the reads of the conversation of the newest answer.
    conversation: ConversationLines,
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

    /// Starts the OCR read `ocr` in place of `read`, when `read` is the newest read and reads the
    /// accessibility tree, and returns its id. `tree_text` is the answer of `read`.
    fn start_ocr_after(
        &mut self,
        read: ScreenReadId,
        tree_text: Option<ScreenText>,
        ocr: OcrRead,
    ) -> Option<ScreenReadId> {
        let is_newest_tree_read = matches!(
            self.screen,
            Screen::Reading { id, source: ScreenSource::Accessibility, .. } if id == read
        );
        is_newest_tree_read.then(|| {
            self.newest = self.newest.next();
            self.screen = Screen::Reading {
                id: self.newest,
                source: ScreenSource::Ocr(ocr),
                tree_text,
            };
            self.newest
        })
    }

    /// Keeps the text of `read` when it answers the newest read at `now`, and returns whether it
    /// does. An OCR answer without lines keeps the text of the read of the tree before it.
    fn answer(&mut self, read: ScreenRead, now: Option<Timestamp>) -> bool {
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
        if let Some(text) = &text {
            self.conversation.add_read(text, now);
        }
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

    /// The text of the newest answer for a prompt in `scope` at `now`. In the column of a chat or
    /// an email app, the lines that earlier reads of the conversation showed and the newest read
    /// does not show are above its lines.
    fn prompt_text(&self, scope: LineScope, now: Option<Timestamp>) -> Option<Cow<'_, ScreenText>> {
        let text = self.text()?;
        let taller = match scope {
            LineScope::Column => self.conversation.taller_screen(text, now),
            LineScope::Window => None,
        };
        Some(taller.map_or(Cow::Borrowed(text), Cow::Owned))
    }

    fn forget(&mut self) {
        self.screen = Screen::Unknown;
        self.shown_above.clear();
        self.conversation = ConversationLines::default();
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
    /// field that gets suggestions, of an app that `screen_text` allows, and returns its id. A
    /// session that has its final screen text or waits for a read starts none.
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
        if !session.field.holds_text() || session.field.is_excluded || !is_allowed {
            return None;
        }
        self.start_screen_read()
    }

    /// Starts the OCR read `ocr` in place of `read`, when `read` is the newest read and reads the
    /// accessibility tree, and returns its id. When the OCR gives no lines, the screen text is
    /// `tree_text`, the answer of `read`.
    pub(crate) fn start_ocr_after(
        &mut self,
        read: ScreenReadId,
        tree_text: Option<ScreenText>,
        ocr: OcrRead,
    ) -> Option<ScreenReadId> {
        self.screen_reads.start_ocr_after(read, tree_text, ocr)
    }

    /// Keeps the text of `read` when it answers the newest read. A session without its final
    /// screen text gets the new text, with what else surrounds the caret in `desktop` now, when its
    /// preface changes. The session then has its final screen text. The next session also gets
    /// the new text.
    pub(crate) fn on_screen_read(&mut self, read: ScreenRead, desktop: &dyn Desktop) -> ScreenAnswer {
        if !self.screen_reads.answer(read, desktop.now()) {
            return ScreenAnswer::Dropped;
        }
        let is_preface_changed = self.add_screen_to_session(desktop);
        if is_preface_changed && self.has_untouched_offer() {
            ScreenAnswer::ChangedSuggestionPrompt
        } else {
            ScreenAnswer::Kept
        }
    }

    /// Gives the screen text to a session without its final screen text, and returns whether the
    /// preface of the session changed.
    fn add_screen_to_session(&mut self, desktop: &dyn Desktop) -> bool {
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
            return false;
        };
        session.has_screen_read = true;
        let Some((preface, reply_to)) = update else {
            return false;
        };
        session.reply_to = reply_to;
        let is_changed = preface != session.preface;
        if is_changed {
            session.use_preface(preface);
        }
        is_changed
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

    /// Picks the examples of the session again with what surrounds the caret in `desktop`, when its
    /// text before the caret reached another step since the last pick (`Session::example_step_now`).
    /// Returns whether the preface of the session changed.
    pub(crate) fn pick_examples_again(&mut self, desktop: &dyn Desktop) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        let step = session.example_step_now();
        if step == session.example_step || session.field.is_excluded {
            return false;
        }
        let preface = self.build_preface(&read_surroundings(desktop), desktop);
        let Some(session) = &mut self.session else {
            return false;
        };
        session.example_step = step;
        let is_changed = preface != session.preface;
        if is_changed {
            session.use_preface(preface);
        }
        is_changed
    }

    /// Lets the session read the screen once more, for when a scroll moved the text on the screen.
    pub(crate) fn read_screen_again(&mut self) {
        if let Some(session) = &mut self.session {
            session.has_screen_read = false;
        }
    }

    /// Forgets the screen text, the read that runs and the screen lines of the last preface, for
    /// when another field can have the focus.
    pub(crate) fn forget_screen(&mut self) {
        self.screen_reads.forget();
    }

    /// Returns what the model reads before the text of the field in `surroundings`. The engine
    /// keeps the lines above the field that a preface with screen text shows, for the next preface.
    pub(super) fn build_preface(&mut self, surroundings: &Surroundings, desktop: &dyn Desktop) -> Preface {
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
        let prompt_screen = self.screen_reads.prompt_text(LineScope::of(surroundings), now);
        let typed = self
            .session
            .as_ref()
            .map_or("", |session| session.text_before.as_str());
        let (preface, screen_lines_above) = prompt::preface_with_screen_lines(
            &writer,
            surroundings,
            recent_copy.map(|copied| copied.text).as_deref(),
            prompt_screen.as_deref().map(|text| ScreenPart {
                text,
                max_tokens: self.screen_tokens,
                shown_above: &self.screen_reads.shown_above,
                typed,
            }),
        );
        if self.screen_reads.text().is_some() {
            self.screen_reads.shown_above = screen_lines_above;
        }
        preface
    }
}
