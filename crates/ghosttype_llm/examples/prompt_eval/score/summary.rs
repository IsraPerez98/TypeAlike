//! The scores of one run, and what each minimum and each cut that the report tries does.

use std::fmt;

use super::{CaseKind, Outcome, WordCounts};

/// The probabilities of the first word that the report tries as a minimum.
const GATE_PROBABILITIES: [f64; 6] = [0.05, 0.1, 0.2, 0.3, 0.4, 0.5];
/// The probabilities of the suggestion up to the end of a word that the report tries as a cut.
const CUT_PROBABILITIES: [f64; 10] = [0.3, 0.2, 0.1, 0.05, 0.02, 0.01, 0.005, 0.002, 0.001, 0.0002];
/// The natural logs of the probabilities of the typed letters that the report tries as a typo cut.
const TYPO_LOG_PROBABILITIES: [f64; 9] = [-6.0, -8.0, -10.0, -11.0, -12.0, -14.0, -16.0, -18.0, -20.0];

/// How many fixes of a typo wrote the word that the writer meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fixes {
    /// The fixes that wrote the word that the writer meant.
    pub right: usize,
    /// The other fixes.
    pub wrong: usize,
}

impl Fixes {
    fn of<'outcome>(outcomes: impl Iterator<Item = &'outcome Outcome>) -> Self {
        outcomes
            .filter_map(|outcome| outcome.is_fix_right)
            .fold(Self::default(), |fixes, is_right| Self {
                right: fixes.right + usize::from(is_right),
                wrong: fixes.wrong + usize::from(!is_right),
            })
    }
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

/// What a cut for the typed letters of the last word does: no suggestion shows when the letters
/// are less likely.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypoPoint {
    /// The natural log of the cut.
    pub log_probability: f64,
    /// The typo cases with a probability of the typed letters.
    pub typos: usize,
    /// The typo cases that the cut catches.
    pub caught: usize,
    /// The cases with a next word and a probability of the typed letters.
    pub others: usize,
    /// The cases with a next word that the cut hides.
    pub hidden: usize,
    /// The cases with a next word that match and that the cut hides.
    pub hidden_matches: usize,
}

impl TypoPoint {
    /// Returns what the cut `log_probability` does to `outcomes`.
    pub fn of(outcomes: &[Outcome], log_probability: f64) -> Self {
        let mut point = Self {
            log_probability,
            typos: 0,
            caught: 0,
            others: 0,
            hidden: 0,
            hidden_matches: 0,
        };
        for outcome in outcomes {
            let Some(typed) = outcome.typed_log_probability else {
                continue;
            };
            let is_caught = typed < log_probability;
            if outcome.kind == CaseKind::Typo {
                point.typos += 1;
                point.caught += usize::from(is_caught);
            } else if outcome.kind.has_next_word() {
                point.others += 1;
                point.hidden += usize::from(is_caught);
                point.hidden_matches += usize::from(is_caught && outcome.is_match);
            }
        }
        point
    }
}

/// How many cases matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Score {
    /// The cases that match, as [`is_match`](super::is_match) tells.
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
    /// What each cut for the typed letters does.
    pub typo_cut: Vec<TypoPoint>,
    /// The fixes of the typo cases.
    pub typo_fixes: Fixes,
    /// The fixes of the other cases, which had no typo.
    pub other_fixes: Fixes,
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
            typo_cut: TYPO_LOG_PROBABILITIES
                .map(|log_probability| TypoPoint::of(outcomes, log_probability))
                .to_vec(),
            typo_fixes: Fixes::of(outcomes.iter().filter(|outcome| outcome.kind == CaseKind::Typo)),
            other_fixes: Fixes::of(outcomes.iter().filter(|outcome| outcome.kind != CaseKind::Typo)),
        }
    }
}

pub fn mean(values: &[f64]) -> Option<f64> {
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
    use crate::score::test_outcomes::{outcome, scored};

    fn timed(id: &str, millis: u64, warm_millis: u64, prompt_tokens: u64) -> Outcome {
        Outcome {
            warm_millis: Some(warm_millis),
            prompt_tokens: Some(prompt_tokens),
            ..outcome(id, CaseKind::NextWord, true, millis)
        }
    }

    fn without_suggestion(id: &str, millis: u64) -> Outcome {
        Outcome {
            suggestion: None,
            ..outcome(id, CaseKind::NextWord, false, millis)
        }
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

    fn fixed(id: &str, kind: CaseKind, is_fix_right: bool) -> Outcome {
        Outcome {
            is_fix_right: Some(is_fix_right),
            ..outcome(id, kind, true, 10)
        }
    }

    #[test]
    fn summary_counts_the_right_and_the_wrong_fixes_of_typo_cases_and_of_other_cases() {
        let outcomes = [
            fixed("a", CaseKind::Typo, true),
            fixed("b", CaseKind::Typo, false),
            fixed("c", CaseKind::Typo, true),
            fixed("d", CaseKind::PartialWord, false),
            outcome("e", CaseKind::Typo, true, 10),
        ];

        let summary = Summary::of(&outcomes);

        assert_eq!(
            (summary.typo_fixes, summary.other_fixes),
            (Fixes { right: 2, wrong: 1 }, Fixes { right: 0, wrong: 1 })
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

    fn typed(id: &str, kind: CaseKind, is_match: bool, typed_log_probability: Option<f64>) -> Outcome {
        Outcome {
            typed_log_probability,
            ..outcome(id, kind, is_match, 10)
        }
    }

    #[test]
    fn typo_cut_catches_the_typos_and_hides_the_other_cases_below_it() {
        let outcomes = [
            typed("a", CaseKind::Typo, true, Some(-20.0)),
            typed("b", CaseKind::Typo, false, Some(-5.0)),
            typed("c", CaseKind::PartialWord, true, Some(-11.0)),
            typed("d", CaseKind::NextWord, false, Some(-13.0)),
            typed("e", CaseKind::NextWord, true, Some(-1.0)),
            typed("f", CaseKind::NoNewLine, true, Some(-30.0)),
            typed("g", CaseKind::PartialWord, true, None),
        ];

        let point = TypoPoint::of(&outcomes, -10.0);

        assert_eq!(
            point,
            TypoPoint {
                log_probability: -10.0,
                typos: 2,
                caught: 1,
                others: 3,
                hidden: 2,
                hidden_matches: 1
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
}
