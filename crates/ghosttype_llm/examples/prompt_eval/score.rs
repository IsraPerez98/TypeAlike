//! Scores the suggestions against the text that the writer typed next.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Where the caret is when the suggestion shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseKind {
    /// Between two words: the suggestion must start the next word.
    NextWord,
    /// Inside a word: the suggestion must finish it.
    PartialWord,
}

impl CaseKind {
    /// Returns the kind of a case where the writer typed `next` after `before`.
    pub fn of(before: &str, next: &str) -> Self {
        let is_inside_word =
            before.ends_with(char::is_alphanumeric) && next.starts_with(char::is_alphanumeric);
        if is_inside_word {
            Self::PartialWord
        } else {
            Self::NextWord
        }
    }
}

/// Returns whether `accepted`, the word that the first Tab types, is the first word of `next`.
/// Signs after the word, such as a period or a comma, do not count.
pub fn is_next_word(accepted: &str, next: &str) -> bool {
    let word = without_trailing_signs(accepted);
    !word.trim().is_empty() && word == without_trailing_signs(first_word(next))
}

/// Returns the first word of `text`, with the whitespace before it.
fn first_word(text: &str) -> &str {
    let rest = text
        .trim_start()
        .trim_start_matches(|character: char| !character.is_whitespace());
    &text[..text.len() - rest.len()]
}

/// Returns `word` up to its last letter or digit, or all of `word` when it has none.
fn without_trailing_signs(word: &str) -> &str {
    word.char_indices()
        .rfind(|(_, character)| character.is_alphanumeric())
        .map_or(word, |(start, character)| &word[..start + character.len_utf8()])
}

/// What one case gave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    /// The id of the case.
    pub id: String,
    /// Where the caret is.
    pub kind: CaseKind,
    /// The ghost text, or `None` when none shows.
    pub suggestion: Option<String>,
    /// Whether the word that the first Tab types is the next word that the writer typed.
    pub is_match: bool,
    /// The milliseconds from the key to the ghost text.
    pub millis: u64,
}

/// How many cases matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Score {
    /// The cases where the word that the first Tab types is the next word that the writer typed.
    pub matches: usize,
    /// All the cases.
    pub cases: usize,
}

impl Score {
    fn of(outcomes: &[Outcome], kind: CaseKind) -> Self {
        let of_kind = outcomes.iter().filter(|outcome| outcome.kind == kind);
        Self {
            matches: of_kind.clone().filter(|outcome| outcome.is_match).count(),
            cases: of_kind.count(),
        }
    }
}

impl fmt::Display for Score {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.matches, self.cases)?;
        let Some(tenths_of_percent) = self.matches.saturating_mul(1000).checked_div(self.cases) else {
            return Ok(());
        };
        write!(
            formatter,
            " ({}.{}%)",
            tenths_of_percent / 10,
            tenths_of_percent % 10
        )
    }
}

/// The scores of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// The cases with the caret between two words.
    pub next_word: Score,
    /// The cases with the caret inside a word.
    pub partial_word: Score,
    /// The cases that show no ghost text.
    pub without_suggestion: usize,
    /// The median milliseconds from the key to the ghost text.
    pub median_millis: u64,
}

impl Summary {
    /// Returns the scores of `outcomes`.
    pub fn of(outcomes: &[Outcome]) -> Self {
        Self {
            next_word: Score::of(outcomes, CaseKind::NextWord),
            partial_word: Score::of(outcomes, CaseKind::PartialWord),
            without_suggestion: outcomes
                .iter()
                .filter(|outcome| outcome.suggestion.is_none())
                .count(),
            median_millis: median(outcomes.iter().map(|outcome| outcome.millis).collect()),
        }
    }
}

/// The cases that changed against an earlier run of the same cases.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Comparison {
    /// The ids of the cases that match now and did not match before.
    pub helped: Vec<String>,
    /// The ids of the cases that matched before and do not match now.
    pub harmed: Vec<String>,
    /// The cases that match now as before, or miss now as before.
    pub unchanged: usize,
}

impl Comparison {
    /// Compares `outcomes` with `baseline`. A case that only one of them has does not count.
    pub fn of(baseline: &[Outcome], outcomes: &[Outcome]) -> Self {
        let matched_before = baseline
            .iter()
            .map(|outcome| (outcome.id.as_str(), outcome.is_match))
            .collect::<HashMap<_, _>>();
        let mut comparison = Self::default();
        for outcome in outcomes {
            match (matched_before.get(outcome.id.as_str()), outcome.is_match) {
                (None, _) => {}
                (Some(false), true) => comparison.helped.push(outcome.id.clone()),
                (Some(true), false) => comparison.harmed.push(outcome.id.clone()),
                (Some(_), _) => comparison.unchanged += 1,
            }
        }
        comparison
    }
}

