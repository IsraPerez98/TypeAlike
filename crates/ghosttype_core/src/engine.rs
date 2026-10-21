//! Decides what ghost text to show as the user types, and what Tab inserts.

use std::time::Duration;
use std::{fmt, mem};

use crate::desktop::{
    Desktop, MAX_CHARS_BEFORE_CARET, ScreenRead, ScreenReadId, ScreenSource, ScreenText, Surroundings,
};
use crate::excerpt::{estimated_tokens, last_chars, last_tokens};
use crate::history::{self, HistoryEntry, HistoryPool, HistoryTables, Place};
use crate::inference::{
    CONTEXT_TOKENS, Completion, Correction, InferenceError, InferenceRequest, LogProbability, NextWordBias,
    Prompt, TokenCount, WordBias,
};
use crate::key::Key;
use crate::place;
use crate::prompt::{self, Writer};
use crate::settings::{
    ClipboardAccess, HistoryScope, KeptTexts, OcrLevel, PersonalizationSettings, ScreenTextApp,
    ScreenTextSettings, Settings, SuggestionLength, TypoFix, TypoRule, WriterName, WriterNotes,
};

const PREPARATION_TOKENS: TokenCount = TokenCount(0);
/// The least likely that the typed letters of the last word can be before GhostType takes them
/// for a typo. In the prompt eval, this cut catches 26 to 29 of the 29 typos and hides 1 to 3 of
/// 393 other cases, each a rare name or word that the suggestion did not finish right.
pub(crate) const TYPO_CUT: LogProbability = LogProbability(-11.0);
/// The characters before the caret that a session keeps when its text reaches
/// [`MAX_CHARS_BEFORE_CARET`]. The text drops its oldest characters in large steps, so that the
/// start of the prompt stays the same for many keys and the model runner can reuse its work.
const TRIMMED_CHARS_BEFORE_CARET: usize = 3000;
/// The age after which a copy has no relation to the text that the user types.
const MAX_CLIPBOARD_AGE: Duration = Duration::from_mins(5);
/// The last words before the caret that a suggestion copies when it holds them in a row. Fewer
/// words are often the correct phrase again.
const COPIED_WORDS: usize = 4;
const MAX_PATTERN_WORDS: usize = 3;
const PATTERN_REPEATS: usize = 3;
const CLOSING_PUNCTUATION: [char; 7] = [',', '.', ';', ':', '!', '?', '…'];
/// The signs that can close a word: closing punctuation, quotes and brackets. The apostrophe is not
/// one, because it also ends a possessive, as in "the students'".
const CLOSING_SIGNS: [char; 13] = [',', '.', ';', ':', '!', '?', '…', '"', '”', ')', ']', '}', '»'];
/// The characters that end a line in the text of a field. Word ends a line inside a paragraph with
/// a vertical tab.
const LINE_BREAKS: [char; 5] = ['\n', '\r', '\u{b}', '\u{2028}', '\u{2029}'];

/// What the platform must do after a key press, a context change or a completion.
#[derive(Debug, Clone, PartialEq)]
pub enum Reaction {
    /// Let the key through and show this ghost text after the caret.
    Show(String),
    /// Let the key through and hide any ghost text.
    Hide,
    /// Swallow the key and type text into the focused app.
    Accept {
        /// The text to type at the caret.
        text: String,
        /// What follows the typed text.
        then: AfterAccept,
    },
    /// Let the key through and show this fix of a typo before the caret.
    ShowFix(WordFix),
    /// Swallow the key, delete the typed letters before the caret, and type the fixed word.
    Replace {
        /// The characters to delete before the caret.
        deleted: usize,
        /// The text to type at the caret after the deletion.
        text: String,
        /// What follows the typed text.
        then: AfterAccept,
    },
    /// Let the key through, hide any ghost text and submit this request to the model runner.
    /// Its completion goes to [`App::on_completion`](crate::app::App::on_completion). It replaces
    /// the requests that the runner did not answer yet.
    Request(InferenceRequest),
}

/// What follows the text that Tab types: the rest of the suggestion, else what the text before the
/// caret gets after a typed key.
#[derive(Debug, Clone, PartialEq)]
pub enum AfterAccept {
    /// Show this rest of the suggestion after the typed text. The user saw it before Tab.
    Rest(String),
    /// The suggestion has no more text. Do what this reaction asks, as after a typed key.
    Next(Box<Reaction>),
}

/// The fix of a typo in the word before the caret, as the ghost text shows it: the typed letters
/// struck through, and the fixed word and the rest of the suggestion after the caret. `Debug` shows
/// only the lengths, so logs never hold what the user wrote.
///
/// ```
/// use ghosttype_core::engine::WordFix;
///
/// let fix = WordFix {
///     typo: "dlei".to_owned(),
///     word: "delivery".to_owned(),
///     rest: " soon".to_owned(),
/// };
///
/// assert_eq!(format!("{fix:?}"), "WordFix(4 chars → 8 chars, 5 chars)");
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct WordFix {
    /// The typed letters just before the caret, which the fix replaces.
    pub typo: String,
    /// The word that replaces `typo`.
    pub word: String,
    /// The rest of the suggestion after `word`, which can be empty.
    pub rest: String,
}

impl fmt::Debug for WordFix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "WordFix({} chars → {} chars, {} chars)",
            self.typo.chars().count(),
            self.word.chars().count(),
            self.rest.chars().count()
        )
    }
}

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
    screen: Screen,
    newest_screen_read: ScreenReadId,
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
            screen: Screen::default(),
            newest_screen_read: ScreenReadId::default(),
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

/// The typing in one place: what the model reads before the text, and the text before the caret.
/// It starts at the first key after a reset, when the key has not reached the app yet.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent fact of the session"
)]
struct Session {
    preface: String,
    text_before: String,
    /// The characters at the start of `text_before` that the field held when the session started.
    field_chars: usize,
    /// The most estimated tokens of `text_before`, so that the prompt fits in the context.
    max_tokens_before: TokenCount,
    text_after: String,
    /// Whether the field hides the text on the line of the caret, so that text can follow the
    /// caret also when `text_after` has none.
    is_caret_line_hidden: bool,
    /// Whether the field gets no suggestions, so that Tab moves the focus out of it.
    is_excluded: bool,
    /// The app where the user types, if it has an id.
    app: Option<ScreenTextApp>,
    /// Whether the platform reads the text of the field.
    is_text_field: bool,
    /// The host of the web page where the user types, if any.
    domain: Option<String>,
    /// Whether the user accepted a suggestion in the session.
    has_accepted: bool,
    /// Whether a read of the screen answered during the session. Until then, the preface holds the
    /// stored text of a read before the session, if any. After it, the session starts no other
    /// read, so that the start of its prompt changes at most one time.
    has_screen_read: bool,
    /// The text before the caret in the field when the user pressed Enter, while the text before
    /// the caret is not known until the next read of the field. It is the text of the session
    /// when the platform gave no field text.
    field_at_enter: Option<String>,
}

impl Session {
    fn is_caret_at_line_end(&self) -> bool {
        !self.is_caret_line_hidden
            && self
                .text_after
                .lines()
                .next()
                .is_none_or(|line| line.trim().is_empty())
    }

    /// Makes `preface` what the model reads before the text, and drops the oldest text before the
    /// caret that does not fit after it.
    fn use_preface(&mut self, preface: String) {
        self.max_tokens_before = tokens_before_caret(&preface);
        self.preface = preface;
        self.keep_recent_text();
    }

    fn keep_recent_text(&mut self) {
        let kept = recent_text(&self.text_before, self.max_tokens_before).len();
        let dropped = self.text_before.drain(..self.text_before.len() - kept);
        self.field_chars = self.field_chars.saturating_sub(dropped.as_str().chars().count());
    }

    fn delete_last_character(&mut self) {
        self.text_before.pop();
        self.field_chars = self.field_chars.min(self.text_before.chars().count());
    }

    /// Replaces `typo` at the end of the text before the caret with `word`.
    fn replace_end(&mut self, typo: &str, word: &str) {
        if let Some(kept) = self.text_before.strip_suffix(typo).map(str::len) {
            self.text_before.truncate(kept);
        }
        self.field_chars = self.field_chars.min(self.text_before.chars().count());
        self.text_before.push_str(word);
    }
}

/// What the engine knows of the text near the field on the screen.
#[derive(Default)]
enum Screen {
    #[default]
    Unknown,
    /// The platform reads the screen from `source`. A session that starts now gets the text when
    /// the answer comes.
    Reading { id: ScreenReadId, source: ScreenSource },
    /// The newest read, for the next session.
    Read(ScreenText),
}

#[derive(Default)]
enum Offer {
    #[default]
    Idle,
    /// The completion of the newest request runs, and its parts grow the ghost text.
    Growing {
        /// The text that the user typed or accepted from the ghost text after the request.
        typed: String,
        /// The ghost text after `typed`, which is empty before the first part.
        shown: String,
    },
    Shown(String),
    /// The fix of a typo before the caret.
    Fix(WordFix),
}

