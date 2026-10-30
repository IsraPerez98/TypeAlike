//! The gate that hides a suggestion whose first word is likely wrong.

use crate::hub::ModelFile;

use super::{LogProbability, TokenChoice};

/// The weights of the score of a first word: of its log probability, and of the margin and the
/// entropy of the choice of its first token. They come from a logistic regression on the cases of
/// the eval, which tells whether the first word is the word that the writer typed next.
const LOG_PROBABILITY_WEIGHT: f32 = 0.461;
const MARGIN_WEIGHT: f32 = 0.480;
const ENTROPY_WEIGHT: f32 = -0.399;
/// The margin from which the model is sure of its choice. A larger margin scores as this one.
const MAX_MARGIN: f32 = 8.0;
/// The least score of a first word of a Gemma model that shows. A first word with a lower score
/// is right in fewer than 1 of 4 cases.
const GEMMA_MIN_SCORE: f32 = -1.169;
/// The least score of a first word of another model that shows, from the cases of Qwen3.5 0.8B.
/// Such a model is right more often than Gemma at the same score.
const OTHER_MIN_SCORE: f32 = -2.046;
/// The text in the file name of a Gemma model.
const GEMMA_NAME: &str = "gemma";

/// Hides a suggestion whose first word is likely wrong: a first word that the model was not sure
/// enough of. A wrong suggestion costs the reader more time than it saves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FirstWordGate {
    min_score: f32,
}

impl FirstWordGate {
    /// Returns the gate for the suggestions of the model in `model`: the gate of Gemma for a file
    /// with "gemma" in its name, else the gate of the other models.
    #[must_use]
    pub fn for_model(model: &ModelFile) -> Self {
        let is_gemma = model.name.to_lowercase().contains(GEMMA_NAME);
        Self {
            min_score: if is_gemma {
                GEMMA_MIN_SCORE
            } else {
                OTHER_MIN_SCORE
            },
        }
    }

    /// Whether a first word with `log_probability` shows, when the model chose its first token with
    /// `choice`.
    #[must_use]
    pub fn shows(self, log_probability: LogProbability, choice: TokenChoice) -> bool {
        score(log_probability, choice) >= self.min_score
    }
}

/// How sure the model was of a first word with `log_probability`, whose first token it chose with
/// `choice`.
fn score(log_probability: LogProbability, choice: TokenChoice) -> f32 {
    LOG_PROBABILITY_WEIGHT * log_probability.0
        + MARGIN_WEIGHT * choice.margin.min(MAX_MARGIN)
        + ENTROPY_WEIGHT * choice.entropy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gemma() -> FirstWordGate {
        FirstWordGate::for_model(&ModelFile {
            repo: "unsloth/gemma-4-E4B-it-qat-GGUF".to_owned(),
            name: "gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf".to_owned(),
        })
    }

    fn qwen() -> FirstWordGate {
        FirstWordGate::for_model(&ModelFile {
            repo: "mradermacher/Qwen3.5-0.8B-Base-i1-GGUF".to_owned(),
            name: "Qwen3.5-0.8B-Base.i1-IQ4_NL.gguf".to_owned(),
        })
    }

    fn choice(margin: f32, entropy: f32) -> TokenChoice {
        TokenChoice { margin, entropy }
    }

    #[test]
    fn likely_first_word_of_a_sure_choice_shows() {
        let shows = gemma().shows(LogProbability(-0.1), choice(5.0, 0.2));

        assert!(shows);
    }

    #[test]
    fn unlikely_first_word_of_an_unsure_choice_is_hidden() {
        let shows = [gemma(), qwen()].map(|gate| gate.shows(LogProbability(-2.5), choice(0.1, 3.0)));

        assert_eq!(shows, [false, false]);
    }

    #[test]
    fn gate_of_other_models_than_gemma_hides_fewer_first_words() {
        let shows = [gemma(), qwen()].map(|gate| gate.shows(LogProbability(-1.0), choice(0.5, 3.0)));

        assert_eq!(shows, [false, true]);
    }

    #[test]
    fn margin_above_the_most_counts_as_the_most() {
        let shows = gemma().shows(LogProbability(-12.0), choice(f32::INFINITY, 0.0));

        assert!(!shows);
    }
}
