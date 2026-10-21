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
mod tests {
    use super::*;

    const REPO: &str = "owner/Model-GGUF";

    fn file(name: &str) -> ModelFile {
        ModelFile {
            repo: REPO.to_owned(),
            name: name.to_owned(),
        }
    }

    fn sized(name: &str, size: u64) -> SizedFile {
        SizedFile {
            file: file(name),
            size: ByteSize(size),
        }
    }

    fn repo(id: &str, downloads: u64, is_gated: bool) -> RepoSummary {
        RepoSummary {
            id: id.to_owned(),
            downloads,
            is_gated,
        }
    }

    fn button(label: &'static str, action: &ModelsAction) -> Button {
        Button {
            id: button_id(action),
            label,
            is_enabled: true,
        }
    }

    fn downloading(name: &str, size: u64) -> ModelsPane {
        let mut pane = ModelsPane::new();
        pane.on_action(ModelsAction::Download(sized(name, size)));
        pane
    }

    fn installed(name: &str, size: u64) -> ModelsPane {
        let mut pane = ModelsPane::new();
        pane.on_event(HubEvent::Installed(vec![sized(name, size)]));
        pane
    }

    fn searched(query: &str, repos: Vec<RepoSummary>) -> ModelsPane {
        let mut pane = ModelsPane::new();
        pane.on_action(ModelsAction::Search(query.to_owned()));
        pane.on_event(HubEvent::SearchFinished {
            query: query.to_owned(),
            result: Ok(repos),
        });
        pane
    }

    fn opening_repo() -> ModelsPane {
        let mut pane = searched("model", vec![repo(REPO, 10, false)]);
        pane.on_action(ModelsAction::ShowFiles(REPO.to_owned()));
        pane
    }

    fn browsing(files: Vec<SizedFile>) -> ModelsPane {
        let mut pane = opening_repo();
        pane.on_event(HubEvent::FilesListed {
            repo: REPO.to_owned(),
            result: Ok(files),
        });
        pane
    }

    fn items(section: Section) -> Vec<Item> {
        section
            .rows
            .into_iter()
            .map(|row| match row {
                Row::Item(item) => item,
                other => panic!("the list shows {other:?}, not items"),
            })
            .collect()
    }

    fn browse(pane: &ModelsPane) -> Section {
        pane.page().sections.remove(0)
    }

    fn downloaded(pane: &ModelsPane) -> Option<Section> {
        pane.page().sections.into_iter().nth(1)
    }

    fn browse_rows(pane: &ModelsPane) -> Vec<Item> {
        items(browse(pane))
    }

    fn downloaded_row(pane: &ModelsPane) -> Item {
        items(downloaded(pane).expect("a file is tracked")).remove(0)
    }

    fn titles(items: Vec<Item>) -> Vec<String> {
        items.into_iter().map(|item| item.title).collect()
    }

    fn message(text: &str, button: Option<Button>) -> Vec<Row> {
        vec![Row::Message(Message {
            text: text.to_owned(),
            button,
        })]
    }

    fn accessory_button(item: Item) -> Button {
        match item.accessory {
            Accessory::Download(button)
            | Accessory::Open(button)
            | Accessory::Downloading { cancel: button, .. }
            | Accessory::Installed { delete: button }
            | Accessory::Failed { retry: button } => button,
            other => panic!("the item shows no button: {other:?}"),
        }
    }

    fn retry_button(section: Section) -> Button {
        match section.rows.into_iter().next() {
            Some(Row::Message(Message {
                button: Some(retry), ..
            })) => retry,
            other => panic!("the list shows no retry: {other:?}"),
        }
    }

    fn failed_search(error: HubError) -> ModelsPane {
        let mut pane = ModelsPane::new();
        pane.on_action(ModelsAction::Search("x".to_owned()));
        pane.on_event(HubEvent::SearchFinished {
            query: "x".to_owned(),
            result: Err(error),
        });
        pane
    }

