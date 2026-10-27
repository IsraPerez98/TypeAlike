//! The Hugging Face Hub HTTP API: model search, repository file lists and file downloads.

use std::fs::{self, File};
use std::io::{self, Read as _, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;
use typealike_core::hub::{ByteSize, HubError, ModelFile, RepoSummary, SizedFile};
use ureq::Agent;

use crate::store;

const HUB_URL: &str = "https://huggingface.co";
const USER_AGENT: &str = concat!("TypeAlike/", env!("CARGO_PKG_VERSION"));
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const SEARCH_LIMIT: &str = "20";
const FILE_ENTRY_KIND: &str = "file";
const DOWNLOAD_CHUNK_BYTES: usize = 256 * 1024;
const PROGRESS_INTERVAL_BYTES: u64 = 4 * 1024 * 1024;

/// Why a request to the hub failed, with the details worth logging.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FetchError {
    #[error("hugging face request failed")]
    Http(#[from] ureq::Error),
    #[error("hugging face sent malformed json")]
    Json(#[from] serde_json::Error),
    #[error("download connection broke")]
    Network(#[source] io::Error),
    #[error("failed to store the model file")]
    Storage(#[source] io::Error),
    #[error("refused an unsafe repository or file name")]
    UnsafeName,
    #[error("download canceled")]
    Canceled,
}

impl FetchError {
    pub(crate) fn to_hub_error(&self) -> HubError {
        match self {
            Self::Http(ureq::Error::StatusCode(401 | 403)) => HubError::AccessDenied,
            Self::Http(ureq::Error::StatusCode(404)) => HubError::NotFound,
            Self::Http(ureq::Error::StatusCode(_)) => HubError::Server,
            Self::Http(_) | Self::Network(_) => HubError::Offline,
            Self::Json(_) | Self::UnsafeName => HubError::InvalidResponse,
            Self::Storage(_) => HubError::Storage,
            Self::Canceled => HubError::Canceled,
        }
    }
}

/// Returns an HTTPS-only client with bounded connect and response waits.
pub(crate) fn new_agent() -> Agent {
    Agent::config_builder()
        .https_only(true)
        .user_agent(USER_AGENT)
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_response(Some(RESPONSE_TIMEOUT))
        .build()
        .into()
}

/// Finds the most downloaded GGUF repositories matching `query`.
pub(crate) fn search(agent: &Agent, query: &str) -> Result<Vec<RepoSummary>, FetchError> {
    let body = agent
        .get(format!("{HUB_URL}/api/models"))
        .query("search", query)
        .query("filter", "gguf")
        .query("sort", "downloads")
        .query("direction", "-1")
        .query("limit", SEARCH_LIMIT)
        .query("expand[]", "downloads")
        .query("expand[]", "gated")
        .call()?
        .body_mut()
        .read_to_string()?;
    Ok(parse_search(&body)?)
}

/// Lists the files at the top of `repo`.
pub(crate) fn list_files(agent: &Agent, repo: &str) -> Result<Vec<SizedFile>, FetchError> {
    if !store::is_safe_repo_id(repo) {
        return Err(FetchError::UnsafeName);
    }
    let body = agent
        .get(format!("{HUB_URL}/api/models/{repo}/tree/main"))
        .call()?
        .body_mut()
        .read_to_string()?;
    Ok(parse_tree(repo, &body)?)
}

/// Downloads `file` into `models_dir`, reporting the bytes stored so far to `on_progress`.
///
/// Stops with [`FetchError::Canceled`] soon after `cancel` turns true. A download that fails or is
/// canceled leaves no file behind.
// ponytail: a quit mid-download leaves the `.part` file. Sweep them before a download if disk use matters.
pub(crate) fn download(
    agent: &Agent,
    models_dir: &Path,
    file: &ModelFile,
    cancel: &AtomicBool,
    on_progress: impl FnMut(ByteSize),
) -> Result<ByteSize, FetchError> {
    let destination = store::file_path(models_dir, file).ok_or(FetchError::UnsafeName)?;
    let partial = store::partial_path(&destination);
    let url = format!("{HUB_URL}/{}/resolve/main/{}", file.repo, file.name);
    let result = stream_to_file(agent, &url, &partial, cancel, on_progress).and_then(|size| {
        fs::rename(&partial, &destination)
            .map(|()| size)
            .map_err(FetchError::Storage)
    });
    if result.is_err()
        && let Err(error) = fs::remove_file(&partial)
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "failed to remove a partial download");
    }
    result
}

// ponytail: no idle timeout per read, so a stalled connection delays cancel. Add one if users hit it.
fn stream_to_file(
    agent: &Agent,
    url: &str,
    partial: &Path,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(ByteSize),
) -> Result<ByteSize, FetchError> {
    let mut response = agent.get(url).call()?;
    if let Some(dir) = partial.parent() {
        fs::create_dir_all(dir).map_err(FetchError::Storage)?;
    }
    let mut output = File::create(partial).map_err(FetchError::Storage)?;
    let mut reader = response.body_mut().as_reader();
    let mut buffer = vec![0; DOWNLOAD_CHUNK_BYTES];
    let mut received = 0_u64;
    let mut reported = 0_u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(FetchError::Canceled);
        }
        let count = reader.read(&mut buffer).map_err(FetchError::Network)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count]).map_err(FetchError::Storage)?;
        received = received.saturating_add(u64::try_from(count).expect("a read never exceeds u64"));
        if received - reported >= PROGRESS_INTERVAL_BYTES {
            on_progress(ByteSize(received));
            reported = received;
        }
    }
    output.sync_all().map_err(FetchError::Storage)?;
    Ok(ByteSize(received))
}

