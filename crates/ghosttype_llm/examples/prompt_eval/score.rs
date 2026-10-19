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
    /// Inside a word with a typo: the suggestion must not finish it.
    Typo,
    /// After an Enter that did not start a new line in the field: no suggestion must show.
    NoNewLine,
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

/// Returns whether a case of `kind` with the ghost text `suggestion` matches. A typo case matches
/// when no suggestion finishes the word with the typo, and a case without a new line when no
/// suggestion shows. Another case matches when `accepted`, the word that the first Tab types, is
/// the first word of `next`.
pub fn is_match(kind: CaseKind, suggestion: Option<&str>, accepted: &str, next: &str) -> bool {
    match kind {
        CaseKind::Typo => !suggestion.is_some_and(|text| text.starts_with(char::is_alphanumeric)),
        CaseKind::NoNewLine => suggestion.is_none(),
        CaseKind::NextWord | CaseKind::PartialWord => is_next_word(accepted, next),
    }
}

/// Returns whether `accepted` is the first word of `next`. Signs after the word, such as a period
/// or a comma, do not count.
fn is_next_word(accepted: &str, next: &str) -> bool {
    let word = without_trailing_signs(accepted);
    !word.trim().is_empty() && word == next_word(next)
}

/// Returns the first word of `next`, with the whitespace before it and without signs after it.
pub fn next_word(next: &str) -> &str {
    without_trailing_signs(first_word(next))
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    /// The id of the case.
    pub id: String,
    /// Where the caret is.
    pub kind: CaseKind,
    /// The ghost text, or `None` when none shows.
    pub suggestion: Option<String>,
    /// Whether the case matches, as [`is_match`] tells.
    pub is_match: bool,
    /// The milliseconds from the key to the ghost text.
    pub millis: u64,
    /// The milliseconds from the key to the ghost text when the app prepared the prompt before the
    /// key, or `None` in results without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warm_millis: Option<u64>,
    /// The tokens of the prompt, or `None` when the case has no prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u64>,
    /// The natural log of the probability of the next word after the prompt, or `None` when the
    /// case has no prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_probability: Option<f64>,
}

/// How many cases matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Score {
    /// The cases that match, as [`is_match`] tells.
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
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    /// The cases with the caret between two words.
    pub next_word: Score,
    /// The cases with the caret inside a word.
    pub partial_word: Score,
    /// The cases with the caret inside a word with a typo.
    pub typo: Score,
    /// The cases with an Enter that did not start a new line.
    pub no_new_line: Score,
    /// The cases that show no ghost text.
    pub without_suggestion: usize,
    /// The median milliseconds from the key to the ghost text.
    pub median_millis: u64,
    /// The median milliseconds from the key to the ghost text after the app prepared the prompt,
    /// of the cases that have one.
    pub median_warm_millis: Option<u64>,
    /// The median tokens of a prompt, of the cases that have a prompt.
    pub median_prompt_tokens: Option<u64>,
    /// The milliseconds from the key to the ghost text for each token of the prompt, of all the
    /// cases that have a prompt together.
    pub millis_per_token: Option<f64>,
    /// The mean log probability of the next word, of the cases that have one.
    pub mean_log_probability: Option<f64>,
}

impl Summary {
    /// Returns the scores of `outcomes`.
    pub fn of(outcomes: &[Outcome]) -> Self {
        Self {
            next_word: Score::of(outcomes, CaseKind::NextWord),
            partial_word: Score::of(outcomes, CaseKind::PartialWord),
            typo: Score::of(outcomes, CaseKind::Typo),
            no_new_line: Score::of(outcomes, CaseKind::NoNewLine),
            without_suggestion: outcomes
                .iter()
                .filter(|outcome| outcome.suggestion.is_none())
                .count(),
            median_millis: median(outcomes.iter().map(|outcome| outcome.millis).collect())
                .unwrap_or_default(),
            median_warm_millis: median(
                outcomes
                    .iter()
                    .filter_map(|outcome| outcome.warm_millis)
                    .collect(),
            ),
            median_prompt_tokens: median(
                outcomes
                    .iter()
                    .filter_map(|outcome| outcome.prompt_tokens)
                    .collect(),
            ),
            millis_per_token: millis_per_token(outcomes),
            mean_log_probability: mean(
                &outcomes
                    .iter()
                    .filter_map(|outcome| outcome.log_probability)
                    .collect::<Vec<_>>(),
            ),
        }
    }
}