    fn press(pane: &ModelsPane, button: Button) -> Option<ModelsAction> {
        pane.action_for(&SettingsInput::Pressed(button.id))
    }

    #[test]
    fn opening_lists_downloaded_files() {
        let pane = ModelsPane::new();

        assert_eq!(pane.on_open(), HubRequest::ListInstalled);
    }

    #[test]
    fn pane_explains_itself_above_the_search_field() {
        let shown = ModelsPane::new().page();

        assert_eq!(
            (shown.title, shown.icon, shown.summary, shown.search),
            (
                "Models",
                PageIcon::Cube,
                "GhostType uses a language model on this computer to suggest text.",
                Some(SearchField {
                    id: ControlId::new(SEARCH),
                    placeholder: "Search Hugging Face",
                })
            )
        );
    }

    #[test]
    fn empty_search_browses_suggested_models() {
        let shown = browse(&ModelsPane::new());

        assert_eq!(
            (shown.heading, shown.back, shown.footer),
            (
                Some("Suggested models".to_owned()),
                None,
                Some(
                    "Smaller models are faster and use less memory. Larger models give better suggestions. When the model runs on the CPU, select a model with 1B parameters or fewer, so that suggestions show sooner."
                )
            )
        );
    }

    #[test]
    fn every_suggested_model_has_a_row() {
        let pane = ModelsPane::new();

        assert_eq!(browse_rows(&pane).len(), SUGGESTED_MODELS.len());
    }

    #[test]
    fn suggested_model_offers_its_file_for_download() {
        let pane = ModelsPane::new();
        let model = &SUGGESTED_MODELS[0];

        let row = browse_rows(&pane).remove(0);

        assert_eq!(
            row,
            Item {
                title: model.name.to_owned(),
                subtitle: model.description(),
                detail: model.size.to_string(),
                accessory: Accessory::Download(button(
                    "Download",
                    &ModelsAction::Download(model.sized_file())
                )),
            }
        );
    }

    #[test]
    fn suggested_model_shows_its_download_state() {
        let model = &SUGGESTED_MODELS[0];
        let mut pane = ModelsPane::new();
        pane.on_action(ModelsAction::Download(model.sized_file()));

        let row = browse_rows(&pane).remove(0);

        assert_eq!(
            row.accessory,
            Accessory::Downloading {
                progress: Permille(0),
                cancel: button("Cancel download", &ModelsAction::Cancel(model.sized_file().file)),
            }
        );
    }

    #[test]
    fn download_action_requests_download() {
        let mut pane = ModelsPane::new();

        let request = pane.on_action(ModelsAction::Download(sized("a.gguf", 100)));

        assert_eq!(request, Some(HubRequest::Download { file: file("a.gguf") }));
    }

    #[test]
    fn downloaded_list_has_a_heading_and_no_footer() {
        let pane = installed("a.gguf", 100);

        let shown = downloaded(&pane).expect("a file is installed");

        assert_eq!(
            (shown.heading, shown.back, shown.footer),
            (Some("Downloaded models".to_owned()), None, None)
        );
    }

    #[test]
    fn running_download_shows_progress_and_cancel() {
        let mut pane = downloading("a.gguf", 2_000_000);

        pane.on_event(HubEvent::DownloadProgress {
            file: file("a.gguf"),
            received: ByteSize(500_000),
        });

        assert_eq!(
            downloaded_row(&pane),
            Item {
                title: "a".to_owned(),
                subtitle: REPO.to_owned(),
                detail: "500 KB of 2 MB".to_owned(),
                accessory: Accessory::Downloading {
                    progress: Permille(250),
                    cancel: button("Cancel download", &ModelsAction::Cancel(file("a.gguf"))),
                },
            }
        );
    }

    #[test]
    fn progress_past_expected_size_shows_as_complete() {
        let mut pane = downloading("a.gguf", 100);

        pane.on_event(HubEvent::DownloadProgress {
            file: file("a.gguf"),
            received: ByteSize(150),
        });

        assert!(matches!(
            downloaded_row(&pane).accessory,
            Accessory::Downloading {
                progress: Permille(1000),
                ..
            }
        ));
    }

