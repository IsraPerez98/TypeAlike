//! The text near the caret on the screen, as the prompt shows it: the lines of its scope, without
//! noise and without the times of chat headers and messages, the nearest lines first, up to a
//! budget of tokens, above and below the caret.

mod conversation;

pub(crate) use conversation::ConversationLines;

use std::collections::HashSet;
use std::fmt;

use crate::desktop::{Confidence, FieldText, OcrArea, ScreenLine, ScreenRect, ScreenText, Surroundings};
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
/// The tokens that the lines of the last prompt in a chat can use above the budget, in percent of
/// the budget. Thus new messages do not change the start of the prompt at each read.
const KEPT_LINES_SLACK_PERCENT: u32 = 25;

/// What the prompt knows of the text of the focused field, for the lines of the screen.
#[derive(Debug, Clone, Copy)]
pub(crate) enum FieldContent<'text> {
    /// The platform reads the text of the field, if the field has text.
    Read(Option<&'text FieldText>),
    /// The field hides its text from the platform, so its lines on the screen are its text, such as
    /// the line of the caret in the editor of VS Code, or the screen of a terminal without a caret.
    Hidden {
        /// The text that the user typed before the caret, which a line of the field can show again.
        typed: &'text str,
    },
}

impl<'text> FieldContent<'text> {
    /// Returns what the prompt knows of the text of the field of `surroundings`, where the user
    /// typed `typed` before the caret.
    pub(crate) fn of(surroundings: &'text Surroundings, typed: &'text str) -> Self {
        if surroundings.hides_field_text() {
            Self::Hidden { typed }
        } else {
            Self::Read(surroundings.field_text.as_ref())
        }
    }

    fn text(self) -> Option<&'text FieldText> {
        match self {
            Self::Read(text) => text,
            Self::Hidden { .. } => None,
        }
    }

    /// Whether a line at `frame`, inside `field` on the screen, holds text of the field.
    fn holds(self, frame: ScreenRect, field: ScreenRect) -> bool {
        matches!(self, Self::Hidden { .. }) && is_inside(frame, field)
    }

    /// Whether the line `text` at `frame`, inside `field` on the screen, shows a part of the typed
    /// text of a field that hides its text. The signs at its start, such as the prompt mark of a
    /// terminal, do not count.
    fn shows_typed_text(self, text: &str, frame: ScreenRect, field: ScreenRect) -> bool {
        let Self::Hidden { typed } = self else {
            return false;
        };
        let words = text
            .trim_start_matches(|character: char| !character.is_alphanumeric())
            .trim_end();
        !words.is_empty() && is_inside(frame, field) && one_line(typed).contains(words)
    }

    /// Returns the frame of the last line of `screen` that shows the typed text of a field that
    /// hides its text, which is the line of the caret when the platform does not know the caret.
    fn typed_line(self, screen: &ScreenText) -> Option<ScreenRect> {
        screen
            .lines
            .iter()
            .filter(|line| self.shows_typed_text(&one_line(&line.text), line.frame, screen.field))
            .map(|line| line.frame)
            .max_by_key(|frame| (frame.top, frame.left))
    }
}

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

