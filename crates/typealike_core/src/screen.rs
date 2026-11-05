//! The text near the caret on the screen, as the prompt shows it: the lines of its scope, without
//! noise and without the times of chat headers, the nearest lines first, up to a budget of tokens,
//! above and below the caret.

use std::collections::HashSet;
use std::fmt;

use crate::desktop::{Confidence, FieldText, ScreenLine, ScreenRect, ScreenText, Surroundings};
use crate::excerpt::{estimated_tokens, one_line};
use crate::inference::TokenCount;
use crate::place::Format;

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
/// The fewest words with a letter in the lines of a read of the accessibility tree that the prompt
/// can keep, for a read without an OCR. The tree of an app that draws its text itself, such as the
/// editor of VS Code, often gives only the labels of the app.
const MIN_TREE_WORDS: usize = 40;

/// A line of text that the prompt can show.
struct Candidate {
    text: String,
    frame: ScreenRect,
    /// Whether the line is the header of a chat message. Its text is then the name in the header.
    is_header: bool,
}

/// The lines of the screen that the prompt shows, each side of the caret in reading order. `Debug`
/// shows only the number of lines, so logs never hold the text on the screen.
#[derive(Default, PartialEq, Eq)]
pub(crate) struct NearbyLines {
    /// The lines whose middle is above the middle of the caret, such as the messages of a chat.
    pub(crate) above: Vec<String>,
    /// The other lines, such as the quote of an email below a reply.
    pub(crate) below: Vec<String>,
}

impl fmt::Debug for NearbyLines {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NearbyLines")
            .field("above", &self.above.len())
            .field("below", &self.below.len())
            .finish()
    }
}

/// How far a line is from the caret. Lines beside the field, such as side panels, rank after all
/// lines above and below it: they are often near the caret, but they are seldom a part of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Distance {
    is_beside_field: bool,
    vertical_gap: u32,
}

/// The lines of the screen that the prompt can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineScope {
    /// All the lines of the window, because a panel beside a form or a document, such as its help,
    /// can tell what the field needs.
    Window,
    /// The lines in the column of the field. Beside the column, a chat or an email app shows the
    /// list of the other conversations, which is not a part of the conversation in the field.
    Column,
}

impl LineScope {
    /// Returns the scope of the field in `surroundings`: its column in a chat or an email app, else
    /// the window.
    pub(crate) fn of(surroundings: &Surroundings) -> Self {
        match Format::of(surroundings) {
            Some(Format::Chat | Format::Email) => Self::Column,
            Some(Format::Document | Format::WebPage) | None => Self::Window,
        }
    }

    /// Returns the lines of `screen` in the scope. A field without width has no column, so all
    /// its lines are in the scope.
    pub(crate) fn lines(self, screen: &ScreenText) -> impl Iterator<Item = &ScreenLine> {
        let column = match self {
            Self::Window => None,
            Self::Column => Column::of(screen),
        };
        screen
            .lines
            .iter()
            .filter(move |line| column.is_none_or(|column| column.holds(line.frame)))
    }
}

/// The span from left to right of the field and of the lines that reach into that span, such as
/// the messages of a chat that start left of its input box. A line wholly left or right of the
/// column is beside the field, such as a sidebar or the list of the other chats.
#[derive(Clone, Copy)]
struct Column {
    left: i32,
    right: i32,
}

impl Column {
    /// Returns the column of the field of `screen`, or `None` when the field has no width.
    fn of(screen: &ScreenText) -> Option<Self> {
        let field = Self {
            left: screen.field.left,
            right: screen.field.right,
        };
        if field.left >= field.right {
            return None;
        }
        let reaching_lines = screen.lines.iter().filter(|line| field.holds(line.frame));
        Some(reaching_lines.fold(field, |column, line| Self {
            left: column.left.min(line.frame.left),
            right: column.right.max(line.frame.right),
        }))
    }

    /// Whether a part of `frame` is in the column.
    fn holds(self, frame: ScreenRect) -> bool {
        frame.left < self.right && self.left < frame.right
    }
}

