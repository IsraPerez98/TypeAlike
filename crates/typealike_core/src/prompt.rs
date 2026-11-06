//! Builds the text the model continues: a preface about the writing, then a heading and the
//! text before the caret. The preface describes and does not give orders, because a base model
//! continues text and does not follow orders. It speaks as the writer, so the model takes the
//! text as the writing of one person and not as a task about a user. It goes from the part that
//! changes least to the part that changes most.

use std::fmt;

use crate::desktop::{ScreenText, Surroundings};
use crate::excerpt::{estimated_tokens, first_tokens, from_word, last_tokens, one_line};
use crate::inference::{Prompt, TokenCount};
use crate::place::{self, Format};
use crate::screen::{self, FieldContent, LineScope, NearbyLines};

/// The start of each preface. The labels of the lines after it depend on it, so the user does not
/// change it. The user writes [`WriterNotes`](crate::settings::WriterNotes) instead.
const FRAME: &str = "I type the text below on my computer, for example an email, a chat \
     message, a note or a document. My writing is clear, natural and correct. It keeps my \
     language, tone and style.";
/// The line just before the text before the caret. Without it, the model reads one or two typed
/// characters as the start of one more line of the preface, and writes a label.
const TEXT_HEADING: &str = "The part of my writing that I am typing now:";
/// The label of the last line above the field, which the preface shows one more time. That line
/// is often the message that the user answers, and its second copy gives it more weight than the
/// older text of the preface.
const LAST_LINE_LABEL: &str = "The last line above the field on my screen:";
/// The start of the line that gives the name of the writer, before the name and a period.
const NAME_LINE_START: &str = "My name is ";
/// The label of the line that tells where the user types, before the app, the window title and
/// the labels of the field.
const PLACE_LABEL: &str = "Where I am typing:";
/// The most estimated tokens of the preface. The text before the caret can use the rest of the
/// context, so a long preface never pushes it out.
const PREFACE_TOKENS: TokenCount = TokenCount(2000);
const NOTES_TOKENS: TokenCount = TokenCount(300);
const NAME_TOKENS: TokenCount = TokenCount(20);
const WINDOW_TITLE_TOKENS: TokenCount = TokenCount(40);
const FIELD_LABEL_TOKENS: TokenCount = TokenCount(40);
const AFTER_CARET_TOKENS: TokenCount = TokenCount(100);
/// In an email reply, the text after the caret holds the quoted message that the user answers.
const EMAIL_AFTER_CARET_TOKENS: TokenCount = TokenCount(300);
const CLIPBOARD_TOKENS: TokenCount = TokenCount(170);
const HISTORY_TOKENS: TokenCount = TokenCount(320);

/// An earlier text of the writer that a prompt shows.
#[derive(Clone)]
pub(crate) struct Example {
    pub(crate) text: String,
    /// The days since the writer wrote the text, if known.
    pub(crate) days_ago: Option<u64>,
}

/// What TypeAlike knows about the person who writes. `Debug` shows only the length of each text
/// and the number of examples, so logs never hold what the user wrote.
#[derive(Clone, Default)]
pub(crate) struct Writer<'text> {
    /// The notes that the writer gives about themself.
    pub(crate) notes: &'text str,
    /// The name of the writer.
    pub(crate) name: &'text str,
    /// Earlier texts of the writer, oldest first.
    pub(crate) examples: Vec<Example>,
}

impl fmt::Debug for Writer<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Writer")
            .field("notes", &self.notes.chars().count())
            .field("name", &self.name.chars().count())
            .field("examples", &self.examples.len())
            .finish()
    }
}

/// The text near the field on the screen that a preface shows, and the most estimated tokens that
/// it can use.
#[derive(Clone, Copy)]
pub(crate) struct ScreenPart<'text> {
    pub(crate) text: &'text ScreenText,
    pub(crate) max_tokens: TokenCount,
    /// The lines above the field that the last preface in the field showed, in reading order.
    pub(crate) shown_above: &'text [String],
    /// The text that the user typed before the caret, which the lines of a field that hides its
    /// text can show again.
    pub(crate) typed: &'text str,
}

