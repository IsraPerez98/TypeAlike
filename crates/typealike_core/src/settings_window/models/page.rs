//! The page of the Models pane: the sections, rows and buttons that it shows.

use std::iter;

use crate::hub::{ByteSize, HubError, RepoSummary, SizedFile, rounded_quotient};
use crate::inference::Backend;
use crate::localization::{MessageArgument, Translations};
use crate::settings_window::models::catalog::{SUGGESTED_MODELS, SuggestedModel, recommended_model};
use crate::settings_window::models::{
    Buttons, Fetch, FileState, ModelsAction, ModelsPane, OpenRepo, SEARCH, Search, TrackedFile,
};
use crate::settings_window::view::{
    Accessory, ControlId, Item, Message, Page, PageIcon, Permille, Row, SearchField, Section,
};

/// The messages of the abbreviated counts of downloads, from the largest unit down.
const COUNT_UNITS: [(u64, &str); 3] = [
    (1_000_000_000, "models-downloads-billions"),
    (1_000_000, "models-downloads-millions"),
    (1_000, "models-downloads-thousands"),
];
const SMALLEST_ABBREVIATED_COUNT: u64 = 1_000;
const KILOBYTE: u64 = 1_000;
const MEGABYTE: u64 = 1_000_000;
const GIGABYTE: u64 = 1_000_000_000;

impl ModelsPane {
    pub(super) fn build_page(&self, buttons: &mut Buttons<'_>, backend: Backend) -> Page {
        let translations = buttons.translations;
        let browse = self.browse_section(buttons, backend);
        let downloaded = self.downloaded_section(buttons);
        Page {
            title: translations.text("models-title"),
            icon: PageIcon::Cube,
            summary: translations.text("models-summary"),
            search: Some(SearchField {
                id: ControlId::new(SEARCH),
                placeholder: translations.text("models-search"),
            }),
            sections: iter::once(browse).chain(downloaded).collect(),
        }
    }

    /// The row of the model that TypeAlike recommends for `backend`, while no model is downloaded.
    pub(super) fn build_recommended_row(&self, buttons: &mut Buttons<'_>, backend: Backend) -> Option<Row> {
        let has_installed = self
            .files
            .values()
            .any(|tracked| matches!(tracked.state, FileState::Installed));
        if has_installed {
            return None;
        }
        let model = self.recommended_model(backend);
        let shown = ShownFile {
            title: model.name.to_owned(),
            idle_subtitle: buttons.translations.text("model-recommended"),
            sized: model.sized_file(),
        };
        Some(self.file_row(shown, buttons))
    }

