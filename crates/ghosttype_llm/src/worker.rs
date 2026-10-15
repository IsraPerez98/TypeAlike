//! The thread that owns llama.cpp: the backend, the loaded model and each completion.

use std::ffi::CString;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Backend, Completion, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest, Prompt,
    SystemPrompt, TokenCount,
};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::{BatchAddError, LlamaBatch};
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::{FitError, LlamaModelParams};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::token_type::{LlamaTokenAttr, LlamaTokenAttrs};
use llama_cpp_2::vocab::LlamaVocab;
use llama_cpp_2::{
    DecodeError, LlamaBackendDevice, LlamaBackendDeviceType, LlamaContextLoadError, LlamaCppError,
    LlamaModelLoadError, LogOptions, list_llama_ggml_backend_devices, max_devices, send_logs_to_tracing,
};
use llama_cpp_sys_2::GGML_LOG_LEVEL_WARN;

const CONTEXT_TOKENS: u32 = 2048;
const SEQUENCE: i32 = 0;
const LAST_OUTPUT: i32 = -1;
/// The GPU memory that a model must leave free, as llama.cpp leaves by default, so that the
/// desktop and other apps keep working.
const GPU_MEMORY_MARGIN_BYTES: usize = 1024 * 1024 * 1024;

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
            InferenceRequest::SetSystemPrompt(system_prompt) => {
                worker.system_prompt = system_prompt;
                continue;
            }
            InferenceRequest::Load { file, backend } => InferenceEvent::Loaded {
                result: worker.load(locate_model(&file), backend),
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
    system_prompt: SystemPrompt,
}

