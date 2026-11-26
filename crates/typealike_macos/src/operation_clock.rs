//! The times of the operations for the suggestion log. [`OperationClock`] times each run of an
//! operation on any thread. With the `suggestion-log` feature, `OperationTotals` writes their sums
//! once each minute. Without the feature, the clock only runs the operations.

#[cfg(feature = "suggestion-log")]
use std::sync::mpsc;
#[cfg(feature = "suggestion-log")]
use std::time::{Duration, Instant};

use typealike_core::diagnostics::Operation;
#[cfg(feature = "suggestion-log")]
use typealike_core::{
    SUGGESTION_LOG_TARGET, SuggestionLog,
    controller::Services,
    diagnostics::operation_times,
    history::{HistoryEntry, WritingHistory},
    settings::{Settings, SettingsStore},
    spelling::{OsDictionaries, SpellChecker},
};

/// Times the runs of operations, and sends each time to the totals of the runtime.
#[derive(Debug, Clone, Default)]
pub(crate) struct OperationClock {
    /// Where the times go, or `None` while the suggestion log is off.
    #[cfg(feature = "suggestion-log")]
    runs: Option<mpsc::Sender<(Operation, Duration)>>,
}

#[cfg(not(feature = "suggestion-log"))]
impl OperationClock {
    /// Runs `action`.
    #[expect(
        clippy::unused_self,
        reason = "the clock of the suggestion-log feature uses itself, and the call sites stay the same"
    )]
    pub(crate) fn time<Output>(&self, _operation: Operation, action: impl FnOnce() -> Output) -> Output {
        action()
    }
}

#[cfg(feature = "suggestion-log")]
impl OperationClock {
    /// Runs `action` as a run of `operation`, and notes its time while the suggestion log is on. A
    /// slow run also writes a `slow operation` record at once.
    pub(crate) fn time<Output>(&self, operation: Operation, action: impl FnOnce() -> Output) -> Output {
        let Some(runs) = &self.runs else {
            return action();
        };
        let started = Instant::now();
        let output = action();
        let duration = started.elapsed();
        if operation.is_slow(duration) {
            tracing::info!(
                target: SUGGESTION_LOG_TARGET,
                ?operation,
                duration_ms = milliseconds(duration),
                "slow operation"
            );
        }
        // The totals go only with the runtime, and then no other record is due.
        let _ = runs.send((operation, duration));
        output
    }
}

/// Writes the `operation times` records: the runs of each operation since the last record.
#[cfg(feature = "suggestion-log")]
#[derive(Debug)]
pub(crate) struct OperationTotals {
    runs: mpsc::Receiver<(Operation, Duration)>,
    window_started: Instant,
}

#[cfg(feature = "suggestion-log")]
impl OperationTotals {
    /// Starts the totals while the suggestion log is on, with the clock that sends them the times.
    /// While the log is off, gives no totals and a clock that measures nothing.
    pub(crate) fn start() -> (Option<Self>, OperationClock) {
        if !SuggestionLog::of_run().is_on() {
            return (None, OperationClock::default());
        }
        let (sender, runs) = mpsc::channel();
        let totals = Self {
            runs,
            window_started: Instant::now(),
        };
        (Some(totals), OperationClock { runs: Some(sender) })
    }

    /// Writes one record for each operation that ran since the last call, and none when nothing
    /// ran.
    pub(crate) fn write(&mut self) {
        let window = self.window_started.elapsed();
        self.window_started = Instant::now();
        for (operation, time) in operation_times(self.runs.try_iter()) {
            tracing::info!(
                target: SUGGESTION_LOG_TARGET,
                ?operation,
                count = time.count,
                total_ms = milliseconds(time.total),
                max_ms = milliseconds(time.longest),
                window_s = window.as_secs(),
                "operation times"
            );
        }
    }
}

#[cfg(feature = "suggestion-log")]
fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Gives `services` with the settings store, the history store and the dictionaries timed by
/// `clock`: their saves and the questions to the dictionaries.
#[cfg(feature = "suggestion-log")]
pub(crate) fn timed_services(services: Services, clock: &OperationClock) -> Services {
    Services {
        settings_store: Box::new(Timed::new(services.settings_store, clock)),
        history: Box::new(Timed::new(services.history, clock)),
        os_dictionaries: Box::new(Timed::new(services.os_dictionaries, clock)),
        ..services
    }
}

/// A service of core whose calls `clock` times.
#[cfg(feature = "suggestion-log")]
struct Timed<Service> {
    service: Service,
    clock: OperationClock,
}

#[cfg(feature = "suggestion-log")]
impl<Service> Timed<Service> {
    fn new(service: Service, clock: &OperationClock) -> Self {
        Self {
            service,
            clock: clock.clone(),
        }
    }
}

#[cfg(feature = "suggestion-log")]
impl SettingsStore for Timed<Box<dyn SettingsStore>> {
    fn save(&mut self, settings: &Settings) {
        self.clock
            .time(Operation::SettingsSave, || self.service.save(settings));
    }

    fn first_start_settings(&self) -> Settings {
        self.service.first_start_settings()
    }
}

#[cfg(feature = "suggestion-log")]
impl WritingHistory for Timed<Box<dyn WritingHistory>> {
    fn load(&mut self) -> Vec<HistoryEntry> {
        self.service.load()
    }

    fn save(&mut self, entries: &[HistoryEntry]) {
        self.clock
            .time(Operation::HistorySave, || self.service.save(entries));
    }

    fn delete(&mut self) {
        self.service.delete();
    }
}

#[cfg(feature = "suggestion-log")]
impl OsDictionaries for Timed<Box<dyn OsDictionaries>> {
    fn preferred_languages(&self) -> Vec<String> {
        self.service.preferred_languages()
    }

    fn available_languages(&self) -> Vec<String> {
        self.service.available_languages()
    }

    fn open(&self, languages: &[String]) -> Box<dyn SpellChecker> {
        Box::new(Timed::new(self.service.open(languages), &self.clock))
    }
}

#[cfg(feature = "suggestion-log")]
impl SpellChecker for Timed<Box<dyn SpellChecker>> {
    fn is_known_word(&self, word: &str) -> bool {
        self.clock
            .time(Operation::WordCheck, || self.service.is_known_word(word))
    }

    fn is_word_start(&self, letters: &str) -> bool {
        self.clock
            .time(Operation::WordCheck, || self.service.is_word_start(letters))
    }

    fn guesses(&self, word: &str) -> Vec<String> {
        self.clock
            .time(Operation::WordGuesses, || self.service.guesses(word))
    }
}
