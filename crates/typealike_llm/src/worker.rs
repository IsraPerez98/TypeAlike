//! The thread that owns llama.cpp: the backend, the loaded model and each completion.

mod correction;
mod generation;
mod loaded_model;
mod logits;
mod model_params;
mod pieces;
mod prompt_cache;
#[cfg(test)]
mod test_support;
mod words;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::{LlamaCppError, LlamaModelLoadError, LogOptions, send_logs_to_tracing};
use typealike_core::hub::{ByteSize, ModelFile};
use typealike_core::inference::{
    Backend, CpuCores, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest,
};

use crate::worker::generation::{Limits, Progress};
use crate::worker::loaded_model::LoadedModel;
pub(crate) use crate::worker::model_params::PromptBatch;
use crate::worker::model_params::{
    ContextSetup, WarmUp, free_gpu_memory, log_devices, model_params, threads_for,
};

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

    /// Whether a job waits. It keeps the job that it receives as the next job.
    fn has_waiting_job(&mut self) -> bool {
        if self.next_job.is_none() {
            self.next_job = self.jobs.try_recv().ok();
        }
        self.next_job.is_some()
    }

    /// Whether the next job, if one waits, completes text for `sink`, so that it replaces the
    /// completion for `sink` that runs now. It keeps the job that it receives as the next job.
    fn has_newer_completion(&mut self, sink: &InferenceEventSink) -> bool {
        self.has_waiting_job()
            && self.next_job.as_ref().is_some_and(|job| {
                matches!(job.request, InferenceRequest::Complete { .. }) && Arc::ptr_eq(&job.sink, sink)
            })
    }
}

/// Runs each job in order until every sender is gone. A model on the CPU runs on threads for
/// `cpu_cores`, or on the threads of llama.cpp when the cores are unknown. Each prompt decode takes
/// at most the tokens of `prompt_batch`.
pub(crate) fn run(
    locate_model: &dyn Fn(&ModelFile) -> Option<PathBuf>,
    cpu_cores: Option<CpuCores>,
    prompt_batch: PromptBatch,
    jobs: &Receiver<Job>,
) {
    let mut queue = JobQueue::new(jobs);
    let mut llama = None;
    while let Some(Job { request, sink }) = queue.next() {
        match request {
            InferenceRequest::Load { file, backend } => {
                let batch_tokens = prompt_batch.tokens_on(backend);
                tracing::info!(
                    repo = %file.repo,
                    name = %file.name,
                    ?backend,
                    batch_tokens,
                    "loading a model"
                );
                let started = Instant::now();
                let loaded = load(&mut llama, locate_model(&file), backend, batch_tokens)
                    .inspect(|_| tracing::info!(duration = ?started.elapsed(), "loaded the model"))
                    .inspect_err(|error| tracing::warn!(?error, "failed to load the model"));
                match (loaded, &llama) {
                    (Ok(model), Some(llama)) => {
                        let setup = ContextSetup {
                            cpu_threads: threads_for(backend, cpu_cores),
                            batch_tokens,
                            warm_up: WarmUp::on(backend),
                        };
                        let mut loaded_model = LoadedModel::new(llama, &model, setup);
                        loaded_model.warm_up(&mut || queue.has_waiting_job());
                        sink(InferenceEvent::Loaded { file, result: Ok(()) });
                        serve(&mut loaded_model, &mut queue);
                    }
                    (loaded, _) => sink(InferenceEvent::Loaded {
                        file,
                        result: loaded.map(drop),
                    }),
                }
            }
            InferenceRequest::Complete { .. } => {
                sink(InferenceEvent::Completed(Err(InferenceError::NoModel)));
            }
            InferenceRequest::Unload => {}
            InferenceRequest::ReadGpuMemory { backend } => {
                let result = read_gpu_memory(&mut llama, backend);
                sink(InferenceEvent::GpuMemory { backend, result });
            }
        }
    }
}