/// What the model reads before the text, and the lines above the field on the screen that it
/// shows.
pub(crate) struct Preface {
    pub(crate) text: String,
    /// The lines above the field on the screen that `text` shows, in reading order.
    pub(crate) screen_lines_above: Vec<String>,
}

/// Returns what the model reads before the text: the [`FRAME`], the notes of the `writer` on one
/// line, the place where the user types, the earlier texts of the `writer` with their age, the
/// `clipboard` text, the text below the field on the `screen`, the text above the field, its last
/// line one more time and the text after the caret. The text above the field comes late, so the
/// messages of a chat come just before the reply. The text after the caret comes last, next to the
/// heading, so that the model writes text that leads into it. Each part that is empty is left out.
/// The parts fill [`PREFACE_TOKENS`] in the order of their priority: the frame, the notes, the
/// text after the caret, the place, the screen, the earlier texts and the clipboard.
pub(crate) fn preface(
    writer: &Writer<'_>,
    surroundings: &Surroundings,
    clipboard: Option<&str>,
    screen: Option<ScreenPart<'_>>,
) -> String {
    preface_with_screen_lines(writer, surroundings, clipboard, screen).text
}

/// Returns the [`preface`] and the lines above the field on the screen that it shows.
pub(crate) fn preface_with_screen_lines(
    writer: &Writer<'_>,
    surroundings: &Surroundings,
    clipboard: Option<&str>,
    screen: Option<ScreenPart<'_>>,
) -> Preface {
    let parts = budgeted_parts(writer, surroundings, clipboard, screen);
    let screen_lines_above = parts.above.clone();
    Preface {
        text: preface_lines(parts),
        screen_lines_above,
    }
}

/// The parts of a preface after their cut to the budget, before their layout.
struct PrefaceParts<'text> {
    notes: Option<String>,
    name: Option<String>,
    place: Option<String>,
    examples: Vec<(&'text str, Option<u64>)>,
    clipboard: Option<&'text str>,
    below: String,
    after_caret: Option<&'text str>,
    above: Vec<String>,
    last_line_above: Option<String>,
}

/// Returns the parts of the preface, each cut to its cap and to what is left of
/// [`PREFACE_TOKENS`]. The parts take the budget in the order of their priority.
fn budgeted_parts<'text>(
    writer: &'text Writer<'_>,
    surroundings: &'text Surroundings,
    clipboard: Option<&'text str>,
    screen: Option<ScreenPart<'_>>,
) -> PrefaceParts<'text> {
    let mut budget = Budget(PREFACE_TOKENS);
    budget.spend(FRAME);
    let notes = budgeted_line(Some(writer.notes), NOTES_TOKENS)
        .map(|line| budget.take_start(&line, NOTES_TOKENS).to_owned());
    let name = budgeted_line(Some(writer.name), NAME_TOKENS).map(|name| {
        let line = format!("{NAME_LINE_START}{name}.");
        budget.spend(&line);
        line
    });
    let after_caret = surroundings
        .field_text
        .as_ref()
        .map(|field_text| budget.take_start(field_text.after_caret.trim(), after_caret_tokens(surroundings)));
    let place = place_line(surroundings).map(|line| budget.take_start(&line, PREFACE_TOKENS).to_owned());
    let nearby = screen
        .map(|screen| nearby_lines(screen, surroundings, budget.left(screen.max_tokens)))
        .unwrap_or_default();
    let below = nearby.below.join("\n");
    budget.spend(&nearby.above.join("\n"));
    budget.spend(&below);
    let last_line_above = nearby
        .above
        .last()
        .and_then(|line| budget.take_whole(line))
        .map(str::to_owned);
    let examples = budget.take_examples(&writer.examples);
    let clipboard = clipboard.map(|text| budget.take_start(text.trim(), CLIPBOARD_TOKENS));
    PrefaceParts {
        notes,
        name,
        place,
        examples,
        clipboard,
        below,
        after_caret,
        above: nearby.above,
        last_line_above,
    }
}

/// Returns the lines of the `screen` part around the field of `surroundings` that fit in `budget`.
/// In a chat or an email app, the lines that the last preface in the field showed above it stay
/// first while they can.
fn nearby_lines(screen: ScreenPart<'_>, surroundings: &Surroundings, budget: TokenCount) -> NearbyLines {
    let field = FieldContent::of(surroundings, screen.typed);
    match LineScope::of(surroundings) {
        LineScope::Column => screen::nearby_lines_from(screen.shown_above, screen.text, field, budget),
        LineScope::Window => screen::nearby_lines(screen.text, field, budget, LineScope::Window),
    }
}

