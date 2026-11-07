//! The context that keeps the last prompt, and the decode loop that continues a copy of it.

use llama_cpp_2::context::kv_cache::KvCacheConversionError;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::{BatchAddError, LlamaBatch};
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::speculative::MtpSpeculativeError;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::vocab::LlamaVocab;
use llama_cpp_2::{DecodeError, LlamaContextLoadError, LlamaStateSeqFlags, SeqState, StateSeqError};
use typealike_core::inference::{Correction, InferenceError, TypoCheck};

use super::correction::typo_fix;
use super::drafter::{DrafterError, GUESSED_TOKENS, ModelContext};
use super::generation::{
    AddedTokens, Decoder, Generation, Healing, Limits, Progress, Step, added_text, is_markup,
};
use super::logits::{log_probability_of, most_likely_tokens, with_bias};
use super::model_params::{ContextSetup, context_params, drafter_context_params};

/// The sequence that keeps the last prompt between completions.
const PROMPT_SEQUENCE: i32 = 1;
/// The sequence that continues a copy of the prompt. The drafter of llama.cpp reads only sequence 0.
const COMPLETION_SEQUENCE: i32 = 0;
/// The index that llama.cpp gives to the output of the last token of a decode.
const LAST_OUTPUT: i32 = -1;
/// The most saved states of the prompt sequence: one for each of the last words, so that a
/// backspace across each of these words goes back to a saved state.
const MAX_SNAPSHOTS: usize = 4;
/// The most likely tokens where a completion looks for a token that keeps its text on the line,
/// after the end of a sentence that it goes on past.
const SAME_LINE_CANDIDATES: usize = 64;

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
    /// The saved states of the prompt sequence, oldest first, for a recurrent model. Such a model
    /// keeps one running state, so it cannot remove only the end of a sequence. `None` for a model
    /// that can.
    snapshots: Option<Vec<Snapshot>>,
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
            snapshots: (model.is_recurrent() || model.is_hybrid()).then(Vec::new),
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
        if let Some(snapshots) = &mut self.snapshots {
            snapshots.clear();
        }
    }

    /// Decodes the tokens of `prompt` after the cached tokens in batches of the context.
    fn decode_rest_of(
        &mut self,
        prompt: &[LlamaToken],
        is_outdated: &mut dyn FnMut() -> bool,
    ) -> Result<(), GenerateError> {
        let batch_size = usize::try_from(self.context.target().n_batch()).expect("a u32 fits in usize");
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
            self.context.target_mut().decode(&mut batch)?;
            self.tokens.extend_from_slice(batch_tokens);
            self.logits.clear();
            self.logits
                .extend_from_slice(self.context.target().get_logits_ith(LAST_OUTPUT));
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
    /// sentence that it goes on past, it takes the most likely token that keeps the text on its
    /// line. Before each
    /// decode, it stops when `progress` tells so, and else sends the text that it added so far.
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
            let chosen = generation.choose_next_token(scores, logits);
            let (token, log_probability) =
                if generation.continues_past_sentence_end(limits) && ends_text_or_line(vocab, chosen.0) {
                    same_line_token(vocab, scores, logits).unwrap_or(chosen)
                } else {
                    chosen
                };
            if vocab.is_eog(token) || is_markup(vocab.attr(token)) {
                generation.add_end_step(logits);
                break;
            }
            let piece = vocab.token_to_piece(token, false, None);
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

    /// Continues the prompt again when `added` stopped for a typo in the `healing` text and a fix
    /// comes: the first step takes the token of the fix, and the added tokens continue the fixed
    /// letters. Their parts do not go to `progress`, because they do not continue the prompt.
    /// Returns `added` when it did not stop for a typo, the limits of the `decoder` ask for no fix,
    /// or no fix comes.
    pub(super) fn continue_fixed(
        &mut self,
        decoder: &Decoder<'_>,
        healing: Healing<'_>,
        added: AddedTokens,
        progress: &mut Progress<'_>,
    ) -> Result<AddedTokens, GenerateError> {
        let limits = decoder.limits;
        let TypoCheck::Fix(min) = limits.typo_check else {
            return Ok(added);
        };
        let has_stopped_for_typo = added.tokens.is_empty() && added.typed_log_probability < min;
        if !has_stopped_for_typo {
            return Ok(added);
        }
        let Some((token, fixed)) = typo_fix(&self.logits, decoder.pieces, healing.text, min, |token| {
            decoder.vocab.token_to_piece(token, false, None)
        }) else {
            return Ok(added);
        };
        let fixed_healing = Healing {
            text: fixed.as_bytes(),
            first_token: Some(token),
        };
        let mut quiet = Progress {
            is_outdated: &mut *progress.is_outdated,
            send_partial: &mut |_| {},
        };
        let fixed_decoder = Decoder {
            limits: Limits {
                typo_check: TypoCheck::Off,
                ..limits
            },
            ..*decoder
        };
        let fixed_added = self.continue_prompt(&fixed_decoder, fixed_healing, &mut quiet)?;
        Ok(AddedTokens {
            typed_log_probability: added.typed_log_probability,
            correction: Some(Correction {
                typed: String::from_utf8_lossy(healing.text).into_owned(),
                fixed,
            }),
            ..fixed_added
        })
    }
}

/// Whether `token` ends the added text or its line: the end of the text, markup or a line break.
fn ends_text_or_line(vocab: &LlamaVocab<'_>, token: LlamaToken) -> bool {
    vocab.is_eog(token)
        || is_markup(vocab.attr(token))
        || vocab.token_to_piece(token, false, None).contains(&b'\n')
}

/// Returns the most likely token of `scores` that keeps the text on its line, among the
/// [`SAME_LINE_CANDIDATES`], and the natural log of its probability in `logits`.
fn same_line_token(vocab: &LlamaVocab<'_>, scores: &[f32], logits: &[f32]) -> Option<(LlamaToken, f32)> {
    most_likely_tokens(scores, SAME_LINE_CANDIDATES)
        .into_iter()
        .find(|&token| !ends_text_or_line(vocab, token))
        .map(|token| (token, log_probability_of(logits, token)))
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
