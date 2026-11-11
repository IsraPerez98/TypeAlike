//! The spelling dictionaries of the languages that the user writes, which tell which words a
//! suggestion may write: the dictionaries of the OS, and on an OS that has few of them, the
//! dictionaries that TypeAlike downloads.

mod dictionaries;
mod languages;

use crate::history::is_apostrophe;

pub use dictionaries::{
    DictionaryEvent, DictionaryEventSink, DictionaryHub, DictionaryRequest, DownloadableDictionary,
    OsDictionaries,
};
pub use languages::{WRITING_LANGUAGES, language_name};
pub(crate) use languages::{is_main_entry, writing_language_of};

/// The fewest letters of a word that the dictionary checks. Shorter words of a chat, such as "ok",
/// "xd" or "po", are often in no dictionary.
const MIN_CHECKED_LETTERS: usize = 3;

/// The signs at the start of a command option or a path, such as `--oneline`, `.gitignore`,
/// `/tmp`, `~/.ssh` or `*.json`. The letters after them are often in no dictionary, and few words
/// of a sentence start with them.
const OPTION_AND_PATH_STARTS: [char; 5] = ['-', '.', '/', '~', '*'];

/// The spelling dictionaries of the languages that the user writes. Core asks them about each word
/// that a suggestion writes, so that a suggestion never writes a word that the model made up.
pub trait SpellChecker {
    /// Whether the dictionary knows `word`, a run of letters with no whitespace, in any language
    /// that the user writes. A platform that cannot check `word` answers `true`.
    fn is_known_word(&self, word: &str) -> bool;

    /// Whether the dictionary knows `letters`, a run of letters with no whitespace, as a word or
    /// as the start of a word, in any language that the user writes. A platform that cannot check
    /// `letters` answers `false`.
    fn is_word_start(&self, letters: &str) -> bool;
}

/// Returns the words of `run`, a run of text with no whitespace, that the dictionary checks: its
/// parts between hyphens, without the signs around it, with at least 3 letters. A run with a digit
/// or another sign inside, such as a number, an address or a name in code, with a capital letter
/// inside, such as `useState`, or with a sign of a command option or a path at its start, such as
/// `--oneline`, `.gitignore` or `~/.ssh`, has none.
pub fn checked_words(run: &str) -> impl Iterator<Item = &str> {
    let word = run.trim_matches(|character: char| !character.is_alphabetic());
    let is_plain = word
        .chars()
        .all(|character| character.is_alphabetic() || character == '-' || is_apostrophe(character));
    let has_inner_capital =
        word.chars().skip(1).any(char::is_uppercase) && word.chars().any(char::is_lowercase);
    let is_option_or_path = run.starts_with(OPTION_AND_PATH_STARTS);
    let is_checked = is_plain && !has_inner_capital && !is_option_or_path;
    let checked = if is_checked { word } else { "" };
    checked.split('-').filter(|part| {
        part.chars().filter(|character| character.is_alphabetic()).count() >= MIN_CHECKED_LETTERS
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn run_that_starts_with_a_sign_of_an_option_or_a_path_is_not_checked() {
        let runs = [
            "--oneline",
            "-Recurse",
            "--force-with-lease",
            ".gitignore",
            "/tmp",
            "~/.ssh",
            "*.json",
        ];

        let checked = runs.map(|run| checked_words(run).collect::<Vec<_>>());

        assert_eq!(checked, runs.map(|_| Vec::<&str>::new()));
    }

    #[test]
    fn word_with_a_sign_after_it_or_another_sign_before_it_is_checked() {
        let runs = ["well-knwn", "knwn-", "knwn/", "—knwn", "(knwn)", "$knwn"];

        let checked = runs.map(|run| checked_words(run).collect::<Vec<_>>());

        assert_eq!(
            checked,
            [
                vec!["well", "knwn"],
                vec!["knwn"],
                vec!["knwn"],
                vec!["knwn"],
                vec!["knwn"],
                vec!["knwn"],
            ]
        );
    }
}
