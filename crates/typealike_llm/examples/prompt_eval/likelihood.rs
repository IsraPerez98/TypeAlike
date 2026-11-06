//! Measures the tokens of each prompt, and how likely the model finds the next word after it. A
//! small change of the prompt moves the likelihood a little, while it seldom changes the word that
//! the model suggests.

use std::num::NonZeroU32;
use std::path::Path;

use anyhow::bail;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::{LlamaLazyMode, LlamaModelParams};
use llama_cpp_2::token::LlamaToken;
use typealike_core::inference::TokenCount;

const BATCH_TOKENS: u32 = 512;
const LAST_OUTPUT: i32 = -1;

/// A prompt and the text that the writer typed after it.
#[derive(Debug)]
pub struct Continuation {
    /// The prompt that the app built.
    pub prompt: String,
    /// The text that the writer typed after the prompt.
    pub text: String,
}

/// What the model gives for one continuation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    /// The tokens of the prompt.
    pub prompt_tokens: u64,
    /// The natural log of the probability of the text after the prompt, or `None` when the text
    /// adds no token.
    pub log_probability: Option<f64>,
}

/// Measures each continuation with the model at `model_path` in a context of `context_tokens`, or
/// gives `None` for no continuation. Call it only when no other model runner is alive, because
/// llama.cpp starts once for each process. It loads the model with the lazy reads of the app, so
/// that the most memory of a run (`/usr/bin/time -l`) stays the memory of the app.
pub fn measure(
    model_path: &Path,
    context_tokens: TokenCount,
    continuations: &[Option<&Continuation>],
) -> anyhow::Result<Vec<Option<Measured>>> {
    let llama = LlamaBackend::init()?;
    let model_params = LlamaModelParams::default().with_lazy_mode(LlamaLazyMode::On);
    let model = LlamaModel::load_from_file(&llama, model_path, &model_params)?;
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(context_tokens.0))
        .with_n_batch(BATCH_TOKENS)
        .with_n_ubatch(BATCH_TOKENS);
    let mut context = model.new_context(&llama, params)?;
    continuations
        .iter()
        .map(|continuation| {
            continuation
                .map(|continuation| measure_one(&mut context, &model, continuation))
                .transpose()
        })
        .collect()
}

/// Returns the tokens of the prompt and the log probability of the continuation text after it.
fn measure_one(
    context: &mut LlamaContext<'_>,
    model: &LlamaModel,
    continuation: &Continuation,
) -> anyhow::Result<Measured> {
    let prompt = model
        .vocab()
        .tokenize(continuation.prompt.as_bytes(), true, false);
    Ok(Measured {
        prompt_tokens: u64::try_from(prompt.len())?,
        log_probability: log_probability(context, model, continuation, &prompt)?,
    })
}

/// Returns the log probability of the continuation text after the `prompt` tokens. When the text
/// joins the last word of the prompt into other tokens, as after a part of a word or a space, the
/// value also holds the probability of these last tokens, like the token healing of the model
/// runner.
fn log_probability(
    context: &mut LlamaContext<'_>,
    model: &LlamaModel,
    continuation: &Continuation,
    prompt: &[LlamaToken],
) -> anyhow::Result<Option<f64>> {
    let vocab = model.vocab();
    let full_text = format!("{}{}", continuation.prompt, continuation.text);
    let full = vocab.tokenize(full_text.as_bytes(), true, false);
    let shared = shared_tokens(prompt, &full);
    let (start, rest) = full.split_at(shared);
    if start.is_empty() || rest.is_empty() {
        return Ok(None);
    }
    if rest.len() > usize::try_from(BATCH_TOKENS)? {
        bail!("the continuation has more than {BATCH_TOKENS} tokens");
    }
    context.clear_kv_cache();
    decode(context, start, 0, Outputs::Last)?;
    let mut log_probability = token_log_probability(context.get_logits_ith(LAST_OUTPUT), rest[0]);
    decode(context, rest, shared, Outputs::All)?;
    for (offset, &token) in rest.iter().enumerate().skip(1) {
        log_probability += token_log_probability(context.get_logits_ith(position(offset - 1)?), token);
    }
    Ok(Some(log_probability))
}

/// The tokens of a decode that give their output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outputs {
    Last,
    All,
}

/// Decodes `tokens` in batches from position `start` on. With [`Outputs::All`], `tokens` must fit
/// in one batch, so that the output of each token stays readable by its index.
fn decode(
    context: &mut LlamaContext<'_>,
    tokens: &[LlamaToken],
    start: usize,
    outputs: Outputs,
) -> anyhow::Result<()> {
    let batch_size = usize::try_from(BATCH_TOKENS)?;
    for (chunk_index, chunk) in tokens.chunks(batch_size).enumerate() {
        let mut batch = LlamaBatch::new(chunk.len(), 1);
        for (offset, &token) in chunk.iter().enumerate() {
            let index = chunk_index * batch_size + offset;
            let has_output = outputs == Outputs::All || index + 1 == tokens.len();
            batch.add(token, position(start + index)?, &[0], has_output)?;
        }
        context.decode(&mut batch)?;
    }
    Ok(())
}

/// Returns how many tokens `left` and `right` both start with.
fn shared_tokens(left: &[LlamaToken], right: &[LlamaToken]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left == right)
        .count()
}

/// Returns the natural log of the probability that `logits` give to `token`.
fn token_log_probability(logits: &[f32], token: LlamaToken) -> f64 {
    let largest = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let total = logits
        .iter()
        .map(|&logit| (f64::from(logit) - f64::from(largest)).exp())
        .sum::<f64>();
    let logit = usize::try_from(token.0)
        .ok()
        .and_then(|index| logits.get(index))
        .map_or(f64::NEG_INFINITY, |&logit| f64::from(logit));
    logit - f64::from(largest) - total.ln()
}

fn position(index: usize) -> anyhow::Result<i32> {
    Ok(i32::try_from(index)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(ids: &[i32]) -> Vec<LlamaToken> {
        ids.iter().copied().map(LlamaToken).collect()
    }

    #[test]
    fn shared_tokens_counts_the_tokens_at_the_start_of_both() {
        let cases: [(&[i32], &[i32], usize); 3] = [
            (&[1, 2, 3], &[1, 2, 4, 5], 2),
            (&[1, 2], &[1, 2, 3], 2),
            (&[7], &[1], 0),
        ];

        let shared = cases.map(|(left, right, _)| shared_tokens(&tokens(left), &tokens(right)));

        assert_eq!(shared, cases.map(|(_, _, count)| count));
    }

    #[test]
    fn token_log_probability_is_the_log_of_its_softmax_share() {
        let logits = [0.0, 2.0_f32.ln(), 0.0];

        let log_probability = token_log_probability(&logits, LlamaToken(1));

        assert!((log_probability - 0.5_f64.ln()).abs() < 1e-6, "{log_probability}");
    }

    #[test]
    fn token_outside_the_vocabulary_is_impossible() {
        let log_probability = token_log_probability(&[0.0, 0.0], LlamaToken(5));

        assert!(log_probability.is_infinite() && log_probability.is_sign_negative());
    }
}
