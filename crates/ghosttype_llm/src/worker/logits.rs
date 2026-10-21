//! The math on the logits of the model, and the bias that a completion adds to them.

use std::collections::HashMap;

use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::vocab::LlamaVocab;

use super::pieces::TokenPieces;
use super::words::starts_word;

/// The probability of the most likely token from which the model is sure of it, so that a word
/// bias does not change it.
const SURE_PROBABILITY: f32 = 0.5;

/// The logits that a completion adds to its first token, and to each later token.
#[derive(Debug, Default)]
pub(super) struct TokenBias {
    first: Vec<(LlamaToken, f32)>,
    later: Vec<(LlamaToken, f32)>,
}

impl TokenBias {
    /// Returns the bias that adds `word_tokens` to each token, and also `next_word_tokens` to the
    /// first token.
    pub(super) fn new(word_tokens: Vec<(LlamaToken, f32)>, next_word_tokens: Vec<(LlamaToken, f32)>) -> Self {
        let mut first = word_tokens.iter().copied().collect::<HashMap<_, _>>();
        for (token, bias) in next_word_tokens {
            *first.entry(token).or_default() += bias;
        }
        Self {
            first: first.into_iter().collect(),
            later: word_tokens,
        }
    }

    /// Returns the logits to add to the token after `added` tokens.
    pub(super) fn at(&self, added: usize) -> &[(LlamaToken, f32)] {
        if added == 0 { &self.first } else { &self.later }
    }
}

/// Returns the token to add for `scores`, and the natural log of its probability in `logits`. While
/// text is unwritten, the token agrees with it, and its probability is its share of the tokens that
/// agree.
pub(super) fn choose_token(
    scores: &[f32],
    logits: &[f32],
    pieces: &TokenPieces,
    unwritten: &[u8],
) -> (LlamaToken, f32) {
    if let Some(token) = best_token_agreeing_with(scores, pieces, unwritten) {
        return (
            token,
            agreeing_log_probability(logits, token, pieces.agreeing_with(unwritten)),
        );
    }
    let token = most_likely(scores);
    (token, log_probability_of(logits, token))
}

/// Returns the natural log of the probability that `logits` give to `token`.
fn log_probability_of(logits: &[f32], token: LlamaToken) -> f32 {
    let highest = highest_of(logits);
    logit_of(logits, token) - highest - exp_total(logits, highest).ln()
}

/// Returns the natural log of the probability that `logits` give to `token`, among the `agreeing`
/// tokens. `token` must be one of them.
pub(super) fn agreeing_log_probability(
    logits: &[f32],
    token: LlamaToken,
    agreeing: impl Iterator<Item = LlamaToken>,
) -> f32 {
    let chosen = logit_of(logits, token);
    let total = agreeing
        .map(|other| (logit_of(logits, other) - chosen).exp())
        .sum::<f32>();
    -total.ln()
}

/// Returns the `count` most likely tokens of `logits`, the most likely first, with the lowest id
/// first among equally likely ones.
pub(super) fn most_likely_tokens(logits: &[f32], count: usize) -> Vec<LlamaToken> {
    let mut ranked = logits
        .iter()
        .enumerate()
        .map(|(index, &logit)| (logit, index))
        .collect::<Vec<_>>();
    let by_logit =
        |left: &(f32, usize), right: &(f32, usize)| right.0.total_cmp(&left.0).then(left.1.cmp(&right.1));
    if count < ranked.len() {
        ranked.select_nth_unstable_by(count, by_logit);
        ranked.truncate(count);
    }
    ranked.sort_unstable_by(by_logit);
    ranked
        .into_iter()
        .map(|(_, index)| LlamaToken(i32::try_from(index).expect("the vocabulary fits in i32")))
        .collect()
}

