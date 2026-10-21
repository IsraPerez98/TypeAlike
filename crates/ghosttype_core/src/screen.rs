//! The text near the caret on the screen, as the prompt shows it: the lines without noise and
//! without the times of chat headers, the nearest lines first, up to a budget of tokens, above and
//! below the caret.

use std::collections::HashSet;

use crate::desktop::{Confidence, FieldText, ScreenRect, ScreenText};
use crate::excerpt::{estimated_tokens, one_line};
use crate::inference::TokenCount;

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
/// The most words of the name in a chat header, as in "Laura Méndez 11:02". A short message with
/// a time at its end, such as "mañana 9:00", has the same form and also loses its time.
const MAX_HEADER_NAME_WORDS: usize = 2;
/// The words after a time that give the half of the day.
const HALVES_OF_DAY: [&str; 2] = ["am", "pm"];
/// The words of a day before the time of a chat header, as in "`chips_dev` — Today at 21:03".
const DAY_WORDS: [&str; 4] = ["today", "yesterday", "hoy", "ayer"];
/// The words between a day word and its time, as in "Today at 21:03" or "Hoy a las 21:03".
const DAY_TIME_JOINS: [&str; 4] = ["at", "a", "las", "la"];

/// A line of text that the prompt can show.
struct Candidate {
    text: String,
    frame: ScreenRect,
}

/// The lines of the screen that the prompt shows, each side of the caret in reading order.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct NearbyLines {
    /// The lines whose middle is above the middle of the caret, such as the messages of a chat.
    pub above: Vec<String>,
    /// The other lines, such as the quote of an email below a reply.
    pub below: Vec<String>,
}

/// How far a line is from the caret. Lines beside the field, such as sidebars and contact lists,
/// rank after all lines above and below it: they are often near the caret, but they are not part
/// of the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Distance {
    is_beside_field: bool,
    vertical_gap: u32,
}

