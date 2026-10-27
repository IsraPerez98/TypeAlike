//! The phrases that the writer repeats and the words that the writer wrote after the same words.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;

use super::entry::HistoryEntry;
use super::redaction::REDACTED;
use super::word_bias::strength_of;
use super::words::{is_apostrophe, runs_of, small_count};
use crate::inference::NextWordBias;
use crate::settings::WordChoice;

/// The words before the next word of a phrase.
const PHRASE_CONTEXT_WORDS: usize = 3;
/// The fewest times that the writer wrote the same next word after the same words, for a
/// suggestion of that word without the model.
const MIN_PHRASE_COUNT: u32 = 3;
/// The smallest share, in percent, of the next word among all the words that the writer wrote
/// after the same words.
const MIN_PHRASE_PERCENT: u32 = 60;
/// The words of the runs before a next word that give a preference for it, longest first.
const RUN_WORDS: [usize; 2] = [PHRASE_CONTEXT_WORDS, 2];
/// The fewest times that the writer wrote a run of words, for a preference for its next words.
const MIN_RUN_COUNT: u32 = 2;
/// The most next words that a completion prefers.
const MAX_NEXT_WORDS: usize = 8;

/// What the writer wrote after each run of [`RUN_WORDS`] words in any case, for the runs that the
/// writer wrote at least [`MIN_RUN_COUNT`] times: the phrases that the writer repeats, and the
/// words that the writer wrote after the last words before the caret. `Debug` shows only the
/// number of runs, so logs never hold what the user wrote.
#[derive(Default)]
pub(super) struct Phrases {
    /// The logits for a word that always came after its run.
    strength: f32,
    /// The followers of the runs of each number of words, longest first.
    runs: Vec<RunFollowers>,
}

/// For each run of `context_words` words, the texts after it up to the end of the next word, with
/// the signs before that word, and their counts.
struct RunFollowers {
    context_words: usize,
    followers: Followers,
}

type Followers = HashMap<String, Vec<(String, u32)>>;

impl fmt::Debug for Phrases {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let runs = self.runs.iter().map(|run| run.followers.len()).sum::<usize>();
        formatter
            .debug_struct("Phrases")
            .field("runs", &runs)
            .finish_non_exhaustive()
    }
}

impl Phrases {
    fn followers_of(&self, context_words: usize) -> Option<&Followers> {
        self.runs
            .iter()
            .find(|run| run.context_words == context_words)
            .map(|run| &run.followers)
    }
}

/// Returns the rest of a phrase after `text_before`, the text before the caret, up to the end of
/// its next word, or `None`. The text can end inside that word. The rest never goes past the end
/// of the line. The first of `tables` that has the [`PHRASE_CONTEXT_WORDS`] words before the next
/// word decides. A phrase needs the same next word, with the same signs before it, at least
/// [`MIN_PHRASE_COUNT`] times and in at least [`MIN_PHRASE_PERCENT`] of the times that the writer
/// wrote these words.
pub(super) fn continuation_in<'tables>(
    tables: &[&'tables Phrases],
    text_before: &str,
) -> Option<&'tables str> {
    let spans = runs_of(text_before, is_phrase_character);
    let ends_inside_word = text_before.ends_with(is_phrase_character);
    let partial_word = ends_inside_word.then(|| spans.len().checked_sub(1)).flatten();
    [partial_word, Some(spans.len())]
        .into_iter()
        .flatten()
        .find_map(|context_end| {
            let (key, typed) = run_before(text_before, &spans, context_end, PHRASE_CONTEXT_WORDS)?;
            let counts = tables
                .iter()
                .find_map(|table| table.followers_of(PHRASE_CONTEXT_WORDS)?.get(&key))?;
            let rest = repeated_follower(counts)?.strip_prefix(typed)?;
            (!rest.is_empty() && !rest.contains(['\r', '\n'])).then_some(rest)
        })
}