/// Returns the text of the lines of `screen` in `scope` that the prompt shows, above and below the
/// caret: the nearest lines to the caret that fit in `budget`, without the noise and the lines that
/// repeat `field_text`.
pub(crate) fn nearby_lines(
    screen: &ScreenText,
    field_text: Option<&FieldText>,
    budget: TokenCount,
    scope: LineScope,
) -> NearbyLines {
    let caret = caret_of(screen);
    let mut candidates = candidates(screen, field_text, scope);
    candidates.sort_by_key(|candidate| {
        (
            distance(candidate.frame, caret, screen.field),
            candidate.frame.top,
            candidate.frame.left,
        )
    });
    let mut kept = within_budget(candidates, budget);
    kept.sort_by_key(|candidate| (candidate.frame.top, candidate.frame.left));
    let (above, below) = kept
        .into_iter()
        .partition::<Vec<_>, _>(|candidate| is_above(candidate.frame, caret));
    NearbyLines {
        above: above.into_iter().map(|candidate| candidate.text).collect(),
        below: below.into_iter().map(|candidate| candidate.text).collect(),
    }
}

/// Returns the text of the lines of `screen` in the column of the field above the caret, in
/// reading order, without the chat headers, the noise and the lines that repeat `field_text`: the
/// messages above the input box of a chat.
pub(crate) fn message_lines_above(screen: &ScreenText, field_text: Option<&FieldText>) -> Vec<String> {
    let caret = caret_of(screen);
    let mut messages = candidates(screen, field_text, LineScope::Column)
        .into_iter()
        .filter(|candidate| !candidate.is_header && is_above(candidate.frame, caret))
        .collect::<Vec<_>>();
    messages.sort_by_key(|candidate| (candidate.frame.top, candidate.frame.left));
    messages.into_iter().map(|candidate| candidate.text).collect()
}

/// Whether the lines of `screen` that the prompt can keep for the field of `surroundings` hold at
/// least [`MIN_TREE_WORDS`] words with a letter.
pub(crate) fn has_enough_words(screen: &ScreenText, surroundings: &Surroundings) -> bool {
    let candidates = candidates(
        screen,
        surroundings.field_text.as_ref(),
        LineScope::of(surroundings),
    );
    let words = candidates
        .iter()
        .flat_map(|candidate| candidate.text.split_whitespace())
        .filter(|word| word.chars().any(char::is_alphabetic));
    words.take(MIN_TREE_WORDS).count() == MIN_TREE_WORDS
}

/// Returns the lines of `screen` in `scope` that the prompt can show: without the noise, the lines
/// inside the field, the lines that repeat `field_text` and the second copy of a line with the
/// same text and frame, which the tree of an Electron app gives. A chat header gives only its name.
fn candidates(screen: &ScreenText, field_text: Option<&FieldText>, scope: LineScope) -> Vec<Candidate> {
    let field = FieldLines::of(field_text);
    let mut seen = HashSet::new();
    scope
        .lines(screen)
        .filter(|line| seen.insert((line.text.as_str(), line.frame)))
        .filter(|line| line.confidence >= MIN_CONFIDENCE && !is_inside(line.frame, screen.field))
        .map(|line| {
            let text = one_line(&line.text);
            match header_name(&text) {
                Some(name) => Candidate {
                    text: name,
                    frame: line.frame,
                    is_header: true,
                },
                None => Candidate {
                    text,
                    frame: line.frame,
                    is_header: false,
                },
            }
        })
        .filter(|candidate| {
            !candidate.text.is_empty() && !is_noise(&candidate.text) && !field.repeats(&candidate.text)
        })
        .collect()
}

/// Returns the caret of `screen`, or the frame of its field when the caret is not known.
fn caret_of(screen: &ScreenText) -> ScreenRect {
    screen.caret.unwrap_or(screen.field)
}

/// Whether the middle of `line` is above the middle of `caret`.
fn is_above(line: ScreenRect, caret: ScreenRect) -> bool {
    line.top.midpoint(line.bottom) < caret.top.midpoint(caret.bottom)
}