/// Returns the text of the lines of `screen` that the prompt shows, above and below the caret:
/// the nearest lines to the caret that fit in `budget`, without the noise and the lines that
/// repeat `field_text`.
pub(crate) fn nearby_lines(
    screen: &ScreenText,
    field_text: Option<&FieldText>,
    budget: TokenCount,
) -> NearbyLines {
    let field = FieldLines::of(field_text);
    let caret = screen.caret.unwrap_or(screen.field);
    let mut candidates = screen
        .lines
        .iter()
        .filter(|line| line.confidence >= MIN_CONFIDENCE && !is_inside(line.frame, screen.field))
        .map(|line| Candidate {
            text: without_header_time(one_line(&line.text)),
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
    let caret_middle = caret.top.midpoint(caret.bottom);
    let (above, below) = kept.into_iter().partition::<Vec<_>, _>(|candidate| {
        candidate.frame.top.midpoint(candidate.frame.bottom) < caret_middle
    });
    NearbyLines {
        above: above.into_iter().map(|candidate| candidate.text).collect(),
        below: below.into_iter().map(|candidate| candidate.text).collect(),
    }
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

/// Returns the name of `line` when it is a chat header, else `line`. A message keeps its times,
/// because a time is often what it tells.
fn without_header_time(line: String) -> String {
    header_name(&line).unwrap_or(line)
}

/// Returns the name of a chat header: at most [`MAX_HEADER_NAME_WORDS`] words, then a time with
/// maybe a day before it, as in "Laura Méndez 11:02" or "`chips_dev` — Today at 21:03". The name of
/// a line with only a time is empty. `None` when `line` is not a chat header.
fn header_name(line: &str) -> Option<String> {
    let mut words = line.split(' ').collect::<Vec<_>>();
    words.pop_if(|word| is_any_of(word, &HALVES_OF_DAY));
    words.pop_if(|word| is_time(word))?;
    if let Some(day) = words.iter().rposition(|word| is_any_of(word, &DAY_WORDS))
        && words[day + 1..]
            .iter()
            .all(|word| is_any_of(word, &DAY_TIME_JOINS))
        && (day == 0 || is_separator(words[day - 1]))
    {
        words.truncate(day);
    }
    words.pop_if(|word| is_separator(word));
    (words.len() <= MAX_HEADER_NAME_WORDS).then(|| words.join(" "))
}

/// Whether `word` is a time such as "9:05" or "21:03".
fn is_time(word: &str) -> bool {
    word.split_once(':').is_some_and(|(hours, minutes)| {
        (1..=2).contains(&hours.len())
            && minutes.len() == 2
            && hours
                .chars()
                .chain(minutes.chars())
                .all(|character| character.is_ascii_digit())
    })
}

fn is_any_of(word: &str, known_words: &[&str]) -> bool {
    known_words.iter().any(|known| known.eq_ignore_ascii_case(word))
}

/// Whether `word` only divides the parts of a line, as "—" does.
fn is_separator(word: &str) -> bool {
    !word.chars().any(char::is_alphanumeric)
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

    fn above_the_field(lines: &[&str]) -> NearbyLines {
        NearbyLines {
            above: texts(lines),
            below: Vec::new(),
        }
    }

    fn below_the_field(lines: &[&str]) -> NearbyLines {
        NearbyLines {
            above: Vec::new(),
            below: texts(lines),
        }
    }

    /// Returns the text that the prompt keeps of each line, one screen for each line.
    fn kept_texts<const COUNT: usize>(lines: [&str; COUNT]) -> [Vec<String>; COUNT] {
        lines.map(|text| nearby_lines(&screen(vec![above(text, 1)]), None, BUDGET).above)
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
            above_the_field(&["From: Sam Reyes", "Dinner on Saturday", "See you at 7?"])
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

        assert_eq!(lines, above_the_field(&[&"c".repeat(30)]));
    }

    #[test]
    fn line_that_does_not_fit_leaves_room_for_a_shorter_line() {
        let screen = screen(vec![above(&"a".repeat(60), 1), above("See you at 7?", 2)]);

        let lines = nearby_lines(&screen, None, TokenCount(10));

        assert_eq!(lines, above_the_field(&["See you at 7?"]));
    }

    #[test]
    fn nearest_line_wins_also_below_the_caret() {
        let below = line("> Priya wrote: the budget is final", rect(300, 980, 900, 1010));
        let screen = screen(vec![above("Older message", 2), below]);

        let lines = nearby_lines(&screen, None, TokenCount(15));

        assert_eq!(lines, below_the_field(&["> Priya wrote: the budget is final"]));
    }

    #[test]
    fn line_beside_the_field_ranks_after_the_lines_above_it() {
        let sidebar = line("General channel", rect(0, 920, 250, 940));
        let screen = screen(vec![sidebar, above("Can you send the deck?", 5)]);

        let lines = nearby_lines(&screen, None, TokenCount(10));

        assert_eq!(lines, above_the_field(&["Can you send the deck?"]));
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

        assert_eq!(lines, below_the_field(&["Below the field"]));
    }

    #[test]
    fn whitespace_runs_become_one_space_and_empty_lines_go() {
        let screen = screen(vec![above("  Sure,\n  see\tyou ", 1), above(" \n ", 2)]);

        let lines = nearby_lines(&screen, None, BUDGET);

        assert_eq!(lines, above_the_field(&["Sure, see you"]));
    }

    #[test]
    fn noisy_lines_go() {
        let screen = screen(vec![
            above("|| ~~ == ##", 1),
            above("he1lo w0rld", 2),
            above("#@%& tags", 3),
        ]);

        let lines = nearby_lines(&screen, None, BUDGET);

        assert_eq!(lines, NearbyLines::default());
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

        assert_eq!(lines, above_the_field(&["See you there"]));
    }

    #[test]
    fn line_inside_the_field_goes() {
        let screen = screen(vec![
            above("Dinner at 7?", 1),
            line("Write a message", rect(320, 915, 600, 945)),
        ]);

        let lines = nearby_lines(&screen, None, BUDGET);

        assert_eq!(lines, above_the_field(&["Dinner at 7?"]));
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

        assert_eq!(lines.above.len(), 4, "{lines:?}");
    }

    #[test]
    fn short_line_inside_a_line_of_the_field_stays() {
        let field_text = FieldText {
            before_caret: "Yes, works for me. See you at ".to_owned(),
            after_caret: String::new(),
        };
        let screen = screen(vec![above("Yes", 1)]);

        let lines = nearby_lines(&screen, Some(&field_text), BUDGET);

        assert_eq!(lines, above_the_field(&["Yes"]));
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

        assert_eq!(lines, above_the_field(&["Want to come over?"]));
    }

    #[test]
    fn lines_split_at_the_middle_of_the_caret() {
        let screen = screen(vec![
            line("Sent from my phone", rect(300, 980, 900, 1010)),
            above("See you at 7?", 1),
            line("Sam", rect(0, 915, 250, 935)),
            line("Ana", rect(0, 925, 250, 945)),
        ]);

        let lines = nearby_lines(&screen, None, BUDGET);

        assert_eq!(
            lines,
            NearbyLines {
                above: texts(&["See you at 7?", "Sam"]),
                below: texts(&["Ana", "Sent from my phone"]),
            }
        );
    }

    #[test]
    fn chat_header_keeps_its_name_without_its_time() {
        let headers = [
            ("chips_dev — Today at 21:03", "chips_dev"),
            ("Laura Méndez  11:02", "Laura Méndez"),
            ("nova  10:43 AM", "nova"),
            ("deploy-bot  APP  3:01 pm", "deploy-bot APP"),
            ("ferris_cl — Hoy a las 21:05", "ferris_cl"),
            ("Marco — Yesterday at 9:15 PM", "Marco"),
        ];

        let kept = kept_texts(headers.map(|(header, _)| header));

        assert_eq!(kept, headers.map(|(_, name)| texts(&[name])));
    }

    #[test]
    fn line_with_only_a_time_goes() {
        let kept = kept_texts(["12:31", "Today at 08:02"]);

        assert_eq!(kept, [Vec::<String>::new(), Vec::new()]);
    }

    #[test]
    fn message_keeps_its_time() {
        let messages = [
            "llega a las 8:40",
            "see you at 8:40",
            "8:40 works for me",
            "standup moved to 3:30 today, same room",
            "Cami: el pasaje decia 8:40",
            "nos vemos hoy a las 9:00",
        ];

        let kept = kept_texts(messages);

        assert_eq!(kept, messages.map(|message| texts(&[message])));
    }
}
