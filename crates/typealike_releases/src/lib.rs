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
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ring::digest::{Context, SHA256};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use typealike_core::release::{
    DownloadSink, DownloadedRelease, ReleaseDownloadFailed, ReleaseFeed, ReleaseInstallFailed,
    ReleaseReadFailed, ReleaseSink, ReleaseTag, SelfUpdate,
};
use ureq::Agent;
use ureq::http::HeaderMap;

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
/// The header with the number of requests that this IP address can still make before the limit of
/// the GitHub API resets.
const RATE_LIMIT_REMAINING: &str = "x-ratelimit-remaining";
/// The header with the time of the reset of the limit, in seconds since the Unix epoch.
const RATE_LIMIT_RESET: &str = "x-ratelimit-reset";

/// Why a read of the newest release failed, with the details worth logging.
#[derive(Debug, thiserror::Error)]
enum FetchError {
    #[error("github request failed")]
    Http(#[from] ureq::Error),
    #[error("github refused the request with status {}", .0.status)]
    Refused(RefusedRequest),
    #[error("github sent malformed json")]
    Json(#[from] serde_json::Error),
}

/// An answer of the GitHub API with a status other than a success, with the details worth logging.
/// It has no part of the body, because the body of a rate limit names the IP address of the
/// computer, and the log goes into bug reports.
#[derive(Debug, PartialEq, Eq)]
struct RefusedRequest {
    status: u16,
    /// The value of `x-ratelimit-remaining`.
    rate_limit_remaining: Option<String>,
    /// The value of `x-ratelimit-reset`.
    rate_limit_reset: Option<String>,
}

impl RefusedRequest {
    fn new(status: u16, headers: &HeaderMap) -> Self {
        let header = |name| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        Self {
            status,
            rate_limit_remaining: header(RATE_LIMIT_REMAINING),
            rate_limit_reset: header(RATE_LIMIT_RESET),
        }
    }

    /// The failure that core gets for this answer at `now`. A status 403 or 429 with no request
    /// left is a rate limit, which resets at the time of `x-ratelimit-reset`. Any other answer, and a
    /// reset that the clock cannot hold, is another failure.
    fn read_failure(&self, now: SystemTime) -> ReleaseReadFailed {
        let is_used_rate_limit =
            matches!(self.status, 403 | 429) && self.rate_limit_remaining.as_deref() == Some("0");
        let reset = self
            .rate_limit_reset
            .as_deref()
            .and_then(|seconds| seconds.parse().ok())
            .and_then(|seconds| UNIX_EPOCH.checked_add(Duration::from_secs(seconds)));
        match reset {
            Some(reset) if is_used_rate_limit => ReleaseReadFailed::RateLimited {
                resets_in: reset.duration_since(now).unwrap_or(Duration::ZERO),
            },
            _ => ReleaseReadFailed::Other,
        }
    }
}

impl FetchError {
    /// The failure that core gets for this error at `now`: a used rate limit, or another failure.
    fn read_failure(&self, now: SystemTime) -> ReleaseReadFailed {
        match self {
            Self::Refused(refused) => refused.read_failure(now),
            Self::Http(_) | Self::Json(_) => ReleaseReadFailed::Other,
        }
    }
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
                    error.read_failure(SystemTime::now())
                });
            if let Ok(tag) = &release {
                tracing::info!(
                    tag = tag.0,
                    is_newer = tag.is_newer_than_build(),
                    "read the newest release"
                );
            }
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

/// Reads the release of the GitHub API at `url`. A status other than a success gives
/// [`FetchError::Refused`] with the rate limit.
fn fetch_release(agent: &Agent, url: &str) -> Result<Release, FetchError> {
    let mut response = agent
        .get(url)
        .header("Accept", ACCEPT)
        .config()
        .http_status_as_error(false)
        .build()
        .call()?;
    let status = response.status();
    if !status.is_success() {
        let refused = RefusedRequest::new(status.as_u16(), response.headers());
        return Err(FetchError::Refused(refused));
    }
    let body = response.body_mut().read_to_string()?;
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

    use ureq::http::HeaderValue;

    fn rate_limit_headers(remaining: &str, reset: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            RATE_LIMIT_REMAINING,
            HeaderValue::from_str(remaining).expect("ascii value"),
        );
        headers.insert(
            RATE_LIMIT_RESET,
            HeaderValue::from_str(reset).expect("ascii value"),
        );
        headers
    }

    #[test]
    fn refused_request_keeps_only_the_status_and_the_rate_limit() {
        let headers = rate_limit_headers("0", "1700000000");

        let refused = RefusedRequest::new(403, &headers);

        assert_eq!(
            refused,
            RefusedRequest {
                status: 403,
                rate_limit_remaining: Some("0".to_owned()),
                rate_limit_reset: Some("1700000000".to_owned()),
            }
        );
    }

    /// The seconds since the Unix epoch at the answers of the tests.
    const NOW_S: u64 = 1_700_000_000;

    /// The failure that core gets for an answer with `status` and `headers` at [`NOW_S`].
    fn read_failure_at_now(status: u16, headers: &HeaderMap) -> ReleaseReadFailed {
        RefusedRequest::new(status, headers).read_failure(UNIX_EPOCH + Duration::from_secs(NOW_S))
    }

    #[test]
    fn rate_limit_without_requests_left_waits_until_its_reset() {
        let reset = (NOW_S + 600).to_string();

        let failures = [403, 429].map(|status| read_failure_at_now(status, &rate_limit_headers("0", &reset)));

        assert_eq!(
            failures,
            [ReleaseReadFailed::RateLimited {
                resets_in: Duration::from_secs(600)
            }; 2]
        );
    }

    #[test]
    fn rate_limit_with_a_reset_in_the_past_resets_at_once() {
        let reset = (NOW_S - 60).to_string();

        let failure = read_failure_at_now(403, &rate_limit_headers("0", &reset));

        assert_eq!(
            failure,
            ReleaseReadFailed::RateLimited {
                resets_in: Duration::ZERO
            }
        );
    }

    #[test]
    fn refusal_that_is_not_a_used_rate_limit_is_another_failure() {
        let reset = (NOW_S + 600).to_string();
        let cases = [
            (403, rate_limit_headers("12", &reset)),
            (404, rate_limit_headers("0", &reset)),
            (403, rate_limit_headers("0", "soon")),
            (403, rate_limit_headers("0", &u64::MAX.to_string())),
            (403, HeaderMap::new()),
        ];

        let failures = cases.map(|(status, headers)| read_failure_at_now(status, &headers));

        assert_eq!(failures, [ReleaseReadFailed::Other; 5]);
    }

    #[test]
    fn malformed_answer_is_another_failure() {
        let error = FetchError::from(parse_json::<Release>("{").err().expect("malformed json"));

        let failure = error.read_failure(UNIX_EPOCH);

        assert_eq!(failure, ReleaseReadFailed::Other);
    }

    #[test]
    fn refused_request_without_rate_limit_headers_has_no_rate_limit() {
        let headers = HeaderMap::new();

        let refused = RefusedRequest::new(404, &headers);

        assert_eq!(
            refused,
            RefusedRequest {
                status: 404,
                rate_limit_remaining: None,
                rate_limit_reset: None,
            }
        );
    }

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