/// Returns the preference for the words that the writer wrote after the last words of
/// `text_before`, the text before the caret: the words after the longest run of the first of
/// `tables` that has some. Each word gets the strength times its share of the words after the run.
/// When the text ends inside a word, the run ends before that word and the words start with its
/// letters. A word gets no preference when a line break or no space comes before it.
pub(super) fn next_word_bias_in(tables: &[&Phrases], text_before: &str) -> NextWordBias {
    let spans = runs_of(text_before, is_phrase_character);
    let partial_words = usize::from(text_before.ends_with(is_phrase_character));
    let context_end = spans.len().saturating_sub(partial_words);
    let words = tables
        .iter()
        .flat_map(|table| table.runs.iter().map(|run| (table.strength, run)))
        .find_map(|(strength, run)| {
            let (key, typed) = run_before(text_before, &spans, context_end, run.context_words)?;
            let words = preferred_words(run.followers.get(&key)?, typed, strength);
            (!words.is_empty()).then_some(words)
        });
    NextWordBias(words.unwrap_or_default())
}

/// Returns the key of the `context_words` words of `spans` that end at the span `context_end`, and
/// the typed letters of `text_before` after these words. The spans are the words of `text_before`.
fn run_before<'text>(
    text_before: &'text str,
    spans: &[Range<usize>],
    context_end: usize,
    context_words: usize,
) -> Option<(String, &'text str)> {
    let context = spans.get(context_end.checked_sub(context_words)?..context_end)?;
    let typed = &text_before[context.last()?.end..];
    Some((context_key(text_before, context), typed))
}

/// Returns the text of `counts` that the writer wrote at least [`MIN_PHRASE_COUNT`] times and in
/// at least [`MIN_PHRASE_PERCENT`] of the times, if any.
fn repeated_follower(counts: &[(String, u32)]) -> Option<&str> {
    let total = counts.iter().map(|(_, count)| count).sum::<u32>();
    let (follower, count) = counts.iter().max_by_key(|(_, count)| count)?;
    let is_repeated = *count >= MIN_PHRASE_COUNT && count * 100 >= total * MIN_PHRASE_PERCENT;
    is_repeated.then_some(follower.as_str())
}

/// Returns the words of `counts` after `typed` with their share of `strength`, most common first.
/// A word needs a space before it and no line break.
fn preferred_words(counts: &[(String, u32)], typed: &str, strength: f32) -> Vec<(String, f32)> {
    let total = small_count(counts.iter().map(|(_, count)| count).sum::<u32>());
    let mut words = counts
        .iter()
        .filter(|(follower, _)| follower.starts_with(typed))
        .filter_map(|(follower, count)| {
            let word = follower.trim_start_matches(|character| !is_phrase_character(character));
            let signs = &follower[..follower.len() - word.len()];
            let is_after_space = signs.ends_with(char::is_whitespace) && !signs.contains(['\r', '\n']);
            is_after_space.then_some((word, *count))
        })
        .collect::<Vec<_>>();
    words.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(right.0)));
    words
        .into_iter()
        .take(MAX_NEXT_WORDS)
        .map(|(word, count)| (word.to_owned(), strength * small_count(count) / total))
        .collect()
}

/// Returns the phrases of `entries` for `choice`, which have none when the choice is off.
pub(super) fn phrases(entries: &[&HistoryEntry], choice: WordChoice) -> Phrases {
    let Some(strength) = strength_of(choice) else {
        return Phrases::default();
    };
    let runs = RUN_WORDS
        .iter()
        .map(|&context_words| RunFollowers {
            context_words,
            followers: repeated_runs(entries, context_words),
        })
        .collect();
    Phrases { strength, runs }
}

/// Returns, for each run of `context_words` words of `entries` in any case that the writer wrote
/// at least [`MIN_RUN_COUNT`] times, the texts that came after it up to the end of the next word,
/// with the signs before that word, and their counts.
fn repeated_runs(entries: &[&HistoryEntry], context_words: usize) -> Followers {
    let mut followers = HashMap::<String, Vec<(&str, u32)>>::new();
    for part in entries.iter().flat_map(|entry| entry.text.split(REDACTED)) {
        count_followers(part, context_words, &mut followers);
    }
    followers
        .into_iter()
        .filter(|(_, counts)| counts.iter().map(|(_, count)| count).sum::<u32>() >= MIN_RUN_COUNT)
        .map(|(context, counts)| {
            let owned = counts
                .into_iter()
                .map(|(follower, count)| (follower.to_owned(), count))
                .collect();
            (context, owned)
        })
        .collect()
}

