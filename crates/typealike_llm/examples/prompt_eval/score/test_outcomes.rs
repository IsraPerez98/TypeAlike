//! The outcomes that the tests of the scores, the summary and the comparison build.

use super::{CaseKind, Outcome, WordCounts};

pub fn outcome(id: &str, kind: CaseKind, is_match: bool, millis: u64) -> Outcome {
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
        typed_log_probability: None,
        fixed_word: None,
        is_fix_right: None,
        reference: None,
    }
}

pub fn scored(id: &str, is_match: bool, log_probability: f64) -> Outcome {
    Outcome {
        log_probability: Some(log_probability),
        ..outcome(id, CaseKind::NextWord, is_match, 10)
    }
}
