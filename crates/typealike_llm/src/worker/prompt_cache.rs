//! The context that keeps the last prompt, and the decode loop that continues a copy of it.

use std::mem;

use llama_cpp_2::context::kv_cache::KvCacheConversionError;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::{BatchAddError, LlamaBatch};
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::speculative::MtpSpeculativeError;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::vocab::LlamaVocab;
use llama_cpp_2::{DecodeError, LlamaContextLoadError, LlamaStateSeqFlags, SeqState, StateSeqError};
use typealike_core::inference::{Correction, InferenceError, LogProbability, SentenceEnd, TypoCheck};

use super::correction::{TypoFix, guessed_letters, typo_fix, with_fix_gap};
use super::drafter::{DrafterError, GUESSED_TOKENS, ModelContext};
use super::generation::{
    AddedTokens, Choice, Decoder, Generation, Healing, Limits, Progress, Step, added_text, is_markup,
    last_word_start,
};
use super::logits::with_bias;
use super::model_params::{ContextSetup, context_params, drafter_context_params};
#[cfg(feature = "suggestion-log")]
use super::suggestion_log::Prefill;

/// The sequence that keeps the last prompt between completions.
const PROMPT_SEQUENCE: i32 = 1;
/// The sequence that continues a copy of the prompt. The drafter of llama.cpp reads only sequence 0.
const COMPLETION_SEQUENCE: i32 = 0;
/// The index that llama.cpp gives to the output of the last token of a decode.
const LAST_OUTPUT: i32 = -1;
/// The most saved states of the prompt sequence: one for each of the last words, so that a
/// backspace across each of these words goes back to a saved state.
const MAX_SNAPSHOTS: usize = 4;