fn median(mut values: Vec<u64>) -> u64 {
    values.sort_unstable();
    let middle = values.len() / 2;
    match values.len() {
        0 => 0,
        count if count % 2 == 1 => values[middle],
        _ => values[middle - 1].midpoint(values[middle]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(id: &str, kind: CaseKind, is_match: bool, millis: u64) -> Outcome {
        Outcome {
            id: id.to_owned(),
            kind,
            suggestion: Some("text".to_owned()),
            is_match,
            millis,
        }
    }

    fn without_suggestion(id: &str, millis: u64) -> Outcome {
        Outcome {
            suggestion: None,
            ..outcome(id, CaseKind::NextWord, false, millis)
        }
    }

    #[test]
    fn caret_after_letters_and_before_letters_is_inside_a_word() {
        let cases = [
            ("the quick re", "ply and"),
            ("la cotiza", "ción que"),
            ("in 20", "26 we"),
        ];

        let kinds = cases.map(|(before, next)| CaseKind::of(before, next));

        assert_eq!(kinds, cases.map(|_| CaseKind::PartialWord));
    }

    #[test]
    fn caret_next_to_a_space_or_a_sign_is_between_words() {
        let cases = [
            ("the quick ", "reply and"),
            ("Thanks for the", " meeting"),
            ("How are you", "? Let me"),
            ("Hi Ana,", " thanks"),
        ];

        let kinds = cases.map(|(before, next)| CaseKind::of(before, next));

        assert_eq!(kinds, cases.map(|_| CaseKind::NextWord));
    }

    #[test]
    fn accepted_word_that_is_the_next_word_matches() {
        let cases = [
            ("ply", "ply and the files"),
            (" meeting", " meeting at noon"),
            ("?", "? Let me know"),
        ];

        let matches = cases.map(|(accepted, next)| is_next_word(accepted, next));

        assert_eq!(matches, cases.map(|_| true));
    }

    #[test]
    fn signs_after_the_word_do_not_count() {
        let cases = [
            ("rt.", "rt or some cookies"),
            ("tomorrow", "tomorrow."),
            ("do\".", "do\" y no me deja"),
        ];

        let matches = cases.map(|(accepted, next)| is_next_word(accepted, next));

        assert_eq!(matches, cases.map(|_| true));
    }

    #[test]
    fn accepted_word_that_differs_from_the_next_word_does_not_match() {
        let cases = [
            ("plied", "ply and the files"),
            ("meeting", " meeting at noon"),
            ("the", "then we"),
            ("1Z999AA1", "1Z999AA10123456784. It"),
            (".", "? Let me know"),
            ("", "the"),
            (" ", " the"),
        ];

        let matches = cases.map(|(accepted, next)| is_next_word(accepted, next));

        assert_eq!(matches, cases.map(|_| false));
    }

    #[test]
    fn summary_scores_each_kind_of_case() {
        let outcomes = [
            outcome("a", CaseKind::NextWord, true, 10),
            outcome("b", CaseKind::NextWord, false, 10),
            outcome("c", CaseKind::PartialWord, true, 10),
            without_suggestion("d", 10),
        ];

        let summary = Summary::of(&outcomes);

        assert_eq!(
            (
                summary.next_word,
                summary.partial_word,
                summary.without_suggestion
            ),
            (Score { matches: 1, cases: 3 }, Score { matches: 1, cases: 1 }, 1)
        );
    }

    #[test]
    fn summary_time_is_the_middle_time() {
        let outcomes = [
            outcome("a", CaseKind::NextWord, true, 300),
            outcome("b", CaseKind::NextWord, true, 100),
            outcome("c", CaseKind::NextWord, true, 200),
        ];

        let summary = Summary::of(&outcomes);

        assert_eq!(summary.median_millis, 200);
    }

    #[test]
    fn summary_time_of_an_even_count_is_between_the_two_middle_times() {
        let outcomes = [
            outcome("a", CaseKind::NextWord, true, 400),
            outcome("b", CaseKind::NextWord, true, 100),
            outcome("c", CaseKind::NextWord, true, 200),
            outcome("d", CaseKind::NextWord, true, 300),
        ];

        let summary = Summary::of(&outcomes);

        assert_eq!(summary.median_millis, 250);
    }

    #[test]
    fn score_shows_the_matches_and_the_share() {
        let score = Score { matches: 2, cases: 3 };

        assert_eq!(score.to_string(), "2/3 (66.6%)");
    }

    #[test]
    fn score_of_no_cases_shows_no_share() {
        assert_eq!(Score::default().to_string(), "0/0");
    }

    #[test]
    fn comparison_names_the_cases_that_changed() {
        let baseline = [
            outcome("helped", CaseKind::NextWord, false, 10),
            outcome("harmed", CaseKind::NextWord, true, 10),
            outcome("still-matches", CaseKind::NextWord, true, 10),
            outcome("still-misses", CaseKind::NextWord, false, 10),
        ];
        let outcomes = [
            outcome("helped", CaseKind::NextWord, true, 10),
            outcome("harmed", CaseKind::NextWord, false, 10),
            outcome("still-matches", CaseKind::NextWord, true, 10),
            outcome("still-misses", CaseKind::NextWord, false, 10),
        ];

        let comparison = Comparison::of(&baseline, &outcomes);

        assert_eq!(
            comparison,
            Comparison {
                helped: vec!["helped".to_owned()],
                harmed: vec!["harmed".to_owned()],
                unchanged: 2,
            }
        );
    }

    #[test]
    fn comparison_skips_cases_that_the_baseline_does_not_have() {
        let outcomes = [outcome("new", CaseKind::NextWord, true, 10)];

        let comparison = Comparison::of(&[], &outcomes);

        assert_eq!(comparison, Comparison::default());
    }
}