    #[test]
    fn empty_expected_size_shows_no_progress_yet() {
        let mut pane = downloading("a.gguf", 0);

        pane.on_event(HubEvent::DownloadProgress {
            file: file("a.gguf"),
            received: ByteSize(150),
        });

        assert!(matches!(
            downloaded_row(&pane).accessory,
            Accessory::Downloading {
                progress: Permille(0),
                ..
            }
        ));
    }

    #[test]
    fn running_download_is_not_started_twice() {
        let mut pane = downloading("a.gguf", 100);

        let request = pane.on_action(ModelsAction::Download(sized("a.gguf", 100)));

        assert_eq!(request, None);
    }

    #[test]
    fn cancel_action_requests_cancel() {
        let mut pane = downloading("a.gguf", 100);

        let request = pane.on_action(ModelsAction::Cancel(file("a.gguf")));

        assert_eq!(request, Some(HubRequest::CancelDownload { file: file("a.gguf") }));
    }

    #[test]
    fn canceling_download_shows_it_is_busy() {
        let mut pane = downloading("a.gguf", 100);

        pane.on_action(ModelsAction::Cancel(file("a.gguf")));

        let row = downloaded_row(&pane);
        assert_eq!(
            (row.detail.as_str(), row.accessory),
            ("Canceling…", Accessory::Canceling)
        );
    }

    #[test]
    fn cancel_without_running_download_is_ignored() {
        let mut pane = installed("a.gguf", 100);

        let request = pane.on_action(ModelsAction::Cancel(file("a.gguf")));

        assert_eq!(request, None);
    }

    #[test]
    fn progress_after_cancel_is_ignored() {
        let mut pane = downloading("a.gguf", 100);
        pane.on_action(ModelsAction::Cancel(file("a.gguf")));

        pane.on_event(HubEvent::DownloadProgress {
            file: file("a.gguf"),
            received: ByteSize(50),
        });

        assert_eq!(downloaded_row(&pane).accessory, Accessory::Canceling);
    }

    #[test]
    fn canceled_download_is_forgotten() {
        let mut pane = downloading("a.gguf", 100);
        pane.on_action(ModelsAction::Cancel(file("a.gguf")));

        pane.on_event(HubEvent::DownloadFinished {
            file: file("a.gguf"),
            result: Err(HubError::Canceled),
        });

        assert_eq!(downloaded(&pane), None);
    }

    #[test]
    fn failure_while_canceling_is_forgotten() {
        let mut pane = downloading("a.gguf", 100);
        pane.on_action(ModelsAction::Cancel(file("a.gguf")));

        pane.on_event(HubEvent::DownloadFinished {
            file: file("a.gguf"),
            result: Err(HubError::Offline),
        });

        assert_eq!(downloaded(&pane), None);
    }

    #[test]
    fn download_finished_before_cancel_is_kept() {
        let mut pane = downloading("a.gguf", 100);
        pane.on_action(ModelsAction::Cancel(file("a.gguf")));

        pane.on_event(HubEvent::DownloadFinished {
            file: file("a.gguf"),
            result: Ok(ByteSize(100)),
        });

        assert_eq!(
            downloaded_row(&pane).accessory,
            Accessory::Installed {
                delete: button("Delete", &ModelsAction::Delete(file("a.gguf")))
            }
        );
    }

    #[test]
    fn finished_download_offers_delete() {
        let mut pane = downloading("a.gguf", 100);

        pane.on_event(HubEvent::DownloadFinished {
            file: file("a.gguf"),
            result: Ok(ByteSize(120)),
        });

        assert_eq!(
            downloaded_row(&pane),
            Item {
                title: "a".to_owned(),
                subtitle: REPO.to_owned(),
                detail: "120 bytes".to_owned(),
                accessory: Accessory::Installed {
                    delete: button("Delete", &ModelsAction::Delete(file("a.gguf")))
                },
            }
        );
    }

