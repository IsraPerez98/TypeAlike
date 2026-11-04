//! Holds the text of a typing session and keeps it in the token budget of the prompt.

use crate::desktop::{MAX_CHARS_BEFORE_CARET, Surroundings, Timestamp};
use crate::excerpt::{estimated_tokens, last_chars, last_tokens};
use crate::inference::{CONTEXT_TOKENS, TokenCount};
use crate::place;
use crate::prompt;
use crate::settings::{ScreenTextApp, SuggestionLength};

use super::shown_suggestions::ShownSuggestions;

/// The characters before the caret that a session keeps when its text reaches
/// [`MAX_CHARS_BEFORE_CARET`]. The text drops its oldest characters in large steps, so that the
/// start of the prompt stays the same for many keys and the model runner can reuse its work.
pub(super) const TRIMMED_CHARS_BEFORE_CARET: usize = 3000;

/// The facts of the field where a session types, which stay the same for the whole session.
pub(super) struct SessionField {
    /// The app where the user types, if it has an id.
    pub(super) app: Option<ScreenTextApp>,
    /// The host of the web page where the user types, if any.
    pub(super) domain: Option<String>,
    /// Whether the platform reads the text of the field.
    pub(super) is_text_field: bool,
    /// Whether the field gets no suggestions, so that Tab moves the focus out of it.
    pub(super) is_excluded: bool,
    /// Whether the field hides the text on the line of the caret, so that text can follow the
    /// caret also when the field gives no text after the caret.
    is_caret_line_hidden: bool,
    /// The typing context of the window, if its title names one.
    pub(super) typing_context: Option<String>,
}

impl SessionField {
    fn of(surroundings: &Surroundings) -> Self {
        Self {
            typing_context: crate::history::typing_context(
                surroundings.app_name.as_deref(),
                surroundings.window_title.as_deref(),
            ),
            app: ScreenTextApp::of(surroundings),
            domain: surroundings.domain.clone(),
            is_text_field: surroundings.field_text.is_some(),
            is_excluded: surroundings.is_search_field || place::is_address_or_subject_field(surroundings),
            is_caret_line_hidden: surroundings.is_caret_line_hidden,
        }
    }
}

/// The typing in one place: what the model reads before the text, and the text before the caret.
/// It starts at the first key after a reset, when the key has not reached the app yet, or at a
/// preparation.
pub(super) struct Session {
    pub(super) preface: String,
    pub(super) text_before: String,
    /// The characters at the start of `text_before` that the field held when the session started.
    pub(super) field_chars: usize,
    /// The characters at the start of `text_before` that the last read of the field showed, up to
    /// the word at the caret of that read. The app can still change the text after them, as an
    /// autocorrect does when the user finishes a word.
    pub(super) read_chars: usize,
    /// The most estimated tokens of `text_before`, so that the prompt fits in the context.
    pub(super) max_tokens_before: TokenCount,
    pub(super) text_after: String,
    pub(super) field: SessionField,
    pub(super) has_accepted: bool,
    /// Whether a read of the screen answered during the session. Until then, the preface holds the
    /// stored text of a read before the session, if any. After it, the session starts no other
    /// read, so that the start of its prompt changes at most one time.
    pub(super) has_screen_read: bool,
    /// The text before the caret in the field when the user pressed Enter, while the text before
    /// the caret is not known until the next read of the field. It is the text of the session
    /// when the platform gave no field text.
    pub(super) field_at_enter: Option<String>,
    /// The suggestions that the ghost text showed in the session, which a backspace or a retype
    /// shows again. They all have the text after the caret of the session.
    pub(super) shown_suggestions: ShownSuggestions,
    /// When the session started, if the desktop has a clock.
    pub(super) started_at: Option<Timestamp>,
    /// The words of the message that the user answers, from the screen text of the preface.
    pub(super) reply_to: Vec<String>,
}

