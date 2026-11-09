//! Takes the changes that the app makes to the typed text, such as an autocorrect, so that the
//! prompt and the earlier writing hold the text of the field.

use crate::desktop::Desktop;
use crate::history::is_within_edits;

use super::{Engine, read_surroundings};

/// The most characters that an app changes in a finished word: an autocorrect, a text replacement
/// such as "q" for "que", a capital letter or a smart quote.
const MAX_CHANGED_CHARS: usize = 2;

impl Engine {
    /// Reads the field of `desktop`. A session whose field held no text takes the field when it
    /// shows text in the same app. Else the session takes the text before the caret when it shows
    /// the text that the user typed since the last read, or that text with words that the app
    /// changed after the user finished them. The session ends when the field shows only the newest
    /// keys, because these keys went to another field. When the read changes the text before the
    /// caret of a suggestion that runs or shows, and the user pressed no key after its request,
    /// the suggestion is due again ([`suggest_when_due`](Self::suggest_when_due)). Returns whether
    /// the session took the field.
    pub(crate) fn follow_field(&mut self, desktop: &dyn Desktop) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        let text_before = session.text_before.clone();
        let has_taken_field = if session.field.holds_text() {
            self.follow_text_field(desktop);
            false
        } else {
            session.take_field_of(&read_surroundings(desktop))
        };
        self.is_suggestion_due |= self.has_untouched_offer() && self.text_before_caret() != text_before;
        has_taken_field
    }

    fn follow_text_field(&mut self, desktop: &dyn Desktop) {
        let Some(session) = self.session.as_mut().filter(|session| session.can_follow_field()) else {
            return;
        };
        let Some(field_text) = read_surroundings(desktop).field_text else {
            return;
        };
        let field_before = &field_text.before_caret;
        let (text_before, read_chars) = (&session.text_before, session.read_chars);
        if let Some(shown) = text_shown_by_field(text_before, read_chars, field_before) {
            session.use_read_text(shown);
        } else if let Some(earlier) = text_before_other_field(text_before, read_chars, field_before) {
            session.use_read_text(earlier);
            self.reset();
        }
    }
}

/// Returns the text before the caret that `field_before` shows for `text_before`, whose first
/// `read_chars` characters an earlier read showed. `None` when the field does not show the
/// characters after them, or a change of them that an app makes.
fn text_shown_by_field(text_before: &str, read_chars: usize, field_before: &str) -> Option<String> {
    let split = text_before
        .char_indices()
        .nth(read_chars)
        .map_or(text_before.len(), |(index, _)| index);
    let (read, typed) = text_before.split_at(split);
    let shown_starts = if read.is_empty() {
        vec![0]
    } else {
        field_before
            .rmatch_indices(read)
            .map(|(index, _)| index + read.len())
            .collect()
    };
    shown_starts
        .into_iter()
        .map(|start| &field_before[start..])
        .find(|shown| *shown == typed || is_app_change_of(typed, shown))
        .map(|shown| format!("{read}{shown}"))
}

/// Returns the start of `text_before` before the end that `field_before` shows, when the field
/// shows only an end of the text that the user typed after the first `read_chars` characters, and
/// that start has a letter. The newest keys then went to another field, as the next cell of a
/// spreadsheet after the edit of a cell stopped. An app can remove a list mark, such as "- ".
fn text_before_other_field(text_before: &str, read_chars: usize, field_before: &str) -> Option<String> {
    let earlier = text_before
        .strip_suffix(field_before)
        .filter(|earlier| !field_before.is_empty() && earlier.chars().count() >= read_chars)?;
    earlier.contains(char::is_alphabetic).then(|| earlier.to_owned())
}

/// Whether `shown` is `typed` after an app changed some of its finished words: the same words, each
/// with at most [`MAX_CHANGED_CHARS`] changed characters, the same word at the caret and the same
/// last character.
fn is_app_change_of(typed: &str, shown: &str) -> bool {
    let typed_words = typed.split_whitespace().collect::<Vec<_>>();
    let shown_words = shown.split_whitespace().collect::<Vec<_>>();
    let keeps_caret_word = typed.ends_with(char::is_whitespace) || typed_words.last() == shown_words.last();
    typed.chars().next_back() == shown.chars().next_back()
        && typed_words.len() == shown_words.len()
        && keeps_caret_word
        && typed_words
            .iter()
            .zip(&shown_words)
            .all(|(typed_word, shown_word)| is_within_changed_chars(typed_word, shown_word))
}

fn is_within_changed_chars(typed_word: &str, shown_word: &str) -> bool {
    let typed_chars = typed_word.chars().collect::<Vec<_>>();
    let shown_chars = shown_word.chars().collect::<Vec<_>>();
    is_within_edits(&typed_chars, &shown_chars, MAX_CHANGED_CHARS)
}
