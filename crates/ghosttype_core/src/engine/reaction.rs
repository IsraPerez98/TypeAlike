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
