//! The seam to the service that runs a language model on this computer.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::hub::ModelFile;

/// A number of model tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TokenCount(pub u32);

/// The tokens of one prompt and its completion that the model runner keeps. Core keeps each
/// prompt in this room with the longest completion.
pub const CONTEXT_TOKENS: TokenCount = TokenCount(4096);

/// The text the model continues: what it knows about the writing, then the text before the caret.
/// `Debug` shows only its length, so logs never hold what the user wrote.
///
/// ```
/// use ghosttype_core::inference::Prompt;
///
/// assert_eq!(format!("{:?}", Prompt("Dear Sir".to_owned())), "Prompt(8 chars)");
/// ```
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Prompt(pub String);

impl fmt::Debug for Prompt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Prompt({} chars)", self.0.chars().count())
    }
}

/// The hardware that runs a model, and the interface that drives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Backend {
    /// The CPU of this computer.
    #[default]
    Cpu,
    /// A GPU through Vulkan: NVIDIA, AMD or Intel.
    Vulkan,
    /// An NVIDIA GPU through CUDA.
    Cuda,
    /// The GPU of a Mac through Metal.
    Metal,
}

/// Work for the model runner. Each request answers with the [`InferenceEvent`] named in its
/// variant, or with no event when it names none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceRequest {
    /// Loads the downloaded `file` in place of the loaded model: [`InferenceEvent::Loaded`].
    Load {
        /// The model file to load.
        file: ModelFile,
        /// The backend that runs the model.
        backend: Backend,
    },
    /// Continues `prompt` with the loaded model: [`InferenceEvent::Completed`], after zero or more
    /// [`InferenceEvent::Partial`]. The added text can also finish the last word of `prompt`. The
    /// runner stops at the first line break that it adds. With a `max_tokens` of zero, the runner
    /// only decodes `prompt`, so that a later prompt that starts with it completes sooner.
    Complete {
        /// The text to continue.
        prompt: Prompt,
        /// The most tokens to add.
        max_tokens: TokenCount,
    },
    /// Unloads the loaded model to free its memory.
    Unload,
}

/// The outcome of an [`InferenceRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceEvent {
    /// A model finished loading.
    Loaded {
        /// The file of the request.
        file: ModelFile,
        /// Nothing, or why the model did not load.
        result: Result<(), InferenceError>,
    },
    /// A completion that still runs: the text that it added so far. Each part continues the text
    /// of the part before it.
    Partial(String),
    /// A completion finished: the added text, or why there is none.
    Completed(Result<Completion, InferenceError>),
}

/// Text a model added after a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// The added text, which can be empty.
    pub text: String,
    /// How many tokens the model added.
    pub tokens: TokenCount,
    /// The time from the request to the last token.
    pub duration: Duration,
}

/// Why an inference request failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceError {
    /// The model file is not on disk.
    ModelMissing,
    /// The file is not a model the runner can load.
    ModelInvalid,
    /// The model does not fit in the free memory of the GPU that the backend uses.
    ModelTooLarge,
    /// The backend cannot run here: this build does not include it, or no GPU supports it.
    BackendUnavailable,
    /// No model is loaded.
    NoModel,
    /// The prompt does not fit in the model's context with room for the completion.
    PromptTooLong,
    /// The runner failed while it computed the completion.
    Failed,
    /// A newer completion request for the same sink replaced the request before it finished.
    Superseded,
}

/// Receives inference events. The runner may call it from any thread.
pub type InferenceEventSink = Arc<dyn Fn(InferenceEvent) + Send + Sync>;

/// Loads a language model and completes text with it, without blocking the caller.
pub trait ModelRunner {
    /// Starts `request` and returns at once. Events reach `sink` in the order of the requests.
    /// An [`InferenceRequest::Complete`] can stop the completion that runs for the same `sink`
    /// (the same [`Arc`]), which then answers with [`InferenceError::Superseded`].
    fn submit(&mut self, request: InferenceRequest, sink: &InferenceEventSink);
}
