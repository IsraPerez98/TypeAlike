//! Language model runner for TypeAlike: loads GGUF models and completes text with llama.cpp.
//!
//! [`LlamaCpp`] implements the core [`ModelRunner`] seam. One worker thread owns llama.cpp and the
//! loaded model and runs requests in order, so the caller's thread never waits for a model.

mod worker;

use std::mem;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;

use typealike_core::hub::ModelFile;
use typealike_core::inference::{Backend, CpuCores, InferenceEventSink, InferenceRequest, ModelRunner};

use crate::worker::{Job, PromptBatch};

/// The backend that runs models in each build, also without the GPU features: Metal on macOS, else
/// the CPU.
pub const BUILT_IN_BACKEND: Backend = if cfg!(target_os = "macos") {
    Backend::Metal
} else {
    Backend::Cpu
};

/// The backend that a new user starts with: the GPU backend of the build (CUDA, else Vulkan), else
/// [`BUILT_IN_BACKEND`].
pub const DEFAULT_BACKEND: Backend = if cfg!(feature = "cuda") {
    Backend::Cuda
} else if cfg!(feature = "vulkan") {
    Backend::Vulkan
} else {
    BUILT_IN_BACKEND
};

/// Runs one GGUF model at a time with llama.cpp, on the backend that each load request names.
/// A GPU backend runs only when the build includes it (the `vulkan` and `cuda` features) and a
/// GPU supports it, and only when the whole model fits in the free memory of that GPU.
///
/// A drop waits until the worker finishes the submitted requests and frees the model. On Metal,
/// llama.cpp stops the process with an assert when the process exits while a model is loaded.
#[derive(Debug)]
pub struct LlamaCpp {
    jobs: mpsc::Sender<Job>,
    worker: Option<thread::JoinHandle<()>>,
}

impl LlamaCpp {
    /// Starts the worker thread. It finds each model file at the path `locate_model` returns, and
    /// starts llama.cpp only when the first model loads.
    ///
    /// A model on the CPU decodes a prompt with one thread on each performance core of
    /// `cpu_cores`, and at least 4 threads when the CPU has that many cores. It adds each token
    /// with at most 4 threads. With `None`, the CPU runs the threads of llama.cpp. Each decode of a
    /// prompt takes at most the batch of the backend that the load names.
    #[must_use]
    pub fn new(
        locate_model: impl Fn(&ModelFile) -> Option<PathBuf> + Send + 'static,
        cpu_cores: Option<CpuCores>,
    ) -> Self {
        Self::start(locate_model, cpu_cores, PromptBatch::OfBackend)
    }

    /// Starts the worker thread as [`LlamaCpp::new`] does, but each decode of a prompt takes at
    /// most `batch_tokens` on each backend. Use it to measure another batch. A newer request can
    /// stop a prompt decode only between two batches, and the GPU keeps memory for the outputs of
    /// a whole batch while the model stays loaded.
    #[must_use]
    pub fn with_prompt_batch(
        locate_model: impl Fn(&ModelFile) -> Option<PathBuf> + Send + 'static,
        cpu_cores: Option<CpuCores>,
        batch_tokens: NonZeroU32,
    ) -> Self {
        Self::start(locate_model, cpu_cores, PromptBatch::Fixed(batch_tokens))
    }

    fn start(
        locate_model: impl Fn(&ModelFile) -> Option<PathBuf> + Send + 'static,
        cpu_cores: Option<CpuCores>,
        prompt_batch: PromptBatch,
    ) -> Self {
        let (jobs, received_jobs) = mpsc::channel();
        let worker =
            thread::spawn(move || worker::run(&locate_model, cpu_cores, prompt_batch, &received_jobs));
        Self {
            jobs,
            worker: Some(worker),
        }
    }
}

impl Drop for LlamaCpp {
    fn drop(&mut self) {
        let (closed_queue, _) = mpsc::channel();
        drop(mem::replace(&mut self.jobs, closed_queue));
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::warn!("the model worker panicked");
        }
    }
}