    #[test]
    fn failed_download_explains_and_offers_retry() {
        let mut pane = downloading("a.gguf", 100);

        pane.on_event(HubEvent::DownloadFinished {
            file: file("a.gguf"),
            result: Err(HubError::Offline),
        });

        assert_eq!(
            downloaded_row(&pane),
            Item {
                title: "a".to_owned(),
                subtitle: "GhostType cannot connect to Hugging Face. Check your internet connection."
                    .to_owned(),
                detail: "100 bytes".to_owned(),
                accessory: Accessory::Failed {
                    retry: button("Try again", &ModelsAction::Download(sized("a.gguf", 100)))
                },
            }
        );
    }

    #[test]
    fn failed_download_can_restart() {
        let mut pane = downloading("a.gguf", 100);
        pane.on_event(HubEvent::DownloadFinished {
            file: file("a.gguf"),
            result: Err(HubError::Storage),
        });

        let request = pane.on_action(ModelsAction::Download(sized("a.gguf", 100)));

        assert_eq!(request, Some(HubRequest::Download { file: file("a.gguf") }));
    }

    #[test]
    fn finish_of_unknown_download_is_ignored() {
        let mut pane = ModelsPane::new();

        pane.on_event(HubEvent::DownloadFinished {
            file: file("a.gguf"),
            result: Ok(ByteSize(100)),
        });

        assert_eq!(downloaded(&pane), None);
    }

    #[test]
    fn second_finish_does_not_change_downloaded_file() {
        let mut pane = installed("a.gguf", 100);

        pane.on_event(HubEvent::DownloadFinished {
            file: file("a.gguf"),
            result: Err(HubError::Offline),
        });

        assert_eq!(downloaded_row(&pane).detail, "100 bytes");
    }

    #[test]
    fn hub_errors_explain_themselves() {
        let cases = [
            (HubError::AccessDenied, "This model needs a Hugging Face account."),
            (HubError::NotFound, "Hugging Face cannot find this model."),
            (
                HubError::Server,
                "Hugging Face is not available. Try again later.",
            ),
            (
                HubError::InvalidResponse,
                "Hugging Face sent an incorrect response. Try again later.",
            ),
            (
                HubError::Storage,
                "GhostType cannot save the model. Make sure that the disk has free space.",
            ),
            (HubError::Canceled, "You canceled the download."),
        ];

        let messages = cases.map(|(error, _)| browse(&failed_search(error)).rows);

        assert_eq!(
            messages,
            cases.map(|(_, text)| message(
                text,
                Some(button("Try again", &ModelsAction::Search("x".to_owned())))
            ))
        );
    }

    #[test]
    fn delete_action_requests_delete_and_forgets_file() {
        let mut pane = installed("a.gguf", 100);

        let request = pane.on_action(ModelsAction::Delete(file("a.gguf")));

        assert_eq!(
            (request, downloaded(&pane)),
            (Some(HubRequest::Delete { file: file("a.gguf") }), None)
        );
    }

    #[test]
    fn delete_of_running_download_is_ignored() {
        let mut pane = downloading("a.gguf", 100);

        let request = pane.on_action(ModelsAction::Delete(file("a.gguf")));

        assert_eq!(request, None);
    }

    #[test]
    fn nothing_downloaded_hides_downloaded_list() {
        let pane = ModelsPane::new();

        assert_eq!(downloaded(&pane), None);
    }

    #[test]
    fn installed_files_are_listed_by_repo_then_name() {
        let mut pane = ModelsPane::new();
        let other = SizedFile {
            file: ModelFile {
                repo: "another/Repo".to_owned(),
                name: "z.gguf".to_owned(),
            },
            size: ByteSize(1),
        };

        pane.on_event(HubEvent::Installed(vec![
            sized("b.gguf", 1),
            other,
            sized("a.gguf", 1),
        ]));

        let shown = downloaded(&pane).expect("files are installed");
        assert_eq!(titles(items(shown)), ["z", "a", "b"]);
    }

