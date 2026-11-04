//! The thread that owns llama.cpp: the backend, the loaded model and each completion.

mod correction;
mod drafter;
mod generation;
mod loaded_model;
mod logits;
mod model_params;
mod pieces;
mod prompt_cache;
#[cfg(test)]
mod test_support;
mod words;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::{LlamaCppError, LlamaModelLoadError, LogOptions, send_logs_to_tracing};
use typealike_core::hub::{ByteSize, ModelFile};
use typealike_core::inference::{
    Backend, CpuCores, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest,
};

use crate::worker::drafter::is_drafter_of;
use crate::worker::generation::{Limits, Progress};
use crate::worker::loaded_model::LoadedModel;
pub(crate) use crate::worker::model_params::PromptBatch;
use crate::worker::model_params::{
    ContextSetup, DrafterUse, WarmUp, drafter_params, free_gpu_memory, log_devices, model_params, threads_for,
};

/// The time without a request after which the worker frees the context of a model on a GPU: its
/// cache of the last prompt and its compute buffers. The next request makes the context again.
const CONTEXT_IDLE_TIMEOUT: Duration = Duration::from_mins(1);

/// A request and where its event goes.
pub(crate) struct Job {
    pub(crate) request: InferenceRequest,
    pub(crate) sink: InferenceEventSink,
}

/// What a wait for the next job with a time limit gives.
enum Waited {
    Job(Job),
    /// No job came before the time limit.
    Idle,
    /// Every sender is gone.
    Closed,
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