/// Counts in `followers` the texts of `part` after each run of `context_words` words, up to the
/// end of the next word, with the signs before that word.
fn count_followers<'text>(
    part: &'text str,
    context_words: usize,
    followers: &mut HashMap<String, Vec<(&'text str, u32)>>,
) {
    let spans = runs_of(part, is_phrase_character);
    for window in spans.windows(context_words + 1) {
        let follower = &part[window[context_words - 1].end..window[context_words].end];
        let counts = followers
            .entry(context_key(part, &window[..context_words]))
            .or_default();
        match counts.iter_mut().find(|(known, _)| *known == follower) {
            Some((_, count)) => *count += 1,
            None => counts.push((follower, 1)),
        }
    }
}

/// Returns the words of `context`, spans of `text`, in lowercase with one space between them.
fn context_key(text: &str, context: &[Range<usize>]) -> String {
    let mut key = String::new();
    for span in context {
        if !key.is_empty() {
            key.push(' ');
        }
        key.extend(text[span.clone()].chars().flat_map(char::to_lowercase));
    }
    key
}

/// The characters of a word of a phrase: letters, digits and apostrophes.
fn is_phrase_character(character: char) -> bool {
    character.is_alphanumeric() || is_apostrophe(character)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{mail, refs, word};

    fn phrase_entries() -> [HistoryEntry; 3] {
        [
            mail("Please let me know if it works."),
            mail("Let me know if you can come."),
            mail("So let me KNOW if that helps."),
        ]
    }

    fn continuation_after(entries: &[HistoryEntry], text_before: &str) -> Option<String> {
        continuation_in(&[&phrases(&refs(entries), WordChoice::Medium)], text_before).map(str::to_owned)
    }

    #[test]
    fn phrase_continues_the_text_with_the_next_word_of_the_writer() {
        let cases = [
            ("Can you let me know ", Some("if")),
            ("Can you Let Me know", Some(" if")),
            ("Can you let me know i", Some("f")),
            ("Can you let me know if", None),
            ("Can you let us know ", None),
        ];

        let continuations = cases.map(|(text_before, _)| continuation_after(&phrase_entries(), text_before));

        assert_eq!(continuations, cases.map(|(_, next)| next.map(str::to_owned)));
    }

    #[test]
    fn phrase_needs_three_times_and_a_share_of_sixty_percent() {
        let cases: [(&[&str], Option<&str>); 4] = [
            (&["if", "if"], None),
            (&["if", "if", "if"], Some("if")),
            (&["if", "if", "if", "when", "when"], Some("if")),
            (&["if", "if", "if", "when", "when", "when"], None),
        ];

        let continuations = cases.map(|(next_words, _)| {
            let entries = next_words
                .iter()
                .map(|word| mail(&format!("let me know {word} it works")))
                .collect::<Vec<_>>();
            continuation_after(&entries, "Can you let me know ")
        });

        assert_eq!(continuations, cases.map(|(_, next)| next.map(str::to_owned)));
    }

    #[test]
    fn phrase_keeps_the_signs_and_the_spelling_of_the_writer() {
        let entries = vec![mail("Thanks a lot.\nBest regards,\nJordan"); 3];

        let continuation = continuation_after(&entries, "Thanks a lot.\nBest regards,\nJ");

        assert_eq!(continuation.as_deref(), Some("ordan"));
    }

    #[test]
    fn phrase_ends_at_the_end_of_the_line() {
        let entries = vec![mail("Thanks, kind regards,\nMateo"); 3];

        let continuation = continuation_after(&entries, "Thanks, kind regards");

        assert_eq!(continuation, None);
    }

    #[test]
    fn redacted_secret_is_no_phrase() {
        let entries = vec![mail("the code is [redacted] today"); 3];

        let continuation = continuation_after(&entries, "So the code is ");

        assert_eq!(continuation, None);
    }

    #[test]
    fn word_choice_off_gives_no_phrases() {
        let continuation = continuation_in(
            &[&phrases(&refs(&phrase_entries()), WordChoice::Off)],
            "Can you let me know ",
        )
        .map(str::to_owned);

        assert_eq!(continuation, None);
    }

    #[test]
    fn debug_of_the_phrases_hides_the_text() {
        let entries = [mail("let me know if"), mail("let me know if")];

        let debug = format!("{:?}", phrases(&refs(&entries), WordChoice::Medium));

        assert_eq!(debug, "Phrases { runs: 3, .. }");
    }

    fn next_word_entries() -> Vec<HistoryEntry> {
        ["if it works", "if you can", "when you can", "if that helps"]
            .map(|rest| mail(&format!("Please let me know {rest}.")))
            .to_vec()
    }

    fn next_words_in(entries: &[HistoryEntry], choice: WordChoice, text_before: &str) -> Vec<(String, f32)> {
        next_word_bias_in(&[&phrases(&refs(entries), choice)], text_before).0
    }

    #[test]
    fn next_word_gets_the_share_of_the_strength_that_it_has_after_the_last_three_words() {
        let next = next_words_in(&next_word_entries(), WordChoice::Medium, "Can you let me know ");

        assert_eq!(next, [word("if", 0.75), word("when", 0.25)]);
    }

    #[test]
    fn next_words_inside_a_word_start_with_the_typed_letters() {
        let next = next_words_in(&next_word_entries(), WordChoice::Medium, "Can you let me know w");

        assert_eq!(next, [word("when", 0.25)]);
    }

    #[test]
    fn next_words_come_after_the_last_two_words_when_the_last_three_have_none() {
        let next = next_words_in(&next_word_entries(), WordChoice::Medium, "Then tell me know ");

        assert_eq!(next, [word("if", 0.75), word("when", 0.25)]);
    }

    #[test]
    fn strength_of_the_choice_scales_the_next_words() {
        let choices = [WordChoice::Off, WordChoice::Light, WordChoice::Strong];

        let next = choices.map(|choice| next_words_in(&next_word_entries(), choice, "let me know "));

        assert_eq!(
            next,
            [
                vec![],
                vec![word("if", 0.375), word("when", 0.125)],
                vec![word("if", 1.5), word("when", 0.5)]
            ]
        );
    }

    #[test]
    fn run_of_words_that_comes_one_time_gives_no_next_words() {
        let entries = [mail("We meet at noon today.")];

        let next = next_words_in(&entries, WordChoice::Medium, "So we meet at ");

        assert_eq!(next, []);
    }

    #[test]
    fn next_word_after_a_line_break_or_without_a_space_gets_no_preference() {
        let entries = ["Thanks a lot.\nJordan", "Thanks a lot, Jordan", "send the e-mail"]
            .repeat(2)
            .into_iter()
            .map(mail)
            .collect::<Vec<_>>();
        let cases = [
            ("Thanks a lot, ", vec![word("Jordan", 0.5)]),
            ("Thanks a lot.\n", vec![]),
            ("Please send the e-", vec![]),
        ];

        let next = cases
            .clone()
            .map(|(text_before, _)| next_words_in(&entries, WordChoice::Medium, text_before));

        assert_eq!(next, cases.map(|(_, words)| words));
    }

    #[test]
    fn next_words_are_at_most_the_eight_most_common() {
        let entries =
            ["a", "b", "c", "d", "e", "f", "g", "h", "i", "a"].map(|next| mail(&format!("we go to {next}x")));

        let next = next_words_in(&entries, WordChoice::Medium, "So we go to ");

        assert_eq!(next.first(), Some(&word("ax", 0.2)));
        assert_eq!(next.len(), 8);
    }
}
