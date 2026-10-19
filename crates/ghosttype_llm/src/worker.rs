//! The thread that owns llama.cpp: the backend, the loaded model and each completion.

use std::collections::HashMap;
use std::ffi::CString;
use std::mem;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Backend, CONTEXT_TOKENS, Completion, CpuCores, InferenceError, InferenceEvent, InferenceEventSink,
    InferenceRequest, LogProbability, Prompt, TokenCount, WordBias,
};
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::kv_cache::KvCacheConversionError;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::{BatchAddError, LlamaBatch};
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::{FitError, LlamaModelParams};
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::token_type::{LlamaTokenAttr, LlamaTokenAttrs};
use llama_cpp_2::vocab::LlamaVocab;
use llama_cpp_2::{
    DecodeError, LlamaBackendDevice, LlamaBackendDeviceType, LlamaContextLoadError, LlamaCppError,
    LlamaModelLoadError, LlamaStateSeqFlags, LogOptions, SeqState, StateSeqError,
    list_llama_ggml_backend_devices, max_devices, send_logs_to_tracing,
};
use llama_cpp_sys_2::GGML_LOG_LEVEL_WARN;

use crate::pieces::TokenPieces;

/// The most prompt tokens that one decode takes. The context keeps GPU memory for the outputs of a
/// whole batch while the model stays loaded, so a small batch keeps that memory small.
const BATCH_TOKENS: u32 = 128;
/// The fewest threads for the tokens of a prompt on the CPU, when the CPU has that many cores: the
/// default of llama.cpp. On a CPU with few performance cores, efficiency cores then also help.
const MIN_PROMPT_THREADS: usize = 4;
/// The most threads for each added token on the CPU. An added token waits mostly for memory, so
/// more threads use much more CPU time for a small gain.
const MAX_GENERATION_THREADS: usize = 4;
/// The sequence that keeps the last prompt between completions.
const PROMPT_SEQUENCE: i32 = 0;
/// The sequence that continues a copy of the prompt.
const COMPLETION_SEQUENCE: i32 = 1;
const SEQUENCES: u32 = 2;
const LAST_OUTPUT: i32 = -1;
/// The probability of the most likely token from which the model is sure of it, so that a word
/// bias does not change it.
const SURE_PROBABILITY: f32 = 0.5;
/// The most text at the end of a prompt that generation takes back and writes again.
const MAX_HEALED_BYTES: usize = 24;
/// The most saved states of the prompt sequence: one for each of the last words, so that a
/// backspace across each of these words goes back to a saved state.
const MAX_SNAPSHOTS: usize = 4;
/// The GPU memory that a model must leave free, as llama.cpp leaves by default, so that the
/// desktop and other apps keep working.
const GPU_MEMORY_MARGIN_BYTES: usize = 1024 * 1024 * 1024;

/// A request and where its event goes.
pub(crate) struct Job {
    pub(crate) request: InferenceRequest,
    pub(crate) sink: InferenceEventSink,
}

/// The jobs that wait.
struct JobQueue<'jobs> {
    jobs: &'jobs Receiver<Job>,
    /// The job that comes before the jobs in `jobs`: a job that a step looked at or gave back.
    next_job: Option<Job>,
}

impl<'jobs> JobQueue<'jobs> {
    fn new(jobs: &'jobs Receiver<Job>) -> Self {
        Self { jobs, next_job: None }
    }

    /// Waits for the next job. Returns `None` when every sender is gone.
    fn next(&mut self) -> Option<Job> {
        self.next_job.take().or_else(|| self.jobs.recv().ok())
    }

    /// Makes `job` the next job again.
    fn give_back(&mut self, job: Job) {
        self.next_job = Some(job);
    }

    /// Whether the next job, if one waits, completes text for `sink`, so that it replaces the
    /// completion for `sink` that runs now.
    fn has_newer_completion(&mut self, sink: &InferenceEventSink) -> bool {
        if self.next_job.is_none() {
            self.next_job = self.jobs.try_recv().ok();
        }
        self.next_job.as_ref().is_some_and(|job| {
            matches!(job.request, InferenceRequest::Complete { .. }) && Arc::ptr_eq(&job.sink, sink)
        })
    }
}

/// Runs each job in order until every sender is gone. A model on the CPU runs on threads for
/// `cpu_cores`, or on the threads of llama.cpp when the cores are unknown.
pub(crate) fn run(
    locate_model: &dyn Fn(&ModelFile) -> Option<PathBuf>,
    cpu_cores: Option<CpuCores>,
    jobs: &Receiver<Job>,
) {
    let mut queue = JobQueue::new(jobs);
    let mut llama = None;
    while let Some(Job { request, sink }) = queue.next() {
        match request {
            InferenceRequest::Load { file, backend } => {
                let loaded = load(&mut llama, locate_model(&file), backend);
                let result = loaded.as_ref().map(|_| ()).map_err(|error| *error);
                sink(InferenceEvent::Loaded { file, result });
                if let (Ok(model), Some(llama)) = (loaded, &llama) {
                    serve(llama, &model, threads_for(backend, cpu_cores), &mut queue);
                }
            }
            InferenceRequest::Complete { .. } => {
                sink(InferenceEvent::Completed(Err(InferenceError::NoModel)));
            }
            InferenceRequest::Unload => {}
        }
    }
}