    /// Waits at most `limit` for the next job.
    fn next_within(&mut self, limit: Duration) -> Waited {
        if let Some(job) = self.next_job.take() {
            return Waited::Job(job);
        }
        match self.jobs.recv_timeout(limit) {
            Ok(job) => Waited::Job(job),
            Err(RecvTimeoutError::Timeout) => Waited::Idle,
            Err(RecvTimeoutError::Disconnected) => Waited::Closed,
        }
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
                let drafter_path = drafter_path(&file, backend, locate_model);
                let loaded = load_with_drafter(
                    &mut llama,
                    locate_model(&file),
                    drafter_path,
                    backend,
                    batch_tokens,
                )
                .inspect(|_| tracing::info!(duration = ?started.elapsed(), "loaded the model"))
                .inspect_err(|error| tracing::warn!(?error, "failed to load the model"));
                match (loaded, &llama) {
                    (Ok((model, drafter)), Some(llama)) => {
                        let setup = ContextSetup {
                            cpu_threads: threads_for(backend, cpu_cores),
                            batch_tokens,
                            warm_up: WarmUp::on(backend),
                        };
                        let mut loaded_model = LoadedModel::new(llama, &model, drafter.as_ref(), setup);
                        loaded_model.warm_up(&mut || queue.has_waiting_job());
                        sink(InferenceEvent::Loaded { file, result: Ok(()) });
                        serve(&mut loaded_model, &mut queue, context_idle_timeout(backend));
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

/// Loads the model at `path` on `backend` without a drafter, as [`load_with_drafter`] does.
#[cfg(test)]
fn load(
    llama: &mut Option<LlamaBackend>,
    path: Option<PathBuf>,
    backend: Backend,
    batch_tokens: u32,
) -> Result<LlamaModel, InferenceError> {
    load_with_drafter(llama, path, None, backend, batch_tokens).map(|(model, _)| model)
}

/// Loads the model at `path` on `backend`, for a context that decodes `batch_tokens` at a time. On
/// another thread at the same time, it loads the drafter at `drafter_path`, so that the drafter
/// adds little time to the load. It starts llama.cpp in `llama` first, when it did not start
/// before. The model runs without the drafter when the drafter fails to load or does not fit it.
fn load_with_drafter(
    llama: &mut Option<LlamaBackend>,
    path: Option<PathBuf>,
    drafter_path: Option<PathBuf>,
    backend: Backend,
    batch_tokens: u32,
) -> Result<(LlamaModel, Option<LlamaModel>), InferenceError> {
    let path = path
        .filter(|path| path.is_file())
        .ok_or(InferenceError::ModelMissing)?;
    let llama = start_backend(llama)?;
    let params = model_params(&path, backend, batch_tokens)?;
    thread::scope(|scope| {
        let loading_drafter =
            drafter_path.map(|drafter_path| scope.spawn(move || load_drafter(llama, &drafter_path, backend)));
        let model =
            LlamaModel::load_from_file(llama, &path, &params).map_err(|error: LlamaModelLoadError| {
                tracing::warn!(%error, "failed to load a model");
                InferenceError::ModelInvalid
            })?;
        let drafter = loading_drafter
            .and_then(|loading| loading.join().ok().flatten())
            .filter(|drafter| {
                let fits = is_drafter_of(drafter, &model);
                if !fits {
                    tracing::warn!("the drafter does not fit the model, so the model runs without it");
                }
                fits
            });
        Ok((model, drafter))
    })
}

/// Returns the path of the drafter of the model of `file`, when the model runs with a drafter on
/// `backend` and `locate_model` finds the file of the drafter.
fn drafter_path(
    file: &ModelFile,
    backend: Backend,
    locate_model: &dyn Fn(&ModelFile) -> Option<PathBuf>,
) -> Option<PathBuf> {
    if DrafterUse::on(backend) == DrafterUse::Off {
        return None;
    }
    locate_model(&file.drafter()?).filter(|path| path.is_file())
}

/// Loads the drafter at `path` on `backend`. Returns `None` when it fails to load, so that the
/// model runs without it.
fn load_drafter(llama: &LlamaBackend, path: &Path, backend: Backend) -> Option<LlamaModel> {
    let params = drafter_params(backend).ok()?;
    let started = Instant::now();
    let drafter = LlamaModel::load_from_file(llama, path, &params)
        .inspect_err(
            |error| tracing::warn!(%error, "failed to load the drafter, so the model runs without it"),
        )
        .ok()?;
    tracing::info!(duration = ?started.elapsed(), "loaded the drafter");
    Some(drafter)
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
/// before the next model loads. It frees the context when no job comes for `idle_timeout`.
fn serve(loaded: &mut LoadedModel<'_>, queue: &mut JobQueue<'_>, idle_timeout: Option<Duration>) {
    while let Some(job) = next_job_for(loaded, queue, idle_timeout) {
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

/// Returns the time without a job after which the worker frees the context of a model on
/// `backend`, or `None` when it keeps the context. On the CPU, the next completion would decode
/// its whole prompt again, which takes seconds on a slow CPU, and the context holds little memory.
fn context_idle_timeout(backend: Backend) -> Option<Duration> {
    match backend {
        Backend::Vulkan | Backend::Cuda | Backend::Metal => Some(CONTEXT_IDLE_TIMEOUT),
        Backend::Cpu => None,
    }
}

/// Waits for the next job for `loaded`, and frees the context of `loaded` when no job comes for
/// `idle_timeout`. Returns `None` when every sender is gone.
fn next_job_for(
    loaded: &mut LoadedModel<'_>,
    queue: &mut JobQueue<'_>,
    idle_timeout: Option<Duration>,
) -> Option<Job> {
    let Some(idle_timeout) = idle_timeout else {
        return queue.next();
    };
    match queue.next_within(idle_timeout) {
        Waited::Job(job) => Some(job),
        Waited::Closed => None,
        Waited::Idle => {
            loaded.free_context();
            queue.next()
        }
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
    fn only_gpu_backends_free_the_context_of_an_idle_model() {
        let backends = [Backend::Metal, Backend::Vulkan, Backend::Cuda, Backend::Cpu];

        let timeouts = backends.map(context_idle_timeout);

        assert_eq!(
            timeouts,
            [
                Some(CONTEXT_IDLE_TIMEOUT),
                Some(CONTEXT_IDLE_TIMEOUT),
                Some(CONTEXT_IDLE_TIMEOUT),
                None,
            ]
        );
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
    fn job_in_the_queue_comes_within_the_limit() {
        let (sender, jobs) = mpsc::channel();
        sender
            .send(Job {
                request: complete_request(),
                sink: sink(),
            })
            .expect("the queue receives jobs");

        let waited = JobQueue::new(&jobs).next_within(Duration::ZERO);

        assert!(matches!(
            waited,
            Waited::Job(Job {
                request: InferenceRequest::Complete { .. },
                ..
            })
        ));
    }

    #[test]
    fn job_that_was_looked_at_comes_within_the_limit() {
        let (sender, jobs) = mpsc::channel();
        sender
            .send(Job {
                request: InferenceRequest::Unload,
                sink: sink(),
            })
            .expect("the queue receives jobs");
        let mut queue = JobQueue::new(&jobs);
        queue.has_waiting_job();

        let waited = queue.next_within(Duration::ZERO);

        assert!(matches!(
            waited,
            Waited::Job(Job {
                request: InferenceRequest::Unload,
                ..
            })
        ));
    }

    #[test]
    fn empty_queue_is_idle_after_the_limit() {
        let (_sender, jobs) = mpsc::channel();

        let waited = JobQueue::new(&jobs).next_within(Duration::ZERO);

        assert!(matches!(waited, Waited::Idle));
    }

    #[test]
    fn queue_without_senders_is_closed() {
        let (sender, jobs) = mpsc::channel::<Job>();
        drop(sender);

        let waited = JobQueue::new(&jobs).next_within(Duration::ZERO);

        assert!(matches!(waited, Waited::Closed));
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