/// Returns the `candidates` whose tokens fit in what is left of `budget`, in their order.
fn within_budget(candidates: Vec<Candidate>, budget: TokenCount) -> Vec<Candidate> {
    let mut left = budget.0;
    candidates
        .into_iter()
        .filter(|candidate| {
            let Some(rest) = left.checked_sub(estimated_tokens(&candidate.text).0) else {
                return false;
            };
            left = rest;
            true
        })
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

/// Whether `line` looks like noise: many signs, or a digit between two lowercase letters of a word,
/// as text recognition makes them. A digit between capital letters is often a code, such as a
/// booking code, which the user can need.
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
    signs.saturating_mul(100) > characters.saturating_mul(MAX_SIGN_PERCENT)
        || has_digit_inside_a_lowercase_word(line)
}

fn has_digit_inside_a_lowercase_word(line: &str) -> bool {
    line.chars()
        .collect::<Vec<_>>()
        .windows(3)
        .any(|run| run[0].is_lowercase() && run[1].is_numeric() && run[2].is_lowercase())
}

/// Returns the name of a chat header: at most [`MAX_HEADER_NAME_WORDS`] words, then a time with
/// maybe a day before it, as in "Laura Méndez 11:02" or "`chips_dev` — Today at 21:03". The name of
/// a line with only a time is empty. `None` when `line` is not a chat header. A message keeps its
/// times, because a time is often what it tells.
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
        lines.map(|text| nearby_lines(&screen(vec![above(text, 1)]), None, BUDGET, LineScope::Window).above)
    }

    #[test]
    fn debug_of_the_nearby_lines_hides_the_text() {
        let lines = NearbyLines {
            above: texts(&["From: Sam Reyes", "See you at 7?"]),
            below: texts(&["Sent from my phone"]),
        };

        let debug = format!("{lines:?}");

        assert_eq!(debug, "NearbyLines { above: 2, below: 1 }");
    }

    #[test]
    fn lines_show_in_reading_order() {
        let screen = screen(vec![
            above("See you at 7?", 1),
            above("From: Sam Reyes", 3),
            above("Dinner on Saturday", 2),
        ]);

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Window);

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

        let lines = nearby_lines(&screen, None, TokenCount(15), LineScope::Window);

        assert_eq!(lines, above_the_field(&[&"c".repeat(30)]));
    }

    #[test]
    fn line_that_does_not_fit_leaves_room_for_a_shorter_line() {
        let screen = screen(vec![above(&"a".repeat(60), 1), above("See you at 7?", 2)]);

        let lines = nearby_lines(&screen, None, TokenCount(10), LineScope::Window);

        assert_eq!(lines, above_the_field(&["See you at 7?"]));
    }

    #[test]
    fn nearest_line_wins_also_below_the_caret() {
        let below = line("> Priya wrote: the budget is final", rect(300, 980, 900, 1010));
        let screen = screen(vec![above("Older message", 2), below]);

        let lines = nearby_lines(&screen, None, TokenCount(15), LineScope::Window);

        assert_eq!(lines, below_the_field(&["> Priya wrote: the budget is final"]));
    }

    #[test]
    fn line_beside_the_field_ranks_after_the_lines_above_it() {
        let sidebar = line("General channel", rect(0, 920, 250, 940));
        let screen = screen(vec![sidebar, above("Can you send the deck?", 5)]);

        let lines = nearby_lines(&screen, None, TokenCount(10), LineScope::Window);

        assert_eq!(lines, above_the_field(&["Can you send the deck?"]));
    }

    #[test]
    fn line_beside_the_column_of_the_field_goes() {
        let sidebar = line("General channel", rect(0, 920, 250, 940));
        let screen = screen(vec![sidebar, above("Can you send the deck?", 5)]);

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Column);

        assert_eq!(lines, above_the_field(&["Can you send the deck?"]));
    }

    #[test]
    fn short_message_left_of_the_field_stays_when_a_message_reaches_into_the_field() {
        let long = line("Can you send the deck before the call?", rect(200, 780, 900, 810));
        let short = line("Thanks", rect(200, 820, 280, 850));
        let screen = screen(vec![long, short]);

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Column);

        assert_eq!(
            lines,
            above_the_field(&["Can you send the deck before the call?", "Thanks"])
        );
    }

    #[test]
    fn chat_and_email_apps_show_the_column_and_other_places_the_window() {
        let scope_of = |app_id: &str| {
            LineScope::of(&Surroundings {
                app_id: Some(app_id.to_owned()),
                ..Surroundings::default()
            })
        };

        let scopes = [
            "com.tinyspeck.slackmacgap",
            "com.apple.mail",
            "com.microsoft.Word",
            "notepad.exe",
        ]
        .map(scope_of);

        assert_eq!(
            scopes,
            [
                LineScope::Column,
                LineScope::Column,
                LineScope::Window,
                LineScope::Window
            ]
        );
    }

    #[test]
    fn field_without_width_keeps_the_lines_beside_it() {
        let screen = ScreenText {
            lines: vec![line("General channel", rect(0, 100, 250, 120))],
            caret: Some(CARET),
            field: rect(300, 900, 300, 960),
        };

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Column);

        assert_eq!(lines, above_the_field(&["General channel"]));
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

        let lines = nearby_lines(&screen, None, TokenCount(6), LineScope::Window);

        assert_eq!(lines, below_the_field(&["Below the field"]));
    }

    #[test]
    fn whitespace_runs_become_one_space_and_empty_lines_go() {
        let screen = screen(vec![above("  Sure,\n  see\tyou ", 1), above(" \n ", 2)]);

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Window);

        assert_eq!(lines, above_the_field(&["Sure, see you"]));
    }

    #[test]
    fn noisy_lines_go() {
        let screen = screen(vec![
            above("|| ~~ == ##", 1),
            above("he1lo w0rld", 2),
            above("#@%& tags", 3),
        ]);

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Window);

        assert_eq!(lines, NearbyLines::default());
    }

    #[test]
    fn line_with_a_code_of_capital_letters_and_digits_stays() {
        let screen = screen(vec![above("Your booking code is KX7P2Q", 1)]);

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Window);

        assert_eq!(lines, above_the_field(&["Your booking code is KX7P2Q"]));
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

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Window);

        assert_eq!(lines, above_the_field(&["See you there"]));
    }

    #[test]
    fn line_that_the_platform_gives_twice_shows_once() {
        let screen = screen(vec![above("EXPLORER", 2), above("EXPLORER", 2), above("Fine", 1)]);

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Window);

        assert_eq!(lines, above_the_field(&["EXPLORER", "Fine"]));
    }

    #[test]
    fn same_text_at_two_places_shows_twice() {
        let screen = screen(vec![above("Fine", 2), above("Fine", 1)]);

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Window);

        assert_eq!(lines, above_the_field(&["Fine", "Fine"]));
    }

    #[test]
    fn line_inside_the_field_goes() {
        let screen = screen(vec![
            above("Dinner at 7?", 1),
            line("Write a message", rect(320, 915, 600, 945)),
        ]);

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Window);

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

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Window);

        assert_eq!(lines.above.len(), 4, "{lines:?}");
    }

    #[test]
    fn short_line_inside_a_line_of_the_field_stays() {
        let field_text = FieldText {
            before_caret: "Yes, works for me. See you at ".to_owned(),
            after_caret: String::new(),
        };
        let screen = screen(vec![above("Yes", 1)]);

        let lines = nearby_lines(&screen, Some(&field_text), BUDGET, LineScope::Window);

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

        let lines = nearby_lines(&screen, Some(&field_text), BUDGET, LineScope::Window);

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

        let lines = nearby_lines(&screen, None, BUDGET, LineScope::Window);

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

    #[test]
    fn message_lines_are_the_column_lines_above_the_caret_without_the_headers() {
        let screen = screen(vec![
            line("General channel", rect(0, 700, 250, 730)),
            above("Dana Kim 9:58", 4),
            above("Standup in 5?", 3),
            above("same room as yesterday", 2),
            line("Sent from my phone", rect(300, 980, 900, 1000)),
        ]);

        let lines = message_lines_above(&screen, None);

        assert_eq!(lines, texts(&["Standup in 5?", "same room as yesterday"]));
    }
}
