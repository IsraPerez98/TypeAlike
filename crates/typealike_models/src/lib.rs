//! Model files for TypeAlike: Hugging Face search, downloads and the local models directory.
//!
//! [`HuggingFace`] implements the core [`ModelHub`] seam. Network requests and the scan of the
//! models directory run on their own threads and report through the caller's sink. Thus the
//! network never blocks the caller's thread. A cancel and a delete finish at once on the caller's
//! thread.

mod api;
mod store;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use typealike_core::hub::{HubError, HubEvent, HubEventSink, HubRequest, ModelFile, ModelHub};

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
        match request {
            HubRequest::ListInstalled => {
                let models_dir = self.models_dir.clone();
                thread::spawn(move || match store::installed_files(&models_dir) {
                    Ok(files) => sink(HubEvent::Installed(files)),
                    Err(error) => tracing::warn!(%error, "failed to list downloaded models"),
                });
            }
            HubRequest::Search { query } => {
                let agent = self.agent.clone();
                thread::spawn(move || {
                    let result = to_hub_result(api::search(&agent, &query), "search");
                    sink(HubEvent::SearchFinished { query, result });
                });
            }
            HubRequest::ListFiles { repo } => {
                let agent = self.agent.clone();
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
                if let Err(error) = store::delete(&self.models_dir, &file) {
                    tracing::warn!(%error, "failed to delete a model file");
                }
            }
        }
    }
}

/// Returns where the downloaded `file` is stored under `models_dir`, or `None` when its repository
/// or name could escape the directory.
#[must_use]
pub fn stored_path(models_dir: &Path, file: &ModelFile) -> Option<PathBuf> {
    store::file_path(models_dir, file)
}

fn to_hub_result<T>(result: Result<T, FetchError>, operation: &'static str) -> Result<T, HubError> {
    result.map_err(|error| {
        if !matches!(error, FetchError::Canceled) {
            tracing::warn!(?error, operation, "hugging face request failed");
        }
        error.to_hub_error()
    })
}