/// Returns the natural log of the probability that `logits` give to all of the `agreeing` tokens
/// together: how likely the model finds the next typed letters.
pub(super) fn typed_log_probability(logits: &[f32], agreeing: impl Iterator<Item = LlamaToken>) -> f32 {
    let highest = highest_of(logits);
    let typed = agreeing
        .map(|token| (logit_of(logits, token) - highest).exp())
        .sum::<f32>();
    typed.ln() - exp_total(logits, highest).ln()
}

fn highest_of(logits: &[f32]) -> f32 {
    logits.iter().copied().fold(f32::NEG_INFINITY, f32::max)
}

/// Returns the sum of `exp(logit - highest)` for each of `logits`: the softmax total, scaled so
/// that it does not overflow.
fn exp_total(logits: &[f32], highest: f32) -> f32 {
    logits.iter().map(|logit| (logit - highest).exp()).sum()
}

fn logit_of(logits: &[f32], token: LlamaToken) -> f32 {
    usize::try_from(token.0)
        .ok()
        .and_then(|index| logits.get(index))
        .copied()
        .unwrap_or(f32::NEG_INFINITY)
}

/// Returns the most likely token in `logits` whose text can write the start of the `unwritten`
/// text, the token with the lowest id of equally likely ones, or `None` when no text is unwritten.
fn best_token_agreeing_with(logits: &[f32], pieces: &TokenPieces, unwritten: &[u8]) -> Option<LlamaToken> {
    if unwritten.is_empty() {
        return None;
    }
    pieces.agreeing_with(unwritten).max_by(|left, right| {
        logit_of(logits, *left)
            .total_cmp(&logit_of(logits, *right))
            .then(right.0.cmp(&left.0))
    })
}

/// Returns the first token of each of `words` after a space, with the most logits of the words
/// that start with it. It leaves out a token that does not start a word or that is only a space.
pub(super) fn first_word_tokens(vocab: &LlamaVocab<'_>, words: &[(String, f32)]) -> Vec<(LlamaToken, f32)> {
    let mut biases = HashMap::<LlamaToken, f32>::new();
    for (word, bias) in words {
        let Some(&first) = vocab
            .tokenize(format!(" {word}").as_bytes(), false, false)
            .first()
        else {
            continue;
        };
        let piece = vocab.token_to_piece(first, false, None);
        if !starts_word(&piece) || piece.trim_ascii().is_empty() {
            continue;
        }
        let kept = biases.entry(first).or_insert(*bias);
        *kept = kept.max(*bias);
    }
    biases.into_iter().collect()
}

/// Returns `logits` with `token_bias` added, in `biased` when the bias has tokens. The bias stays
/// out when the model is sure of its first choice, at least [`SURE_PROBABILITY`].
pub(super) fn with_bias<'logits>(
    logits: &'logits [f32],
    token_bias: &[(LlamaToken, f32)],
    biased: &'logits mut Vec<f32>,
) -> &'logits [f32] {
    if token_bias.is_empty() || top_probability(logits) >= SURE_PROBABILITY {
        return logits;
    }
    biased.clear();
    biased.extend_from_slice(logits);
    for &(token, bias) in token_bias {
        let index = usize::try_from(token.0).expect("a token is never negative");
        if let Some(logit) = biased.get_mut(index) {
            *logit += bias;
        }
    }
    biased
}

/// Returns the probability of the most likely token in `logits`.
fn top_probability(logits: &[f32]) -> f32 {
    exp_total(logits, highest_of(logits)).recip()
}

