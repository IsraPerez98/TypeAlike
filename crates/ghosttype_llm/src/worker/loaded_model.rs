//! A loaded model, which completes each prompt and keeps the cache of the last one.

use std::time::Instant;

use ghosttype_core::inference::{
    CONTEXT_TOKENS, Completion, InferenceError, NextWordBias, Prompt, TokenCount, WordBias,
};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::LlamaModel;

use super::generation::{
    AddedTokens, Healing, Limits, Progress, added_text, last_word_start, take_back_last_word,
};
use super::logits::{LogitBias, TokenBias, first_word_tokens};
use super::model_params::CpuThreads;
use super::pieces::TokenPieces;
use super::prompt_cache::{GenerateError, PromptCache};

/// A loaded model, with the cache of its last prompt.
pub(super) struct LoadedModel<'model> {
    llama: &'model LlamaBackend,
    model: &'model LlamaModel,
    cpu_threads: Option<CpuThreads>,
    cache: Option<PromptCache<'model>>,
    /// The logits of the last word bias.
    word_tokens: Option<WordTokens>,
    pieces: TokenPieces,
}

/// The logits that one version of the word bias adds to the first token of each word.
struct WordTokens {
    version: u64,
    biases: Vec<LogitBias>,
}

impl<'model> LoadedModel<'model> {
    /// Keeps `model` with an empty cache. A context on the CPU runs on `cpu_threads`, or on the
    /// threads of llama.cpp.
    pub(super) fn new(
        llama: &'model LlamaBackend,
        model: &'model LlamaModel,
        cpu_threads: Option<CpuThreads>,
    ) -> Self {
        Self {
            llama,
            model,
            cpu_threads,
            cache: None,
            word_tokens: None,
            pieces: TokenPieces::new(model.tokens(false)),
        }
    }

    /// Completes `prompt`, and sends the text to `progress` while it grows. It stops early when
    /// `progress` tells that a newer completion waits.
    pub(super) fn complete(
        &mut self,
        prompt: &Prompt,
        limits: Limits,
        token_bias: &TokenBias,
        progress: &mut Progress<'_>,
    ) -> Result<Completion, InferenceError> {
        let started = Instant::now();
        let (text, added) = self
            .generate(prompt, limits, token_bias, progress)
            .map_err(|error| {
                if !matches!(error, GenerateError::Superseded) {
                    tracing::warn!(%error, "failed to complete text");
                }
                InferenceError::from(error)
            })?;
        Ok(Completion {
            text,
            tokens: TokenCount(u32::try_from(added.tokens.len()).expect("added tokens fit in the context")),
            duration: started.elapsed(),
            word_log_probabilities: added.word_log_probabilities,
            typed_log_probability: added.typed_log_probability,
            correction: added.correction,
        })
    }

    /// Returns the logits that `word_bias` adds to each token, and that `next_word_bias` adds to
    /// the first token. It finds the tokens of each version of `word_bias` one time.
    pub(super) fn token_bias(
        &mut self,
        word_bias: Option<&WordBias>,
        next_word_bias: &NextWordBias,
    ) -> TokenBias {
        let next_word_tokens = first_word_tokens(&self.model.vocab(), &next_word_bias.0);
        TokenBias::new(self.word_tokens(word_bias), next_word_tokens)
    }

    /// Returns the logits that `word_bias` adds to each token, which it finds one time for each
    /// version.
    fn word_tokens(&mut self, word_bias: Option<&WordBias>) -> Vec<LogitBias> {
        let Some(word_bias) = word_bias else {
            return Vec::new();
        };
        let is_known = self
            .word_tokens
            .as_ref()
            .is_some_and(|known| known.version == word_bias.version);
        if !is_known {
            self.word_tokens = Some(WordTokens {
                version: word_bias.version,
                biases: first_word_tokens(&self.model.vocab(), &word_bias.words),
            });
        }
        self.word_tokens
            .as_ref()
            .map(|known| known.biases.clone())
            .unwrap_or_default()
    }

    /// Continues `prompt` greedily until the model ends the text, writes markup, ends the line or
    /// reaches the `limits`. It adds `token_bias` to the logits before it picks each token. It
    /// keeps the prompt in the cache for the next completion, and forgets the cache when llama.cpp
    /// fails.
    ///
    /// It heals the last word: it takes the tokens of the last word back from the prompt, and makes
    /// the first added tokens write that text again. A prompt that ends in a space or in a part of
    /// a word otherwise ends on tokens that the model seldom saw there, and the model continues it
    /// badly.
    fn generate(
        &mut self,
        prompt: &Prompt,
        limits: Limits,
        token_bias: &TokenBias,
        progress: &mut Progress<'_>,
    ) -> Result<(String, AddedTokens), GenerateError> {
        let model = self.model;
        let vocab = model.vocab();
        let mut prompt_tokens = vocab.tokenize(prompt.0.as_bytes(), true, false);
        let healed_text = take_back_last_word(&vocab, &mut prompt_tokens);
        let last_word_start = last_word_start(&vocab, &prompt_tokens);
        let context_tokens = CONTEXT_TOKENS.0.min(model.n_ctx_train());
        let capacity = usize::try_from(context_tokens).expect("a u32 fits in usize");
        if prompt_tokens.is_empty() {
            return Err(GenerateError::EmptyPrompt);
        }
        if prompt_tokens.len().saturating_add(limits.max_added) > capacity {
            return Err(GenerateError::PromptTooLong);
        }
        if self.cache.is_none() {
            self.cache = Some(PromptCache::new(
                self.llama,
                model,
                context_tokens,
                self.cpu_threads,
            )?);
        }
        let cache = self.cache.as_mut().expect("the cache exists above");
        let healing = Healing {
            pieces: &self.pieces,
            text: &healed_text,
            first_token: None,
        };
        let added = cache
            .decode_prompt(&prompt_tokens, last_word_start, progress.is_outdated)
            .and_then(|()| cache.continue_prompt(&vocab, &healing, limits, token_bias, progress))
            .and_then(|added| cache.continue_fixed(&vocab, &healing, added, limits, token_bias, progress));
        match added {
            Ok(added) => {
                let healed = added
                    .correction
                    .as_ref()
                    .map_or(healed_text.as_slice(), |correction| correction.fixed.as_bytes());
                Ok((added_text(&vocab, &prompt_tokens, &added.tokens, healed), added))
            }
            Err(GenerateError::Superseded) => Err(GenerateError::Superseded),
            Err(error) => {
                self.cache = None;
                Err(error)
            }
        }
    }
}
