//! Language model runner for GhostType: loads GGUF models and completes text with llama.cpp.
//!
//! [`LlamaCpp`] implements the core [`ModelRunner`] seam. One worker thread owns llama.cpp and the
//! loaded model and runs requests in order, so the caller's thread never waits for a model.

mod worker;

use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;

use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{InferenceEventSink, InferenceRequest, ModelRunner};

use crate::worker::Job;

/// Runs one GGUF model at a time with llama.cpp, on the backend that each load request names.
/// A GPU backend runs only when the build includes it (the `vulkan` and `cuda` features) and a
/// GPU supports it, and only when the whole model fits in the free memory of that GPU.
#[derive(Debug)]
pub struct LlamaCpp {
    jobs: mpsc::Sender<Job>,
}

impl LlamaCpp {
    /// Starts the worker thread. It finds each model file at the path `locate_model` returns, and
    /// starts llama.cpp only when the first model loads.
    #[must_use]
    pub fn new(locate_model: impl Fn(&ModelFile) -> Option<PathBuf> + Send + 'static) -> Self {
        let (jobs, received_jobs) = mpsc::channel();
        thread::spawn(move || worker::run(&locate_model, &received_jobs));
        Self { jobs }
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

    use ghosttype_core::inference::{InferenceError, InferenceEvent, Prompt, TokenCount};

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
            backend: None,
        }
    }

    #[test]
    fn model_without_a_path_is_missing() {
        let runner = LlamaCpp::new(|_| None);

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
        let runner = LlamaCpp::new(|_| Some(PathBuf::from("/nonexistent/ghosttype/model.gguf")));

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
        let path = std::env::temp_dir().join(format!("ghosttype-llm-{}-damaged.gguf", std::process::id()));
        fs::write(&path, b"not a model").expect("temp dir is writable");
        let located = path.clone();
        let runner = LlamaCpp::new(move |_| Some(located.clone()));

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
    fn completing_without_a_model_fails() {
        let runner = LlamaCpp::new(|_| None);

        let event = first_event(
            runner,
            InferenceRequest::Complete {
                prompt: Prompt("Dear".to_owned()),
                max_tokens: TokenCount(4),
            },
        );

        assert_eq!(event, InferenceEvent::Completed(Err(InferenceError::NoModel)));
    }
}
