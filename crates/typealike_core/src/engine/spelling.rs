//! Cuts a suggestion before a word that the model made up: a word that the dictionary of the OS
//! does not know and that the writer did not write. Also tells the typed letters that the typo
//! rule must not take for a typo.

use std::collections::{HashMap, HashSet};

use crate::inference::WordBias;
use crate::spelling::{SpellChecker, checked_words};

use super::Engine;
use super::session::Session;
use super::suggestion::{word_starts, words};

/// The most answers of the dictionary that the engine keeps. Each answer costs a call to the OS.
const MAX_KEPT_ANSWERS: usize = 10_000;
/// The fewest checked words that the writer finished before the caret that tell whether the
/// dictionary fits the language of the writer.
const MIN_FIT_WORDS: usize = 3;
/// The most checked words that the writer finished before the caret, from the last, that tell
/// whether the dictionary fits the language of the writer.
const MAX_FIT_WORDS: usize = 30;
/// The fewest typed letters that only the dictionaries of the language of the text check after a
/// fix. Fewer letters that another language knows as a word are often the start of a longer word
/// of the text, such as "hart" for "harto" in a Spanish text.
const MIN_LANGUAGE_LETTERS: usize = 5;

/// The dictionaries of the languages that the user writes, with the answers that they gave. A
/// word is known when the dictionary of any language knows it. Without a dictionary, it knows each
/// word and no start of a word, as a platform that cannot check them.
pub(super) struct Dictionary {
    languages: Vec<LanguageDictionary>,
    word_starts: HashMap<String, bool>,
    /// Whether the dictionary knew most words of the last text of the writer that had enough of
    /// them ([`Dictionary::fits_writer`]).
    fits_writer: bool,
}

/// The dictionary of one language that the user writes, with the answers that it gave.
struct LanguageDictionary {
    spell_checker: Box<dyn SpellChecker>,
    answers: HashMap<String, bool>,
}

impl LanguageDictionary {
    fn knows(&mut self, word: &str) -> bool {
        let spell_checker = &self.spell_checker;
        kept_answer(&mut self.answers, word, |word| spell_checker.is_known_word(word))
    }

    /// Whether the dictionary knows at least half of `words`.
    fn knows_most(&mut self, words: &[&str]) -> bool {
        let known = words.iter().filter(|word| self.knows(word)).count();
        known * 2 >= words.len()
    }
}

impl Dictionary {
    fn new(spell_checkers: Vec<Box<dyn SpellChecker>>) -> Self {
        Self {
            languages: spell_checkers
                .into_iter()
                .map(|spell_checker| LanguageDictionary {
                    spell_checker,
                    answers: HashMap::new(),
                })
                .collect(),
            word_starts: HashMap::new(),
            fits_writer: true,
        }
    }

    /// Whether the dictionary fits the language of the writer: it knows at least half of the last
    /// [`MAX_FIT_WORDS`] checked words that the writer finished in `text_before`. With fewer than
    /// [`MIN_FIT_WORDS`] such words, the answer for the last text with enough of them counts, and
    /// before any such text the dictionary fits. A dictionary of another language does not know
    /// most words of the writer, so its cut would hide good suggestions.
    fn fits_writer(&mut self, text_before: &str) -> bool {
        let typed_words = finished_words(text_before);
        if typed_words.len() >= MIN_FIT_WORDS {
            let known = typed_words.iter().filter(|word| self.knows(word)).count();
            self.fits_writer = known * 2 >= typed_words.len();
        }
        self.fits_writer
    }

    fn knows(&mut self, word: &str) -> bool {
        self.languages.is_empty() || self.languages.iter_mut().any(|language| language.knows(word))
    }

    fn knows_start(&mut self, letters: &str) -> bool {
        let languages = &self.languages;
        kept_answer(&mut self.word_starts, letters, |letters| {
            languages
                .iter()
                .any(|language| language.spell_checker.is_word_start(letters))
        })
    }

