//! The seam to the service that runs a language model on this computer.

use std::fmt;
use std::num::NonZeroUsize;
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

/// The physical cores of the CPU of this computer. A core with two threads counts one time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuCores {
    /// The cores of the fastest kind. On a CPU with one kind of core, every core.
    pub performance: NonZeroUsize,
    /// The cores of slower kinds, such as the efficiency cores of a hybrid CPU.
    pub efficiency: usize,
}

/// Work for the model runner. Each request answers with the [`InferenceEvent`] named in its
/// variant, or with no event when it names none.
#[derive(Debug, Clone, PartialEq)]
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
        /// The preference for the words that the writer often writes, if any.
        word_bias: Option<WordBias>,
        /// The least likely that the added text can get after its first word, if any. The runner
        /// stops before a token that makes the text less likely, and leaves out the word of that
        /// token.
        min_log_probability: Option<LogProbability>,
    },
    /// Unloads the loaded model to free its memory.
    Unload,
}

/// A preference of the model for the words that the writer often writes: the logits to add to the
/// first token of each word, after a space. The runner prepares each `version` one time, so two
/// biases with the same version hold the same words. `Debug` shows only the version and the
/// number of words, so logs never hold what the user wrote.
#[derive(Clone)]
pub struct WordBias {
    /// Names the words. A new table of words gets a new version.
    pub version: u64,
    /// Each word in the spelling that the writer uses most, with the logits to add to its first
    /// token.
    pub words: Arc<[(String, f32)]>,
}

impl PartialEq for WordBias {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
    }
}

impl Eq for WordBias {}

impl fmt::Debug for WordBias {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WordBias")
            .field("version", &self.version)
            .field("words", &self.words.len())
            .finish()
    }
}

/// The outcome of an [`InferenceRequest`].
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    /// The added text, which can be empty.
    pub text: String,
    /// How many tokens the model added.
    pub tokens: TokenCount,
    /// The time from the request to the last token.
    pub duration: Duration,
    /// How likely the model found each word of the added text, in order. A word starts at each
    /// whitespace character after a character that is not whitespace. When the text finishes the
    /// last word of the prompt, the first word also holds how likely the model found the end of
    /// that word, among the texts that agree with the letters that the prompt has of it.
    pub word_log_probabilities: Vec<LogProbability>,
}

/// The natural log of a probability: zero for a certain event, and lower for a less likely one.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct LogProbability(pub f32);

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