impl Session {
    /// Starts a session at `started_at` in the field of `surroundings`, where the model reads
    /// `preface` before the text. The session keeps the end of the field text before the caret
    /// that fits after it.
    pub(super) fn new(preface: String, surroundings: &Surroundings, started_at: Option<Timestamp>) -> Self {
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
        Self {
            preface,
            field_chars: text_before.chars().count(),
            read_chars: finished_chars(&text_before),
            text_before,
            max_tokens_before,
            text_after,
            field: SessionField::of(surroundings),
            has_accepted: false,
            has_screen_read: false,
            field_at_enter: None,
            shown_suggestions: ShownSuggestions::default(),
            started_at,
            reply_to: Vec::new(),
        }
    }

    /// Whether a read of the field shows the text before the caret of the session: the platform
    /// reads the field, the field shows the line of the caret, and no read after Enter waits.
    pub(super) fn can_follow_field(&self) -> bool {
        self.field.is_text_field && !self.field.is_caret_line_hidden && self.field_at_enter.is_none()
    }

    pub(super) fn is_caret_at_line_end(&self) -> bool {
        !self.field.is_caret_line_hidden && self.line_after().is_empty()
    }

    /// The text after the caret on its line, or empty when that line has only whitespace after
    /// the caret.
    pub(super) fn line_after(&self) -> &str {
        let line = self.text_after.lines().next().unwrap_or_default();
        if line.trim().is_empty() { "" } else { line }
    }

    /// Makes `preface` what the model reads before the text, and drops the oldest text before the
    /// caret that does not fit after it.
    pub(super) fn use_preface(&mut self, preface: String) {
        self.max_tokens_before = tokens_before_caret(&preface);
        self.preface = preface;
        self.keep_recent_text();
    }

    /// Adds `text` before the caret, and drops the oldest text before the caret that does not fit.
    pub(super) fn append_text(&mut self, text: &str) {
        self.text_before.push_str(text);
        self.keep_recent_text();
    }

    pub(super) fn keep_recent_text(&mut self) {
        let kept = recent_text(&self.text_before, self.max_tokens_before).len();
        let dropped_chars = self
            .text_before
            .drain(..self.text_before.len() - kept)
            .as_str()
            .chars()
            .count();
        self.field_chars = self.field_chars.saturating_sub(dropped_chars);
        self.read_chars = self.read_chars.saturating_sub(dropped_chars);
    }

    pub(super) fn delete_last_character(&mut self) {
        self.text_before.pop();
        self.keep_counts_inside_text();
    }

    /// Replaces `typo` at the end of the text before the caret with `word`, and drops the oldest
    /// text before the caret that does not fit.
    pub(super) fn replace_end(&mut self, typo: &str, word: &str) {
        if let Some(kept) = self.text_before.strip_suffix(typo).map(str::len) {
            self.text_before.truncate(kept);
        }
        self.keep_counts_inside_text();
        self.append_text(word);
    }

    /// Makes `text_before` the text before the caret that a read of the field showed, and drops
    /// the oldest text before the caret that does not fit.
    pub(super) fn use_read_text(&mut self, text_before: String) {
        self.read_chars = finished_chars(&text_before);
        self.text_before = text_before;
        self.keep_counts_inside_text();
        self.keep_recent_text();
    }

    fn keep_counts_inside_text(&mut self) {
        let chars = self.text_before.chars().count();
        self.field_chars = self.field_chars.min(chars);
        self.read_chars = self.read_chars.min(chars);
    }
}

/// Returns the characters of `text` before the word at its end, or all of them when `text` ends
/// with whitespace.
fn finished_chars(text: &str) -> usize {
    text.trim_end_matches(|character: char| !character.is_whitespace())
        .chars()
        .count()
}

/// Returns the most estimated tokens of the text before the caret in a prompt with `preface`, so
/// that the prompt leaves room for the longest completion.
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
pub(super) fn recent_text(text: &str, max_tokens: TokenCount) -> &str {
    if text.chars().count() < MAX_CHARS_BEFORE_CARET && estimated_tokens(text) < max_tokens {
        return text;
    }
    let trimmed_tokens = TokenCount(max_tokens.0 / 4 * 3);
    last_tokens(last_chars(text, TRIMMED_CHARS_BEFORE_CARET), trimmed_tokens)
}
