//! A loaded model, which completes each prompt and keeps the cache of the last one.

use std::time::Instant;

use llama_cpp_2::LlamaContextLoadError;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::LlamaModel;
use typealike_core::inference::{Completion, InferenceError, NextWordBias, Prompt, TokenCount, WordBias};

use super::drafter::GUESSED_TOKENS;
#[cfg(feature = "suggestion-log")]
use super::generation::StopReason;
use super::generation::{
    AddedTokens, Decoder, Healing, Limits, Progress, Stop, added_text, last_word_start, take_back_last_word,
    word_before_typed_space,
};
use super::logits::{LogitBias, TokenBias, first_word_tokens};
use super::model_params::{ContextSetup, WarmUp};
use super::pieces::TokenPieces;
use super::prompt_cache::{FinishedWord, GenerateError, PromptCache};

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
    /// Whether the backend decoded added tokens since the load.
    added_token_decodes: AddedTokenDecodes,
    /// The logits of the last word bias.
    word_tokens: Option<WordTokens>,
    pieces: TokenPieces,
}

/// Whether the backend decoded added tokens since the load. The first decodes of added tokens
/// prepare the kernels and the memory of the backend for them, which makes them slow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddedTokenDecodes {
    /// No warm-up and no completion decoded added tokens yet.
    NotRun,
    /// A warm-up or a completion decoded added tokens.
    Ran,
}