impl Worker {
    fn load(&mut self, path: Option<PathBuf>, backend: Option<Backend>) -> Result<(), InferenceError> {
        self.model = None;
        let path = path
            .filter(|path| path.is_file())
            .ok_or(InferenceError::ModelMissing)?;
        let llama = self.start_backend().map_err(|error| {
            tracing::error!(%error, "failed to start llama.cpp");
            InferenceError::Failed
        })?;
        let params = model_params(&path, backend)?;
        let model =
            LlamaModel::load_from_file(llama, &path, &params).map_err(|error: LlamaModelLoadError| {
                tracing::warn!(%error, "failed to load a model");
                InferenceError::ModelInvalid
            })?;
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
        let full_prompt = with_system_prompt(&self.system_prompt, prompt);
        let (text, tokens) = generate(backend, model, &full_prompt, max_tokens).map_err(|error| {
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

/// Returns the text that the model continues: the system prompt, a blank line, then `prompt`.
/// The blank line parts them without a label that the model could copy into its suggestion.
fn with_system_prompt(system_prompt: &SystemPrompt, prompt: &Prompt) -> Prompt {
    let system_prompt = system_prompt.0.trim();
    if system_prompt.is_empty() {
        return prompt.clone();
    }
    Prompt(format!("{system_prompt}\n\n{}", prompt.0))
}

/// Returns the parameters that load the model at `path` on `backend`. For a GPU backend, it first
/// makes sure that the whole model fits in the GPU, because llama.cpp otherwise loads a part of it
/// into slow shared memory, or fails at a later step.
fn model_params(path: &Path, backend: Option<Backend>) -> Result<LlamaModelParams, InferenceError> {
    match backend {
        None => Ok(LlamaModelParams::default()),
        Some(Backend::Cpu) => on_devices(&[]),
        Some(gpu_backend @ (Backend::Vulkan | Backend::Cuda)) => {
            let devices = list_llama_ggml_backend_devices();
            let device = gpu_device(&devices, gpu_backend).ok_or_else(|| {
                tracing::warn!(backend = ?gpu_backend, "no GPU device for the backend");
                InferenceError::BackendUnavailable
            })?;
            tracing::info!(backend = ?gpu_backend, device = %device.description, "loading the model on a GPU");
            check_fit(path, device.index, GPU_MEMORY_MARGIN_BYTES)?;
            on_devices(&[device.index])
        }
    }
}

/// Returns the device that runs `backend`: its first discrete GPU, else its first integrated GPU.
/// The CPU backend has none.
fn gpu_device(devices: &[LlamaBackendDevice], backend: Backend) -> Option<&LlamaBackendDevice> {
    let registry = match backend {
        Backend::Cpu => return None,
        Backend::Vulkan => "Vulkan",
        Backend::Cuda => "CUDA",
    };
    let of_type = |device_type| {
        devices
            .iter()
            .find(|device| device.backend == registry && device.device_type == device_type)
    };
    of_type(LlamaBackendDeviceType::Gpu).or_else(|| of_type(LlamaBackendDeviceType::IntegratedGpu))
}

/// Returns parameters that put the model on the GPU `devices` only. With no devices, the model
/// runs on the CPU only.
fn on_devices(devices: &[usize]) -> Result<LlamaModelParams, InferenceError> {
    LlamaModelParams::default()
        .with_devices(devices)
        .map_err(|error| {
            tracing::warn!(%error, "failed to select the GPU");
            InferenceError::BackendUnavailable
        })
}

/// Makes sure that the model at `path`, with all its layers and its context, fits in the free
/// memory of `device` and leaves `margin_bytes` free. It reads the model but does not load it.
fn check_fit(path: &Path, device: usize, margin_bytes: usize) -> Result<(), InferenceError> {
    let model_path = path
        .to_str()
        .and_then(|path| CString::new(path).ok())
        .ok_or(InferenceError::ModelInvalid)?;
    let mut params = on_devices(&[device])?;
    let all_layers = params.n_gpu_layers();
    let mut context = context_params(CONTEXT_TOKENS);
    let mut margins = vec![margin_bytes; max_devices()];
    let fitted = Pin::new(&mut params).fit_params(
        &model_path,
        &mut context,
        &mut margins,
        CONTEXT_TOKENS,
        GGML_LOG_LEVEL_WARN,
    );
    fit_verdict(fitted.map(|_| params.n_gpu_layers() == all_layers))
}

/// Reads the outcome of llama.cpp's fit: whether it kept every layer on the GPU, or why it failed.
/// llama.cpp moves layers to the CPU when the model does not fit. GhostType refuses that instead.
fn fit_verdict(fitted: Result<bool, FitError>) -> Result<(), InferenceError> {
    match fitted {
        Ok(true) => Ok(()),
        Ok(false) | Err(FitError::Failure) => Err(InferenceError::ModelTooLarge),
        Err(FitError::Error) => Err(InferenceError::ModelInvalid),
    }
}

fn context_params(context_tokens: u32) -> LlamaContextParams {
    LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(context_tokens))
        .with_n_batch(context_tokens)
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

/// Continues `prompt` greedily until the model ends the text, writes markup or adds `max_tokens`.
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
    let mut context = model.new_context(backend, context_params(context_tokens))?;
    let mut batch = LlamaBatch::new(capacity, 1);
    batch.add_sequence(&prompt_tokens, SEQUENCE, false)?;
    context.decode(&mut batch)?;
    let mut sampler = LlamaSampler::greedy();
    let mut added = Vec::new();
    let mut position = i32::try_from(prompt_tokens.len()).expect("the prompt fits in the context");
    while added.len() < max_added {
        let token = sampler.sample(&context, LAST_OUTPUT);
        if vocab.is_eog(token) || is_markup(vocab.attr(token)) {
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

/// Tells if a token with `attrs` is markup, such as `<think>`, that never belongs in typed text.
fn is_markup(attrs: LlamaTokenAttrs) -> bool {
    attrs.intersects(LlamaTokenAttr::Control | LlamaTokenAttr::UserDefined)
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

    fn device(index: usize, backend: &str, device_type: LlamaBackendDeviceType) -> LlamaBackendDevice {
        LlamaBackendDevice {
            index,
            name: format!("{backend}{index}"),
            description: String::new(),
            backend: backend.to_owned(),
            memory_total: 0,
            memory_free: 0,
            device_type,
        }
    }

    fn gpu_index(devices: &[LlamaBackendDevice], backend: Backend) -> Option<usize> {
        gpu_device(devices, backend).map(|device| device.index)
    }

    #[test]
    fn discrete_gpu_runs_the_backend_before_an_integrated_one() {
        let devices = [
            device(0, "CPU", LlamaBackendDeviceType::Cpu),
            device(1, "Vulkan", LlamaBackendDeviceType::IntegratedGpu),
            device(2, "Vulkan", LlamaBackendDeviceType::Gpu),
        ];

        assert_eq!(gpu_index(&devices, Backend::Vulkan), Some(2));
    }

    #[test]
    fn integrated_gpu_runs_the_backend_without_a_discrete_one() {
        let devices = [
            device(0, "Vulkan", LlamaBackendDeviceType::IntegratedGpu),
            device(1, "CPU", LlamaBackendDeviceType::Cpu),
        ];

        assert_eq!(gpu_index(&devices, Backend::Vulkan), Some(0));
    }

    #[test]
    fn gpu_of_another_backend_does_not_run_it() {
        let devices = [
            device(0, "CUDA", LlamaBackendDeviceType::Gpu),
            device(1, "CPU", LlamaBackendDeviceType::Cpu),
        ];

        assert_eq!(gpu_index(&devices, Backend::Vulkan), None);
    }

    #[test]
    fn cuda_backend_runs_on_a_cuda_device() {
        let devices = [
            device(0, "Vulkan", LlamaBackendDeviceType::Gpu),
            device(1, "CUDA", LlamaBackendDeviceType::Gpu),
        ];

        assert_eq!(gpu_index(&devices, Backend::Cuda), Some(1));
    }

    #[test]
    fn cpu_backend_has_no_gpu() {
        let devices = [device(0, "Vulkan", LlamaBackendDeviceType::Gpu)];

        assert_eq!(gpu_index(&devices, Backend::Cpu), None);
    }

    #[test]
    fn only_a_fit_that_keeps_every_layer_on_the_gpu_passes() {
        let fits = [Ok(true), Ok(false), Err(FitError::Failure), Err(FitError::Error)];

        let verdicts = fits.map(fit_verdict);

        assert_eq!(
            verdicts,
            [
                Ok(()),
                Err(InferenceError::ModelTooLarge),
                Err(InferenceError::ModelTooLarge),
                Err(InferenceError::ModelInvalid),
            ]
        );
    }

    #[test]
    #[ignore = "needs a GPU build, a GPU and the path of a GGUF model file in GHOSTTYPE_TEST_MODEL"]
    fn model_does_not_fit_when_the_margin_takes_all_gpu_memory() {
        const ALL_MEMORY_BYTES: usize = 1 << 50;
        let path =
            PathBuf::from(std::env::var_os("GHOSTTYPE_TEST_MODEL").expect("GHOSTTYPE_TEST_MODEL is set"));
        let mut worker = Worker::default();
        worker.start_backend().expect("llama.cpp starts");
        let devices = list_llama_ggml_backend_devices();
        let device = gpu_device(&devices, Backend::Vulkan)
            .or_else(|| gpu_device(&devices, Backend::Cuda))
            .expect("the build has a GPU backend and the computer has a GPU");

        let verdict = check_fit(&path, device.index, ALL_MEMORY_BYTES);

        assert_eq!(verdict, Err(InferenceError::ModelTooLarge));
    }

    #[test]
    fn system_prompt_comes_before_the_prompt_after_a_blank_line() {
        let system_prompt = SystemPrompt("  Formal Spanish.\n".to_owned());

        let full_prompt = with_system_prompt(&system_prompt, &Prompt("Estimado".to_owned()));

        assert_eq!(full_prompt, Prompt("Formal Spanish.\n\nEstimado".to_owned()));
    }

    #[test]
    fn blank_system_prompt_adds_nothing() {
        let prompts = [SystemPrompt::default(), SystemPrompt(" \n ".to_owned())];

        let full_prompts =
            prompts.map(|system_prompt| with_system_prompt(&system_prompt, &Prompt("Dear".to_owned())));

        assert_eq!(
            full_prompts,
            [Prompt("Dear".to_owned()), Prompt("Dear".to_owned())]
        );
    }

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

    #[test]
    fn only_control_and_user_defined_tokens_are_markup() {
        let cases = [
            (LlamaTokenAttr::Normal, false),
            (LlamaTokenAttr::Byte, false),
            (LlamaTokenAttr::Control, true),
            (LlamaTokenAttr::UserDefined, true),
        ];

        let markup = cases.map(|(attr, _)| is_markup(LlamaTokenAttrs(attr.into())));

        assert_eq!(markup, cases.map(|(_, expected)| expected));
    }
}
