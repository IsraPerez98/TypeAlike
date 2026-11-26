//! Sends the logs to the terminal and to a file in the temp folder. A build with the
//! `suggestion-log` feature also sends the records of the suggestion log to their own file.

use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::{env, io, panic};

use tracing::Subscriber;
use tracing_subscriber::Layer as _;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::{self, MakeWriter};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt as _;
use typealike_core::SUGGESTION_LOG_TARGET;

/// The environment variable that sets the log levels, for example `debug` or
/// `info,typealike_llm=debug,llama-cpp-2=warn`.
const LEVELS_VARIABLE: &str = "TYPEALIKE_LOG";
/// The levels without `TYPEALIKE_LOG`. llama.cpp writes many info lines for each model load, so
/// only its warnings and errors go to the log.
const DEFAULT_LEVELS: &str = "info,llama-cpp-2=warn";
const LOG_FILE: &str = "TypeAlike.log";
const PREVIOUS_LOG_FILE: &str = "TypeAlike.previous.log";
/// The size above which a start moves the log to the previous log, so the log does not grow
/// without end.
const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;

/// Sends the logs to the terminal and appends them to `TypeAlike.log` in the temp folder, with the
/// levels of `TYPEALIKE_LOG`, else [`DEFAULT_LEVELS`]. A panic also goes to the logs. With the
/// `suggestion-log` feature, it also appends the records of the suggestion log to
/// `suggestions.jsonl` in the data folder, when that file exists.
pub(crate) fn start() {
    let levels = env::var(LEVELS_VARIABLE).unwrap_or_else(|_| DEFAULT_LEVELS.to_owned());
    let (main_targets, filter_error) = main_targets(&levels);
    let path = env::temp_dir().join(LOG_FILE);
    let file = open_log_file(&path);
    let file_error = file.as_ref().err().map(ToString::to_string);
    let subscriber = main_subscriber(main_targets, io::stdout, file.ok());
    #[cfg(feature = "suggestion-log")]
    let subscriber = subscriber.with(suggestion_log::layer());
    subscriber.init();
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        tracing::error!(%info, "TypeAlike panicked");
        default_hook(info);
    }));
    log_start(&path);
    if let Some(error) = filter_error {
        tracing::warn!(
            %error,
            levels,
            default = DEFAULT_LEVELS,
            "the log levels are not valid, so the log uses the default levels"
        );
    }
    if let Some(error) = file_error {
        tracing::warn!(%error, "failed to open the log file");
    }
}

/// The subscriber that sends the logs at `targets` to `terminal`, and to `file` without colors when
/// the log file opened.
fn main_subscriber(
    targets: Targets,
    terminal: impl for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
    file: Option<impl for<'writer> MakeWriter<'writer> + Send + Sync + 'static>,
) -> impl Subscriber + for<'span> LookupSpan<'span> + Send + Sync {
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(terminal).with_filter(targets.clone()))
        .with(file.map(|file| {
            fmt::layer()
                .with_ansi(false)
                .with_writer(file)
                .with_filter(targets)
        }))
}

/// Logs the start of the run: the build, the process, the path of the `log` file and whether the
/// suggestion log is on.
fn log_start(log: &Path) {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        process = std::process::id(),
        vulkan = cfg!(feature = "vulkan"),
        cuda = cfg!(feature = "cuda"),
        log = %log.display(),
        suggestion_log = tracing::enabled!(target: SUGGESTION_LOG_TARGET, tracing::Level::INFO),
        "TypeAlike started"
    );
}

/// The levels of the terminal and of `TypeAlike.log` from `levels`, else [`DEFAULT_LEVELS`], and
/// why `levels` is not valid. They never take the records of the suggestion log, which hold what
/// the user wrote, also when `levels` names their target.
fn main_targets(levels: &str) -> (Targets, Option<String>) {
    let (targets, error) = match levels.parse::<Targets>() {
        Ok(targets) => (targets, None),
        Err(error) => (
            DEFAULT_LEVELS.parse().expect("the default levels are valid"),
            Some(error.to_string()),
        ),
    };
    (
        targets.with_target(SUGGESTION_LOG_TARGET, LevelFilter::OFF),
        error,
    )
}

