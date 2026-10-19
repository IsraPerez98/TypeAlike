//! Scores the suggestions against the text that the writer typed next.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// The probabilities of the first word that the report tries as a minimum.
const GATE_PROBABILITIES: [f64; 6] = [0.05, 0.1, 0.2, 0.3, 0.4, 0.5];
/// The probabilities of the suggestion up to the end of a word that the report tries as a cut.
const CUT_PROBABILITIES: [f64; 7] = [0.3, 0.2, 0.1, 0.05, 0.02, 0.01, 0.005];

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

    /// Whether a case of this kind matches when the suggestion starts with the text that the
    /// writer typed next.
    fn has_next_word(self) -> bool {
        matches!(self, Self::NextWord | Self::PartialWord)
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

/// Returns the words of `text`, as the model runner counts them: a word starts at each whitespace
/// character after a character that is not whitespace.
pub fn words(text: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut start = 0;
    for ((offset, before), character) in text.char_indices().zip(text.chars().skip(1)) {
        if !before.is_whitespace() && character.is_whitespace() {
            let end = offset + before.len_utf8();
            words.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        words.push(&text[start..]);
    }
    words
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
    /// The natural log of the probability that the model gave to the suggestion up to the end of
    /// each shown word, as the model runner gave it. It is empty when no completion of the model
    /// shows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cumulative_log_probabilities: Vec<f64>,
    /// The shown words against the words of the text that the writer typed next.
    #[serde(default)]
    pub words: WordCounts,
}

/// The words of a suggestion against the words of the text that the writer typed next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WordCounts {
    /// The words before the first word that differs from the word of the text at its place.
    pub right: usize,
    /// The words from the first word that differs on. A word after the end of the text counts only
    /// after a wrong word.
    pub wrong: usize,
}

impl WordCounts {
    /// Compares the words of `suggestion` with the words of `next`. Signs after a word do not
    /// count.
    pub fn of(suggestion: &str, next: &str) -> Self {
        let shown = words(suggestion);
        let typed = words(next);
        let right = shown
            .iter()
            .zip(&typed)
            .take_while(|(shown, typed)| without_trailing_signs(shown) == without_trailing_signs(typed))
            .count();
        let has_wrong_word = right < shown.len() && right < typed.len();
        Self {
            right,
            wrong: if has_wrong_word { shown.len() - right } else { 0 },
        }
    }

    /// Returns the counts when only the first `shown` words of the suggestion show.
    fn of_first(self, shown: usize) -> Self {
        Self {
            right: self.right.min(shown),
            wrong: if self.wrong > 0 {
                shown.saturating_sub(self.right)
            } else {
                0
            },
        }
    }
}

/// Returns the sum of the first `log_probabilities` up to each of the first `shown` words.
pub fn cumulative_log_probabilities(log_probabilities: &[f64], shown: usize) -> Vec<f64> {
    log_probabilities
        .iter()
        .take(shown)
        .scan(0.0, |total, log_probability| {
            *total += log_probability;
            Some(*total)
        })
        .collect()
}

/// What a minimum for the probability of the first word does: a suggestion whose first word is
/// less likely does not show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GatePoint {
    /// The natural log of the minimum.
    pub log_probability: f64,
    /// The suggestions that still show.
    pub kept: usize,
    /// The suggestions that still show and match.
    pub kept_matches: usize,
    /// The suggestions of the model.
    pub suggestions: usize,
    /// The suggestions of the model that match.
    pub matches: usize,
}

impl GatePoint {
    /// Returns what the minimum `log_probability` does to the suggestions of the model in
    /// `outcomes`.
    pub fn of(outcomes: &[Outcome], log_probability: f64) -> Self {
        let mut point = Self {
            log_probability,
            kept: 0,
            kept_matches: 0,
            suggestions: 0,
            matches: 0,
        };
        for outcome in outcomes {
            let Some(&first_word) = outcome.cumulative_log_probabilities.first() else {
                continue;
            };
            let is_kept = first_word >= log_probability;
            point.suggestions += 1;
            point.matches += usize::from(outcome.is_match);
            point.kept += usize::from(is_kept);
            point.kept_matches += usize::from(is_kept && outcome.is_match);
        }
        point
    }
}

/// What a cut of the tail does: a word after the first word does not show when the suggestion up
/// to its end is less likely than the cut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutPoint {
    /// The natural log of the cut.
    pub log_probability: f64,
    /// The words that still show.
    pub shown: usize,
    /// The words that still show before the first wrong word.
    pub right: usize,
    /// The wrong words that still show.
    pub wrong: usize,
}

impl CutPoint {
    /// Returns what the cut `log_probability` does to the suggestions of the model in `outcomes`.
    pub fn of(outcomes: &[Outcome], log_probability: f64) -> Self {
        let mut point = Self {
            log_probability,
            shown: 0,
            right: 0,
            wrong: 0,
        };
        for outcome in outcomes {
            let cumulative = &outcome.cumulative_log_probabilities;
            if cumulative.is_empty() {
                continue;
            }
            let shown = cumulative
                .iter()
                .take_while(|&&cumulative| cumulative >= log_probability)
                .count()
                .max(1);
            let counts = outcome.words.of_first(shown);
            point.shown += shown;
            point.right += counts.right;
            point.wrong += counts.wrong;
        }
        point
    }
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
    /// The shown words of the cases with a next word.
    pub words: WordCounts,
    /// The mean log probability of the first shown word, of the cases with a next word that show
    /// a completion of the model.
    pub mean_first_word_log_probability: Option<f64>,
    /// What each minimum for the first word does to the cases with a next word.
    pub gate: Vec<GatePoint>,
    /// What no cut, then each cut of the tail does to the cases with a next word.
    pub cut: Vec<CutPoint>,
}

