//! What the platform shows for a suggestion: the ghost text and the mirror of the typed word.

use std::fmt;

use crate::desktop::{LinesBelow, ScreenRect};
use crate::engine::WordFix;

/// The most characters of the word before the caret that the mirror shows.
const MIRROR_WORD_CHARS: usize = 24;
/// The room that the ghost text after the caret keeps free inside each side of its field.
const FIELD_EDGE_GAP: i32 = 4;

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

/// The lines below the line of the caret that a long ghost text after the caret wraps onto, as
/// its field wraps the text when the user types it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrappedLines {
    /// The left edge of each line, near the left edge of the field, in the units of
    /// [`ScreenRect`].
    pub left: i32,
    /// The text of each line, top first. Each line shows one caret height below the line above
    /// it, in the font and the color of the ghost text after the caret.
    pub lines: Vec<String>,
}

/// What the platform shows for a suggestion: the ghost text at the caret or in the mirror below
/// it, else, when it finds no caret, the mirror next to the focused field.
#[derive(Clone, PartialEq, Eq)]
pub struct GhostView {
    /// The ghost text, or with [`wrapped`](Self::wrapped) lines only its part on the line of the
    /// caret.
    pub ghost: Ghost,
    /// The rest of a ghost text after the caret that does not fit on the line of the caret.
    pub wrapped: Option<WrappedLines>,
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
            wrapped: None,
            placement,
        }
    }

    /// Fits the ghost text after the caret into `room`, as `width_of` measures a ghost text after
    /// the caret. A ghost text that does not fit on the line of the caret wraps onto the lines
    /// below. When the field has no room for those lines, or they have text that the ghost text
    /// would cover, only the words that fit on the line of the caret show, and Tab shows the rest
    /// after them. When not even its first word fits there, the whole ghost text moves below the
    /// caret. A view below the caret stays whole.
    pub(super) fn fit_to(self, room: &FieldRoom, width_of: impl Fn(&Ghost) -> i32) -> Self {
        if self.placement == GhostPlacement::BelowCaret || width_of(&self.ghost) <= room.caret_line {
            return self;
        }
        if let Some((ghost, lines)) = room.wrap(&self.ghost, &width_of) {
            return Self {
                ghost,
                wrapped: Some(WrappedLines {
                    left: room.line_left,
                    lines,
                }),
                ..self
            };
        }
        match room.caret_line_part(&self.ghost, &width_of) {
            Some(ghost) => Self { ghost, ..self },
            None => Self {
                placement: GhostPlacement::BelowCaret,
                ..self
            },
        }
    }

    /// The length in bytes of the end of `whole`, the ghost text that this view fits, that the
    /// view does not show, because it did not fit on the line of the caret.
    pub(super) fn hidden_len(&self, whole: &Ghost) -> usize {
        let wrapped_len: usize = self
            .wrapped
            .as_ref()
            .map_or(0, |wrapped| wrapped.lines.iter().map(String::len).sum());
        let shown_len = self.ghost.cuttable_text().len().saturating_add(wrapped_len);
        whole.cuttable_text().len().saturating_sub(shown_len)
    }
}

/// The room for a ghost text after the caret inside its field, between the gaps at its edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FieldRoom {
    /// The width between the caret and the right edge.
    caret_line: i32,
    /// The left edge of each line below the line of the caret.
    line_left: i32,
    /// The width of each line below the line of the caret.
    line_width: i32,
    /// How many lines of the height of the caret fit below the line of the caret, and have no
    /// text to cover.
    lines_below: usize,
}

impl FieldRoom {
    /// The room after `caret` in `field`. The lines below the line of the caret count only when
    /// `lines_below` has no text that a ghost text on them would cover.
    pub(super) fn after(caret: ScreenRect, field: ScreenRect, lines_below: LinesBelow) -> Self {
        let line_left = field.left.saturating_add(FIELD_EDGE_GAP);
        let line_right = field.right.saturating_sub(FIELD_EDGE_GAP);
        Self {
            caret_line: line_right.saturating_sub(caret.right),
            line_left,
            line_width: line_right.saturating_sub(line_left),
            lines_below: match lines_below {
                LinesBelow::Empty => lines_below_in(caret, field),
                LinesBelow::WithText => 0,
            },
        }
    }

    /// Splits `ghost` after the whitespace between its words into its part on the line of the
    /// caret and the lines below, each as long as fits, as `width_of` measures each without the
    /// whitespace at its end. `None` when a part or a line does not fit, or the lines do not fit
    /// below the caret.
    fn wrap(&self, ghost: &Ghost, width_of: impl Fn(&Ghost) -> i32) -> Option<(Ghost, Vec<String>)> {
        let text = ghost.cuttable_text();
        let breaks = line_breaks(text);
        let first_end = self.caret_line_end(ghost, &breaks, &width_of)?;
        let mut lines = Vec::new();
        let mut start = first_end;
        while start < text.len() {
            let end = longest_line(text, start, &breaks, |end| {
                width_of(&Ghost::Text(text[start..end].to_owned())) <= self.line_width
            })?;
            lines.push(text[start..end].to_owned());
            start = end;
        }
        (lines.len() <= self.lines_below).then(|| (ghost.cut_at(first_end), lines))
    }

