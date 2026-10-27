//! The Models pane of the settings window: suggested models, Hugging Face search and downloads.

mod catalog;
mod page;

use std::collections::BTreeMap;

use crate::hub::{
    ByteSize, HubError, HubEvent, HubRequest, ModelFile, RepoSummary, SizedFile, usable_model_files,
};
use crate::settings_window::view::{Button, ControlId, Page, SettingsInput};

/// Something the user did in the Models pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelsAction {
    /// Submitted the search field with this text.
    Search(String),
    /// Opened the file list of the repository with this id.
    ShowFiles(String),
    /// Left a repository's file list for the search results.
    BackToResults,
    /// Asked to download a file.
    Download(SizedFile),
    /// Asked to stop a running download.
    Cancel(ModelFile),
    /// Asked to delete a downloaded file.
    Delete(ModelFile),
}

pub(crate) const SEARCH: &str = "models-search";

/// State of the Models pane: running downloads, downloaded files and the current search.
///
/// ```
/// use typealike_core::hub::HubRequest;
/// use typealike_core::settings_window::models::{ModelsAction, ModelsPane};
///
/// let mut pane = ModelsPane::new();
///
/// let request = pane.on_action(ModelsAction::Search("qwen".to_owned()));
///
/// assert_eq!(request, Some(HubRequest::Search { query: "qwen".to_owned() }));
/// ```
#[derive(Debug, Default)]
pub struct ModelsPane {
    files: BTreeMap<ModelFile, TrackedFile>,
    search: Option<Search>,
}

#[derive(Debug, Clone)]
struct TrackedFile {
    size: ByteSize,
    state: FileState,
}

#[derive(Debug, Clone, Copy)]
enum FileState {
    Downloading { received: ByteSize },
    Canceling,
    Installed,
    Failed(HubError),
}

#[derive(Debug)]
struct Search {
    query: String,
    results: Fetch<Vec<RepoSummary>>,
    open_repo: Option<OpenRepo>,
}

#[derive(Debug)]
struct OpenRepo {
    id: String,
    files: Fetch<Vec<SizedFile>>,
}

#[derive(Debug)]
enum Fetch<T> {
    Pending,
    Done(T),
    Failed(HubError),
}

impl<T> From<Result<T, HubError>> for Fetch<T> {
    fn from(result: Result<T, HubError>) -> Self {
        result.map_or_else(Self::Failed, Self::Done)
    }
}

/// The buttons that a page shows, each with the action that a click on it runs.
#[derive(Debug, Default)]
struct Buttons(Vec<(ControlId, ModelsAction)>);

impl Buttons {
    /// Returns a button that runs `action`, and keeps the action for [`Self::action_of`].
    fn add(&mut self, label: &'static str, action: ModelsAction) -> Button {
        let id = button_id(&action);
        self.0.push((id.clone(), action));
        Button {
            id,
            label,
            is_enabled: true,
        }
    }

    fn action_of(self, id: &ControlId) -> Option<ModelsAction> {
        self.0
            .into_iter()
            .find(|(button, _)| button == id)
            .map(|(_, action)| action)
    }
}

impl ModelsPane {
    /// Creates a pane that knows of no downloaded files and has no search.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the request to send each time the pane opens.
    #[must_use]
    pub fn on_open(&self) -> HubRequest {
        HubRequest::ListInstalled
    }

    /// Handles a user action. Returns the request that it needs, if any.
    pub fn on_action(&mut self, action: ModelsAction) -> Option<HubRequest> {
        match action {
            ModelsAction::Search(query) => self.start_search(query.trim()),
            ModelsAction::ShowFiles(repo) => self.open_repo(repo),
            ModelsAction::BackToResults => self.back_to_results(),
            ModelsAction::Download(sized) => self.start_download(sized),
            ModelsAction::Cancel(file) => self.cancel_download(file),
            ModelsAction::Delete(file) => self.delete(file),
        }
    }

    /// Returns the action of `input`, when `input` is for a control of the pane: the search field
    /// or a button that the pane shows now.
    #[must_use]
    pub fn action_for(&self, input: &SettingsInput) -> Option<ModelsAction> {
        match input {
            SettingsInput::Submitted { id, text } if id.as_str() == SEARCH => {
                Some(ModelsAction::Search(text.clone()))
            }
            SettingsInput::Pressed(id) => {
                let mut buttons = Buttons::default();
                self.build_page(&mut buttons);
                buttons.action_of(id)
            }
            _ => None,
        }
    }

    /// Handles the outcome of an earlier request.
    pub fn on_event(&mut self, event: HubEvent) {
        match event {
            HubEvent::Installed(files) => self.on_installed(files),
            HubEvent::SearchFinished { query, result } => self.on_search_finished(&query, result),
            HubEvent::FilesListed { repo, result } => self.on_files_listed(&repo, result),
            HubEvent::DownloadProgress { file, received } => self.on_download_progress(&file, received),
            HubEvent::DownloadFinished { file, result } => self.on_download_finished(file, result),
        }
    }

    /// Returns the downloaded model files, by repository and then name.
    #[must_use]
    pub fn installed_files(&self) -> Vec<ModelFile> {
        self.files
            .iter()
            .filter(|(_, tracked)| matches!(tracked.state, FileState::Installed))
            .map(|(file, _)| file.clone())
            .collect()
    }