/// Whether a model warms up the decodes of added tokens after a preparation: only when its load
/// warms up the backend (`warm_up`), and its `decodes` did not run since the load.
fn warms_up_added_tokens(warm_up: WarmUp, decodes: AddedTokenDecodes) -> bool {
    warm_up == WarmUp::AtLoad && decodes == AddedTokenDecodes::NotRun
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
            added_token_decodes: AddedTokenDecodes::NotRun,
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
            Ok(()) => {
                self.added_token_decodes = AddedTokenDecodes::Ran;
                tracing::info!(duration = ?started.elapsed(), "warmed up the model");
            }
            Err(GenerateError::Superseded) => tracing::info!("stopped the warm-up for a waiting request"),
            Err(error) => {
                self.cache = None;
                tracing::warn!(%error, "failed to warm up the model");
            }
        }
    }

    /// Decodes two added tokens after the prompt of the cache, as the next completion does, and
    /// then forgets them, so that the first key after a load does not prepare the backend for
    /// them. The next completion gives the same tokens. It runs once after a load that warms up
    /// the backend, when no warm-up and no completion decoded added tokens since the load and
    /// `is_outdated` does not tell that a request waits. Call it after a preparation.
    pub(super) fn warm_up_added_tokens(&mut self, is_outdated: &mut dyn FnMut() -> bool) {
        if !warms_up_added_tokens(self.setup.warm_up, self.added_token_decodes) || is_outdated() {
            return;
        }
        let Some(cache) = self.cache.as_mut() else {
            return;
        };
        let started = Instant::now();
        match cache.warm_up_added_tokens() {
            Ok(true) => {
                self.added_token_decodes = AddedTokenDecodes::Ran;
                tracing::info!(duration = ?started.elapsed(), "warmed up the decodes of added tokens");
            }
            Ok(false) => {}
            Err(error) => {
                self.added_token_decodes = AddedTokenDecodes::Ran;
                tracing::warn!(%error, "failed to warm up the decodes of added tokens");
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

    /// Returns the tokens of the context: those of the setup, or fewer when the model learned fewer.
    fn context_tokens(&self) -> u32 {
        self.setup.context_tokens.min(self.model.n_ctx_train())
    }

    /// Completes `prompt`, and sends the text to `progress` while it grows. It stops early when
    /// `progress` tells that a newer completion waits. Also returns why the completion stopped.
    pub(super) fn complete(
        &mut self,
        prompt: &Prompt,
        limits: Limits<'_>,
        token_bias: &TokenBias,
        progress: &mut Progress<'_>,
    ) -> Result<(Completion, Stop), InferenceError> {
        let started = Instant::now();
        if limits.max_added > 0 {
            self.added_token_decodes = AddedTokenDecodes::Ran;
        }
        let (text, added) = self
            .generate(prompt, limits, token_bias, progress)
            .map_err(|error| {
                if !matches!(error, GenerateError::Superseded) {
                    tracing::warn!(%error, "failed to complete text");
                }
                InferenceError::from(error)
            })?;
        let completion = Completion {
            text,
            tokens: TokenCount(u32::try_from(added.tokens.len()).expect("added tokens fit in the context")),
            duration: started.elapsed(),
            word_log_probabilities: added.word_log_probabilities,
            typed_log_probability: added.typed_log_probability,
            correction: added.correction,
        };
        tracing::debug!(
            ?prompt,
            added_tokens = completion.tokens.0,
            added_chars = completion.text.chars().count(),
            duration = ?completion.duration,
            is_corrected = completion.correction.is_some(),
            "completed the prompt"
        );
        Ok((completion, added.stop))
    }

    /// Returns the prompt decodes of the cache since the last take, and starts a new count.
    #[cfg(feature = "suggestion-log")]
    pub(super) fn take_prefill(&mut self) -> super::suggestion_log::Prefill {
        self.cache
            .as_mut()
            .map(PromptCache::take_prefill)
            .unwrap_or_default()
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
            #[cfg(feature = "suggestion-log")]
            tracing::info!(
                target: typealike_core::SUGGESTION_LOG_TARGET,
                version = word_bias.version,
                words = ?word_bias.words,
                "new word bias"
            );
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
        let finished_word = limits.finished_word_fix.and_then(|min| {
            Some((
                min,
                word_before_typed_space(&vocab, &prompt_tokens, &healed_text)?,
            ))
        });
        let fixed_word = match finished_word {
            Some((min, (before_word, word))) => cache.continue_fixed_word(
                &decoder,
                min,
                FinishedWord {
                    before: &before_word,
                    word: &word,
                    after: &healed_text,
                },
                progress,
            ),
            None => Ok(None),
        };
        let added = fixed_word.and_then(|fixed_word| match fixed_word {
            Some(added) => Ok(added),
            None => cache
                .decode_prompt(&prompt_tokens, last_word_start, progress.is_outdated)
                .and_then(|()| cache.continue_healed(&decoder, healing, progress)),
        });
        match added {
            Ok(mut added) => {
                let dropped_bytes = added
                    .dropped
                    .iter()
                    .flat_map(|&token| vocab.token_to_piece(token, false, None))
                    .collect::<Vec<_>>();
                added.stop.dropped_text = String::from_utf8_lossy(&dropped_bytes).into_owned();
                #[cfg(feature = "suggestion-log")]
                if added.stop.reason == StopReason::Typo && added.correction.is_none() {
                    added.stop.fix_candidate = cache.best_fix_candidate(&decoder, &healed_text);
                }
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
    use std::collections::HashSet;
    use std::path::PathBuf;

    use typealike_core::inference::{Backend, LogProbability, SentenceEnd, TypoCheck};

    use super::*;
    use crate::worker::load;
    use crate::worker::model_params::drafter_params;

    const BATCH_TOKENS: u32 = 128;
    const MAX_ADDED: usize = 12;
    const STORY: &str = "Once upon a time";
    const PROMPTS: [&str; 4] = [
        STORY,
        "Once upon a time there was a",
        "Hi Anna,\n\nThanks for the notes from the meeting. I think we",
        "Dear Mr. Smith, I am writing to ask about the status of my ord",
    ];

    fn complete(loaded: &mut LoadedModel<'_>, prompt: &str) -> String {
        completion_of(loaded, prompt, MAX_ADDED).text
    }

    fn completion_of(loaded: &mut LoadedModel<'_>, prompt: &str, max_added: usize) -> Completion {
        let limits = Limits {
            max_added,
            min_log_probability: Some(LogProbability(-6.0)),
            typo_check: TypoCheck::Off,
            finished_word_fix: None,
            sentence_end: SentenceEnd::Continue,
            line_after: b"",
            guesses: &[],
        };
        let token_bias = loaded.token_bias(None, &NextWordBias::default());
        let mut progress = Progress {
            is_outdated: &mut || false,
            send_partial: &mut |_| {},
        };
        loaded
            .complete(&Prompt(prompt.to_owned()), limits, &token_bias, &mut progress)
            .expect("the model completes the prompt")
            .0
    }

    fn setup() -> ContextSetup {
        ContextSetup {
            cpu_threads: None,
            batch_tokens: BATCH_TOKENS,
            warm_up: WarmUp::Off,
            context_tokens: 4096,
        }
    }

    fn warm_up_setup() -> ContextSetup {
        ContextSetup {
            warm_up: WarmUp::AtLoad,
            ..setup()
        }
    }

    /// Returns the text and the word probabilities of a completion of `prompt` after a preparation
    /// of it.
    fn words_after_a_preparation(
        loaded: &mut LoadedModel<'_>,
        prompt: &str,
    ) -> (String, Vec<LogProbability>) {
        completion_of(loaded, prompt, 0);
        let completion = completion_of(loaded, prompt, MAX_ADDED);
        (completion.text, completion.word_log_probabilities)
    }

    /// Returns the words of a completion of `prompt` as [`words_after_a_preparation`] does, with
    /// the warm-up of the added tokens after the preparation.
    fn words_after_a_warm_up(loaded: &mut LoadedModel<'_>, prompt: &str) -> (String, Vec<LogProbability>) {
        completion_of(loaded, prompt, 0);
        loaded.warm_up_added_tokens(&mut || false);
        let completion = completion_of(loaded, prompt, MAX_ADDED);
        (completion.text, completion.word_log_probabilities)
    }

    #[test]
    fn only_a_model_that_warms_up_and_decoded_no_added_tokens_warms_them_up() {
        let cases = [
            (WarmUp::AtLoad, AddedTokenDecodes::NotRun, true),
            (WarmUp::AtLoad, AddedTokenDecodes::Ran, false),
            (WarmUp::Off, AddedTokenDecodes::NotRun, false),
            (WarmUp::Off, AddedTokenDecodes::Ran, false),
        ];

        let warm_ups = cases.map(|(warm_up, decodes, _)| warms_up_added_tokens(warm_up, decodes));

        assert_eq!(warm_ups, cases.map(|(_, _, warms_up)| warms_up));
    }

    #[test]
    #[ignore = "needs the path of a GGUF model file in TYPEALIKE_TEST_MODEL"]
    fn warm_up_after_a_preparation_decodes_added_tokens() {
        let mut llama = None;
        let model = load(
            &mut llama,
            Some(path_in("TYPEALIKE_TEST_MODEL")),
            Backend::Cpu,
            BATCH_TOKENS,
        )
        .expect("the model loads");
        let llama = llama.as_ref().expect("the load starts llama.cpp");
        let mut loaded = LoadedModel::new(llama, &model, None, warm_up_setup());
        completion_of(&mut loaded, STORY, 0);
        let cache = loaded.cache.as_mut().expect("the preparation makes the cache");

        let decoded = cache.warm_up_added_tokens();

        assert!(matches!(decoded, Ok(true)), "the warm-up gives {decoded:?}");
    }

    #[test]
    #[ignore = "needs the path of a GGUF model file in TYPEALIKE_TEST_MODEL"]
    fn completion_after_the_warm_up_of_added_tokens_gives_the_same_words() {
        let mut llama = None;
        let model = load(
            &mut llama,
            Some(path_in("TYPEALIKE_TEST_MODEL")),
            Backend::Cpu,
            BATCH_TOKENS,
        )
        .expect("the model loads");
        let llama = llama.as_ref().expect("the load starts llama.cpp");
        let without_warm_up = PROMPTS.map(|prompt| {
            words_after_a_preparation(
                &mut LoadedModel::new(llama, &model, None, warm_up_setup()),
                prompt,
            )
        });

        let with_warm_up = PROMPTS.map(|prompt| {
            words_after_a_warm_up(
                &mut LoadedModel::new(llama, &model, None, warm_up_setup()),
                prompt,
            )
        });

        assert_eq!(with_warm_up, without_warm_up);
    }

    #[test]
    #[ignore = "needs the paths of a Gemma 4 model in TYPEALIKE_TEST_MODEL and of its drafter in \
                TYPEALIKE_TEST_DRAFTER"]
    fn completion_with_the_drafter_after_the_warm_up_of_added_tokens_gives_the_same_words() {
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
        let without_warm_up = PROMPTS.map(|prompt| {
            let mut loaded = LoadedModel::new(llama, &model, Some(&drafter), warm_up_setup());
            words_after_a_preparation(&mut loaded, prompt)
        });

        let with_warm_up = PROMPTS.map(|prompt| {
            let mut loaded = LoadedModel::new(llama, &model, Some(&drafter), warm_up_setup());
            words_after_a_warm_up(&mut loaded, prompt)
        });

        assert_eq!(with_warm_up, without_warm_up);
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
    #[ignore = "needs the path of a GGUF model file in TYPEALIKE_TEST_MODEL"]
    fn each_byte_of_a_healed_text_has_a_token_of_its_own() {
        let mut llama = None;
        let model = load(
            &mut llama,
            Some(path_in("TYPEALIKE_TEST_MODEL")),
            Backend::Cpu,
            BATCH_TOKENS,
        )
        .expect("the model loads");
        let one_byte_texts = model
            .tokens(false)
            .filter_map(|(_, piece)| <[u8; 1]>::try_from(piece).ok())
            .collect::<HashSet<_>>();

        let bytes_without_a_token = (0..=u8::MAX)
            .filter(|&byte| !one_byte_texts.contains(&[byte]))
            .collect::<Vec<_>>();

        assert_eq!(bytes_without_a_token, []);
    }

    #[test]
    #[ignore = "needs the paths of a Gemma 4 model in TYPEALIKE_TEST_MODEL and of its drafter in \
                TYPEALIKE_TEST_DRAFTER"]
    fn completions_with_the_drafter_give_the_texts_without_it() {
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
        let without_drafter = PROMPTS.map(|prompt| complete(&mut plain, prompt));
        let mut with_guesses = LoadedModel::new(llama, &model, Some(&drafter), setup());

        let with_drafter = PROMPTS.map(|prompt| complete(&mut with_guesses, prompt));

        assert_eq!(with_drafter, without_drafter);
    }
}
