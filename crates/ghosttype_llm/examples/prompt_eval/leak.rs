//! Finds the text of a suggestion that the model took from the prompt, not from the writing: a leak.
//! Text that the writer typed in the case or in the earlier writing is never a leak.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Words about the prompt or the task, which a writer seldom types.
const PROMPT_WORDS: [&str; 11] = [
    "assistant",
    "caret",
    "clipboard",
    "cursor",
    "model",
    "prompt",
    "screen",
    "style",
    "text",
    "user",
    "writer",
];

/// The most words of a label before its colon, as in "Text after the caret:".
const MAX_LABEL_WORDS: usize = 4;

/// The quote that opens the text of a label in the prompt. Writers mostly type a straight quote.
const PROMPT_QUOTE: char = '“';

/// The signs that end the phrase before a label.
const PHRASE_ENDS: [char; 9] = ['\n', '.', ',', ';', '!', '?', '(', '“', '"'];

/// The most words other than numbers on a screen line with a name and a time, as in
/// "`chips_dev` — Today at 21:03".
const MAX_HEADER_WORDS: usize = 4;

/// The fewest letters of a name in the notes. A shorter name is often a part of a common word.
const MIN_NAME_LETTERS: usize = 4;

/// What the model took from the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Leak {
    /// A label of a few words with a colon, such as "Title:" or "Your message:", when the field
    /// has one word or the quote of the prompt comes after the colon. Other labels are often
    /// writing.
    Label,
    /// A word about the prompt or the task, such as "user" or "assistant".
    PromptWord,
    /// A name from the notes about the writer.
    Notes,
    /// Two words or a time from a line of the screen with only a name and a time.
    Screen,
}

/// The texts of a case that tell a leak from the writing.
#[derive(Debug)]
pub struct CaseTexts<'case> {
    /// The text of the field before the caret.
    pub before: &'case str,
    /// The text of the field after the caret.
    pub after: &'case str,
    /// The text that the writer typed next.
    pub next: &'case str,
    /// The earlier texts of the writer.
    pub earlier: Vec<&'case str>,
    /// The notes about the writer.
    pub notes: &'case str,
    /// The lines of the screen text.
    pub screen_lines: Vec<&'case str>,
}

impl Leak {
    /// Returns the leak in `suggestion`, or `None`. The suggestion counts with the start of its
    /// word in the text before the caret, so "ara" after "S" is "Sara".
    pub fn find(suggestion: &str, texts: &CaseTexts<'_>) -> Option<Self> {
        let last_word = texts
            .before
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or_default();
        let written = format!("{last_word}{suggestion}");
        let own_text = format!(
            "{}{}\n{}\n{}",
            texts.before,
            texts.next,
            texts.after,
            texts.earlier.join("\n")
        );
        let is_one_word_field = texts.before.split_whitespace().nth(1).is_none();
        let own_words = words(&own_text).map(str::to_lowercase).collect::<HashSet<_>>();
        let new_words = words(&written)
            .map(str::to_lowercase)
            .filter(|word| !own_words.contains(word))
            .collect::<Vec<_>>();
        let is_label_leak = |(label, rest): (&str, &str)| {
            (is_one_word_field || rest.trim_start().starts_with(PROMPT_QUOTE)) && !own_text.contains(label)
        };
        if labels(&written).any(is_label_leak) {
            return Some(Self::Label);
        }
        if new_words.iter().any(|word| PROMPT_WORDS.contains(&word.as_str())) {
            return Some(Self::PromptWord);
        }
        let names = names_in_notes(texts.notes);
        if new_words
            .iter()
            .any(|word| names.iter().any(|name| word.starts_with(name)))
        {
            return Some(Self::Notes);
        }
        has_header_text(&written, &own_text, &texts.screen_lines).then_some(Self::Screen)
    }
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|character: char| !(character.is_alphanumeric() || matches!(character, '_' | '-')))
        .filter(|word| !word.is_empty())
}

/// Returns the labels in `text`, each a phrase of at most [`MAX_LABEL_WORDS`] words with its colon,
/// and the text after it. A colon of a time or a link ends no label.
fn labels(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.match_indices(':').filter_map(move |(colon, _)| {
        let rest = &text[colon + 1..];
        let ends_label =
            rest.is_empty() || rest.starts_with(char::is_whitespace) || rest.starts_with(['“', '"']);
        let phrase = text[..colon].rsplit(PHRASE_ENDS).next()?.trim_start();
        (ends_label && is_label_phrase(phrase)).then(|| (&text[colon - phrase.len()..=colon], rest))
    })
}

fn is_label_phrase(phrase: &str) -> bool {
    let phrase_words = phrase.split_whitespace().collect::<Vec<_>>();
    (1..=MAX_LABEL_WORDS).contains(&phrase_words.len())
        && phrase_words.iter().all(|word| {
            word.chars()
                .all(|character| character.is_alphabetic() || matches!(character, '-' | '_' | '\''))
        })
}