impl Engine {
    /// Uses the writer name, the writer notes, the clipboard access and the personalization of
    /// `settings` from the next session on, and its suggestion length from the next request on.
    pub fn use_settings(&mut self, settings: &Settings) {
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

    /// Uses `entries`, the earlier writing of the user, oldest first, from the next session on.
    pub fn use_history(&mut self, entries: Vec<HistoryEntry>) {
        self.history = entries;
        self.refresh_tables();
    }

    /// Updates the text at the caret for a key pressed in the focused field of `desktop`.
    pub fn on_key(&mut self, key: Key, desktop: &dyn Desktop) -> Reaction {
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

    /// Reads the field of `desktop` again after Enter, and returns the suggestion for the new line
    /// when the session continues there. `None` when the user pressed no Enter since the last read,
    /// or when the session ends.
    pub fn suggest_on_new_line(&mut self, desktop: &dyn Desktop) -> Option<Reaction> {
        self.read_field_after_enter(desktop).then(|| self.request())
    }

    /// Hides the ghost text, keeps the text of the field in `desktop`, and lets the session wait
    /// for a read of its field. A second Enter keeps the text of the first. A session in a field
    /// that the platform cannot read or that gets no suggestions ends.
    fn wait_for_new_line(&mut self, desktop: &dyn Desktop) -> Reaction {
        let Some(session) = self
            .session
            .as_mut()
            .filter(|session| session.is_text_field && !session.is_excluded)
        else {
            return self.reset();
        };
        if session.field_at_enter.is_none() {
            let field_text = desktop.surroundings().field_text;
            let text = field_text
                .as_ref()
                .map_or(session.text_before.as_str(), |field_text| {
                    recent_text(&field_text.before_caret, session.max_tokens_before)
                });
            session.field_at_enter = Some(text.to_owned());
        }
        self.dismiss()
    }

    /// Reads the field of `desktop` when the session waits for a read after Enter. The session
    /// continues with the text that Enter added when the field shows the text of the session, or
    /// the text of the field at Enter, and a new line after it. Else it ends. Returns whether the
    /// session continues on a new line.
    fn read_field_after_enter(&mut self, desktop: &dyn Desktop) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        let Some(field_at_enter) = session.field_at_enter.take() else {
            return false;
        };
        let field_text = desktop.surroundings().field_text.unwrap_or_default();
        let Some(text_before) =
            text_after_enter(&session.text_before, &field_at_enter, &field_text.before_caret)
        else {
            self.reset();
            return false;
        };
        session.text_before = text_before;
        session.keep_recent_text();
        true
    }

    /// Starts the session for the focused field of `desktop` when none runs, and returns the
    /// request that decodes its prompt before the next key. Its completion adds no text. `None`
    /// when a completion of the session runs or the field gets no suggestions.
    pub fn prepare(&mut self, desktop: &dyn Desktop) -> Option<InferenceRequest> {
        let is_completing = self.session.is_some() && self.unanswered_requests > 0;
        if is_completing {
            return None;
        }
        let session = self.start_session(desktop);
        if session.is_excluded {
            return None;
        }
        let prompt = prompt::prompt(&session.preface, &session.text_before);
        self.unanswered_requests = self.unanswered_requests.saturating_add(1);
        Some(InferenceRequest::Complete {
            prompt,
            max_tokens: PREPARATION_TOKENS,
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            min_typed_log_probability: None,
            fixes_typos: false,
        })
    }

    /// Whether the earlier writing that suggestions may use holds a text of the place of the
    /// focused field of `desktop`.
    pub fn has_earlier_writing_in(&self, desktop: &dyn Desktop) -> bool {
        if self.history.is_empty() {
            return false;
        }
        history::session_texts(&self.history, &desktop.surroundings(), &self.personalization).has_local()
    }

    /// Handles the completion of the oldest [`Reaction::Request`] that has no completion yet.
    /// Only the completion of the newest request can show.
    pub fn on_completion(&mut self, result: Result<Completion, InferenceError>) -> Reaction {
        self.unanswered_requests = self.unanswered_requests.saturating_sub(1);
        if self.unanswered_requests > 0 {
            return self.ghost_reaction();
        }
        if let Ok(completion) = result {
            match &completion.correction {
                None => {
                    self.grow(&completion.text);
                }
                Some(correction) => {
                    if let Some(fix) = self.fix_of(correction, &completion.text) {
                        self.offer = Offer::Fix(fix);
                    }
                }
            }
        }
        match mem::take(&mut self.offer) {
            Offer::Growing { shown, .. } | Offer::Shown(shown) if !shown.is_empty() => self.show(shown),
            Offer::Fix(fix) => self.show_fix(fix),
            Offer::Idle | Offer::Growing { .. } | Offer::Shown(_) => Reaction::Hide,
        }
    }

    /// Returns the fix that the `correction` and the added `text` of the newest request give, when
    /// the user typed nothing since the request, the text before the caret ends with the typo and
    /// the suggestion after the fixed letters is not bad.
    fn fix_of(&self, correction: &Correction, text: &str) -> Option<WordFix> {
        let is_untouched = matches!(&self.offer, Offer::Growing { typed, .. } if typed.is_empty());
        if !is_untouched || self.typo_fix == TypoFix::Off {
            return None;
        }
        let session = self.session.as_ref()?;
        let before_typo = session.text_before.strip_suffix(correction.typed.as_str())?;
        let fixed_before = format!("{before_typo}{}", correction.fixed);
        let suggestion = suggestion_after(&fixed_before, text).unwrap_or_default();
        if !suggestion.is_empty() && is_bad_suggestion(&suggestion, &fixed_before, &session.text_after) {
            return None;
        }
        let word_end = suggestion.find(char::is_whitespace).unwrap_or(suggestion.len());
        let unclosed_end = suggestion[..word_end].trim_end_matches(CLOSING_SIGNS).len();
        let (word_rest, rest) = suggestion.split_at(unclosed_end);
        Some(WordFix {
            typo: correction.typed.trim_start().to_owned(),
            word: format!("{}{word_rest}", correction.fixed.trim_start()),
            rest: rest.to_owned(),
        })
    }

    /// Handles `text`, the text that the completion of the oldest [`Reaction::Request`] added so
    /// far. Only its complete words show, and only for the newest request. Returns the reaction
    /// when the ghost text changes, or `None`.
    pub fn on_partial_completion(&mut self, text: &str) -> Option<Reaction> {
        let is_newest = self.unanswered_requests == 1;
        (is_newest && self.grow(complete_words(text))).then(|| self.ghost_reaction())
    }

    /// Returns the reaction that shows the ghost text, or hides it when it is empty.
    fn ghost_reaction(&self) -> Reaction {
        if let Offer::Fix(fix) = &self.offer {
            return Reaction::ShowFix(fix.clone());
        }
        match self.shown() {
            "" => Reaction::Hide,
            shown => Reaction::Show(shown.to_owned()),
        }
    }

    /// Makes the ghost text the text of `completion`, from the newest request, after the typed
    /// text, when it has text there. A bad suggestion empties the ghost text. Returns whether the
    /// ghost text changed.
    fn grow(&mut self, completion: &str) -> bool {
        let (Some(session), Offer::Growing { typed, shown }) = (&self.session, &mut self.offer) else {
            return false;
        };
        match suggestion_after_typed(session, typed, completion) {
            Some(grown) if grown != *shown => {
                *shown = grown;
                true
            }
            _ => false,
        }
    }

    /// Whether the line of the caret has no text after the caret, also without a session. `false`
    /// in a field that hides that line, because text can follow the caret there.
    #[must_use]
    pub fn is_caret_at_line_end(&self) -> bool {
        self.session.as_ref().is_none_or(Session::is_caret_at_line_end)
    }

    /// Ends the session, for when the caret moved somewhere unknown. The history keeps the text
    /// before the caret when the personalization allows it.
    pub fn reset(&mut self) -> Reaction {
        if let Some(session) = self.session.take() {
            self.keep_text_of(&session);
        }
        self.offer = Offer::Idle;
        Reaction::Hide
    }

    fn keep_text_of(&mut self, session: &Session) {
        let Some(app) = &session.app else {
            return;
        };
        let is_wanted = session.has_accepted || self.personalization.kept_texts == KeptTexts::All;
        if session.is_excluded
            || !is_wanted
            || self.personalization.scope_of(&app.app_id) == HistoryScope::Off
        {
            return;
        }
        let domain = session.domain.as_deref();
        let written = history::own_text(
            &self.history,
            &session.text_before,
            session.field_chars,
            &app.app_id,
            domain,
            &self.personalization,
        );
        let Some(entry) = HistoryEntry::of(written, &app.app_id, domain) else {
            return;
        };
        history::add(&mut self.history, entry, &self.personalization);
        self.has_unsaved_history = true;
    }

    /// Returns the earlier writing of the user, oldest first, when it changed since the last call.
    /// The word bias and the phrases then come from the new history.
    pub fn history_to_save(&mut self) -> Option<&[HistoryEntry]> {
        if !mem::take(&mut self.has_unsaved_history) {
            return None;
        }
        self.refresh_tables();
        Some(&self.history)
    }

    /// Forgets the earlier writing of the app with `app_id`.
    pub fn forget_history_of(&mut self, app_id: &str) {
        self.history.retain(|entry| entry.app_id != app_id);
        self.has_unsaved_history = true;
    }

    /// Whether a session runs.
    pub fn has_session(&self) -> bool {
        self.session.is_some()
    }

    /// The app of the session, if a session runs in an app with an id.
    pub fn session_app(&self) -> Option<&ScreenTextApp> {
        self.session.as_ref()?.app.as_ref()
    }

    /// Starts a read of the accessibility tree of the screen, and returns its id. A session that
    /// has its final screen text or waits for a read starts none.
    pub fn start_screen_read(&mut self) -> Option<ScreenReadId> {
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
    pub fn start_session_screen_read(&mut self, screen_text: &ScreenTextSettings) -> Option<ScreenReadId> {
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
    pub fn start_ocr_after(&mut self, read: ScreenReadId, level: OcrLevel) -> Option<ScreenReadId> {
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
    pub fn on_screen_read(&mut self, read: ScreenRead, desktop: &dyn Desktop) -> bool {
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
    pub fn forget_screen(&mut self) {
        self.screen = Screen::Unknown;
    }

    /// Returns the session, and starts it with what surrounds the caret in `desktop` when there
    /// is none.
    fn start_session(&mut self, desktop: &dyn Desktop) -> &mut Session {
        let session = match self.session.take() {
            Some(session) => session,
            None => self.new_session(desktop),
        };
        self.session.insert(session)
    }

    fn new_session(&self, desktop: &dyn Desktop) -> Session {
        let surroundings = desktop.surroundings();
        let preface = self.preface(&surroundings, desktop);
        let max_tokens_before = tokens_before_caret(&preface);
        let (text_before, text_after) = surroundings
            .field_text
            .as_ref()
            .map(|field_text| {
                (
                    recent_text(&field_text.before_caret, max_tokens_before).to_owned(),
                    field_text.after_caret.clone(),
                )
            })
            .unwrap_or_default();
        Session {
            preface,
            field_chars: text_before.chars().count(),
            text_before,
            max_tokens_before,
            text_after,
            is_caret_line_hidden: surroundings.is_caret_line_hidden,
            is_excluded: place::is_address_or_subject_field(&surroundings),
            app: ScreenTextApp::of(&surroundings),
            domain: surroundings.domain.clone(),
            has_accepted: false,
            is_text_field: surroundings.field_text.is_some(),
            has_screen_read: false,
            field_at_enter: None,
        }
    }

    /// Returns what the model reads before the text of the field in `surroundings`.
    fn preface(&self, surroundings: &Surroundings, desktop: &dyn Desktop) -> String {
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

    fn on_typed(&mut self, character: char) -> Reaction {
        self.keep_recent_text();
        let rest = match self.shown().strip_prefix(character) {
            Some(rest) if !rest.is_empty() => rest.to_owned(),
            _ => return self.request(),
        };
        self.take_from_ghost(character.encode_utf8(&mut [0; 4]), &rest);
        Reaction::Show(rest)
    }

    fn accept(&mut self) -> Reaction {
        if let Offer::Fix(fix) = &self.offer {
            let fix = fix.clone();
            return self.accept_fix(fix);
        }
        let suggestion = self.shown().to_owned();
        let Some(session) = self.session.as_mut().filter(|_| !suggestion.is_empty()) else {
            return self.reset();
        };
        let (part, rest) = split_next_part(&suggestion);
        session.text_before.push_str(part);
        session.has_accepted = true;
        self.keep_recent_text();
        self.take_from_ghost(part, rest);
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
        self.take_from_ghost(&fix.word, &fix.rest);
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

    /// Records that `taken`, the start of the ghost text, is now before the caret, and that
    /// `rest` stays as the ghost text.
    fn take_from_ghost(&mut self, taken: &str, rest: &str) {
        match &mut self.offer {
            Offer::Growing { typed, shown } => {
                typed.push_str(taken);
                rest.clone_into(shown);
            }
            offer @ (Offer::Idle | Offer::Shown(_) | Offer::Fix(_)) => {
                *offer = if rest.is_empty() {
                    Offer::Idle
                } else {
                    Offer::Shown(rest.to_owned())
                };
            }
        }
    }

    fn dismiss(&mut self) -> Reaction {
        self.offer = Offer::Idle;
        Reaction::Hide
    }

    /// Shows the rest of a phrase that the writer repeats, else requests a completion.
    fn request(&mut self) -> Reaction {
        if let Some(phrase) = self.repeated_phrase() {
            return self.show(phrase);
        }
        let Some(prompt) = self.prompt() else {
            self.offer = Offer::Idle;
            return Reaction::Hide;
        };
        self.offer = Offer::Growing {
            typed: String::new(),
            shown: String::new(),
        };
        self.unanswered_requests = self.unanswered_requests.saturating_add(1);
        Reaction::Request(InferenceRequest::Complete {
            prompt,
            max_tokens: self.suggestion_length.max_tokens(),
            word_bias: self.session_word_bias(),
            next_word_bias: self.session_next_word_bias(),
            min_log_probability: Some(self.suggestion_length.tail_cut()),
            min_typed_log_probability: self.typo_cut(),
            fixes_typos: self.typo_rule == TypoRule::HideSuggestions && self.typo_fix == TypoFix::Offer,
        })
    }

    /// The least likely that the typed letters of the last word can be, when the rule for typos
    /// hides suggestions.
    fn typo_cut(&self) -> Option<LogProbability> {
        (self.typo_rule == TypoRule::HideSuggestions).then_some(TYPO_CUT)
    }

    /// The pool of the earlier writing that the session uses, and the place of the session. `None`
    /// when no session runs or its app is off.
    fn session_pool(&self) -> Option<(&Session, HistoryPool, Place<&str>)> {
        let session = self.session.as_ref()?;
        let app = session.app.as_ref()?;
        let pool = HistoryPool::of(&app.app_id, &self.personalization)?;
        Some((session, pool, Place::of(&app.app_id, session.domain.as_deref())))
    }

    /// The word bias of the pool of the session.
    fn session_word_bias(&self) -> Option<WordBias> {
        let (_, pool, _) = self.session_pool()?;
        self.tables.word_bias(&pool)
    }

    /// The words that the writer wrote after the last words before the caret, in the pool of the
    /// session.
    fn session_next_word_bias(&self) -> NextWordBias {
        self.session_pool()
            .map(|(session, pool, place)| self.tables.next_word_bias(&pool, place, &session.text_before))
            .unwrap_or_default()
    }

    /// The rest of a phrase that the writer repeats after the text before the caret, in the pool
    /// of the session, when the line after the caret is empty.
    fn repeated_phrase(&self) -> Option<String> {
        let (session, pool, place) = self.session_pool()?;
        if session.is_excluded || !session.is_caret_at_line_end() {
            return None;
        }
        self.tables
            .continuation_of(&pool, place, &session.text_before)
            .map(str::to_owned)
    }

    /// Builds the word bias of each pool, with new versions, and the phrases again from the
    /// history.
    fn refresh_tables(&mut self) {
        self.tables = HistoryTables::of(&self.history, &self.personalization, &mut self.word_bias_version);
    }

    /// Returns the prompt for the text before the caret, or `None` when that text has no words or
    /// the field gets no suggestions.
    fn prompt(&self) -> Option<Prompt> {
        let session = self.session.as_ref().filter(|session| !session.is_excluded)?;
        let has_words = !session.text_before.trim().is_empty();
        has_words.then(|| prompt::prompt(&session.preface, &session.text_before))
    }

    fn show(&mut self, suggestion: String) -> Reaction {
        self.offer = Offer::Shown(suggestion.clone());
        Reaction::Show(suggestion)
    }

    fn show_fix(&mut self, fix: WordFix) -> Reaction {
        self.offer = Offer::Fix(fix.clone());
        Reaction::ShowFix(fix)
    }

    /// Returns the ghost text, which is empty when none shows.
    fn shown(&self) -> &str {
        match &self.offer {
            Offer::Idle | Offer::Fix(_) => "",
            Offer::Growing { shown, .. } | Offer::Shown(shown) => shown,
        }
    }

    /// Returns the text before the caret in the session, empty without a session.
    #[must_use]
    pub fn text_before_caret(&self) -> &str {
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

/// Returns the most estimated tokens of the text before the caret in a prompt with `preface`, so
/// that the prompt leaves room for the longest completion.
/// Whether the tables of the earlier writing for the personalization `new` differ from the tables
/// for `old`: another word choice, or another scope of an app.
fn needs_new_tables(old: &PersonalizationSettings, new: &PersonalizationSettings) -> bool {
    old.word_choice != new.word_choice
        || old.history != new.history
        || old
            .apps
            .iter()
            .chain(&new.apps)
            .any(|app| old.scope_of(&app.app_id) != new.scope_of(&app.app_id))
}

fn tokens_before_caret(preface: &str) -> TokenCount {
    let room = CONTEXT_TOKENS
        .0
        .saturating_sub(SuggestionLength::VeryLong.max_tokens().0);
    let prompt_without_text = prompt::prompt(preface, "");
    TokenCount(room.saturating_sub(estimated_tokens(&prompt_without_text.0).0))
}

/// Returns the end of `text` that a session keeps: all of it while it is shorter than
/// [`MAX_CHARS_BEFORE_CARET`] and `max_tokens`, else its last [`TRIMMED_CHARS_BEFORE_CARET`]
/// characters, within three quarters of `max_tokens`.
fn recent_text(text: &str, max_tokens: TokenCount) -> &str {
    if text.chars().count() < MAX_CHARS_BEFORE_CARET && estimated_tokens(text) < max_tokens {
        return text;
    }
    let trimmed_tokens = TokenCount(max_tokens.0 / 4 * 3);
    last_tokens(last_chars(text, TRIMMED_CHARS_BEFORE_CARET), trimmed_tokens)
}

/// Returns the text before the caret of a session after Enter: `typed`, the text of the session,
/// else `field_at_enter`, the text of the field at Enter, with what Enter added in `field_text`.
/// The field text counts when the app changed the typed text, as a dead key or an autocorrect
/// does. `None` when `field_text` shows neither with a new line.
fn text_after_enter(typed: &str, field_at_enter: &str, field_text: &str) -> Option<String> {
    [typed, field_at_enter]
        .into_iter()
        .find_map(|text| text_on_new_line(text, field_text).map(|added| format!("{text}{added}")))
}

/// Returns what Enter added after `text` in `field_text`, the text before the caret after Enter:
/// line breaks, then signs without letters, such as an indent or a bullet. `None` when
/// `field_text` is not `text` with such an addition.
fn text_on_new_line<'field>(text: &str, field_text: &'field str) -> Option<&'field str> {
    let line_start = field_text
        .trim_end_matches(|character| !LINE_BREAKS.contains(&character))
        .len();
    let breaks_start = field_text[..line_start].trim_end().len();
    let added_start = field_text[breaks_start..line_start]
        .char_indices()
        .filter(|(_, character)| LINE_BREAKS.contains(character))
        .map(|(offset, _)| breaks_start + offset)
        .find(|&end| field_text[..end].ends_with(text))?;
    let new_line = &field_text[line_start..];
    (!new_line.contains(char::is_alphabetic)).then(|| &field_text[added_start..])
}

/// Returns the first line of `completion` up to a placeholder as ghost text after `context`, or
/// `None` when it has no text. Spaces that would double the space at the end of `context`, or
/// trail, are dropped.
fn suggestion_after(context: &str, completion: &str) -> Option<String> {
    let first_line = completion.lines().next().unwrap_or_default();
    let line = if context.ends_with(char::is_whitespace) {
        first_line.trim_start()
    } else {
        first_line
    };
    let suggestion = before_placeholder(line).trim_end();
    (!suggestion.is_empty()).then(|| suggestion.to_owned())
}

/// Returns `text` up to the first word after an opening square bracket, such as "[Your Name]",
/// which a model writes in place of a fact that it does not know. The token limit can cut the
/// placeholder before its closing bracket.
fn before_placeholder(text: &str) -> &str {
    text.match_indices('[')
        .find(|&(start, _)| text[start + 1..].starts_with(char::is_alphanumeric))
        .map_or(text, |(start, _)| &text[..start])
}

/// Returns the ghost text that `completion` gives after the `typed` text, which is empty for a
/// bad suggestion, or `None` when it adds nothing after `typed`. The text before the caret of
/// `session` ends with `typed`, and the request continued the text before `typed`.
fn suggestion_after_typed(session: &Session, typed: &str, completion: &str) -> Option<String> {
    let context = session.text_before.strip_suffix(typed)?;
    let suggestion = suggestion_after(context, completion)?;
    if is_bad_suggestion(&suggestion, context, &session.text_after) {
        return Some(String::new());
    }
    let rest = suggestion.strip_prefix(typed).filter(|rest| !rest.is_empty())?;
    Some(rest.to_owned())
}

/// Whether `suggestion` after `context` has a shape that is wrong almost each time: no letter or
/// digit, closing punctuation after a space, the start of `text_after` again, a copy of the last
/// words of `context`, or a loop of a few words.
fn is_bad_suggestion(suggestion: &str, context: &str, text_after: &str) -> bool {
    let has_no_word = !suggestion.chars().any(char::is_alphanumeric);
    let closes_after_space =
        context.ends_with(char::is_whitespace) && suggestion.starts_with(CLOSING_PUNCTUATION);
    has_no_word
        || closes_after_space
        || repeats_text_after(suggestion, text_after)
        || copies_last_words(suggestion, context)
        || repeats_a_pattern(suggestion)
}

/// Returns the words of `text` in lowercase: its runs of letters, digits and apostrophes.
fn words(text: &str) -> impl DoubleEndedIterator<Item = String> + '_ {
    text.split(|character: char| !(character.is_alphanumeric() || matches!(character, '\'' | '’')))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

/// Whether the words of `suggestion` and of `text_after` are the same up to the end of the
/// shorter.
fn repeats_text_after(suggestion: &str, text_after: &str) -> bool {
    let suggested = words(suggestion).collect::<Vec<_>>();
    let after = words(text_after).take(suggested.len()).collect::<Vec<_>>();
    !after.is_empty() && suggested.starts_with(&after)
}

fn copies_last_words(suggestion: &str, context: &str) -> bool {
    let mut last_words = words(context).rev().take(COPIED_WORDS).collect::<Vec<_>>();
    last_words.reverse();
    let suggested = words(suggestion).collect::<Vec<_>>();
    last_words.len() == COPIED_WORDS
        && suggested
            .windows(COPIED_WORDS)
            .any(|run| run == last_words.as_slice())
}

/// Whether `suggestion` has a pattern of one to [`MAX_PATTERN_WORDS`] words
/// [`PATTERN_REPEATS`] times in a row.
fn repeats_a_pattern(suggestion: &str) -> bool {
    let suggested = words(suggestion).collect::<Vec<_>>();
    (1..=MAX_PATTERN_WORDS).any(|pattern_words| {
        suggested
            .windows(pattern_words * PATTERN_REPEATS)
            .any(|run| run[pattern_words..] == run[..run.len() - pattern_words])
    })
}

/// Returns `text` up to its last whitespace, without the word after it, which can still grow.
fn complete_words(text: &str) -> &str {
    text.rfind(char::is_whitespace).map_or("", |end| &text[..end])
}

/// Splits `suggestion` into the part that Tab types and the rest. The part is the first word with
/// the whitespace before it, without the [`CLOSING_SIGNS`] at its end, which the next Tab types.
/// A word of only signs is one part.
fn split_next_part(suggestion: &str) -> (&str, &str) {
    let rest = suggestion
        .trim_start()
        .trim_start_matches(|character: char| !character.is_whitespace());
    let word = &suggestion[..suggestion.len() - rest.len()];
    let unclosed_word = word.trim_end_matches(CLOSING_SIGNS);
    let part = if unclosed_word.trim().is_empty() {
        word
    } else {
        unclosed_word
    };
    suggestion.split_at(part.len())
}

impl fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("text_before_chars", &self.text_before_caret().chars().count())
            .field("is_growing", &matches!(self.offer, Offer::Growing { .. }))
            .field("has_suggestion", &!self.shown().is_empty())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::app::Field;
    use crate::desktop::{Confidence, CopiedText, FieldLabels, FieldText, ScreenLine, ScreenRect};
    use crate::inference::Backend;
    use crate::settings::{HistoryAccess, HistoryApp, WordChoice};

    #[derive(Default)]
    struct FakeDesktop {
        surroundings: Surroundings,
        copied: Option<CopiedText>,
        surroundings_reads: Cell<usize>,
        clipboard_reads: Cell<usize>,
        is_ghost_hidden: bool,
    }

    impl FakeDesktop {
        fn with_field_text(before_caret: &str) -> Self {
            Self::with_text_around(before_caret, "")
        }

        fn with_text_around(before_caret: &str, after_caret: &str) -> Self {
            Self {
                surroundings: Surroundings {
                    field_text: Some(FieldText {
                        before_caret: before_caret.to_owned(),
                        after_caret: after_caret.to_owned(),
                    }),
                    ..Surroundings::default()
                },
                ..Self::default()
            }
        }

        fn in_field(app_id: &str, field_name: &str) -> Self {
            Self {
                surroundings: Surroundings {
                    app_id: Some(app_id.to_owned()),
                    field_labels: FieldLabels {
                        name: Some(field_name.to_owned()),
                        ..FieldLabels::default()
                    },
                    ..Surroundings::default()
                },
                ..Self::default()
            }
        }

        fn with_copy(text: &str, age: Option<Duration>) -> Self {
            Self {
                copied: Some(CopiedText {
                    text: text.to_owned(),
                    age,
                }),
                ..Self::default()
            }
        }
    }

    impl Desktop for FakeDesktop {
        fn field(&self) -> Field {
            Field::Open
        }

        fn surroundings(&self) -> Surroundings {
            self.surroundings_reads.set(self.surroundings_reads.get() + 1);
            self.surroundings.clone()
        }

        fn copied_text(&self) -> Option<CopiedText> {
            self.clipboard_reads.set(self.clipboard_reads.get() + 1);
            self.copied.clone()
        }

        fn is_ghost_shown(&self) -> bool {
            !self.is_ghost_hidden
        }
    }

    fn press(engine: &mut Engine, key: Key) -> Reaction {
        engine.on_key(key, &FakeDesktop::default())
    }

    fn engine_with(settings: &Settings) -> Engine {
        let mut engine = Engine::default();
        engine.use_settings(settings);
        engine
    }

    fn request_with(lines: &[&str], text_before: &str) -> Reaction {
        Reaction::Request(InferenceRequest::Complete {
            prompt: prompt::framed_prompt(lines, text_before),
            max_tokens: Settings::default().suggestion_length_in_use().max_tokens(),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: Some(Settings::default().suggestion_length_in_use().tail_cut()),
            min_typed_log_probability: Some(TYPO_CUT),
            fixes_typos: true,
        })
    }

    fn request(text_before: &str) -> Reaction {
        request_with(&[], text_before)
    }

    #[expect(clippy::unnecessary_wraps, reason = "on_completion takes a Result")]
    fn completion(text: &str) -> Result<Completion, InferenceError> {
        Ok(Completion {
            text: text.to_owned(),
            tokens: TokenCount(1),
            duration: Duration::ZERO,
            word_log_probabilities: Vec::new(),
            typed_log_probability: LogProbability::default(),
            correction: None,
        })
    }

    fn show(text: &str) -> Reaction {
        Reaction::Show(text.to_owned())
    }

    fn accept(text: &str, rest: &str) -> Reaction {
        Reaction::Accept {
            text: text.to_owned(),
            then: AfterAccept::Rest(rest.to_owned()),
        }
    }

    fn accept_last(text: &str, next: Reaction) -> Reaction {
        Reaction::Accept {
            text: text.to_owned(),
            then: AfterAccept::Next(Box::new(next)),
        }
    }

    fn type_text(engine: &mut Engine, text: &str) -> Reaction {
        text.chars()
            .map(|character| press(engine, Key::Char(character)))
            .last()
            .expect("text is not empty")
    }

    fn engine_waiting_for(context: &str) -> Engine {
        let mut engine = Engine::default();
        for character in context.chars() {
            engine.on_completion(completion(""));
            press(&mut engine, Key::Char(character));
        }
        engine
    }

    fn engine_showing(context: &str, suggestion: &str) -> Engine {
        let mut engine = engine_waiting_for(context);
        engine.on_completion(completion(suggestion));
        engine
    }

    #[test]
    fn typing_requests_completion_of_context() {
        let mut engine = Engine::default();

        let reaction = press(&mut engine, Key::Char('h'));

        assert_eq!(reaction, request("h"));
    }

    #[test]
    fn typing_while_waiting_requests_the_new_context() {
        let mut engine = engine_waiting_for("h");

        let reaction = press(&mut engine, Key::Char('e'));

        assert_eq!(reaction, request("he"));
    }

    #[test]
    fn whitespace_context_requests_nothing() {
        let mut engine = Engine::default();

        let reaction = press(&mut engine, Key::Char(' '));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn completion_for_current_context_shows() {
        let mut engine = engine_waiting_for("hel");

        let reaction = engine.on_completion(completion("lo"));

        assert_eq!(reaction, show("lo"));
    }

    #[test]
    fn completion_of_a_replaced_request_is_dropped() {
        let mut engine = engine_waiting_for("he");
        press(&mut engine, Key::Char('l'));

        let reaction = engine.on_completion(completion("llo"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn completion_of_the_newest_request_shows() {
        let mut engine = engine_waiting_for("he");
        press(&mut engine, Key::Char('l'));
        engine.on_completion(Err(InferenceError::Superseded));

        let reaction = engine.on_completion(completion("lo"));

        assert_eq!(reaction, show("lo"));
    }

    #[test]
    fn failed_completion_hides() {
        let mut engine = engine_waiting_for("hel");

        let reaction = engine.on_completion(Err(InferenceError::NoModel));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn completion_without_text_on_first_line_hides() {
        let cases = ["", "  ", "\nnext line"];

        let reactions = cases.map(|text| engine_waiting_for("hel").on_completion(completion(text)));

        assert_eq!(reactions, cases.map(|_| Reaction::Hide));
    }

    #[expect(clippy::unnecessary_wraps, reason = "on_completion takes a Result")]
    fn correction(typed: &str, fixed: &str, text: &str) -> Result<Completion, InferenceError> {
        Ok(Completion {
            correction: Some(Correction {
                typed: typed.to_owned(),
                fixed: fixed.to_owned(),
            }),
            ..completion(text).expect("the completion has text")
        })
    }

    fn word_fix(typo: &str, word: &str, rest: &str) -> WordFix {
        WordFix {
            typo: typo.to_owned(),
            word: word.to_owned(),
            rest: rest.to_owned(),
        }
    }

    fn engine_showing_a_fix() -> Engine {
        let mut engine = engine_waiting_for("the dlei");
        engine.on_completion(correction(" dlei", " deli", "very soon"));
        engine
    }

    #[test]
    fn completion_that_fixes_a_typo_shows_the_fix() {
        let mut engine = engine_waiting_for("the dlei");

        let reaction = engine.on_completion(correction(" dlei", " deli", "very soon"));

        assert_eq!(reaction, Reaction::ShowFix(word_fix("dlei", "delivery", " soon")));
    }

    #[test]
    fn fix_that_finishes_its_word_shows_the_rest_after_it() {
        let mut engine = engine_waiting_for("I will recieve");

        let reaction = engine.on_completion(correction(" recieve", " receive", " the parcel"));

        assert_eq!(
            reaction,
            Reaction::ShowFix(word_fix("recieve", "receive", " the parcel"))
        );
    }

    #[test]
    fn tab_on_a_fix_replaces_the_typo_with_the_fixed_word() {
        let mut engine = engine_showing_a_fix();

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(
            reaction,
            Reaction::Replace {
                deleted: 4,
                text: "delivery".to_owned(),
                then: AfterAccept::Rest(" soon".to_owned()),
            }
        );
    }

    #[test]
    fn tab_on_a_fix_without_a_rest_requests_the_next_suggestion() {
        let mut engine = engine_waiting_for("the dlei");
        engine.on_completion(correction(" dlei", " deli", "very"));

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(
            reaction,
            Reaction::Replace {
                deleted: 4,
                text: "delivery".to_owned(),
                then: AfterAccept::Next(Box::new(request("the delivery"))),
            }
        );
    }

    #[test]
    fn tab_on_a_fix_leaves_the_closing_punctuation_for_the_next_tab() {
        let mut engine = engine_waiting_for("the dlei");
        engine.on_completion(correction(" dlei", " deli", "very, soon"));

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(
            reaction,
            Reaction::Replace {
                deleted: 4,
                text: "delivery".to_owned(),
                then: AfterAccept::Rest(", soon".to_owned()),
            }
        );
    }

    #[test]
    fn text_before_the_caret_holds_the_fixed_word_after_tab() {
        let mut engine = engine_showing_a_fix();
        press(&mut engine, Key::Tab);
        press(&mut engine, Key::Escape);

        let reaction = press(&mut engine, Key::Backspace);

        assert_eq!(reaction, request("the deliver"));
    }

    #[test]
    fn typing_while_a_fix_shows_asks_again() {
        let mut engine = engine_showing_a_fix();

        let reaction = press(&mut engine, Key::Char('v'));

        assert_eq!(reaction, request("the dleiv"));
    }

    #[test]
    fn escape_hides_a_fix() {
        let mut engine = engine_showing_a_fix();

        let reaction = press(&mut engine, Key::Escape);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn fix_with_a_bad_suggestion_after_it_hides() {
        let mut engine = engine_waiting_for("the dlei");

        let reaction = engine.on_completion(correction(" dlei", " deli", "very very very very"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn fix_of_text_that_does_not_end_the_text_before_the_caret_hides() {
        let mut engine = engine_waiting_for("the dlei");

        let reaction = engine.on_completion(correction(" plea", " please", " help"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn fix_with_typo_fixes_off_hides() {
        let mut engine = engine_with(&Settings {
            typo_fix: TypoFix::Off,
            ..Settings::default()
        });
        type_text(&mut engine, "the dlei");

        let reaction = engine.on_completion(correction(" dlei", " deli", "very soon"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn typo_fixes_off_asks_for_no_fixes() {
        let mut engine = engine_with(&Settings {
            typo_fix: TypoFix::Off,
            ..Settings::default()
        });

        let reaction = press(&mut engine, Key::Char('h'));

        assert!(
            matches!(
                reaction,
                Reaction::Request(InferenceRequest::Complete {
                    fixes_typos: false,
                    ..
                })
            ),
            "unexpected {reaction:?}"
        );
    }

    #[test]
    fn completion_stops_at_line_break() {
        let mut engine = engine_waiting_for("hel");

        let reaction = engine.on_completion(completion("lo there\nNext line"));

        assert_eq!(reaction, show("lo there"));
    }

    #[test]
    fn completion_drops_trailing_whitespace() {
        let mut engine = engine_waiting_for("hel");

        let reaction = engine.on_completion(completion("lo  "));

        assert_eq!(reaction, show("lo"));
    }

    #[test]
    fn completion_after_whitespace_drops_leading_whitespace() {
        let mut engine = engine_waiting_for("Hi ");

        let reaction = engine.on_completion(completion("  there"));

        assert_eq!(reaction, show("there"));
    }

    #[test]
    fn completion_after_word_keeps_leading_space() {
        let mut engine = engine_waiting_for("Hi");

        let reaction = engine.on_completion(completion(" there"));

        assert_eq!(reaction, show(" there"));
    }

    #[test]
    fn bad_completion_hides() {
        let cases = [
            ("Best", ","),
            ("Best regards", " …"),
            ("Best ", ", regards"),
            ("I think", " no no no"),
            ("Thanks.", " Hi S. Hi S. Hi S."),
            ("Hi Sarah, thanks for s", "ending. Hi Sarah, thanks for s"),
            ("My new address is", " [Your New Address]."),
            ("Send it to 14 Orchard Lane,", " [City"),
        ];

        let reactions =
            cases.map(|(context, text)| engine_waiting_for(context).on_completion(completion(text)));

        assert_eq!(reactions, cases.map(|_| Reaction::Hide));
    }

    #[test]
    fn completion_stops_before_a_placeholder() {
        let cases = [
            ("Send it to", " 14 Orchard Lane, [City", " 14 Orchard Lane,"),
            ("See you at", " 5 on [day]", " 5 on"),
            ("See [the docs", "] for details", "] for details"),
        ];

        let reactions =
            cases.map(|(context, text, _)| engine_waiting_for(context).on_completion(completion(text)));

        assert_eq!(reactions, cases.map(|(_, _, shown)| show(shown)));
    }

    #[test]
    fn completion_close_to_a_bad_one_shows() {
        let cases = [
            ("Best", ", regards", ", regards"),
            ("I think", " no no", " no no"),
            ("Thanks.", " Hi S. Hi S.", " Hi S. Hi S."),
            (
                "Hi Sarah, thanks for the",
                " thanks for the help",
                " thanks for the help",
            ),
        ];

        let reactions =
            cases.map(|(context, text, _)| engine_waiting_for(context).on_completion(completion(text)));

        assert_eq!(reactions, cases.map(|(_, _, shown)| show(shown)));
    }

    #[test]
    fn completion_that_repeats_the_text_after_the_caret_hides() {
        let cases = ["Update on the budget.", "update", "update on the budget. See you"];

        let reactions = cases.map(|text| {
            let mut engine = Engine::default();
            engine.on_key(
                Key::Char(' '),
                &FakeDesktop::with_text_around("Thanks for the", "update on the budget."),
            );
            engine.on_completion(completion(text))
        });

        assert_eq!(reactions, cases.map(|_| Reaction::Hide));
    }

    #[test]
    fn caret_is_at_the_line_end_while_its_line_has_no_text_after_it() {
        let texts_after = ["", "  \nSee you", " on Friday."];

        let line_ends = texts_after.map(|text_after| {
            let mut engine = Engine::default();
            engine.on_key(
                Key::Char(' '),
                &FakeDesktop::with_text_around("Let's meet", text_after),
            );
            engine.is_caret_at_line_end()
        });

        assert_eq!(line_ends, [true, true, false]);
    }

    #[test]
    fn caret_in_a_field_that_hides_its_line_is_not_at_the_line_end() {
        let desktop = FakeDesktop {
            surroundings: Surroundings {
                is_caret_line_hidden: true,
                ..FakeDesktop::with_field_text("Let's meet").surroundings
            },
            ..FakeDesktop::default()
        };
        let mut engine = Engine::default();

        engine.on_key(Key::Char(' '), &desktop);

        assert!(!engine.is_caret_at_line_end());
    }

    #[test]
    fn completion_that_starts_like_the_text_after_the_caret_shows() {
        let mut engine = Engine::default();
        engine.on_key(
            Key::Char(' '),
            &FakeDesktop::with_text_around("Thanks for the", "update on the budget."),
        );

        let reaction = engine.on_completion(completion("update from Priya"));

        assert_eq!(reaction, show("update from Priya"));
    }

    #[test]
    fn bad_partial_completion_hides() {
        let mut engine = engine_waiting_for("Thanks.");
        engine.on_partial_completion(" Hi S. Hi S. Hi");

        let reaction = engine.on_partial_completion(" Hi S. Hi S. Hi S. Hi");

        assert_eq!(reaction, Some(Reaction::Hide));
    }

    #[test]
    fn bad_completion_hides_the_shown_partial_completion() {
        let mut engine = engine_waiting_for("Thanks.");
        engine.on_partial_completion(" Hi S. Hi S. Hi");

        let reaction = engine.on_completion(completion(" Hi S. Hi S. Hi S."));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn good_partial_completion_after_a_bad_one_shows() {
        let mut engine = Engine::default();
        engine.on_key(
            Key::Char(' '),
            &FakeDesktop::with_text_around("I like it.", "The end."),
        );
        engine.on_partial_completion("The weat");

        let reaction = engine.on_partial_completion("The weather is");

        assert_eq!(reaction, Some(show("The weather")));
    }

    #[test]
    fn partial_completion_shows_its_complete_words() {
        let mut engine = engine_waiting_for("hel");

        let reaction = engine.on_partial_completion("lo big wor");

        assert_eq!(reaction, Some(show("lo big")));
    }

    #[test]
    fn partial_completion_without_a_complete_word_changes_nothing() {
        let cases = ["lo", " big"];

        let reactions = cases.map(|text| engine_waiting_for("hel").on_partial_completion(text));

        assert_eq!(reactions, cases.map(|_| None));
    }

    #[test]
    fn partial_completion_that_adds_no_word_changes_nothing() {
        let mut engine = engine_waiting_for("hel");
        engine.on_partial_completion("lo big wo");

        let reaction = engine.on_partial_completion("lo big wor");

        assert_eq!(reaction, None);
    }

    #[test]
    fn partial_completion_of_a_replaced_request_changes_nothing() {
        let mut engine = engine_waiting_for("he");
        press(&mut engine, Key::Char('l'));

        let reaction = engine.on_partial_completion("llo big world");

        assert_eq!(reaction, None);
    }

    #[test]
    fn partial_completion_after_escape_changes_nothing() {
        let mut engine = engine_waiting_for("hel");
        press(&mut engine, Key::Escape);

        let reaction = engine.on_partial_completion("lo big world");

        assert_eq!(reaction, None);
    }

    #[test]
    fn completion_after_partial_completion_shows_all_its_text() {
        let mut engine = engine_waiting_for("hel");
        engine.on_partial_completion("lo big wor");

        let reaction = engine.on_completion(completion("lo big world"));

        assert_eq!(reaction, show("lo big world"));
    }

    #[test]
    fn failed_completion_keeps_the_shown_partial_completion() {
        let mut engine = engine_waiting_for("hel");
        engine.on_partial_completion("lo big wor");

        let reaction = engine.on_completion(Err(InferenceError::Failed));

        assert_eq!(reaction, show("lo big"));
    }

    #[test]
    fn typed_character_of_a_partial_completion_stays_out_of_later_parts() {
        let mut engine = engine_waiting_for("hel");
        engine.on_partial_completion("lo big wor");
        press(&mut engine, Key::Char('l'));

        let reaction = engine.on_partial_completion("lo big world and");

        assert_eq!(reaction, Some(show("o big world")));
    }

    #[test]
    fn typed_space_after_a_word_stays_out_of_later_parts() {
        let mut engine = engine_waiting_for("Hi");
        engine.on_partial_completion(" there my fri");
        press(&mut engine, Key::Char(' '));

        let reaction = engine.on_completion(completion(" there my friend"));

        assert_eq!(reaction, show("there my friend"));
    }

    #[test]
    fn accepted_word_of_a_partial_completion_stays_out_of_later_parts() {
        let mut engine = engine_waiting_for("hel");
        engine.on_partial_completion("lo big wor");
        press(&mut engine, Key::Tab);

        let reaction = engine.on_partial_completion("lo big world and");

        assert_eq!(reaction, Some(show(" big world")));
    }

    #[test]
    fn partial_completion_after_accepting_all_shown_words_shows_the_next_words() {
        let mut engine = engine_waiting_for("hel");
        engine.on_partial_completion("lo big");
        press(&mut engine, Key::Tab);

        let reaction = engine.on_completion(completion("lo big world"));

        assert_eq!(reaction, show(" big world"));
    }

    #[test]
    fn completion_that_differs_from_the_typed_text_keeps_the_shown_text() {
        let mut engine = engine_waiting_for("hel");
        engine.on_partial_completion("lo big wor");
        press(&mut engine, Key::Char('l'));

        let reaction = engine.on_completion(completion("p me"));

        assert_eq!(reaction, show("o big"));
    }

    #[test]
    fn tab_on_a_partial_completion_accepts_its_first_word() {
        let mut engine = engine_waiting_for("hel");
        engine.on_partial_completion("lo big wor");

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, accept("lo", " big"));
    }

    #[test]
    fn typing_next_suggested_character_shows_rest() {
        let mut engine = engine_showing("hel", "lo world");

        let reaction = press(&mut engine, Key::Char('l'));

        assert_eq!(reaction, show("o world"));
    }

    #[test]
    fn typing_other_character_requests_new_completion() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = press(&mut engine, Key::Char('p'));

        assert_eq!(reaction, request("help"));
    }

    #[test]
    fn typing_whole_suggestion_requests_continuation() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = type_text(&mut engine, "lo");

        assert_eq!(reaction, request("hello"));
    }

    #[test]
    fn tab_accepts_next_word_of_suggestion() {
        let mut engine = engine_showing("hel", "lo big world");

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, accept("lo", " big world"));
    }

    #[test]
    fn tab_types_the_word_before_its_closing_punctuation() {
        let cases = [
            (" regards, Sam", " regards", ", Sam"),
            (" you?! Yes", " you", "?! Yes"),
            (" \"yes.\" Then", " \"yes", ".\" Then"),
            (" (soon). Then", " (soon", "). Then"),
            (" so… Then", " so", "… Then"),
        ];

        let reactions =
            cases.map(|(suggestion, _, _)| press(&mut engine_showing("I see", suggestion), Key::Tab));

        assert_eq!(reactions, cases.map(|(_, word, rest)| accept(word, rest)));
    }

    #[test]
    fn tab_after_a_word_types_its_closing_punctuation() {
        let mut engine = engine_showing("Best", " regards, Sam");
        press(&mut engine, Key::Tab);

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, accept(",", " Sam"));
    }

    #[test]
    fn tab_types_a_word_with_signs_inside_it_whole() {
        let cases = [" don't know", " 3.5% more", " e-mail me", " U.S. law"];

        let reactions = cases.map(|suggestion| press(&mut engine_showing("I see", suggestion), Key::Tab));

        assert_eq!(
            reactions,
            [
                accept(" don't", " know"),
                accept(" 3.5%", " more"),
                accept(" e-mail", " me"),
                accept(" U.S", ". law"),
            ]
        );
    }

    #[test]
    fn tab_types_a_part_of_only_punctuation_whole() {
        let cases = [(" ... maybe", " ...", " maybe"), (", see you", ",", " see you")];

        let reactions =
            cases.map(|(suggestion, _, _)| press(&mut engine_showing("I see", suggestion), Key::Tab));

        assert_eq!(reactions, cases.map(|(_, part, rest)| accept(part, rest)));
    }

    #[test]
    fn tab_passes_to_the_app_while_the_ghost_text_is_not_on_the_screen() {
        let mut engine = engine_showing("hel", "lo big world");
        let desktop = FakeDesktop {
            is_ghost_hidden: true,
            ..FakeDesktop::default()
        };

        let reaction = engine.on_key(Key::Tab, &desktop);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn tab_accepts_word_with_space_before_it() {
        let mut engine = engine_showing("hello", " big world");

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, accept(" big", " world"));
    }

    #[test]
    fn tab_after_accepted_word_accepts_following_word() {
        let mut engine = engine_showing("hel", "lo big world");
        press(&mut engine, Key::Tab);

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, accept(" big", " world"));
    }

    #[test]
    fn tab_on_the_last_part_requests_the_next_suggestion() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, accept_last("lo", request("hello")));
    }

    #[test]
    fn tab_on_the_last_shown_part_of_a_running_completion_waits_for_its_next_words() {
        let mut engine = engine_waiting_for("hel");
        engine.on_partial_completion("lo big");

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, accept_last("lo", Reaction::Hide));
    }

    #[test]
    fn typing_after_accepted_word_follows_rest() {
        let mut engine = engine_showing("hel", "lo big world");
        press(&mut engine, Key::Tab);

        let reaction = press(&mut engine, Key::Char(' '));

        assert_eq!(reaction, show("big world"));
    }

    #[test]
    fn tab_without_suggestion_passes_through() {
        let mut engine = engine_waiting_for("hel");

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn accepted_word_becomes_context() {
        let mut engine = engine_showing("hel", "lo big world");
        press(&mut engine, Key::Tab);

        let reaction = press(&mut engine, Key::Char('!'));

        assert_eq!(reaction, request("hello!"));
    }

    #[test]
    fn suggestion_is_offered_once_after_accept() {
        let mut engine = engine_showing("hel", "lo");
        press(&mut engine, Key::Tab);

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn backspace_requests_completion_of_shorter_context() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = press(&mut engine, Key::Backspace);

        assert_eq!(reaction, request("he"));
    }

    #[test]
    fn backspace_on_empty_context_hides() {
        let mut engine = Engine::default();

        let reaction = press(&mut engine, Key::Backspace);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn escape_hides_suggestion() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = press(&mut engine, Key::Escape);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn tab_after_escape_passes_through() {
        let mut engine = engine_showing("hel", "lo");
        press(&mut engine, Key::Escape);

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn escape_while_waiting_drops_completion() {
        let mut engine = engine_waiting_for("hel");
        press(&mut engine, Key::Escape);

        let reaction = engine.on_completion(completion("lo"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn typing_after_escape_while_waiting_requests_again() {
        let mut engine = engine_waiting_for("hel");
        press(&mut engine, Key::Escape);

        let reaction = press(&mut engine, Key::Char('p'));

        assert_eq!(reaction, request("help"));
    }

    #[test]
    fn other_key_clears_context() {
        let mut engine = engine_showing("he", "llo");
        press(&mut engine, Key::Other);

        let reaction = press(&mut engine, Key::Char('l'));

        assert_eq!(reaction, request("l"));
    }

    #[test]
    fn control_character_clears_context() {
        let mut engine = engine_showing("he", "llo");
        press(&mut engine, Key::Char('\r'));

        let reaction = press(&mut engine, Key::Char('l'));

        assert_eq!(reaction, request("l"));
    }

    /// A field with `before_caret` and a recent copy, which a new session puts into its preface.
    fn field_with_copy(before_caret: &str) -> FakeDesktop {
        FakeDesktop {
            copied: Some(CopiedText {
                text: "Sara Novak".to_owned(),
                age: Some(Duration::from_secs(1)),
            }),
            ..FakeDesktop::with_field_text(before_caret)
        }
    }

    fn engine_after_enter(typed: &str) -> Engine {
        engine_after_enter_in(typed, typed)
    }

    fn engine_after_enter_in(typed: &str, field_at_enter: &str) -> Engine {
        let mut engine = engine_with(&Settings {
            clipboard_access: ClipboardAccess::Allowed,
            ..Settings::default()
        });
        engine.prepare(&FakeDesktop::with_field_text(typed));
        engine.on_completion(completion(""));
        engine.on_key(Key::Enter, &FakeDesktop::with_field_text(field_at_enter));
        engine
    }

    #[test]
    fn new_line_continues_the_field_text_when_the_app_changed_the_typed_text() {
        let mut engine = engine_after_enter_in("¿Que tal?", "¿Qué tal?");

        let reaction = engine.suggest_on_new_line(&field_with_copy("¿Qué tal?\n"));

        assert_eq!(reaction, Some(request("¿Qué tal?\n")));
    }

    #[test]
    fn new_line_continues_the_typed_text_when_the_field_read_at_enter_is_old() {
        let mut engine = engine_after_enter_in("Best regards,", "Best regards");

        let reaction = engine.suggest_on_new_line(&field_with_copy("Best regards,\n"));

        assert_eq!(reaction, Some(request("Best regards,\n")));
    }

    #[test]
    fn dead_key_keeps_the_session_for_the_accented_letter() {
        let mut engine = Engine::default();
        engine.on_key(Key::Char('u'), &FakeDesktop::with_field_text("¿Q"));
        engine.on_key(Key::Dead, &FakeDesktop::with_field_text("¿Qu"));

        let reaction = engine.on_key(Key::Char('e'), &FakeDesktop::with_field_text("¿Qu´"));

        assert_eq!(reaction, request("¿Que"));
    }

    #[test]
    fn dead_key_hides_the_suggestion() {
        let mut engine = engine_showing("qu", "é tal");

        let reaction = press(&mut engine, Key::Dead);

        assert_eq!(
            (reaction, engine.shown(), engine.has_session()),
            (Reaction::Hide, "", true)
        );
    }

    #[test]
    fn enter_hides_the_suggestion() {
        let mut engine = engine_showing("he", "llo");

        let reaction = press(&mut engine, Key::Enter);

        assert_eq!((reaction, engine.shown()), (Reaction::Hide, ""));
    }

    #[test]
    fn completion_of_a_request_before_enter_shows_nothing() {
        let mut engine = Engine::default();
        let desktop = FakeDesktop::with_field_text("Best regards");
        engine.on_key(Key::Char(','), &desktop);
        engine.on_key(Key::Enter, &desktop);

        let reaction = engine.on_completion(completion(" Sara"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn settled_input_after_enter_requests_the_new_line_with_the_same_preface() {
        let mut engine = engine_after_enter("Best regards,");

        let reaction = engine.suggest_on_new_line(&field_with_copy("Best regards,\n"));

        assert_eq!(reaction, Some(request("Best regards,\n")));
    }

    #[test]
    fn key_after_enter_continues_the_session_on_the_new_line() {
        let mut engine = engine_after_enter("Best regards,");

        let reaction = engine.on_key(Key::Char('S'), &field_with_copy("Best regards,\n"));

        assert_eq!(reaction, request("Best regards,\nS"));
    }

    #[test]
    fn new_line_keeps_what_the_app_adds_after_the_line_break() {
        let cases = [
            "- milk\n- ",
            "- milk\n\n",
            "- milk\r\n",
            "- milk\n    ",
            "- milk\n2. ",
        ];

        let reactions =
            cases.map(|field| engine_after_enter("- milk").suggest_on_new_line(&field_with_copy(field)));

        assert_eq!(reactions, cases.map(|field| Some(request(field))));
    }

    #[test]
    fn second_enter_waits_for_the_same_read() {
        let mut engine = engine_after_enter("Hi Tom,");
        press(&mut engine, Key::Enter);

        let reaction = engine.suggest_on_new_line(&field_with_copy("Hi Tom,\n\n"));

        assert_eq!(reaction, Some(request("Hi Tom,\n\n")));
    }

    #[test]
    fn session_ends_when_enter_starts_no_new_line() {
        let cases = [
            ("See you at 8", ""),
            ("weather in lisbon", "weather in lisbon"),
            ("明日の会議はとうきょう", "明日の会議は東京"),
            ("$ ls", "$ ls\nnotes.txt\n$ "),
            ("Dear Ana,", "Dear Ana,\nSara"),
            ("Thanks", "Thanks, "),
        ];

        let outcomes = cases.map(|(typed, field)| {
            let mut engine = engine_after_enter(typed);
            let reaction = engine.suggest_on_new_line(&field_with_copy(field));
            (reaction, engine.has_session())
        });

        assert_eq!(outcomes, cases.map(|_| (None, false)));
    }

    #[test]
    fn key_after_enter_without_a_new_line_starts_a_new_session() {
        let mut engine = engine_after_enter("See you at 8");

        let reaction = engine.on_key(Key::Char('O'), &field_with_copy(""));

        assert_eq!(
            reaction,
            request_with(&["Text on my clipboard: “Sara Novak”"], "O")
        );
    }

    #[test]
    fn settled_input_without_enter_suggests_nothing() {
        let mut engine = engine_waiting_for("he");

        let reaction = engine.suggest_on_new_line(&FakeDesktop::with_field_text("he\n"));

        assert_eq!(reaction, None);
    }

    #[test]
    fn enter_ends_a_session_whose_field_the_platform_cannot_read() {
        let mut engine = engine_waiting_for("he");

        press(&mut engine, Key::Enter);

        assert!(!engine.has_session());
    }

    #[test]
    fn enter_ends_a_session_in_a_subject_field() {
        let mut engine = Engine::default();
        let desktop = field_of(Some("com.apple.mail"), Some("Subject"), "Lunch");
        engine.on_key(Key::Char('?'), &desktop);

        engine.on_key(Key::Enter, &desktop);

        assert!(!engine.has_session());
    }

    #[test]
    fn new_line_after_a_trimmed_text_continues_its_end() {
        let mut engine = Engine::default();
        let desktop = FakeDesktop::with_field_text(&format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 1)));
        engine.on_key(Key::Char('y'), &desktop);
        engine.on_completion(completion(""));
        engine.on_key(Key::Enter, &desktop);
        let field = format!("{}\n", "y".repeat(MAX_CHARS_BEFORE_CARET - 1));

        let reaction = engine.suggest_on_new_line(&FakeDesktop::with_field_text(&field));

        assert_eq!(
            reaction,
            Some(request(&format!(
                "{}\n",
                "y".repeat(TRIMMED_CHARS_BEFORE_CARET + 1)
            )))
        );
    }

    #[test]
    fn reset_hides_and_clears_context() {
        let mut engine = engine_showing("he", "llo");

        let reset_reaction = engine.reset();
        let typed_reaction = press(&mut engine, Key::Char('l'));

        assert_eq!((reset_reaction, typed_reaction), (Reaction::Hide, request("l")));
    }

    #[test]
    fn completion_after_reset_is_dropped() {
        let mut engine = engine_waiting_for("hel");
        engine.reset();

        let reaction = engine.on_completion(completion("lo"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn typing_after_reset_while_waiting_requests_the_new_context() {
        let mut engine = engine_waiting_for("hel");
        engine.reset();

        let reaction = press(&mut engine, Key::Char('x'));

        assert_eq!(reaction, request("x"));
    }

    #[test]
    fn completion_from_before_a_reset_is_dropped_after_a_new_request() {
        let mut engine = engine_waiting_for("hel");
        engine.reset();
        press(&mut engine, Key::Char('x'));

        let reaction = engine.on_completion(completion("lo"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn context_that_reaches_the_limit_drops_old_characters_in_one_step() {
        let mut engine = engine_waiting_for(&format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 2)));
        engine.on_completion(completion(""));

        let reaction = press(&mut engine, Key::Char('y'));

        assert_eq!(reaction, request(&"y".repeat(TRIMMED_CHARS_BEFORE_CARET)));
    }

    #[test]
    fn context_below_the_limit_keeps_its_start() {
        let mut engine = engine_waiting_for(&format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 3)));
        engine.on_completion(completion(""));

        let reaction = press(&mut engine, Key::Char('y'));

        assert_eq!(
            reaction,
            request(&format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 2)))
        );
    }

    #[test]
    fn first_key_continues_the_field_text_before_the_caret() {
        let mut engine = Engine::default();

        let reaction = engine.on_key(Key::Char('l'), &FakeDesktop::with_field_text("Hello wor"));

        assert_eq!(reaction, request("Hello worl"));
    }

    #[test]
    fn field_text_is_read_once_per_session() {
        let mut engine = Engine::default();
        let desktop = FakeDesktop::with_field_text("Hello ");
        engine.on_key(Key::Char('w'), &desktop);
        engine.on_completion(completion(""));

        let reaction = engine.on_key(Key::Char('o'), &desktop);

        assert_eq!(
            (reaction, desktop.surroundings_reads.get()),
            (request("Hello wo"), 1)
        );
    }

    #[test]
    fn reset_reads_the_field_again() {
        let mut engine = Engine::default();
        engine.on_key(Key::Char('x'), &FakeDesktop::with_field_text("Old "));
        engine.on_completion(completion(""));
        engine.reset();

        let reaction = engine.on_key(Key::Char('d'), &FakeDesktop::with_field_text("New wor"));

        assert_eq!(reaction, request("New word"));
    }

    #[test]
    fn backspace_deletes_from_the_field_text() {
        let mut engine = Engine::default();

        let reaction = engine.on_key(Key::Backspace, &FakeDesktop::with_field_text("Helloo"));

        assert_eq!(reaction, request("Hello"));
    }

    #[test]
    fn long_field_text_keeps_its_end() {
        let mut engine = Engine::default();
        let before_caret = format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 1));

        let reaction = engine.on_key(Key::Char('y'), &FakeDesktop::with_field_text(&before_caret));

        assert_eq!(reaction, request(&"y".repeat(TRIMMED_CHARS_BEFORE_CARET + 1)));
    }

    #[test]
    fn writer_notes_come_after_the_frame() {
        let mut engine = engine_with(&Settings {
            writer_notes: WriterNotes("I am a nurse.".to_owned()),
            ..Settings::default()
        });

        let reaction = press(&mut engine, Key::Char('h'));

        assert_eq!(
            reaction,
            Reaction::Request(InferenceRequest::Complete {
                prompt: prompt::prompt_with_notes("I am a nurse.", &[], "h"),
                max_tokens: Settings::default().suggestion_length_in_use().max_tokens(),
                word_bias: None,
                next_word_bias: NextWordBias::default(),
                min_log_probability: Some(Settings::default().suggestion_length_in_use().tail_cut()),
                min_typed_log_probability: Some(TYPO_CUT),
                fixes_typos: true,
            })
        );
    }

    #[test]
    fn writer_name_comes_after_the_notes() {
        let mut engine = engine_with(&Settings {
            writer_name: WriterName("Sara Novak".to_owned()),
            ..Settings::default()
        });

        let reaction = press(&mut engine, Key::Char('h'));

        assert_eq!(reaction, request_with(&["My name is Sara Novak."], "h"));
    }

    fn history_settings(history: HistoryAccess, apps: Vec<HistoryApp>) -> Settings {
        Settings {
            personalization: PersonalizationSettings {
                history,
                apps,
                word_choice: WordChoice::Off,
                ..PersonalizationSettings::default()
            },
            ..Settings::default()
        }
    }

    fn mail_entry(text: &str) -> HistoryEntry {
        HistoryEntry {
            text: text.to_owned(),
            app_id: "com.apple.mail".to_owned(),
            domain: None,
        }
    }

    fn mail_app(scope: HistoryScope) -> HistoryApp {
        HistoryApp {
            app_id: "com.apple.mail".to_owned(),
            app_name: "Mail".to_owned(),
            app_path: None,
            scope,
        }
    }

    fn engine_with_history(settings: &Settings) -> Engine {
        let mut engine = engine_with(settings);
        engine.use_history(vec![mail_entry("Cheers,\nTom")]);
        engine
    }

    fn mail_field() -> FakeDesktop {
        FakeDesktop {
            surroundings: Surroundings {
                app_id: Some("com.apple.mail".to_owned()),
                ..Surroundings::default()
            },
            ..FakeDesktop::default()
        }
    }

    #[test]
    fn earlier_writing_of_the_app_comes_before_the_text() {
        let mut engine = engine_with_history(&history_settings(HistoryAccess::On, Vec::new()));

        let reaction = engine.on_key(Key::Char('h'), &mail_field());

        assert_eq!(
            reaction,
            request_with(
                &[
                    "Where I am typing: Format: email",
                    "My earlier writing:\n“Cheers,\nTom”"
                ],
                "h"
            )
        );
    }

    #[test]
    fn earlier_writing_stays_out_when_the_history_or_the_app_is_off() {
        let settings = [
            history_settings(HistoryAccess::Off, Vec::new()),
            history_settings(HistoryAccess::On, vec![mail_app(HistoryScope::Off)]),
        ];

        let reactions =
            settings.map(|settings| engine_with_history(&settings).on_key(Key::Char('h'), &mail_field()));

        assert_eq!(
            reactions,
            [
                request_with(&["Where I am typing: Format: email"], "h"),
                request_with(&["Where I am typing: Format: email"], "h")
            ]
        );
    }

    const GREETING: &str = "Hi Ana,\n\n";
    const TYPED_TEXT: &str = "The report is ready, and the numbers look goo";
    const EARLIER_TEXT: &str = "Hi Ana,\n\nThe report is ready, and the numbers look goo";

    fn field_of(app_id: Option<&str>, field_name: Option<&str>, before_caret: &str) -> FakeDesktop {
        FakeDesktop {
            surroundings: Surroundings {
                app_id: app_id.map(str::to_owned),
                field_labels: FieldLabels {
                    name: field_name.map(str::to_owned),
                    ..FieldLabels::default()
                },
                field_text: Some(FieldText {
                    before_caret: before_caret.to_owned(),
                    after_caret: String::new(),
                }),
                ..Surroundings::default()
            },
            ..FakeDesktop::default()
        }
    }

    /// Types `typed` and the key `d` in `desktop`, accepts a suggestion when `accepts` and ends
    /// the session. Returns the history to save.
    fn finish_typing_text(
        engine: &mut Engine,
        desktop: &FakeDesktop,
        typed: &str,
        accepts: bool,
    ) -> Option<Vec<HistoryEntry>> {
        for character in typed.chars() {
            if let Reaction::Request(_) = engine.on_key(Key::Char(character), desktop) {
                engine.on_completion(completion(""));
            }
        }
        engine.on_key(Key::Char('d'), desktop);
        engine.on_completion(completion(". Best, Tom"));
        if accepts {
            engine.on_key(Key::Tab, desktop);
        }
        engine.on_key(Key::Other, desktop);
        engine.history_to_save().map(<[HistoryEntry]>::to_vec)
    }

    fn finish_typing(engine: &mut Engine, desktop: &FakeDesktop, accepts: bool) -> Option<Vec<HistoryEntry>> {
        finish_typing_text(engine, desktop, TYPED_TEXT, accepts)
    }

    fn kept_settings(kept_texts: KeptTexts) -> Settings {
        let mut settings = history_settings(HistoryAccess::On, Vec::new());
        settings.personalization.kept_texts = kept_texts;
        settings
    }

    #[test]
    fn finished_text_with_an_accepted_suggestion_is_kept() {
        let mut engine = engine_with(&kept_settings(KeptTexts::AcceptedOnly));

        let saved = finish_typing(
            &mut engine,
            &field_of(Some("com.apple.mail"), None, GREETING),
            true,
        );

        assert_eq!(saved, Some(vec![mail_entry(&format!("{EARLIER_TEXT}d."))]));
    }

    #[test]
    fn finished_text_without_an_accepted_suggestion_is_kept_only_when_all_texts_are_kept() {
        let settings = [KeptTexts::AcceptedOnly, KeptTexts::All].map(kept_settings);

        let saved = settings.map(|settings| {
            finish_typing(
                &mut engine_with(&settings),
                &field_of(Some("com.apple.mail"), None, GREETING),
                false,
            )
        });

        assert_eq!(saved, [None, Some(vec![mail_entry(&format!("{EARLIER_TEXT}d"))])]);
    }

    #[test]
    fn finished_text_is_not_kept_where_the_rules_forbid_it() {
        let cases = [
            (
                history_settings(HistoryAccess::Off, Vec::new()),
                field_of(Some("com.apple.mail"), None, GREETING),
            ),
            (
                history_settings(HistoryAccess::On, vec![mail_app(HistoryScope::Off)]),
                field_of(Some("com.apple.mail"), None, GREETING),
            ),
            (
                kept_settings(KeptTexts::All),
                field_of(Some("com.apple.Terminal"), None, GREETING),
            ),
            (kept_settings(KeptTexts::All), field_of(None, None, GREETING)),
            (
                kept_settings(KeptTexts::All),
                field_of(Some("com.apple.mail"), Some("Subject"), GREETING),
            ),
        ];

        let saved =
            cases.map(|(settings, desktop)| finish_typing(&mut engine_with(&settings), &desktop, true));

        assert_eq!(saved, [None, None, None, None, None]);
    }

    fn long_field() -> String {
        "The terms of the lease are in force. ".repeat(6)
    }

    fn mail_desktop(before_caret: &str) -> FakeDesktop {
        field_of(Some("com.apple.mail"), None, before_caret)
    }

    #[test]
    fn short_finished_text_is_not_kept() {
        let mut engine = engine_with(&kept_settings(KeptTexts::All));

        let saved = finish_typing_text(&mut engine, &mail_desktop(GREETING), "Thanks", true);

        assert_eq!(saved, None);
    }

    #[test]
    fn text_that_the_field_held_before_the_session_is_not_kept() {
        let mut engine = engine_with(&kept_settings(KeptTexts::All));

        let saved = finish_typing_text(&mut engine, &mail_desktop(&long_field()), TYPED_TEXT, false);

        assert_eq!(saved, Some(vec![mail_entry(&format!("{TYPED_TEXT}d"))]));
    }

    #[test]
    fn a_few_words_that_the_user_adds_to_a_long_field_are_not_kept() {
        let mut engine = engine_with(&kept_settings(KeptTexts::All));

        let saved = finish_typing_text(&mut engine, &mail_desktop(&long_field()), "Thanks", false);

        assert_eq!(saved, None);
    }

    #[test]
    fn draft_that_an_entry_holds_is_kept_with_the_new_text_in_one_entry() {
        let mut engine = engine_with(&kept_settings(KeptTexts::All));
        engine.use_history(vec![mail_entry(long_field().trim())]);

        let saved = finish_typing_text(&mut engine, &mail_desktop(&long_field()), TYPED_TEXT, false);

        assert_eq!(
            saved,
            Some(vec![mail_entry(&format!("{}{TYPED_TEXT}d", long_field()))])
        );
    }

    #[test]
    fn backspace_into_the_text_of_the_field_makes_it_count_less() {
        let mut engine = engine_with(&kept_settings(KeptTexts::All));
        let desktop = mail_desktop(&long_field());
        for _ in 0..190 {
            engine.on_key(Key::Backspace, &desktop);
        }

        let saved = finish_typing_text(&mut engine, &desktop, TYPED_TEXT, false);

        assert_eq!(
            saved,
            Some(vec![mail_entry(&format!("{}{TYPED_TEXT}d", &long_field()[..32]))])
        );
    }

    #[test]
    fn cut_of_the_start_for_the_token_limit_makes_the_text_of_the_field_count_less() {
        let mut engine = engine_with(&kept_settings(KeptTexts::All));
        let desktop = mail_desktop(&"word ".repeat(798));

        let saved = finish_typing_text(&mut engine, &desktop, TYPED_TEXT, false);

        assert_eq!(saved, Some(vec![mail_entry(&format!("{TYPED_TEXT}d"))]));
    }

    #[test]
    fn pasted_text_is_not_kept() {
        let mut engine = engine_with(&kept_settings(KeptTexts::All));
        finish_typing(&mut engine, &mail_desktop(GREETING), false);
        let pasted = "Pasted line from a web page that the user did not write. ".repeat(3);
        let desktop = mail_desktop(&format!("{EARLIER_TEXT}d{pasted}"));

        let saved = finish_typing_text(&mut engine, &desktop, "", false);

        assert_eq!(saved, None);
    }

    #[test]
    fn session_with_two_lines_is_kept_in_one_entry() {
        let mut engine = engine_with(&kept_settings(KeptTexts::All));
        let first_line = "Hi Ana,";
        engine.prepare(&mail_desktop(""));
        type_text(&mut engine, first_line);
        engine.on_key(Key::Enter, &mail_desktop(""));
        let new_line = mail_desktop(&format!("{first_line}\n"));
        engine.suggest_on_new_line(&new_line);

        let saved = finish_typing_text(&mut engine, &new_line, TYPED_TEXT, false);

        assert_eq!(
            saved,
            Some(vec![mail_entry(&format!("{first_line}\n{TYPED_TEXT}d"))])
        );
    }

    fn word_choice_settings(history: HistoryAccess, apps: Vec<HistoryApp>) -> Settings {
        let mut settings = history_settings(history, apps);
        settings.personalization.word_choice = WordChoice::Medium;
        settings
    }

    fn word_bias_of_request(reaction: Reaction) -> Option<Vec<(String, f32)>> {
        match reaction {
            Reaction::Request(InferenceRequest::Complete { word_bias, .. }) => {
                word_bias.map(|bias| bias.words.to_vec())
            }
            _ => None,
        }
    }

    fn engine_with_words(settings: &Settings) -> Engine {
        let mut engine = engine_with(settings);
        engine.use_history(vec![mail_entry(&"reckon ".repeat(20))]);
        engine
    }

    #[test]
    fn request_prefers_the_words_of_the_history() {
        let mut engine = engine_with_words(&word_choice_settings(HistoryAccess::On, Vec::new()));

        let reaction = engine.on_key(Key::Char('h'), &mail_field());

        assert_eq!(
            word_bias_of_request(reaction),
            Some(vec![("reckon".to_owned(), 1.0)])
        );
    }

    #[test]
    fn request_has_no_word_bias_when_the_history_or_the_app_is_off() {
        let settings = [
            word_choice_settings(HistoryAccess::Off, Vec::new()),
            word_choice_settings(HistoryAccess::On, vec![mail_app(HistoryScope::Off)]),
        ];

        let biases = settings.map(|settings| {
            word_bias_of_request(engine_with_words(&settings).on_key(Key::Char('h'), &mail_field()))
        });

        assert_eq!(biases, [None, None]);
    }

    #[test]
    fn new_word_choice_or_new_history_gives_a_new_version() {
        let mut engine = engine_with_words(&word_choice_settings(HistoryAccess::On, Vec::new()));
        let first = engine
            .tables
            .word_bias(&HistoryPool::Shared)
            .map(|bias| bias.version);
        engine.use_settings(&{
            let mut settings = word_choice_settings(HistoryAccess::On, Vec::new());
            settings.personalization.word_choice = WordChoice::Strong;
            settings
        });
        let second = engine
            .tables
            .word_bias(&HistoryPool::Shared)
            .map(|bias| bias.version);
        finish_typing(
            &mut engine,
            &field_of(Some("com.apple.mail"), None, GREETING),
            true,
        );

        let third = engine
            .tables
            .word_bias(&HistoryPool::Shared)
            .map(|bias| bias.version);

        assert!(first < second && second < third, "{first:?} {second:?} {third:?}");
    }

    fn engine_with_phrases(settings: &Settings) -> Engine {
        let mut engine = engine_with(settings);
        engine.use_history(vec![mail_entry("Let me know if it works."); 3]);
        engine
    }

    fn mail_field_around(before_caret: &str, after_caret: &str) -> FakeDesktop {
        let mut desktop = field_of(Some("com.apple.mail"), None, before_caret);
        desktop.surroundings.field_text = Some(FieldText {
            before_caret: before_caret.to_owned(),
            after_caret: after_caret.to_owned(),
        });
        desktop
    }

    #[test]
    fn phrase_that_the_writer_repeats_shows_without_a_request() {
        let mut engine = engine_with_phrases(&word_choice_settings(HistoryAccess::On, Vec::new()));

        let reaction = engine.on_key(Key::Char(' '), &mail_field_around("Please let me know", ""));

        assert_eq!(reaction, show("if"));
    }

    #[test]
    fn phrase_shows_only_when_the_line_after_the_caret_is_empty() {
        let texts_after = ["", "\nBye", " today"];

        let shows = texts_after.map(|text_after| {
            let mut engine = engine_with_phrases(&word_choice_settings(HistoryAccess::On, Vec::new()));
            engine.on_key(
                Key::Char(' '),
                &mail_field_around("Please let me know", text_after),
            ) == show("if")
        });

        assert_eq!(shows, [true, true, false]);
    }

    #[test]
    fn phrase_needs_the_word_choice_the_history_and_the_app() {
        let settings = [
            history_settings(HistoryAccess::On, Vec::new()),
            word_choice_settings(HistoryAccess::Off, Vec::new()),
            word_choice_settings(HistoryAccess::On, vec![mail_app(HistoryScope::Off)]),
        ];

        let requests = settings.map(|settings| {
            let reaction = engine_with_phrases(&settings)
                .on_key(Key::Char(' '), &mail_field_around("Please let me know", ""));
            matches!(reaction, Reaction::Request(_))
        });

        assert_eq!(requests, [true, true, true]);
    }

    fn next_word_bias_after(settings: &Settings, before_caret: &str) -> Vec<(String, f32)> {
        let mut engine = engine_with(settings);
        engine.use_history(vec![
            mail_entry("Let me know if it works."),
            mail_entry("Let me know when you can."),
        ]);
        match engine.on_key(Key::Char(' '), &mail_field_around(before_caret, "")) {
            Reaction::Request(InferenceRequest::Complete { next_word_bias, .. }) => next_word_bias.0,
            _ => Vec::new(),
        }
    }

    #[test]
    fn request_prefers_the_words_that_the_writer_wrote_after_the_last_words() {
        let settings = word_choice_settings(HistoryAccess::On, Vec::new());

        let next = next_word_bias_after(&settings, "Please let me know");

        assert_eq!(next, [("if".to_owned(), 0.5), ("when".to_owned(), 0.5)]);
    }

    #[test]
    fn request_has_no_next_word_bias_without_the_word_choice_the_history_or_the_app() {
        let settings = [
            history_settings(HistoryAccess::On, Vec::new()),
            word_choice_settings(HistoryAccess::Off, Vec::new()),
            word_choice_settings(HistoryAccess::On, vec![mail_app(HistoryScope::Off)]),
        ];

        let next = settings.map(|settings| next_word_bias_after(&settings, "Please let me know"));

        assert_eq!(next, [vec![], vec![], vec![]]);
    }

    #[test]
    fn completion_of_an_older_request_keeps_the_phrase() {
        let mut engine = engine_with_phrases(&word_choice_settings(HistoryAccess::On, Vec::new()));
        engine.on_key(Key::Char('n'), &mail_field_around("Please let me k", ""));
        type_text(&mut engine, "ow");

        let reaction = engine.on_completion(Err(InferenceError::Superseded));

        assert_eq!(reaction, show(" if"));
    }

    const CHAT_APP: &str = "net.whatsapp.WhatsApp";
    const OTHER_CHAT_APP: &str = "com.tinyspeck.slackmacgap";

    fn app_with(app_id: &str, scope: HistoryScope) -> HistoryApp {
        HistoryApp {
            app_id: app_id.to_owned(),
            app_name: app_id.to_owned(),
            app_path: None,
            scope,
        }
    }

    fn chat_entry(text: &str) -> HistoryEntry {
        HistoryEntry {
            app_id: CHAT_APP.to_owned(),
            ..mail_entry(text)
        }
    }

    /// The settings of each case of the pools: the chat app, then the other chat app.
    fn pool_cases() -> [(HistoryScope, HistoryScope); 3] {
        [
            (HistoryScope::AllApps, HistoryScope::AllApps),
            (HistoryScope::ThisApp, HistoryScope::AllApps),
            (HistoryScope::AllApps, HistoryScope::ThisApp),
        ]
    }

    fn pool_settings((chat, other_chat): (HistoryScope, HistoryScope)) -> Settings {
        word_choice_settings(
            HistoryAccess::On,
            vec![app_with(CHAT_APP, chat), app_with(OTHER_CHAT_APP, other_chat)],
        )
    }

    #[test]
    fn only_apps_with_all_apps_give_examples_to_each_other() {
        let shared = pool_cases().map(|scopes| {
            let mut engine = engine_with(&pool_settings(scopes));
            engine.use_history(vec![chat_entry("See you at the station at six")]);
            let reaction = engine.on_key(Key::Char('h'), &field_of(Some(OTHER_CHAT_APP), None, ""));
            matches!(reaction, Reaction::Request(InferenceRequest::Complete { prompt, .. })
                if prompt.0.contains("See you at the station"))
        });

        assert_eq!(shared, [true, false, false]);
    }

    #[test]
    fn only_apps_with_all_apps_give_phrases_to_each_other() {
        let shown = pool_cases().map(|scopes| {
            let mut engine = engine_with(&pool_settings(scopes));
            engine.use_history(vec![chat_entry("Let me know if it works."); 3]);
            let desktop = field_of(Some(OTHER_CHAT_APP), None, "Please let me know");
            engine.on_key(Key::Char(' '), &desktop) == show("if")
        });

        assert_eq!(shown, [true, false, false]);
    }

    #[test]
    fn only_apps_with_all_apps_give_next_words_and_word_bias_to_each_other() {
        let biases = pool_cases().map(|scopes| {
            let mut engine = engine_with(&pool_settings(scopes));
            engine.use_history(vec![
                chat_entry("Let me know if it works. Reckon reckon reckon."),
                chat_entry("Let me know when you can."),
            ]);
            let desktop = field_of(Some(OTHER_CHAT_APP), None, "Please let me know");
            match engine.on_key(Key::Char(' '), &desktop) {
                Reaction::Request(InferenceRequest::Complete {
                    word_bias,
                    next_word_bias,
                    ..
                }) => (word_bias.is_some(), !next_word_bias.0.is_empty()),
                _ => (false, false),
            }
        });

        assert_eq!(biases, [(true, true), (false, false), (false, false)]);
    }

    #[test]
    fn app_with_only_this_app_uses_its_own_texts() {
        let mut engine = engine_with(&pool_settings((HistoryScope::ThisApp, HistoryScope::AllApps)));
        engine.use_history(vec![chat_entry("Let me know if it works."); 3]);

        let reaction = engine.on_key(
            Key::Char(' '),
            &field_of(Some(CHAT_APP), None, "Please let me know"),
        );

        assert_eq!(reaction, show("if"));
    }

    #[test]
    fn new_scope_of_an_app_builds_the_tables_again() {
        let mut engine = engine_with(&pool_settings((HistoryScope::AllApps, HistoryScope::AllApps)));
        engine.use_history(vec![chat_entry("Let me know if it works."); 3]);
        engine.use_settings(&pool_settings((HistoryScope::ThisApp, HistoryScope::AllApps)));

        let reaction = engine.on_key(
            Key::Char(' '),
            &field_of(Some(OTHER_CHAT_APP), None, "Please let me know"),
        );

        assert!(matches!(reaction, Reaction::Request(_)), "{reaction:?}");
    }

    #[test]
    fn history_to_save_comes_once_for_each_change() {
        let mut engine = engine_with(&kept_settings(KeptTexts::All));
        finish_typing(
            &mut engine,
            &field_of(Some("com.apple.mail"), None, GREETING),
            false,
        );

        let saved_again = engine.history_to_save().is_some();

        assert!(!saved_again);
    }

    #[test]
    fn forgetting_an_app_drops_its_texts() {
        let mut engine = engine_with(&kept_settings(KeptTexts::All));
        engine.use_history(vec![
            mail_entry("Cheers,\nTom"),
            HistoryEntry {
                app_id: "com.apple.Notes".to_owned(),
                ..mail_entry("Shopping list")
            },
        ]);

        engine.forget_history_of("com.apple.mail");

        assert_eq!(
            engine.history_to_save().map(<[HistoryEntry]>::to_vec),
            Some(vec![HistoryEntry {
                app_id: "com.apple.Notes".to_owned(),
                ..mail_entry("Shopping list")
            }])
        );
    }

    #[test]
    fn recent_copy_comes_before_the_text() {
        let mut engine = engine_with(&Settings {
            clipboard_access: ClipboardAccess::Allowed,
            ..Settings::default()
        });
        let desktop = FakeDesktop::with_copy(
            "1Z999",
            Some(MAX_CLIPBOARD_AGE.saturating_sub(Duration::from_secs(1))),
        );

        let reaction = engine.on_key(Key::Char('h'), &desktop);

        assert_eq!(reaction, request_with(&["Text on my clipboard: “1Z999”"], "h"));
    }

    #[test]
    fn old_copy_or_copy_from_before_the_start_stays_out() {
        let settings = Settings {
            clipboard_access: ClipboardAccess::Allowed,
            ..Settings::default()
        };
        let ages = [Some(MAX_CLIPBOARD_AGE), None];

        let reactions = ages
            .map(|age| engine_with(&settings).on_key(Key::Char('h'), &FakeDesktop::with_copy("1Z999", age)));

        assert_eq!(reactions, ages.map(|_| request("h")));
    }

    #[test]
    fn cpu_backend_asks_for_short_suggestions() {
        let mut engine = engine_with(&Settings {
            backend: Backend::Cpu,
            suggestion_length: SuggestionLength::VeryLong,
            ..Settings::default()
        });

        let reaction = press(&mut engine, Key::Char('h'));

        assert_eq!(
            reaction,
            Reaction::Request(InferenceRequest::Complete {
                prompt: prompt::framed_prompt(&[], "h"),
                max_tokens: TokenCount(6),
                word_bias: None,
                next_word_bias: NextWordBias::default(),
                min_log_probability: Some(SuggestionLength::Short.tail_cut()),
                min_typed_log_probability: Some(TYPO_CUT),
                fixes_typos: true,
            })
        );
    }

    #[test]
    fn typo_rule_that_suggests_asks_for_no_typo_cut() {
        let mut engine = engine_with(&Settings {
            typo_rule: TypoRule::Suggest,
            ..Settings::default()
        });

        let reaction = press(&mut engine, Key::Char('h'));

        assert_eq!(
            reaction,
            Reaction::Request(InferenceRequest::Complete {
                prompt: prompt::framed_prompt(&[], "h"),
                max_tokens: Settings::default().suggestion_length_in_use().max_tokens(),
                word_bias: None,
                next_word_bias: NextWordBias::default(),
                min_log_probability: Some(Settings::default().suggestion_length_in_use().tail_cut()),
                min_typed_log_probability: None,
                fixes_typos: false,
            })
        );
    }

    #[test]
    fn very_short_suggestion_length_asks_for_three_tokens() {
        let mut engine = engine_with(&Settings {
            backend: Backend::Vulkan,
            suggestion_length: SuggestionLength::VeryShort,
            ..Settings::default()
        });

        let reaction = press(&mut engine, Key::Char('h'));

        assert_eq!(
            reaction,
            Reaction::Request(InferenceRequest::Complete {
                prompt: prompt::framed_prompt(&[], "h"),
                max_tokens: TokenCount(3),
                word_bias: None,
                next_word_bias: NextWordBias::default(),
                min_log_probability: Some(SuggestionLength::VeryShort.tail_cut()),
                min_typed_log_probability: Some(TYPO_CUT),
                fixes_typos: true,
            })
        );
    }

    #[test]
    fn longer_suggestion_lengths_ask_for_more_tokens_and_lower_tail_cuts() {
        let lengths = [SuggestionLength::Long, SuggestionLength::VeryLong];

        let reactions = lengths.map(|suggestion_length| {
            let mut engine = engine_with(&Settings {
                backend: Backend::Vulkan,
                suggestion_length,
                ..Settings::default()
            });
            press(&mut engine, Key::Char('h'))
        });

        assert_eq!(
            reactions,
            [(15, -5.9), (22, -8.6)].map(|(tokens, cut)| {
                Reaction::Request(InferenceRequest::Complete {
                    prompt: prompt::framed_prompt(&[], "h"),
                    max_tokens: TokenCount(tokens),
                    word_bias: None,
                    next_word_bias: NextWordBias::default(),
                    min_log_probability: Some(LogProbability(cut)),
                    min_typed_log_probability: Some(TYPO_CUT),
                    fixes_typos: true,
                })
            })
        );
    }

    #[test]
    fn denied_clipboard_is_never_read() {
        let mut engine = engine_with(&Settings {
            ..Settings::default()
        });
        let desktop = FakeDesktop::with_copy("1Z999", Some(Duration::ZERO));

        let reaction = engine.on_key(Key::Char('h'), &desktop);

        assert_eq!((reaction, desktop.clipboard_reads.get()), (request("h"), 0));
    }

    #[test]
    fn new_settings_apply_from_the_next_session() {
        let mut engine = Engine::default();
        press(&mut engine, Key::Char('h'));
        engine.on_completion(completion(""));
        engine.use_settings(&Settings::default());

        let reaction = press(&mut engine, Key::Char('i'));

        assert_eq!(reaction, request("hi"));
    }

    #[expect(clippy::unnecessary_wraps, reason = "prepare returns an Option")]
    fn preparation(context: &str) -> Option<InferenceRequest> {
        Some(InferenceRequest::Complete {
            prompt: prompt::framed_prompt(&[], context),
            max_tokens: TokenCount(0),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            min_typed_log_probability: None,
            fixes_typos: false,
        })
    }

    #[test]
    fn preparing_decodes_the_field_text_and_adds_no_tokens() {
        let mut engine = Engine::default();

        let request = engine.prepare(&FakeDesktop::with_field_text("Hello wor"));

        assert_eq!(request, preparation("Hello wor"));
    }

    #[test]
    fn preparing_an_empty_field_decodes_the_preface() {
        let mut engine = Engine::default();

        let request = engine.prepare(&FakeDesktop::default());

        assert_eq!(request, preparation(""));
    }

    #[test]
    fn first_key_after_preparing_continues_the_prepared_session() {
        let mut engine = Engine::default();
        let desktop = FakeDesktop::with_field_text("Hello wor");
        engine.prepare(&desktop);

        let reaction = engine.on_key(Key::Char('l'), &desktop);

        assert_eq!(
            (reaction, desktop.surroundings_reads.get()),
            (request("Hello worl"), 1)
        );
    }

    #[test]
    fn first_key_after_preparing_a_long_field_continues_the_prepared_text() {
        let mut engine = Engine::default();
        let desktop = FakeDesktop::with_field_text(&format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 1)));
        let recent_text = "y".repeat(TRIMMED_CHARS_BEFORE_CARET);
        let prepared = engine.prepare(&desktop);

        let reaction = engine.on_key(Key::Char('z'), &desktop);

        assert_eq!(
            (prepared, reaction),
            (preparation(&recent_text), request(&format!("{recent_text}z")))
        );
    }

    #[test]
    fn preparing_while_a_completion_runs_does_nothing() {
        let mut engine = engine_waiting_for("h");

        let request = engine.prepare(&FakeDesktop::with_field_text("Hello"));

        assert_eq!(request, None);
    }

    #[test]
    fn preparing_an_idle_session_decodes_its_prompt() {
        let mut engine = engine_showing("h", "ello");

        let request = engine.prepare(&FakeDesktop::with_field_text("Hello"));

        assert_eq!(request, preparation("h"));
    }

    #[test]
    fn completion_of_the_preparation_shows_nothing() {
        let mut engine = Engine::default();
        engine.prepare(&FakeDesktop::with_field_text("Hello wor"));

        let reaction = engine.on_completion(completion(""));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn completion_of_a_key_after_the_preparation_shows() {
        let mut engine = Engine::default();
        let desktop = FakeDesktop::with_field_text("Hello wor");
        engine.prepare(&desktop);
        engine.on_key(Key::Char('l'), &desktop);
        engine.on_completion(Err(InferenceError::Superseded));

        let reaction = engine.on_completion(completion("d"));

        assert_eq!(reaction, show("d"));
    }

    #[test]
    fn address_field_of_an_email_gets_no_request() {
        let mut engine = Engine::default();

        let reaction = engine.on_key(Key::Char('a'), &FakeDesktop::in_field("com.apple.mail", "To:"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn subject_field_of_an_email_is_not_prepared() {
        let mut engine = Engine::default();

        let request = engine.prepare(&FakeDesktop::in_field("com.apple.mail", "Subject"));

        assert_eq!(request, None);
    }

    #[test]
    fn body_of_an_email_gets_a_request() {
        let mut engine = Engine::default();

        let reaction = engine.on_key(
            Key::Char('a'),
            &FakeDesktop::in_field("com.apple.mail", "Message"),
        );

        assert!(matches!(reaction, Reaction::Request(_)), "{reaction:?}");
    }

    #[test]
    fn debug_does_not_reveal_typed_or_suggested_text() {
        let engine = engine_showing("secret", "word");

        let debug = format!("{engine:?}");

        assert!(!debug.contains("secret") && !debug.contains("word"));
    }
    fn screen_with(line: &str) -> ScreenText {
        ScreenText {
            lines: vec![ScreenLine {
                text: line.to_owned(),
                confidence: Confidence::CERTAIN,
                frame: ScreenRect {
                    left: 0,
                    top: 100,
                    right: 500,
                    bottom: 120,
                },
            }],
            caret: None,
            field: ScreenRect {
                left: 0,
                top: 200,
                right: 500,
                bottom: 240,
            },
        }
    }

    fn engine_that_read(line: &str) -> Engine {
        let mut engine = Engine::default();
        let id = engine.start_screen_read().expect("no session runs");
        engine.on_screen_read(
            ScreenRead {
                id,
                text: Some(screen_with(line)),
            },
            &FakeDesktop::default(),
        );
        engine
    }

    #[test]
    fn session_shows_the_screen_text_of_the_read_before_it() {
        let mut engine = engine_that_read("Dinner at 7?");

        let reaction = press(&mut engine, Key::Char('S'));

        assert_eq!(
            reaction,
            request_with(&["Above the field on my screen: “Dinner at 7?”"], "S")
        );
    }

    #[test]
    fn session_that_starts_during_a_read_has_no_screen_text() {
        let mut engine = engine_that_read("Dinner at 7?");
        engine.start_screen_read();

        let reaction = press(&mut engine, Key::Char('S'));

        assert_eq!(reaction, request("S"));
    }

    #[test]
    fn read_that_comes_during_a_session_goes_into_it() {
        let mut engine = Engine::default();
        let id = engine.start_screen_read().expect("no session runs");
        press(&mut engine, Key::Char('S'));
        engine.on_screen_read(
            ScreenRead {
                id,
                text: Some(screen_with("Dinner at 7?")),
            },
            &FakeDesktop::default(),
        );

        let reaction = press(&mut engine, Key::Char('u'));

        assert_eq!(
            reaction,
            request_with(&["Above the field on my screen: “Dinner at 7?”"], "Su")
        );
    }

    #[test]
    fn screen_text_that_comes_during_a_session_keeps_room_for_the_completion() {
        let mut engine = Engine::default();
        let desktop = FakeDesktop::with_field_text(&"字".repeat(MAX_CHARS_BEFORE_CARET - 1));
        let id = engine.start_screen_read().expect("no session runs");
        engine.on_key(Key::Char('字'), &desktop);
        engine.on_completion(completion(""));
        engine.on_screen_read(
            ScreenRead {
                id,
                text: Some(screen_with(&"屏".repeat(1100))),
            },
            &desktop,
        );

        let request = engine.prepare(&desktop);

        let Some(InferenceRequest::Complete { prompt, .. }) = request else {
            panic!("an idle session prepares its prompt: {request:?}");
        };
        let room = CONTEXT_TOKENS.0 - SuggestionLength::VeryLong.max_tokens().0;
        assert!(estimated_tokens(&prompt.0).0 <= room, "{prompt:?}");
    }

    #[test]
    fn answer_to_an_older_read_is_dropped() {
        let mut engine = Engine::default();
        let older = engine.start_screen_read().expect("no session runs");
        engine.start_screen_read();

        let is_kept = engine.on_screen_read(
            ScreenRead {
                id: older,
                text: Some(screen_with("Dinner at 7?")),
            },
            &FakeDesktop::default(),
        );

        assert!(!is_kept);
    }

    fn answer_read(engine: &mut Engine, id: ScreenReadId, text: Option<ScreenText>) {
        engine.on_screen_read(ScreenRead { id, text }, &FakeDesktop::default());
    }

    #[test]
    fn read_starts_during_a_session_with_stored_screen_text() {
        let mut engine = engine_that_read("Dinner at 7?");
        press(&mut engine, Key::Char('S'));

        let read = engine.start_screen_read();

        assert!(read.is_some());
    }

    #[test]
    fn new_screen_text_replaces_the_stored_text_one_time() {
        let mut engine = engine_that_read("Dinner at 7?");
        press(&mut engine, Key::Char('S'));
        let id = engine
            .start_screen_read()
            .expect("the session uses stored screen text");
        answer_read(&mut engine, id, Some(screen_with("Lunch at 1?")));

        let reaction = press(&mut engine, Key::Char('u'));
        let next_read = engine.start_screen_read();

        assert_eq!(
            (reaction, next_read),
            (
                request_with(&["Above the field on my screen: “Lunch at 1?”"], "Su"),
                None
            )
        );
    }

    #[test]
    fn same_screen_text_keeps_the_prompt() {
        let mut engine = engine_that_read("Dinner at 7?");
        press(&mut engine, Key::Char('S'));
        let id = engine
            .start_screen_read()
            .expect("the session uses stored screen text");
        answer_read(&mut engine, id, Some(screen_with("Dinner at 7?")));

        let reaction = press(&mut engine, Key::Char('u'));

        assert_eq!(
            reaction,
            request_with(&["Above the field on my screen: “Dinner at 7?”"], "Su")
        );
    }

    #[test]
    fn failed_read_keeps_the_stored_screen_text() {
        let mut engine = engine_that_read("Dinner at 7?");
        press(&mut engine, Key::Char('S'));
        let id = engine
            .start_screen_read()
            .expect("the session uses stored screen text");
        answer_read(&mut engine, id, None);

        let reaction = press(&mut engine, Key::Char('u'));

        assert_eq!(
            reaction,
            request_with(&["Above the field on my screen: “Dinner at 7?”"], "Su")
        );
    }

    #[test]
    fn read_starts_during_a_session_without_screen_text() {
        let mut engine = engine_waiting_for("S");

        let read = engine.start_screen_read();

        assert!(read.is_some());
    }

    #[test]
    fn no_read_starts_while_a_read_of_the_session_runs() {
        let mut engine = engine_waiting_for("S");
        engine.start_screen_read();

        let read = engine.start_screen_read();

        assert_eq!(read, None);
    }

    #[test]
    fn ocr_read_starts_after_the_newest_read() {
        let mut engine = Engine::default();
        let read = engine.start_screen_read().expect("no session runs");

        let ocr = engine.start_ocr_after(read, OcrLevel::Fast);

        assert!(ocr.is_some());
    }

    #[test]
    fn no_ocr_read_starts_after_an_older_read() {
        let mut engine = Engine::default();
        let older = engine.start_screen_read().expect("no session runs");
        engine.start_screen_read();

        let ocr = engine.start_ocr_after(older, OcrLevel::Fast);

        assert_eq!(ocr, None);
    }

    #[test]
    fn no_ocr_read_starts_after_an_ocr_read() {
        let mut engine = Engine::default();
        let read = engine.start_screen_read().expect("no session runs");
        let ocr = engine
            .start_ocr_after(read, OcrLevel::Fast)
            .expect("the read is the newest");

        let next = engine.start_ocr_after(ocr, OcrLevel::Fast);

        assert_eq!(next, None);
    }

    #[test]
    fn answer_to_the_read_before_an_ocr_read_is_dropped() {
        let mut engine = Engine::default();
        let read = engine.start_screen_read().expect("no session runs");
        engine.start_ocr_after(read, OcrLevel::Fast);

        let is_kept = engine.on_screen_read(
            ScreenRead {
                id: read,
                text: Some(screen_with("Dinner at 7?")),
            },
            &FakeDesktop::default(),
        );

        assert!(!is_kept);
    }

    #[test]
    fn session_that_got_an_answer_without_text_starts_no_other_read() {
        let mut engine = engine_waiting_for("S");
        let id = engine
            .start_screen_read()
            .expect("the session has no screen text");
        engine.on_screen_read(ScreenRead { id, text: None }, &FakeDesktop::default());

        let read = engine.start_screen_read();

        assert_eq!(read, None);
    }

    #[test]
    fn forgotten_screen_text_is_not_used() {
        let mut engine = engine_that_read("Dinner at 7?");
        engine.forget_screen();

        let reaction = press(&mut engine, Key::Char('S'));

        assert_eq!(reaction, request("S"));
    }

    #[test]
    fn answer_to_a_forgotten_read_is_dropped() {
        let mut engine = Engine::default();
        let id = engine.start_screen_read().expect("no session runs");
        engine.forget_screen();

        let is_kept = engine.on_screen_read(
            ScreenRead {
                id,
                text: Some(screen_with("Dinner at 7?")),
            },
            &FakeDesktop::default(),
        );

        assert!(!is_kept);
    }

    #[test]
    fn session_knows_its_app() {
        let mut engine = Engine::default();

        engine.on_key(
            Key::Char('S'),
            &FakeDesktop::in_field("com.apple.mail", "Message"),
        );

        assert_eq!(
            engine.session_app().map(|app| app.app_id.as_str()),
            Some("com.apple.mail")
        );
    }

    #[test]
    fn text_with_few_characters_for_each_token_keeps_room_for_the_completion() {
        let mut engine = engine_that_read(&"屏".repeat(1100));
        let desktop = FakeDesktop::with_field_text(&"字".repeat(MAX_CHARS_BEFORE_CARET - 1));

        let reaction = engine.on_key(Key::Char('字'), &desktop);

        let Reaction::Request(InferenceRequest::Complete { prompt, .. }) = reaction else {
            panic!("a key in an open field requests a completion: {reaction:?}");
        };
        let room = CONTEXT_TOKENS.0 - SuggestionLength::VeryLong.max_tokens().0;
        assert!(estimated_tokens(&prompt.0).0 <= room, "{prompt:?}");
    }
}
