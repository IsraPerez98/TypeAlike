//! The Hugging Face Hub HTTP API: model search, repository file lists and file downloads.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Read, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ring::digest::{Context, SHA256};
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
    #[error("the repository lists no file of that name")]
    MissingFile,
    #[error("the download has another size or sha-256 digest than the repository lists")]
    Mismatch,
    #[error("download canceled")]
    Canceled,
}

impl FetchError {
    pub(crate) fn to_hub_error(&self) -> HubError {
        match self {
            Self::Http(ureq::Error::StatusCode(401 | 403)) => HubError::AccessDenied,
            Self::Http(ureq::Error::StatusCode(404)) | Self::MissingFile => HubError::NotFound,
            Self::Http(ureq::Error::StatusCode(_)) => HubError::Server,
            Self::Http(_) | Self::Network(_) => HubError::Offline,
            Self::Json(_) | Self::UnsafeName | Self::Mismatch => HubError::InvalidResponse,
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
    Ok(parse_tree(repo, &fetch_tree(agent, repo)?)?)
}

fn fetch_tree(agent: &Agent, repo: &str) -> Result<String, FetchError> {
    if !store::is_safe_repo_id(repo) {
        return Err(FetchError::UnsafeName);
    }
    Ok(agent
        .get(format!("{HUB_URL}/api/models/{repo}/tree/main"))
        .call()?
        .body_mut()
        .read_to_string()?)
}

/// Downloads `file` into `models_dir`, reporting the bytes stored so far to `on_progress`.
///
/// Keeps the download only when it has the size and the SHA-256 digest that the repository lists.
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
    let listed = listed_file(&fetch_tree(agent, &file.repo)?, &file.name)?;
    let partial = store::partial_path(&destination);
    let url = format!("{HUB_URL}/{}/resolve/main/{}", file.repo, file.name);
    let result = agent
        .get(url)
        .call()
        .map_err(FetchError::from)
        .and_then(|mut response| {
            write_partial(response.body_mut().as_reader(), &partial, cancel, on_progress)
        })
        .and_then(|received| listed.check(&received))
        .and_then(|size| {
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

/// The size and the SHA-256 digest of a downloaded body.
#[derive(Debug, PartialEq, Eq)]
struct Received {
    size: ByteSize,
    /// The digest in lowercase hex.
    sha256: String,
}

// ponytail: no idle timeout per read, so a stalled connection delays cancel. Add one if users hit it.
fn write_partial(
    mut body: impl Read,
    partial: &Path,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(ByteSize),
) -> Result<Received, FetchError> {
    if let Some(dir) = partial.parent() {
        fs::create_dir_all(dir).map_err(FetchError::Storage)?;
    }
    let mut output = File::create(partial).map_err(FetchError::Storage)?;
    let mut digest = Context::new(&SHA256);
    let mut buffer = vec![0; DOWNLOAD_CHUNK_BYTES];
    let mut received = 0_u64;
    let mut reported = 0_u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(FetchError::Canceled);
        }
        let count = body.read(&mut buffer).map_err(FetchError::Network)?;
        if count == 0 {
            break;
        }
        let chunk = &buffer[..count];
        output.write_all(chunk).map_err(FetchError::Storage)?;
        digest.update(chunk);
        received = received.saturating_add(u64::try_from(count).expect("a read never exceeds u64"));
        if received - reported >= PROGRESS_INTERVAL_BYTES {
            on_progress(ByteSize(received));
            reported = received;
        }
    }
    output.sync_all().map_err(FetchError::Storage)?;
    Ok(Received {
        size: ByteSize(received),
        sha256: to_hex(digest.finish().as_ref()),
    })
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, byte| {
        write!(hex, "{byte:02x}").expect("a write to a String never fails");
        hex
    })
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
    lfs: Option<LargeFileEntry>,
}

/// The large file storage part of a tree entry.
#[derive(Deserialize)]
struct LargeFileEntry {
    /// The SHA-256 digest of the file in hex.
    oid: Option<String>,
}

/// The size and the SHA-256 digest that a repository lists for a file.
#[derive(Debug, PartialEq, Eq)]
struct ListedFile {
    size: ByteSize,
    /// The digest in hex. Only a file in large file storage has one.
    sha256: Option<String>,
}

impl ListedFile {
    /// Returns the size of `received` when it has the listed size and digest.
    fn check(&self, received: &Received) -> Result<ByteSize, FetchError> {
        let has_listed_digest = self
            .sha256
            .as_ref()
            .is_none_or(|sha256| sha256.eq_ignore_ascii_case(&received.sha256));
        if received.size == self.size && has_listed_digest {
            Ok(received.size)
        } else {
            Err(FetchError::Mismatch)
        }
    }
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

fn listed_file(json: &str, name: &str) -> Result<ListedFile, FetchError> {
    let entries: Vec<TreeEntry> = serde_json::from_str(json)?;
    entries
        .into_iter()
        .find(|entry| entry.kind == FILE_ENTRY_KIND && entry.path == name)
        .map(|entry| ListedFile {
            size: ByteSize(entry.size),
            sha256: entry.lfs.and_then(|lfs| lfs.oid),
        })
        .ok_or(FetchError::MissingFile)
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

    const GGUF_SHA256: &str = "b83633aa785344791618f2fddf131b010ea04912a60430760b070bad293f65bd";
    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn tree_lists_the_size_and_the_sha256_of_a_file() {
        let json = r#"[
            {"type":"file","oid":"a1b2","path":"README.md","size":1094},
            {"type":"file","oid":"c3d4","path":"q4.gguf","size":4,"lfs":{"oid":"B83633AA785344791618F2FDDF131B010EA04912A60430760B070BAD293F65BD","size":4,"pointerSize":127}}
        ]"#;

        let listed = ["q4.gguf", "README.md"].map(|name| listed_file(json, name).ok());

        assert_eq!(
            listed,
            [
                Some(ListedFile {
                    size: ByteSize(4),
                    sha256: Some(GGUF_SHA256.to_uppercase()),
                }),
                Some(ListedFile {
                    size: ByteSize(1094),
                    sha256: None,
                }),
            ]
        );
    }

