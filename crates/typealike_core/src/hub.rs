//! The seam to the service that finds, downloads and stores model files.

use std::sync::Arc;

/// An amount of data, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ByteSize(pub u64);

pub(crate) fn rounded_quotient(dividend: u64, divisor: u64) -> u64 {
    dividend.saturating_add(divisor / 2) / divisor
}

/// A file in a Hugging Face model repository.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelFile {
    /// Repository id, `owner/name`.
    pub repo: String,
    /// File name inside the repository.
    pub name: String,
}

const GGUF_EXTENSION: &str = ".gguf";
const VISION_PROJECTOR_MARKER: &str = "mmproj";
const DRAFT_MODEL_PREFIX: &str = "mtp-";
const SPLIT_PART_SEPARATOR: &str = "-of-";

impl ModelFile {
    /// The name that the settings window shows for the file: its file name without the GGUF
    /// extension.
    pub(crate) fn model_name(&self) -> String {
        self.name
            .strip_suffix(GGUF_EXTENSION)
            .unwrap_or(&self.name)
            .to_owned()
    }

    /// Returns the file of the drafter of this model, if it has one: a small model that guesses the
    /// next tokens, so that a suggestion takes fewer steps of this model. It downloads and deletes
    /// with the model.
    #[must_use]
    pub fn drafter(&self) -> Option<ModelFile> {
        crate::settings_window::models::drafter_of(self)
    }
}

/// A model file and how big it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizedFile {
    /// Which file.
    pub file: ModelFile,
    /// Its size on the hub or on disk.
    pub size: ByteSize,
}

/// The files of `files` that TypeAlike can load as a language model, smallest first.
pub(crate) fn usable_model_files(files: Vec<SizedFile>) -> Vec<SizedFile> {
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

/// A model repository found by a search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSummary {
    /// Repository id, `owner/name`.
    pub id: String,
    /// How often the repository was downloaded recently.
    pub downloads: u64,
    /// Whether the owner requires an account and accepted terms before download.
    pub is_gated: bool,
}

/// Work for the hub. Each request answers with the [`HubEvent`]s named in its variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubRequest {
    /// Lists the downloaded model files: [`HubEvent::Installed`].
    ListInstalled,
    /// Finds GGUF model repositories matching `query`: [`HubEvent::SearchFinished`].
    Search {
        /// Words to look for, never empty.
        query: String,
    },
    /// Lists the files at the top of `repo`: [`HubEvent::FilesListed`].
    ListFiles {
        /// Repository id, `owner/name`.
        repo: String,
    },
    /// Downloads `file`: [`HubEvent::DownloadProgress`] while it runs, then [`HubEvent::DownloadFinished`].
    Download {
        /// The file to download.
        file: ModelFile,
    },
    /// Stops a running download of `file`, which then finishes with [`HubError::Canceled`].
    CancelDownload {
        /// The file being downloaded.
        file: ModelFile,
    },
    /// Deletes the downloaded `file`. No event follows.
    Delete {
        /// The file to delete.
        file: ModelFile,
    },
}

/// The outcome of a [`HubRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubEvent {
    /// Every downloaded model file.
    Installed(Vec<SizedFile>),
    /// Repositories matching `query`, most downloaded first.
    SearchFinished {
        /// The query of the request.
        query: String,
        /// The repositories, or why the search failed.
        result: Result<Vec<RepoSummary>, HubError>,
    },
    /// The files at the top of `repo`.
    FilesListed {
        /// The repository of the request.
        repo: String,
        /// The files, or why listing failed.
        result: Result<Vec<SizedFile>, HubError>,
    },
    /// A running download of `file` stored `received` bytes so far.
    DownloadProgress {
        /// The file being downloaded.
        file: ModelFile,
        /// Bytes stored so far.
        received: ByteSize,
    },
    /// A download of `file` ended.
    DownloadFinished {
        /// The downloaded file.
        file: ModelFile,
        /// The stored size, or why the download stopped.
        result: Result<ByteSize, HubError>,
    },
}

/// Why a hub request failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubError {
    /// The hub could not be reached.
    Offline,
    /// The repository is gated or private.
    AccessDenied,
    /// The repository or file does not exist.
    NotFound,
    /// The hub answered with an error of its own.
    Server,
    /// The hub answered with something unexpected.
    InvalidResponse,
    /// The file could not be written to disk.
    Storage,
    /// The user canceled the download.
    Canceled,
}

/// Receives hub events. The hub may call it from any thread.
pub type HubEventSink = Arc<dyn Fn(HubEvent) + Send + Sync>;

/// Finds, downloads and deletes model files without blocking the caller.
pub trait ModelHub {
    /// Starts `request` and returns at once. Its events reach `sink` in the order they happen.
    fn submit(&mut self, request: HubRequest, sink: &HubEventSink);
}
