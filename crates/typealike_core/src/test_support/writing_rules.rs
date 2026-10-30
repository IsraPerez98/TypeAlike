//! The writing rules of the text that the user sees: ASD-STE100 (`.claude/rules/ste100.md`), the
//! sentence case of Fluent (`.claude/rules/windows-ui.md`), and the terms and the table "Not in the
//! UI" of `PRODUCT.md`.

use crate::settings_window::view::{Access, Accessory, Button, ChoiceText, Page, Row, Section, SettingsView};
use crate::setup::SetupView;

/// The longest sentence that a description can have, in words.
const MAX_SENTENCE_WORDS: usize = 25;

const CONTRACTION_ENDINGS: [&str; 6] = ["n't", "'re", "'ll", "'ve", "'m", "'d"];

const CONTRACTIONS_WITH_IS: [&str; 10] = [
    "it's", "that's", "there's", "here's", "what's", "let's", "who's", "where's", "he's", "she's",
];

const MARKETING_WORDS: [&str; 8] = [
    "seamless",
    "robust",
    "powerful",
    "cutting-edge",
    "effortless",
    "world-class",
    "next-generation",
    "revolutionary",
];

/// The words that the UI does not use, each with the term of `PRODUCT.md` that replaces it.
const REPLACED_TERMS: [(&str, &str); 8] = [
    ("completion", "suggestion"),
    ("completions", "suggestions"),
    ("prediction", "suggestion"),
    ("predictions", "suggestions"),
    ("autocomplete", "suggestion"),
    ("history", "earlier writing"),
    ("llm", "model"),
    ("fullscreen", "full screen"),
];

/// The words of the code and the docs that the UI does not show (`PRODUCT.md`, "Not in the UI").
const CODE_WORDS: [&str; 17] = [
    "backend",
    "caret",
    "token",
    "tokens",
    "prompt",
    "prompts",
    "inference",
    "ghost",
    "load",
    "loads",
    "loaded",
    "loading",
    "unload",
    "unloads",
    "unloaded",
    "unloading",
    "gguf",
];

/// The names that keep their capital letters inside a text in sentence case: the names of the
/// product, of its services, of a graphics interface, of a pane and a permission of macOS, of the
/// apps of the tests and of the country in the name of a language.
const PROPER_NAMES: [&str; 7] = [
    "TypeAlike",
    "Hugging Face",
    "Vulkan",
    "System Settings",
    "Screen & System Audio Recording",
    "Safari",
    "Brasil",
];

/// The word that takes the place of a proper name before the check of the sentence case.
const PROPER_NAME_STAND_IN: &str = "name";

/// What a text of the UI is, which gives the rules that apply to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextRole {
    /// A title, a heading, a label, a choice, a button or a menu entry, in sentence case.
    Name,
    /// One or more sentences, such as a description, a note or a message.
    Prose,
}

/// A text that the user sees, with its role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiText {
    role: TextRole,
    text: String,
}

impl UiText {
    pub(crate) fn name(text: impl Into<String>) -> Self {
        Self {
            role: TextRole::Name,
            text: text.into(),
        }
    }

    pub(crate) fn prose(text: impl Into<String>) -> Self {
        Self {
            role: TextRole::Prose,
            text: text.into(),
        }
    }
}

/// Each break of the writing rules in `texts`, as the text and the rule that it breaks.
pub(crate) fn writing_rule_breaks(texts: &[UiText]) -> Vec<String> {
    texts
        .iter()
        .flat_map(|text| {
            breaks_of(text)
                .into_iter()
                .map(|rule| format!("“{}”: {rule}", text.text))
        })
        .collect()
}

impl SettingsView {
    /// The texts of all the pages, without the text of the user, of a model and of an app.
    pub(crate) fn texts(&self) -> Vec<UiText> {
        self.pages.iter().flat_map(Page::texts).collect()
    }
}

impl SetupView {
    /// The texts of the step, without the text of the user, of a model and of the demo.
    pub(crate) fn texts(&self) -> Vec<UiText> {
        let mut texts = vec![
            UiText::name(&self.window_title),
            UiText::name(&self.progress.text),
            UiText::name(&self.title),
        ];
        texts.extend(self.body.iter().map(UiText::prose));
        texts.extend(self.sections.iter().flat_map(section_texts));
        texts.extend(self.footnote.iter().map(UiText::prose));
        texts.extend(self.back.iter().map(button_text));
        texts.push(button_text(&self.next));
        texts
    }
}

