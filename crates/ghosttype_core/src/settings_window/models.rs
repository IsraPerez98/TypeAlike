//! The Models pane of the settings window: suggested models, Hugging Face search and downloads.

mod catalog;

use std::collections::BTreeMap;
use std::iter;

use crate::hub::{
    ByteSize, HubError, HubEvent, HubRequest, ModelFile, RepoSummary, SizedFile, rounded_quotient,
};
use crate::settings_window::models::catalog::SUGGESTED_MODELS;
use crate::settings_window::view::{
    Accessory, Button, ControlId, Item, Message, Page, PageIcon, Permille, Row, SearchField, Section,
    SettingsInput,
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
const PANE_TITLE: &str = "Models";
const PANE_SUMMARY: &str = "GhostType uses a language model on this computer to suggest text.";
const SEARCH_PLACEHOLDER: &str = "Search Hugging Face";
const SUGGESTED_HEADING: &str = "Suggested models";
const SUGGESTED_FOOTER: &str = "Smaller models are faster and use less memory. Larger models give better \
     suggestions. When the model runs on the CPU, select a model with 1B parameters or fewer, so that \
     suggestions show sooner.";
const RESULTS_FOOTER: &str =
    "The list shows only GGUF models. The models with the most downloads are at the top.";
const FILES_FOOTER: &str = "Smaller files are faster. Larger files give better suggestions. If you are not sure, select a Q4_K_M file.";
const DOWNLOADED_HEADING: &str = "Downloaded models";
const SEARCHING_TEXT: &str = "Searching…";
const LOADING_FILES_TEXT: &str = "Loading the files…";
const NO_USABLE_FILES_TEXT: &str = "This model has no GGUF files that GhostType can use.";
const GATED_REPO_NOTE: &str = "Needs a Hugging Face account";
const CANCELING_DETAIL: &str = "Canceling…";
const COUNT_UNITS: [(u64, &str); 3] = [(1_000_000_000, "B"), (1_000_000, "M"), (1_000, "K")];
const SMALLEST_ABBREVIATED_COUNT: u64 = 1_000;
const GGUF_EXTENSION: &str = ".gguf";
const VISION_PROJECTOR_MARKER: &str = "mmproj";
const DRAFT_MODEL_PREFIX: &str = "mtp-";
const SPLIT_PART_SEPARATOR: &str = "-of-";
const PERMILLE_FULL: u16 = 1000;

/// State of the Models pane: running downloads, downloaded files and the current search.
///
/// ```
/// use ghosttype_core::hub::HubRequest;
/// use ghosttype_core::settings_window::models::{ModelsAction, ModelsPane};
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

    /// The action of the button `id`.
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

    /// Handles a user action; returns the request it needs, if any.
    pub fn on_action(&mut self, action: ModelsAction) -> Option<HubRequest> {
        match action {
            ModelsAction::Search(query) => self.start_search(query.trim()),
            ModelsAction::ShowFiles(repo) => self.open_repo(repo),
            ModelsAction::BackToResults => {
                if let Some(search) = &mut self.search {
                    search.open_repo = None;
                }
                None
            }
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

    fn build_page(&self, buttons: &mut Buttons) -> Page {
        let browse = self.browse_section(buttons);
        let downloaded = self.downloaded_section(buttons);
        Page {
            title: PANE_TITLE,
            icon: PageIcon::Cube,
            summary: PANE_SUMMARY,
            search: Some(SearchField {
                id: ControlId::new(SEARCH),
                placeholder: SEARCH_PLACEHOLDER,
            }),
            sections: iter::once(browse).chain(downloaded).collect(),
        }
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

    fn browse_section(&self, buttons: &mut Buttons) -> Section {
        match &self.search {
            None => self.suggested_section(buttons),
            Some(Search {
                open_repo: Some(open_repo),
                ..
            }) => self.repo_files_section(open_repo, buttons),
            Some(Search {
                query,
                results,
                open_repo: None,
            }) => results_section(query, results, buttons),
        }
    }

    fn suggested_section(&self, buttons: &mut Buttons) -> Section {
        let rows = SUGGESTED_MODELS
            .iter()
            .map(|model| {
                let shown = ShownFile {
                    title: model.name.to_owned(),
                    idle_subtitle: model.description(),
                    sized: model.sized_file(),
                };
                self.file_row(shown, buttons)
            })
            .collect();
        Section {
            heading: Some(SUGGESTED_HEADING.to_owned()),
            back: None,
            rows,
            footer: Some(SUGGESTED_FOOTER),
        }
    }

    fn repo_files_section(&self, open_repo: &OpenRepo, buttons: &mut Buttons) -> Section {
        let rows = match &open_repo.files {
            Fetch::Pending => vec![Row::Loading(LOADING_FILES_TEXT)],
            Fetch::Failed(error) => {
                failure_message(*error, ModelsAction::ShowFiles(open_repo.id.clone()), buttons)
            }
            Fetch::Done(files) if files.is_empty() => message(NO_USABLE_FILES_TEXT.to_owned()),
            Fetch::Done(files) => files
                .iter()
                .map(|sized| {
                    let shown = ShownFile {
                        title: model_name(&sized.file),
                        idle_subtitle: String::new(),
                        sized: sized.clone(),
                    };
                    self.file_row(shown, buttons)
                })
                .collect(),
        };
        Section {
            heading: Some(open_repo.id.clone()),
            back: Some(buttons.add("Results", ModelsAction::BackToResults)),
            rows,
            footer: Some(FILES_FOOTER),
        }
    }

    fn downloaded_section(&self, buttons: &mut Buttons) -> Option<Section> {
        let rows: Vec<Row> = self
            .files
            .iter()
            .map(|(file, tracked)| {
                let shown = ShownFile {
                    title: model_name(file),
                    idle_subtitle: file.repo.clone(),
                    sized: SizedFile {
                        file: file.clone(),
                        size: tracked.size,
                    },
                };
                self.file_row(shown, buttons)
            })
            .collect();
        (!rows.is_empty()).then(|| Section {
            heading: Some(DOWNLOADED_HEADING.to_owned()),
            ..Section::new(rows)
        })
    }

    fn file_row(&self, shown: ShownFile, buttons: &mut Buttons) -> Row {
        let ShownFile {
            title,
            idle_subtitle,
            sized,
        } = shown;
        let Some(tracked) = self.files.get(&sized.file) else {
            return Row::Item(Item {
                title,
                subtitle: idle_subtitle,
                detail: sized.size.to_string(),
                accessory: Accessory::Download(buttons.add("Download", ModelsAction::Download(sized))),
            });
        };
        let subtitle = match tracked.state {
            FileState::Failed(error) => error_message(error).to_owned(),
            FileState::Downloading { .. } | FileState::Canceling | FileState::Installed => idle_subtitle,
        };
        let (detail, accessory) = tracked_status(&sized, tracked, buttons);
        Row::Item(Item {
            title,
            subtitle,
            detail,
            accessory,
        })
    }
}

/// A file that a list shows: its title, the subtitle while it does not download, and its size.
#[derive(Debug)]
struct ShownFile {
    title: String,
    idle_subtitle: String,
    sized: SizedFile,
}

fn tracked_status(sized: &SizedFile, tracked: &TrackedFile, buttons: &mut Buttons) -> (String, Accessory) {
    let size = tracked.size.to_string();
    match tracked.state {
        FileState::Downloading { received } => (
            format!("{received} of {size}"),
            Accessory::Downloading {
                progress: permille(received, tracked.size),
                cancel: buttons.add("Cancel download", ModelsAction::Cancel(sized.file.clone())),
            },
        ),
        FileState::Canceling => (CANCELING_DETAIL.to_owned(), Accessory::Canceling),
        FileState::Installed => (
            size,
            Accessory::Installed {
                delete: buttons.add("Delete", ModelsAction::Delete(sized.file.clone())),
            },
        ),
        FileState::Failed(_) => (
            size,
            Accessory::Failed {
                retry: buttons.add("Try again", ModelsAction::Download(sized.clone())),
            },
        ),
    }
}

fn results_section(query: &str, results: &Fetch<Vec<RepoSummary>>, buttons: &mut Buttons) -> Section {
    let rows = match results {
        Fetch::Pending => vec![Row::Loading(SEARCHING_TEXT)],
        Fetch::Failed(error) => failure_message(*error, ModelsAction::Search(query.to_owned()), buttons),
        Fetch::Done(repos) if repos.is_empty() => message(format!("No models match “{query}”.")),
        Fetch::Done(repos) => repos.iter().map(|repo| repo_row(repo, buttons)).collect(),
    };
    Section {
        heading: Some(format!("Results for “{query}”")),
        back: None,
        rows,
        footer: Some(RESULTS_FOOTER),
    }
}

fn repo_row(repo: &RepoSummary, buttons: &mut Buttons) -> Row {
    let (owner, name) = repo.id.split_once('/').unwrap_or_default();
    let byline = format!("by {owner}");
    let (subtitle, accessory) = if repo.is_gated {
        (format!("{byline} · {GATED_REPO_NOTE}"), Accessory::Unavailable)
    } else {
        let show_files = buttons.add("Show files", ModelsAction::ShowFiles(repo.id.clone()));
        (byline, Accessory::Open(show_files))
    };
    Row::Item(Item {
        title: name.to_owned(),
        subtitle,
        detail: format!("{} downloads", compact_count(repo.downloads)),
        accessory,
    })
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

pub(crate) fn model_name(file: &ModelFile) -> String {
    file.name
        .strip_suffix(GGUF_EXTENSION)
        .unwrap_or(&file.name)
        .to_owned()
}

fn message(text: String) -> Vec<Row> {
    vec![Row::Message(Message { text, button: None })]
}

fn failure_message(error: HubError, retry_action: ModelsAction, buttons: &mut Buttons) -> Vec<Row> {
    vec![Row::Message(Message {
        text: error_message(error).to_owned(),
        button: Some(buttons.add("Try again", retry_action)),
    })]
}

fn permille(received: ByteSize, total: ByteSize) -> Permille {
    let thousandths = received
        .0
        .saturating_mul(u64::from(PERMILLE_FULL))
        .checked_div(total.0)
        .unwrap_or_default();
    Permille(
        u16::try_from(thousandths)
            .unwrap_or(PERMILLE_FULL)
            .min(PERMILLE_FULL),
    )
}

fn error_message(error: HubError) -> &'static str {
    match error {
        HubError::Offline => "GhostType cannot connect to Hugging Face. Check your internet connection.",
        HubError::AccessDenied => "This model needs a Hugging Face account.",
        HubError::NotFound => "Hugging Face cannot find this model.",
        HubError::Server => "Hugging Face is not available. Try again later.",
        HubError::InvalidResponse => "Hugging Face sent an incorrect response. Try again later.",
        HubError::Storage => "GhostType cannot save the model. Make sure that the disk has free space.",
        HubError::Canceled => "You canceled the download.",
    }
}

// ponytail: English K/M/B suffixes only, use the OS number formatter for other locales.
fn compact_count(count: u64) -> String {
    if count < SMALLEST_ABBREVIATED_COUNT {
        return count.to_string();
    }
    let (tenths, suffix) = COUNT_UNITS
        .iter()
        .map(|&(unit, suffix)| (rounded_quotient(count, unit / 10), suffix))
        .find(|&(tenths, _)| tenths >= 10)
        .expect("a count of at least one thousand has a unit");
    match tenths % 10 {
        0 => format!("{}{suffix}", tenths / 10),
        decimal => format!("{}.{decimal}{suffix}", tenths / 10),
    }
}

fn usable_model_files(files: Vec<SizedFile>) -> Vec<SizedFile> {
    let mut usable: Vec<_> = files
        .into_iter()
        .filter(|sized| is_single_file_language_model(&sized.file.name))
        .collect();
    usable.sort_by_key(|sized| sized.size);
    usable
}

fn is_single_file_language_model(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(GGUF_EXTENSION) else {
        return false;
    };
    let is_companion_file =
        stem.to_lowercase().contains(VISION_PROJECTOR_MARKER) || stem.starts_with(DRAFT_MODEL_PREFIX);
    !is_companion_file && !is_split_part(stem)
}

/// Whether `stem` names one part of a split model, like `Model-00001-of-00003`.
fn is_split_part(stem: &str) -> bool {
    let is_number = |text: &str| !text.is_empty() && text.chars().all(|character| character.is_ascii_digit());
    stem.rsplit_once(SPLIT_PART_SEPARATOR)
        .is_some_and(|(head, total)| {
            is_number(total) && head.rsplit_once('-').is_some_and(|(_, part)| is_number(part))
        })
}

#[cfg(test)]
mod tests;
