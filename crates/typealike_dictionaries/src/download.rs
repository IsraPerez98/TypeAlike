//! The download of the files of a dictionary, each checked against its pinned size and SHA-256
//! digest.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Read, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ring::digest::{Context, SHA256};
use typealike_core::hub::{ByteSize, HubError};
use ureq::Agent;

use crate::catalog::{CatalogEntry, PinnedFile};
use crate::store;

const USER_AGENT: &str = concat!("TypeAlike/", env!("CARGO_PKG_VERSION"));
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_CHUNK_BYTES: usize = 64 * 1024;
const PROGRESS_INTERVAL_BYTES: u64 = 256 * 1024;

/// Why a download failed, with the details worth logging.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FetchError {
    #[error("the request for a dictionary failed")]
    Http(#[from] ureq::Error),
    #[error("the download connection broke")]
    Network(#[source] io::Error),
    #[error("failed to store the dictionary")]
    Storage(#[source] io::Error),
    #[error("the download has another size or sha-256 digest than the catalog lists")]
    Mismatch,
    #[error("the catalog has no dictionary of that language")]
    Unknown,
    #[error("download canceled")]
    Canceled,
}

impl FetchError {
    pub(crate) fn to_hub_error(&self) -> HubError {
        match self {
            Self::Http(ureq::Error::StatusCode(404)) | Self::Unknown => HubError::NotFound,
            Self::Http(ureq::Error::StatusCode(_)) => HubError::Server,
            Self::Http(_) | Self::Network(_) => HubError::Offline,
            Self::Mismatch => HubError::InvalidResponse,
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

/// Downloads each file of `entry` into the folder of its language in `dictionaries_dir`, and
/// reports the bytes stored so far to `on_progress`.
///
/// The files go to a partial folder first, and the folder gets its name only when each file has
/// its pinned size and digest. Stops with [`FetchError::Canceled`] soon after `cancel` turns true.
/// A download that fails or is canceled leaves no folder behind.
pub(crate) fn download(
    agent: &Agent,
    dictionaries_dir: &Path,
    entry: &CatalogEntry,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(ByteSize),
) -> Result<ByteSize, FetchError> {
    let destination = store::folder(dictionaries_dir, entry);
    let partial = store::partial_folder(dictionaries_dir, entry);
    let mut received = 0;
    let result = fs::create_dir_all(&partial)
        .map_err(FetchError::Storage)
        .and_then(|()| {
            for file in &entry.files {
                let mut response = agent.get(entry.url(file)).call()?;
                received = write_checked(
                    response.body_mut().as_reader(),
                    &partial.join(file.name),
                    file,
                    cancel,
                    received,
                    &mut on_progress,
                )?;
            }
            fs::rename(&partial, &destination).map_err(FetchError::Storage)
        });
    if result.is_err()
        && let Err(error) = fs::remove_dir_all(&partial)
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "failed to remove a partial dictionary download");
    }
    result.map(|()| ByteSize(received))
}

/// Writes `body` to `path` and makes sure that it is the pinned `file`. Returns the bytes of the
/// whole download so far, `received` before this file.
fn write_checked(
    mut body: impl Read,
    path: &Path,
    file: &PinnedFile,
    cancel: &AtomicBool,
    received: u64,
    on_progress: &mut impl FnMut(ByteSize),
) -> Result<u64, FetchError> {
    let mut output = File::create(path).map_err(FetchError::Storage)?;
    let mut digest = Context::new(&SHA256);
    let mut buffer = vec![0; DOWNLOAD_CHUNK_BYTES];
    let mut size = 0_u64;
    let mut reported = received;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(FetchError::Canceled);
        }
        let count = body.read(&mut buffer).map_err(FetchError::Network)?;
        if count == 0 {
            break;
        }
        let chunk = &buffer[..count];
        size = size.saturating_add(u64::try_from(count).expect("a read never exceeds u64"));
        if size > file.size {
            return Err(FetchError::Mismatch);
        }
        output.write_all(chunk).map_err(FetchError::Storage)?;
        digest.update(chunk);
        if received + size - reported >= PROGRESS_INTERVAL_BYTES {
            reported = received + size;
            on_progress(ByteSize(reported));
        }
    }
    output.sync_all().map_err(FetchError::Storage)?;
    if size != file.size || to_hex(digest.finish().as_ref()) != file.sha256 {
        return Err(FetchError::Mismatch);
    }
    Ok(received + size)
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, byte| {
        write!(hex, "{byte:02x}").expect("a write to a String never fails");
        hex
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// The SHA-256 digest of "hola".
    const HOLA_SHA256: &str = "b221d9dbb083a7f33428d7c2a3c3198ae925614d70210e28716ccaa7cd4ddb79";

    fn temporary_file(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("typealike_dictionaries_{}", std::process::id()));
        fs::create_dir_all(&folder).expect("the temp folder is writable");
        folder.join(name)
    }

    fn hola(size: u64, sha256: &'static str) -> PinnedFile {
        PinnedFile {
            name: "index.dic",
            size,
            sha256,
        }
    }

    #[test]
    fn body_with_the_pinned_size_and_digest_is_kept() {
        let path = temporary_file("kept.dic");

        let written = write_checked(
            &b"hola"[..],
            &path,
            &hola(4, HOLA_SHA256),
            &AtomicBool::new(false),
            10,
            &mut |_| {},
        );

        let text = fs::read_to_string(&path).expect("the file is written");
        fs::remove_file(&path).ok();
        assert_eq!((written.ok(), text.as_str()), (Some(14), "hola"));
    }

    #[test]
    fn body_with_another_digest_or_size_is_refused() {
        let cases = [hola(4, "00"), hola(3, HOLA_SHA256), hola(5, HOLA_SHA256)];

        let results = cases.map(|file| {
            let path = temporary_file("refused.dic");
            let result = write_checked(
                &b"hola"[..],
                &path,
                &file,
                &AtomicBool::new(false),
                0,
                &mut |_| {},
            );
            fs::remove_file(&path).ok();
            matches!(result, Err(FetchError::Mismatch))
        });

        assert_eq!(results, [true, true, true]);
    }

    #[test]
    fn canceled_download_stops() {
        let path = temporary_file("canceled.dic");

        let result = write_checked(
            &b"hola"[..],
            &path,
            &hola(4, HOLA_SHA256),
            &AtomicBool::new(true),
            0,
            &mut |_| {},
        );

        fs::remove_file(&path).ok();
        assert!(matches!(result, Err(FetchError::Canceled)));
    }

    #[test]
    fn errors_tell_the_hub_why_the_download_failed() {
        let errors = [
            FetchError::Http(ureq::Error::StatusCode(404)),
            FetchError::Http(ureq::Error::StatusCode(500)),
            FetchError::Network(io::Error::other("reset")),
            FetchError::Mismatch,
            FetchError::Storage(io::Error::other("full")),
            FetchError::Canceled,
        ];

        let hub_errors = errors.map(|error| error.to_hub_error());

        assert_eq!(
            hub_errors,
            [
                HubError::NotFound,
                HubError::Server,
                HubError::Offline,
                HubError::InvalidResponse,
                HubError::Storage,
                HubError::Canceled,
            ]
        );
    }
}