/// Returns the most estimated tokens of the text after the caret in the field of `surroundings`.
fn after_caret_tokens(surroundings: &Surroundings) -> TokenCount {
    if Format::of(surroundings) == Some(Format::Email) {
        EMAIL_AFTER_CARET_TOKENS
    } else {
        AFTER_CARET_TOKENS
    }
}

/// Returns the lines of the `parts` in reading order after the [`FRAME`], without the empty parts.
fn preface_lines(parts: PrefaceParts<'_>) -> String {
    let lines = [
        Some(FRAME.to_owned()),
        parts.notes.map(|text| format!("Notes about me: “{text}”")),
        parts.name,
        parts
            .place
            .filter(|line| !line.is_empty())
            .map(|line| format!("{PLACE_LABEL} {line}")),
        (!parts.examples.is_empty()).then(|| {
            // A quote just after an earlier text makes the model end the typed text with a quote.
            let quoted = parts
                .examples
                .iter()
                .map(|&(text, days_ago)| match days_ago {
                    Some(days_ago) => format!("({}) “{text}\n”", age_label(days_ago)),
                    None => format!("“{text}\n”"),
                })
                .collect::<Vec<_>>();
            format!("My earlier writing:\n{}", quoted.join("\n"))
        }),
        parts
            .clipboard
            .and_then(non_empty)
            .map(|text| format!("Text on my clipboard: “{text}”")),
        non_empty(&parts.below).map(|text| format!("Below the field on my screen: “{text}”")),
        non_empty(&parts.above.join("\n")).map(|text| format!("Above the field on my screen: “{text}”")),
        parts
            .last_line_above
            .map(|line| format!("{LAST_LINE_LABEL} “{line}”")),
        parts
            .after_caret
            .and_then(non_empty)
            .map(|text| format!("The text that comes right after the part that I am typing now: “{text}”")),
    ];
    lines.into_iter().flatten().collect::<Vec<_>>().join("\n")
}

/// Returns how long ago a text of `days_ago` days is, in words.
fn age_label(days_ago: u64) -> String {
    match days_ago {
        0 => "today".to_owned(),
        1 => "yesterday".to_owned(),
        2..14 => format!("{days_ago} days ago"),
        _ => format!("{} weeks ago", days_ago / 7),
    }
}

/// The estimated tokens that the preface can still use.
struct Budget(TokenCount);

impl Budget {
    /// Returns the tokens that a part with the most tokens `cap` can use.
    fn left(&self, cap: TokenCount) -> TokenCount {
        cap.min(self.0)
    }

    /// Returns the start of `text` that fits in `cap` and in the budget, and spends its tokens.
    fn take_start<'text>(&mut self, text: &'text str, cap: TokenCount) -> &'text str {
        let kept = first_tokens(text, self.left(cap));
        self.spend(kept);
        kept
    }

    /// Returns all of `text` when it fits in the budget, and spends its tokens.
    fn take_whole<'text>(&mut self, text: &'text str) -> Option<&'text str> {
        let fits = estimated_tokens(text) <= self.0;
        fits.then(|| {
            self.spend(text);
            text
        })
    }

    /// Returns the ends of `examples` that fit in [`HISTORY_TOKENS`] and in the budget, each from
    /// the start of a word with its days ago, and spends their tokens. The newest examples keep
    /// the most text.
    fn take_examples<'text>(&mut self, examples: &'text [Example]) -> Vec<(&'text str, Option<u64>)> {
        let mut left = self.left(HISTORY_TOKENS);
        let mut kept = examples
            .iter()
            .rev()
            .map(|example| {
                let end = from_word(&example.text, last_tokens(&example.text, left));
                left = TokenCount(left.0.saturating_sub(estimated_tokens(end).0));
                self.spend(end);
                (end, example.days_ago)
            })
            .filter(|(end, _)| !end.is_empty())
            .collect::<Vec<_>>();
        kept.reverse();
        kept
    }

    fn spend(&mut self, text: &str) {
        self.0 = TokenCount(self.0.0.saturating_sub(estimated_tokens(text).0));
    }
}

