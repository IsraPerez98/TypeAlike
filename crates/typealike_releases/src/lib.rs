//! The newest release of TypeAlike on GitHub: the check for updates of the About pane, and the
//! download and the install of a new version.
//!
//! [`GitHubReleases`] implements the core [`ReleaseFeed`] seam. Each read and each download runs on
//! its own thread and reports through the caller's sink, so the network never blocks the caller's
//! thread.

mod install;

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Read, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ring::digest::{Context, SHA256};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use typealike_core::release::{
    DownloadSink, DownloadedRelease, ReleaseDownloadFailed, ReleaseFeed, ReleaseInstallFailed,
    ReleaseReadFailed, ReleaseSink, ReleaseTag, SelfUpdate,
};
use ureq::Agent;

const RELEASES_URL: &str = "https://api.github.com/repos/IsraPerez98/TypeAlike/releases";
const ACCEPT: &str = "application/vnd.github+json";
const USER_AGENT: &str = concat!("TypeAlike/", env!("CARGO_PKG_VERSION"));
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
/// The longest download of an installer, so that a stalled connection does not block the next
/// check forever.
const BODY_TIMEOUT: Duration = Duration::from_hours(1);
const UPDATE_FOLDER: &str = "TypeAlike-update";
const DIGEST_PREFIX: &str = "sha256:";
const DOWNLOAD_CHUNK_BYTES: usize = 64 * 1024;

