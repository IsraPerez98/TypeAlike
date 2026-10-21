//! Builds the text the model continues: a preface about the writing, then a heading and the
//! text before the caret. The preface describes and does not give orders, because a base model
//! continues text and does not follow orders. It speaks as the writer, so the model takes the
//! text as the writing of one person and not as a task about a user. It goes from the part that
//! changes least to the part that changes most.

use crate::desktop::{ScreenText, Surroundings};
use crate::excerpt::{estimated_tokens, first_tokens, from_word, last_tokens, one_line};
use crate::inference::{Prompt, TokenCount};
use crate::place::{self, Format};
use crate::screen;

/// The start of each preface. The labels of the lines after it depend on it, so the user does not
/// change it. The user writes [`WriterNotes`](crate::settings::WriterNotes) instead.
pub(crate) const FRAME: &str = "I type the text below on my computer, for example an email, a chat \
     message, a note or a document. My writing is clear, natural and correct. It keeps my \
     language, tone and style.";
/// The line just before the text before the caret. Without it, the model reads one or two typed
/// characters as the start of one more line of the preface, and writes a label.
const TEXT_HEADING: &str = "The part of my writing that I am typing now:";
/// The most estimated tokens of the preface. The text before the caret can use the rest of the
/// context, so a long preface never pushes it out.
pub(crate) const PREFACE_TOKENS: TokenCount = TokenCount(2000);
const NOTES_TOKENS: TokenCount = TokenCount(300);
const NAME_TOKENS: TokenCount = TokenCount(20);
const WINDOW_TITLE_TOKENS: TokenCount = TokenCount(40);
const FIELD_LABEL_TOKENS: TokenCount = TokenCount(40);
const AFTER_CARET_TOKENS: TokenCount = TokenCount(100);
/// More screen text is too slow to decode on the CPU: 1200 tokens add about 6 s with a 4B model.
const SCREEN_TOKENS: TokenCount = TokenCount(300);
const CLIPBOARD_TOKENS: TokenCount = TokenCount(170);
const HISTORY_TOKENS: TokenCount = TokenCount(200);

/// What GhostType knows about the person who writes.
#[derive(Debug, Clone, Default)]
pub(crate) struct Writer<'text> {
    /// The notes that the writer gives about themself.
    pub notes: &'text str,
    /// The name of the writer.
    pub name: &'text str,
    /// Earlier texts of the writer, oldest first.
    pub examples: Vec<String>,
}

