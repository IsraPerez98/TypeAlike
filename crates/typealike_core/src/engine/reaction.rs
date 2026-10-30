//! Gives what the platform must do after the engine handles an event.

use std::fmt;

use crate::inference::InferenceRequest;

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
    /// Swallow the key and hide any ghost text, so that the focused app does not get the key.
    Swallow,
    /// Let the key through, hide any ghost text and pause the suggestions in the field, because the
    /// user rejected the suggestion that showed. The keys there get no suggestion until
    /// [`App::end_field_pause`](crate::app::App::end_field_pause) or until the caret moves
    /// somewhere unknown.
    PauseField,
    /// Let the key through, hide any ghost text and submit this request to the model runner.
    /// Its completion goes to [`App::on_completion`](crate::app::App::on_completion). It replaces
    /// the requests that the runner did not answer yet.
    Request(InferenceRequest),
}

/// What follows the text that Tab types: the rest of the suggestion, else what the text before the
/// caret gets after a typed key.
#[derive(Debug, Clone, PartialEq)]
pub enum AfterAccept {
    /// Show this rest of the suggestion after the typed text. The user saw it before Tab, unless
    /// the field had no room for it.
    Rest(String),
    /// The suggestion has no more text. Do what this reaction asks, as after a typed key.
    Next(Box<Reaction>),
}

/// The fix of a typo in the word before the caret, as the ghost text shows it: the typed letters
/// struck through, and the fixed word and the rest of the suggestion after the caret. `Debug` shows
/// only the lengths, so logs never hold what the user wrote.
///
/// ```
/// use typealike_core::engine::WordFix;
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
    /// The typed text just before the caret, which the fix replaces: the typed letters, and the
    /// space or the sign that the user typed after them while the fix showed, if any.
    pub typo: String,
    /// The text that replaces `typo`: the fixed word, with the same space or sign at its end.
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
