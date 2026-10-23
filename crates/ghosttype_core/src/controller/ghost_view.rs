//! What the platform shows for a suggestion: the ghost text and the mirror of the typed word.

use std::{fmt, iter};

use crate::engine::{WordFix, split_next_part};

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

    /// Keeps the whole Tab parts of the ghost text that fit in `room` after the caret, as
    /// `width_of` measures a ghost text after the caret. A fix keeps its word and cuts only its
    /// rest. When not even the first part fits, the whole ghost text moves below the caret. A view
    /// below the caret stays whole.
    #[must_use]
    pub fn fit_to(self, room: i32, width_of: impl Fn(&Ghost) -> i32) -> Self {
        if self.placement == GhostPlacement::BelowCaret || width_of(&self.ghost) <= room {
            return self;
        }
        let fitting = self
            .ghost
            .cuts()
            .take_while(|ghost| width_of(ghost) <= room)
            .last();
        match fitting {
            Some(ghost) => Self { ghost, ..self },
            None => Self {
                placement: GhostPlacement::BelowCaret,
                ..self
            },
        }
    }

    /// Returns the characters at the end of `whole` that this view of it leaves out.
    pub(super) fn left_out_chars(&self, whole: &Ghost) -> usize {
        let shown_chars = self.ghost.cuttable_text().chars().count();
        whole.cuttable_text().chars().count().saturating_sub(shown_chars)
    }
}

impl Ghost {
    /// The text that a cut can shorten: the whole ghost text, or the rest after the word of a fix.
    fn cuttable_text(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Fix(fix) => &fix.rest,
        }
    }

    /// Returns each cut of this ghost text after a whole Tab part, shortest first. A ghost text
    /// keeps at least its first part, and a fix keeps at least its word.
    fn cuts(&self) -> impl Iterator<Item = Self> + '_ {
        let word_only = matches!(self, Self::Fix(_)).then_some(0);
        word_only
            .into_iter()
            .chain(part_ends(self.cuttable_text()))
            .map(|end| self.cut_at(end))
    }

    /// Returns this ghost text with its cuttable text up to the byte index `end`.
    fn cut_at(&self, end: usize) -> Self {
        match self {
            Self::Text(text) => Self::Text(text[..end].to_owned()),
            Self::Fix(fix) => Self::Fix(WordFix {
                rest: fix.rest[..end].to_owned(),
                ..fix.clone()
            }),
        }
    }
}

/// Returns the byte index after each Tab part of `text`, in order.
fn part_ends(text: &str) -> impl Iterator<Item = usize> + '_ {
    let mut end = 0;
    iter::from_fn(move || {
        let (part, _) = split_next_part(&text[end..]);
        if part.is_empty() {
            return None;
        }
        end += part.len();
        Some(end)
    })
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

    /// The width of `ghost` after the caret at 10 pixels for each character, with the space that
    /// the platforms draw before the word of a fix.
    fn width_of(ghost: &Ghost) -> i32 {
        let characters = match ghost {
            Ghost::Text(text) => text.chars().count(),
            Ghost::Fix(fix) => 1 + fix.word.chars().count() + fix.rest.chars().count(),
        };
        i32::try_from(characters * 10).expect("a test ghost is short")
    }

    fn inline_text(text: &str) -> GhostView {
        GhostView::new(Ghost::Text(text.to_owned()), "Best", GhostPlacement::AfterCaret)
    }

    fn inline_fix(rest: &str) -> GhostView {
        let fix = WordFix {
            typo: "teh".to_owned(),
            word: "the".to_owned(),
            rest: rest.to_owned(),
        };
        GhostView::new(Ghost::Fix(fix), "Until teh", GhostPlacement::AfterCaret)
    }

    #[test]
    fn fit_keeps_a_ghost_text_that_fits_whole() {
        let view = inline_text(" regards, Sam");

        let fitted = view.clone().fit_to(130, width_of);

        assert_eq!(fitted, view);
    }

    #[test]
    fn fit_cuts_the_ghost_text_after_the_last_whole_part_that_fits() {
        let rooms = [80, 129, 130, 169];

        let ghosts = rooms.map(|room| inline_text(" regards, Sam and Ana").fit_to(room, width_of).ghost);

        assert_eq!(
            ghosts,
            [" regards", " regards,", " regards, Sam", " regards, Sam"]
                .map(|text| Ghost::Text(text.to_owned()))
        );
    }

    #[test]
    fn fit_moves_the_whole_ghost_text_below_the_caret_when_its_first_part_does_not_fit() {
        let rooms = [79, 0, -40];

        let views = rooms.map(|room| inline_text(" regards, Sam").fit_to(room, width_of));

        let below = GhostView {
            placement: GhostPlacement::BelowCaret,
            ..inline_text(" regards, Sam")
        };
        assert_eq!(views, [below.clone(), below.clone(), below]);
    }

    #[test]
    fn fit_keeps_the_word_of_a_fix_and_cuts_only_its_rest() {
        let rooms = [40, 79, 80];

        let ghosts = rooms.map(|room| inline_fix(" end of it").fit_to(room, width_of).ghost);

        assert_eq!(
            ghosts,
            [inline_fix(""), inline_fix(""), inline_fix(" end")].map(|view| view.ghost)
        );
    }

    #[test]
    fn fit_moves_a_fix_below_the_caret_when_its_word_does_not_fit() {
        let fitted = inline_fix(" end").fit_to(39, width_of);

        assert_eq!(
            fitted,
            GhostView {
                placement: GhostPlacement::BelowCaret,
                ..inline_fix(" end")
            }
        );
    }

    #[test]
    fn fit_leaves_a_ghost_text_below_the_caret_whole() {
        let view = GhostView {
            placement: GhostPlacement::BelowCaret,
            ..inline_text(" regards, Sam")
        };

        let fitted = view.clone().fit_to(10, width_of);

        assert_eq!(fitted, view);
    }

    #[test]
    fn fitted_view_tells_the_characters_that_it_leaves_out() {
        let ghost = Ghost::Text(" regards, Sam and Ana".to_owned());

        let left_out = [80, 130, 400].map(|room| {
            inline_text(" regards, Sam and Ana")
                .fit_to(room, width_of)
                .left_out_chars(&ghost)
        });

        assert_eq!(left_out, [13, 8, 0]);
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
