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
use crate::screen;

/// The start of each preface. The labels of the lines after it depend on it, so the user does not
/// change it. The user writes [`WriterNotes`](crate::settings::WriterNotes) instead.
const FRAME: &str = "I type the text below on my computer, for example an email, a chat \
     message, a note or a document. My writing is clear, natural and correct. It keeps my \
     language, tone and style.";
/// The line just before the text before the caret. Without it, the model reads one or two typed
/// characters as the start of one more line of the preface, and writes a label.
const TEXT_HEADING: &str = "The part of my writing that I am typing now:";
/// The most estimated tokens of the preface. The text before the caret can use the rest of the
/// context, so a long preface never pushes it out.
const PREFACE_TOKENS: TokenCount = TokenCount(2000);
const NOTES_TOKENS: TokenCount = TokenCount(300);
const NAME_TOKENS: TokenCount = TokenCount(20);
const WINDOW_TITLE_TOKENS: TokenCount = TokenCount(40);
const FIELD_LABEL_TOKENS: TokenCount = TokenCount(40);
const AFTER_CARET_TOKENS: TokenCount = TokenCount(100);
/// More screen text is too slow to decode on the CPU: 1200 tokens add about 6 s with a 4B model.
const SCREEN_TOKENS: TokenCount = TokenCount(300);
const CLIPBOARD_TOKENS: TokenCount = TokenCount(170);
const HISTORY_TOKENS: TokenCount = TokenCount(200);

/// What TypeAlike knows about the person who writes. `Debug` shows only the length of each text
/// and the number of examples, so logs never hold what the user wrote.
#[derive(Clone, Default)]
pub(crate) struct Writer<'text> {
    /// The notes that the writer gives about themself.
    pub(crate) notes: &'text str,
    /// The name of the writer.
    pub(crate) name: &'text str,
    /// Earlier texts of the writer, oldest first.
    pub(crate) examples: Vec<String>,
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

/// Returns what the model reads before the text: the [`FRAME`], the notes of the `writer` on one
/// line, the place where the user types, the earlier texts of the `writer`, the `clipboard` text,
/// the text below the field on the `screen`, the text above the field and the text after the
/// caret. The text above the field comes late, so the messages of a chat come just before the
/// reply. The text after the caret comes last, next to the heading, so that the model writes text
/// that leads into it. Each part that is empty is left out. The parts fill [`PREFACE_TOKENS`] in
/// the order of their priority: the frame, the notes, the text after the caret, the place, the
/// screen, the earlier texts and the clipboard.
pub(crate) fn preface(
    writer: &Writer<'_>,
    surroundings: &Surroundings,
    clipboard: Option<&str>,
    screen: Option<&ScreenText>,
) -> String {
    preface_lines(budgeted_parts(writer, surroundings, clipboard, screen))
}

/// The parts of a preface after their cut to the budget, before their layout.
struct PrefaceParts<'text> {
    notes: Option<String>,
    name: Option<String>,
    place: Option<String>,
    examples: Vec<&'text str>,
    clipboard: Option<&'text str>,
    below: String,
    after_caret: Option<&'text str>,
    above: String,
}

/// Returns the parts of the preface, each cut to its cap and to what is left of
/// [`PREFACE_TOKENS`]. The parts take the budget in the order of their priority.
fn budgeted_parts<'text>(
    writer: &'text Writer<'_>,
    surroundings: &'text Surroundings,
    clipboard: Option<&'text str>,
    screen: Option<&ScreenText>,
) -> PrefaceParts<'text> {
    let mut budget = Budget(PREFACE_TOKENS);
    budget.spend(FRAME);
    let notes = budgeted_line(Some(writer.notes), NOTES_TOKENS)
        .map(|line| budget.take_start(&line, NOTES_TOKENS).to_owned());
    let name = budgeted_line(Some(writer.name), NAME_TOKENS).map(|name| {
        let line = format!("My name is {name}.");
        budget.spend(&line);
        line
    });
    let after_caret = surroundings
        .field_text
        .as_ref()
        .map(|field_text| budget.take_start(field_text.after_caret.trim(), AFTER_CARET_TOKENS));
    let place = place_line(surroundings).map(|line| budget.take_start(&line, PREFACE_TOKENS).to_owned());
    let nearby = screen
        .map(|screen| {
            screen::nearby_lines(
                screen,
                surroundings.field_text.as_ref(),
                budget.left(SCREEN_TOKENS),
            )
        })
        .unwrap_or_default();
    let above = nearby.above.join("\n");
    let below = nearby.below.join("\n");
    budget.spend(&above);
    budget.spend(&below);
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
        above,
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
            .map(|line| format!("Where I am typing: {line}")),
        (!parts.examples.is_empty()).then(|| {
            let quoted = parts
                .examples
                .iter()
                .map(|text| format!("“{text}”"))
                .collect::<Vec<_>>();
            format!("My earlier writing:\n{}", quoted.join("\n"))
        }),
        parts
            .clipboard
            .and_then(non_empty)
            .map(|text| format!("Text on my clipboard: “{text}”")),
        non_empty(&parts.below).map(|text| format!("Below the field on my screen: “{text}”")),
        non_empty(&parts.above).map(|text| format!("Above the field on my screen: “{text}”")),
        parts
            .after_caret
            .and_then(non_empty)
            .map(|text| format!("The text that comes right after the part that I am typing now: “{text}”")),
    ];
    lines.into_iter().flatten().collect::<Vec<_>>().join("\n")
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

    /// Returns the ends of `examples` that fit in [`HISTORY_TOKENS`] and in the budget, each from
    /// the start of a word, and spends their tokens. The newest examples keep the most text.
    fn take_examples<'text>(&mut self, examples: &'text [String]) -> Vec<&'text str> {
        let mut left = self.left(HISTORY_TOKENS);
        let mut kept = examples
            .iter()
            .rev()
            .map(|example| {
                let end = from_word(example, last_tokens(example, left));
                left = TokenCount(left.0.saturating_sub(estimated_tokens(end).0));
                self.spend(end);
                end
            })
            .filter(|end| !end.is_empty())
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
