//! The drafter of a model: a small model that guesses the next token, so that one decode of the
//! model checks the guess and can add two tokens. The model still chooses each added token.

use llama_cpp_2::LlamaContextLoadError;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::speculative::{MtpSpeculative, MtpSpeculativeError, MtpSpeculativeParams};
use llama_cpp_2::token::LlamaToken;

use super::prompt_cache::GenerateError;

/// The tokens that the drafter guesses before each decode of the model. On Metal, one guess was
/// faster than two or three.
pub(super) const GUESSED_TOKENS: usize = 1;
/// The architecture of the drafters of Gemma 4, which read the memory of the context of their
/// model.
const GEMMA_DRAFTER_ARCHITECTURE: &str = "gemma4-assistant";
const ARCHITECTURE_KEY: &str = "general.architecture";
/// The index of the output of the decoded token in a decode that also checks a guess.
const TOKEN_OUTPUT: i32 = 0;
/// The index of the output of the guess in a decode that checks it.
const GUESS_OUTPUT: i32 = 1;

#[derive(Debug, thiserror::Error)]
pub(super) enum DrafterError {
    #[error("failed to create the context of the drafter")]
    Context(#[from] LlamaContextLoadError),
    #[error("failed to start the drafts of llama.cpp")]
    Speculative(#[from] MtpSpeculativeError),
}

/// Whether `drafter` can guess the tokens of `model`: a drafter of Gemma 4 with the width of the
/// hidden state of `model`. llama.cpp stops the process when the widths differ.
pub(super) fn is_drafter_of(drafter: &LlamaModel, model: &LlamaModel) -> bool {
    let is_gemma_drafter = drafter
        .meta_val_str(ARCHITECTURE_KEY)
        .is_ok_and(|architecture| architecture == GEMMA_DRAFTER_ARCHITECTURE);
    is_gemma_drafter && drafter.n_embd_out() == model.n_embd_out()
}

/// The context of a model, alone or with the context of its drafter.
pub(super) enum ModelContext<'model> {
    Alone(LlamaContext<'model>),
    WithDrafter(DraftedContext<'model>),
}

/// The context of a model and the context of its drafter.
pub(super) struct DraftedContext<'model> {
    speculative: MtpSpeculative<'model>,
    state: DraftState,
    guesses: usize,
    right_guesses: usize,
}

/// What the drafter has after the last decode of a completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DraftState {
    /// No decode of this completion gave the drafter the state of the model yet, so it cannot
    /// guess.
    Empty,
    /// The drafter has the state of the model after the last decoded token.
    Ready,
    /// As [`DraftState::Ready`], and the last decode also checked this guess of the next token.
    Guessed(LlamaToken),
}

impl<'model> ModelContext<'model> {
    /// Returns `context` with a context of `drafter` next to it, made with `params`.
    pub(super) fn with_drafter(
        llama: &LlamaBackend,
        context: LlamaContext<'model>,
        drafter: &'model LlamaModel,
        params: LlamaContextParams,
    ) -> Result<Self, DrafterError> {
        let drafter_context = drafter.new_context_with_ctx_other(llama, params, &context)?;
        let guesses = i32::try_from(GUESSED_TOKENS).expect("a few tokens fit in i32");
        let speculative = MtpSpeculative::new(
            context,
            drafter_context,
            MtpSpeculativeParams {
                n_max: guesses,
                n_min: 0,
                p_min: 0.0,
            },
        )?;
        Ok(Self::WithDrafter(DraftedContext {
            speculative,
            state: DraftState::Empty,
            guesses: 0,
            right_guesses: 0,
        }))
    }

    pub(super) fn target(&self) -> &LlamaContext<'model> {
        match self {
            Self::Alone(context) => context,
            Self::WithDrafter(drafted) => drafted.speculative.target_context(),
        }
    }

    pub(super) fn target_mut(&mut self) -> &mut LlamaContext<'model> {
        match self {
            Self::Alone(context) => context,
            Self::WithDrafter(drafted) => drafted.speculative.target_context_mut(),
        }
    }

    /// Forgets the guesses of the last completion. Call it before a new completion decodes its
    /// first token.
    pub(super) fn start_completion(&mut self) -> Result<(), MtpSpeculativeError> {
        let Self::WithDrafter(drafted) = self else {
            return Ok(());
        };
        drafted.log_guesses();
        drafted.state = DraftState::Empty;
        drafted.speculative.begin(&[])
    }

    /// Decodes `token` at `token_position` of `sequence` and returns the logits after it. With a
    /// drafter, the decode also checks a guess of the next token, and when the next token is that
    /// guess, its logits need no decode. `batch` holds the token and its guess.
    pub(super) fn decode_next(
        &mut self,
        batch: &mut LlamaBatch<'_>,
        token: LlamaToken,
        token_position: i32,
        sequence: i32,
    ) -> Result<&[f32], GenerateError> {
        match self {
            Self::Alone(context) => {
                batch.clear();
                batch.add(token, token_position, &[sequence], true)?;
                context.decode(batch)?;
                Ok(context.get_logits_ith(TOKEN_OUTPUT))
            }
            Self::WithDrafter(drafted) => drafted.decode_next(batch, token, token_position, sequence),
        }
    }
}

impl DraftedContext<'_> {
    fn decode_next(
        &mut self,
        batch: &mut LlamaBatch<'_>,
        token: LlamaToken,
        token_position: i32,
        sequence: i32,
    ) -> Result<&[f32], GenerateError> {
        match self.state {
            DraftState::Guessed(guess) if guess == token => {
                self.right_guesses += 1;
                self.speculative.accept(1)?;
                self.state = DraftState::Ready;
                return Ok(self.speculative.target_context().get_logits_ith(GUESS_OUTPUT));
            }
            DraftState::Guessed(_) => {
                self.speculative.accept(0)?;
                let wrong_guess_position = u32::try_from(token_position).expect("a position is not negative");
                self.speculative.target_context_mut().kv_cache_seq_rm(
                    sequence,
                    Some(wrong_guess_position),
                    None,
                )?;
            }
            DraftState::Empty | DraftState::Ready => {}
        }
        let guess = match self.state {
            DraftState::Empty => None,
            DraftState::Ready | DraftState::Guessed(_) => self
                .speculative
                .draft(token_position, token, &[])?
                .first()
                .copied(),
        };
        batch.clear();
        batch.add(token, token_position, &[sequence], true)?;
        if let Some(guess) = guess {
            batch.add(guess, token_position + 1, &[sequence], true)?;
            self.guesses += 1;
        }
        self.speculative.target_context_mut().decode(batch)?;
        self.speculative.process(batch)?;
        self.state = guess.map_or(DraftState::Ready, DraftState::Guessed);
        Ok(self.speculative.target_context().get_logits_ith(TOKEN_OUTPUT))
    }

    fn log_guesses(&mut self) {
        if self.guesses > 0 {
            tracing::debug!(
                guesses = self.guesses,
                right_guesses = self.right_guesses,
                "the drafter guessed tokens of a completion"
            );
        }
        self.guesses = 0;
        self.right_guesses = 0;
    }
}