/// Why a read of the newest release failed, with the details worth logging.
#[derive(Debug, thiserror::Error)]
enum FetchError {
    #[error("github request failed")]
    Http(#[from] ureq::Error),
    #[error("github sent malformed json")]
    Json(#[from] serde_json::Error),
}

/// Why the download of a release failed, with the details worth logging.
#[derive(Debug, thiserror::Error)]
enum DownloadError {
    #[error("failed to read the release")]
    Release(#[from] FetchError),
    #[error("the release has no file that ends with {0}")]
    MissingAsset(&'static str),
    #[error("the release has more than one file that ends with {0}")]
    SeveralAssets(&'static str),
    #[error("the file of the release has no sha-256 digest")]
    MissingDigest,
    #[error("download failed")]
    Network(#[source] ureq::Error),
    #[error("failed to read the downloaded bytes")]
    Read(#[source] io::Error),
    #[error("failed to store the download")]
    Storage(#[source] io::Error),
    #[error("the download is larger than the file of the release")]
    TooLarge,
    #[error("the sha-256 digest of the download differs from the digest of the release")]
    DigestMismatch,
    #[error("failed to prepare the install")]
    Prepare(#[source] io::Error),
}

/// The part of a release of the GitHub API that the check and the download need.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// A file of a release.
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// `sha256:` and the digest in hex.
    digest: Option<String>,
    /// The size in bytes.
    size: Option<u64>,
}

/// Reads the newest release of TypeAlike from the GitHub API, and downloads and installs it. Drafts
/// and pre-releases do not count.
#[derive(Debug)]
pub struct GitHubReleases {
    agent: Agent,
    asset_suffix: &'static str,
    installed_copy: Option<PathBuf>,
    update_folder: PathBuf,
}

impl GitHubReleases {
    /// Creates a feed with an HTTPS-only client that has bounded waits. `asset_suffix` is the end of
    /// the name of the release file that installs this build, such as `-macos-arm64.dmg`. Removes
    /// the files that an earlier update left in the temporary folder.
    #[must_use]
    pub fn new(asset_suffix: &'static str) -> Self {
        let agent = Agent::config_builder()
            .https_only(true)
            .user_agent(USER_AGENT)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_response(Some(RESPONSE_TIMEOUT))
            .timeout_recv_body(Some(BODY_TIMEOUT))
            .build()
            .into();
        let update_folder = std::env::temp_dir().join(UPDATE_FOLDER);
        remove_folder(&update_folder);
        Self {
            agent,
            asset_suffix,
            installed_copy: install::installed_copy(),
            update_folder,
        }
    }
}

impl ReleaseFeed for GitHubReleases {
    fn read_newest(&mut self, sink: &ReleaseSink) {
        let agent = self.agent.clone();
        let sink = Arc::clone(sink);
        thread::spawn(move || {
            let release = fetch_release(&agent, &format!("{RELEASES_URL}/latest"))
                .map(|release| ReleaseTag(release.tag_name))
                .map_err(|error| {
                    tracing::warn!(?error, "failed to read the newest release");
                    ReleaseReadFailed
                });
            sink(release);
        });
    }

    fn self_update(&self) -> SelfUpdate {
        if self.installed_copy.is_some() {
            SelfUpdate::Supported
        } else {
            SelfUpdate::Unsupported
        }
    }

    fn download(&mut self, tag: &ReleaseTag, sink: &DownloadSink) {
        let agent = self.agent.clone();
        let sink = Arc::clone(sink);
        let tag = tag.clone();
        let asset_suffix = self.asset_suffix;
        let folder = self.update_folder.clone();
        thread::spawn(move || {
            let release = download_release(&agent, &tag, asset_suffix, &folder).map_err(|error| {
                tracing::warn!(?error, tag = tag.0, "failed to download the release");
                ReleaseDownloadFailed
            });
            sink(release.map(DownloadedRelease));
        });
    }

    fn install(&mut self, release: &DownloadedRelease) -> Result<(), ReleaseInstallFailed> {
        let Some(installed) = &self.installed_copy else {
            return Err(ReleaseInstallFailed);
        };
        install::start_install(&release.0, installed, &self.update_folder).map_err(|error| {
            tracing::warn!(%error, "failed to install the release");
            ReleaseInstallFailed
        })?;
        tracing::info!("the install of the new version started, TypeAlike quits");
        Ok(())
    }
}

fn fetch_release(agent: &Agent, url: &str) -> Result<Release, FetchError> {
    let body = agent
        .get(url)
        .header("Accept", ACCEPT)
        .call()?
        .body_mut()
        .read_to_string()?;
    Ok(parse_json(&body)?)
}

fn parse_json<Parsed: DeserializeOwned>(body: &str) -> Result<Parsed, serde_json::Error> {
    serde_json::from_str(body)
}

/// Downloads the file of the release of `tag` that ends with `asset_suffix` into `folder`, makes
/// sure that it has the digest of the release, and prepares its install.
fn download_release(
    agent: &Agent,
    tag: &ReleaseTag,
    asset_suffix: &'static str,
    folder: &Path,
) -> Result<PathBuf, DownloadError> {
    let release = fetch_release(agent, &format!("{RELEASES_URL}/tags/{}", tag.0))?;
    let asset = release_asset(release.assets, asset_suffix)?;
    let expected_digest = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix(DIGEST_PREFIX))
        .ok_or(DownloadError::MissingDigest)?;
    remove_folder(folder);
    fs::create_dir_all(folder).map_err(DownloadError::Storage)?;
    let download = folder.join(format!("TypeAlike{asset_suffix}"));
    let mut response = agent
        .get(&asset.browser_download_url)
        .call()
        .map_err(DownloadError::Network)?;
    store_download(
        response.body_mut().as_reader(),
        &download,
        expected_digest,
        asset.size,
    )?;
    install::prepare(&download, folder).map_err(DownloadError::Prepare)
}

/// The only file of `assets` whose name ends with `asset_suffix`.
fn release_asset(assets: Vec<Asset>, asset_suffix: &'static str) -> Result<Asset, DownloadError> {
    let mut matching = assets
        .into_iter()
        .filter(|asset| asset.name.ends_with(asset_suffix));
    let asset = matching.next().ok_or(DownloadError::MissingAsset(asset_suffix))?;
    match matching.next() {
        Some(_) => Err(DownloadError::SeveralAssets(asset_suffix)),
        None => Ok(asset),
    }
}

/// Writes `body` to `path` when it has the `expected_digest`, and stops when it grows past `size`.
/// A download that fails leaves no file behind.
fn store_download(
    body: impl Read,
    path: &Path,
    expected_digest: &str,
    size: Option<u64>,
) -> Result<(), DownloadError> {
    let result = write_body(body, path, size).and_then(|digest| {
        if digest.eq_ignore_ascii_case(expected_digest) {
            Ok(())
        } else {
            Err(DownloadError::DigestMismatch)
        }
    });
    if result.is_err()
        && let Err(error) = fs::remove_file(path)
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "failed to remove a failed download of a release");
    }
    result
}

/// Writes `body` to `path`, and returns its SHA-256 digest in hex.
fn write_body(mut body: impl Read, path: &Path, size: Option<u64>) -> Result<String, DownloadError> {
    let mut output = File::create(path).map_err(DownloadError::Storage)?;
    let mut digest = Context::new(&SHA256);
    let mut buffer = vec![0; DOWNLOAD_CHUNK_BYTES];
    let mut received = 0_u64;
    loop {
        let count = body.read(&mut buffer).map_err(DownloadError::Read)?;
        if count == 0 {
            break;
        }
        received = received.saturating_add(u64::try_from(count).expect("a read never exceeds u64"));
        if size.is_some_and(|size| received > size) {
            return Err(DownloadError::TooLarge);
        }
        let chunk = &buffer[..count];
        output.write_all(chunk).map_err(DownloadError::Storage)?;
        digest.update(chunk);
    }
    output.sync_all().map_err(DownloadError::Storage)?;
    Ok(to_hex(digest.finish().as_ref()))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, byte| {
        write!(hex, "{byte:02x}").expect("a write to a String never fails");
        hex
    })
}

/// Removes `folder` and all that it holds. A file that a running installer holds stays until the
/// next start.
fn remove_folder(folder: &Path) {
    if let Err(error) = fs::remove_dir_all(folder)
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::debug!(%error, "failed to remove the files of an earlier update");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE_JSON: &str = r#"{
        "tag_name": "v0.7.0",
        "draft": false,
        "assets": [
            {
                "name": "TypeAlike-0.7.0-macos-arm64.dmg",
                "browser_download_url": "https://github.com/IsraPerez98/TypeAlike/releases/download/v0.7.0/TypeAlike-0.7.0-macos-arm64.dmg",
                "digest": "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
            },
            {
                "name": "TypeAlike-0.7.0-windows-x64-cuda-setup.exe",
                "browser_download_url": "https://github.com/IsraPerez98/TypeAlike/releases/download/v0.7.0/TypeAlike-0.7.0-windows-x64-cuda-setup.exe",
                "digest": null
            }
        ]
    }"#;

    #[test]
    fn release_reads_its_tag_and_its_files() {
        let release: Release = parse_json(RELEASE_JSON).expect("valid json");

        let assets: Vec<_> = release
            .assets
            .iter()
            .map(|asset| (asset.name.as_str(), asset.digest.is_some()))
            .collect();
        assert_eq!(
            (release.tag_name.as_str(), assets),
            (
                "v0.7.0",
                vec![
                    ("TypeAlike-0.7.0-macos-arm64.dmg", true),
                    ("TypeAlike-0.7.0-windows-x64-cuda-setup.exe", false),
                ]
            )
        );
    }

    #[test]
    fn release_without_files_has_no_assets() {
        let release: Release =
            parse_json(r#"{"tag_name":"v0.4.0","name":"TypeAlike 0.4.0"}"#).expect("valid json");

        assert_eq!((release.tag_name.as_str(), release.assets.len()), ("v0.4.0", 0));
    }

    #[test]
    fn answer_without_a_tag_is_no_release() {
        let release = parse_json::<Release>(r#"{"message":"Not Found"}"#);

        assert!(release.is_err());
    }

    #[test]
    fn release_with_one_file_for_this_build_gives_that_file() {
        let release: Release = parse_json(RELEASE_JSON).expect("valid json");

        let asset = release_asset(release.assets, "-macos-arm64.dmg");

        assert_eq!(
            asset.map(|asset| asset.name).ok(),
            Some("TypeAlike-0.7.0-macos-arm64.dmg".to_owned())
        );
    }

    #[test]
    fn release_with_two_files_for_this_build_gives_no_file() {
        let release: Release = parse_json(
            r#"{"tag_name":"v0.7.0","assets":[
                {"name":"TypeAlike-0.7.0-macos-arm64.dmg","browser_download_url":"https://example.com/a.dmg"},
                {"name":"Copy-TypeAlike-0.7.0-macos-arm64.dmg","browser_download_url":"https://example.com/b.dmg"}
            ]}"#,
        )
        .expect("valid json");

        let asset = release_asset(release.assets, "-macos-arm64.dmg");

        assert!(matches!(
            asset,
            Err(DownloadError::SeveralAssets("-macos-arm64.dmg"))
        ));
    }

    const TEST_SHA256: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    fn download_path(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("typealike-releases-{}-{test_name}", std::process::id()))
    }

    #[test]
    fn download_with_the_digest_of_the_release_is_kept() {
        let path = download_path("kept");

        let stored = store_download(&b"test"[..], &path, TEST_SHA256, Some(4));

        let contents = fs::read(&path).ok();
        fs::remove_file(&path).ok();
        assert_eq!((stored.is_ok(), contents), (true, Some(b"test".to_vec())));
    }

    #[test]
    fn download_with_another_digest_leaves_no_file() {
        let path = download_path("other-digest");

        let stored = store_download(&b"other"[..], &path, TEST_SHA256, None);

        assert_eq!(
            (
                matches!(stored, Err(DownloadError::DigestMismatch)),
                path.exists()
            ),
            (true, false)
        );
    }

    #[test]
    fn download_stops_when_it_grows_past_the_size_of_the_file() {
        let path = download_path("too-large");
        let mut body = io::Read::take(io::repeat(0), 1_000_000);

        let stored = store_download(&mut body, &path, TEST_SHA256, Some(4));

        assert_eq!(
            (
                matches!(stored, Err(DownloadError::TooLarge)),
                body.limit() > 0,
                path.exists()
            ),
            (true, true, false)
        );
    }

    #[test]
    fn hex_has_two_lowercase_digits_for_each_byte() {
        let digest = Context::new(&SHA256).finish();

        assert_eq!(
            to_hex(digest.as_ref()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
