//! Sends the logs to the terminal and to a file in the temp folder.

use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::{env, io, panic};

use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// The environment variable that sets the log levels, for example `debug` or
/// `info,ghosttype_llm=debug,llama-cpp-2=warn`.
const LEVELS_VARIABLE: &str = "GHOSTTYPE_LOG";
const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;
const LOG_FILE: &str = "GhostType.log";
const PREVIOUS_LOG_FILE: &str = "GhostType.previous.log";
/// The size above which a start moves the log to the previous log, so the log does not grow
/// without end.
const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;

/// Sends the logs to the terminal and appends them to `GhostType.log` in the temp folder, with the
/// levels of `GHOSTTYPE_LOG`, else `info`. A panic also goes to the logs.
pub(crate) fn start() {
    let levels = env::var(LEVELS_VARIABLE).unwrap_or_else(|_| DEFAULT_LEVEL.to_string());
    let filter = levels.parse::<Targets>();
    let filter_error = filter.as_ref().err().map(ToString::to_string);
    let path = env::temp_dir().join(LOG_FILE);
    let file = open_log_file(&path);
    let file_error = file.as_ref().err().map(ToString::to_string);
    tracing_subscriber::registry()
        .with(filter.unwrap_or_else(|_| Targets::new().with_default(DEFAULT_LEVEL)))
        .with(fmt::layer())
        .with(
            file.ok()
                .map(|file| fmt::layer().with_ansi(false).with_writer(file)),
        )
        .init();
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        tracing::error!(%info, "GhostType panicked");
        default_hook(info);
    }));
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        process = std::process::id(),
        vulkan = cfg!(feature = "vulkan"),
        cuda = cfg!(feature = "cuda"),
        log = %path.display(),
        "GhostType started"
    );
    if let Some(error) = filter_error {
        tracing::warn!(%error, levels, "the log levels are not valid, so the log uses info");
    }
    if let Some(error) = file_error {
        tracing::warn!(%error, "failed to open the log file");
    }
}

/// Opens the log at `path` to append to it, so a start after a crash keeps the log of the crash.
/// A log above `MAX_LOG_BYTES` first replaces the previous log.
fn open_log_file(path: &Path) -> io::Result<File> {
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() > MAX_LOG_BYTES) {
        fs::rename(path, path.with_file_name(PREVIOUS_LOG_FILE))?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::path::PathBuf;

    use super::*;

    fn temporary_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("ghosttype-logging-{}-{name}", std::process::id()));
        fs::create_dir_all(&folder).expect("temp dir is writable");
        folder
    }

    #[test]
    fn small_log_keeps_the_lines_of_the_earlier_run() {
        let folder = temporary_folder("small");
        let path = folder.join(LOG_FILE);
        fs::write(&path, "earlier run\n").expect("temp dir is writable");

        open_log_file(&path)
            .and_then(|mut file| file.write_all(b"this run\n"))
            .expect("the log opens");
        let lines = fs::read_to_string(&path).ok();

        fs::remove_dir_all(&folder).ok();
        assert_eq!(lines.as_deref(), Some("earlier run\nthis run\n"));
    }

    #[test]
    fn large_log_moves_to_the_previous_log() {
        let folder = temporary_folder("large");
        let path = folder.join(LOG_FILE);
        File::create(&path)
            .and_then(|file| file.set_len(MAX_LOG_BYTES + 1))
            .expect("temp dir is writable");

        open_log_file(&path).expect("the log opens");
        let sizes = [path.clone(), folder.join(PREVIOUS_LOG_FILE)]
            .map(|file| fs::metadata(file).map(|metadata| metadata.len()).ok());

        fs::remove_dir_all(&folder).ok();
        assert_eq!(sizes, [Some(0), Some(MAX_LOG_BYTES + 1)]);
    }
}
