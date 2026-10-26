//! Cuts a suggestion before a word that the model made up: a word that the dictionary of the OS
//! does not know and that the writer did not write.

use std::collections::HashMap;

use crate::history::is_apostrophe;
use crate::inference::WordBias;
use crate::spelling::SpellChecker;

use super::Engine;
use super::session::Session;
use super::suggestion::{word_starts, words};

/// The fewest letters of a word that the dictionary checks. Shorter words of a chat, such as "ok",
/// "xd" or "po", are often in no dictionary.
const MIN_CHECKED_LETTERS: usize = 3;
/// The most answers of the dictionary that the engine keeps. Each answer costs a call to the OS.
const MAX_KEPT_ANSWERS: usize = 10_000;

/// The dictionary of the OS, with the answers that it gave.
pub(super) struct Dictionary {
    spell_checker: Box<dyn SpellChecker>,
    answers: HashMap<String, bool>,
}

impl Dictionary {
    fn knows(&mut self, word: &str) -> bool {
        if self.answers.len() >= MAX_KEPT_ANSWERS {
            self.answers.clear();
        }
        let spell_checker = &self.spell_checker;
        *self
            .answers
            .entry(word.to_owned())
            .or_insert_with(|| spell_checker.is_known_word(word))
    }
}

/// The words that the writer wrote or prefers: the words of the prompt and of the text after the
/// caret of a session, and the words of its word bias.
pub(super) struct WrittenWords<'session> {
    pub(super) session: &'session Session,
    pub(super) word_bias: Option<&'session WordBias>,
}

impl WrittenWords<'_> {
    /// Whether the writer wrote or prefers `word`, in any case.
    fn holds(&self, word: &str) -> bool {
        let lowercase = word.to_lowercase();
        let session = self.session;
        let is_in_session = [&session.preface, &session.text_before, &session.text_after]
            .into_iter()
            .any(|text| words(text).any(|written| written == lowercase));
        is_in_session
            || self.word_bias.is_some_and(|bias| {
                bias.words
                    .iter()
                    .any(|(written, _)| written.to_lowercase() == lowercase)
            })
    }
}

impl Engine {
    /// Cuts each suggestion of the model before a word that `spell_checker` does not know and that
    /// the writer did not write.
    pub(crate) fn use_spell_checker(&mut self, spell_checker: Box<dyn SpellChecker>) {
        self.dictionary = Some(Dictionary {
            spell_checker,
            answers: HashMap::new(),
        });
    }
}

/// Returns `suggestion`, the ghost text after `text_before`, up to its first word that the
/// `dictionary` does not know and that the `written` words do not hold, without the whitespace
/// before that word. A first word that finishes the last word of `text_before` counts with its
/// typed letters. Without a dictionary, it returns all of `suggestion`.
pub(super) fn known_start<'text>(
    dictionary: Option<&mut Dictionary>,
    text_before: &str,
    suggestion: &'text str,
    written: &WrittenWords<'_>,
) -> &'text str {
    let Some(dictionary) = dictionary else {
        return suggestion;
    };
    let typed_letters = if suggestion.starts_with(char::is_whitespace) {
        ""
    } else {
        text_before.rsplit(char::is_whitespace).next().unwrap_or_default()
    };
    before_unknown_word(typed_letters, suggestion, |word| {
        dictionary.knows(word) || written.holds(word)
    })
}

/// Returns `suggestion` up to the run without whitespace that holds its first word that `is_known`
/// does not accept, without the whitespace before that run. The first run also has the
/// `typed_letters` before it.
fn before_unknown_word<'text>(
    typed_letters: &str,
    suggestion: &'text str,
    mut is_known: impl FnMut(&str) -> bool,
) -> &'text str {
    let text = format!("{typed_letters}{suggestion}");
    let cut = word_starts(&text)
        .find(|&start| {
            let run = text[start..]
                .split(char::is_whitespace)
                .next()
                .unwrap_or_default();
            checked_words(run).any(|word| !is_known(word))
        })
        .map(|start| start.saturating_sub(typed_letters.len()));
    cut.map_or(suggestion, |end| suggestion[..end].trim_end())
}

/// Returns the words of `run` that the dictionary checks: its parts between hyphens, without the
/// signs around it, with at least [`MIN_CHECKED_LETTERS`] letters. A run with a digit or another
/// sign inside, such as a number, an address or a name in code, or with a capital letter inside,
/// such as `useState`, has none.
fn checked_words(run: &str) -> impl Iterator<Item = &str> {
    let word = run.trim_matches(|character: char| !character.is_alphabetic());
    let is_plain = word
        .chars()
        .all(|character| character.is_alphabetic() || character == '-' || is_apostrophe(character));
    let has_inner_capital =
        word.chars().skip(1).any(char::is_uppercase) && word.chars().any(char::is_lowercase);
    let checked = if is_plain && !has_inner_capital { word } else { "" };
    checked.split('-').filter(|part| {
        part.chars().filter(|character| character.is_alphabetic()).count() >= MIN_CHECKED_LETTERS
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cut_without(unknown: &[&str], typed_letters: &str, suggestion: &str) -> String {
        before_unknown_word(typed_letters, suggestion, |word| !unknown.contains(&word)).to_owned()
    }

    #[test]
    fn suggestion_ends_before_the_run_with_the_first_unknown_word() {
        let cases = [
            (&["prostu"][..], "", " prostu nie", ""),
            (&["prostu"], "", "vamos prostu nie", "vamos"),
            (&["Turjal"], "Tu", "rjal.", ""),
            (&["Turjal"], "", "call (Turjal) today", "call"),
            (&["knwn"], "", "a well-knwn fact", "a"),
            (&["Turjal"], "", "all known words", "all known words"),
        ];

        let cuts = cases
            .map(|(unknown, typed_letters, suggestion, _)| cut_without(unknown, typed_letters, suggestion));

        assert_eq!(cuts, cases.map(|(_, _, _, expected)| expected.to_owned()));
    }

    #[test]
    fn only_plain_words_of_three_letters_or_more_are_checked() {
        let runs = [
            "15th",
            "example.com",
            "useState",
            "po",
            "ok,",
            "¿Qué",
            "don't",
            "well-known",
            "NASA",
        ];

        let checked = runs.map(|run| checked_words(run).collect::<Vec<_>>());

        assert_eq!(
            checked,
            [
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec!["Qué"],
                vec!["don't"],
                vec!["well", "known"],
                vec!["NASA"],
            ]
        );
    }
}
