//! The text near the caret on the screen, as the prompt shows it: the lines without noise, the
//! nearest lines first, up to a budget of tokens.

use std::collections::HashSet;

use crate::desktop::{Confidence, FieldText, ScreenRect, ScreenText};
use crate::inference::TokenCount;
use crate::prompt::estimated_tokens;

/// The lowest confidence of a recognized line that is not noise. Vision gives 30% to its guesses.
const MIN_CONFIDENCE: Confidence = Confidence(40);
/// The most signs in a line that is not noise, in percent of its characters.
const MAX_SIGN_PERCENT: usize = 20;
/// The fewest characters of a line that repeats a part of a line of the field. A shorter line,
/// such as "Yes", is often a reply that the field only quotes.
const MIN_REPEATED_PART_CHARS: usize = 20;
/// The signs that plain text has often, which do not count as signs of noise.
const PLAIN_PUNCTUATION: [char; 15] = [
    '.', ',', ';', ':', '!', '?', '\'', '’', '"', '“', '”', '(', ')', '-', '…',
];

/// A line of text that the prompt can show.
struct Candidate {
    text: String,
    frame: ScreenRect,
}

/// How far a line is from the caret. Lines beside the field, such as sidebars and contact lists,
/// rank after all lines above and below it: they are often near the caret, but they are not part
/// of the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Distance {
    is_beside_field: bool,
    vertical_gap: u32,
}