    /// Returns the page that the pane shows now: the search field, then the suggested models
    /// while the search is empty, else the search results or the files of a repository, then the
    /// downloaded and downloading files.
    #[must_use]
    pub fn page(&self) -> Page {
        self.build_page(&mut Buttons::default())
    }

    fn start_search(&mut self, query: &str) -> Option<HubRequest> {
        if query.is_empty() {
            self.search = None;
            return None;
        }
        self.search = Some(Search {
            query: query.to_owned(),
            results: Fetch::Pending,
            open_repo: None,
        });
        Some(HubRequest::Search {
            query: query.to_owned(),
        })
    }

    fn open_repo(&mut self, repo: String) -> Option<HubRequest> {
        let search = self
            .search
            .as_mut()
            .filter(|search| matches!(search.results, Fetch::Done(_)))?;
        search.open_repo = Some(OpenRepo {
            id: repo.clone(),
            files: Fetch::Pending,
        });
        Some(HubRequest::ListFiles { repo })
    }

    fn back_to_results(&mut self) -> Option<HubRequest> {
        if let Some(search) = &mut self.search {
            search.open_repo = None;
        }
        None
    }

    fn start_download(&mut self, sized: SizedFile) -> Option<HubRequest> {
        let is_startable = self
            .files
            .get(&sized.file)
            .is_none_or(|tracked| matches!(tracked.state, FileState::Failed(_)));
        if !is_startable {
            return None;
        }
        let tracked = TrackedFile {
            size: sized.size,
            state: FileState::Downloading {
                received: ByteSize::default(),
            },
        };
        self.files.insert(sized.file.clone(), tracked);
        Some(HubRequest::Download { file: sized.file })
    }

    fn cancel_download(&mut self, file: ModelFile) -> Option<HubRequest> {
        let tracked = self.files.get_mut(&file)?;
        let FileState::Downloading { .. } = tracked.state else {
            return None;
        };
        tracked.state = FileState::Canceling;
        Some(HubRequest::CancelDownload { file })
    }

    fn delete(&mut self, file: ModelFile) -> Option<HubRequest> {
        let FileState::Installed = self.files.get(&file)?.state else {
            return None;
        };
        self.files.remove(&file);
        Some(HubRequest::Delete { file })
    }

    fn on_installed(&mut self, installed: Vec<SizedFile>) {
        self.files
            .retain(|_, tracked| !matches!(tracked.state, FileState::Installed));
        for SizedFile { file, size } in installed {
            let is_running = self.files.get(&file).is_some_and(|tracked| {
                matches!(
                    tracked.state,
                    FileState::Downloading { .. } | FileState::Canceling
                )
            });
            if !is_running {
                self.files.insert(
                    file,
                    TrackedFile {
                        size,
                        state: FileState::Installed,
                    },
                );
            }
        }
    }

    fn on_search_finished(&mut self, query: &str, result: Result<Vec<RepoSummary>, HubError>) {
        let Some(search) = self.search.as_mut().filter(|search| search.query == query) else {
            return;
        };
        if matches!(search.results, Fetch::Pending) {
            search.results = result.into();
        }
    }

    fn on_files_listed(&mut self, repo: &str, result: Result<Vec<SizedFile>, HubError>) {
        let Some(open_repo) = self.search.as_mut().and_then(|search| search.open_repo.as_mut()) else {
            return;
        };
        if open_repo.id == repo && matches!(open_repo.files, Fetch::Pending) {
            open_repo.files = result.map(usable_model_files).into();
        }
    }

    fn on_download_progress(&mut self, file: &ModelFile, received: ByteSize) {
        if let Some(tracked) = self.files.get_mut(file)
            && let FileState::Downloading { .. } = tracked.state
        {
            tracked.state = FileState::Downloading { received };
        }
    }

    fn on_download_finished(&mut self, file: ModelFile, result: Result<ByteSize, HubError>) {
        let Some(tracked) = self.files.get(&file) else {
            return;
        };
        let next = match (result, tracked.state) {
            (_, FileState::Installed | FileState::Failed(_)) => return,
            (Ok(size), _) => Some(TrackedFile {
                size,
                state: FileState::Installed,
            }),
            (Err(HubError::Canceled), _) | (Err(_), FileState::Canceling) => None,
            (Err(error), FileState::Downloading { .. }) => Some(TrackedFile {
                size: tracked.size,
                state: FileState::Failed(error),
            }),
        };
        match next {
            Some(tracked) => self.files.insert(file, tracked),
            None => self.files.remove(&file),
        };
    }
}

/// The id of the button that runs `action`. It holds no text that the user typed.
fn button_id(action: &ModelsAction) -> ControlId {
    let name = match action {
        ModelsAction::Search(_) => "search-again".to_owned(),
        ModelsAction::ShowFiles(repo) => format!("show-files/{repo}"),
        ModelsAction::BackToResults => "back-to-results".to_owned(),
        ModelsAction::Download(sized) => format!("download/{}/{}", sized.file.repo, sized.file.name),
        ModelsAction::Cancel(file) => format!("cancel/{}/{}", file.repo, file.name),
        ModelsAction::Delete(file) => format!("delete/{}/{}", file.repo, file.name),
    };
    ControlId::from(name)
}

#[cfg(test)]
mod tests;
