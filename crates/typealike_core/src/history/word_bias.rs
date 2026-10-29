//! The preference of a completion for the words that the writer wrote often, on more than one
//! day.

use std::collections::HashMap;

use super::entry::{HistoryEntry, MAX_COUNT_PER_GROUP, counted_groups};
use super::redaction::REDACTED;
use super::words::{is_distinctive_word, small_count, words};
use crate::desktop::Timestamp;
use crate::inference::WordBias;
use crate::settings::WordChoice;

/// The smallest count of a word that suggestions prefer. One earlier day adds at most
/// [`MAX_COUNT_PER_GROUP`], so a word of one long conversation of an earlier day is a word of its
/// topic, not of the writer.
const MIN_WORD_COUNT: u32 = 3;
/// The count of a word that gives it the full strength of the word choice.
const FULL_WEIGHT_COUNT: u16 = 10;

/// The use of one word in any case: the times of each spelling, and the count of the word.
#[derive(Default)]
struct WordUse {
    spellings: Vec<(String, u32)>,
    count: u32,
}

impl WordUse {
    fn add(&mut self, spelling: &str, times: u32) {
        match self.spellings.iter_mut().find(|(known, _)| known == spelling) {
            Some((_, count)) => *count += times,
            None => self.spellings.push((spelling.to_owned(), times)),
        }
    }
}

/// Returns the preference of `choice` for the words of `entries`, oldest first, at the time `now`,
/// with `version`, or `None` when the choice is off. A word is a run of letters and apostrophes
/// that [`is_distinctive_word`] accepts, with a count of at least [`MIN_WORD_COUNT`] in any case.
/// Each group of [`counted_groups`] adds its times of the word, at most
/// [`MAX_COUNT_PER_GROUP`]. The preference has the spelling that the entries use most.
pub(super) fn word_bias(
    entries: &[&HistoryEntry],
    choice: WordChoice,
    version: u64,
    now: Option<Timestamp>,
) -> Option<WordBias> {
    let mut uses = HashMap::<String, WordUse>::new();
    for group in counted_groups(entries, now) {
        let mut times_in_group = HashMap::<String, u32>::new();
        for word in group
            .iter()
            .flat_map(|entry| entry.text.split(REDACTED).flat_map(words))
        {
            let lowercase = word.to_lowercase();
            uses.entry(lowercase.clone()).or_default().add(word, 1);
            *times_in_group.entry(lowercase).or_default() += 1;
        }
        for (lowercase, times) in times_in_group {
            if let Some(word_use) = uses.get_mut(&lowercase) {
                word_use.count += times.min(MAX_COUNT_PER_GROUP);
            }
        }
    }
    bias_of(uses, choice, version)
}

/// Returns the preference of `choice` for the words of `counts`, each with its count, or `None`
/// when the choice is off. Spellings that differ only in case count as one word, and the
/// preference has the spelling with the highest count, and the lowercase one when two have the
/// same count. A word needs at least 3 letters and a count of 3, and the most common words of
/// English and Spanish get no preference. A word with a count of 10 or more gets the full strength
/// of the choice.
#[must_use]
pub fn word_bias_of_counts(
    counts: impl IntoIterator<Item = (String, u32)>,
    choice: WordChoice,
    version: u64,
) -> Option<WordBias> {
    let mut uses = HashMap::<String, WordUse>::new();
    for (word, count) in counts {
        let word_use = uses.entry(word.to_lowercase()).or_default();
        word_use.add(&word, count);
        word_use.count += count;
    }
    bias_of(uses, choice, version)
}

