//! A loaded model, which completes each prompt and keeps the cache of the last one.

use std::time::Instant;

use llama_cpp_2::LlamaContextLoadError;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::LlamaModel;
use typealike_core::inference::{
    CONTEXT_TOKENS, Completion, InferenceError, NextWordBias, Prompt, TokenCount, WordBias,
};

use super::drafter::GUESSED_TOKENS;
use super::generation::{
    AddedTokens, Decoder, Healing, Limits, Progress, added_text, last_word_start, take_back_last_word,
};
use super::logits::{LogitBias, TokenBias, first_word_tokens};
use super::model_params::{ContextSetup, WarmUp};
use super::pieces::TokenPieces;
use super::prompt_cache::{GenerateError, PromptCache};

/// The text that the warm-up repeats to fill its prompt. The kernels do not depend on the text.
const WARM_UP_TEXT: &str = "The quick brown fox jumps over the lazy dog. ";

/// A loaded model, with the cache of its last prompt.
pub(super) struct LoadedModel<'model> {
    llama: &'model LlamaBackend,
    model: &'model LlamaModel,
    /// The model that guesses the next tokens of `model`, if any.
    drafter: Option<&'model LlamaModel>,
    setup: ContextSetup,
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
    /// Keeps `model` with an empty cache. Its context runs as `setup` tells, and its completions
    /// also run the `drafter`, if any.
    pub(super) fn new(
        llama: &'model LlamaBackend,
        model: &'model LlamaModel,
        drafter: Option<&'model LlamaModel>,
        setup: ContextSetup,
    ) -> Self {
        if let Some(threads) = setup.cpu_threads {
            tracing::info!(
                prompt = threads.prompt,
                generation = threads.generation,
                "running the model on CPU threads"
            );
        }
        Self {
            llama,
            model,
            drafter,
            setup,
            cache: None,
            word_tokens: None,
            pieces: TokenPieces::new(model.tokens(false), |token| model.vocab().is_eog(token)),
        }
    }

    /// Makes the context and runs the decodes of a first completion on a prompt of one batch, when
    /// the setup asks for a warm-up. Thus the backend prepares its kernels and its memory before
    /// the first completion. Then the cache forgets that prompt. Before each step, it stops when
    /// `is_outdated` tells so: a waiting completion does the same work, and a waiting load or
    /// unload needs no warm-up.
    pub(super) fn warm_up(&mut self, is_outdated: &mut dyn FnMut() -> bool) {
        if self.setup.warm_up == WarmUp::Off {
            return;
        }
        let started = Instant::now();
        match self.decode_warm_up(is_outdated) {
            Ok(()) => tracing::info!(duration = ?started.elapsed(), "warmed up the model"),
            Err(GenerateError::Superseded) => tracing::info!("stopped the warm-up for a waiting request"),
            Err(error) => {
                self.cache = None;
                tracing::warn!(%error, "failed to warm up the model");
            }
        }
    }

    fn decode_warm_up(&mut self, is_outdated: &mut dyn FnMut() -> bool) -> Result<(), GenerateError> {
        if is_outdated() {
            return Err(GenerateError::Superseded);
        }
        self.make_cache()?;
        let text_tokens = self.model.vocab().tokenize(WARM_UP_TEXT.as_bytes(), false, false);
        let batch_tokens = usize::try_from(self.setup.batch_tokens).expect("a u32 fits in usize");
        let prompt = text_tokens
            .iter()
            .copied()
            .cycle()
            .take(batch_tokens)
            .collect::<Vec<_>>();
        let cache = self.cache.as_mut().expect("the cache exists above");
        cache.warm_up(&prompt, is_outdated)
    }

    /// Frees the context with the cache of the last prompt and the buffers of the backend. The
    /// next completion makes them again and decodes its whole prompt.
    pub(super) fn free_context(&mut self) {
        if self.cache.take().is_some() {
            tracing::info!("freed the context of the model");
        }
    }

    /// Makes the cache of the prompts, when it does not exist. When the drafter fails to start, it
    /// makes the cache without the drafter, and forgets the drafter.
    fn make_cache(&mut self) -> Result<(), LlamaContextLoadError> {
        if self.cache.is_some() {
            return Ok(());
        }
        let context_tokens = self.context_tokens();
        if let Some(drafter) = self.drafter {
            match PromptCache::with_drafter(self.llama, self.model, drafter, context_tokens, self.setup) {
                Ok(cache) => {
                    self.cache = Some(cache);
                    return Ok(());
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to start the drafter, so the model runs without it");
                    self.drafter = None;
                }
            }
        }
        self.cache = Some(PromptCache::new(
            self.llama,
            self.model,
            context_tokens,
            self.setup,
        )?);
        Ok(())
    }

    /// Returns the tokens of the context: those of the app, or fewer when the model learned fewer.
    fn context_tokens(&self) -> u32 {
        CONTEXT_TOKENS.0.min(self.model.n_ctx_train())
    }

    /// Completes `prompt`, and sends the text to `progress` while it grows. It stops early when
    /// `progress` tells that a newer completion waits.
    pub(super) fn complete(
        &mut self,
        prompt: &Prompt,
        limits: Limits<'_>,
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
        limits: Limits<'_>,
        token_bias: &TokenBias,
        progress: &mut Progress<'_>,
    ) -> Result<(String, AddedTokens), GenerateError> {
        let model = self.model;
        let vocab = model.vocab();
        let mut prompt_tokens = vocab.tokenize(prompt.0.as_bytes(), true, false);
        let healed_text = take_back_last_word(&vocab, &mut prompt_tokens);
        let last_word_start = last_word_start(&vocab, &prompt_tokens);
        let capacity = usize::try_from(self.context_tokens()).expect("a u32 fits in usize");
        if prompt_tokens.is_empty() {
            return Err(GenerateError::EmptyPrompt);
        }
        if prompt_tokens
            .len()
            .saturating_add(limits.most_added() + GUESSED_TOKENS)
            > capacity
        {
            return Err(GenerateError::PromptTooLong);
        }
        self.make_cache()?;
        let cache = self.cache.as_mut().expect("the cache exists above");
        let decoder = Decoder {
            vocab: &vocab,
            pieces: &self.pieces,
            token_bias,
            limits,
        };
        let healing = Healing {
            text: &healed_text,
            first_token: None,
        };
        let added = cache
            .decode_prompt(&prompt_tokens, last_word_start, progress.is_outdated)
            .and_then(|()| cache.continue_prompt(&decoder, healing, progress))
            .and_then(|added| cache.continue_fixed(&decoder, healing, added, progress));
        match added {
            Ok(added) => {
                let healed = added
                    .correction
                    .as_ref()
                    .map_or(healed_text.as_slice(), |correction| correction.fixed.as_bytes());
                let text = added_text(&added.tokens, healed, |token| {
                    vocab.token_to_piece(token, false, None)
                });
                Ok((text, added))
            }
            Err(GenerateError::Superseded) => Err(GenerateError::Superseded),
            Err(error) => {
                self.cache = None;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use typealike_core::inference::{Backend, LogProbability, SentenceEnd, TypoCheck};

    use super::*;
    use crate::worker::load;
    use crate::worker::model_params::drafter_params;

    const BATCH_TOKENS: u32 = 128;
    const STORY: &str = "Once upon a time";

    fn complete(loaded: &mut LoadedModel<'_>, prompt: &str) -> String {
        let limits = Limits {
            max_added: 12,
            min_log_probability: Some(LogProbability(-6.0)),
            typo_check: TypoCheck::Off,
            sentence_end: SentenceEnd::Continue,
            line_after: b"",
        };
        let token_bias = loaded.token_bias(None, &NextWordBias::default());
        let mut progress = Progress {
            is_outdated: &mut || false,
            send_partial: &mut |_| {},
        };
        loaded
            .complete(&Prompt(prompt.to_owned()), limits, &token_bias, &mut progress)
            .expect("the model completes the prompt")
            .text
    }

    fn setup() -> ContextSetup {
        ContextSetup {
            cpu_threads: None,
            batch_tokens: BATCH_TOKENS,
            warm_up: WarmUp::Off,
        }
    }

    fn path_in(variable: &str) -> PathBuf {
        PathBuf::from(std::env::var_os(variable).unwrap_or_else(|| panic!("{variable} is set")))
    }

    #[test]
    #[ignore = "needs the path of a GGUF model file in TYPEALIKE_TEST_MODEL"]
    fn completion_after_freeing_the_context_gives_the_same_text() {
        let mut llama = None;
        let model = load(
            &mut llama,
            Some(path_in("TYPEALIKE_TEST_MODEL")),
            Backend::Cpu,
            BATCH_TOKENS,
        )
        .expect("the model loads");
        let llama = llama.as_ref().expect("the load starts llama.cpp");
        let mut loaded = LoadedModel::new(llama, &model, None, setup());
        let before = complete(&mut loaded, STORY);

        loaded.free_context();
        let after = complete(&mut loaded, STORY);

        assert_eq!(after, before);
    }

    #[test]
    #[ignore = "needs the paths of a Gemma 4 model in TYPEALIKE_TEST_MODEL and of its drafter in \
                TYPEALIKE_TEST_DRAFTER"]
    fn completions_with_the_drafter_give_the_texts_without_it() {
        let prompts = [
            STORY,
            "Once upon a time there was a",
            "Hi Anna,\n\nThanks for the notes from the meeting. I think we",
            "Dear Mr. Smith, I am writing to ask about the status of my ord",
        ];
        let mut llama = None;
        let model = load(
            &mut llama,
            Some(path_in("TYPEALIKE_TEST_MODEL")),
            Backend::Cpu,
            BATCH_TOKENS,
        )
        .expect("the model loads");
        let llama = llama.as_ref().expect("the load starts llama.cpp");
        let drafter_params = drafter_params(Backend::Cpu).expect("the CPU runs the drafter");
        let drafter = LlamaModel::load_from_file(llama, path_in("TYPEALIKE_TEST_DRAFTER"), &drafter_params)
            .expect("the drafter loads");
        let mut plain = LoadedModel::new(llama, &model, None, setup());
        let without_drafter = prompts.map(|prompt| complete(&mut plain, prompt));
        let mut with_guesses = LoadedModel::new(llama, &model, Some(&drafter), setup());

        let with_drafter = prompts.map(|prompt| complete(&mut with_guesses, prompt));

        assert_eq!(with_drafter, without_drafter);
    }
}
