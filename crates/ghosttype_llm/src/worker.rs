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

use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{
    Backend, CpuCores, InferenceError, InferenceEvent, InferenceEventSink, InferenceRequest,
};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::{LlamaCppError, LlamaModelLoadError, LogOptions, send_logs_to_tracing};

use crate::worker::generation::{Limits, Progress};
use crate::worker::loaded_model::LoadedModel;
use crate::worker::model_params::{CpuThreads, model_params, threads_for};
use crate::worker::pieces::TokenPieces;

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
            next_word_bias,
            min_log_probability,
            min_typed_log_probability,
            fixes_typos,
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
            min_typed_log_probability: *min_typed_log_probability,
            fixes_typos: *fixes_typos,
        };
        let token_bias = loaded.token_bias(word_bias.as_ref(), next_word_bias);
        let result = loaded.complete(prompt, limits, &token_bias, &mut progress);
        (job.sink)(InferenceEvent::Completed(result));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use ghosttype_core::inference::{NextWordBias, Prompt, TokenCount};

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
            min_typed_log_probability: None,
            fixes_typos: false,
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
}