/// Loads the model at `path` on `backend`. It starts llama.cpp in `llama` first, when it did not
/// start before.
fn load(
    llama: &mut Option<LlamaBackend>,
    path: Option<PathBuf>,
    backend: Backend,
) -> Result<LlamaModel, InferenceError> {
    let path = path
        .filter(|path| path.is_file())
        .ok_or(InferenceError::ModelMissing)?;
    let llama = start_backend(llama).map_err(|error| {
        tracing::error!(%error, "failed to start llama.cpp");
        InferenceError::Failed
    })?;
    let params = model_params(&path, backend)?;
    LlamaModel::load_from_file(llama, &path, &params).map_err(|error: LlamaModelLoadError| {
        tracing::warn!(%error, "failed to load a model");
        InferenceError::ModelInvalid
    })
}

fn start_backend(llama: &mut Option<LlamaBackend>) -> Result<&LlamaBackend, LlamaCppError> {
    if llama.is_none() {
        send_logs_to_tracing(LogOptions::default());
        *llama = Some(LlamaBackend::init()?);
    }
    Ok(llama.as_ref().expect("the backend started above"))
}

/// Runs the completions for the loaded `model` until a job loads or unloads a model. A model on the
/// CPU runs on `cpu_threads`, or on the threads of llama.cpp. It gives the job that loads or
/// unloads back to `queue`, so the model is gone before the next model loads.
fn serve(
    llama: &LlamaBackend,
    model: &LlamaModel,
    cpu_threads: Option<CpuThreads>,
    queue: &mut JobQueue<'_>,
) {
    if let Some(threads) = cpu_threads {
        tracing::info!(
            prompt = threads.prompt,
            generation = threads.generation,
            "running the model on CPU threads"
        );
    }
    let mut loaded = LoadedModel {
        llama,
        model,
        cpu_threads,
        cache: None,
        word_tokens: None,
        pieces: TokenPieces::new(model.tokens(false)),
    };
    while let Some(job) = queue.next() {
        let InferenceRequest::Complete {
            prompt,
            max_tokens,
            word_bias,
            min_log_probability,
        } = &job.request
        else {
            queue.give_back(job);
            return;
        };
        let mut progress = Progress {
            is_outdated: &mut || queue.has_newer_completion(&job.sink),
            send_partial: &mut |text| (job.sink)(InferenceEvent::Partial(text)),
        };
        let limits = Limits {
            max_added: usize::try_from(max_tokens.0).expect("a u32 fits in usize"),
            min_log_probability: *min_log_probability,
        };
        let result = loaded.complete(prompt, limits, word_bias.as_ref(), &mut progress);
        (job.sink)(InferenceEvent::Completed(result));
    }
}

/// Where a completion stops adding tokens.
#[derive(Debug, Clone, Copy)]
struct Limits {
    max_added: usize,
    /// The least likely that the added text can get after its first word.
    min_log_probability: Option<LogProbability>,
}

/// What a running completion asks and tells its job.
struct Progress<'job> {
    /// Tells that a newer completion waits, so that this completion must stop.
    is_outdated: &'job mut dyn FnMut() -> bool,
    /// Receives the text that the completion added so far.
    send_partial: &'job mut dyn FnMut(String),
}