    /// The words of `ghost` that fit on the line of the caret, as `width_of` measures them,
    /// without the whitespace after them. A fix always keeps its word. `None` when not even the
    /// first word of a ghost text fits.
    fn caret_line_part(&self, ghost: &Ghost, width_of: impl Fn(&Ghost) -> i32) -> Option<Ghost> {
        let text = ghost.cuttable_text();
        let end = self.caret_line_end(ghost, &line_breaks(text), width_of)?;
        let part = text[..end].trim_end();
        let has_word = !part.is_empty() || matches!(ghost, Ghost::Fix(_));
        has_word.then(|| ghost.cut_at(part.len()))
    }

    /// The end of the part of `ghost` on the line of the caret, at one of `breaks`.
    fn caret_line_end(
        &self,
        ghost: &Ghost,
        breaks: &[usize],
        width_of: impl Fn(&Ghost) -> i32,
    ) -> Option<usize> {
        longest_line(ghost.cuttable_text(), 0, breaks, |end| {
            width_of(&ghost.cut_at(end)) <= self.caret_line
        })
    }
}

/// Returns how many lines of the height of `caret` fit in `field` below the line of `caret`.
fn lines_below_in(caret: ScreenRect, field: ScreenRect) -> usize {
    field
        .bottom
        .saturating_sub(caret.bottom)
        .checked_div(caret.bottom.saturating_sub(caret.top))
        .and_then(|lines| usize::try_from(lines).ok())
        .unwrap_or(0)
}

/// Returns the end of the longest line of `text` from `start` to one of `breaks` whose text up to
/// the end of its last word `fits`. `None` when not even the first word fits.
fn longest_line(text: &str, start: usize, breaks: &[usize], fits: impl Fn(usize) -> bool) -> Option<usize> {
    breaks
        .iter()
        .copied()
        .filter(|&end| end > start)
        .take_while(|&end| fits(start + text[start..end].trim_end().len()))
        .last()
}

/// Returns the byte index after each run of whitespace before a word of `text`, then the end of
/// `text`: the places where a field wraps a line.
fn line_breaks(text: &str) -> Vec<usize> {
    text.char_indices()
        .zip(text.chars().skip(1))
        .filter(|&((_, character), next)| character.is_whitespace() && !next.is_whitespace())
        .map(|((index, character), _)| index + character.len_utf8())
        .chain([text.len()])
        .collect()
}

