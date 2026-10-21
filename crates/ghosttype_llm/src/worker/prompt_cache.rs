//! The context that keeps the last prompt, and the decode loop that continues a copy of it.

use ghosttype_core::inference::{Correction, InferenceError};
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::kv_cache::KvCacheConversionError;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::{BatchAddError, LlamaBatch};
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::vocab::LlamaVocab;
use llama_cpp_2::{DecodeError, LlamaContextLoadError, LlamaStateSeqFlags, SeqState, StateSeqError};

use super::correction::{TypedLetters, typo_fix};
use super::generation::{
    AddedTokens, Healing, Limits, Progress, added_after, added_text, after_prefix, is_markup,
    writes_line_break,
};
use super::logits::{TokenBias, agreeing_log_probability, choose_token, with_bias};
use super::model_params::{BATCH_TOKENS, CpuThreads, context_params};
use super::words::{WordLogProbabilities, ends_sentence, starts_next_sentence};

/// The sequence that keeps the last prompt between completions.
const PROMPT_SEQUENCE: i32 = 0;
/// The sequence that continues a copy of the prompt.
const COMPLETION_SEQUENCE: i32 = 1;
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
    #[error("a newer completion waits")]
    Superseded,
}

impl GenerateError {
    pub(super) fn to_inference_error(&self) -> InferenceError {
        match self {
            Self::PromptTooLong => InferenceError::PromptTooLong,
            Self::Superseded => InferenceError::Superseded,
            Self::EmptyPrompt
            | Self::Context(_)
            | Self::Batch(_)
            | Self::Decode(_)
            | Self::Sequence(_)
            | Self::Snapshot(_) => InferenceError::Failed,
        }
    }
}

