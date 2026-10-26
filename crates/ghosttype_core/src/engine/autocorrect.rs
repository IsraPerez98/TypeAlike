//! Takes the changes that the app makes to the typed text, such as an autocorrect, so that the
//! prompt and the earlier writing hold the text of the field.

use crate::desktop::Desktop;
use crate::history::is_within_edits;

use super::{Engine, read_surroundings};

/// The most characters that an app changes in a finished word: an autocorrect, a text replacement
/// such as "q" for "que", a capital letter or a smart quote.
const MAX_CHANGED_CHARS: usize = 2;

impl Engine {
    /// Reads the field of `desktop`, and takes its text before the caret when it shows the text
    /// that the user typed since the last read, or that text with words that the app changed after
    /// the user finished them.
    pub(crate) fn follow_field(&mut self, desktop: &dyn Desktop) {
        let Some(session) = self.session.as_mut().filter(|session| session.can_follow_field()) else {
            return;
        };
        let Some(field_text) = read_surroundings(desktop).field_text else {
            return;
        };
        if let Some(text_before) =
            text_shown_by_field(&session.text_before, session.read_chars, &field_text.before_caret)
        {
            session.use_read_text(text_before);
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