impl Ghost {
    /// The text that a wrap can split: the whole ghost text, or the rest after the word of a fix.
    fn cuttable_text(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Fix(fix) => &fix.rest,
        }
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

impl fmt::Debug for GhostView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GhostView")
            .field("ghost", &self.ghost)
            .field("wrapped", &self.wrapped)
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
            "cat crates/typealike_core/src/controller",
            GhostPlacement::BelowCaret,
        );

        assert_eq!(view.typed_word, "…like_core/src/controller");
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

    /// A field with three lines of 20 pixels below a caret line at the top, and room for 19
    /// characters on each line between the gaps at its edges.
    const FIELD: ScreenRect = ScreenRect {
        left: 0,
        top: 0,
        right: 204,
        bottom: 80,
    };

    /// A caret of 20 pixels on the first line of [`FIELD`], with room for 10 characters after a
    /// caret at 100.
    fn caret_at(left: i32) -> ScreenRect {
        ScreenRect {
            left,
            top: 0,
            right: left,
            bottom: 20,
        }
    }

    /// The room after a caret at `caret_left` in `field`, above lines that `lines_below` tells
    /// about.
    fn room(caret_left: i32, field: ScreenRect, lines_below: LinesBelow) -> FieldRoom {
        FieldRoom::after(caret_at(caret_left), field, lines_below)
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

    fn fix_with_rest(rest: &str) -> Ghost {
        inline_fix(rest).ghost
    }

    fn wrapped(lines: &[&str]) -> WrappedLines {
        WrappedLines {
            left: 4,
            lines: lines.iter().copied().map(str::to_owned).collect(),
        }
    }

    fn below_caret(view: GhostView) -> GhostView {
        GhostView {
            placement: GhostPlacement::BelowCaret,
            ..view
        }
    }

    #[test]
    fn fit_keeps_a_ghost_text_that_fits_whole() {
        let view = inline_text(" regards");

        let fitted = view
            .clone()
            .fit_to(&room(100, FIELD, LinesBelow::Empty), width_of);

        assert_eq!(fitted, view);
    }

    #[test]
    fn fit_wraps_the_rest_onto_the_lines_below_near_the_left_edge_of_the_field() {
        let view = inline_text(" see you next week to discuss the findings");

        let fitted = view.fit_to(&room(100, FIELD, LinesBelow::Empty), width_of);

        assert_eq!(
            (fitted.ghost, fitted.wrapped),
            (
                Ghost::Text(" see you ".to_owned()),
                Some(wrapped(&["next week to ", "discuss the ", "findings"]))
            )
        );
    }

    #[test]
    fn fit_puts_a_first_word_that_does_not_fit_on_the_next_line() {
        let view = inline_text(" regards");

        let fitted = view.fit_to(&room(170, FIELD, LinesBelow::Empty), width_of);

        assert_eq!(
            (fitted.ghost, fitted.wrapped),
            (Ghost::Text(" ".to_owned()), Some(wrapped(&["regards"])))
        );
    }

    #[test]
    fn fit_keeps_closing_signs_on_the_line_of_their_word() {
        let view = inline_text(" regards, Sam");

        let fitted = view.fit_to(&room(120, FIELD, LinesBelow::Empty), width_of);

        assert_eq!(
            (fitted.ghost, fitted.wrapped),
            (Ghost::Text(" ".to_owned()), Some(wrapped(&["regards, Sam"])))
        );
    }

    #[test]
    fn fit_keeps_the_word_of_a_fix_on_the_line_of_the_caret_and_wraps_its_rest() {
        let view = inline_fix(" end of the story");

        let fitted = view.fit_to(&room(100, FIELD, LinesBelow::Empty), width_of);

        assert_eq!(
            (fitted.ghost, fitted.wrapped),
            (fix_with_rest(" end "), Some(wrapped(&["of the story"])))
        );
    }

    #[test]
    fn fit_shows_only_the_words_that_fit_on_the_line_of_the_caret_when_the_field_has_no_room_for_its_lines() {
        let fields = [20, 60].map(|bottom| ScreenRect { bottom, ..FIELD });
        let view = inline_text(" see you next week to discuss the findings");

        let fitted = fields.map(|field| {
            view.clone()
                .fit_to(&room(100, field, LinesBelow::Empty), width_of)
        });

        assert_eq!(fitted, [inline_text(" see you"), inline_text(" see you")]);
    }

    #[test]
    fn fit_shows_the_word_of_a_fix_and_the_words_of_its_rest_that_fit_on_the_line_of_the_caret() {
        let view = inline_fix(" end of the story");

        let fitted = view.fit_to(&room(100, FIELD, LinesBelow::WithText), width_of);

        assert_eq!(fitted, inline_fix(" end"));
    }

    #[test]
    fn fit_moves_the_whole_ghost_text_below_the_caret_when_its_first_word_does_not_fit() {
        let views = [inline_text("ld of hope"), inline_fix(" end")];

        let fitted = views
            .clone()
            .map(|view| view.fit_to(&room(190, FIELD, LinesBelow::Empty), width_of));

        assert_eq!(fitted, views.map(below_caret));
    }

    #[test]
    fn fit_moves_the_whole_ghost_text_below_the_caret_when_a_word_is_wider_than_a_line() {
        let view = inline_text(" supercalifragilisticexpialidocious");

        let fitted = view
            .clone()
            .fit_to(&room(100, FIELD, LinesBelow::Empty), width_of);

        assert_eq!(fitted, below_caret(view));
    }

    #[test]
    fn fit_shows_only_the_words_that_fit_on_the_line_of_the_caret_above_text_below() {
        let view = inline_text(" see you next week to discuss the findings");

        let fitted = view.fit_to(&room(100, FIELD, LinesBelow::WithText), width_of);

        assert_eq!(fitted, inline_text(" see you"));
    }

    #[test]
    fn hidden_len_is_the_length_of_the_words_that_do_not_fit_on_the_line_of_the_caret() {
        let whole = Ghost::Text(" see you next week to discuss the findings".to_owned());
        let view = inline_text(" see you next week to discuss the findings");

        let fitted = view.fit_to(&room(100, FIELD, LinesBelow::WithText), width_of);

        assert_eq!(
            fitted.hidden_len(&whole),
            " next week to discuss the findings".len()
        );
    }

    #[test]
    fn hidden_len_is_zero_for_a_ghost_text_that_shows_whole_on_its_lines() {
        let whole = Ghost::Text(" see you next week to discuss the findings".to_owned());
        let wide_field = ScreenRect { right: 1000, ..FIELD };
        let fields = [wide_field, FIELD];

        let fitted = fields.map(|field| {
            inline_text(" see you next week to discuss the findings")
                .fit_to(&room(100, field, LinesBelow::Empty), width_of)
                .hidden_len(&whole)
        });

        assert_eq!(fitted, [0, 0]);
    }

    #[test]
    fn fit_keeps_a_ghost_text_that_fits_whole_on_the_line_of_the_caret_above_text_below() {
        let view = inline_text(" regards");

        let fitted = view
            .clone()
            .fit_to(&room(100, FIELD, LinesBelow::WithText), width_of);

        assert_eq!(fitted, view);
    }

    #[test]
    fn fit_leaves_a_ghost_text_below_the_caret_whole() {
        let view = below_caret(inline_text(" see you next week to discuss the findings"));

        let fitted = view
            .clone()
            .fit_to(&room(100, FIELD, LinesBelow::Empty), width_of);

        assert_eq!(fitted, view);
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
