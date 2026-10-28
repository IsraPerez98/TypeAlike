//! The Models pane of the settings window: suggested models, Hugging Face search and downloads.

mod catalog;
mod page;

use std::collections::BTreeMap;

use crate::hub::{
    ByteSize, HubError, HubEvent, HubRequest, ModelFile, RepoSummary, SizedFile, usable_model_files,
};
use crate::inference::{Backend, InferenceError};
use crate::localization::Translations;
use crate::settings_window::view::{
    Button, ControlId, Note, NoteTone, Page, Row, RowIcon, Section, SettingsInput, TextField,
};

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
    /// The last read of the free memory of the GPU that runs a backend.
    gpu_memory: GpuMemory,
}

/// The last read of the free memory of the GPU that runs a backend.
#[derive(Debug, Clone, Copy, Default)]
enum GpuMemory {
    #[default]
    NotRead,
    Reading(Backend),
    Free(Backend, ByteSize),
    Failed(Backend),
}

impl GpuMemory {
    fn free_for(self, backend: Backend) -> Option<ByteSize> {
        match self {
            Self::Free(read_backend, free) if read_backend == backend => Some(free),
            _ => None,
        }
    }
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

/// The buttons that a page shows, each with the action that a click on it runs, and the texts of
/// the page.
#[derive(Debug)]
struct Buttons<'translations> {
    translations: &'translations Translations,
    actions: Vec<(ControlId, ModelsAction)>,
}

impl<'translations> Buttons<'translations> {
    fn new(translations: &'translations Translations) -> Self {
        Self {
            translations,
            actions: Vec::new(),
        }
    }

    /// Returns a button with the label of `message` that runs `action`, and keeps the action for
    /// [`Self::action_of`].
    fn add(&mut self, message: &str, action: ModelsAction) -> Button {
        let id = button_id(&action);
        self.actions.push((id.clone(), action));
        Button {
            id,
            label: self.translations.text(message),
            is_enabled: true,
        }
    }

    fn action_of(self, id: &ControlId) -> Option<ModelsAction> {
        self.actions
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

    /// Returns the action of `input`, when `input` is for a control of the pane: the search field,
    /// a button that the pane shows now, or a button of the row of the model that TypeAlike
    /// recommends for `backend`. The `translations` give the texts of the buttons.
    #[must_use]
    pub fn action_for(
        &self,
        input: &SettingsInput,
        backend: Backend,
        translations: &Translations,
    ) -> Option<ModelsAction> {
        match input {
            SettingsInput::Submitted { id, text } if id.as_str() == SEARCH => {
                Some(ModelsAction::Search(text.clone()))
            }
            SettingsInput::Pressed(id) => {
                let mut buttons = Buttons::new(translations);
                self.build_page(&mut buttons, backend);
                self.build_recommended_row(&mut buttons, backend);
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

    /// Keeps that a read of the free memory of the GPU that runs `backend` started. Until its
    /// answer, [`Self::is_reading_gpu_memory`] tells it.
    pub fn start_gpu_memory_read(&mut self, backend: Backend) {
        self.gpu_memory = GpuMemory::Reading(backend);
    }

    /// Keeps the `result` of a read of the free memory of the GPU that runs `backend`. The model
    /// that TypeAlike recommends for `backend` then fits in that memory. After a failed read,
    /// TypeAlike recommends a model that fits in most graphics cards, with a warning.
    pub fn on_gpu_memory(&mut self, backend: Backend, result: Result<ByteSize, InferenceError>) {
        self.gpu_memory = match result {
            Ok(free) => GpuMemory::Free(backend, free),
            Err(_) => GpuMemory::Failed(backend),
        };
    }

    /// Whether the read of the free memory of the GPU that runs `backend` did not answer yet.
    #[must_use]
    pub fn is_reading_gpu_memory(&self, backend: Backend) -> bool {
        matches!(self.gpu_memory, GpuMemory::Reading(read_backend) if read_backend == backend)
    }

    /// Whether a model downloads now.
    #[must_use]
    pub fn has_running_download(&self) -> bool {
        self.files.values().any(|tracked| {
            matches!(
                tracked.state,
                FileState::Downloading { .. } | FileState::Canceling
            )
        })
    }

    /// Returns the warning that TypeAlike cannot read the graphics card, with the texts of
    /// `translations`, while the row of the model that TypeAlike recommends for `backend` shows
    /// after a failed read of the free memory of its GPU.
    #[must_use]
    pub fn recommendation_warning(&self, backend: Backend, translations: &Translations) -> Option<Row> {
        let has_failed =
            matches!(self.gpu_memory, GpuMemory::Failed(read_backend) if read_backend == backend);
        (has_failed && self.recommended_row(backend, translations).is_some()).then(|| {
            Row::Note(Note {
                text: translations.text("model-recommendation-unknown-gpu"),
                tone: NoteTone::Warning,
            })
        })
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

    /// Returns the page that the pane shows now, with the texts of `translations`: the search
    /// field, then the recommended models while the search is empty, else the search results or
    /// the files of a repository, then the downloaded and downloading files. The list marks the
    /// model that TypeAlike recommends for `backend`.
    #[must_use]
    pub fn page(&self, backend: Backend, translations: &Translations) -> Page {
        self.build_page(&mut Buttons::new(translations), backend)
    }

    /// Returns the sections of the model browser of the setup, with the texts of `translations`:
    /// the search of Hugging Face as a field with a button, then the sections of the page, as the
    /// page shows them for `backend`.
    #[must_use]
    pub fn browser_sections(&self, backend: Backend, translations: &Translations) -> Vec<Section> {
        let page = self.page(backend, translations);
        let search = page.search.map(|field| {
            Section::new(vec![Row::TextField(TextField {
                id: field.id,
                icon: RowIcon::MagnifyingGlass,
                label: field.placeholder,
                placeholder: translations.text("models-search-placeholder"),
                submit_label: translations.text("models-search-button"),
                is_submit_enabled: true,
            })])
        });
        search.into_iter().chain(page.sections).collect()
    }

    /// Returns the row of the model that TypeAlike recommends for `backend`, with the texts of
    /// `translations`, while no model is downloaded. Its button downloads the model, and the row
    /// shows the progress of the download.
    #[must_use]
    pub fn recommended_row(&self, backend: Backend, translations: &Translations) -> Option<Row> {
        self.build_recommended_row(&mut Buttons::new(translations), backend)
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