/// Returns the part of the window that an OCR reads for the field of `surroundings`. A browser
/// shows its tabs, its address bar and its bookmarks around the page, so the OCR reads the content
/// of the field there, and also outside of a chat or an email app. In a chat or an email app, the
/// name in the header of a chat or the address of an email can be outside of that content.
pub(crate) fn ocr_area(surroundings: &Surroundings) -> OcrArea {
    let is_browser = Format::of_place(surroundings.app_id.as_deref(), None) == Some(Format::WebPage);
    if is_browser || LineScope::of(surroundings) == LineScope::Window {
        OcrArea::Content
    } else {
        OcrArea::Window
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
/// repeat the text of `field`. The lines of a field that hides its text count as above the caret.
pub(crate) fn nearby_lines(
    screen: &ScreenText,
    field: FieldContent<'_>,
    budget: TokenCount,
    scope: LineScope,
) -> NearbyLines {
    let lines = in_reading_order(screen, field, candidates(screen, field, scope));
    let nearest = within_budget(&lines, &by_distance(screen, field, &lines), budget);
    split_at_caret(screen, field, lines, &nearest)
}

/// Returns the text of the lines of `screen` in the column of the field that the prompt shows,
/// after a prompt in the same field that showed `shown_above`. These lines stay first while they
/// are still on the screen in the same order, and the lines of [`nearby_lines`] after them follow.
/// Thus a new chat message does not change the start of the prompt. The lines of
/// [`nearby_lines`] come again when these lines need more than [`KEPT_LINES_SLACK_PERCENT`] above
/// `budget`. They also come again when these lines leave out a line that [`nearby_lines`] keeps
/// before its first line that does not fit.
pub(crate) fn nearby_lines_from(
    shown_above: &[String],
    screen: &ScreenText,
    field: FieldContent<'_>,
    budget: TokenCount,
) -> NearbyLines {
    let lines = in_reading_order(screen, field, candidates(screen, field, LineScope::Column));
    let ranked = by_distance(screen, field, &lines);
    let nearest = within_budget(&lines, &ranked, budget);
    let kept = shown_and_newer(shown_above, &lines, &nearest)
        .filter(|kept| tokens_of(&lines, kept) <= with_slack(budget))
        .filter(|kept| first_fitting(&lines, &ranked, budget).is_subset(kept))
        .unwrap_or(nearest);
    split_at_caret(screen, field, lines, &kept)
}

/// Returns the text of the lines of `screen` in the column of the field above the caret, in
/// reading order, without the chat headers, the noise and the lines that repeat the text of
/// `field`: the messages above the input box of a chat.
pub(crate) fn message_lines_above(screen: &ScreenText, field: FieldContent<'_>) -> Vec<String> {
    let caret = caret_of(screen, field);
    let mut messages = candidates(screen, field, LineScope::Column)
        .into_iter()
        .filter(|candidate| !candidate.is_header && is_above(candidate.frame, caret))
        .collect::<Vec<_>>();
    messages.sort_by_key(|candidate| (candidate.frame.top, candidate.frame.left));
    messages.into_iter().map(|candidate| candidate.text).collect()
}

/// Whether the lines of `screen` that the prompt can keep for the field of `surroundings` hold at
/// least [`MIN_TREE_WORDS`] words with a letter. A field that hides its text needs the lines of an
/// OCR, which hold its text.
pub(crate) fn has_enough_words(screen: &ScreenText, surroundings: &Surroundings) -> bool {
    if surroundings.hides_field_text() {
        return false;
    }
    let candidates = candidates(
        screen,
        FieldContent::of(surroundings, ""),
        LineScope::of(surroundings),
    );
    let words = candidates
        .iter()
        .flat_map(|candidate| candidate.text.split_whitespace())
        .filter(|word| word.chars().any(char::is_alphabetic));
    words.take(MIN_TREE_WORDS).count() == MIN_TREE_WORDS
}

/// Returns the lines of `screen` in `scope` that the prompt can show: without the noise, the lines
/// inside a field that gives its text, the lines that repeat the text of `field`, the lines without
/// a letter beside the field, such as line numbers, and the second copy of a line with the same
/// text and frame, which the tree of an Electron app gives. A chat header gives only its name.
fn candidates(screen: &ScreenText, field: FieldContent<'_>, scope: LineScope) -> Vec<Candidate> {
    let field_lines = FieldLines::of(field.text());
    let mut seen = HashSet::new();
    let lines = scope
        .lines(screen)
        .filter(|line| seen.insert((line.text.as_str(), line.frame)))
        .filter(|line| line.confidence >= MIN_CONFIDENCE)
        .filter(|line| field.holds(line.frame, screen.field) || !is_inside(line.frame, screen.field))
        .filter(|line| !is_beside(line.frame, screen.field) || line.text.chars().any(char::is_alphabetic))
        .collect::<Vec<_>>();
    let message_times = MessageTimes::of(&lines, scope);
    lines
        .into_iter()
        .map(|line| {
            let text = message_times.without_time_of_message(one_line(&line.text), line.frame);
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
            !candidate.text.is_empty()
                && !is_noise(&candidate.text)
                && !field_lines.repeats(&candidate.text)
                && !field.shows_typed_text(&candidate.text, candidate.frame, screen.field)
        })
        .collect()
}

/// Returns the caret of `screen`, else the line that shows the typed text of a field that hides
/// its text, else the frame of the field.
fn caret_of(screen: &ScreenText, field: FieldContent<'_>) -> ScreenRect {
    screen
        .caret
        .or_else(|| field.typed_line(screen))
        .unwrap_or(screen.field)
}

/// Whether the middle of `line` is above the middle of `caret`.
fn is_above(line: ScreenRect, caret: ScreenRect) -> bool {
    line.top.midpoint(line.bottom) < caret.top.midpoint(caret.bottom)
}

/// Whether the middle of `line` is below the row of `caret`.
fn is_below_row(line: ScreenRect, caret: ScreenRect) -> bool {
    line.top.midpoint(line.bottom) >= caret.bottom
}

/// Returns `lines` from top to bottom, then from left to right, then the nearest to the caret of
/// `screen` first. The lines of a field that hides its text come after the other lines, so that
/// they lead to the caret, also when a panel beside the field has lines nearer to the caret row.
fn in_reading_order(
    screen: &ScreenText,
    field: FieldContent<'_>,
    mut lines: Vec<Candidate>,
) -> Vec<Candidate> {
    let caret = caret_of(screen, field);
    lines.sort_by_key(|line| {
        (
            field.holds(line.frame, screen.field),
            line.frame.top,
            line.frame.left,
            distance(line.frame, caret, screen.field),
        )
    });
    lines
}

/// Returns the positions of `lines`, the nearest to the caret of `screen` first. Lines at the same
/// distance keep their order.
fn by_distance(screen: &ScreenText, field: FieldContent<'_>, lines: &[Candidate]) -> Vec<usize> {
    let caret = caret_of(screen, field);
    let mut ranked = (0..lines.len()).collect::<Vec<_>>();
    ranked.sort_by_key(|&position| distance(lines[position].frame, caret, screen.field));
    ranked
}

/// Returns the positions of `ranked` whose lines fit in what is left of `budget`, each in turn.
fn within_budget(lines: &[Candidate], ranked: &[usize], budget: TokenCount) -> HashSet<usize> {
    let mut left = budget.0;
    ranked
        .iter()
        .copied()
        .filter(|&position| {
            let Some(rest) = left.checked_sub(estimated_tokens(&lines[position].text).0) else {
                return false;
            };
            left = rest;
            true
        })
        .collect()
}

/// Returns the first positions of `ranked` whose lines fit in `budget`, up to the first line that
/// does not fit.
fn first_fitting(lines: &[Candidate], ranked: &[usize], budget: TokenCount) -> HashSet<usize> {
    let mut left = budget.0;
    ranked
        .iter()
        .copied()
        .map_while(|position| {
            left = left.checked_sub(estimated_tokens(&lines[position].text).0)?;
            Some(position)
        })
        .collect()
}

/// Returns the positions of the `lines` that show `shown` again in the same order, and of the
/// `nearest` lines after them. Each line of `shown` takes the last line with its text before the
/// line that `shown` has after it. `None` when `shown` is empty or a line of it is not in `lines`.
fn shown_and_newer(
    shown: &[String],
    lines: &[Candidate],
    nearest: &HashSet<usize>,
) -> Option<HashSet<usize>> {
    let mut end = lines.len();
    let shown_again = shown
        .iter()
        .rev()
        .map(|text| {
            end = lines[..end].iter().rposition(|line| line.text == *text)?;
            Some(end)
        })
        .collect::<Option<Vec<_>>>()?;
    let &last_shown = shown_again.first()?;
    let newer = nearest.iter().copied().filter(|&position| position > last_shown);
    Some(shown_again.into_iter().chain(newer).collect())
}

fn tokens_of(lines: &[Candidate], kept: &HashSet<usize>) -> u32 {
    kept.iter()
        .map(|&position| estimated_tokens(&lines[position].text).0)
        .fold(0, u32::saturating_add)
}

/// Returns `budget` with [`KEPT_LINES_SLACK_PERCENT`] more.
fn with_slack(budget: TokenCount) -> u32 {
    budget
        .0
        .saturating_add(budget.0.saturating_mul(KEPT_LINES_SLACK_PERCENT) / 100)
}

/// Returns the text of the `kept` positions of `lines`, each side of the caret of `screen` in
/// reading order. The lines of a field that hides its text count as above the caret, also on the
/// row of the caret, but the lines of the field below that row are below the caret.
fn split_at_caret(
    screen: &ScreenText,
    field: FieldContent<'_>,
    lines: Vec<Candidate>,
    kept: &HashSet<usize>,
) -> NearbyLines {
    let caret = caret_of(screen, field);
    let (above, below) = lines
        .into_iter()
        .enumerate()
        .filter_map(|(position, line)| kept.contains(&position).then_some(line))
        .partition::<Vec<_>, _>(|line| {
            is_above(line.frame, caret)
                || (field.holds(line.frame, screen.field) && !is_below_row(line.frame, caret))
        });
    NearbyLines {
        above: above.into_iter().map(|line| line.text).collect(),
        below: below.into_iter().map(|line| line.text).collect(),
    }
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
        is_beside_field: is_beside(line, field),
        vertical_gap,
    }
}

/// Whether `line` is wholly left or right of `field`, as a sidebar or the line numbers of an
/// editor are.
fn is_beside(line: ScreenRect, field: ScreenRect) -> bool {
    line.right <= field.left || field.right <= line.left
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

/// The frames of the lines of a chat or an email app that hold only a time. Such lines show that
/// the app gives the time of each message at the end of its bubble, as chats on a phone do.
/// Text recognition reads that time as a line of its own when a gap divides it from the message,
/// else as the end of the last line of the message.
struct MessageTimes(Vec<ScreenRect>);

impl MessageTimes {
    fn of(lines: &[&ScreenLine], scope: LineScope) -> Self {
        let frames = match scope {
            LineScope::Window => Vec::new(),
            LineScope::Column => lines
                .iter()
                .filter(|line| without_time_at_end(&one_line(&line.text)).is_some_and(|rest| rest.is_empty()))
                .map(|line| line.frame)
                .collect(),
        };
        Self(frames)
    }

    /// Returns `text` of the line at `frame` without the time of its message at its end. The line
    /// keeps its time when the app shows no times of messages, or when the time of its message is
    /// a line of its own on the same row: then the time at its end is a part of the message.
    fn without_time_of_message(&self, text: String, frame: ScreenRect) -> String {
        if self.0.is_empty() || self.0.iter().any(|&time| is_on_the_row_of(time, frame)) {
            return text;
        }
        without_time_at_end(&text).unwrap_or(text)
    }
}

/// Returns `line` without the time at its end, as "9:57 PM" or "21:57". `None` when `line` does
/// not end with a time.
fn without_time_at_end(line: &str) -> Option<String> {
    let mut words = line.split(' ').collect::<Vec<_>>();
    words.pop_if(|word| is_any_of(word, &HALVES_OF_DAY));
    words.pop_if(|word| is_time(word))?;
    Some(words.join(" ").trim_end().to_owned())
}

/// Whether the middle of `line` is between the top and the bottom of `row`.
fn is_on_the_row_of(line: ScreenRect, row: ScreenRect) -> bool {
    (row.top..row.bottom).contains(&line.top.midpoint(line.bottom))
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
    /// The budget of four chat messages.
    const CHAT_BUDGET: TokenCount = TokenCount(40);
    /// Chat messages of 10 estimated tokens each, oldest first.
    const CABIN: &str = "Did you book the cabin by now?";
    const FRIDAY: &str = "Yes, it is ours from Friday on";
    const KAYAKS: &str = "Great, I can bring the kayaks.";
    const COOLER: &str = "Bring the big cooler too, okay";
    const DRIVER: &str = "Sure, who drives us on Friday?";
    const SIX: &str = "I can drive, I leave at six pm";
    const SEE_YOU: &str = "Perfect, see you all at six pm";

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

    /// The time of a chat message in a line of its own, at the right end of the bubble `rows`
    /// lines above the field, as text recognition reads it when a gap divides it from the message.
    fn message_time(text: &str, rows: i32) -> ScreenLine {
        line(text, rect(1000, 902 - 40 * rows, 1060, 928 - 40 * rows))
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

    /// Returns the lines that a chat shows with [`CHAT_BUDGET`] after a prompt that showed
    /// `shown_above`, each line of `messages` `rows` lines above the field.
    fn chat_lines_after(shown_above: &[&str], messages: &[(&str, i32)]) -> NearbyLines {
        let lines = messages.iter().map(|&(text, rows)| above(text, rows)).collect();
        nearby_lines_from(
            &texts(shown_above),
            &screen(lines),
            FieldContent::Read(None),
            CHAT_BUDGET,
        )
    }

    /// Returns the text that the prompt keeps of each line, one screen for each line.
    fn kept_texts<const COUNT: usize>(lines: [&str; COUNT]) -> [Vec<String>; COUNT] {
        lines.map(|text| {
            nearby_lines(
                &screen(vec![above(text, 1)]),
                FieldContent::Read(None),
                BUDGET,
                LineScope::Window,
            )
            .above
        })
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

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Window);

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

        let lines = nearby_lines(
            &screen,
            FieldContent::Read(None),
            TokenCount(15),
            LineScope::Window,
        );

        assert_eq!(lines, above_the_field(&[&"c".repeat(30)]));
    }

    #[test]
    fn line_that_does_not_fit_leaves_room_for_a_shorter_line() {
        let screen = screen(vec![above(&"a".repeat(60), 1), above("See you at 7?", 2)]);

        let lines = nearby_lines(
            &screen,
            FieldContent::Read(None),
            TokenCount(10),
            LineScope::Window,
        );

        assert_eq!(lines, above_the_field(&["See you at 7?"]));
    }

    #[test]
    fn nearest_line_wins_also_below_the_caret() {
        let below = line("> Priya wrote: the budget is final", rect(300, 980, 900, 1010));
        let screen = screen(vec![above("Older message", 2), below]);

        let lines = nearby_lines(
            &screen,
            FieldContent::Read(None),
            TokenCount(15),
            LineScope::Window,
        );

        assert_eq!(lines, below_the_field(&["> Priya wrote: the budget is final"]));
    }

    #[test]
    fn line_beside_the_field_ranks_after_the_lines_above_it() {
        let sidebar = line("General channel", rect(0, 920, 250, 940));
        let screen = screen(vec![sidebar, above("Can you send the deck?", 5)]);

        let lines = nearby_lines(
            &screen,
            FieldContent::Read(None),
            TokenCount(10),
            LineScope::Window,
        );

        assert_eq!(lines, above_the_field(&["Can you send the deck?"]));
    }

    #[test]
    fn line_beside_the_column_of_the_field_goes() {
        let sidebar = line("General channel", rect(0, 920, 250, 940));
        let screen = screen(vec![sidebar, above("Can you send the deck?", 5)]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Column);

        assert_eq!(lines, above_the_field(&["Can you send the deck?"]));
    }

    #[test]
    fn short_message_left_of_the_field_stays_when_a_message_reaches_into_the_field() {
        let long = line("Can you send the deck before the call?", rect(200, 780, 900, 810));
        let short = line("Thanks", rect(200, 820, 280, 850));
        let screen = screen(vec![long, short]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Column);

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

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Column);

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

        let lines = nearby_lines(
            &screen,
            FieldContent::Read(None),
            TokenCount(6),
            LineScope::Window,
        );

        assert_eq!(lines, below_the_field(&["Below the field"]));
    }

    #[test]
    fn whitespace_runs_become_one_space_and_empty_lines_go() {
        let screen = screen(vec![above("  Sure,\n  see\tyou ", 1), above(" \n ", 2)]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Window);

        assert_eq!(lines, above_the_field(&["Sure, see you"]));
    }

    #[test]
    fn noisy_lines_go() {
        let screen = screen(vec![
            above("|| ~~ == ##", 1),
            above("he1lo w0rld", 2),
            above("#@%& tags", 3),
        ]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Window);

        assert_eq!(lines, NearbyLines::default());
    }

    #[test]
    fn line_with_a_code_of_capital_letters_and_digits_stays() {
        let screen = screen(vec![above("Your booking code is KX7P2Q", 1)]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Window);

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

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Window);

        assert_eq!(lines, above_the_field(&["See you there"]));
    }

    #[test]
    fn line_that_the_platform_gives_twice_shows_once() {
        let screen = screen(vec![above("EXPLORER", 2), above("EXPLORER", 2), above("Fine", 1)]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Window);

        assert_eq!(lines, above_the_field(&["EXPLORER", "Fine"]));
    }

    #[test]
    fn same_text_at_two_places_shows_twice() {
        let screen = screen(vec![above("Fine", 2), above("Fine", 1)]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Window);

        assert_eq!(lines, above_the_field(&["Fine", "Fine"]));
    }

    #[test]
    fn line_inside_a_field_that_hides_its_text_is_the_last_line_above() {
        let screen = screen(vec![
            above("41. Parking is free after six.", 1),
            line("and QA signs off on", rect(320, 915, 600, 945)),
        ]);

        let lines = nearby_lines(
            &screen,
            FieldContent::Hidden { typed: "" },
            BUDGET,
            LineScope::Window,
        );

        assert_eq!(
            lines,
            above_the_field(&["41. Parking is free after six.", "and QA signs off on"])
        );
    }

    const TERMINAL: ScreenRect = rect(0, 0, 1000, 800);
    const TYPED_PROMPT: &str = "fix the login bug in the auth module and then add a test for it ";

    /// A row of a terminal whose whole screen is a field without a caret, `index` rows from the top.
    fn terminal_row(text: &str, index: i32) -> ScreenLine {
        line(text, rect(10, 20 * index, 990, 20 * index + 16))
    }

    fn terminal(lines: Vec<ScreenLine>) -> ScreenText {
        ScreenText {
            lines,
            caret: None,
            field: TERMINAL,
        }
    }

    #[test]
    fn rows_of_a_hidden_field_that_show_the_typed_text_after_a_marker_go() {
        let screen = terminal(vec![
            terminal_row("I changed login.rs and the tests pass.", 0),
            terminal_row("❯ fix the login bug in the auth module and then", 1),
            terminal_row("add a test for it", 2),
        ]);

        let lines = nearby_lines(
            &screen,
            FieldContent::Hidden { typed: TYPED_PROMPT },
            BUDGET,
            LineScope::Window,
        );

        assert_eq!(
            lines,
            above_the_field(&["I changed login.rs and the tests pass."])
        );
    }

    #[test]
    fn rows_after_the_typed_text_of_a_hidden_field_are_below_the_caret() {
        let screen = terminal(vec![
            terminal_row("I changed login.rs and the tests pass.", 0),
            terminal_row("❯ fix the login bug in the auth module and then", 1),
            terminal_row("add a test for it", 2),
            terminal_row("auto mode on", 3),
        ]);

        let lines = nearby_lines(
            &screen,
            FieldContent::Hidden { typed: TYPED_PROMPT },
            BUDGET,
            LineScope::Window,
        );

        assert_eq!(
            lines,
            NearbyLines {
                above: texts(&["I changed login.rs and the tests pass."]),
                below: texts(&["auto mode on"]),
            }
        );
    }

    #[test]
    fn rows_of_a_hidden_field_below_a_known_caret_are_below_it() {
        let screen = ScreenText {
            caret: Some(rect(200, 20, 200, 36)),
            ..terminal(vec![
                terminal_row("I changed login.rs and the tests pass.", 0),
                terminal_row("❯ fix the login bug in the auth", 1),
                terminal_row("auto mode on", 3),
            ])
        };

        let lines = nearby_lines(
            &screen,
            FieldContent::Hidden { typed: "module" },
            BUDGET,
            LineScope::Window,
        );

        assert_eq!(
            lines,
            NearbyLines {
                above: texts(&[
                    "I changed login.rs and the tests pass.",
                    "❯ fix the login bug in the auth"
                ]),
                below: texts(&["auto mode on"]),
            }
        );
    }

    #[test]
    fn rows_of_a_hidden_field_come_after_the_other_lines_above_the_caret() {
        let screen = ScreenText {
            lines: vec![
                line("I changed login.rs and the tests pass.", rect(310, 100, 990, 116)),
                line("chore: release 0.5.0", rect(0, 140, 290, 156)),
            ],
            caret: Some(rect(320, 180, 320, 196)),
            field: rect(300, 0, 1000, 800),
        };

        let lines = nearby_lines(
            &screen,
            FieldContent::Hidden { typed: TYPED_PROMPT },
            BUDGET,
            LineScope::Window,
        );

        assert_eq!(
            lines,
            above_the_field(&["chore: release 0.5.0", "I changed login.rs and the tests pass."])
        );
    }

    #[test]
    fn rows_nearest_the_typed_text_of_a_hidden_field_fit_first() {
        let screen = terminal(vec![
            terminal_row("The first answer is far above the prompt.", 0),
            terminal_row("The newest answer is just above the prompt.", 1),
            terminal_row("❯ fix the login bug in the auth module and then", 2),
        ]);
        let one_line = estimated_tokens("The newest answer is just above the prompt.");

        let lines = nearby_lines(
            &screen,
            FieldContent::Hidden { typed: TYPED_PROMPT },
            one_line,
            LineScope::Window,
        );

        assert_eq!(
            lines,
            above_the_field(&["The newest answer is just above the prompt."])
        );
    }

    #[test]
    fn line_without_letters_beside_the_field_goes() {
        let screen = screen(vec![
            line("60", rect(250, 905, 290, 925)),
            line("General channel", rect(0, 905, 250, 925)),
        ]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Window);

        assert_eq!(lines, above_the_field(&["General channel"]));
    }

    #[test]
    fn line_inside_the_field_goes() {
        let screen = screen(vec![
            above("Dinner at 7?", 1),
            line("Write a message", rect(320, 915, 600, 945)),
        ]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Window);

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

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Window);

        assert_eq!(lines.above.len(), 4, "{lines:?}");
    }

    #[test]
    fn short_line_inside_a_line_of_the_field_stays() {
        let field_text = FieldText {
            before_caret: "Yes, works for me. See you at ".to_owned(),
            after_caret: String::new(),
        };
        let screen = screen(vec![above("Yes", 1)]);

        let lines = nearby_lines(
            &screen,
            FieldContent::Read(Some(&field_text)),
            BUDGET,
            LineScope::Window,
        );

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

        let lines = nearby_lines(
            &screen,
            FieldContent::Read(Some(&field_text)),
            BUDGET,
            LineScope::Window,
        );

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

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Window);

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
    fn message_loses_the_time_that_the_chat_shows_at_its_end() {
        let screen = screen(vec![
            above("Did you book the cabin by now?", 3),
            message_time("9:41 PM", 2),
            above("Yes, it is ours from Friday on 9:57 PM", 1),
        ]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Column);

        assert_eq!(
            lines,
            above_the_field(&["Did you book the cabin by now?", "Yes, it is ours from Friday on"])
        );
    }

    #[test]
    fn message_keeps_the_time_at_its_end_in_a_chat_that_shows_no_message_times() {
        let messages = ["AF 1288, lands at 18:05", "mine is AF 1240, it lands at 17:40"];
        let screen = screen(vec![above(messages[0], 2), above(messages[1], 1)]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Column);

        assert_eq!(lines, above_the_field(&messages));
    }

    #[test]
    fn message_keeps_the_time_at_its_end_when_its_own_time_is_beside_it() {
        let screen = screen(vec![
            above("nos vemos en la plaza a las 21:30", 1),
            message_time("21:52", 1),
        ]);

        let lines = nearby_lines(&screen, FieldContent::Read(None), BUDGET, LineScope::Column);

        assert_eq!(lines, above_the_field(&["nos vemos en la plaza a las 21:30"]));
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

        let lines = message_lines_above(&screen, FieldContent::Read(None));

        assert_eq!(lines, texts(&["Standup in 5?", "same room as yesterday"]));
    }

    #[test]
    fn lines_of_the_last_prompt_stay_first_and_the_new_message_comes_after_them() {
        let messages = [
            (CABIN, 6),
            (FRIDAY, 5),
            (KAYAKS, 4),
            (COOLER, 3),
            (DRIVER, 2),
            (SIX, 1),
        ];

        let lines = chat_lines_after(&[FRIDAY, KAYAKS, COOLER, DRIVER], &messages);

        assert_eq!(lines, above_the_field(&[FRIDAY, KAYAKS, COOLER, DRIVER, SIX]));
    }

    #[test]
    fn lines_of_the_last_prompt_go_when_they_need_more_than_the_slack() {
        let messages = [
            (CABIN, 7),
            (FRIDAY, 6),
            (KAYAKS, 5),
            (COOLER, 4),
            (DRIVER, 3),
            (SIX, 2),
            (SEE_YOU, 1),
        ];

        let lines = chat_lines_after(&[FRIDAY, KAYAKS, COOLER, DRIVER], &messages);

        assert_eq!(lines, above_the_field(&[COOLER, DRIVER, SIX, SEE_YOU]));
    }

    #[test]
    fn lines_of_the_last_prompt_go_when_their_first_line_left_the_screen() {
        let messages = [(FRIDAY, 5), (KAYAKS, 4), (COOLER, 3), (DRIVER, 2), (SIX, 1)];

        let lines = chat_lines_after(&[CABIN, FRIDAY, KAYAKS, COOLER], &messages);

        assert_eq!(lines, above_the_field(&[KAYAKS, COOLER, DRIVER, SIX]));
    }

    #[test]
    fn lines_of_the_last_prompt_go_when_a_new_message_comes_between_them() {
        let messages = [
            (CABIN, 6),
            (FRIDAY, 5),
            (KAYAKS, 4),
            (COOLER, 3),
            (DRIVER, 2),
            ("Seen", 1),
        ];

        let lines = chat_lines_after(&[FRIDAY, KAYAKS, COOLER, "Seen"], &messages);

        assert_eq!(lines, above_the_field(&[KAYAKS, COOLER, DRIVER, "Seen"]));
    }

    #[test]
    fn short_lines_of_the_last_prompt_far_from_the_caret_stay() {
        let messages = [
            ("Ana", 7),
            (CABIN, 6),
            ("Ana", 5),
            (FRIDAY, 4),
            (KAYAKS, 3),
            (COOLER, 2),
            (DRIVER, 1),
        ];

        let lines = chat_lines_after(&["Ana", "Ana", FRIDAY, KAYAKS, COOLER], &messages);

        assert_eq!(
            lines,
            above_the_field(&["Ana", "Ana", FRIDAY, KAYAKS, COOLER, DRIVER])
        );
    }

    #[test]
    fn older_lines_that_fit_the_budget_come_back_before_the_lines_of_the_last_prompt() {
        let messages = [(CABIN, 4), (FRIDAY, 3), (KAYAKS, 2), (COOLER, 1)];

        let lines = chat_lines_after(&[KAYAKS, COOLER], &messages);

        assert_eq!(lines, above_the_field(&[CABIN, FRIDAY, KAYAKS, COOLER]));
    }

    #[test]
    fn same_screen_shows_the_lines_of_the_last_prompt_again() {
        let screen = screen(vec![
            above(CABIN, 5),
            above(FRIDAY, 4),
            above(KAYAKS, 3),
            above(COOLER, 2),
            above(DRIVER, 1),
            line("Sent", rect(300, 980, 900, 1010)),
            line(SEE_YOU, rect(300, 1100, 900, 1130)),
        ]);
        let shown = nearby_lines(&screen, FieldContent::Read(None), CHAT_BUDGET, LineScope::Column);

        let lines = nearby_lines_from(&shown.above, &screen, FieldContent::Read(None), CHAT_BUDGET);

        assert_eq!(
            lines,
            NearbyLines {
                above: texts(&[KAYAKS, COOLER, DRIVER]),
                below: texts(&["Sent"]),
            }
        );
    }
}