#[derive(Debug, thiserror::Error)]
pub(super) enum GenerateError {
    #[error("the prompt has no tokens")]
    EmptyPrompt,
    #[error("the prompt and completion do not fit in the context")]
    PromptTooLong,
    #[error("failed to create a context")]
    Context(#[from] LlamaContextLoadError),
    #[error("failed to fill a batch")]
    Batch(#[from] BatchAddError),
    #[error("failed to decode a batch")]
    Decode(#[from] DecodeError),
    #[error("failed to change the cache of a sequence")]
    Sequence(#[from] KvCacheConversionError),
    #[error("failed to save the state of a sequence")]
    Snapshot(#[from] StateSeqError),
    #[error("the drafter failed")]
    Drafter(#[from] MtpSpeculativeError),
    #[error("a newer completion waits")]
    Superseded,
}

impl From<GenerateError> for InferenceError {
    fn from(error: GenerateError) -> Self {
        match error {
            GenerateError::PromptTooLong => Self::PromptTooLong,
            GenerateError::Superseded => Self::Superseded,
            GenerateError::EmptyPrompt
            | GenerateError::Context(_)
            | GenerateError::Batch(_)
            | GenerateError::Decode(_)
            | GenerateError::Sequence(_)
            | GenerateError::Snapshot(_)
            | GenerateError::Drafter(_) => Self::Failed,
        }
    }
}

/// A context that keeps the last prompt, so that a later prompt that starts with the same tokens
/// decodes only the tokens that it adds.
pub(super) struct PromptCache<'model> {
    context: ModelContext<'model>,
    /// The tokens in the prompt sequence.
    tokens: Vec<LlamaToken>,
    /// The output of the model after the last of `tokens`.
    logits: Vec<f32>,
    /// The output of the model after the start of `tokens`, before the last decode added the rest,
    /// while `tokens` still start with those tokens.
    earlier: Option<EarlierLogits>,
    /// The saved states of the prompt sequence, oldest first, for a recurrent model. Such a model
    /// keeps one running state, so it cannot remove only the end of a sequence. `None` for a model
    /// that can.
    snapshots: Option<Vec<Snapshot>>,
    /// The prompt decodes since the last take, for the suggestion log.
    #[cfg(feature = "suggestion-log")]
    prefill: Prefill,
}

/// The output of the model after the first `token_count` tokens of the prompt sequence.
struct EarlierLogits {
    token_count: usize,
    logits: Vec<f32>,
}

/// A saved state of the prompt sequence after its first `token_count` tokens.
struct Snapshot {
    token_count: usize,
    state: SeqState,
    logits: Vec<f32>,
}

impl<'model> PromptCache<'model> {
    pub(super) fn new(
        llama: &LlamaBackend,
        model: &'model LlamaModel,
        context_tokens: u32,
        setup: ContextSetup,
    ) -> Result<Self, LlamaContextLoadError> {
        let context = model.new_context(llama, context_params(context_tokens, setup))?;
        Ok(Self::of(model, ModelContext::Alone(context)))
    }

    /// Returns a cache as [`PromptCache::new`] does, whose completions also run `drafter`.
    pub(super) fn with_drafter(
        llama: &LlamaBackend,
        model: &'model LlamaModel,
        drafter: &'model LlamaModel,
        context_tokens: u32,
        setup: ContextSetup,
    ) -> Result<Self, DrafterError> {
        let context = model.new_context(llama, context_params(context_tokens, setup))?;
        let drafter_params = drafter_context_params(context_tokens, setup);
        let context = ModelContext::with_drafter(llama, context, drafter, drafter_params)?;
        Ok(Self::of(model, context))
    }

    fn of(model: &LlamaModel, context: ModelContext<'model>) -> Self {
        Self {
            context,
            tokens: Vec::new(),
            logits: Vec::new(),
            earlier: None,
            snapshots: (model.is_recurrent() || model.is_hybrid()).then(Vec::new),
            #[cfg(feature = "suggestion-log")]
            prefill: Prefill::default(),
        }
    }

    /// Makes the prompt sequence hold `prompt`. It first frees the cache of the last completion. It
    /// keeps the cached tokens that `prompt` starts with, and decodes the rest in batches. Before
    /// each batch, it stops when `is_outdated` tells so, and keeps the batches that it decoded.
    ///
    /// For a recurrent model, it saves the state before the token at `last_word_start`, so that a
    /// later prompt without the last word goes back to that state.
    pub(super) fn decode_prompt(
        &mut self,
        prompt: &[LlamaToken],
        last_word_start: usize,
        is_outdated: &mut dyn FnMut() -> bool,
    ) -> Result<(), GenerateError> {
        self.context
            .target_mut()
            .kv_cache_seq_rm(COMPLETION_SEQUENCE, None, None)?;
        self.keep_start_of(prompt);
        if self.snapshots.is_some() && self.tokens.len() <= last_word_start {
            self.decode_rest_of(&prompt[..last_word_start], is_outdated)?;
            self.save_snapshot()?;
        }
        self.decode_rest_of(prompt, is_outdated)
    }

    /// Decodes `prompt` and two added tokens as a completion does, and then forgets them. Thus the
    /// backend prepares the kernels and the memory of these decodes before the first completion,
    /// also the guess of a drafter and its check. Before each decode, it stops when `is_outdated`
    /// tells so.
    pub(super) fn warm_up(
        &mut self,
        prompt: &[LlamaToken],
        is_outdated: &mut dyn FnMut() -> bool,
    ) -> Result<(), GenerateError> {
        let decoded = self.decode_as_completion(prompt, is_outdated);
        self.forget();
        decoded
    }

    fn decode_as_completion(
        &mut self,
        prompt: &[LlamaToken],
        is_outdated: &mut dyn FnMut() -> bool,
    ) -> Result<(), GenerateError> {
        if prompt.is_empty() {
            return Ok(());
        }
        self.decode_prompt(prompt, prompt.len() - 1, is_outdated)?;
        if is_outdated() {
            return Err(GenerateError::Superseded);
        }
        self.start_completion()?;
        let mut batch = LlamaBatch::new(1 + GUESSED_TOKENS, 1);
        for (offset, &added_token) in prompt.iter().take(1 + GUESSED_TOKENS).enumerate() {
            let token_position = position(prompt.len() + offset);
            self.context
                .decode_next(&mut batch, added_token, token_position, COMPLETION_SEQUENCE)?;
        }
        Ok(())
    }

    /// Makes the completion sequence a copy of the prompt sequence, for a new completion.
    fn start_completion(&mut self) -> Result<(), GenerateError> {
        let context = self.context.target_mut();
        context.kv_cache_seq_rm(COMPLETION_SEQUENCE, None, None)?;
        context.copy_kv_cache_seq(PROMPT_SEQUENCE, COMPLETION_SEQUENCE, None, None)?;
        self.context.start_completion()?;
        Ok(())
    }

    /// Removes all the tokens and the saved states.
    fn forget(&mut self) {
        self.context.target_mut().clear_kv_cache();
        self.tokens.clear();
        self.logits.clear();
        self.earlier = None;
        if let Some(snapshots) = &mut self.snapshots {
            snapshots.clear();
        }
        #[cfg(feature = "suggestion-log")]
        {
            self.prefill = Prefill::default();
        }
    }

    /// Returns the prompt decodes since the last take, and starts a new count.
    #[cfg(feature = "suggestion-log")]
    pub(super) fn take_prefill(&mut self) -> Prefill {
        std::mem::take(&mut self.prefill)
    }

    /// Decodes the tokens of `prompt` after the cached tokens in batches of the context.
    fn decode_rest_of(
        &mut self,
        prompt: &[LlamaToken],
        is_outdated: &mut dyn FnMut() -> bool,
    ) -> Result<(), GenerateError> {
        let batch_size = usize::try_from(self.context.target().n_batch()).expect("a u32 fits in usize");
        #[cfg(feature = "suggestion-log")]
        self.prefill.start_prompt(prompt.len());
        for batch_tokens in prompt[self.tokens.len()..].chunks(batch_size) {
            if is_outdated() {
                return Err(GenerateError::Superseded);
            }
            let mut batch = LlamaBatch::new(batch_tokens.len(), 1);
            for (offset, &token) in batch_tokens.iter().enumerate() {
                let is_last = offset + 1 == batch_tokens.len();
                batch.add(
                    token,
                    position(self.tokens.len() + offset),
                    &[PROMPT_SEQUENCE],
                    is_last,
                )?;
            }
            #[cfg(feature = "suggestion-log")]
            let decode_started = std::time::Instant::now();
            self.context.target_mut().decode(&mut batch)?;
            if !self.logits.is_empty() {
                self.earlier = Some(EarlierLogits {
                    token_count: self.tokens.len(),
                    logits: mem::take(&mut self.logits),
                });
            }
            self.tokens.extend_from_slice(batch_tokens);
            self.logits.clear();
            self.logits
                .extend_from_slice(self.context.target().get_logits_ith(LAST_OUTPUT));
            #[cfg(feature = "suggestion-log")]
            self.prefill
                .add_batch(batch_tokens.len(), decode_started.elapsed());
        }
        Ok(())
    }

    /// Keeps the cached tokens that `prompt` starts with. A recurrent model cannot remove only the
    /// end of a sequence, so it goes back to the last saved state that `prompt` starts with, or
    /// keeps no tokens.
    fn keep_start_of(&mut self, prompt: &[LlamaToken]) {
        let count = kept_tokens(&self.tokens, prompt);
        if count == self.tokens.len() {
            return;
        }
        let start = u32::try_from(count).expect("a prompt position fits in u32");
        if self
            .context
            .target_mut()
            .kv_cache_seq_rm(PROMPT_SEQUENCE, Some(start), None)
            .is_ok()
        {
            self.tokens.truncate(count);
        } else if !self.restore_snapshot(common_tokens(&self.tokens, prompt)) {
            self.context.target_mut().clear_kv_cache();
            self.tokens.clear();
        }
        if let Some(snapshots) = &mut self.snapshots {
            snapshots.retain(|snapshot| snapshot.token_count <= self.tokens.len());
        }
        if self
            .earlier
            .as_ref()
            .is_some_and(|earlier| earlier.token_count > self.tokens.len())
        {
            self.earlier = None;
        }
    }

    /// Returns the output of the model after `prefix` without a decode, when the cached tokens
    /// start with `prefix` and the cache kept that output.
    fn logits_after(&self, prefix: &[LlamaToken]) -> Option<&[f32]> {
        if self.tokens == prefix {
            return Some(&self.logits);
        }
        self.earlier
            .as_ref()
            .filter(|earlier| earlier.token_count == prefix.len() && self.tokens.starts_with(prefix))
            .map(|earlier| earlier.logits.as_slice())
    }

    /// Saves the state of the prompt sequence for a recurrent model, when it has no saved state for
    /// it yet. It forgets the oldest saved state when it has too many.
    fn save_snapshot(&mut self) -> Result<(), StateSeqError> {
        let Some(snapshots) = &mut self.snapshots else {
            return Ok(());
        };
        if snapshots
            .last()
            .is_some_and(|last| last.token_count == self.tokens.len())
        {
            return Ok(());
        }
        let state = self
            .context
            .target_mut()
            .state_seq_get(PROMPT_SEQUENCE, LlamaStateSeqFlags::PARTIAL_ONLY)?;
        if snapshots.len() == MAX_SNAPSHOTS {
            snapshots.remove(0);
        }
        snapshots.push(Snapshot {
            token_count: self.tokens.len(),
            state,
            logits: self.logits.clone(),
        });
        Ok(())
    }

    /// Makes the prompt sequence go back to the last saved state with no more than the first
    /// `common` tokens. Returns whether it went back.
    fn restore_snapshot(&mut self, common: usize) -> bool {
        let Some(snapshot) = self
            .snapshots
            .iter()
            .flatten()
            .rev()
            .find(|snapshot| snapshot.token_count <= common)
        else {
            return false;
        };
        let start = u32::try_from(snapshot.token_count).expect("a prompt position fits in u32");
        let is_restored = self
            .context
            .target_mut()
            .state_seq_set(&snapshot.state, PROMPT_SEQUENCE)
            .is_ok()
            && self
                .context
                .target_mut()
                .kv_cache_seq_rm(PROMPT_SEQUENCE, Some(start), None)
                .is_ok();
        if is_restored {
            self.tokens.truncate(snapshot.token_count);
            self.logits.clone_from(&snapshot.logits);
        }
        is_restored
    }

    /// Continues a copy of the prompt sequence greedily, so that the prompt sequence stays as it is
    /// for the next completion. Its first added tokens write the `healing` text again. It stops at
    /// the end of the text, at markup, and where each [`Generation`] step stops. After the end of a
    /// sentence that it goes on past, and after a typed space before its first text, it takes the
    /// most likely token that keeps the text on its line, or starts again with another first token
    /// ([`Choice::StartWith`]). Before each decode, it stops when `progress` tells so, and else
    /// sends the text that it added so far.
    pub(super) fn continue_prompt(
        &mut self,
        decoder: &Decoder<'_>,
        healing: Healing<'_>,
        progress: &mut Progress<'_>,
    ) -> Result<AddedTokens, GenerateError> {
        self.start_completion()?;
        let Decoder { vocab, limits, .. } = *decoder;
        let mut batch = LlamaBatch::new(1 + GUESSED_TOKENS, 1);
        let mut logits = self.logits.as_slice();
        let mut biased = Vec::new();
        let mut generation = Generation::new(decoder.pieces, healing);
        while generation.has_room(limits) {
            if generation.add_healing_step(logits, limits.typo_check.min_log_probability()) == Step::Stop {
                return Ok(generation.into_typo_stop());
            }
            generation.add_join_step(logits, limits.line_after);
            let bias = decoder.token_bias.at(generation.tokens().len());
            let scores = with_bias(logits, bias, &mut biased);
            let (token, log_probability) =
                match generation.choose_next_token(scores, logits, limits, |token| text_of(vocab, token)) {
                    Choice::Add(token, log_probability) => (token, log_probability),
                    Choice::StartWith(first_token) => {
                        let healing = Healing {
                            first_token: Some(first_token),
                            ..healing
                        };
                        return self.continue_prompt(decoder, healing, progress);
                    }
                };
            let Some(piece) = text_of(vocab, token) else {
                generation.add_end_step(logits);
                break;
            };
            if generation.add_token(token, &piece, log_probability, logits, limits) == Step::Stop {
                break;
            }
            if (progress.is_outdated)() {
                return Err(GenerateError::Superseded);
            }
            (progress.send_partial)(added_text(generation.shown_tokens(), healing.text, |token| {
                vocab.token_to_piece(token, false, None)
            }));
            let token_position = position(self.tokens.len() + generation.tokens().len() - 1);
            logits = self
                .context
                .decode_next(&mut batch, token, token_position, COMPLETION_SEQUENCE)?;
        }
        Ok(generation.into_added())
    }

    /// Continues the prompt after the `healing` text, as [`PromptCache::continue_prompt`] does. When
    /// the limits of the `decoder` ask for fixes, the typed letters must also be not much less
    /// likely than their best fix ([`with_fix_gap`]). When the completion stops for a typo and a
    /// fix comes, the added tokens continue the fixed letters.
    pub(super) fn continue_healed(
        &mut self,
        decoder: &Decoder<'_>,
        healing: Healing<'_>,
        progress: &mut Progress<'_>,
    ) -> Result<AddedTokens, GenerateError> {
        let fix = match self.typo_fix(decoder, healing) {
            Some(fix) => Some(fix),
            None => self.guess_fix(decoder, healing, progress)?,
        };
        let decoder = Decoder {
            limits: Limits {
                typo_check: with_fix_gap(decoder.limits.typo_check, healing.text, fix.as_ref()),
                ..decoder.limits
            },
            ..*decoder
        };
        let added = self.continue_prompt(&decoder, healing, progress)?;
        self.continue_fixed(&decoder, healing, added, fix, progress)
    }

    /// Returns the most likely fix with one edit of the `typed` letters after the prompt, without
    /// the minimum of a fix, for the suggestion log.
    #[cfg(feature = "suggestion-log")]
    pub(super) fn best_fix_candidate(&self, decoder: &Decoder<'_>, typed: &[u8]) -> Option<TypoFix> {
        typo_fix(
            &self.logits,
            decoder.pieces,
            typed,
            LogProbability(f32::NEG_INFINITY),
            |token| decoder.vocab.token_to_piece(token, false, None),
        )
    }

    /// Returns the best fix of the typed letters of the `healing` text after the prompt, when the
    /// limits of the `decoder` ask for fixes.
    fn typo_fix(&self, decoder: &Decoder<'_>, healing: Healing<'_>) -> Option<TypoFix> {
        let TypoCheck::Fix(min) = decoder.limits.typo_check else {
            return None;
        };
        typo_fix(&self.logits, decoder.pieces, healing.text, min, |token| {
            decoder.vocab.token_to_piece(token, false, None)
        })
    }

    /// Returns the fix of the typed letters of the `healing` text with a guess of the dictionary in
    /// the limits of the `decoder` ([`guessed_letters`]), when they ask for fixes: the guess whose
    /// letters the model finds most likely as a completion writes them again, when they are at
    /// least as likely as the minimum. Each guess costs the decodes of its tokens.
    fn guess_fix(
        &mut self,
        decoder: &Decoder<'_>,
        healing: Healing<'_>,
        progress: &mut Progress<'_>,
    ) -> Result<Option<TypoFix>, GenerateError> {
        let TypoCheck::Fix(min) = decoder.limits.typo_check else {
            return Ok(None);
        };
        let mut best: Option<TypoFix> = None;
        for letters in guessed_letters(healing.text, decoder.limits.guesses) {
            let written = Healing {
                text: letters.as_bytes(),
                first_token: None,
            };
            let Some((log_probability, _)) = self.heal(decoder, written, None, progress)? else {
                continue;
            };
            let is_better = log_probability >= min
                && best
                    .as_ref()
                    .is_none_or(|best| log_probability > best.log_probability);
            if is_better {
                best = Some(TypoFix {
                    token: None,
                    letters,
                    log_probability,
                });
            }
        }
        Ok(best)
    }

    /// Continues the prompt again with `fix` when `added` stopped for a typo in the `healing` text:
    /// the first step takes the token of the fix, and the added tokens continue the fixed letters.
    /// Their parts do not go to `progress`, because they do not continue the prompt. Returns
    /// `added` when it did not stop for a typo, the limits of the `decoder` ask for no fix, or no
    /// fix comes.
    fn continue_fixed(
        &mut self,
        decoder: &Decoder<'_>,
        healing: Healing<'_>,
        added: AddedTokens,
        fix: Option<TypoFix>,
        progress: &mut Progress<'_>,
    ) -> Result<AddedTokens, GenerateError> {
        let TypoCheck::Fix(min) = decoder.limits.typo_check else {
            return Ok(added);
        };
        let has_stopped_for_typo = added.tokens.is_empty() && added.typed_log_probability < min;
        let Some(fix) = fix.filter(|_| has_stopped_for_typo) else {
            return Ok(added);
        };
        self.continue_from_fix(decoder, healing.text, fix, added.typed_log_probability, progress)
    }

    /// Continues the prompt from the `fix` of the `typed` letters, as [`PromptCache::continue_fixed`]
    /// does. `typed_log_probability` tells how likely the model found the typed letters.
    fn continue_from_fix(
        &mut self,
        decoder: &Decoder<'_>,
        typed: &[u8],
        fix: TypoFix,
        typed_log_probability: LogProbability,
        progress: &mut Progress<'_>,
    ) -> Result<AddedTokens, GenerateError> {
        let fixed_healing = Healing {
            text: fix.letters.as_bytes(),
            first_token: fix.token,
        };
        let mut quiet = Progress {
            is_outdated: &mut *progress.is_outdated,
            send_partial: &mut |_| {},
        };
        let fixed_decoder = Decoder {
            limits: Limits {
                typo_check: TypoCheck::Off,
                finished_word_fix: None,
                ..decoder.limits
            },
            ..*decoder
        };
        let fixed_added = self.continue_prompt(&fixed_decoder, fixed_healing, &mut quiet)?;
        Ok(AddedTokens {
            typed_log_probability,
            correction: Some(Correction {
                typed: String::from_utf8_lossy(typed).into_owned(),
                fixed: fix.letters,
            }),
            ..fixed_added
        })
    }

    /// Continues the prompt after the `finished` word from the fix of that word, when it holds a
    /// typo with a fix, as the request of the word alone with [`TypoCheck::Fix`] and `min` finds
    /// them. The fixed letters then end with the typed text after the word. Returns `None`
    /// otherwise, so that the completion continues the prompt with the word as without this check.
    pub(super) fn continue_fixed_word(
        &mut self,
        decoder: &Decoder<'_>,
        min: LogProbability,
        finished: FinishedWord<'_>,
        progress: &mut Progress<'_>,
    ) -> Result<Option<AddedTokens>, GenerateError> {
        let FinishedWord { before, word, after } = finished;
        let word_start = last_word_start(decoder.vocab, before);
        let next_start = after.len() - after.trim_ascii_start().len();
        let (space, next_letters) = after.split_at(next_start);
        let typed_word = [word, space].concat();
        let fix_after = |logits: &[f32]| {
            typo_fix(logits, decoder.pieces, &typed_word, min, |token| {
                decoder.vocab.token_to_piece(token, false, None)
            })
        };
        let fix = if let Some(logits) = self.logits_after(before) {
            fix_after(logits)
        } else {
            self.decode_prompt(before, word_start, progress.is_outdated)?;
            fix_after(&self.logits)
        };
        let Some(mut fix) = fix else {
            return Ok(None);
        };
        self.decode_prompt(before, word_start, progress.is_outdated)?;
        let check = with_fix_gap(TypoCheck::Fix(min), word, Some(&fix));
        let word_healing = Healing {
            text: word,
            first_token: None,
        };
        let Some(typed_log_probability) = check
            .min_log_probability()
            .map(|min| self.typo_stop(decoder, word_healing, min, progress))
            .transpose()?
            .flatten()
        else {
            return Ok(None);
        };
        fix.letters.push_str(&String::from_utf8_lossy(next_letters));
        let typed = [word, after].concat();
        self.continue_from_fix(decoder, &typed, fix, typed_log_probability, progress)
            .map(Some)
    }

    /// Writes the `healing` text after the prompt as a completion does, without the text after it.
    /// Returns how likely the model found the healed text when a healing step stops for a typo,
    /// because the text got less likely than `min`, else `None`.
    fn typo_stop(
        &mut self,
        decoder: &Decoder<'_>,
        healing: Healing<'_>,
        min: LogProbability,
        progress: &mut Progress<'_>,
    ) -> Result<Option<LogProbability>, GenerateError> {
        let healed = self.heal(decoder, healing, Some(min), progress)?;
        Ok(healed.and_then(|(log_probability, step)| (step == Step::Stop).then_some(log_probability)))
    }

    /// Writes the `healing` text after the prompt as a completion does, without the text after it.
    /// Returns how likely the model found the healed text, with [`Step::Stop`] when a healing step
    /// stopped for a typo because the text got less likely than `min`, or with [`Step::Continue`]
    /// when the tokens wrote all of it. `None` when the completion leaves the healed text first.
    fn heal(
        &mut self,
        decoder: &Decoder<'_>,
        healing: Healing<'_>,
        min: Option<LogProbability>,
        progress: &mut Progress<'_>,
    ) -> Result<Option<(LogProbability, Step)>, GenerateError> {
        self.start_completion()?;
        let vocab = decoder.vocab;
        let limits = Limits {
            max_added: healing.text.len() + 1,
            min_log_probability: None,
            typo_check: TypoCheck::Off,
            finished_word_fix: None,
            sentence_end: SentenceEnd::Continue,
            line_after: b"",
            guesses: &[],
        };
        let mut batch = LlamaBatch::new(1 + GUESSED_TOKENS, 1);
        let mut logits = self.logits.as_slice();
        let mut biased = Vec::new();
        let mut generation = Generation::new(decoder.pieces, healing);
        loop {
            if generation.add_healing_step(logits, min) == Step::Stop {
                return Ok(Some((generation.typed_log_probability(), Step::Stop)));
            }
            let bias = decoder.token_bias.at(generation.tokens().len());
            let scores = with_bias(logits, bias, &mut biased);
            let Choice::Add(token, log_probability) =
                generation.choose_next_token(scores, logits, limits, |token| text_of(vocab, token))
            else {
                return Ok(None);
            };
            let Some(piece) = text_of(vocab, token) else {
                return Ok(None);
            };
            let step = generation.add_token(token, &piece, log_probability, logits, limits);
            if generation.is_healed() {
                return Ok(Some((generation.typed_log_probability(), Step::Continue)));
            }
            if step == Step::Stop {
                return Ok(None);
            }
            if (progress.is_outdated)() {
                return Err(GenerateError::Superseded);
            }
            let token_position = position(self.tokens.len() + generation.tokens().len() - 1);
            logits = self
                .context
                .decode_next(&mut batch, token, token_position, COMPLETION_SEQUENCE)?;
        }
    }
}

/// The last finished word of a prompt: the prompt without it, its text, and the typed text after
/// it, which starts with a space.
#[derive(Clone, Copy)]
pub(super) struct FinishedWord<'text> {
    pub(super) before: &'text [LlamaToken],
    pub(super) word: &'text [u8],
    pub(super) after: &'text [u8],
}

/// Returns the text of `token`, or `None` when it ends the text or is markup.
fn text_of(vocab: &LlamaVocab<'_>, token: LlamaToken) -> Option<Vec<u8>> {
    let ends_text = vocab.is_eog(token) || is_markup(vocab.attr(token));
    (!ends_text).then(|| vocab.token_to_piece(token, false, None))
}

/// Returns how many cached tokens a new `prompt` keeps: the tokens that both start with. It keeps
/// one less when `prompt` ends inside the cached tokens, because the output after its last token
/// must be computed again.
fn kept_tokens(cached: &[LlamaToken], prompt: &[LlamaToken]) -> usize {
    let common = common_tokens(cached, prompt);
    if common == prompt.len() && common < cached.len() {
        common.saturating_sub(1)
    } else {
        common
    }
}

/// Returns how many tokens `cached` and `prompt` both start with.
fn common_tokens(cached: &[LlamaToken], prompt: &[LlamaToken]) -> usize {
    cached
        .iter()
        .zip(prompt)
        .take_while(|(cached, prompt)| cached == prompt)
        .count()
}

/// Returns the position of the token at `index` in a sequence.
fn position(index: usize) -> i32 {
    i32::try_from(index).expect("the prompt fits in the context")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(ids: &[i32]) -> Vec<LlamaToken> {
        ids.iter().copied().map(LlamaToken).collect()
    }

    #[test]
    fn prompt_keeps_the_cached_tokens_it_starts_with() {
        let cases = [
            (&[1, 2, 3][..], &[1, 2, 3][..], 3),
            (&[1, 2], &[1, 2, 3], 2),
            (&[1, 2, 3], &[1, 4], 1),
            (&[], &[1], 0),
            (&[1, 2], &[3], 0),
        ];

        let kept = cases.map(|(cached, prompt, _)| kept_tokens(&tokens(cached), &tokens(prompt)));

        assert_eq!(kept, cases.map(|(_, _, kept)| kept));
    }

    #[test]
    fn prompt_that_ends_inside_the_cached_tokens_decodes_its_last_token_again() {
        let kept = kept_tokens(&tokens(&[1, 2, 3]), &tokens(&[1, 2]));

        assert_eq!(kept, 1);
    }
}