/// Opens the log at `path` to append to it, so a start after a crash keeps the log of the crash.
/// A log above `MAX_LOG_BYTES` first replaces the previous log. When that move fails, the run
/// appends to the large log.
fn open_log_file(path: &Path) -> io::Result<File> {
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() > MAX_LOG_BYTES) {
        // Another run can move the log first, and a log in the large file is better than none.
        let _ = fs::rename(path, path.with_file_name(PREVIOUS_LOG_FILE));
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// The suggestion log: `suggestions.jsonl` in the data folder, one JSON object for each record.
#[cfg(feature = "suggestion-log")]
mod suggestion_log {
    use std::fs::{File, OpenOptions};
    use std::path::Path;

    use tracing::Subscriber;
    use tracing_subscriber::Layer;
    use tracing_subscriber::filter::{LevelFilter, Targets};
    use tracing_subscriber::fmt;
    use tracing_subscriber::registry::LookupSpan;
    use typealike_core::SUGGESTION_LOG_TARGET;

    const FILE: &str = "suggestions.jsonl";

    /// The layer that appends the records of the suggestion log to its file, or `None` when the
    /// file does not exist in the data folder: the user makes the file to start the log, and
    /// deletes it to stop the log. TypeAlike looks for the file only when it starts.
    pub(super) fn layer<S>() -> Option<impl Layer<S>>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        let data_dir = crate::platform::data_dir().ok()?;
        open(&data_dir.join(FILE)).map(json_layer)
    }

    /// Writes each record of the suggestion log to `file` as one line of JSON, with its fields at
    /// the top level and the fields of its span, such as the number of its completion, in `span`.
    fn json_layer<S>(file: File) -> impl Layer<S>
    where
        S: Subscriber + for<'span> LookupSpan<'span>,
    {
        fmt::layer()
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(false)
            .with_writer(file)
            .with_filter(Targets::new().with_target(SUGGESTION_LOG_TARGET, LevelFilter::TRACE))
    }

    /// Opens the suggestion log at `path` to append to it, or `None` when the file does not exist.
    fn open(path: &Path) -> Option<File> {
        OpenOptions::new().append(true).open(path).ok()
    }

    #[cfg(test)]
    mod tests {
        use std::fs;
        use std::io::Write as _;

        use tracing_subscriber::layer::SubscriberExt as _;

        use super::*;
        use crate::logging::tests::temporary_folder;
        use crate::logging::{LOG_FILE, main_targets};

        /// Runs `log` with the main log and the suggestion log in a new folder, and returns the
        /// lines of each.
        fn logged_lines(name: &str, log: impl FnOnce()) -> (String, String) {
            let folder = temporary_folder(name);
            let [main_path, suggestion_path] = [LOG_FILE, FILE].map(|file| folder.join(file));
            let [main_file, suggestion_file] =
                [&main_path, &suggestion_path].map(|path| File::create(path).expect("temp dir is writable"));
            let subscriber = tracing_subscriber::registry()
                .with(
                    fmt::layer()
                        .with_writer(main_file)
                        .with_filter(main_targets("trace").0),
                )
                .with(json_layer(suggestion_file));

            tracing::subscriber::with_default(subscriber, log);
            let [main_log, suggestion_log] =
                [&main_path, &suggestion_path].map(|path| fs::read_to_string(path).unwrap_or_default());

            fs::remove_dir_all(&folder).ok();
            (main_log, suggestion_log)
        }

        #[test]
        fn record_goes_to_the_suggestion_log_as_one_json_line_and_not_to_the_main_log() {
            let (main_log, suggestion_log) = logged_lines("suggestion-json", || {
                tracing::info!(target: SUGGESTION_LOG_TARGET, text = "Dear\nSir", "showed the ghost text");
            });

            assert_eq!(
                (
                    main_log.as_str(),
                    suggestion_log.lines().count(),
                    suggestion_log.contains(r#""text":"Dear\nSir""#)
                ),
                ("", 1, true)
            );
        }

        #[test]
        fn record_names_the_completion_of_its_parent_span() {
            let (main_log, suggestion_log) = logged_lines("suggestion-span", || {
                let span = tracing::info_span!(target: SUGGESTION_LOG_TARGET, "completion", completion = 7);
                tracing::info!(target: SUGGESTION_LOG_TARGET, parent: &span, "showed the ghost text");
            });

            assert_eq!(
                (
                    main_log.as_str(),
                    suggestion_log.contains(r#""span":{"completion":7"#)
                ),
                ("", true)
            );
        }

        #[test]
        fn suggestion_log_is_off_without_its_file() {
            let folder = temporary_folder("no-suggestion-log");

            let log = open(&folder.join(FILE));

            fs::remove_dir_all(&folder).ok();
            assert!(log.is_none());
        }

        #[test]
        fn suggestion_log_appends_to_its_file() {
            let folder = temporary_folder("suggestion-log");
            let path = folder.join(FILE);
            fs::write(&path, "earlier\n").expect("temp dir is writable");

            open(&path)
                .expect("the file exists")
                .write_all(b"this run\n")
                .expect("the log is writable");
            let lines = fs::read_to_string(&path).ok();

            fs::remove_dir_all(&folder).ok();
            assert_eq!(lines.as_deref(), Some("earlier\nthis run\n"));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::path::PathBuf;

    use super::*;

    pub(super) fn temporary_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("typealike-logging-{}-{name}", std::process::id()));
        fs::create_dir_all(&folder).expect("temp dir is writable");
        folder
    }

    #[test]
    fn main_log_leaves_out_the_suggestion_log_at_each_level() {
        let levels = [
            "trace",
            "typealike=trace",
            "typealike_suggestions=trace",
            "trace,typealike_suggestions=trace",
        ];

        let is_enabled = levels.map(|levels| {
            main_targets(levels)
                .0
                .would_enable(SUGGESTION_LOG_TARGET, &tracing::Level::ERROR)
        });

        assert_eq!(is_enabled, [false; 4]);
    }

    #[test]
    fn default_levels_keep_only_the_warnings_of_llama_cpp() {
        let lines = [
            ("typealike", tracing::Level::INFO),
            ("llama-cpp-2", tracing::Level::INFO),
            ("llama-cpp-2", tracing::Level::WARN),
        ];

        let targets = main_targets(DEFAULT_LEVELS).0;
        let is_enabled = lines.map(|(target, level)| targets.would_enable(target, &level));

        assert_eq!(is_enabled, [true, false, true]);
    }

    #[test]
    fn levels_that_are_not_valid_give_the_default_levels() {
        let (targets, error) = main_targets("typealike=loud");

        let is_enabled = [
            targets.would_enable("typealike", &tracing::Level::INFO),
            targets.would_enable("llama-cpp-2", &tracing::Level::INFO),
        ];

        assert_eq!((is_enabled, error.is_some()), ([true, false], true));
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

    #[test]
    fn large_log_opens_when_the_move_to_the_previous_log_fails() {
        let folder = temporary_folder("blocked");
        let path = folder.join(LOG_FILE);
        File::create(&path)
            .and_then(|file| file.set_len(MAX_LOG_BYTES + 1))
            .expect("temp dir is writable");
        fs::create_dir(folder.join(PREVIOUS_LOG_FILE)).expect("temp dir is writable");

        let opened = open_log_file(&path).map(drop).map_err(|error| error.kind());

        fs::remove_dir_all(&folder).ok();
        assert_eq!(opened, Ok(()));
    }
}
