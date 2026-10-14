//! The thread that owns llama.cpp: the backend, the loaded model and each completion.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::str;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Completion, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest, Prompt, TokenCount,
};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::{BatchAddError, LlamaBatch};
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::vocab::LlamaVocab;
use llama_cpp_2::{
    DecodeError, LlamaContextLoadError, LlamaCppError, LlamaModelLoadError, LogOptions, send_logs_to_tracing,
};

const CONTEXT_TOKENS: u32 = 2048;
const SEQUENCE: i32 = 0;
const LAST_OUTPUT: i32 = -1;

/// A request and where its event goes.
pub(crate) struct Job {
    pub(crate) request: InferenceRequest,
    pub(crate) sink: InferenceEventSink,
}

/// Runs each job in order until every sender is gone.
pub(crate) fn run(locate_model: &dyn Fn(&ModelFile) -> Option<PathBuf>, jobs: &Receiver<Job>) {
    let mut worker = Worker::default();
    for Job { request, sink } in jobs {
        let event = match request {
            InferenceRequest::Load { file } => InferenceEvent::Loaded {
                result: worker.load(locate_model(&file)),
                file,
            },
            InferenceRequest::Complete { prompt, max_tokens } => {
                InferenceEvent::Completed(worker.complete(&prompt, max_tokens))
            }
        };
        sink(event);
    }
}

#[derive(Default)]
struct Worker {
    backend: Option<LlamaBackend>,
    model: Option<LlamaModel>,
}

impl Worker {
    fn load(&mut self, path: Option<PathBuf>) -> Result<(), InferenceError> {
        self.model = None;
        let path = path
            .filter(|path| path.is_file())
            .ok_or(InferenceError::ModelMissing)?;
        let backend = self.start_backend().map_err(|error| {
            tracing::error!(%error, "failed to start llama.cpp");
            InferenceError::Failed
        })?;
        let model = LlamaModel::load_from_file(backend, &path, &LlamaModelParams::default()).map_err(
            |error: LlamaModelLoadError| {
                tracing::warn!(%error, "failed to load a model");
                InferenceError::ModelInvalid
            },
        )?;
        self.model = Some(model);
        Ok(())
    }

    fn start_backend(&mut self) -> Result<&LlamaBackend, LlamaCppError> {
        if self.backend.is_none() {
            send_logs_to_tracing(LogOptions::default());
            self.backend = Some(LlamaBackend::init()?);
        }
        Ok(self.backend.as_ref().expect("the backend started above"))
    }

    fn complete(&self, prompt: &Prompt, max_tokens: TokenCount) -> Result<Completion, InferenceError> {
        let started = Instant::now();
        let (Some(backend), Some(model)) = (&self.backend, &self.model) else {
            return Err(InferenceError::NoModel);
        };
        let (text, tokens) = generate(backend, model, prompt, max_tokens).map_err(|error| {
            tracing::warn!(%error, "failed to complete text");
            error.to_inference_error()
        })?;
        Ok(Completion {
            text,
            tokens,
            duration: started.elapsed(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
enum GenerateError {
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
}

impl GenerateError {
    fn to_inference_error(&self) -> InferenceError {
        match self {
            Self::PromptTooLong => InferenceError::PromptTooLong,
            Self::EmptyPrompt | Self::Context(_) | Self::Batch(_) | Self::Decode(_) => InferenceError::Failed,
        }
    }
}

/// Continues `prompt` greedily until the model ends the text or adds `max_tokens`.
fn generate(
    backend: &LlamaBackend,
    model: &LlamaModel,
    prompt: &Prompt,
    max_tokens: TokenCount,
) -> Result<(String, TokenCount), GenerateError> {
    let vocab = model.vocab();
    let prompt_tokens = vocab.tokenize(prompt.0.as_bytes(), true, false);
    let context_tokens = CONTEXT_TOKENS.min(model.n_ctx_train());
    let max_added = usize::try_from(max_tokens.0).expect("a u32 fits in usize");
    let capacity = usize::try_from(context_tokens).expect("a u32 fits in usize");
    if prompt_tokens.is_empty() {
        return Err(GenerateError::EmptyPrompt);
    }
    if prompt_tokens.len().saturating_add(max_added) > capacity {
        return Err(GenerateError::PromptTooLong);
    }
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(context_tokens))
        .with_n_batch(context_tokens);
    let mut context = model.new_context(backend, params)?;
    let mut batch = LlamaBatch::new(capacity, 1);
    batch.add_sequence(&prompt_tokens, SEQUENCE, false)?;
    context.decode(&mut batch)?;
    let mut sampler = LlamaSampler::greedy();
    let mut added = Vec::new();
    let mut position = i32::try_from(prompt_tokens.len()).expect("the prompt fits in the context");
    while added.len() < max_added {
        let token = sampler.sample(&context, LAST_OUTPUT);
        if vocab.is_eog(token) {
            break;
        }
        added.push(token);
        if added.len() == max_added {
            break;
        }
        batch.clear();
        batch.add(token, position, &[SEQUENCE], true)?;
        position = position.saturating_add(1);
        context.decode(&mut batch)?;
    }
    let tokens = TokenCount(u32::try_from(added.len()).expect("added tokens fit in the context"));
    Ok((added_text(&vocab, &prompt_tokens, &added), tokens))
}

/// Returns the text `added` puts after `prompt_tokens`, with the spacing it has after them.
fn added_text(vocab: &LlamaVocab<'_>, prompt_tokens: &[LlamaToken], added: &[LlamaToken]) -> String {
    let prompt_bytes = vocab.detokenize(prompt_tokens, false, false);
    let all_bytes = vocab.detokenize(&[prompt_tokens, added].concat(), false, false);
    let added_bytes = all_bytes
        .strip_prefix(prompt_bytes.as_slice())
        .map_or_else(|| vocab.detokenize(added, false, false), <[u8]>::to_vec);
    text_without_cut_character(&added_bytes)
}

/// Returns `bytes` as text, without a character that the token limit cut at the end.
fn text_without_cut_character(bytes: &[u8]) -> String {
    let end = match str::from_utf8(bytes) {
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        _ => bytes.len(),
    };
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_text_is_kept() {
        let text = text_without_cut_character("café".as_bytes());

        assert_eq!(text, "café");
    }

    #[test]
    fn character_cut_at_the_end_is_dropped() {
        let bytes = "café".as_bytes();

        let text = text_without_cut_character(&bytes[..bytes.len() - 1]);

        assert_eq!(text, "caf");
    }

    #[test]
    fn invalid_bytes_inside_become_replacement_characters() {
        let text = text_without_cut_character(b"a\xffb");

        assert_eq!(text, "a\u{FFFD}b");
    }
}