/// Returns the parameters that load the model at `path` on `backend`. For Vulkan and CUDA, it first
/// makes sure that the whole model fits in the GPU, because llama.cpp otherwise loads a part of it
/// into slow shared memory, or fails at a later step. Metal keeps the parameters of llama.cpp,
/// which put the whole model on the GPU of the Mac.
fn model_params(path: &Path, backend: Backend) -> Result<LlamaModelParams, InferenceError> {
    match backend {
        Backend::Cpu => on_devices(&[]),
        Backend::Metal => Ok(LlamaModelParams::default()),
        gpu_backend @ (Backend::Vulkan | Backend::Cuda) => {
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
/// The CPU backend has none, and llama.cpp selects the GPU for Metal.
fn gpu_device(devices: &[LlamaBackendDevice], backend: Backend) -> Option<&LlamaBackendDevice> {
    let registry = match backend {
        Backend::Cpu | Backend::Metal => return None,
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
    let mut context = context_params(CONTEXT_TOKENS.0, None);
    let mut margins = vec![margin_bytes; max_devices()];
    let fitted = Pin::new(&mut params).fit_params(
        &model_path,
        &mut context,
        &mut margins,
        CONTEXT_TOKENS.0,
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

/// Returns the parameters of a context. A context on the CPU runs on `cpu_threads`, or on the
/// threads of llama.cpp.
fn context_params(context_tokens: u32, cpu_threads: Option<CpuThreads>) -> LlamaContextParams {
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(context_tokens))
        .with_n_batch(BATCH_TOKENS)
        .with_n_ubatch(BATCH_TOKENS)
        .with_n_seq_max(SEQUENCES)
        .with_kv_unified(true);
    match cpu_threads {
        Some(threads) => params
            .with_n_threads(threads.generation)
            .with_n_threads_batch(threads.prompt),
        None => params,
    }
}

/// The threads that run a model on the CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CpuThreads {
    /// The threads for the tokens of a prompt.
    prompt: i32,
    /// The threads for each added token.
    generation: i32,
}

impl CpuThreads {
    /// Returns the threads for a CPU with `cores`. A prompt runs one thread on each performance
    /// core, because its threads work in lockstep: a thread on a slower core, or on the second
    /// thread of a core, makes the other threads wait.
    fn for_cores(cores: CpuCores) -> Self {
        let physical = cores.performance.get().saturating_add(cores.efficiency);
        let prompt = cores.performance.get().max(physical.min(MIN_PROMPT_THREADS));
        let generation = prompt.min(MAX_GENERATION_THREADS);
        Self {
            prompt: i32::try_from(prompt).unwrap_or(i32::MAX),
            generation: i32::try_from(generation).unwrap_or(i32::MAX),
        }
    }
}

/// Returns the threads for a model on `backend` with the `cpu_cores` of this computer. Returns
/// `None` to keep the threads of llama.cpp: on a GPU backend, and when the cores are unknown.
fn threads_for(backend: Backend, cpu_cores: Option<CpuCores>) -> Option<CpuThreads> {
    match backend {
        Backend::Cpu => cpu_cores.map(CpuThreads::for_cores),
        Backend::Vulkan | Backend::Cuda | Backend::Metal => None,
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
    #[error("failed to change the cache of a sequence")]
    Sequence(#[from] KvCacheConversionError),
    #[error("failed to save the state of a sequence")]
    Snapshot(#[from] StateSeqError),
    #[error("a newer completion waits")]
    Superseded,
}

impl GenerateError {
    fn to_inference_error(&self) -> InferenceError {
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

/// A loaded model, with the cache of its last prompt.
struct LoadedModel<'model> {
    llama: &'model LlamaBackend,
    model: &'model LlamaModel,
    cpu_threads: Option<CpuThreads>,
    cache: Option<PromptCache<'model>>,
    /// The version of the last word bias, and the logits that it adds to each token.
    word_tokens: Option<(u64, Vec<(LlamaToken, f32)>)>,
    pieces: TokenPieces,
}

impl LoadedModel<'_> {
    /// Completes `prompt`, and sends the text to `progress` while it grows. It stops early when
    /// `progress` tells that a newer completion waits.
    fn complete(
        &mut self,
        prompt: &Prompt,
        limits: Limits,
        word_bias: Option<&WordBias>,
        progress: &mut Progress<'_>,
    ) -> Result<Completion, InferenceError> {
        let started = Instant::now();
        let token_bias = self.token_bias(word_bias);
        let (text, added) = self
            .generate(prompt, limits, &token_bias, progress)
            .map_err(|error| {
                if !matches!(error, GenerateError::Superseded) {
                    tracing::warn!(%error, "failed to complete text");
                }
                error.to_inference_error()
            })?;
        Ok(Completion {
            text,
            tokens: TokenCount(u32::try_from(added.tokens.len()).expect("added tokens fit in the context")),
            duration: started.elapsed(),
            word_log_probabilities: added.word_log_probabilities,
        })
    }

    /// Returns the logits that `word_bias` adds to each token. It finds the tokens of each version
    /// one time.
    fn token_bias(&mut self, word_bias: Option<&WordBias>) -> Vec<(LlamaToken, f32)> {
        let Some(word_bias) = word_bias else {
            return Vec::new();
        };
        let is_known = self
            .word_tokens
            .as_ref()
            .is_some_and(|(version, _)| *version == word_bias.version);
        if !is_known {
            let tokens = first_word_tokens(&self.model.vocab(), &word_bias.words);
            self.word_tokens = Some((word_bias.version, tokens));
        }
        self.word_tokens
            .as_ref()
            .map(|(_, tokens)| tokens.clone())
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
        limits: Limits,
        token_bias: &[(LlamaToken, f32)],
        progress: &mut Progress<'_>,
    ) -> Result<(String, AddedTokens), GenerateError> {
        let model = self.model;
        let vocab = model.vocab();
        let mut prompt_tokens = vocab.tokenize(prompt.0.as_bytes(), true, false);
        let healed_text = take_back_last_word(&vocab, &mut prompt_tokens);
        let last_word_start = last_word_start(&vocab, &prompt_tokens);
        let context_tokens = CONTEXT_TOKENS.0.min(model.n_ctx_train());
        let capacity = usize::try_from(context_tokens).expect("a u32 fits in usize");
        if prompt_tokens.is_empty() {
            return Err(GenerateError::EmptyPrompt);
        }
        if prompt_tokens.len().saturating_add(limits.max_added) > capacity {
            return Err(GenerateError::PromptTooLong);
        }
        if self.cache.is_none() {
            self.cache = Some(PromptCache::new(
                self.llama,
                model,
                context_tokens,
                self.cpu_threads,
            )?);
        }
        let cache = self.cache.as_mut().expect("the cache exists above");
        let added = cache
            .decode_prompt(&prompt_tokens, last_word_start, progress.is_outdated)
            .and_then(|()| {
                let healing = Healing {
                    pieces: &self.pieces,
                    text: &healed_text,
                };
                cache.continue_prompt(&vocab, &healing, limits, token_bias, progress)
            });
        match added {
            Ok(added) => Ok((
                added_text(&vocab, &prompt_tokens, &added.tokens, &healed_text),
                added,
            )),
            Err(GenerateError::Superseded) => Err(GenerateError::Superseded),
            Err(error) => {
                self.cache = None;
                Err(error)
            }
        }
    }
}

/// A context that keeps the last prompt, so that a later prompt that starts with the same tokens
/// decodes only the tokens that it adds.
struct PromptCache<'model> {
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

/// A saved state of the prompt sequence after its first `tokens` tokens.
struct Snapshot {
    tokens: usize,
    state: SeqState,
    logits: Vec<f32>,
}

impl<'model> PromptCache<'model> {
    fn new(
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
    fn decode_prompt(
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
            snapshots.retain(|snapshot| snapshot.tokens <= self.tokens.len());
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
            .is_some_and(|last| last.tokens == self.tokens.len())
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
            tokens: self.tokens.len(),
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
            .find(|snapshot| snapshot.tokens <= common)
        else {
            return false;
        };
        let start = u32::try_from(snapshot.tokens).expect("a prompt position fits in u32");
        let is_restored = self
            .context
            .state_seq_set(&snapshot.state, PROMPT_SEQUENCE)
            .is_ok()
            && self
                .context
                .kv_cache_seq_rm(PROMPT_SEQUENCE, Some(start), None)
                .is_ok();
        if is_restored {
            self.tokens.truncate(snapshot.tokens);
            self.logits.clone_from(&snapshot.logits);
        }
        is_restored
    }

    /// Continues a copy of the prompt sequence greedily, so that the prompt sequence stays as it is
    /// for the next completion. Its first added tokens write the `healing` text again. It stops at
    /// the `limits`, and leaves out the word that the minimum probability cuts. Before each decode,
    /// it stops when `progress` tells so, and else sends the text that it added so far.
    fn continue_prompt(
        &mut self,
        vocab: &LlamaVocab<'_>,
        healing: &Healing<'_>,
        limits: Limits,
        token_bias: &[(LlamaToken, f32)],
        progress: &mut Progress<'_>,
    ) -> Result<AddedTokens, GenerateError> {
        self.context
            .copy_kv_cache_seq(PROMPT_SEQUENCE, COMPLETION_SEQUENCE, None, None)?;
        let mut batch = LlamaBatch::new(1, 1);
        let mut logits = self.logits.as_slice();
        let mut biased = Vec::new();
        let mut unwritten = healing.text;
        let mut added = Vec::new();
        let mut words = WordLogProbabilities::default();
        while added.len() < limits.max_added {
            let scores = with_bias(logits, token_bias, &mut biased);
            let (token, log_probability) = choose_token(scores, logits, healing.pieces, unwritten);
            if vocab.is_eog(token) || is_markup(vocab.attr(token)) {
                break;
            }
            let piece = vocab.token_to_piece(token, false, None);
            let text = added_after(&piece, unwritten);
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
            unwritten = unwritten_after(unwritten, &piece);
            added.push(token);
            if ends_line || added.len() == limits.max_added {
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
        })
    }
}

/// The text that the first added tokens write again, and the text of each token, which finds the
/// tokens that agree with it.
struct Healing<'text> {
    pieces: &'text TokenPieces,
    text: &'text [u8],
}

/// The tokens that a completion added, and how likely the model found each word of them.
struct AddedTokens {
    tokens: Vec<LlamaToken>,
    word_log_probabilities: Vec<LogProbability>,
}

/// Sums the log probabilities of the added tokens for each word that they write.
#[derive(Debug, Default)]
struct WordLogProbabilities {
    words: Vec<LogProbability>,
    /// The tokens of the last word.
    last_word_tokens: usize,
    /// Whether the added text ends with a character that is not whitespace.
    ends_in_word: bool,
}

impl WordLogProbabilities {
    /// Adds a token that writes `text` after the healed text. Whitespace after a word starts the
    /// next word. A token that only writes healed text belongs to the first word.
    fn add(&mut self, text: &[u8], log_probability: f32) {
        let starts_word = self.starts_word(text);
        match self.words.last_mut() {
            Some(word) if !starts_word => {
                word.0 += log_probability;
                self.last_word_tokens += 1;
            }
            _ => {
                self.words.push(LogProbability(log_probability));
                self.last_word_tokens = 1;
            }
        }
        if let Some(last) = text.last() {
            self.ends_in_word = !last.is_ascii_whitespace();
        }
    }

    /// Whether a token that writes `text` with `log_probability` keeps the added text at least as
    /// likely as `cut`. Each token of the first word does.
    fn keeps(&self, text: &[u8], log_probability: f32, cut: LogProbability) -> bool {
        let is_in_first_word = self.words.len() <= 1 && !self.starts_word(text);
        let total = self.words.iter().map(|word| word.0).sum::<f32>();
        is_in_first_word || total + log_probability >= cut.0
    }

    /// Ends the words before a token that writes `text`: it drops the last word when the token
    /// would continue it. Returns how many tokens the dropped word had.
    fn end_before(&mut self, text: &[u8]) -> usize {
        if self.starts_word(text) {
            return 0;
        }
        self.words.pop();
        mem::take(&mut self.last_word_tokens)
    }

    fn starts_word(&self, text: &[u8]) -> bool {
        self.ends_in_word && text.first().is_some_and(u8::is_ascii_whitespace)
    }

    fn into_vec(self) -> Vec<LogProbability> {
        self.words
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

/// Removes the tokens of the last word from the end of `prompt_tokens`, and returns their text.
/// It stops at the token that starts the word, at markup, at a line break, and before the first
/// token, so the prompt never becomes empty. A line break ends a word, and the model writes too
/// many line breaks when it writes the last ones again.
fn take_back_last_word(vocab: &LlamaVocab<'_>, prompt_tokens: &mut Vec<LlamaToken>) -> Vec<u8> {
    let mut text = Vec::new();
    while text.len() < MAX_HEALED_BYTES && prompt_tokens.len() > 1 {
        let Some(&token) = prompt_tokens
            .last()
            .filter(|&&token| !is_markup(vocab.attr(token)))
        else {
            break;
        };
        let piece = vocab.token_to_piece(token, false, None);
        if piece.contains(&b'\n') {
            break;
        }
        prompt_tokens.pop();
        text.splice(0..0, piece.iter().copied());
        if starts_word(&piece) {
            break;
        }
    }
    text
}

/// Returns the index of the token that starts the last word of `prompt_tokens`, as healing finds
/// that word.
fn last_word_start(vocab: &LlamaVocab<'_>, prompt_tokens: &[LlamaToken]) -> usize {
    let mut before_last_word = prompt_tokens.to_vec();
    take_back_last_word(vocab, &mut before_last_word);
    before_last_word.len()
}

/// Returns the token to add for `scores`, and the natural log of its probability in `logits`. While
/// text is unwritten, the token agrees with it, and its probability is its share of the tokens that
/// agree.
fn choose_token(scores: &[f32], logits: &[f32], pieces: &TokenPieces, unwritten: &[u8]) -> (LlamaToken, f32) {
    if let Some(token) = best_token_agreeing_with(scores, pieces, unwritten) {
        return (
            token,
            agreeing_log_probability(logits, token, pieces.agreeing_with(unwritten)),
        );
    }
    let token = most_likely(scores);
    (token, log_probability_of(logits, token))
}

/// Returns the natural log of the probability that `logits` give to `token`.
fn log_probability_of(logits: &[f32], token: LlamaToken) -> f32 {
    let highest = highest_of(logits);
    let total = logits.iter().map(|logit| (logit - highest).exp()).sum::<f32>();
    logit_of(logits, token) - highest - total.ln()
}

/// Returns the natural log of the probability that `logits` give to `token`, among the `agreeing`
/// tokens. `token` must be one of them.
fn agreeing_log_probability(
    logits: &[f32],
    token: LlamaToken,
    agreeing: impl Iterator<Item = LlamaToken>,
) -> f32 {
    let chosen = logit_of(logits, token);
    let total = agreeing
        .map(|other| (logit_of(logits, other) - chosen).exp())
        .sum::<f32>();
    -total.ln()
}

fn highest_of(logits: &[f32]) -> f32 {
    logits.iter().copied().fold(f32::NEG_INFINITY, f32::max)
}

fn logit_of(logits: &[f32], token: LlamaToken) -> f32 {
    usize::try_from(token.0)
        .ok()
        .and_then(|index| logits.get(index))
        .copied()
        .unwrap_or(f32::NEG_INFINITY)
}

/// Returns the most likely token in `logits` whose text can write the start of the `unwritten`
/// text, the token with the lowest id of equally likely ones, or `None` when no text is unwritten.
fn best_token_agreeing_with(logits: &[f32], pieces: &TokenPieces, unwritten: &[u8]) -> Option<LlamaToken> {
    if unwritten.is_empty() {
        return None;
    }
    pieces.agreeing_with(unwritten).max_by(|left, right| {
        logit_of(logits, *left)
            .total_cmp(&logit_of(logits, *right))
            .then(right.0.cmp(&left.0))
    })
}

/// Returns what stays unwritten of `unwritten` after a token with `piece` as text.
fn unwritten_after<'text>(unwritten: &'text [u8], piece: &[u8]) -> &'text [u8] {
    unwritten.strip_prefix(piece).unwrap_or_default()
}

/// Returns the first token of each of `words` after a space, with the most logits of the words
/// that start with it. It leaves out a token that does not start a word or that is only a space.
fn first_word_tokens(vocab: &LlamaVocab<'_>, words: &[(String, f32)]) -> Vec<(LlamaToken, f32)> {
    let mut biases = HashMap::<LlamaToken, f32>::new();
    for (word, bias) in words {
        let Some(&first) = vocab
            .tokenize(format!(" {word}").as_bytes(), false, false)
            .first()
        else {
            continue;
        };
        let piece = vocab.token_to_piece(first, false, None);
        if !starts_word(&piece) || piece.trim_ascii().is_empty() {
            continue;
        }
        let kept = biases.entry(first).or_insert(*bias);
        *kept = kept.max(*bias);
    }
    biases.into_iter().collect()
}

/// Returns `logits` with `token_bias` added, in `biased` when the bias has tokens. The bias stays
/// out when the model is sure of its first choice, at least [`SURE_PROBABILITY`].
fn with_bias<'logits>(
    logits: &'logits [f32],
    token_bias: &[(LlamaToken, f32)],
    biased: &'logits mut Vec<f32>,
) -> &'logits [f32] {
    if token_bias.is_empty() || top_probability(logits) >= SURE_PROBABILITY {
        return logits;
    }
    biased.clear();
    biased.extend_from_slice(logits);
    for &(token, bias) in token_bias {
        let index = usize::try_from(token.0).expect("a token is never negative");
        if let Some(logit) = biased.get_mut(index) {
            *logit += bias;
        }
    }
    biased
}

/// Returns the probability of the most likely token in `logits`.
fn top_probability(logits: &[f32]) -> f32 {
    let highest = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let total = logits.iter().map(|logit| (logit - highest).exp()).sum::<f32>();
    total.recip()
}

/// Returns the most likely token in `logits`, the token with the highest id of equally likely
/// ones.
fn most_likely(logits: &[f32]) -> LlamaToken {
    let highest = highest_of(logits);
    let index = logits
        .iter()
        .rposition(|logit| logit.total_cmp(&highest).is_eq())
        .unwrap_or(0);
    LlamaToken(i32::try_from(index).expect("the vocabulary fits in i32"))
}

/// Whether a token with `piece` as text adds a line break after it writes the `unwritten` text.
fn writes_line_break(piece: &[u8], unwritten: &[u8]) -> bool {
    added_after(piece, unwritten).contains(&b'\n')
}

/// Returns the text that a token with `piece` as text adds after it writes the `unwritten` text.
fn added_after<'piece>(piece: &'piece [u8], unwritten: &[u8]) -> &'piece [u8] {
    piece.get(unwritten.len()..).unwrap_or_default()
}

fn starts_word(piece: &[u8]) -> bool {
    piece.first().is_some_and(u8::is_ascii_whitespace)
}

/// Tells if a token with `attrs` is markup, such as `<think>`, that never belongs in typed text.
fn is_markup(attrs: LlamaTokenAttrs) -> bool {
    attrs.intersects(LlamaTokenAttr::Control | LlamaTokenAttr::UserDefined)
}

/// Returns the text `added` puts after `prompt_tokens`, with the spacing it has after them, without
/// the `healed_text` that the prompt already ends with.
fn added_text(
    vocab: &LlamaVocab<'_>,
    prompt_tokens: &[LlamaToken],
    added: &[LlamaToken],
    healed_text: &[u8],
) -> String {
    let prompt_bytes = vocab.detokenize(prompt_tokens, false, false);
    let all_bytes = vocab.detokenize(&[prompt_tokens, added].concat(), false, false);
    let added_bytes = all_bytes
        .strip_prefix(prompt_bytes.as_slice())
        .map_or_else(|| vocab.detokenize(added, false, false), <[u8]>::to_vec);
    text_without_cut_character(text_after_prefix(&added_bytes, healed_text))
}

/// Returns `bytes` after `prefix`, or nothing when they do not start with it, so that a completion
/// never types again text that is already there.
fn text_after_prefix<'bytes>(bytes: &'bytes [u8], prefix: &[u8]) -> &'bytes [u8] {
    bytes.strip_prefix(prefix).unwrap_or_default()
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
    use std::num::NonZeroUsize;
    use std::sync::mpsc;

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

    fn cpu_cores(performance: usize, efficiency: usize) -> CpuCores {
        CpuCores {
            performance: NonZeroUsize::new(performance).expect("a test CPU has a performance core"),
            efficiency,
        }
    }

    #[test]
    fn prompt_runs_on_each_performance_core_and_on_at_least_four_cores() {
        let cores = [
            cpu_cores(1, 0),
            cpu_cores(2, 0),
            cpu_cores(2, 8),
            cpu_cores(4, 8),
            cpu_cores(8, 8),
            cpu_cores(16, 0),
        ];

        let threads = cores.map(|cores| CpuThreads::for_cores(cores).prompt);

        assert_eq!(threads, [1, 2, 4, 4, 8, 16]);
    }

    #[test]
    fn each_added_token_runs_on_at_most_four_threads() {
        let cores = [cpu_cores(2, 0), cpu_cores(2, 8), cpu_cores(16, 0)];

        let threads = cores.map(|cores| CpuThreads::for_cores(cores).generation);

        assert_eq!(threads, [2, 4, 4]);
    }

    #[test]
    fn cpu_backend_runs_the_threads_for_its_cores() {
        let threads = threads_for(Backend::Cpu, Some(cpu_cores(8, 8)));

        assert_eq!(
            threads,
            Some(CpuThreads {
                prompt: 8,
                generation: 4
            })
        );
    }

    #[test]
    fn gpu_backends_and_unknown_cores_keep_the_threads_of_llama_cpp() {
        let loads = [
            (Backend::Cpu, None),
            (Backend::Vulkan, Some(cpu_cores(8, 8))),
            (Backend::Cuda, Some(cpu_cores(8, 8))),
            (Backend::Metal, Some(cpu_cores(4, 6))),
        ];

        let threads = loads.map(|(backend, cores)| threads_for(backend, cores));

        assert_eq!(threads, [None; 4]);
    }

    #[test]
    fn context_runs_the_cpu_threads() {
        let threads = CpuThreads {
            prompt: 8,
            generation: 4,
        };

        let params = context_params(CONTEXT_TOKENS.0, Some(threads));

        assert_eq!((params.n_threads_batch(), params.n_threads()), (8, 4));
    }

    #[test]
    fn context_without_cpu_threads_keeps_the_threads_of_llama_cpp() {
        let defaults = LlamaContextParams::default();

        let params = context_params(CONTEXT_TOKENS.0, None);

        assert_eq!(
            (params.n_threads_batch(), params.n_threads()),
            (defaults.n_threads_batch(), defaults.n_threads())
        );
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
        let mut llama = None;
        start_backend(&mut llama).expect("llama.cpp starts");
        let devices = list_llama_ggml_backend_devices();
        let device = gpu_device(&devices, Backend::Vulkan)
            .or_else(|| gpu_device(&devices, Backend::Cuda))
            .expect("the build has a GPU backend and the computer has a GPU");

        let verdict = check_fit(&path, device.index, ALL_MEMORY_BYTES);

        assert_eq!(verdict, Err(InferenceError::ModelTooLarge));
    }

    #[test]
    fn healing_takes_the_most_likely_agreeing_token_and_the_lowest_id_of_a_tie() {
        let pieces = TokenPieces::new(
            [" be", " beauti", " beautiful", " cat"]
                .iter()
                .zip(0..)
                .map(|(piece, id)| (LlamaToken(id), piece.as_bytes().to_vec())),
        );

        let token = best_token_agreeing_with(&[1.0, 3.0, 3.0, 9.0], &pieces, b" beauti");

        assert_eq!(token, Some(LlamaToken(1)));
    }

    #[test]
    fn written_piece_leaves_the_rest_unwritten() {
        let cases = [
            (&b" beauti"[..], &b" beaut"[..], &b"i"[..]),
            (b" beauti", b" beautiful", b""),
        ];

        let rests = cases.map(|(unwritten, piece, _)| unwritten_after(unwritten, piece));

        assert_eq!(rests, cases.map(|(_, _, rest)| rest));
    }

    fn sink() -> InferenceEventSink {
        Arc::new(|_| {})
    }

    fn complete_request() -> InferenceRequest {
        InferenceRequest::Complete {
            prompt: Prompt("Dear".to_owned()),
            max_tokens: TokenCount(4),
            word_bias: None,
            min_log_probability: None,
        }
    }

    fn has_newer_completion(
        waiting: InferenceRequest,
        waiting_sink: &InferenceEventSink,
        running_sink: &InferenceEventSink,
    ) -> bool {
        let (sender, jobs) = mpsc::channel();
        sender
            .send(Job {
                request: waiting,
                sink: Arc::clone(waiting_sink),
            })
            .expect("the queue receives jobs");
        JobQueue::new(&jobs).has_newer_completion(running_sink)
    }

    #[test]
    fn only_a_waiting_completion_for_the_same_sink_is_newer() {
        let running = sink();
        let other = sink();
        let cases = [
            (complete_request(), &running, true),
            (complete_request(), &other, false),
            (InferenceRequest::Unload, &running, false),
        ];

        let newer = cases
            .clone()
            .map(|(request, waiting_sink, _)| has_newer_completion(request, waiting_sink, &running));

        assert_eq!(newer, cases.map(|(_, _, is_newer)| is_newer));
    }

    #[test]
    fn empty_queue_has_no_newer_completion() {
        let (_sender, jobs) = mpsc::channel();

        let is_newer = JobQueue::new(&jobs).has_newer_completion(&sink());

        assert!(!is_newer);
    }

    #[test]
    fn job_that_was_looked_at_comes_next() {
        let (sender, jobs) = mpsc::channel();
        sender
            .send(Job {
                request: complete_request(),
                sink: sink(),
            })
            .expect("the queue receives jobs");
        let mut queue = JobQueue::new(&jobs);
        queue.has_newer_completion(&sink());

        let next = queue.next().map(|job| job.request);

        assert_eq!(next, Some(complete_request()));
    }

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

    #[test]
    fn bias_adds_to_the_logits_of_its_tokens() {
        let mut biased = Vec::new();

        let scores = with_bias(
            &[1.0, 1.2, 1.4],
            &[(LlamaToken(0), 2.5), (LlamaToken(9), 1.0)],
            &mut biased,
        );

        assert_eq!(scores, [3.5, 1.2, 1.4]);
    }

    #[test]
    fn bias_stays_out_when_the_model_is_sure_of_its_first_choice() {
        let mut biased = Vec::new();

        let scores = with_bias(&[9.0, 1.0, 1.0], &[(LlamaToken(1), 20.0)], &mut biased);

        assert_eq!(scores, [9.0, 1.0, 1.0]);
    }

    #[test]
    fn top_probability_is_the_softmax_of_the_highest_logit() {
        let probabilities = [&[0.0, 0.0][..], &[0.0, 0.0, 0.0, 0.0], &[5.0]].map(top_probability);

        assert_eq!(probabilities, [0.5, 0.25, 1.0]);
    }

    #[test]
    fn no_bias_keeps_the_logits() {
        let mut biased = Vec::new();

        let scores = with_bias(&[1.0, 2.0], &[], &mut biased);

        assert_eq!(scores, [1.0, 2.0]);
    }

    #[test]
    fn most_likely_token_has_the_highest_logit() {
        let token = most_likely(&[0.5, 2.0, -1.0]);

        assert_eq!(token, LlamaToken(1));
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-5, "{actual} is not {expected}");
    }

    #[test]
    fn log_probability_is_the_log_of_the_softmax_share() {
        let log_probability = log_probability_of(&[0.0, 2.0_f32.ln(), 0.0], LlamaToken(1));

        assert_near(log_probability, 0.5_f32.ln());
    }

    #[test]
    fn healing_log_probability_is_the_share_among_the_agreeing_tokens() {
        let logits = [3.0_f32.ln(), 1.0_f32.ln(), 5.0_f32.ln()];

        let log_probability =
            agreeing_log_probability(&logits, LlamaToken(0), [LlamaToken(0), LlamaToken(1)].into_iter());

        assert_near(log_probability, 0.75_f32.ln());
    }

    #[test]
    fn token_after_the_end_of_a_word_starts_the_next_word() {
        let mut words = WordLogProbabilities::default();

        for (text, log_probability) in [("very", -1.0), (" soon", -2.0), (",", -0.5), (" thanks", -3.0)] {
            words.add(text.as_bytes(), log_probability);
        }

        assert_eq!(
            words.into_vec(),
            [LogProbability(-1.0), LogProbability(-2.5), LogProbability(-3.0)]
        );
    }

    #[test]
    fn token_that_only_writes_the_healed_text_belongs_to_the_first_word() {
        let mut words = WordLogProbabilities::default();

        for (text, log_probability) in [("", -0.5), (" meeting", -1.0), (" at", -2.0)] {
            words.add(text.as_bytes(), log_probability);
        }

        assert_eq!(words.into_vec(), [LogProbability(-1.5), LogProbability(-2.0)]);
    }

    #[test]
    fn whitespace_after_whitespace_stays_in_its_word() {
        let mut words = WordLogProbabilities::default();

        for (text, log_probability) in [("Hi", -1.0), (" ", -1.0), (" there", -1.0)] {
            words.add(text.as_bytes(), log_probability);
        }

        assert_eq!(words.into_vec(), [LogProbability(-1.0), LogProbability(-2.0)]);
    }

    fn words_of(tokens: &[(&str, f32)]) -> WordLogProbabilities {
        let mut words = WordLogProbabilities::default();
        for (text, log_probability) in tokens {
            words.add(text.as_bytes(), *log_probability);
        }
        words
    }

    #[test]
    fn cut_never_stops_the_first_word() {
        let words = words_of(&[("", -0.5), (" meet", -2.0)]);

        let is_kept = words.keeps(b"ing", -4.0, LogProbability(-3.0));

        assert!(is_kept);
    }

    #[test]
    fn cut_keeps_a_later_token_while_the_text_stays_as_likely_as_the_cut() {
        let words = words_of(&[("very", -1.0)]);

        let kept = [(&b" soon"[..], -2.0), (b" soon", -2.5)]
            .map(|(text, log_probability)| words.keeps(text, log_probability, LogProbability(-3.0)));

        assert_eq!(kept, [true, false]);
    }

    #[test]
    fn stop_before_a_new_word_keeps_all_words() {
        let mut words = words_of(&[("very", -1.0), (" soon", -0.5)]);

        let dropped = words.end_before(b" thanks");

        assert_eq!(
            (dropped, words.into_vec()),
            (0, vec![LogProbability(-1.0), LogProbability(-0.5)])
        );
    }

    #[test]
    fn stop_inside_a_word_drops_that_word() {
        let mut words = words_of(&[("very", -1.0), (" th", -0.5), ("an", -0.5)]);

        let dropped = words.end_before(b"ks");

        assert_eq!((dropped, words.into_vec()), (2, vec![LogProbability(-1.0)]));
    }

    #[test]
    fn text_after_the_healed_text_is_what_a_token_adds() {
        let cases = [
            (&b" world"[..], &b" wor"[..], &b"ld"[..]),
            (b" wo", b" wor", b""),
            (b" world", b"", b" world"),
        ];

        let added = cases.map(|(piece, unwritten, _)| added_after(piece, unwritten));

        assert_eq!(added, cases.map(|(_, _, added)| added));
    }

    #[test]
    fn only_a_line_break_after_the_healed_text_ends_the_line() {
        let cases = [
            (&b"\n"[..], &b""[..], true),
            (b"ld\nHi", b"ld", true),
            (b" world", b"", false),
            (b"\nHi", b"\nHi", false),
            (b"\nHi", b"\nHi there", false),
        ];

        let ends = cases.map(|(piece, unwritten, _)| writes_line_break(piece, unwritten));

        assert_eq!(ends, cases.map(|(_, _, ends)| ends));
    }

    #[test]
    fn only_a_piece_with_leading_whitespace_starts_a_word() {
        let cases = [
            (&b" beaut"[..], true),
            (b"\nHi", true),
            (b"i", false),
            (b"", false),
        ];

        let starts = cases.map(|(piece, _)| starts_word(piece));

        assert_eq!(starts, cases.map(|(_, starts)| starts));
    }

    #[test]
    fn text_after_the_healed_prefix_is_kept() {
        let text = text_after_prefix(b" world, hi", b" wor");

        assert_eq!(text, b"ld, hi");
    }

    #[test]
    fn text_that_does_not_start_with_the_healed_prefix_is_dropped() {
        let text = text_after_prefix(b" planet", b" wor");

        assert_eq!(text, b"");
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
