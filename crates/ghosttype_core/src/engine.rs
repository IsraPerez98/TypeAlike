//! Decides what ghost text to show as the user types, and what Tab inserts.

mod earlier_writing;
mod new_line;
mod offer;
mod preface;
mod reaction;
mod request;
mod session;
mod suggestion;

use std::fmt;

use crate::desktop::Desktop;
use crate::history::{HistoryEntry, HistoryTables};
use crate::key::Key;
use crate::settings::{
    ClipboardAccess, PersonalizationSettings, ScreenTextApp, Settings, SuggestionLength, TypoFix, TypoRule,
    WriterName, WriterNotes,
};

use earlier_writing::needs_new_tables;
use offer::Offer;
use preface::ScreenReads;
use session::Session;
use suggestion::split_next_part;

pub use reaction::{AfterAccept, Reaction, WordFix};
#[cfg(test)]
pub(crate) use request::TYPO_CUT;

/// Tracks the text at the caret and the model's suggestion for it.
pub(crate) struct Engine {
    session: Option<Session>,
    offer: Offer,
    unanswered_requests: usize,
    writer_notes: WriterNotes,
    writer_name: WriterName,
    clipboard_access: ClipboardAccess,
    suggestion_length: SuggestionLength,
    typo_rule: TypoRule,
    typo_fix: TypoFix,
    /// The most texts of the history that match the topic of the field, which a prompt shows.
    matching_examples: usize,
    screen_reads: ScreenReads,
    personalization: PersonalizationSettings,
    /// The earlier writing of the user, oldest first.
    history: Vec<HistoryEntry>,
    /// Whether `history` changed since the last [`Engine::history_to_save`].
    has_unsaved_history: bool,
    /// The preference for the words of each pool of `history`, the phrases that it repeats and the
    /// words after its runs of words, which have none when the word choice is off.
    tables: HistoryTables,
    /// The newest version of a word bias of `tables`.
    word_bias_version: u64,
}

impl Default for Engine {
    /// Returns an engine that uses the default settings.
    fn default() -> Self {
        let mut engine = Self {
            session: None,
            offer: Offer::default(),
            unanswered_requests: 0,
            writer_notes: WriterNotes::default(),
            writer_name: WriterName::default(),
            clipboard_access: ClipboardAccess::default(),
            suggestion_length: SuggestionLength::default(),
            typo_rule: TypoRule::default(),
            typo_fix: TypoFix::default(),
            matching_examples: 0,
            screen_reads: ScreenReads::default(),
            personalization: PersonalizationSettings::default(),
            history: Vec::new(),
            has_unsaved_history: false,
            tables: HistoryTables::default(),
            word_bias_version: 0,
        };
        engine.use_settings(&Settings::default());
        engine
    }
}

