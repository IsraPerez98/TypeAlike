//! The seam to the service that runs a language model on this computer.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::hub::ModelFile;

/// A number of model tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TokenCount(pub u32);

/// Text the user typed for the model to continue. `Debug` shows only its length, so logs never
/// hold what the user wrote.
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

/// Work for the model runner. Each request answers with the [`InferenceEvent`] named in its variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceRequest {
    /// Loads the downloaded `file` in place of the loaded model: [`InferenceEvent::Loaded`].
    Load {
        /// The model file to load.
        file: ModelFile,
    },
    /// Continues `prompt` with the loaded model: [`InferenceEvent::Completed`].
    Complete {
        /// The text to continue.
        prompt: Prompt,
        /// The most tokens to add.
        max_tokens: TokenCount,
    },
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
    /// No model is loaded.
    NoModel,
    /// The prompt does not fit in the model's context with room for the completion.
    PromptTooLong,
    /// The runner failed while it computed the completion.
    Failed,
}

/// Receives inference events. The runner may call it from any thread.
pub type InferenceEventSink = Arc<dyn Fn(InferenceEvent) + Send + Sync>;

/// Loads a language model and completes text with it, without blocking the caller.
pub trait ModelRunner {
    /// Starts `request` and returns at once. Events reach `sink` in the order of the requests.
    fn submit(&mut self, request: InferenceRequest, sink: &InferenceEventSink);
}