/// Returns the most likely token in `logits`, the token with the highest id of equally likely
/// ones.
fn most_likely(logits: &[f32]) -> LlamaToken {
    let highest = highest_of(logits);
    let index = logits
        .iter()
        .rposition(|logit| logit.total_cmp(&highest).is_eq())
        .unwrap_or(0);
    LlamaToken(i32::try_from(index).expect("the vocabulary fits in i32"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::test_support::assert_near;

    #[test]
    fn healing_takes_the_most_likely_agreeing_token_and_the_lowest_id_of_a_tie() {
        let pieces = TokenPieces::new(
            [" be", " beauti", " beautiful", " cat"]
                .iter()
                .zip(0..)
                .map(|(piece, id)| (LlamaToken(id), piece.as_bytes().to_vec())),
        );

        let token = best_token_agreeing_with(&[1.0, 3.0, 3.0, 9.0], &pieces, b" beauti");

        assert_eq!(token, Some(LlamaToken(1)));
    }

    #[test]
    fn bias_adds_to_the_logits_of_its_tokens() {
        let mut biased = Vec::new();

        let scores = with_bias(
            &[1.0, 1.2, 1.4],
            &[(LlamaToken(0), 2.5), (LlamaToken(9), 1.0)],
            &mut biased,
        );

        assert_eq!(scores, [3.5, 1.2, 1.4]);
    }

    #[test]
    fn bias_stays_out_when_the_model_is_sure_of_its_first_choice() {
        let mut biased = Vec::new();

        let scores = with_bias(&[9.0, 1.0, 1.0], &[(LlamaToken(1), 20.0)], &mut biased);

        assert_eq!(scores, [9.0, 1.0, 1.0]);
    }

    #[test]
    fn next_word_bias_adds_to_the_word_bias_of_the_first_token_only() {
        let bias = TokenBias::new(
            vec![(LlamaToken(1), 1.0)],
            vec![(LlamaToken(1), 0.5), (LlamaToken(2), 0.25)],
        );

        let mut first = bias.at(0).to_vec();
        first.sort_by_key(|(token, _)| token.0);
        assert_eq!(first, [(LlamaToken(1), 1.5), (LlamaToken(2), 0.25)]);
        assert_eq!(bias.at(1), [(LlamaToken(1), 1.0)]);
    }

    #[test]
    fn top_probability_is_the_softmax_of_the_highest_logit() {
        let probabilities = [&[0.0, 0.0][..], &[0.0, 0.0, 0.0, 0.0], &[5.0]].map(top_probability);

        assert_eq!(probabilities, [0.5, 0.25, 1.0]);
    }

    #[test]
    fn no_bias_keeps_the_logits() {
        let mut biased = Vec::new();

        let scores = with_bias(&[1.0, 2.0], &[], &mut biased);

        assert_eq!(scores, [1.0, 2.0]);
    }

    #[test]
    fn most_likely_token_has_the_highest_logit() {
        let token = most_likely(&[0.5, 2.0, -1.0]);

        assert_eq!(token, LlamaToken(1));
    }

    #[test]
    fn log_probability_is_the_log_of_the_softmax_share() {
        let log_probability = log_probability_of(&[0.0, 2.0_f32.ln(), 0.0], LlamaToken(1));

        assert_near(log_probability, 0.5_f32.ln());
    }

    #[test]
    fn healing_log_probability_is_the_share_among_the_agreeing_tokens() {
        let logits = [3.0_f32.ln(), 1.0_f32.ln(), 5.0_f32.ln()];

        let log_probability =
            agreeing_log_probability(&logits, LlamaToken(0), [LlamaToken(0), LlamaToken(1)].into_iter());

        assert_near(log_probability, 0.75_f32.ln());
    }

    #[test]
    fn typed_log_probability_is_the_softmax_share_of_all_agreeing_tokens() {
        let logits = [3.0_f32.ln(), 1.0_f32.ln(), 5.0_f32.ln()];

        let log_probability = typed_log_probability(&logits, [LlamaToken(0), LlamaToken(1)].into_iter());

        assert_near(log_probability, (4.0_f32 / 9.0).ln());
    }

    #[test]
    fn most_likely_tokens_come_first() {
        let tokens = most_likely_tokens(&[1.0, 5.0, 3.0, 4.0], 3);

        assert_eq!(tokens, [1, 3, 2].map(LlamaToken));
    }
}