    #[test]
    fn file_that_the_tree_does_not_list_is_not_found() {
        let json = r#"[{"type":"file","path":"q4.gguf","size":4}]"#;

        let listed = listed_file(json, "q8.gguf");

        assert!(matches!(listed, Err(FetchError::MissingFile)));
    }

    #[test]
    fn body_is_stored_with_its_size_and_sha256() {
        let partial = std::env::temp_dir().join(format!("typealike-api-{}-body.part", std::process::id()));

        let received = write_partial(&b"GGUF"[..], &partial, &AtomicBool::new(false), |_| {});

        let stored = fs::read(&partial).ok();
        fs::remove_file(&partial).ok();
        assert_eq!(
            (received.ok(), stored),
            (
                Some(Received {
                    size: ByteSize(4),
                    sha256: GGUF_SHA256.to_owned(),
                }),
                Some(b"GGUF".to_vec())
            )
        );
    }

    #[test]
    fn download_is_kept_only_with_the_listed_size_and_sha256() {
        let listed = ListedFile {
            size: ByteSize(4),
            sha256: Some(GGUF_SHA256.to_uppercase()),
        };
        let unlisted_digest = ListedFile {
            size: ByteSize(4),
            sha256: None,
        };
        let cases = [
            (&listed, ByteSize(4), GGUF_SHA256, Some(ByteSize(4))),
            (&listed, ByteSize(3), GGUF_SHA256, None),
            (&listed, ByteSize(4), EMPTY_SHA256, None),
            (&unlisted_digest, ByteSize(4), EMPTY_SHA256, Some(ByteSize(4))),
            (&unlisted_digest, ByteSize(5), GGUF_SHA256, None),
        ];

        let kept = cases.each_ref().map(|(listed, size, sha256, _)| {
            listed
                .check(&Received {
                    size: *size,
                    sha256: (*sha256).to_owned(),
                })
                .ok()
        });

        assert_eq!(kept, cases.map(|(_, _, _, expected)| expected));
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
            (FetchError::Http(ureq::Error::StatusCode(429)), HubError::Server),
            (FetchError::Http(ureq::Error::StatusCode(500)), HubError::Server),
            (FetchError::Http(ureq::Error::HostNotFound), HubError::Offline),
            (
                FetchError::Network(io::ErrorKind::ConnectionReset.into()),
                HubError::Offline,
            ),
            (FetchError::UnsafeName, HubError::InvalidResponse),
            (FetchError::Mismatch, HubError::InvalidResponse),
            (FetchError::MissingFile, HubError::NotFound),
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