#[derive(Deserialize)]
struct SearchEntry {
    id: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    gated: serde_json::Value,
}

#[derive(Deserialize)]
struct TreeEntry {
    #[serde(rename = "type")]
    kind: String,
    path: String,
    #[serde(default)]
    size: u64,
}

fn parse_search(json: &str) -> Result<Vec<RepoSummary>, serde_json::Error> {
    let entries: Vec<SearchEntry> = serde_json::from_str(json)?;
    Ok(entries
        .into_iter()
        .filter(|entry| store::is_safe_repo_id(&entry.id))
        .map(|entry| RepoSummary {
            is_gated: !matches!(
                entry.gated,
                serde_json::Value::Null | serde_json::Value::Bool(false)
            ),
            id: entry.id,
            downloads: entry.downloads,
        })
        .collect())
}

fn parse_tree(repo: &str, json: &str) -> Result<Vec<SizedFile>, serde_json::Error> {
    let entries: Vec<TreeEntry> = serde_json::from_str(json)?;
    Ok(entries
        .into_iter()
        .filter(|entry| entry.kind == FILE_ENTRY_KIND && store::is_safe_name(&entry.path))
        .map(|entry| SizedFile {
            file: ModelFile {
                repo: repo.to_owned(),
                name: entry.path,
            },
            size: ByteSize(entry.size),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_reads_id_downloads_and_gating() {
        let json = r#"[
            {"_id":"1","id":"unsloth/Qwen3.5-9B-GGUF","gated":false,"downloads":1229091},
            {"_id":"2","id":"google/gemma-3-1b-it","gated":"manual","downloads":2414703},
            {"_id":"3","id":"someone/no-fields"}
        ]"#;

        let repos = parse_search(json).expect("json is valid");

        assert_eq!(
            repos,
            [
                RepoSummary {
                    id: "unsloth/Qwen3.5-9B-GGUF".to_owned(),
                    downloads: 1_229_091,
                    is_gated: false,
                },
                RepoSummary {
                    id: "google/gemma-3-1b-it".to_owned(),
                    downloads: 2_414_703,
                    is_gated: true,
                },
                RepoSummary {
                    id: "someone/no-fields".to_owned(),
                    downloads: 0,
                    is_gated: false,
                },
            ]
        );
    }

    #[test]
    fn search_drops_unsafe_repo_ids() {
        let json = r#"[{"id":"../escape","downloads":1}]"#;

        let repos = parse_search(json).expect("json is valid");

        assert_eq!(repos, Vec::new());
    }

    #[test]
    fn tree_keeps_top_level_files_with_sizes() {
        let json = r#"[
            {"type":"directory","path":"BF16","size":0},
            {"type":"file","path":"README.md","size":1094},
            {"type":"file","path":"q4.gguf","size":59235872,"lfs":{"size":59235872}},
            {"type":"file","path":"BF16/x.gguf","size":3}
        ]"#;

        let files = parse_tree("owner/repo", json).expect("json is valid");

        let names_and_sizes: Vec<_> = files
            .into_iter()
            .map(|sized| (sized.file.name, sized.size))
            .collect();
        assert_eq!(
            names_and_sizes,
            [
                ("README.md".to_owned(), ByteSize(1094)),
                ("q4.gguf".to_owned(), ByteSize(59_235_872))
            ]
        );
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse_tree("owner/repo", "<html>").is_err());
    }

    #[test]
    fn errors_map_to_what_the_user_can_act_on() {
        let cases = [
            (
                FetchError::Http(ureq::Error::StatusCode(401)),
                HubError::AccessDenied,
            ),
            (
                FetchError::Http(ureq::Error::StatusCode(403)),
                HubError::AccessDenied,
            ),
            (FetchError::Http(ureq::Error::StatusCode(404)), HubError::NotFound),
            (FetchError::Http(ureq::Error::StatusCode(500)), HubError::Server),
            (FetchError::Http(ureq::Error::HostNotFound), HubError::Offline),
            (
                FetchError::Network(io::ErrorKind::ConnectionReset.into()),
                HubError::Offline,
            ),
            (FetchError::UnsafeName, HubError::InvalidResponse),
            (
                FetchError::Storage(io::ErrorKind::StorageFull.into()),
                HubError::Storage,
            ),
            (FetchError::Canceled, HubError::Canceled),
        ];

        let mapped = cases.each_ref().map(|(error, _)| error.to_hub_error());

        assert_eq!(mapped, cases.map(|(_, expected)| expected));
    }

    #[test]
    fn unsafe_repo_is_not_requested() {
        let result = list_files(&new_agent(), "../../etc");

        assert!(matches!(result, Err(FetchError::UnsafeName)));
    }
}