impl Page {
    /// The texts of the page, without the text of the user, of a model and of an app.
    pub(crate) fn texts(&self) -> Vec<UiText> {
        let mut texts = vec![UiText::name(&self.title), UiText::prose(&self.summary)];
        texts.extend(self.search.iter().map(|search| UiText::name(&search.placeholder)));
        texts.extend(self.sections.iter().flat_map(section_texts));
        texts
    }
}

fn section_texts(section: &Section) -> Vec<UiText> {
    let mut texts: Vec<UiText> = section.heading.iter().map(UiText::name).collect();
    texts.extend(section.back.iter().map(button_text));
    texts.extend(section.rows.iter().flat_map(row_texts));
    texts.extend(section.footer.iter().map(UiText::prose));
    texts
}

fn row_texts(row: &Row) -> Vec<UiText> {
    match row {
        Row::Switch(switch) => labeled(
            &switch.label,
            &switch.description,
            switch.note.iter().map(|note| note.text.as_str()),
        ),
        Row::Picker(picker) => {
            let mut texts = labeled(
                &picker.label,
                &picker.description,
                picker.note.iter().map(|note| note.text.as_str()),
            );
            if picker.choice_text == ChoiceText::Label {
                texts.extend(picker.choices.iter().map(UiText::name));
            }
            texts
        }
        Row::AppPicker(picker) => picker.choices.iter().map(UiText::name).collect(),
        Row::TextArea(text_area) => {
            let mut texts = labeled(&text_area.label, &text_area.description, None);
            texts.extend(text_area.button.iter().map(button_text));
            texts
        }
        Row::TextLine(text_line) => labeled(&text_line.label, &text_line.description, None),
        Row::TextField(text_field) => vec![
            UiText::name(&text_field.label),
            UiText::name(&text_field.placeholder),
            UiText::name(&text_field.submit_label),
        ],
        Row::Note(note) => vec![UiText::prose(&note.text)],
        Row::Output(output) => vec![UiText::prose(&output.detail)],
        Row::Item(item) => {
            let mut texts = vec![UiText::prose(&item.subtitle), UiText::prose(&item.detail)];
            texts.extend(accessory_button(&item.accessory).map(button_text));
            texts
        }
        Row::Loading(text) => vec![UiText::prose(text)],
        Row::Message(message) => {
            let mut texts = vec![UiText::prose(&message.text)];
            texts.extend(message.button.iter().map(button_text));
            texts
        }
        Row::Permission(permission) => {
            let mut texts = labeled(&permission.label, &permission.description, None);
            texts.push(match &permission.access {
                Access::Granted(text) => UiText::name(text),
                Access::Missing(button) => button_text(button),
            });
            texts
        }
        Row::Link(button) => vec![button_text(button)],
        Row::AppSwitch(_) | Row::SiteSwitch(_) => Vec::new(),
    }
}

fn labeled<'note>(
    label: &str,
    description: &str,
    notes: impl IntoIterator<Item = &'note str>,
) -> Vec<UiText> {
    let mut texts = vec![UiText::name(label), UiText::prose(description)];
    texts.extend(notes.into_iter().map(UiText::prose));
    texts
}

fn button_text(button: &Button) -> UiText {
    UiText::name(&button.label)
}

fn accessory_button(accessory: &Accessory) -> Option<&Button> {
    match accessory {
        Accessory::Download(button)
        | Accessory::Open(button)
        | Accessory::Downloading { cancel: button, .. }
        | Accessory::Installed { delete: button }
        | Accessory::Failed { retry: button } => Some(button),
        Accessory::Canceling | Accessory::Unavailable => None,
    }
}

fn breaks_of(text: &UiText) -> Vec<String> {
    let mut breaks = Vec::new();
    if text.text.contains('—') {
        breaks.push("has an em dash".to_owned());
    }
    if text.text.contains(';') {
        breaks.push("has a semicolon".to_owned());
    }
    let words = words_of(&text.text);
    breaks.extend(
        words
            .iter()
            .filter(|word| is_contraction(word))
            .map(|word| format!("has the contraction “{word}”")),
    );
    breaks.extend(
        words
            .iter()
            .filter(|word| MARKETING_WORDS.contains(&word.as_str()))
            .map(|word| format!("has the marketing word “{word}”")),
    );
    breaks.extend(
        words
            .iter()
            .filter_map(|word| replaced_term(word))
            .map(|(word, term)| format!("says “{word}”, not “{term}”")),
    );
    breaks.extend(
        words
            .iter()
            .filter(|word| CODE_WORDS.contains(&word.as_str()))
            .map(|word| format!("has the code word “{word}”")),
    );
    match text.role {
        TextRole::Name => breaks.extend(
            capitalized_words(&text.text)
                .into_iter()
                .map(|word| format!("is not in sentence case at “{word}”")),
        ),
        TextRole::Prose => breaks.extend(
            long_sentences(&text.text)
                .map(|words| format!("has a sentence of {words} words, more than {MAX_SENTENCE_WORDS}")),
        ),
    }
    breaks
}