/// A context that keeps the last prompt, so that a later prompt that starts with the same tokens
/// decodes only the tokens that it adds.
pub(super) struct PromptCache<'model> {
    context: LlamaContext<'model>,
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
        cpu_threads: Option<CpuThreads>,
    ) -> Result<Self, LlamaContextLoadError> {
        Ok(Self {
            context: model.new_context(llama, context_params(context_tokens, cpu_threads))?,
            tokens: Vec::new(),
            logits: Vec::new(),
            snapshots: (model.is_recurrent() || model.is_hybrid()).then(Vec::new),
        })
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
        self.context.kv_cache_seq_rm(COMPLETION_SEQUENCE, None, None)?;
        self.keep_start_of(prompt);
        if self.snapshots.is_some() && self.tokens.len() <= last_word_start {
            self.decode_rest_of(&prompt[..last_word_start], is_outdated)?;
            self.save_snapshot()?;
        }
        self.decode_rest_of(prompt, is_outdated)
    }

    /// Decodes the tokens of `prompt` after the cached tokens in batches.
    fn decode_rest_of(
        &mut self,
        prompt: &[LlamaToken],
        is_outdated: &mut dyn FnMut() -> bool,
    ) -> Result<(), GenerateError> {
        let batch_size = usize::try_from(BATCH_TOKENS).expect("a u32 fits in usize");
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
            self.context.decode(&mut batch)?;
            self.tokens.extend_from_slice(batch_tokens);
            self.logits.clear();
            self.logits
                .extend_from_slice(self.context.get_logits_ith(LAST_OUTPUT));
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
            .kv_cache_seq_rm(PROMPT_SEQUENCE, Some(start), None)
            .is_ok()
        {
            self.tokens.truncate(count);
        } else if !self.restore_snapshot(common_tokens(&self.tokens, prompt)) {
            self.context.clear_kv_cache();
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
            .state_seq_set(&snapshot.state, PROMPT_SEQUENCE)
            .is_ok()
            && self
                .context
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
    /// the end of a sentence and at the `limits`. It leaves out the word that the minimum
    /// probability cuts, and the last word at the token limit unless that word ends a sentence,
    /// because the limit can cut it. It never leaves out the only word. It adds no tokens when the
    /// healing text is less likely than the minimum for typed letters. Before each decode, it
    /// stops when `progress` tells so, and else sends the text that it added so far.
    pub(super) fn continue_prompt(
        &mut self,
        vocab: &LlamaVocab<'_>,
        healing: &Healing<'_>,
        limits: Limits,
        token_bias: &TokenBias,
        progress: &mut Progress<'_>,
    ) -> Result<AddedTokens, GenerateError> {
        self.context.kv_cache_seq_rm(COMPLETION_SEQUENCE, None, None)?;
        self.context
            .copy_kv_cache_seq(PROMPT_SEQUENCE, COMPLETION_SEQUENCE, None, None)?;
        let mut batch = LlamaBatch::new(1, 1);
        let mut logits = self.logits.as_slice();
        let mut biased = Vec::new();
        let mut unwritten = healing.text;
        let mut added = Vec::new();
        let mut written = Vec::new();
        let mut words = WordLogProbabilities::default();
        let mut typed = TypedLetters::default();
        while added.len() < limits.max_added {
            if !unwritten.is_empty() {
                typed.add(logits, healing.pieces, unwritten);
                if limits
                    .min_typed_log_probability
                    .is_some_and(|min| typed.log_probability() < min)
                {
                    return Ok(AddedTokens {
                        tokens: Vec::new(),
                        word_log_probabilities: Vec::new(),
                        typed_log_probability: typed.log_probability(),
                        correction: None,
                    });
                }
            }
            let scores = with_bias(logits, token_bias.at(added.len()), &mut biased);
            let (token, log_probability) = match healing.first_token.filter(|_| added.is_empty()) {
                Some(token) => (
                    token,
                    agreeing_log_probability(logits, token, healing.pieces.agreeing_with(unwritten)),
                ),
                None => choose_token(scores, logits, healing.pieces, unwritten),
            };
            if vocab.is_eog(token) || is_markup(vocab.attr(token)) {
                break;
            }
            let piece = vocab.token_to_piece(token, false, None);
            let text = added_after(&piece, unwritten);
            if starts_next_sentence(healing.text, &written, text) {
                break;
            }
            let is_cut = limits
                .min_log_probability
                .is_some_and(|cut| !words.keeps(text, log_probability, cut));
            if is_cut {
                let dropped = words.end_before(text);
                added.truncate(added.len().saturating_sub(dropped));
                break;
            }
            let ends_line = writes_line_break(&piece, unwritten);
            words.add(text, log_probability);
            written.extend_from_slice(text);
            unwritten = after_prefix(unwritten, &piece);
            added.push(token);
            if ends_line {
                break;
            }
            if added.len() == limits.max_added {
                if !ends_sentence(healing.text, &written) {
                    let dropped = words.drop_last_word();
                    added.truncate(added.len().saturating_sub(dropped));
                }
                break;
            }
            if (progress.is_outdated)() {
                return Err(GenerateError::Superseded);
            }
            (progress.send_partial)(added_text(vocab, &self.tokens, &added, healing.text));
            batch.clear();
            batch.add(
                token,
                position(self.tokens.len() + added.len() - 1),
                &[COMPLETION_SEQUENCE],
                true,
            )?;
            self.context.decode(&mut batch)?;
            logits = self.context.get_logits_ith(LAST_OUTPUT);
        }
        Ok(AddedTokens {
            tokens: added,
            word_log_probabilities: words.into_vec(),
            typed_log_probability: typed.log_probability(),
            correction: None,
        })
    }
}

impl PromptCache<'_> {
    /// Continues the prompt again when `added` stopped for a typo in the `healing` text and a fix
    /// comes: the first step takes the token of the fix, and the added tokens continue the fixed
    /// letters. Their parts do not go to `progress`, because they do not continue the prompt.
    /// Returns `added` when it did not stop for a typo, the `limits` ask for no fix, or no fix
    /// comes.
    pub(super) fn continue_fixed(
        &mut self,
        vocab: &LlamaVocab<'_>,
        healing: &Healing<'_>,
        added: AddedTokens,
        limits: Limits,
        token_bias: &TokenBias,
        progress: &mut Progress<'_>,
    ) -> Result<AddedTokens, GenerateError> {
        let Some(min) = limits.min_typed_log_probability.filter(|&min| {
            limits.fixes_typos && added.tokens.is_empty() && added.typed_log_probability < min
        }) else {
            return Ok(added);
        };
        let Some((token, fixed)) = typo_fix(&self.logits, healing.pieces, healing.text, min, |token| {
            vocab.token_to_piece(token, false, None)
        }) else {
            return Ok(added);
        };
        let fixed_healing = Healing {
            pieces: healing.pieces,
            text: fixed.as_bytes(),
            first_token: Some(token),
        };
        let mut quiet = Progress {
            is_outdated: &mut *progress.is_outdated,
            send_partial: &mut |_| {},
        };
        let limits = Limits {
            min_typed_log_probability: None,
            fixes_typos: false,
            ..limits
        };
        let fixed_added = self.continue_prompt(vocab, &fixed_healing, limits, token_bias, &mut quiet)?;
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