    #[test]
    fn only_complete_downloads_are_installed_files() {
        let mut pane = downloading("b.gguf", 100);
        pane.on_event(HubEvent::Installed(vec![sized("a.gguf", 1)]));

        let installed = pane.installed_files();

        assert_eq!(installed, [file("a.gguf")]);
    }

    #[test]
    fn installed_list_forgets_files_deleted_elsewhere() {
        let mut pane = installed("a.gguf", 100);

        pane.on_event(HubEvent::Installed(Vec::new()));

        assert_eq!(downloaded(&pane), None);
    }

    #[test]
    fn installed_list_keeps_running_download() {
        let mut pane = downloading("a.gguf", 100);

        pane.on_event(HubEvent::Installed(vec![sized("a.gguf", 40)]));

        assert!(matches!(
            downloaded_row(&pane).accessory,
            Accessory::Downloading { .. }
        ));
    }

    #[test]
    fn installed_list_replaces_failed_download() {
        let mut pane = downloading("a.gguf", 100);
        pane.on_event(HubEvent::DownloadFinished {
            file: file("a.gguf"),
            result: Err(HubError::Offline),
        });

        pane.on_event(HubEvent::Installed(vec![sized("a.gguf", 100)]));

        assert!(matches!(
            downloaded_row(&pane).accessory,
            Accessory::Installed { .. }
        ));
    }

    #[test]
    fn search_action_requests_trimmed_query() {
        let mut pane = ModelsPane::new();

        let request = pane.on_action(ModelsAction::Search("  qwen ".to_owned()));

        assert_eq!(
            request,
            Some(HubRequest::Search {
                query: "qwen".to_owned()
            })
        );
    }

    #[test]
    fn running_search_replaces_suggestions_with_loading_results() {
        let mut pane = ModelsPane::new();

        pane.on_action(ModelsAction::Search("qwen".to_owned()));

        let shown = browse(&pane);
        assert_eq!(
            (shown.heading, shown.rows),
            (
                Some("Results for “qwen”".to_owned()),
                vec![Row::Loading("Searching…")]
            )
        );
    }

    #[test]
    fn blank_search_returns_to_suggestions() {
        let mut pane = searched("qwen", vec![repo(REPO, 1, false)]);

        let request = pane.on_action(ModelsAction::Search(" ".to_owned()));

        assert_eq!(
            (request, browse(&pane).heading),
            (None, Some("Suggested models".to_owned()))
        );
    }

    #[test]
    fn search_result_shows_repo_owner_and_downloads() {
        let pane = searched("qwen", vec![repo(REPO, 1_234_567, false)]);

        let rows = browse_rows(&pane);

        assert_eq!(
            rows,
            [Item {
                title: "Model-GGUF".to_owned(),
                subtitle: "by owner".to_owned(),
                detail: "1.2M downloads".to_owned(),
                accessory: Accessory::Open(button("Show files", &ModelsAction::ShowFiles(REPO.to_owned()))),
            }]
        );
    }

    #[test]
    fn search_results_explain_their_order() {
        let shown = browse(&searched("qwen", vec![repo(REPO, 1, false)]));

        assert_eq!(
            (shown.heading, shown.footer),
            (
                Some("Results for “qwen”".to_owned()),
                Some("The list shows only GGUF models. The models with the most downloads are at the top.")
            )
        );
    }

    #[test]
    fn download_counts_are_abbreviated() {
        let counts = [999, 1_000, 4_581, 80_347, 999_950, 1_229_091, 2_000_000_000];

        let details = counts.map(|count| {
            browse_rows(&searched("q", vec![repo(REPO, count, false)]))
                .remove(0)
                .detail
        });

        assert_eq!(
            details,
            [
                "999 downloads",
                "1K downloads",
                "4.6K downloads",
                "80.3K downloads",
                "1M downloads",
                "1.2M downloads",
                "2B downloads"
            ]
        );
    }