/// Returns what the model reads before the text: the [`FRAME`], the notes of the `writer` on one
/// line, the place where the user types, the earlier texts of the `writer`, the `clipboard` text,
/// the text below the field on the `screen`, the text after the caret and the text above the
/// field. The text above the field comes last, so the messages of a chat come just before the
/// reply. Each part that is empty is left out. The parts fill [`PREFACE_TOKENS`] in the order of
/// their priority: the frame, the notes, the text after the caret, the place, the screen, the
/// earlier texts and the clipboard.
pub(crate) fn preface(
    writer: &Writer<'_>,
    surroundings: &Surroundings,
    clipboard: Option<&str>,
    screen: Option<&ScreenText>,
) -> String {
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
    let place = place(surroundings).map(|line| budget.take_start(&line, PREFACE_TOKENS).to_owned());
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
    let lines = [
        Some(FRAME.to_owned()),
        notes.map(|text| format!("Notes about me: “{text}”")),
        name,
        place
            .filter(|line| !line.is_empty())
            .map(|line| format!("Where I am typing: {line}")),
        (!examples.is_empty()).then(|| {
            let quoted = examples
                .iter()
                .map(|text| format!("“{text}”"))
                .collect::<Vec<_>>();
            format!("My earlier writing:\n{}", quoted.join("\n"))
        }),
        clipboard
            .and_then(non_empty)
            .map(|text| format!("Text on my clipboard: “{text}”")),
        non_empty(&below).map(|text| format!("Below the field on my screen: “{text}”")),
        after_caret
            .and_then(non_empty)
            .map(|text| format!("My text after the cursor: “{text}”")),
        non_empty(&above).map(|text| format!("Above the field on my screen: “{text}”")),
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
fn place(surroundings: &Surroundings) -> Option<String> {
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
mod tests {
    use super::*;
    use crate::desktop::{Confidence, FieldLabels, FieldText, ScreenLine, ScreenRect};

    fn notes(text: &str) -> Writer<'_> {
        Writer {
            notes: text,
            ..Writer::default()
        }
    }

    fn examples<'text>(examples: &[&str]) -> Writer<'text> {
        Writer {
            examples: examples.iter().map(|text| (*text).to_owned()).collect(),
            ..Writer::default()
        }
    }

    /// Returns the preface that has `lines` after the frame.
    fn framed(lines: &str) -> String {
        format!("{FRAME}\n{lines}")
    }

    /// Returns surroundings whose place line leaves `tokens` of the preface for the parts after
    /// it.
    fn surroundings_leaving(tokens: u32) -> Surroundings {
        let place_tokens = PREFACE_TOKENS.0 - estimated_tokens(FRAME).0 - tokens;
        let app_name = "a".repeat(3 * usize::try_from(place_tokens).unwrap() - "App: ".len());
        surroundings(Some(&app_name), None)
    }

    fn named_field(name: &str) -> FieldLabels {
        FieldLabels {
            name: Some(name.to_owned()),
            ..FieldLabels::default()
        }
    }

    fn surroundings(app_name: Option<&str>, window_title: Option<&str>) -> Surroundings {
        Surroundings {
            app_name: app_name.map(str::to_owned),
            window_title: window_title.map(str::to_owned),
            ..Surroundings::default()
        }
    }

    /// Returns `character` as many times as fit in `tokens`, then `extra` times more.
    fn ascii_text(character: char, tokens: TokenCount, extra: usize) -> String {
        character
            .to_string()
            .repeat(3 * usize::try_from(tokens.0).unwrap() + extra)
    }

    fn with_text_after(text: &str) -> Surroundings {
        Surroundings {
            field_text: Some(FieldText {
                before_caret: "Hi ".to_owned(),
                after_caret: text.to_owned(),
            }),
            ..Surroundings::default()
        }
    }

    #[test]
    fn preface_goes_from_the_frame_to_text_after_the_caret() {
        let surroundings = Surroundings {
            app_id: Some("com.apple.mail".to_owned()),
            field_labels: named_field("Message"),
            field_text: Some(FieldText {
                before_caret: "Hi ".to_owned(),
                after_caret: "\n\n> Priya wrote\n".to_owned(),
            }),
            ..surroundings(Some("Mail"), Some("Re: Budget"))
        };

        let preface = preface(&notes(" I am a nurse. "), &surroundings, Some(" 1Z999 "), None);

        assert_eq!(
            preface,
            framed(
                "Notes about me: “I am a nurse.”\nWhere I am typing: Format: email; App: Mail; Title: Re: Budget; Field: Message\nText on my clipboard: “1Z999”\nMy text after the cursor: “> Priya wrote”"
            )
        );
    }

    #[test]
    fn empty_parts_are_left_out() {
        let writer = Writer {
            notes: " \n ",
            name: " ",
            ..Writer::default()
        };

        let preface = preface(&writer, &with_text_after(" \n "), Some(" "), None);

        assert_eq!(preface, FRAME);
    }

    #[test]
    fn name_of_the_writer_comes_after_the_notes_on_one_line() {
        let writer = Writer {
            notes: "I am a nurse.",
            name: "  Sara \n Novak ",
            ..Writer::default()
        };

        let preface = preface(&writer, &Surroundings::default(), None, None);

        assert_eq!(
            preface,
            framed("Notes about me: “I am a nurse.”\nMy name is Sara Novak.")
        );
    }

    #[test]
    fn notes_stay_on_one_line() {
        let preface = preface(
            &notes("I am a nurse.\n\n  I write in Spanish."),
            &Surroundings::default(),
            None,
            None,
        );

        assert_eq!(
            preface,
            framed("Notes about me: “I am a nurse. I write in Spanish.”")
        );
    }

    #[test]
    fn long_notes_keep_their_start() {
        let long_notes = ascii_text('n', NOTES_TOKENS, 1);

        let preface = preface(&notes(&long_notes), &Surroundings::default(), None, None);

        assert_eq!(
            preface,
            framed(&format!("Notes about me: “{}”", ascii_text('n', NOTES_TOKENS, 0)))
        );
    }

    #[test]
    fn place_names_the_app_and_the_window() {
        let cases = [
            (
                Some("Mail"),
                Some("Re: Budget"),
                "Where I am typing: App: Mail; Title: Re: Budget",
            ),
            (
                Some("Slack"),
                Some("#design - Acme - Slack"),
                "Where I am typing: Title: #design - Acme - Slack",
            ),
            (Some("Notes"), None, "Where I am typing: App: Notes"),
            (None, Some("Re: Budget"), "Where I am typing: Title: Re: Budget"),
            (
                Some(" "),
                Some("Re: Budget"),
                "Where I am typing: Title: Re: Budget",
            ),
        ];

        let prefaces =
            cases.map(|(app, window, _)| preface(&Writer::default(), &surroundings(app, window), None, None));

        assert_eq!(prefaces, cases.map(|(_, _, place)| framed(place)));
    }

    #[test]
    fn web_page_gives_the_format_of_the_place() {
        let surroundings = Surroundings {
            app_id: Some("com.google.Chrome".to_owned()),
            domain: Some("mail.google.com".to_owned()),
            field_labels: named_field("Message Body"),
            ..surroundings(Some("Google Chrome"), Some("Inbox - Gmail - Google Chrome"))
        };

        let preface = preface(&Writer::default(), &surroundings, None, None);

        assert_eq!(
            preface,
            framed(
                "Where I am typing: Format: email; Title: Inbox - Gmail - Google Chrome; Field: Message Body"
            )
        );
    }

    #[test]
    fn other_labels_of_the_field_give_a_hint() {
        let surroundings = Surroundings {
            field_labels: FieldLabels {
                caption: Some("Message".to_owned()),
                name: Some("message".to_owned()),
                placeholder: Some("Message Priya Shah".to_owned()),
                help: Some("Shift+Enter adds a line".to_owned()),
            },
            ..surroundings(None, Some("Priya Shah"))
        };

        let preface = preface(&Writer::default(), &surroundings, None, None);

        assert_eq!(
            preface,
            framed(
                "Where I am typing: Title: Priya Shah; Field: Message; Hint: Message Priya Shah, Shift+Enter adds a line"
            )
        );
    }

    #[test]
    fn place_stays_on_one_line() {
        let surroundings = Surroundings {
            field_labels: named_field(" Message\n  body "),
            ..surroundings(None, Some("Re:\tBudget"))
        };

        let preface = preface(&Writer::default(), &surroundings, None, None);

        assert_eq!(
            preface,
            framed("Where I am typing: Title: Re: Budget; Field: Message body")
        );
    }

    #[test]
    fn long_parts_keep_their_start() {
        let title = ascii_text('t', WINDOW_TITLE_TOKENS, 1);
        let label = ascii_text('l', FIELD_LABEL_TOKENS, 1);
        let hint = ascii_text('h', FIELD_LABEL_TOKENS, 1);
        let clipboard = ascii_text('c', CLIPBOARD_TOKENS, 1);
        let after = ascii_text('a', AFTER_CARET_TOKENS, 1);
        let surroundings = Surroundings {
            window_title: Some(title),
            field_labels: FieldLabels {
                name: Some(label),
                placeholder: Some(hint),
                ..FieldLabels::default()
            },
            ..with_text_after(&after)
        };

        let preface = preface(&Writer::default(), &surroundings, Some(&clipboard), None);

        assert_eq!(
            preface,
            framed(&format!(
                "Where I am typing: Title: {}; Field: {}; Hint: {}\nText on my clipboard: “{}”\nMy text after the cursor: “{}”",
                ascii_text('t', WINDOW_TITLE_TOKENS, 0),
                ascii_text('l', FIELD_LABEL_TOKENS, 0),
                ascii_text('h', FIELD_LABEL_TOKENS, 0),
                ascii_text('c', CLIPBOARD_TOKENS, 0),
                ascii_text('a', AFTER_CARET_TOKENS, 0)
            ))
        );
    }

    fn screen(lines: &[(&str, i32)]) -> ScreenText {
        let line = |&(text, top): &(&str, i32)| ScreenLine {
            text: text.to_owned(),
            confidence: Confidence::CERTAIN,
            frame: ScreenRect {
                left: 0,
                top,
                right: 600,
                bottom: top + 20,
            },
        };
        ScreenText {
            lines: lines.iter().map(line).collect(),
            caret: None,
            field: ScreenRect {
                left: 0,
                top: 500,
                right: 600,
                bottom: 540,
            },
        }
    }

    #[test]
    fn screen_text_below_the_field_goes_before_the_text_after_the_caret_and_above_it_goes_last() {
        let screen = screen(&[
            ("See you at 7?", 400),
            ("Sent from my phone", 600),
            ("From: Sam", 300),
        ]);

        let preface = preface(
            &Writer::default(),
            &with_text_after("Best, Ana"),
            Some("1Z999"),
            Some(&screen),
        );

        assert_eq!(
            preface,
            framed(
                "Text on my clipboard: “1Z999”\nBelow the field on my screen: “Sent from my phone”\nMy text after the cursor: “Best, Ana”\nAbove the field on my screen: “From: Sam\nSee you at 7?”"
            )
        );
    }

    #[test]
    fn screen_text_fits_in_what_the_preface_has_left() {
        let screen = screen(&[("Dinner on Saturday?", 300), ("Hi", 480)]);

        let preface = preface(&Writer::default(), &surroundings_leaving(2), None, Some(&screen));

        assert!(
            preface.ends_with("\nAbove the field on my screen: “Hi”"),
            "{preface}"
        );
    }

    #[test]
    fn screen_text_keeps_whole_lines() {
        let screen = screen(&[("abc", 400), ("def", 420)]);

        let preface = preface(&Writer::default(), &surroundings_leaving(2), None, Some(&screen));

        assert!(
            preface.ends_with("\nAbove the field on my screen: “abc\ndef”"),
            "{preface}"
        );
    }

    #[test]
    fn screen_without_lines_adds_nothing() {
        let preface = preface(
            &Writer::default(),
            &Surroundings::default(),
            None,
            Some(&screen(&[])),
        );

        assert_eq!(preface, FRAME);
    }

    #[test]
    fn earlier_writing_goes_between_the_place_and_the_clipboard() {
        let surroundings = Surroundings {
            field_text: Some(FieldText {
                before_caret: "Hi ".to_owned(),
                after_caret: "Best, Ana".to_owned(),
            }),
            ..surroundings(Some("Mail"), None)
        };

        let preface = preface(
            &examples(&["Cheers,\nTom", "See you soon!"]),
            &surroundings,
            Some("1Z999"),
            None,
        );

        assert_eq!(
            preface,
            framed(
                "Where I am typing: App: Mail\nMy earlier writing:\n“Cheers,\nTom”\n“See you soon!”\nText on my clipboard: “1Z999”\nMy text after the cursor: “Best, Ana”"
            )
        );
    }

    #[test]
    fn earlier_writing_drops_the_older_examples_that_do_not_fit() {
        let newest = ascii_text('n', HISTORY_TOKENS, 0);
        let older = format!("{} end", "word ".repeat(30));

        let preface = preface(
            &examples(&[&older, "first words", &newest]),
            &Surroundings::default(),
            None,
            None,
        );

        assert_eq!(preface, framed(&format!("My earlier writing:\n“{newest}”")));
    }

    #[test]
    fn earlier_writing_cuts_an_example_at_the_start_of_a_word() {
        let newest = ascii_text('n', TokenCount(HISTORY_TOKENS.0 - 4), 0);

        let preface = preface(
            &examples(&["one two three", "end of the example", &newest]),
            &Surroundings::default(),
            None,
            None,
        );

        assert_eq!(
            preface,
            framed(&format!("My earlier writing:\n“the example”\n“{newest}”"))
        );
    }

    #[test]
    fn prompt_puts_a_blank_line_and_a_heading_between_preface_and_text() {
        let cases = [
            (
                "Dear Sir",
                "Formal.\n\nThe part of my writing that I am typing now:\nDear Sir",
            ),
            ("", "Formal.\n\nThe part of my writing that I am typing now:\n"),
        ];

        let prompts = cases.map(|(text_before, _)| prompt("Formal.", text_before));

        assert_eq!(prompts, cases.map(|(_, prompt)| Prompt(prompt.to_owned())));
    }
}