/// Returns the preference of `choice` for the words of `uses`, each in lowercase with its use.
fn bias_of(uses: HashMap<String, WordUse>, choice: WordChoice, version: u64) -> Option<WordBias> {
    let strength = strength_of(choice)?;
    let full_weight = f32::from(FULL_WEIGHT_COUNT).ln();
    let mut words = uses
        .into_iter()
        .filter_map(|(lowercase, word_use)| {
            let is_wanted = word_use.count >= MIN_WORD_COUNT && is_distinctive_word(&lowercase);
            let (usual, _) = word_use.spellings.into_iter().max_by_key(|(spelling, count)| {
                (
                    *count,
                    *spelling == lowercase,
                    std::cmp::Reverse(spelling.clone()),
                )
            })?;
            let capped = small_count(word_use.count).min(f32::from(FULL_WEIGHT_COUNT));
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
    use crate::history::test_support::{at_hour, mail, mail_on_days, on_day, on_own_days, refs};

    fn words_of(bias: Option<WordBias>) -> Option<Vec<(String, f32)>> {
        bias.map(|bias| bias.words.to_vec())
    }

    fn spellings_of(bias: Option<WordBias>) -> Option<Vec<String>> {
        words_of(bias).map(|words| words.into_iter().map(|(word, _)| word).collect())
    }

    #[test]
    fn word_bias_holds_the_words_of_three_letters_written_on_three_days() {
        let entries = on_own_days([
            mail("Cheers mate, ok ok ok."),
            mail("Cheers MATE!"),
            mail("Cheers. Mate's"),
        ]);

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1, None);

        assert_eq!(
            words_of(bias),
            Some(vec![("Cheers".to_owned(), 3.0_f32.ln() / 10.0_f32.ln())])
        );
    }

    #[test]
    fn word_of_one_day_gets_no_preference_however_often_it_comes() {
        let entries = vec![on_day(mail("reckon reckon reckon reckon"), 1); 5];

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1, None);

        assert_eq!(words_of(bias), Some(Vec::new()));
    }

    #[test]
    fn texts_without_a_time_count_as_one_day() {
        let entries = vec![mail("reckon reckon"); 3];

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1, None);

        assert_eq!(words_of(bias), Some(Vec::new()));
    }

    #[test]
    fn word_bias_has_the_spelling_that_the_writer_uses_most() {
        let entries = mail_on_days("Bluefin bluefin Bluefin. reckon Reckon reckon", 3);

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1, None);

        assert_eq!(
            spellings_of(bias),
            Some(vec!["Bluefin".to_owned(), "reckon".to_owned()])
        );
    }

    #[test]
    fn word_bias_has_the_lowercase_spelling_when_two_spellings_come_the_same_times() {
        let entries = mail_on_days("Cheers cheers", 3);

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1, None);

        assert_eq!(spellings_of(bias), Some(vec!["cheers".to_owned()]));
    }

    #[test]
    fn word_of_ten_days_gets_the_full_strength_of_the_choice() {
        let entries = mail_on_days("reckon", 10);
        let choices = [WordChoice::Light, WordChoice::Medium, WordChoice::Strong];

        let biases = choices.map(|choice| words_of(word_bias(&refs(&entries), choice, 1, None)));

        assert_eq!(
            biases,
            [0.5, 1.0, 2.0].map(|strength| Some(vec![("reckon".to_owned(), strength)]))
        );
    }

    #[test]
    fn common_words_get_no_preference() {
        let entries = mail_on_days("the reckon que", 10);

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1, None);

        assert_eq!(words_of(bias), Some(vec![("reckon".to_owned(), 1.0)]));
    }

    #[test]
    fn word_choice_off_gives_no_bias() {
        let entries = mail_on_days("reckon", 10);

        let bias = word_bias(&refs(&entries), WordChoice::Off, 1, None);

        assert_eq!(bias, None);
    }

    #[test]
    fn redacted_secrets_are_not_words_of_the_writer() {
        let entries = mail_on_days("and [redacted] or [redacted]", 3);

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1, None);

        assert_eq!(words_of(bias), Some(Vec::new()));
    }

    #[test]
    fn days_of_each_spelling_add_up() {
        let days = [("Reckon".to_owned(), 2), ("reckon".to_owned(), 1)];

        let bias = word_bias_of_counts(days, WordChoice::Medium, 1);

        assert_eq!(
            words_of(bias),
            Some(vec![("Reckon".to_owned(), 3.0_f32.ln() / 10.0_f32.ln())])
        );
    }

    #[test]
    fn each_text_of_the_last_day_counts_and_an_older_day_counts_at_most_two_times() {
        let entries = vec![on_day(mail("reckon"), 5); 3];
        let clocks = [at_hour(5, 18), at_hour(7, 12)];

        let biases = clocks.map(|now| {
            word_bias(&refs(&entries), WordChoice::Medium, 1, Some(now)).map(|bias| bias.words.len())
        });

        assert_eq!(biases, [Some(1), Some(0)]);
    }

    #[test]
    fn older_day_and_one_recent_text_give_a_preference() {
        let mut entries = vec![on_day(mail("reckon"), 1); 5];
        entries.push(on_day(mail("reckon"), 5));

        let bias = word_bias(&refs(&entries), WordChoice::Medium, 1, Some(at_hour(5, 18)));

        assert_eq!(
            words_of(bias),
            Some(vec![("reckon".to_owned(), 3.0_f32.ln() / 10.0_f32.ln())])
        );
    }
}