/// Returns the text of the lines of `screen` that the prompt shows, in reading order: the
/// nearest lines to the caret that fit in `budget`, without the noise and the lines that repeat
/// `field_text`.
pub(crate) fn nearby_lines(
    screen: &ScreenText,
    field_text: Option<&FieldText>,
    budget: TokenCount,
) -> Vec<String> {
    let field = FieldLines::of(field_text);
    let caret = screen.caret.unwrap_or(screen.field);
    let mut candidates = screen
        .lines
        .iter()
        .filter(|line| line.confidence >= MIN_CONFIDENCE && !is_inside(line.frame, screen.field))
        .map(|line| Candidate {
            text: one_line(&line.text),
            frame: line.frame,
        })
        .filter(|candidate| {
            !candidate.text.is_empty() && !is_noise(&candidate.text) && !field.repeats(&candidate.text)
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| {
        (
            distance(candidate.frame, caret, screen.field),
            candidate.frame.top,
            candidate.frame.left,
        )
    });
    let mut kept = within_budget(candidates, budget);
    kept.sort_by_key(|candidate| (candidate.frame.top, candidate.frame.left));
    kept.into_iter().map(|candidate| candidate.text).collect()
}

/// Returns the `candidates` whose tokens fit in what is left of `budget`, in their order.
fn within_budget(candidates: Vec<Candidate>, budget: TokenCount) -> Vec<Candidate> {
    let mut left = budget.0;
    candidates
        .into_iter()
        .filter(
            |candidate| match left.checked_sub(estimated_tokens(&candidate.text).0) {
                Some(rest) => {
                    left = rest;
                    true
                }
                None => false,
            },
        )
        .collect()
}

/// The text of the focused field, to find the screen lines that repeat it.
struct FieldLines {
    text: String,
    lines: HashSet<String>,
}

impl FieldLines {
    fn of(field_text: Option<&FieldText>) -> Self {
        let text = field_text.map_or_else(String::new, |text| {
            format!("{}{}", text.before_caret, text.after_caret)
        });
        Self {
            lines: text.lines().map(one_line).collect(),
            text: one_line(&text),
        }
    }

    /// Whether `line` is a line of the field, or a long part of its text.
    fn repeats(&self, line: &str) -> bool {
        self.lines.contains(line)
            || (line.chars().count() >= MIN_REPEATED_PART_CHARS && self.text.contains(line))
    }
}

fn distance(line: ScreenRect, caret: ScreenRect, field: ScreenRect) -> Distance {
    let vertical_gap = if line.bottom <= caret.top {
        line.bottom.abs_diff(caret.top)
    } else if caret.bottom <= line.top {
        caret.bottom.abs_diff(line.top)
    } else {
        0
    };
    Distance {
        is_beside_field: line.right <= field.left || field.right <= line.left,
        vertical_gap,
    }
}

/// Whether the middle of `line` is in `field`. Such a line is text of the field, such as its
/// placeholder or the text that the user types.
fn is_inside(line: ScreenRect, field: ScreenRect) -> bool {
    let middle_x = line.left.midpoint(line.right);
    let middle_y = line.top.midpoint(line.bottom);
    (field.left..field.right).contains(&middle_x) && (field.top..field.bottom).contains(&middle_y)
}

/// Whether `line` looks like noise: many signs, or a digit between two letters of a word, as text
/// recognition makes them.
fn is_noise(line: &str) -> bool {
    let characters = line.chars().count();
    let signs = line
        .chars()
        .filter(|&character| {
            !character.is_alphanumeric()
                && !character.is_whitespace()
                && !PLAIN_PUNCTUATION.contains(&character)
        })
        .count();
    signs.saturating_mul(100) > characters.saturating_mul(MAX_SIGN_PERCENT) || has_digit_inside_a_word(line)
}

fn has_digit_inside_a_word(line: &str) -> bool {
    line.chars()
        .collect::<Vec<_>>()
        .windows(3)
        .any(|run| run[0].is_alphabetic() && run[1].is_numeric() && run[2].is_alphabetic())
}

/// Returns `text` with each run of whitespace as one space, and no whitespace at its ends.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::ScreenLine;

    const BUDGET: TokenCount = TokenCount(1200);
    const FIELD: ScreenRect = rect(300, 900, 1400, 960);
    const CARET: ScreenRect = rect(320, 920, 321, 940);

    const fn rect(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
        ScreenRect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn line(text: &str, frame: ScreenRect) -> ScreenLine {
        ScreenLine {
            text: text.to_owned(),
            confidence: Confidence::CERTAIN,
            frame,
        }
    }

    /// A line in the column of the field, `rows` lines of 40 pixels above the field.
    fn above(text: &str, rows: i32) -> ScreenLine {
        line(text, rect(300, 900 - 40 * rows, 900, 930 - 40 * rows))
    }

    fn screen(lines: Vec<ScreenLine>) -> ScreenText {
        ScreenText {
            lines,
            caret: Some(CARET),
            field: FIELD,
        }
    }

    fn texts(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|&text| text.to_owned()).collect()
    }

    #[test]
    fn lines_show_in_reading_order() {
        let screen = screen(vec![
            above("See you at 7?", 1),
            above("From: Sam Reyes", 3),
            above("Dinner on Saturday", 2),
        ]);

        let lines = nearby_lines(&screen, None, BUDGET);

        assert_eq!(
            lines,
            texts(&["From: Sam Reyes", "Dinner on Saturday", "See you at 7?"])
        );
    }

    #[test]
    fn budget_keeps_the_nearest_lines() {
        let screen = screen(vec![
            above(&"a".repeat(30), 3),
            above(&"b".repeat(30), 2),
            above(&"c".repeat(30), 1),
        ]);

        let lines = nearby_lines(&screen, None, TokenCount(15));

        assert_eq!(lines, texts(&[&"c".repeat(30)]));
    }

    #[test]
    fn line_that_does_not_fit_leaves_room_for_a_shorter_line() {
        let screen = screen(vec![above(&"a".repeat(60), 1), above("See you at 7?", 2)]);

        let lines = nearby_lines(&screen, None, TokenCount(10));

        assert_eq!(lines, texts(&["See you at 7?"]));
    }

    #[test]
    fn nearest_line_wins_also_below_the_caret() {
        let below = line("> Priya wrote: the budget is final", rect(300, 980, 900, 1010));
        let screen = screen(vec![above("Older message", 2), below]);

        let lines = nearby_lines(&screen, None, TokenCount(15));

        assert_eq!(lines, texts(&["> Priya wrote: the budget is final"]));
    }

    #[test]
    fn line_beside_the_field_ranks_after_the_lines_above_it() {
        let sidebar = line("General channel", rect(0, 920, 250, 940));
        let screen = screen(vec![sidebar, above("Can you send the deck?", 5)]);

        let lines = nearby_lines(&screen, None, TokenCount(10));

        assert_eq!(lines, texts(&["Can you send the deck?"]));
    }

    #[test]
    fn field_frame_stands_for_an_unknown_caret() {
        let screen = ScreenText {
            lines: vec![
                line("Above the field", rect(300, 300, 900, 330)),
                line("Below the field", rect(300, 640, 900, 670)),
            ],
            caret: None,
            field: rect(300, 500, 1400, 600),
        };

        let lines = nearby_lines(&screen, None, TokenCount(6));

        assert_eq!(lines, texts(&["Below the field"]));
    }

    #[test]
    fn whitespace_runs_become_one_space_and_empty_lines_go() {
        let screen = screen(vec![above("  Sure,\n  see\tyou ", 1), above(" \n ", 2)]);

        let lines = nearby_lines(&screen, None, BUDGET);

        assert_eq!(lines, texts(&["Sure, see you"]));
    }

    #[test]
    fn noisy_lines_go() {
        let screen = screen(vec![
            above("|| ~~ == ##", 1),
            above("he1lo w0rld", 2),
            above("#@%& tags", 3),
        ]);

        let lines = nearby_lines(&screen, None, BUDGET);

        assert_eq!(lines, Vec::<String>::new());
    }

    #[test]
    fn lines_that_the_ocr_is_not_sure_of_go() {
        let screen = screen(vec![
            ScreenLine {
                confidence: Confidence::from_fraction(0.3),
                ..above("Dinner at 7?", 1)
            },
            ScreenLine {
                confidence: Confidence::from_fraction(0.5),
                ..above("See you there", 2)
            },
        ]);

        let lines = nearby_lines(&screen, None, BUDGET);

        assert_eq!(lines, texts(&["See you there"]));
    }

    #[test]
    fn line_inside_the_field_goes() {
        let screen = screen(vec![
            above("Dinner at 7?", 1),
            line("Write a message", rect(320, 915, 600, 945)),
        ]);

        let lines = nearby_lines(&screen, None, BUDGET);

        assert_eq!(lines, texts(&["Dinner at 7?"]));
    }

    #[test]
    fn plain_lines_with_punctuation_and_numbers_stay() {
        let screen = screen(vec![
            above("Yes.", 1),
            above("Re: Q3 budget, 10:42 AM", 2),
            above("Total: $12.50 (incl. tax)", 3),
            above("Apt 3B, 1458 Maple Avenue", 4),
        ]);

        let lines = nearby_lines(&screen, None, BUDGET);

        assert_eq!(lines.len(), 4, "{lines:?}");
    }

    #[test]
    fn short_line_inside_a_line_of_the_field_stays() {
        let field_text = FieldText {
            before_caret: "Yes, works for me. See you at ".to_owned(),
            after_caret: String::new(),
        };
        let screen = screen(vec![above("Yes", 1)]);

        let lines = nearby_lines(&screen, Some(&field_text), BUDGET);

        assert_eq!(lines, texts(&["Yes"]));
    }

    #[test]
    fn lines_that_repeat_the_field_text_go() {
        let field_text = FieldText {
            before_caret: "Hi Sam,\n\nSaturday  works great".to_owned(),
            after_caret: " for us.".to_owned(),
        };
        let screen = screen(vec![
            above("Saturday works great for us.", 1),
            above("Hi Sam,", 2),
            above("Want to come over?", 3),
        ]);

        let lines = nearby_lines(&screen, Some(&field_text), BUDGET);

        assert_eq!(lines, texts(&["Want to come over?"]));
    }
}