/// The cases that changed against an earlier run of the same cases.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Comparison {
    /// The ids of the cases that match now and did not match before.
    pub helped: Vec<String>,
    /// The ids of the cases that matched before and do not match now.
    pub harmed: Vec<String>,
    /// The cases that match now as before, or miss now as before.
    pub unchanged: usize,
    /// The change of the log probability of the next word, of the cases that have one in both
    /// runs.
    pub log_probability: Option<LogProbabilityChange>,
}

impl Comparison {
    /// Compares `outcomes` with `baseline`. A case that only one of them has does not count.
    pub fn of(baseline: &[Outcome], outcomes: &[Outcome]) -> Self {
        let before_by_id = baseline
            .iter()
            .map(|outcome| (outcome.id.as_str(), outcome))
            .collect::<HashMap<_, _>>();
        let mut comparison = Self::default();
        let mut changes = Vec::new();
        for outcome in outcomes {
            let Some(before) = before_by_id.get(outcome.id.as_str()) else {
                continue;
            };
            match (before.is_match, outcome.is_match) {
                (false, true) => comparison.helped.push(outcome.id.clone()),
                (true, false) => comparison.harmed.push(outcome.id.clone()),
                _ => comparison.unchanged += 1,
            }
            if let (Some(before), Some(now)) = (before.log_probability, outcome.log_probability) {
                changes.push(now - before);
            }
        }
        comparison.log_probability = LogProbabilityChange::of(&changes);
        comparison
    }
}

/// How the log probability of the next word changed against an earlier run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogProbabilityChange {
    /// The mean change. A positive change makes the next word more likely.
    pub mean: f64,
    /// The cases that make the next word more likely now.
    pub higher: usize,
    /// The cases that make the next word less likely now.
    pub lower: usize,
}

impl LogProbabilityChange {
    fn of(changes: &[f64]) -> Option<Self> {
        Some(Self {
            mean: mean(changes)?,
            higher: changes.iter().filter(|&&change| change > 0.0).count(),
            lower: changes.iter().filter(|&&change| change < 0.0).count(),
        })
    }
}

fn mean(values: &[f64]) -> Option<f64> {
    let count = u32::try_from(values.len()).ok().filter(|&count| count > 0)?;
    Some(values.iter().sum::<f64>() / f64::from(count))
}

fn median(mut values: Vec<u64>) -> Option<u64> {
    values.sort_unstable();
    let middle = values.len() / 2;
    match values.len() {
        0 => None,
        count if count % 2 == 1 => Some(values[middle]),
        _ => Some(values[middle - 1].midpoint(values[middle])),
    }
}

