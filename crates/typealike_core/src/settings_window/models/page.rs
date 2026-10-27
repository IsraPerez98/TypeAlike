//! The page of the Models pane: the sections, rows and buttons that it shows.

use std::iter;

use crate::hub::{ByteSize, HubError, RepoSummary, SizedFile, rounded_quotient};
use crate::settings_window::models::catalog::SUGGESTED_MODELS;
use crate::settings_window::models::{
    Buttons, Fetch, FileState, ModelsAction, ModelsPane, OpenRepo, SEARCH, Search, TrackedFile,
};
use crate::settings_window::view::{
    Accessory, ControlId, Item, Message, Page, PageIcon, Permille, Row, SearchField, Section,
};

const PANE_TITLE: &str = "Models";
const PANE_SUMMARY: &str = "TypeAlike uses a language model on this computer to suggest text.";
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
const NO_USABLE_FILES_TEXT: &str = "This model has no GGUF files that TypeAlike can use.";
const GATED_REPO_NOTE: &str = "Needs a Hugging Face account";
const CANCELING_DETAIL: &str = "Canceling…";
const COUNT_UNITS: [(u64, &str); 3] = [(1_000_000_000, "B"), (1_000_000, "M"), (1_000, "K")];
const SMALLEST_ABBREVIATED_COUNT: u64 = 1_000;

impl ModelsPane {
    pub(super) fn build_page(&self, buttons: &mut Buttons) -> Page {
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
            Fetch::Done(files) if files.is_empty() => message_rows(NO_USABLE_FILES_TEXT.to_owned()),
            Fetch::Done(files) => files
                .iter()
                .map(|sized| {
                    let shown = ShownFile {
                        title: sized.file.model_name(),
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
                    title: file.model_name(),
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
        Fetch::Done(repos) if repos.is_empty() => message_rows(format!("No models match “{query}”.")),
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

fn message_rows(text: String) -> Vec<Row> {
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
        .saturating_mul(u64::from(Permille::FULL.0))
        .checked_div(total.0)
        .unwrap_or_default();
    Permille(
        u16::try_from(thousandths)
            .unwrap_or(Permille::FULL.0)
            .min(Permille::FULL.0),
    )
}

fn error_message(error: HubError) -> &'static str {
    match error {
        HubError::Offline => "TypeAlike cannot connect to Hugging Face. Check your internet connection.",
        HubError::AccessDenied => "This model needs a Hugging Face account.",
        HubError::NotFound => "Hugging Face cannot find this model.",
        HubError::Server => "Hugging Face is not available. Try again later.",
        HubError::InvalidResponse => "Hugging Face sent an incorrect response. Try again later.",
        HubError::Storage => "TypeAlike cannot save the model. Make sure that the disk has free space.",
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
