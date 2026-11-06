//! Decides what ghost text to show as the user types, and what Tab inserts.

mod autocorrect;
mod earlier_writing;
mod new_line;
mod offer;
mod preface;
mod reaction;
mod request;
mod session;
mod shown_suggestions;
mod spelling;
mod suggestion;

use std::fmt;

use crate::desktop::{Desktop, Surroundings, Timestamp};
use crate::excerpt::{plain_spaces, without_zero_width_spaces};
use crate::history::{HistoryEntry, HistoryTables};
use crate::inference::{SentenceEnd, TokenCount, TypoCheck};
use crate::key::{Key, Press};
use crate::settings::{
    ClipboardAccess, PersonalizationSettings, ScreenTextApp, Settings, SuggestionLength, TextLimits, TypoFix,
    WriterName, WriterNotes,
};

use earlier_writing::needs_new_tables;
use offer::Offer;
use preface::ScreenReads;
use session::Session;
use spelling::Dictionary;
use suggestion::{ends_with_typo, fix_after_delimiter, kept_delimiter};

pub(crate) use preface::ScreenAnswer;
pub use reaction::{AfterAccept, Reaction, WordFix};
pub(crate) use suggestion::split_next_part;

/// Tracks the text at the caret and the model's suggestion for it.
pub(crate) struct Engine {
    session: Option<Session>,
    offer: Offer,
    unanswered_requests: usize,
    writer_notes: WriterNotes,
    writer_name: WriterName,
    clipboard_access: ClipboardAccess,
    suggestion_length: SuggestionLength,
    typo_check: TypoCheck,
    typo_fix: TypoFix,
    sentence_end: SentenceEnd,
    /// The most texts of the history that match the topic of the field, which a prompt shows.
    matching_examples: usize,
    /// The most estimated tokens of the screen text in a prompt.
    screen_tokens: TokenCount,
    /// How much text before the caret a session keeps.
    text_limits: TextLimits,
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
    /// The newest time of the clock of the desktop, read at the start of each session.
    clock: Option<Timestamp>,
    /// The time of the clock when the engine built `tables`.
    tables_clock: Option<Timestamp>,
    /// The dictionary of the OS that cuts a suggestion before a word that the model made up, if
    /// the platform has one.
    dictionary: Option<Dictionary>,
    /// Whether the user rejected a suggestion in the field, so that the keys there get none.
    is_field_paused: bool,
    /// Whether the OS repeated a held key after the last suggestion, so that the end of the hold
    /// asks for one.
    is_suggestion_due: bool,
}