    #[test]
    fn gated_repo_cannot_be_opened() {
        let pane = searched("gemma", vec![repo(REPO, 5, true)]);

        let row = browse_rows(&pane).remove(0);

        assert_eq!(
            (row.subtitle.as_str(), row.accessory),
            ("by owner · Needs a Hugging Face account", Accessory::Unavailable)
        );
    }

    #[test]
    fn search_without_results_says_so() {
        let pane = searched("nothing", Vec::new());

        assert_eq!(browse(&pane).rows, message("No models match “nothing”.", None));
    }

    #[test]
    fn result_of_older_query_is_dropped() {
        let mut pane = ModelsPane::new();
        pane.on_action(ModelsAction::Search("old".to_owned()));
        pane.on_action(ModelsAction::Search("new".to_owned()));

        pane.on_event(HubEvent::SearchFinished {
            query: "old".to_owned(),
            result: Ok(vec![repo(REPO, 1, false)]),
        });

        assert_eq!(browse(&pane).rows, vec![Row::Loading("Searching…")]);
    }

    #[test]
    fn show_files_requests_listing() {
        let mut pane = searched("model", vec![repo(REPO, 1, false)]);

        let request = pane.on_action(ModelsAction::ShowFiles(REPO.to_owned()));

        assert_eq!(
            request,
            Some(HubRequest::ListFiles {
                repo: REPO.to_owned()
            })
        );
    }

    #[test]
    fn opened_repo_takes_over_the_browse_list() {
        let shown = browse(&opening_repo());

        assert_eq!(
            shown,
            Section {
                heading: Some(REPO.to_owned()),
                back: Some(button("Results", &ModelsAction::BackToResults)),
                rows: vec![Row::Loading("Loading the files…")],
                footer: Some(
                    "Smaller files are faster. Larger files give better suggestions. If you are not sure, select a Q4_K_M file."
                ),
            }
        );
    }

    #[test]
    fn show_files_without_results_is_ignored() {
        let mut pane = ModelsPane::new();
        pane.on_action(ModelsAction::Search("model".to_owned()));

        let request = pane.on_action(ModelsAction::ShowFiles(REPO.to_owned()));

        assert_eq!(request, None);
    }

    #[test]
    fn listed_file_offers_download_with_size() {
        let pane = browsing(vec![sized("Model-Q4_K_M.gguf", 2_000_000)]);

        assert_eq!(
            browse_rows(&pane),
            [Item {
                title: "Model-Q4_K_M".to_owned(),
                subtitle: String::new(),
                detail: "2 MB".to_owned(),
                accessory: Accessory::Download(button(
                    "Download",
                    &ModelsAction::Download(sized("Model-Q4_K_M.gguf", 2_000_000))
                )),
            }]
        );
    }

    #[test]
    fn listed_files_keep_single_file_language_models_smallest_first() {
        let pane = browsing(vec![
            sized("Model-Q8_0.gguf", 8),
            sized("README.md", 1),
            sized("mmproj-Model-F16.gguf", 2),
            sized("mtp-Model-Q4_0.gguf", 2),
            sized("Model-Q4_K_M.gguf", 4),
            sized("Model-F16-00001-of-00002.gguf", 3),
        ]);

        assert_eq!(titles(browse_rows(&pane)), ["Model-Q4_K_M", "Model-Q8_0"]);
    }

    #[test]
    fn name_resembling_split_part_is_kept() {
        let pane = browsing(vec![sized("Best-of-Breed-Q4.gguf", 1)]);

        assert_eq!(browse_rows(&pane).len(), 1);
    }

    #[test]
    fn repo_without_usable_files_says_so() {
        let pane = browsing(vec![sized("README.md", 1)]);

        assert_eq!(
            browse(&pane).rows,
            message("This model has no GGUF files that GhostType can use.", None)
        );
    }

    #[test]
    fn failed_listing_offers_retry() {
        let mut pane = opening_repo();

        pane.on_event(HubEvent::FilesListed {
            repo: REPO.to_owned(),
            result: Err(HubError::NotFound),
        });

        assert_eq!(
            browse(&pane).rows,
            message(
                "Hugging Face cannot find this model.",
                Some(button("Try again", &ModelsAction::ShowFiles(REPO.to_owned())))
            )
        );
    }