/// Returns the names in `notes` in lowercase: the words with a capital letter that do not start a
/// sentence.
fn names_in_notes(notes: &str) -> Vec<String> {
    notes
        .split(['.', '!', '?', '\n'])
        .flat_map(|sentence| words(sentence).skip(1))
        .filter(|word| word.starts_with(char::is_uppercase) && word.chars().count() >= MIN_NAME_LETTERS)
        .map(str::to_lowercase)
        .collect()
}

/// Returns whether `written` has a time or two words in a row of a chat header on the screen,
/// which the writer did not type.
fn has_header_text(written: &str, own_text: &str, screen_lines: &[&str]) -> bool {
    let headers = screen_lines
        .iter()
        .filter(|line| is_header(line))
        .collect::<Vec<_>>();
    if headers.is_empty() {
        return false;
    }
    let header_pairs = headers
        .iter()
        .flat_map(|line| word_pairs(line))
        .collect::<HashSet<_>>();
    let own_pairs = word_pairs(own_text);
    times(written).any(|time| !own_text.contains(time))
        || word_pairs(written)
            .iter()
            .any(|pair| header_pairs.contains(pair) && !own_pairs.contains(pair))
}

/// Returns whether `line` is the header of a chat message: a time and at most
/// [`MAX_HEADER_WORDS`] other words, such as a name.
fn is_header(line: &str) -> bool {
    let other_words = words(line).filter(|word| !word.chars().all(|character| character.is_ascii_digit()));
    times(line).next().is_some() && other_words.count() <= MAX_HEADER_WORDS
}

fn word_pairs(text: &str) -> Vec<String> {
    words(text)
        .collect::<Vec<_>>()
        .windows(2)
        .map(|pair| pair.join(" "))
        .collect()
}