impl Default for Engine {
    /// Returns an engine that uses the default settings.
    fn default() -> Self {
        let settings = Settings::default();
        let mut engine = Self {
            session: None,
            offer: Offer::default(),
            unanswered_requests: 0,
            writer_notes: WriterNotes::default(),
            writer_name: WriterName::default(),
            clipboard_access: ClipboardAccess::default(),
            suggestion_length: SuggestionLength::default(),
            typo_check: TypoCheck::Off,
            typo_fix: TypoFix::default(),
            sentence_end: SentenceEnd::default(),
            matching_examples: 0,
            screen_tokens: TokenCount(0),
            text_limits: settings.text_limits(),
            screen_reads: ScreenReads::default(),
            personalization: PersonalizationSettings::default(),
            history: Vec::new(),
            has_unsaved_history: false,
            tables: HistoryTables::default(),
            word_bias_version: 0,
            clock: None,
            tables_clock: None,
            dictionary: None,
            is_field_paused: false,
            is_suggestion_due: false,
        };
        engine.use_settings(&settings);
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
    /// Uses the writer name, the writer notes, the clipboard access, the personalization, the
    /// number of matching examples, the budget of the screen text and the limits of the text before
    /// the caret of `settings` from the next session on. Uses its suggestion length, its typo rule,
    /// its typo fix and its sentence end from the next request on.
    pub(crate) fn use_settings(&mut self, settings: &Settings) {
        let needs_new_tables = needs_new_tables(&self.personalization, &settings.personalization);
        self.writer_notes = settings.writer_notes.clone();
        self.writer_name = settings.writer_name.clone();
        self.personalization = settings.personalization.clone();
        self.clipboard_access = settings.clipboard_access;
        self.suggestion_length = settings.suggestion_length_in_use();
        self.typo_check = settings.typo_check();
        self.typo_fix = settings.typo_fix;
        self.sentence_end = settings.sentence_end;
        self.matching_examples = settings.matching_examples();
        self.screen_tokens = settings.screen_tokens();
        self.text_limits = settings.text_limits();
        self.drop_reply_words_when_off();
        if needs_new_tables {
            self.refresh_tables();
        }
    }

    /// Updates the text at the caret for a key pressed in the focused field of `desktop`. A key
    /// that the OS repeats changes the text, but asks for no suggestion until
    /// [`suggest_after_hold`](Self::suggest_after_hold).
    pub(crate) fn on_key(&mut self, key: Key, press: Press, desktop: &dyn Desktop) -> Reaction {
        if let Some(session) = &mut self.session {
            session.has_untouched_request = false;
        }
        if key != Key::Enter {
            self.read_field_after_enter(desktop);
        }
        match key {
            Key::Char(character) if !character.is_control() => {
                self.start_session(desktop)
                    .append_text(character.encode_utf8(&mut [0; 4]));
                self.on_typed(character, press)
            }
            Key::Backspace => {
                self.start_session(desktop).delete_last_character();
                self.suggest_after(press)
            }
            Key::Tab if desktop.is_ghost_shown() => self.accept(desktop),
            Key::Escape if desktop.is_ghost_shown() => self.pause_field(),
            Key::Escape => self.reject(),
            Key::Dead => self.dismiss(),
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
        self.is_suggestion_due = false;
        Reaction::Hide
    }

    /// Ends the session, for when suggestions turn off in its place. The history keeps none of its
    /// text.
    pub(crate) fn discard_session(&mut self) {
        self.session = None;
        self.offer = Offer::Idle;
        self.is_suggestion_due = false;
    }

    /// Ends the pause of the suggestions in the field, so that the next key there gets a suggestion
    /// again.
    pub(crate) fn end_field_pause(&mut self) {
        self.is_field_paused = false;
    }

    /// Whether a session runs.
    #[must_use]
    pub(crate) fn has_session(&self) -> bool {
        self.session.is_some()
    }

    /// Whether [`on_key`](Self::on_key) with `key` starts a session: a key that types or deletes a
    /// character while no session runs.
    #[must_use]
    pub(crate) fn starts_session_with(&self, key: Key) -> bool {
        let edits_text =
            matches!(key, Key::Char(character) if !character.is_control()) || key == Key::Backspace;
        edits_text && self.session.is_none()
    }

    /// Whether the ghost text has a suggestion or a fix, or the completion of the newest request
    /// still runs and can give one.
    #[must_use]
    pub(crate) fn has_offer(&self) -> bool {
        !matches!(self.offer, Offer::Idle)
    }

    /// The app of the session, if a session runs in an app with an id.
    #[must_use]
    pub(crate) fn session_app(&self) -> Option<&ScreenTextApp> {
        self.session.as_ref()?.field.app.as_ref()
    }

    /// The host of the website of the session, if a session runs on a website.
    #[must_use]
    pub(crate) fn session_domain(&self) -> Option<&str> {
        self.session.as_ref()?.field.domain.as_deref()
    }

    /// Returns the session, and starts it with what surrounds the caret in `desktop` when there
    /// is none.
    fn start_session(&mut self, desktop: &dyn Desktop) -> &mut Session {
        if self.session.is_none() {
            self.follow_clock(desktop);
        }
        let session = self.session.take().unwrap_or_else(|| {
            let surroundings = read_surroundings(desktop);
            let mut session = Session::new(
                self.build_preface(&surroundings, desktop),
                &surroundings,
                desktop.now(),
                self.text_limits,
            );
            session.reply_to = self.reply_words(&surroundings);
            session
        });
        self.session.insert(session)
    }

    fn on_typed(&mut self, character: char, press: Press) -> Reaction {
        if let Offer::Fix(fix) = &self.offer
            && let Some(kept) = fix_after_delimiter(fix, character)
        {
            self.offer = Offer::Fix(kept);
            return self.offer.reaction();
        }
        let rest = match self.offer.shown().strip_prefix(character) {
            Some(rest) if !rest.is_empty() => rest.to_owned(),
            _ if self.keeps_completion_after(character) => String::new(),
            _ => return self.suggest_after(press),
        };
        self.offer.take(character.encode_utf8(&mut [0; 4]), &rest);
        self.offer.reaction()
    }

    fn accept(&mut self, desktop: &dyn Desktop) -> Reaction {
        if let Offer::Fix(fix) = &self.offer {
            let fix = fix.clone();
            return self.accept_fix(fix, desktop);
        }
        let suggestion = self.offer.shown().to_owned();
        let Some(session) = self.session.as_mut().filter(|_| !suggestion.is_empty()) else {
            return self.reset();
        };
        let (part, rest) = split_next_part(&suggestion);
        session.append_text(part);
        session.has_accepted = true;
        self.offer.take(part, rest);
        Reaction::Accept {
            text: part.to_owned(),
            then: self.after_accept(rest),
        }
    }

    /// Replaces the typo of `fix` before the caret with its word, and keeps the rest as the ghost
    /// text. When the field of `desktop` shows that the app changed the typo, it replaces nothing
    /// and ends the session, so that the next key starts from the text of the field.
    fn accept_fix(&mut self, fix: WordFix, desktop: &dyn Desktop) -> Reaction {
        if is_typo_changed_in(desktop, &fix) {
            self.reset();
            return Reaction::Swallow;
        }
        let Some(session) = self.session.as_mut() else {
            return self.reset();
        };
        session.replace_end(&fix.typo, &fix.word);
        session.has_accepted = true;
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
            self.suggest()
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
}

/// Whether the field of `desktop` shows other text than the typo of `fix`, because the app changed
/// the word at the space or the sign that the fix kept, as an autocorrect does. It reads the field
/// only for a fix that kept a space or a sign. A field without text counts as unchanged.
fn is_typo_changed_in(desktop: &dyn Desktop, fix: &WordFix) -> bool {
    !kept_delimiter(fix).is_empty()
        && read_surroundings(desktop)
            .field_text
            .is_some_and(|field_text| !ends_with_typo(&field_text.before_caret, fix))
}

/// Reads what surrounds the caret in `desktop`, with plain spaces and no zero-width spaces in the
/// field text. A web editor shows a space that the user typed at the end of a text as a no-break
/// space.
fn read_surroundings(desktop: &dyn Desktop) -> Surroundings {
    let mut surroundings = desktop.surroundings();
    if let Some(field_text) = &mut surroundings.field_text {
        field_text.before_caret = without_zero_width_spaces(&plain_spaces(&field_text.before_caret));
        field_text.after_caret = without_zero_width_spaces(&plain_spaces(&field_text.after_caret));
    }
    surroundings
}

#[cfg(test)]
mod tests;
