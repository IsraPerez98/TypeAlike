//! The cases that changed against an earlier run of the same cases.

use std::collections::HashMap;

use super::Outcome;
use super::summary::mean;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::CaseKind;
    use crate::score::test_outcomes::{outcome, scored};

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
    fn comparison_skips_cases_that_the_baseline_does_not_have() {
        let outcomes = [outcome("new", CaseKind::NextWord, true, 10)];

        let comparison = Comparison::of(&[], &outcomes);

        assert_eq!(comparison, Comparison::default());
    }
}
