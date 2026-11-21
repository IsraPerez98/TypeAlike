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

    /// Returns the words that the dictionary offers in place of `word`, a run of letters with no
    /// whitespace that it does not know, in any language that the user writes. A platform that
    /// offers no words gives none.
    fn guesses(&self, word: &str) -> Vec<String>;
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

/// Returns how many edits turn `left` into `right`: a swap of two letters next to each other, a
/// change, an extra or a missing letter count one edit each.
#[must_use]
pub fn edit_distance(left: &[char], right: &[char]) -> usize {
    let mut rows = vec![(0..=right.len()).collect::<Vec<_>>()];
    for (row, &left_char) in left.iter().enumerate() {
        let mut current = vec![row + 1; right.len() + 1];
        for (column, &right_char) in right.iter().enumerate() {
            let above = &rows[row];
            let change = above[column] + usize::from(left_char != right_char);
            let is_swap =
                row > 0 && column > 0 && left_char == right[column - 1] && left[row - 1] == right_char;
            let cost = change.min(above[column + 1] + 1).min(current[column] + 1);
            let swap = is_swap.then(|| rows[row - 1][column - 1] + 1);
            current[column + 1] = swap.map_or(cost, |swap| swap.min(cost));
        }
        rows.push(current);
    }
    rows.last()
        .and_then(|row| row.last())
        .copied()
        .unwrap_or_default()
}

/// Whether the `fixed` word starts with the first letter of the `typed` letters, or with their
/// first two letters swapped. A fix of the first letter more often turns a name into another word.
#[must_use]
pub fn keeps_start(typed: &[char], fixed: &str) -> bool {
    let mut start = fixed.chars();
    let (first, second) = (start.next(), start.next());
    first == typed.first().copied() || (first == typed.get(1).copied() && second == typed.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edits_between(left: &str, right: &str) -> usize {
        edit_distance(
            &left.chars().collect::<Vec<_>>(),
            &right.chars().collect::<Vec<_>>(),
        )
    }

    #[test]
    fn one_swap_change_extra_or_missing_letter_is_one_edit() {
        let pairs = [
            ("dlei", "deli"),
            ("delu", "deli"),
            ("dleli", "deli"),
            ("del", "deli"),
        ];

        let distances = pairs.map(|(left, right)| edits_between(left, right));

        assert_eq!(distances, [1, 1, 1, 1]);
    }

    #[test]
    fn two_changes_are_two_edits() {
        assert_eq!(edits_between("dxlx", "deli"), 2);
    }

    #[test]
    fn fix_keeps_the_start_with_the_same_first_letter_or_the_first_two_letters_swapped() {
        let pairs = [("dlei", "deli"), ("hte", "the"), ("Car", "parte")];

        let answers = pairs.map(|(typed, fixed)| keeps_start(&typed.chars().collect::<Vec<_>>(), fixed));

        assert_eq!(answers, [true, true, false]);
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
