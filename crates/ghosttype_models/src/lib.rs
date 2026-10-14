//! Model files for GhostType: Hugging Face search, downloads and the local models directory.
//!
//! [`HuggingFace`] implements the core [`ModelHub`] seam. Every request runs on its own thread
//! and reports through the caller's sink, so the caller's thread never blocks on the network.

mod api;
mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use ghosttype_core::hub::{HubError, HubEvent, HubEventSink, HubRequest, ModelFile, ModelHub};

use crate::api::FetchError;

/// Finds GGUF models on Hugging Face and stores downloads in one local directory.
#[derive(Debug)]
pub struct HuggingFace {
    agent: ureq::Agent,
    models_dir: PathBuf,
    cancel_flags: HashMap<ModelFile, Arc<AtomicBool>>,
}

impl HuggingFace {
    /// Creates a hub that keeps downloaded models in `models_dir`, created on first download.
    #[must_use]
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            agent: api::new_agent(),
            models_dir,
            cancel_flags: HashMap::new(),
        }
    }

    fn start_download(&mut self, file: ModelFile, sink: HubEventSink) {
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_flags.insert(file.clone(), Arc::clone(&cancel));
        let agent = self.agent.clone();
        let models_dir = self.models_dir.clone();
        thread::spawn(move || {
            let report_progress = |received| {
                sink(HubEvent::DownloadProgress {
                    file: file.clone(),
                    received,
                });
            };
            let result = api::download(&agent, &models_dir, &file, &cancel, report_progress);
            sink(HubEvent::DownloadFinished {
                result: to_hub_result(result, "download"),
                file,
            });
        });
    }
}

impl ModelHub for HuggingFace {
    fn submit(&mut self, request: HubRequest, sink: &HubEventSink) {
        let sink = Arc::clone(sink);
        let agent = self.agent.clone();
        let models_dir = self.models_dir.clone();
        match request {
            HubRequest::ListInstalled => {
                thread::spawn(move || match store::installed_files(&models_dir) {
                    Ok(files) => sink(HubEvent::Installed(files)),
                    Err(error) => tracing::warn!(%error, "failed to list downloaded models"),
                });
            }
            HubRequest::Search { query } => {
                thread::spawn(move || {
                    let result = to_hub_result(api::search(&agent, &query), "search");
                    sink(HubEvent::SearchFinished { query, result });
                });
            }
            HubRequest::ListFiles { repo } => {
                thread::spawn(move || {
                    let result = to_hub_result(api::list_files(&agent, &repo), "list files");
                    sink(HubEvent::FilesListed { repo, result });
                });
            }
            HubRequest::Download { file } => self.start_download(file, sink),
            HubRequest::CancelDownload { file } => {
                if let Some(cancel) = self.cancel_flags.remove(&file) {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            HubRequest::Delete { file } => {
                if let Err(error) = store::delete(&models_dir, &file) {
                    tracing::warn!(%error, "failed to delete a model file");
                }
            }
        }
    }
}

fn to_hub_result<T>(result: Result<T, FetchError>, operation: &'static str) -> Result<T, HubError> {
    result.map_err(|error| {
        if !matches!(error, FetchError::Canceled) {
            tracing::warn!(?error, operation, "hugging face request failed");
        }
        error.to_hub_error()
    })
}