    fn recommended_model(&self, backend: Backend) -> &'static SuggestedModel {
        recommended_model(backend, self.gpu_memory.free_for(backend))
    }

    fn browse_section(&self, buttons: &mut Buttons<'_>, backend: Backend) -> Section {
        match &self.search {
            None => self.suggested_section(buttons, backend),
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

    fn suggested_section(&self, buttons: &mut Buttons<'_>, backend: Backend) -> Section {
        let translations = buttons.translations;
        let recommended = self.recommended_model(backend);
        let rows = SUGGESTED_MODELS
            .iter()
            .map(|model| {
                let byline = if model == recommended {
                    "models-developer-recommended"
                } else {
                    "models-developer"
                };
                let shown = ShownFile {
                    title: model.name.to_owned(),
                    idle_subtitle: translations
                        .text_with(byline, &[("developer", MessageArgument::Text(model.developer))]),
                    sized: model.sized_file(),
                };
                self.file_row(shown, buttons)
            })
            .collect();
        Section {
            heading: Some(translations.text("models-recommended-heading")),
            back: None,
            rows,
            footer: Some(translations.text("models-recommended-footer")),
        }
    }

    fn repo_files_section(&self, open_repo: &OpenRepo, buttons: &mut Buttons<'_>) -> Section {
        let translations = buttons.translations;
        let rows = match &open_repo.files {
            Fetch::Pending => vec![Row::Loading(translations.text("models-files-loading"))],
            Fetch::Failed(error) => {
                failure_message(*error, ModelsAction::ShowFiles(open_repo.id.clone()), buttons)
            }
            Fetch::Done(files) if files.is_empty() => message_rows(translations.text("models-no-files")),
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
            back: Some(buttons.add("models-back-to-results", ModelsAction::BackToResults)),
            rows,
            footer: Some(translations.text("models-files-footer")),
        }
    }

    fn downloaded_section(&self, buttons: &mut Buttons<'_>) -> Option<Section> {
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
            heading: Some(buttons.translations.text("models-downloaded-heading")),
            ..Section::new(rows)
        })
    }

    fn file_row(&self, shown: ShownFile, buttons: &mut Buttons<'_>) -> Row {
        let ShownFile {
            title,
            idle_subtitle,
            sized,
        } = shown;
        let Some(tracked) = self.files.get(&sized.file) else {
            return Row::Item(Item {
                title,
                subtitle: idle_subtitle,
                detail: size_text(sized.size, buttons.translations),
                accessory: Accessory::Download(buttons.add("models-download", ModelsAction::Download(sized))),
            });
        };
        let subtitle = match tracked.state {
            FileState::Failed(error) => buttons.translations.text(error_message(error)),
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

fn tracked_status(
    sized: &SizedFile,
    tracked: &TrackedFile,
    buttons: &mut Buttons<'_>,
) -> (String, Accessory) {
    let translations = buttons.translations;
    match tracked.state {
        FileState::Downloading { received } => {
            let progress = permille(received, tracked.size);
            let percent = MessageArgument::Number(u32::from(progress.0 / 10));
            (
                translations.text_with("models-download-progress", &[("percent", percent)]),
                Accessory::Downloading {
                    progress,
                    cancel: buttons.add("models-cancel-download", ModelsAction::Cancel(sized.file.clone())),
                },
            )
        }
        FileState::Canceling => (translations.text("models-canceling"), Accessory::Canceling),
        FileState::Installed => (
            size_text(tracked.size, translations),
            Accessory::Installed {
                delete: buttons.add("models-delete", ModelsAction::Delete(sized.file.clone())),
            },
        ),
        FileState::Failed(_) => (
            size_text(tracked.size, translations),
            Accessory::Failed {
                retry: buttons.add("models-try-again", ModelsAction::Download(sized.clone())),
            },
        ),
    }
}

fn results_section(query: &str, results: &Fetch<Vec<RepoSummary>>, buttons: &mut Buttons<'_>) -> Section {
    let translations = buttons.translations;
    let query_argument = [("query", MessageArgument::Text(query))];
    let rows = match results {
        Fetch::Pending => vec![Row::Loading(translations.text("models-searching"))],
        Fetch::Failed(error) => failure_message(*error, ModelsAction::Search(query.to_owned()), buttons),
        Fetch::Done(repos) if repos.is_empty() => {
            message_rows(translations.text_with("models-no-results", &query_argument))
        }
        Fetch::Done(repos) => repos.iter().map(|repo| repo_row(repo, buttons)).collect(),
    };
    Section {
        heading: Some(translations.text_with("models-results-heading", &query_argument)),
        back: None,
        rows,
        footer: Some(translations.text("models-results-footer")),
    }
}

fn repo_row(repo: &RepoSummary, buttons: &mut Buttons<'_>) -> Row {
    let translations = buttons.translations;
    let (owner, name) = repo.id.split_once('/').unwrap_or_default();
    let owner = [("owner", MessageArgument::Text(owner))];
    let (subtitle, accessory) = if repo.is_gated {
        (
            translations.text_with("models-owner-gated", &owner),
            Accessory::Unavailable,
        )
    } else {
        let show_files = buttons.add("models-show-files", ModelsAction::ShowFiles(repo.id.clone()));
        (
            translations.text_with("models-owner", &owner),
            Accessory::Open(show_files),
        )
    };
    Row::Item(Item {
        title: name.to_owned(),
        subtitle,
        detail: downloads_text(repo.downloads, translations),
        accessory,
    })
}

fn message_rows(text: String) -> Vec<Row> {
    vec![Row::Message(Message { text, button: None })]
}

fn failure_message(error: HubError, retry_action: ModelsAction, buttons: &mut Buttons<'_>) -> Vec<Row> {
    vec![Row::Message(Message {
        text: buttons.translations.text(error_message(error)),
        button: Some(buttons.add("models-try-again", retry_action)),
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
        HubError::Offline => "models-error-offline",
        HubError::AccessDenied => "models-error-account",
        HubError::NotFound => "models-error-not-found",
        HubError::Server => "models-error-server",
        HubError::InvalidResponse => "models-error-response",
        HubError::Storage => "models-error-storage",
        HubError::Canceled => "models-error-canceled",
    }
}

/// The text of `size` in decimal units, as Finder and Explorer show file sizes: whole bytes,
/// kilobytes and megabytes, and gigabytes with one digit after the decimal separator.
fn size_text(size: ByteSize, translations: &Translations) -> String {
    let (message, size) = match size.0 {
        bytes if bytes < KILOBYTE => ("models-size-bytes", whole_number(bytes)),
        bytes if bytes < MEGABYTE => (
            "models-size-kilobytes",
            whole_number(rounded_quotient(bytes, KILOBYTE)),
        ),
        bytes if bytes < GIGABYTE => (
            "models-size-megabytes",
            whole_number(rounded_quotient(bytes, MEGABYTE)),
        ),
        bytes => (
            "models-size-gigabytes",
            tenths(rounded_quotient(bytes, GIGABYTE / 10)),
        ),
    };
    translations.text_with(message, &[("size", size)])
}

/// The text of a count of downloads, with a unit from a thousand up and one digit after the
/// decimal separator when it is not zero.
fn downloads_text(count: u64, translations: &Translations) -> String {
    if count < SMALLEST_ABBREVIATED_COUNT {
        return translations.text_with("models-downloads", &[("count", whole_number(count))]);
    }
    let (count_tenths, message) = COUNT_UNITS
        .iter()
        .map(|&(unit, message)| (rounded_quotient(count, unit / 10), message))
        .find(|&(count_tenths, _)| count_tenths >= 10)
        .expect("a count of at least one thousand has a unit");
    let count = if count_tenths % 10 == 0 {
        whole_number(count_tenths / 10)
    } else {
        tenths(count_tenths)
    };
    translations.text_with(message, &[("count", count)])
}

fn whole_number(number: u64) -> MessageArgument<'static> {
    MessageArgument::Number(u32::try_from(number).unwrap_or(u32::MAX))
}

fn tenths(count_tenths: u64) -> MessageArgument<'static> {
    MessageArgument::Decimal {
        value: u32::try_from(count_tenths).unwrap_or(u32::MAX),
        digits: 1,
    }
}