    #[test]
    fn retried_listing_loads_again() {
        let mut pane = opening_repo();
        pane.on_event(HubEvent::FilesListed {
            repo: REPO.to_owned(),
            result: Err(HubError::Offline),
        });

        let request = pane.on_action(ModelsAction::ShowFiles(REPO.to_owned()));

        assert_eq!(
            (request, browse(&pane).rows),
            (
                Some(HubRequest::ListFiles {
                    repo: REPO.to_owned()
                }),
                vec![Row::Loading("Loading the files…")]
            )
        );
    }

    #[test]
    fn listing_of_other_repo_is_dropped() {
        let mut pane = opening_repo();

        pane.on_event(HubEvent::FilesListed {
            repo: "someone/Else".to_owned(),
            result: Ok(vec![sized("a.gguf", 1)]),
        });

        assert_eq!(browse(&pane).rows, vec![Row::Loading("Loading the files…")]);
    }

    #[test]
    fn back_returns_to_results() {
        let mut pane = browsing(vec![sized("a.gguf", 1)]);

        pane.on_action(ModelsAction::BackToResults);

        let shown = browse(&pane);
        let back = shown.back.clone();
        assert_eq!(
            (back, titles(items(shown))),
            (None, vec!["Model-GGUF".to_owned()])
        );
    }

    #[test]
    fn submitting_the_search_field_searches() {
        let pane = ModelsPane::new();
        let input = SettingsInput::Submitted {
            id: ControlId::new(SEARCH),
            text: "qwen".to_owned(),
        };

        let action = pane.action_for(&input);

        assert_eq!(action, Some(ModelsAction::Search("qwen".to_owned())));
    }

    #[test]
    fn pressing_download_starts_download() {
        let pane = ModelsPane::new();
        let download = accessory_button(browse_rows(&pane).remove(0));

        let action = press(&pane, download);

        assert_eq!(
            action,
            Some(ModelsAction::Download(SUGGESTED_MODELS[0].sized_file()))
        );
    }

    #[test]
    fn pressing_cancel_cancels_the_download() {
        let pane = downloading("a.gguf", 100);
        let cancel = accessory_button(downloaded_row(&pane));

        let action = press(&pane, cancel);

        assert_eq!(action, Some(ModelsAction::Cancel(file("a.gguf"))));
    }

    #[test]
    fn pressing_delete_deletes_the_file() {
        let pane = installed("a.gguf", 100);
        let delete = accessory_button(downloaded_row(&pane));

        let action = press(&pane, delete);

        assert_eq!(action, Some(ModelsAction::Delete(file("a.gguf"))));
    }

    #[test]
    fn pressing_show_files_opens_the_repo() {
        let pane = searched("qwen", vec![repo(REPO, 1, false)]);
        let show_files = accessory_button(browse_rows(&pane).remove(0));

        let action = press(&pane, show_files);

        assert_eq!(action, Some(ModelsAction::ShowFiles(REPO.to_owned())));
    }

    #[test]
    fn pressing_back_returns_to_results() {
        let pane = browsing(vec![sized("a.gguf", 1)]);
        let back = browse(&pane).back.expect("the files list goes back");

        let action = press(&pane, back);

        assert_eq!(action, Some(ModelsAction::BackToResults));
    }

    #[test]
    fn pressing_retry_after_a_failed_search_searches_again() {
        let pane = failed_search(HubError::Offline);
        let retry = retry_button(browse(&pane));

        let action = press(&pane, retry);

        assert_eq!(action, Some(ModelsAction::Search("x".to_owned())));
    }

    #[test]
    fn pressing_a_button_that_the_pane_does_not_show_does_nothing() {
        let pane = ModelsPane::new();

        let action = press(&pane, button("Delete", &ModelsAction::Delete(file("a.gguf"))));

        assert_eq!(action, None);
    }
}
