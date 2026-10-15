//! Builds the text the model continues: a preface about the writing, then the text before the
//! caret. The preface describes and does not give orders, because a base model continues text
//! and does not follow orders. It goes from the part that changes least to the part that changes
//! most.

use crate::desktop::{MAX_CHARS_AFTER_CARET, Surroundings};
use crate::inference::{Prompt, SystemPrompt};

const MAX_WINDOW_TITLE_CHARS: usize = 120;
const MAX_CLIPBOARD_CHARS: usize = 500;

/// Returns what the model reads before the text: the system prompt, the app and window, the
/// `clipboard` text and the text after the caret. Each part that is empty is left out.
pub(crate) fn preface(
    system_prompt: &SystemPrompt,
    surroundings: &Surroundings,
    clipboard: Option<&str>,
) -> String {
    let after_caret = surroundings
        .field_text
        .as_ref()
        .map(|field_text| first_chars(field_text.after_caret.trim(), MAX_CHARS_AFTER_CARET));
    let lines = [
        non_empty(system_prompt.0.trim()).map(str::to_owned),
        place(surroundings),
        clipboard.and_then(|text| non_empty(text.trim())).map(|text| {
            format!(
                "Text on the clipboard: “{}”",
                first_chars(text, MAX_CLIPBOARD_CHARS)
            )
        }),
        after_caret
            .and_then(non_empty)
            .map(|text| format!("Text after the caret: “{text}”")),
    ];
    lines.into_iter().flatten().collect::<Vec<_>>().join("\n")
}

/// Returns the prompt that continues `text_before` after `preface`, with a blank line between
/// them and no label that the model could copy into its suggestion.
pub(crate) fn prompt(preface: &str, text_before: &str) -> Prompt {
    if preface.is_empty() {
        return Prompt(text_before.to_owned());
    }
    Prompt(format!("{preface}\n\n{text_before}"))
}

/// Returns the last `count` characters of `text`.
pub(crate) fn last_chars(text: &str, count: usize) -> &str {
    let excess = text.chars().count().saturating_sub(count);
    text.char_indices()
        .nth(excess)
        .map_or("", |(start, _)| &text[start..])
}

/// The app and the window, as one line. A window title that names the app stands alone.
fn place(surroundings: &Surroundings) -> Option<String> {
    let app = surroundings
        .app_name
        .as_deref()
        .map(str::trim)
        .and_then(non_empty);
    let window = surroundings
        .window_title
        .as_deref()
        .map(|title| first_chars(title.trim(), MAX_WINDOW_TITLE_CHARS))
        .and_then(non_empty);
    match (app, window) {
        (Some(app), Some(window)) if window.to_lowercase().contains(&app.to_lowercase()) => {
            Some(window.to_owned())
        }
        (Some(app), Some(window)) => Some(format!("{app} - {window}")),
        (Some(name), None) | (None, Some(name)) => Some(name.to_owned()),
        (None, None) => None,
    }
}

fn first_chars(text: &str, count: usize) -> &str {
    text.char_indices()
        .nth(count)
        .map_or(text, |(end, _)| &text[..end])
}

fn non_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::FieldText;

    fn system_prompt(text: &str) -> SystemPrompt {
        SystemPrompt(text.to_owned())
    }

    fn surroundings(app_name: Option<&str>, window_title: Option<&str>) -> Surroundings {
        Surroundings {
            app_name: app_name.map(str::to_owned),
            window_title: window_title.map(str::to_owned),
            field_text: None,
        }
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
    fn preface_goes_from_system_prompt_to_text_after_the_caret() {
        let surroundings = Surroundings {
            field_text: Some(FieldText {
                before_caret: "Hi ".to_owned(),
                after_caret: "\n\n> Priya wrote\n".to_owned(),
            }),
            ..surroundings(Some("Mail"), Some("Re: Budget"))
        };

        let preface = preface(&system_prompt(" Formal. "), &surroundings, Some(" 1Z999 "));

        assert_eq!(
            preface,
            "Formal.\nMail - Re: Budget\nText on the clipboard: “1Z999”\nText after the caret: “> Priya wrote”"
        );
    }

    #[test]
    fn empty_parts_are_left_out() {
        let preface = preface(&system_prompt(" "), &with_text_after(" \n "), Some(" "));

        assert_eq!(preface, "");
    }

    #[test]
    fn place_names_the_app_and_the_window() {
        let cases = [
            (Some("Mail"), Some("Re: Budget"), "Mail - Re: Budget"),
            (
                Some("Slack"),
                Some("#design - Acme - Slack"),
                "#design - Acme - Slack",
            ),
            (Some("Notes"), None, "Notes"),
            (None, Some("Re: Budget"), "Re: Budget"),
            (Some(" "), Some("Re: Budget"), "Re: Budget"),
        ];

        let prefaces =
            cases.map(|(app, window, _)| preface(&SystemPrompt::default(), &surroundings(app, window), None));

        assert_eq!(prefaces, cases.map(|(_, _, place)| place.to_owned()));
    }

    #[test]
    fn long_parts_keep_their_start() {
        let title = "t".repeat(MAX_WINDOW_TITLE_CHARS + 1);
        let clipboard = "c".repeat(MAX_CLIPBOARD_CHARS + 1);
        let after = "a".repeat(MAX_CHARS_AFTER_CARET + 1);
        let surroundings = Surroundings {
            window_title: Some(title),
            ..with_text_after(&after)
        };

        let preface = preface(&SystemPrompt::default(), &surroundings, Some(&clipboard));

        assert_eq!(
            preface,
            format!(
                "{}\nText on the clipboard: “{}”\nText after the caret: “{}”",
                "t".repeat(MAX_WINDOW_TITLE_CHARS),
                "c".repeat(MAX_CLIPBOARD_CHARS),
                "a".repeat(MAX_CHARS_AFTER_CARET)
            )
        );
    }

    #[test]
    fn prompt_puts_a_blank_line_between_preface_and_text() {
        let prompt = prompt("Formal.", "Dear Sir");

        assert_eq!(prompt, Prompt("Formal.\n\nDear Sir".to_owned()));
    }

    #[test]
    fn prompt_without_preface_is_the_text() {
        let prompt = prompt("", "Dear Sir");

        assert_eq!(prompt, Prompt("Dear Sir".to_owned()));
    }

    #[test]
    fn last_chars_keeps_the_end_of_the_text() {
        let cases = [("café au lait", 4, "lait"), ("café", 10, "café"), ("café", 0, "")];

        let kept = cases.map(|(text, count, _)| last_chars(text, count));

        assert_eq!(kept, cases.map(|(_, _, end)| end));
    }
}
