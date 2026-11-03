//! Cuts a suggestion before a word that the model made up: a word that the dictionary of the OS
//! does not know and that the writer did not write. Also tells the typed letters that the typo
//! rule must not take for a typo.

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
    word_starts: HashMap<String, bool>,
}

impl Dictionary {
    fn knows(&mut self, word: &str) -> bool {
        let spell_checker = &self.spell_checker;
        kept_answer(&mut self.answers, word, |word| spell_checker.is_known_word(word))
    }

    fn knows_start(&mut self, letters: &str) -> bool {
        let spell_checker = &self.spell_checker;
        kept_answer(&mut self.word_starts, letters, |letters| {
            spell_checker.is_word_start(letters)
        })
    }

    /// Whether the dictionary checks the typed letters of the last word of `text_before` and knows
    /// them as a word.
    pub(super) fn knows_typed_word(&mut self, text_before: &str) -> bool {
        typed_letters(text_before).is_some_and(|letters| self.knows(letters))
    }

    /// Whether the dictionary knows the typed letters of the last word of `text_before` as a word
    /// or the start of a word. Letters that it does not check, such as a word with fewer than
    /// [`MIN_CHECKED_LETTERS`] letters, count as known.
    pub(super) fn knows_typed_start(&mut self, text_before: &str) -> bool {
        typed_letters(text_before).is_none_or(|letters| self.knows_start(letters))
    }
}

/// Returns the typed letters of the last word of `text_before` that the dictionary checks.
fn typed_letters(text_before: &str) -> Option<&str> {
    let run = text_before.rsplit(char::is_whitespace).next().unwrap_or_default();
    checked_words(run).last()
}

/// Returns the answer in `answers` for `text`, else asks for it and keeps it.
fn kept_answer(answers: &mut HashMap<String, bool>, text: &str, ask: impl FnOnce(&str) -> bool) -> bool {
    if answers.len() >= MAX_KEPT_ANSWERS {
        answers.clear();
    }
    *answers.entry(text.to_owned()).or_insert_with(|| ask(text))
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
    /// the writer did not write. Where the typo rule took typed letters that `spell_checker` knows
    /// for a typo, asks again without the rule.
    pub(crate) fn use_spell_checker(&mut self, spell_checker: Box<dyn SpellChecker>) {
        self.dictionary = Some(Dictionary {
            spell_checker,
            answers: HashMap::new(),
            word_starts: HashMap::new(),
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
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::test_support::FakeSpellChecker;

    fn dictionary_of(spell_checker: FakeSpellChecker) -> Dictionary {
        Dictionary {
            spell_checker: Box::new(spell_checker),
            answers: HashMap::new(),
            word_starts: HashMap::new(),
        }
    }

    #[test]
    fn typed_start_is_the_last_checked_word_of_the_last_run() {
        let mut dictionary = dictionary_of(FakeSpellChecker {
            unknown_starts: vec!["kno", "Frid"],
            ..FakeSpellChecker::default()
        });
        let texts = [
            "a well-kno",
            "on (Frid",
            "on Friday?",
            "the 15t",
            "on Fr",
            "a known-word",
        ];

        let answers = texts.map(|text| dictionary.knows_typed_start(text));

        assert_eq!(answers, [false, false, true, true, true, true]);
    }

    #[test]
    fn typed_word_is_known_only_when_the_dictionary_checks_it() {
        let mut dictionary = dictionary_of(FakeSpellChecker::without(&["teh"]));
        let texts = ["the ope", "the teh", "the B", "the 15t"];

        let answers = texts.map(|text| dictionary.knows_typed_word(text));

        assert_eq!(answers, [true, false, false, false]);
    }

    #[test]
    fn dictionary_asks_about_the_same_start_once() {
        let checked_words = Rc::new(Cell::new(0));
        let mut dictionary = dictionary_of(FakeSpellChecker {
            checked_words: Rc::clone(&checked_words),
            ..FakeSpellChecker::default()
        });
        dictionary.knows_typed_start("before Frid");

        dictionary.knows_typed_start("before Frid");

        assert_eq!(checked_words.get(), 1);
    }

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