impl fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("text_before_chars", &self.text_before_caret().chars().count())
            .field("is_growing", &matches!(self.offer, Offer::Growing { .. }))
            .field("has_suggestion", &!self.offer.shown().is_empty())
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// Uses the writer name, the writer notes, the clipboard access and the personalization of
    /// `settings` from the next session on, and its suggestion length from the next request on.
    pub(crate) fn use_settings(&mut self, settings: &Settings) {
        let needs_new_tables = needs_new_tables(&self.personalization, &settings.personalization);
        self.writer_notes = settings.writer_notes.clone();
        self.writer_name = settings.writer_name.clone();
        self.personalization = settings.personalization.clone();
        self.clipboard_access = settings.clipboard_access;
        self.suggestion_length = settings.suggestion_length_in_use();
        self.typo_rule = settings.typo_rule;
        self.typo_fix = settings.typo_fix;
        self.matching_examples = settings.matching_examples();
        if needs_new_tables {
            self.refresh_tables();
        }
    }

    /// Updates the text at the caret for a key pressed in the focused field of `desktop`.
    pub(crate) fn on_key(&mut self, key: Key, desktop: &dyn Desktop) -> Reaction {
        if key != Key::Enter {
            self.read_field_after_enter(desktop);
        }
        match key {
            Key::Char(character) if !character.is_control() => {
                self.start_session(desktop).text_before.push(character);
                self.on_typed(character)
            }
            Key::Backspace => {
                self.start_session(desktop).delete_last_character();
                self.request()
            }
            Key::Tab if desktop.is_ghost_shown() => self.accept(),
            Key::Escape | Key::Dead => self.dismiss(),
            Key::Enter => self.wait_for_new_line(desktop),
            Key::Char(_) | Key::Tab | Key::Other => self.reset(),
        }
    }

    /// Whether the line of the caret has no text after the caret, also without a session. `false`
    /// in a field that hides that line, because text can follow the caret there.
    #[must_use]
    pub(crate) fn is_caret_at_line_end(&self) -> bool {
        self.session.as_ref().is_none_or(Session::is_caret_at_line_end)
    }

    /// Ends the session, for when the caret moved somewhere unknown. The history keeps the text
    /// before the caret when the personalization allows it.
    pub(crate) fn reset(&mut self) -> Reaction {
        if let Some(session) = self.session.take() {
            self.keep_text_of(&session);
        }
        self.offer = Offer::Idle;
        Reaction::Hide
    }

    /// Whether a session runs.
    #[must_use]
    pub(crate) fn has_session(&self) -> bool {
        self.session.is_some()
    }

    /// The app of the session, if a session runs in an app with an id.
    #[must_use]
    pub(crate) fn session_app(&self) -> Option<&ScreenTextApp> {
        self.session.as_ref()?.field.app.as_ref()
    }

    /// Returns the session, and starts it with what surrounds the caret in `desktop` when there
    /// is none.
    fn start_session(&mut self, desktop: &dyn Desktop) -> &mut Session {
        let session = self.session.take().unwrap_or_else(|| {
            let surroundings = desktop.surroundings();
            Session::new(self.preface(&surroundings, desktop), &surroundings)
        });
        self.session.insert(session)
    }

    fn on_typed(&mut self, character: char) -> Reaction {
        self.keep_recent_text();
        let rest = match self.offer.shown().strip_prefix(character) {
            Some(rest) if !rest.is_empty() => rest.to_owned(),
            _ => return self.request(),
        };
        self.offer.take(character.encode_utf8(&mut [0; 4]), &rest);
        Reaction::Show(rest)
    }

    fn accept(&mut self) -> Reaction {
        if let Offer::Fix(fix) = &self.offer {
            let fix = fix.clone();
            return self.accept_fix(fix);
        }
        let suggestion = self.offer.shown().to_owned();
        let Some(session) = self.session.as_mut().filter(|_| !suggestion.is_empty()) else {
            return self.reset();
        };
        let (part, rest) = split_next_part(&suggestion);
        session.text_before.push_str(part);
        session.has_accepted = true;
        self.keep_recent_text();
        self.offer.take(part, rest);
        Reaction::Accept {
            text: part.to_owned(),
            then: self.after_accept(rest),
        }
    }

    /// Replaces the typo of `fix` before the caret with its word, and keeps the rest as the ghost
    /// text.
    fn accept_fix(&mut self, fix: WordFix) -> Reaction {
        let Some(session) = self.session.as_mut() else {
            return self.reset();
        };
        session.replace_end(&fix.typo, &fix.word);
        session.has_accepted = true;
        self.keep_recent_text();
        self.offer.take(&fix.word, &fix.rest);
        Reaction::Replace {
            deleted: fix.typo.chars().count(),
            then: self.after_accept(&fix.rest),
            text: fix.word,
        }
    }

    /// Returns what follows a part that Tab typed: `rest`, the rest of the suggestion, else what a
    /// typed key gets. A completion that still runs gets no new request, because it can add words.
    fn after_accept(&mut self, rest: &str) -> AfterAccept {
        if !rest.is_empty() {
            return AfterAccept::Rest(rest.to_owned());
        }
        let next = if matches!(self.offer, Offer::Growing { .. }) {
            Reaction::Hide
        } else {
            self.request()
        };
        AfterAccept::Next(Box::new(next))
    }

    /// Returns the text before the caret in the session, empty without a session.
    #[must_use]
    pub(crate) fn text_before_caret(&self) -> &str {
        self.session
            .as_ref()
            .map_or("", |session| session.text_before.as_str())
    }

    fn keep_recent_text(&mut self) {
        if let Some(session) = &mut self.session {
            session.keep_recent_text();
        }
    }
}

#[cfg(test)]
mod tests;