/// Returns the milliseconds of the `outcomes` that have a prompt for each token of their prompts.
fn millis_per_token(outcomes: &[Outcome]) -> Option<f64> {
    let (millis, tokens) = outcomes
        .iter()
        .filter_map(|outcome| Some((outcome.millis, outcome.prompt_tokens?)))
        .fold((0, 0), |(millis, tokens), (case_millis, case_tokens)| {
            (millis + case_millis, tokens + case_tokens)
        });
    let tokens = u32::try_from(tokens).ok().filter(|&tokens| tokens > 0)?;
    Some(f64::from(u32::try_from(millis).ok()?) / f64::from(tokens))
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
            warm_millis: None,
            prompt_tokens: None,
            log_probability: None,
        }
    }

    fn timed(id: &str, millis: u64, warm_millis: u64, prompt_tokens: u64) -> Outcome {
        Outcome {
            warm_millis: Some(warm_millis),
            prompt_tokens: Some(prompt_tokens),
            ..outcome(id, CaseKind::NextWord, true, millis)
        }
    }

    fn scored(id: &str, is_match: bool, log_probability: f64) -> Outcome {
        Outcome {
            log_probability: Some(log_probability),
            ..outcome(id, CaseKind::NextWord, is_match, 10)
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
    fn next_word_keeps_the_space_before_it_and_drops_the_signs_after_it() {
        let cases = [
            (" meeting at noon", " meeting"),
            ("ción que", "ción"),
            ("tomorrow.", "tomorrow"),
            ("? Let", "?"),
        ];

        let words = cases.map(|(next, _)| next_word(next));

        assert_eq!(words, cases.map(|(_, word)| word));
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
    fn typo_case_matches_without_a_suggestion_that_finishes_the_word() {
        let cases = [
            (Some("cia antes del miércoles"), false),
            (Some(" antes"), true),
            (Some("?"), true),
            (None, true),
        ];

        let matches = cases.map(|(suggestion, _)| is_match(CaseKind::Typo, suggestion, "cia", "cia antes"));

        assert_eq!(matches, cases.map(|(_, is_right)| is_right));
    }

    #[test]
    fn case_without_a_new_line_matches_without_a_suggestion() {
        let suggestions = [None, Some(" Sara")];

        let matches = suggestions.map(|suggestion| is_match(CaseKind::NoNewLine, suggestion, "", ""));

        assert_eq!(matches, [true, false]);
    }

    #[test]
    fn other_cases_match_when_the_accepted_word_is_the_next_word() {
        let kinds = [CaseKind::NextWord, CaseKind::PartialWord];

        let matches = kinds.map(|kind| is_match(kind, Some(" meeting at"), " meeting", " meeting at noon"));

        assert_eq!(matches, [true, true]);
    }

    #[test]
    fn summary_scores_the_typo_cases_apart() {
        let outcomes = [
            outcome("a", CaseKind::Typo, true, 10),
            outcome("b", CaseKind::Typo, false, 10),
            outcome("c", CaseKind::NextWord, true, 10),
        ];

        let summary = Summary::of(&outcomes);

        assert_eq!(
            (summary.next_word, summary.typo),
            (Score { matches: 1, cases: 1 }, Score { matches: 1, cases: 2 })
        );
    }

    #[test]
    fn summary_scores_the_cases_without_a_new_line_apart() {
        let outcomes = [
            outcome("a", CaseKind::NoNewLine, true, 10),
            outcome("b", CaseKind::NoNewLine, false, 10),
            outcome("c", CaseKind::NextWord, true, 10),
        ];

        let summary = Summary::of(&outcomes);

        assert_eq!(
            (summary.next_word, summary.no_new_line),
            (Score { matches: 1, cases: 1 }, Score { matches: 1, cases: 2 })
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
    fn summary_warm_time_is_the_middle_warm_time_of_the_cases_that_have_one() {
        let outcomes = [
            timed("a", 900, 30, 400),
            timed("b", 900, 50, 400),
            timed("c", 900, 40, 400),
            without_suggestion("d", 900),
        ];

        let summary = Summary::of(&outcomes);

        assert_eq!(summary.median_warm_millis, Some(40));
    }

    #[test]
    fn summary_gives_the_middle_prompt_tokens_and_the_time_of_each_token() {
        let outcomes = [
            timed("a", 300, 30, 100),
            timed("b", 900, 30, 500),
            timed("c", 600, 30, 400),
            without_suggestion("d", 5),
        ];

        let summary = Summary::of(&outcomes);

        assert_eq!(
            (summary.median_prompt_tokens, summary.millis_per_token),
            (Some(400), Some(1.8))
        );
    }

    #[test]
    fn summary_of_cases_without_timing_details_has_none() {
        let summary = Summary::of(&[without_suggestion("a", 10)]);

        assert_eq!(
            (
                summary.median_warm_millis,
                summary.median_prompt_tokens,
                summary.millis_per_token
            ),
            (None, None, None)
        );
    }

    #[test]
    fn summary_log_probability_is_the_mean_of_the_cases_that_have_one() {
        let outcomes = [
            scored("a", true, -1.0),
            scored("b", false, -3.0),
            without_suggestion("c", 10),
        ];

        let summary = Summary::of(&outcomes);

        assert_eq!(summary.mean_log_probability, Some(-2.0));
    }

    #[test]
    fn summary_of_cases_without_log_probability_has_none() {
        let summary = Summary::of(&[without_suggestion("a", 10)]);

        assert_eq!(summary.mean_log_probability, None);
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
                log_probability: None,
            }
        );
    }

    #[test]
    fn comparison_gives_the_mean_change_of_the_log_probability() {
        let baseline = [
            scored("up", true, -2.0),
            scored("down", true, -1.0),
            scored("same", true, -1.0),
        ];
        let outcomes = [
            scored("up", true, -1.0),
            scored("down", true, -3.0),
            scored("same", true, -1.0),
        ];

        let comparison = Comparison::of(&baseline, &outcomes);

        assert_eq!(
            comparison.log_probability,
            Some(LogProbabilityChange {
                mean: -1.0 / 3.0,
                higher: 1,
                lower: 1,
            })
        );
    }

    #[test]
    fn log_probability_reads_back_from_json_as_written() {
        let outcome = scored("a", true, -1.142_062_436_409_791_3);

        let read_back = serde_json::from_str::<Outcome>(&serde_json::to_string(&outcome).unwrap()).unwrap();

        assert_eq!(read_back, outcome);
    }

    #[test]
    fn comparison_skips_cases_that_the_baseline_does_not_have() {
        let outcomes = [outcome("new", CaseKind::NextWord, true, 10)];

        let comparison = Comparison::of(&[], &outcomes);

        assert_eq!(comparison, Comparison::default());
    }
}