/// Returns the times in `text`, such as "9:05" or "21:03", and also a time that the token limit
/// cut, such as "5:0".
fn times(text: &str) -> impl Iterator<Item = &str> {
    text.match_indices(':').filter_map(move |(colon, _)| {
        let hours = text[..colon]
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .count();
        let minutes = text[colon + 1..].chars().take_while(char::is_ascii_digit).count();
        let is_time = (1..=2).contains(&hours) && (1..=2).contains(&minutes);
        is_time.then(|| &text[colon - hours..=colon + minutes])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_NOTES: &str = "";

    fn texts_of<'case>(before: &'case str, next: &'case str) -> CaseTexts<'case> {
        CaseTexts {
            before,
            after: "",
            next,
            earlier: Vec::new(),
            notes: NO_NOTES,
            screen_lines: Vec::new(),
        }
    }

    fn leak_of(before: &str, suggestion: &str, notes: &str, screen_lines: &[&str]) -> Option<Leak> {
        let texts = CaseTexts {
            notes,
            screen_lines: screen_lines.to_vec(),
            ..texts_of(before, "")
        };
        Leak::find(suggestion, &texts)
    }

    fn leak_with_next(before: &str, suggestion: &str, next: &str) -> Option<Leak> {
        Leak::find(suggestion, &texts_of(before, next))
    }

    #[test]
    fn short_phrase_with_a_colon_is_a_label() {
        let cases = [
            ("c", "hat; Title: #random"),
            ("Y", "ou type:"),
            ("S", "elected text: “Sí, mamá"),
            ("H", "ere is my reply: sure"),
        ];

        let leaks = cases.map(|(before, suggestion)| leak_of(before, suggestion, NO_NOTES, &[]));

        assert_eq!(leaks, cases.map(|_| Some(Leak::Label)));
    }

    #[test]
    fn colon_of_a_time_a_link_or_a_long_phrase_is_not_a_label() {
        let cases = [
            ("see you at ", "8:40"),
            ("the page is ", "https://acme.dev/docs"),
            ("", "The plan for the next two weeks is: ship"),
        ];

        let leaks = cases.map(|(before, suggestion)| leak_of(before, suggestion, NO_NOTES, &[]));

        assert_eq!(leaks, cases.map(|_| None));
    }

    #[test]
    fn label_after_more_than_one_word_is_a_leak_only_before_the_quote_of_the_prompt() {
        let cases = [
            ("Pasos para reprodu", "cir el error:", None),
            (
                "The form shows this error ",
                "message: \"Cannot send invoice\"",
                None,
            ),
            (
                "les pido que las envíen a la oficina de ",
                "Rancagua, en la dirección:",
                None,
            ),
            (
                "Yes, it is the ",
                "migration. Nearby on screen: “",
                Some(Leak::Label),
            ),
        ];

        let leaks = cases.map(|(before, suggestion, _)| leak_of(before, suggestion, NO_NOTES, &[]));

        assert_eq!(leaks, cases.map(|(_, _, leak)| leak));
    }

    #[test]
    fn label_that_the_writer_typed_is_not_a_leak() {
        let leak = leak_with_next("n", "it: rename it", "it: the function name says user but");

        assert_eq!(leak, None);
    }

    #[test]
    fn word_about_the_prompt_is_a_leak() {
        let cases = [
            ("you need to ", "respond to the user's message."),
            ("Y", "ou are a helpful assistant."),
            ("I", " will now provide the text to"),
        ];

        let leaks = cases.map(|(before, suggestion)| leak_of(before, suggestion, NO_NOTES, &[]));

        assert_eq!(leaks, cases.map(|_| Some(Leak::PromptWord)));
    }

    #[test]
    fn word_about_the_prompt_that_the_writer_typed_is_not_a_leak() {
        let leak = leak_with_next(
            "Can you send me the ",
            "text of the contract",
            "text of the contract",
        );

        assert_eq!(leak, None);
    }

    #[test]
    fn word_about_the_prompt_in_earlier_writing_is_not_a_leak() {
        let texts = CaseTexts {
            earlier: vec!["One nit: the function name says user but it reads accounts."],
            ..texts_of("Looks good to me. One ", "nit: the button label")
        };

        let leak = Leak::find("nit: the function name says user but", &texts);

        assert_eq!(leak, None);
    }

    #[test]
    fn name_from_the_notes_is_a_leak() {
        let notes = "I am Sara Novak (saranovak). I write in English or Spanish.";
        let cases = [("S", "ara Novak (saranovak"), ("ask ", "saranovak on Slack")];

        let leaks = cases.map(|(before, suggestion)| leak_of(before, suggestion, notes, &[]));

        assert_eq!(leaks, cases.map(|_| Some(Leak::Notes)));
    }

    #[test]
    fn word_that_starts_a_sentence_of_the_notes_is_not_a_name() {
        let notes = "Logistics is my work. Tracking numbers are long.";

        let leak = leak_of("our ", "logistics team tracks it", notes, &[]);

        assert_eq!(leak, None);
    }

    #[test]
    fn name_from_the_notes_that_the_writer_typed_is_not_a_leak() {
        let texts = CaseTexts {
            notes: "I am Sara Novak.",
            ..texts_of("Best,\nSara", " Novak")
        };

        let leak = Leak::find(" Novak", &texts);

        assert_eq!(leak, None);
    }

    #[test]
    fn name_or_time_of_a_chat_header_is_a_leak() {
        let screen = [
            "chips_dev — Today at 21:03",
            "post the code",
            "Laura Méndez 11:02",
            "Can we move the sync to Thursday?",
        ];
        let cases = [
            ("c", "hips_dev 5:0"),
            ("s", "tars_dev — Today at"),
            ("Thursday ", "11:04"),
            ("Hi ", "Laura Méndez, thanks"),
        ];

        let leaks = cases.map(|(before, suggestion)| leak_of(before, suggestion, NO_NOTES, &screen));

        assert_eq!(leaks, cases.map(|_| Some(Leak::Screen)));
    }

    #[test]
    fn one_word_of_a_chat_header_is_not_a_leak() {
        let screen = ["Laura Méndez 11:02", "Can we move the sync to Thursday?"];

        let leak = leak_of("no problem ", "Laura, I will", NO_NOTES, &screen);

        assert_eq!(leak, None);
    }

    #[test]
    fn words_and_times_of_a_message_on_the_screen_are_not_a_leak() {
        let screen = ["Cami: el pasaje q me mandaste decia 8:40 parece", "Delivered"];
        let cases = [("llega a las ", "8:45"), ("el ", "pasaje dice eso")];

        let leaks = cases.map(|(before, suggestion)| leak_of(before, suggestion, NO_NOTES, &screen));

        assert_eq!(leaks, cases.map(|_| None));
    }

    #[test]
    fn name_or_time_of_a_chat_header_that_the_writer_typed_is_not_a_leak() {
        let texts = CaseTexts {
            screen_lines: vec!["Laura Méndez 11:02", "¿A qué hora es la llamada? ¿11:30?"],
            ..texts_of("Hola Laura, la llamada es a las ", "11:30 entonces")
        };

        let leak = Leak::find("11:30, Laura", &texts);

        assert_eq!(leak, None);
    }

    #[test]
    fn writing_without_prompt_text_is_not_a_leak() {
        let screen = ["nova  10:43 AM", "looks like the migration test again"];

        let leak = leak_of(
            "Yes, it is the ",
            "migration test again",
            "I am a nurse.",
            &screen,
        );

        assert_eq!(leak, None);
    }
}