/// Returns the prompt that continues `text_before` after `preface`, with a blank line and the
/// [`TEXT_HEADING`] between them.
pub(crate) fn prompt(preface: &str, text_before: &str) -> Prompt {
    Prompt(format!("{preface}\n\n{TEXT_HEADING}\n{text_before}"))
}

/// Returns the lines that the prompt writes itself around the quoted texts of `preface`, such as
/// the [`FRAME`] and the labels, and the [`TEXT_HEADING`]. A quoted text holds what the writer
/// wrote or saw, so no line holds it. The place line keeps only its [`PLACE_LABEL`], because the
/// window title and the labels of the field are text that the writer can type.
pub(crate) fn frame_lines(preface: &str) -> Vec<String> {
    let mut depth = 0_usize;
    let mut unquoted = String::new();
    for character in preface.chars() {
        match character {
            '“' => depth += 1,
            '”' => depth = depth.saturating_sub(1),
            _ if depth == 0 => unquoted.push(character),
            _ => {}
        }
    }
    unquoted
        .lines()
        .map(|line| {
            if line.starts_with(PLACE_LABEL) {
                PLACE_LABEL
            } else {
                line
            }
        })
        .map(str::to_owned)
        .chain([TEXT_HEADING.to_owned()])
        .collect()
}

/// Returns the name of the writer that the frame of `preface` gives, if any.
pub(crate) fn writer_name(preface: &str) -> Option<String> {
    frame_lines(preface).into_iter().find_map(|line| {
        line.strip_prefix(NAME_LINE_START)?
            .strip_suffix('.')
            .map(str::to_owned)
    })
}

/// Where the user types, as one line of labeled parts. A window title that names the app stands
/// without the app. The first label of the field names it, and its other labels give a hint.
fn place_line(surroundings: &Surroundings) -> Option<String> {
    let app = budgeted_line(surroundings.app_name.as_deref(), PREFACE_TOKENS);
    let title = budgeted_line(surroundings.window_title.as_deref(), WINDOW_TITLE_TOKENS);
    let names_app = matches!((&app, &title), (Some(app), Some(title))
        if title.to_lowercase().contains(&app.to_lowercase()));
    let mut labels = place::field_labels(surroundings).into_iter();
    let field = labels.next();
    let hint = labels.collect::<Vec<_>>().join(", ");
    let parts = [
        (
            "Format",
            Format::of(surroundings).map(|format| format.name().to_owned()),
        ),
        ("App", app.filter(|_| !names_app)),
        ("Title", title),
        ("Field", budgeted_line(field.as_deref(), FIELD_LABEL_TOKENS)),
        ("Hint", budgeted_line(Some(&hint), FIELD_LABEL_TOKENS)),
    ];
    let line = parts
        .into_iter()
        .filter_map(|(name, value)| Some(format!("{name}: {}", value?)))
        .collect::<Vec<_>>()
        .join("; ");
    (!line.is_empty()).then_some(line)
}

/// Returns the start of `text` that fits in `budget`, with each run of whitespace as one space, or
/// `None` when it has no other characters.
fn budgeted_line(text: Option<&str>, budget: TokenCount) -> Option<String> {
    let line = one_line(text?);
    non_empty(first_tokens(&line, budget)).map(str::to_owned)
}

fn non_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

/// Returns the prompt with the [`FRAME`], the default notes and the `lines` of the preface, then
/// `text_before`.
#[cfg(test)]
pub(crate) fn framed_prompt(lines: &[&str], text_before: &str) -> Prompt {
    prompt_with_notes(&crate::settings::WriterNotes::default().0, lines, text_before)
}

/// Returns the prompt with the [`FRAME`], the `notes` and the `lines` of the preface, then
/// `text_before`.
#[cfg(test)]
pub(crate) fn prompt_with_notes(notes: &str, lines: &[&str], text_before: &str) -> Prompt {
    let notes_line = format!("Notes about me: “{notes}”");
    let preface = [FRAME, notes_line.as_str()]
        .into_iter()
        .chain(lines.iter().copied())
        .collect::<Vec<_>>();
    prompt(&preface.join("\n"), text_before)
}

#[cfg(test)]
mod tests;
