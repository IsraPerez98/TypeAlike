//! Model files for TypeAlike: Hugging Face search, downloads and the local models directory.
//!
//! [`HuggingFace`] implements the core [`ModelHub`] seam. Network requests and the scan of the
//! models directory run on their own threads and report through the caller's sink. Thus the
//! network never blocks the caller's thread. A cancel and a delete finish at once on the caller's
//! thread. The drafter of a model ([`ModelFile::drafter`]) downloads and deletes with the model. A
//! cancel after the model file is complete stops only the download of the drafter, and the model
//! stays. A download of a file that already downloads does not start.

mod api;
mod store;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use typealike_core::hub::{HubError, HubEvent, HubEventSink, HubRequest, ModelFile, ModelHub};

use crate::api::FetchError;

/// Finds GGUF models on Hugging Face and stores downloads in one local directory.
#[derive(Debug)]
pub struct HuggingFace {
    agent: ureq::Agent,
    models_dir: PathBuf,
    cancel_flags: HashMap<ModelFile, Arc<AtomicBool>>,
    running: RunningDownloads,
}

impl HuggingFace {
    /// Creates a hub that keeps downloaded models in `models_dir`, created on first download.
    #[must_use]
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            agent: api::new_agent(),
            models_dir,
            cancel_flags: HashMap::new(),
            running: RunningDownloads::default(),
        }
    }

    fn start_download(&mut self, file: ModelFile, sink: HubEventSink) {
        let Some(claim) = self.running.claim(&file) else {
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_flags.insert(file.clone(), Arc::clone(&cancel));
        let agent = self.agent.clone();
        let models_dir = self.models_dir.clone();
        let running = self.running.clone();
        thread::spawn(move || {
            let report_progress = |received| {
                sink(HubEvent::DownloadProgress {
                    file: file.clone(),
                    received,
                });
            };
            let result = api::download(&agent, &models_dir, &file, &cancel, report_progress);
            if result.is_ok()
                && let Some(drafter) = file.drafter()
                && let Some(_drafter_claim) = running.claim(&drafter)
            {
                download_drafter(&agent, &models_dir, &drafter, &cancel);
            }
            drop(claim);
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
                if let Err(error) = store::delete_with_drafter(&self.models_dir, &file) {
                    tracing::warn!(%error, "failed to delete a model file");
                }
            }
        }
    }
}

/// Downloads `drafter` into `models_dir` when it is not stored yet. The model runs without its
/// drafter, so a failure only goes to the log.
fn download_drafter(agent: &ureq::Agent, models_dir: &Path, drafter: &ModelFile, cancel: &AtomicBool) {
    if store::file_path(models_dir, drafter).is_some_and(|path| path.is_file()) {
        return;
    }
    if let Err(error) = api::download(agent, models_dir, drafter, cancel, |_| {})
        && !matches!(error, FetchError::Canceled)
    {
        tracing::warn!(?error, "failed to download the drafter of a model");
    }
}

/// The files that download now, shared by the download threads.
#[derive(Debug, Clone, Default)]
struct RunningDownloads(Arc<Mutex<HashSet<ModelFile>>>);

impl RunningDownloads {
    /// Marks `file` as downloading until the returned claim drops, or returns `None` when `file`
    /// already downloads.
    fn claim(&self, file: &ModelFile) -> Option<DownloadClaim> {
        let is_new = self.files().insert(file.clone());
        is_new.then(|| DownloadClaim {
            running: self.clone(),
            file: file.clone(),
        })
    }

    fn files(&self) -> MutexGuard<'_, HashSet<ModelFile>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A file that downloads until this claim drops.
#[derive(Debug)]
struct DownloadClaim {
    running: RunningDownloads,
    file: ModelFile,
}

impl Drop for DownloadClaim {
    fn drop(&mut self) {
        self.running.files().remove(&self.file);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn drafter() -> ModelFile {
        ModelFile {
            repo: "unsloth/gemma-4-E4B-it-qat-GGUF".to_owned(),
            name: "mtp-gemma-4-E4B-it.gguf".to_owned(),
        }
    }

    #[test]
    fn file_that_downloads_does_not_start_a_second_download() {
        let running = RunningDownloads::default();
        let _first = running.claim(&drafter());

        let second = running.claim(&drafter());

        assert!(second.is_none());
    }

    #[test]
    fn file_downloads_again_after_its_download_ends() {
        let running = RunningDownloads::default();
        drop(running.claim(&drafter()));

        let second = running.claim(&drafter());

        assert!(second.is_some());
    }
}