/// Loads the model at `path` on `backend`, for a context that decodes `batch_tokens` at a time. It
/// starts llama.cpp in `llama` first, when it did not start before.
fn load(
    llama: &mut Option<LlamaBackend>,
    path: Option<PathBuf>,
    backend: Backend,
    batch_tokens: u32,
) -> Result<LlamaModel, InferenceError> {
    let path = path
        .filter(|path| path.is_file())
        .ok_or(InferenceError::ModelMissing)?;
    let llama = start_backend(llama)?;
    let params = model_params(&path, backend, batch_tokens)?;
    LlamaModel::load_from_file(llama, &path, &params).map_err(|error: LlamaModelLoadError| {
        tracing::warn!(%error, "failed to load a model");
        InferenceError::ModelInvalid
    })
}

/// Reads the free memory of the GPU that runs `backend`. For a GPU backend, it starts llama.cpp in
/// `llama` first, when it did not start before.
fn read_gpu_memory(llama: &mut Option<LlamaBackend>, backend: Backend) -> Result<ByteSize, InferenceError> {
    if backend == Backend::Cpu {
        return Err(InferenceError::BackendUnavailable);
    }
    start_backend(llama)?;
    free_gpu_memory(backend)
}

fn start_backend(llama: &mut Option<LlamaBackend>) -> Result<&LlamaBackend, InferenceError> {
    if llama.is_none() {
        send_logs_to_tracing(LogOptions::default());
        let started = LlamaBackend::init().map_err(|error: LlamaCppError| {
            tracing::error!(%error, "failed to start llama.cpp");
            InferenceError::Failed
        })?;
        *llama = Some(started);
        log_devices();
    }
    Ok(llama.as_ref().expect("the backend started above"))
}

/// Runs the completions and the reads of GPU memory for the `loaded` model until a job loads or
/// unloads a model. It gives the job that loads or unloads back to `queue`, so the model is gone
/// before the next model loads.
fn serve(loaded: &mut LoadedModel<'_>, queue: &mut JobQueue<'_>) {
    while let Some(job) = queue.next() {
        let InferenceRequest::Complete {
            prompt,
            max_tokens,
            word_bias,
            next_word_bias,
            min_log_probability,
            typo_check,
            sentence_end,
            line_after,
        } = &job.request
        else {
            if let InferenceRequest::ReadGpuMemory { backend } = job.request {
                let result = free_gpu_memory(backend);
                (job.sink)(InferenceEvent::GpuMemory { backend, result });
                continue;
            }
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
            typo_check: *typo_check,
            sentence_end: *sentence_end,
            line_after: line_after.as_bytes(),
        };
        let token_bias = loaded.token_bias(word_bias.as_ref(), next_word_bias);
        let result = loaded.complete(prompt, limits, &token_bias, &mut progress);
        (job.sink)(InferenceEvent::Completed(result));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use typealike_core::inference::{NextWordBias, Prompt, SentenceEnd, TokenCount, TypoCheck};

    use super::*;

    fn sink() -> InferenceEventSink {
        Arc::new(|_| {})
    }

    fn complete_request() -> InferenceRequest {
        InferenceRequest::Complete {
            prompt: Prompt("Dear".to_owned()),
            max_tokens: TokenCount(4),
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: None,
            typo_check: TypoCheck::Off,
            sentence_end: SentenceEnd::Continue,
            line_after: String::new(),
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

    #[test]
    fn any_job_in_the_queue_waits() {
        let (sender, jobs) = mpsc::channel();
        sender
            .send(Job {
                request: InferenceRequest::Unload,
                sink: sink(),
            })
            .expect("the queue receives jobs");

        let waits = JobQueue::new(&jobs).has_waiting_job();

        assert!(waits);
    }

    #[test]
    fn empty_queue_has_no_waiting_job() {
        let (_sender, jobs) = mpsc::channel();

        let waits = JobQueue::new(&jobs).has_waiting_job();

        assert!(!waits);
    }

    #[test]
    fn waiting_job_comes_next() {
        let (sender, jobs) = mpsc::channel();
        sender
            .send(Job {
                request: complete_request(),
                sink: sink(),
            })
            .expect("the queue receives jobs");
        let mut queue = JobQueue::new(&jobs);
        queue.has_waiting_job();

        let next = queue.next().map(|job| job.request);

        assert_eq!(next, Some(complete_request()));
    }
}