    /// Whether a dictionary of the language of `text_before` checks the typed letters of its last
    /// word and knows them as a word. The dictionary of a language fits the text when it knows at
    /// least half of the last [`MAX_FIT_WORDS`] checked words that the writer finished in
    /// `text_before`. With fewer than [`MIN_FIT_WORDS`] such words, fewer than
    /// [`MIN_LANGUAGE_LETTERS`] typed letters, or when no dictionary fits, the dictionary of each
    /// language counts. Thus "reunion" in a Spanish text is no known word for an English and
    /// Spanish writer.
    pub(super) fn knows_typed_word(&mut self, text_before: &str) -> bool {
        let Some(letters) = typed_letters(text_before) else {
            return false;
        };
        if self.languages.is_empty() {
            return true;
        }
        let typed_words = finished_words(text_before);
        let has_language =
            typed_words.len() >= MIN_FIT_WORDS && letters.chars().count() >= MIN_LANGUAGE_LETTERS;
        let fits = self
            .languages
            .iter_mut()
            .map(|language| !has_language || language.knows_most(&typed_words))
            .collect::<Vec<_>>();
        let has_fit = fits.contains(&true);
        self.languages
            .iter_mut()
            .zip(fits)
            .filter(|&(_, fits)| fits || !has_fit)
            .any(|(language, _)| language.knows(letters))
    }

    /// Whether the dictionary knows the typed letters of the last word of `text_before` as a word
    /// or the start of a word. Letters that it does not check ([`checked_words`]), such as a word
    /// with fewer than 3 letters, count as known.
    pub(super) fn knows_typed_start(&mut self, text_before: &str) -> bool {
        typed_letters(text_before).is_none_or(|letters| self.knows_start(letters))
    }
}

/// Returns the last [`MAX_FIT_WORDS`] checked words that the writer finished in `text_before`,
/// from the last.
fn finished_words(text_before: &str) -> Vec<&str> {
    let finished = text_before.trim_end_matches(|character: char| !character.is_whitespace());
    finished
        .split_whitespace()
        .rev()
        .flat_map(checked_words)
        .take(MAX_FIT_WORDS)
        .collect()
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
/// caret of a session, the words of its word bias, and the words of the earlier writing.
pub(super) struct WrittenWords<'session> {
    pub(super) session: &'session Session,
    pub(super) word_bias: Option<&'session WordBias>,
    /// The words of the earlier writing in lowercase.
    pub(super) history_words: &'session HashSet<String>,
}

impl WrittenWords<'_> {
    /// Whether the writer wrote or prefers `word`, in any case.
    fn holds(&self, word: &str) -> bool {
        let lowercase = word.to_lowercase();
        let session = self.session;
        let is_in_session = [&session.preface.text, &session.text_before, &session.text_after]
            .into_iter()
            .any(|text| words(text).any(|written| written == lowercase));
        is_in_session
            || self.history_words.contains(&lowercase)
            || self.word_bias.is_some_and(|bias| {
                bias.words
                    .iter()
                    .any(|(written, _)| written.to_lowercase() == lowercase)
            })
    }
}

impl Engine {
    /// Cuts each suggestion of the model before a word that no one of `spell_checkers`, the
    /// dictionary of each language that the user writes, knows, and that the writer did not
    /// write. Where the typo rule took typed letters that they know for a typo, asks again without
    /// the rule.
    pub(crate) fn use_spell_checkers(&mut self, spell_checkers: Vec<Box<dyn SpellChecker>>) {
        self.dictionary = Some(Dictionary::new(spell_checkers));
    }
}

/// Returns `suggestion`, the ghost text after `text_before`, up to its first word that the
/// `dictionary` does not know and that the `written` words do not hold, without the whitespace
/// before that word. A first word that finishes the last word of `text_before` counts with its
/// typed letters. Without a dictionary, or with one that does not fit the language of the writer
/// ([`Dictionary::fits_writer`]), it returns all of `suggestion`.
pub(super) fn known_start<'text>(
    dictionary: Option<&mut Dictionary>,
    text_before: &str,
    suggestion: &'text str,
    written: &WrittenWords<'_>,
) -> &'text str {
    let Some(dictionary) = dictionary else {
        return suggestion;
    };
    if !dictionary.fits_writer(text_before) {
        return suggestion;
    }
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::test_support::FakeSpellChecker;

    fn dictionary_of(spell_checker: FakeSpellChecker) -> Dictionary {
        Dictionary::new(vec![Box::new(spell_checker)])
    }

    #[test]
    fn dictionaries_know_a_word_that_any_of_them_knows() {
        let mut dictionary = Dictionary::new(vec![
            Box::new(FakeSpellChecker::without(&["casa", "zorbly"])),
            Box::new(FakeSpellChecker::without(&["house", "zorbly"])),
        ]);

        let answers = ["casa", "house", "zorbly"].map(|word| dictionary.knows(word));

        assert_eq!(answers, [true, true, false]);
    }

    #[test]
    fn no_dictionary_knows_each_word_and_no_start_of_a_word() {
        let mut dictionary = Dictionary::new(Vec::new());

        let answers = (dictionary.knows("zorbly"), dictionary.knows_start("zor"));

        assert_eq!(answers, (true, false));
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
}
