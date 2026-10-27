//! The preference of a completion for the words that the writer wrote often.

use std::collections::HashMap;

use super::entry::HistoryEntry;
use super::redaction::REDACTED;
use super::words::{is_distinctive_word, small_count, words};
use crate::inference::WordBias;
use crate::settings::WordChoice;

/// The fewest times that the writer wrote a word that suggestions prefer.
const MIN_WORD_COUNT: u32 = 3;
/// The times of a word that give it the full strength of the word choice.
const FULL_WEIGHT_COUNT: u16 = 20;

/// Returns the preference of `choice` for the words of `entries`, with `version`, or `None` when
/// the choice is off. A word is a run of letters and apostrophes that [`is_distinctive_word`]
/// accepts, which the entries hold at least [`MIN_WORD_COUNT`] times in any case. The preference
/// has the spelling that the entries use most.
pub(super) fn word_bias(entries: &[&HistoryEntry], choice: WordChoice, version: u64) -> Option<WordBias> {
    let mut counts = HashMap::<String, u32>::new();
    for entry in entries {
        for word in entry.text.split(REDACTED).flat_map(words) {
            *counts.entry(word.to_owned()).or_default() += 1;
        }
    }
    word_bias_of_counts(counts, choice, version)
}

/// Returns the preference of `choice` for the words of `counts`, each with the number of times
/// that the writer wrote it, or `None` when the choice is off. Spellings that differ only in case
/// count as one word, and the preference has the spelling that comes most, and the lowercase one
/// when two come the same number of times. A word needs at least 3 letters and 3 times, and the
/// most common words of English and Spanish get no preference. A word that comes 20 times or more
/// gets the full strength of the choice.
#[must_use]
pub fn word_bias_of_counts(
    counts: impl IntoIterator<Item = (String, u32)>,
    choice: WordChoice,
    version: u64,
) -> Option<WordBias> {
    let strength = strength_of(choice)?;
    let full_weight = f32::from(FULL_WEIGHT_COUNT).ln();
    let mut spellings = HashMap::<String, Vec<(String, u32)>>::new();
    for (word, count) in counts {
        spellings
            .entry(word.to_lowercase())
            .or_default()
            .push((word, count));
    }
    let mut words = spellings
        .into_iter()
        .filter_map(|(lowercase, spellings)| {
            let count = spellings.iter().map(|(_, count)| count).sum::<u32>();
            let is_wanted = count >= MIN_WORD_COUNT && is_distinctive_word(&lowercase);
            let (usual, _) = spellings.into_iter().max_by_key(|(spelling, count)| {
                (
                    *count,
                    *spelling == lowercase,
                    std::cmp::Reverse(spelling.clone()),
                )
            })?;
            let capped = small_count(count).min(f32::from(FULL_WEIGHT_COUNT));
            is_wanted.then(|| (usual, strength * capped.ln() / full_weight))
        })
        .collect::<Vec<_>>();
    words.sort_by(|left, right| left.0.cmp(&right.0));
    Some(WordBias {
        version,
        words: words.into(),
    })
}

/// Returns the most logits that `choice` adds to a word, or `None` when the choice is off.
pub(super) fn strength_of(choice: WordChoice) -> Option<f32> {
    match choice {
        WordChoice::Off => None,
        WordChoice::Light => Some(0.5),
        WordChoice::Medium => Some(1.0),
        WordChoice::Strong => Some(2.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{mail, refs};

    fn words_of(bias: Option<WordBias>) -> Option<Vec<(String, f32)>> {
        bias.map(|bias| bias.words.to_vec())
    }

    #[test]
    fn word_bias_holds_the_words_of_three_letters_that_come_three_times() {
        let entries = [mail("Cheers mate, ok ok ok. Cheers"), mail("Cheers MATE! Mate's")];

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1);

        assert_eq!(
            words_of(bias),
            Some(vec![("Cheers".to_owned(), 3.0_f32.ln() / 20.0_f32.ln())])
        );
    }

    #[test]
    fn word_bias_has_the_spelling_that_the_writer_uses_most() {
        let entries = [mail("Bluefin bluefin Bluefin. reckon Reckon reckon")];

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1);

        let spellings =
            words_of(bias).map(|words| words.into_iter().map(|(word, _)| word).collect::<Vec<_>>());
        assert_eq!(spellings, Some(vec!["Bluefin".to_owned(), "reckon".to_owned()]));
    }

    #[test]
    fn word_bias_has_the_lowercase_spelling_when_two_spellings_come_the_same_times() {
        let entries = [mail("Cheers cheers Cheers cheers")];

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1);

        let spellings =
            words_of(bias).map(|words| words.into_iter().map(|(word, _)| word).collect::<Vec<_>>());
        assert_eq!(spellings, Some(vec!["cheers".to_owned()]));
    }

    #[test]
    fn word_that_comes_often_gets_the_full_strength_of_the_choice() {
        let entries = [mail(&"reckon ".repeat(25))];
        let choices = [WordChoice::Light, WordChoice::Medium, WordChoice::Strong];

        let biases = choices.map(|choice| words_of(word_bias(&refs(&entries), choice, 1)));

        assert_eq!(
            biases,
            [0.5, 1.0, 2.0].map(|strength| Some(vec![("reckon".to_owned(), strength)]))
        );
    }

    #[test]
    fn common_words_get_no_preference() {
        let entries = [mail(&"the reckon que ".repeat(25))];

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1);

        assert_eq!(words_of(bias), Some(vec![("reckon".to_owned(), 1.0)]));
    }

    #[test]
    fn word_choice_off_gives_no_bias() {
        let entries = [mail(&"reckon ".repeat(25))];

        let bias = word_bias(&refs(&entries), WordChoice::Off, 1);

        assert_eq!(bias, None);
    }

    #[test]
    fn redacted_secrets_are_not_words_of_the_writer() {
        let entries = [mail("key [redacted] and [redacted] or [redacted]")];

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1);

        assert_eq!(words_of(bias), Some(Vec::new()));
    }
}