impl ModelRunner for LlamaCpp {
    fn submit(&mut self, request: InferenceRequest, sink: &InferenceEventSink) {
        let job = Job {
            request,
            sink: Arc::clone(sink),
        };
        if self.jobs.send(job).is_err() {
            tracing::warn!("the model worker stopped");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use typealike_core::inference::{
        InferenceError, InferenceEvent, NextWordBias, Prompt, SentenceEnd, TokenCount, TypoCheck,
    };

    use super::*;

    fn model_file() -> ModelFile {
        ModelFile {
            repo: "owner/repo".to_owned(),
            name: "model.gguf".to_owned(),
        }
    }

    fn first_event(mut runner: LlamaCpp, request: InferenceRequest) -> InferenceEvent {
        let (sender, events) = mpsc::channel();
        let sink: InferenceEventSink = Arc::new(move |event| {
            sender.send(event).ok();
        });
        runner.submit(request, &sink);
        drop(sink);
        events.recv().expect("the worker answers every request")
    }

    fn load_request() -> InferenceRequest {
        InferenceRequest::Load {
            file: model_file(),
            backend: BUILT_IN_BACKEND,
        }
    }

    #[test]
    fn model_without_a_path_is_missing() {
        let runner = LlamaCpp::new(|_| None, None);

        let event = first_event(runner, load_request());

        assert_eq!(
            event,
            InferenceEvent::Loaded {
                file: model_file(),
                result: Err(InferenceError::ModelMissing),
            }
        );
    }

    #[test]
    fn model_whose_file_is_gone_is_missing() {
        let runner = LlamaCpp::new(|_| Some(PathBuf::from("/nonexistent/typealike/model.gguf")), None);

        let event = first_event(runner, load_request());

        assert_eq!(
            event,
            InferenceEvent::Loaded {
                file: model_file(),
                result: Err(InferenceError::ModelMissing),
            }
        );
    }

    #[test]
    fn damaged_model_file_does_not_load() {
        let path = std::env::temp_dir().join(format!("typealike-llm-{}-damaged.gguf", std::process::id()));
        fs::write(&path, b"not a model").expect("temp dir is writable");
        let located = path.clone();
        let runner = LlamaCpp::new(move |_| Some(located.clone()), None);

        let event = first_event(runner, load_request());

        fs::remove_file(Path::new(&path)).ok();
        assert_eq!(
            event,
            InferenceEvent::Loaded {
                file: model_file(),
                result: Err(InferenceError::ModelInvalid),
            }
        );
    }

    #[test]
    fn unloading_answers_with_no_event() {
        let mut runner = LlamaCpp::new(|_| None, None);
        let (sender, events) = mpsc::channel();
        let sink: InferenceEventSink = Arc::new(move |event| {
            sender.send(event).ok();
        });

        runner.submit(InferenceRequest::Unload, &sink);
        drop(sink);
        drop(runner);

        assert_eq!(events.recv().ok(), None);
    }

    #[test]
    fn dropping_the_runner_waits_until_its_worker_stops() {
        let (worker_alive, worker_signals) = mpsc::channel();
        let mut runner = LlamaCpp::new(
            move |_| {
                worker_alive.send(()).ok();
                None
            },
            None,
        );
        let sink: InferenceEventSink = Arc::new(|_| {});
        runner.submit(load_request(), &sink);

        drop(runner);

        assert_eq!(
            (worker_signals.try_recv(), worker_signals.try_recv()),
            (Ok(()), Err(mpsc::TryRecvError::Disconnected))
        );
    }

    #[test]
    fn completing_without_a_model_fails() {
        let runner = LlamaCpp::new(|_| None, None);

        let event = first_event(
            runner,
            InferenceRequest::Complete {
                prompt: Prompt("Dear".to_owned()),
                max_tokens: TokenCount(4),
                word_bias: None,
                next_word_bias: NextWordBias::default(),
                min_log_probability: None,
                typo_check: TypoCheck::Off,
                sentence_end: SentenceEnd::Continue,
                line_after: String::new(),
            },
        );

        assert_eq!(event, InferenceEvent::Completed(Err(InferenceError::NoModel)));
    }
}
