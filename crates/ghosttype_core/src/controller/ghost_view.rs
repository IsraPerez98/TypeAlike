//! What the platform shows for a suggestion: the ghost text and the mirror of the typed word.

use std::fmt;

use crate::engine::WordFix;

/// The most characters of the word before the caret that the mirror shows.
const MIRROR_WORD_CHARS: usize = 24;

/// What the ghost text shows at the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ghost {
    /// A suggestion after the caret.
    Text(String),
    /// The fix of a typo before the caret, and the rest of the suggestion after it.
    Fix(WordFix),
}

/// Where the platform shows the ghost text when it finds the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhostPlacement {
    /// Right after the caret, at the end of its line.
    AfterCaret,
    /// In the mirror below the caret, because at the caret the ghost text would cover the text
    /// that follows the caret on its line.
    BelowCaret,
}

/// What the platform shows for a suggestion: the ghost text at the caret or in the mirror below
/// it, else, when it finds no caret, the mirror next to the focused field.
#[derive(Clone, PartialEq, Eq)]
pub struct GhostView {
    /// The ghost text.
    pub ghost: Ghost,
    /// The end of the word before the caret, without the typo of a fix. The mirror shows it
    /// before the ghost text, so that the user sees which part they typed.
    pub typed_word: String,
    /// Where the ghost text shows when the platform finds the caret.
    pub placement: GhostPlacement,
}

impl GhostView {
    /// Shows `ghost` after `text_before_caret`, the text of the session before the caret, at
    /// `placement`.
    #[must_use]
    pub fn new(ghost: Ghost, text_before_caret: &str, placement: GhostPlacement) -> Self {
        let text_before_typo = match &ghost {
            Ghost::Fix(fix) => text_before_caret
                .strip_suffix(fix.typo.as_str())
                .unwrap_or(text_before_caret),
            Ghost::Text(_) => text_before_caret,
        };
        Self {
            typed_word: mirrored_word(text_before_typo),
            ghost,
            placement,
        }
    }
}

impl fmt::Debug for GhostView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GhostView")
            .field("ghost", &self.ghost)
            .field(
                "typed_word",
                &format_args!("{} chars", self.typed_word.chars().count()),
            )
            .field("placement", &self.placement)
            .finish()
    }
}

/// Returns the word at the end of `text`, after its last whitespace. A word of more than
/// [`MIRROR_WORD_CHARS`] characters keeps only its end, after an ellipsis.
fn mirrored_word(text: &str) -> String {
    let word = text.rsplit(char::is_whitespace).next().unwrap_or_default();
    let cut_chars = word.chars().count().saturating_sub(MIRROR_WORD_CHARS);
    match word.char_indices().nth(cut_chars) {
        Some((start, _)) if cut_chars > 0 => format!("…{}", &word[start..]),
        _ => word.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_gets_the_word_before_the_caret() {
        let texts_before_caret = ["Dear Ana,\nThanks for the", "git sta", "Hello "];

        let typed_words = texts_before_caret.map(|text| {
            GhostView::new(Ghost::Text("rest".to_owned()), text, GhostPlacement::BelowCaret).typed_word
        });

        assert_eq!(typed_words, ["the", "sta", ""].map(str::to_owned));
    }

    #[test]
    fn mirror_keeps_the_end_of_a_long_word() {
        let view = GhostView::new(
            Ghost::Text(".rs".to_owned()),
            "cat crates/ghosttype_core/src/controller",
            GhostPlacement::BelowCaret,
        );

        assert_eq!(view.typed_word, "…type_core/src/controller");
    }

    #[test]
    fn mirror_leaves_out_the_typo_that_a_fix_replaces() {
        let fix = WordFix {
            typo: "teh".to_owned(),
            word: "the".to_owned(),
            rest: " end".to_owned(),
        };

        let view = GhostView::new(Ghost::Fix(fix), "Until (teh", GhostPlacement::BelowCaret);

        assert_eq!(view.typed_word, "(");
    }

    #[test]
    fn ghost_view_debug_hides_the_typed_word() {
        let view = GhostView::new(
            Ghost::Text(" soon".to_owned()),
            "See you",
            GhostPlacement::AfterCaret,
        );

        let debug = format!("{view:?}");

        assert!(debug.contains("typed_word: 3 chars") && !debug.contains("you"));
    }
}