fn words_of(text: &str) -> Vec<String> {
    text.split(|character: char| !(character.is_alphanumeric() || matches!(character, '\'' | '’' | '-')))
        .filter(|word| !word.is_empty())
        .map(|word| word.replace('’', "'").to_lowercase())
        .collect()
}

fn is_contraction(word: &str) -> bool {
    CONTRACTION_ENDINGS.iter().any(|ending| word.ends_with(ending)) || CONTRACTIONS_WITH_IS.contains(&word)
}

fn replaced_term(word: &str) -> Option<(&str, &'static str)> {
    REPLACED_TERMS
        .iter()
        .find(|(replaced, _)| *replaced == word)
        .map(|(_, term)| (word, *term))
}

/// The words after the first that start with a capital letter and a small letter, outside the
/// proper names.
fn capitalized_words(text: &str) -> Vec<String> {
    PROPER_NAMES
        .iter()
        .fold(text.to_owned(), |text, name| {
            text.replace(name, PROPER_NAME_STAND_IN)
        })
        .split_whitespace()
        .skip(1)
        .filter(|word| starts_as_a_title(word))
        .map(str::to_owned)
        .collect()
}

fn starts_as_a_title(word: &str) -> bool {
    let mut characters = word
        .trim_start_matches(|character: char| !character.is_alphanumeric())
        .chars();
    characters.next().is_some_and(char::is_uppercase) && characters.next().is_some_and(char::is_lowercase)
}

fn long_sentences(text: &str) -> impl Iterator<Item = usize> {
    text.split(['.', '!', '?', '…'])
        .map(|sentence| sentence.split_whitespace().count())
        .filter(|words| *words > MAX_SENTENCE_WORDS)
}

#[cfg(test)]
mod tests {
    use std::slice;

    use super::*;

    #[test]
    fn text_that_follows_the_rules_has_no_breaks() {
        let texts = [
            UiText::name("Show suggestions in Safari"),
            UiText::name("Search Hugging Face"),
            UiText::name("Graphics card (Vulkan)"),
            UiText::name("Open System Settings"),
            UiText::prose("TypeAlike cannot use this model. Select another model, or download it again."),
        ];

        let breaks = writing_rule_breaks(&texts);

        assert_eq!(breaks, Vec::<String>::new());
    }

    #[test]
    fn each_rule_gives_its_break() {
        let cases = [
            (
                UiText::prose("Fast — and local"),
                "“Fast — and local”: has an em dash",
            ),
            (
                UiText::prose("It is fast; it is local"),
                "“It is fast; it is local”: has a semicolon",
            ),
            (
                UiText::prose("You can’t undo this"),
                "“You can’t undo this”: has the contraction “can't”",
            ),
            (
                UiText::prose("It's local"),
                "“It's local”: has the contraction “it's”",
            ),
            (
                UiText::prose("A seamless setup"),
                "“A seamless setup”: has the marketing word “seamless”",
            ),
            (
                UiText::prose("The completion stopped"),
                "“The completion stopped”: says “completion”, not “suggestion”",
            ),
            (
                UiText::prose("Suggestions stop in fullscreen"),
                "“Suggestions stop in fullscreen”: says “fullscreen”, not “full screen”",
            ),
            (
                UiText::prose("TypeAlike unloads the model"),
                "“TypeAlike unloads the model”: has the code word “unloads”",
            ),
            (
                UiText::name("Screen Text"),
                "“Screen Text”: is not in sentence case at “Text”",
            ),
            (
                UiText::prose(
                    "When you do not type or click for this time, TypeAlike stops suggestions and frees its memory so \
                     that the computer has much more free memory.",
                ),
                "“When you do not type or click for this time, TypeAlike stops suggestions and frees its memory so \
                 that the computer has much more free memory.”: has a sentence of 26 words, more than 25",
            ),
        ];

        let breaks: Vec<String> = cases
            .iter()
            .flat_map(|(text, _)| writing_rule_breaks(slice::from_ref(text)))
            .collect();

        assert_eq!(breaks, cases.map(|(_, expected)| expected.to_owned()));
    }
}