impl Summary {
    /// Returns the scores of `outcomes`.
    pub fn of(outcomes: &[Outcome]) -> Self {
        let with_next_word = outcomes
            .iter()
            .filter(|outcome| outcome.kind.has_next_word())
            .cloned()
            .collect::<Vec<_>>();
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
            words: with_next_word
                .iter()
                .fold(WordCounts::default(), |total, outcome| WordCounts {
                    right: total.right + outcome.words.right,
                    wrong: total.wrong + outcome.words.wrong,
                }),
            mean_first_word_log_probability: mean(
                &with_next_word
                    .iter()
                    .filter_map(|outcome| outcome.cumulative_log_probabilities.first().copied())
                    .collect::<Vec<_>>(),
            ),
            gate: GATE_PROBABILITIES
                .map(|probability| GatePoint::of(&with_next_word, probability.ln()))
                .to_vec(),
            cut: [0.0]
                .into_iter()
                .chain(CUT_PROBABILITIES)
                .map(|probability| CutPoint::of(&with_next_word, probability.ln()))
                .collect(),
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
            cumulative_log_probabilities: Vec::new(),
            words: WordCounts::default(),
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
    fn word_starts_at_whitespace_after_a_word() {
        let cases: [(&str, &[&str]); 4] = [
            ("very soon, thanks", &["very", " soon,", " thanks"]),
            (" meeting at", &[" meeting", " at"]),
            ("Hi  there", &["Hi", "  there"]),
            ("", &[]),
        ];

        let split = cases.map(|(text, _)| words(text));

        assert_eq!(split, cases.map(|(_, words)| words.to_vec()));
    }

    #[test]
    fn words_count_as_right_up_to_the_first_wrong_word_and_wrong_from_it() {
        let cases = [
            ("very soon, thanks", "very soon. Thanks a lot", 2, 1),
            (" the meeting today", " the report today", 1, 2),
            ("ply and the files", "ply and", 2, 0),
            ("plied", "ply and", 0, 1),
        ];

        let counts = cases.map(|(suggestion, next, _, _)| WordCounts::of(suggestion, next));

        assert_eq!(
            counts,
            cases.map(|(_, _, right, wrong)| WordCounts { right, wrong })
        );
    }

    #[test]
    fn cumulative_log_probability_follows_each_shown_word() {
        let cumulative = cumulative_log_probabilities(&[-1.0, -0.5, -2.0], 2);

        assert_eq!(cumulative, [-1.0, -1.5]);
    }

    fn with_words(id: &str, is_match: bool, cumulative: &[f64], right: usize, wrong: usize) -> Outcome {
        Outcome {
            cumulative_log_probabilities: cumulative.to_vec(),
            words: WordCounts { right, wrong },
            ..outcome(id, CaseKind::NextWord, is_match, 10)
        }
    }

    #[test]
    fn gate_keeps_the_suggestions_whose_first_word_reaches_the_threshold() {
        let outcomes = [
            with_words("a", true, &[-0.1], 1, 0),
            with_words("b", false, &[-3.0], 0, 1),
            with_words("c", true, &[-2.5], 1, 0),
            outcome("d", CaseKind::NextWord, false, 10),
        ];

        let point = GatePoint::of(&outcomes, -2.0);

        assert_eq!(
            point,
            GatePoint {
                log_probability: -2.0,
                kept: 1,
                kept_matches: 1,
                suggestions: 3,
                matches: 2
            }
        );
    }

    #[test]
    fn cut_keeps_the_first_word_and_the_words_above_it() {
        let outcomes = [
            with_words("a", true, &[-0.5, -1.0, -4.0], 2, 1),
            with_words("b", false, &[-5.0, -6.0], 0, 2),
            with_words("c", true, &[-0.1, -0.2, -3.0], 3, 0),
        ];

        let point = CutPoint::of(&outcomes, -2.0);

        assert_eq!(
            point,
            CutPoint {
                log_probability: -2.0,
                shown: 5,
                right: 4,
                wrong: 1
            }
        );
    }

    #[test]
    fn summary_counts_the_words_and_the_first_word_of_the_cases_with_a_next_word() {
        let outcomes = [
            with_words("a", true, &[-1.0, -2.0], 2, 0),
            with_words("b", false, &[-3.0], 0, 1),
            Outcome {
                kind: CaseKind::Typo,
                ..with_words("c", false, &[-5.0], 0, 4)
            },
        ];

        let summary = Summary::of(&outcomes);

        assert_eq!(
            (summary.words, summary.mean_first_word_log_probability),
            (WordCounts { right: 2, wrong: 1 }, Some(-2.0))
        );
    }

    #[test]
    fn comparison_skips_cases_that_the_baseline_does_not_have() {
        let outcomes = [outcome("new", CaseKind::NextWord, true, 10)];

        let comparison = Comparison::of(&[], &outcomes);

        assert_eq!(comparison, Comparison::default());
    }
}
