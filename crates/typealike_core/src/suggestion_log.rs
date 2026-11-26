//! Whether the suggestion log takes the records of this run.

use std::sync::OnceLock;

/// The state of the suggestion log for this run, which the bin sets once when it starts the logs.
static STATE_OF_RUN: OnceLock<SuggestionLog> = OnceLock::new();

/// Whether the suggestion log takes the records of this run. Only a build with the
/// `suggestion-log` feature can turn it on, and only when the file of the log exists at start.
///
/// The code that collects a record asks [`SuggestionLog::of_run`] first, so that it does no work
/// while the log is off. The state is one value for the whole process, because that code runs in
/// many crates and threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionLog {
    /// No record goes to the suggestion log.
    Off,
    /// The records go to the suggestion log.
    On,
}

impl SuggestionLog {
    /// The state of this run: the state that [`SuggestionLog::set_for_run`] set, else `Off`.
    #[must_use]
    pub fn of_run() -> Self {
        STATE_OF_RUN.get().copied().unwrap_or(Self::Off)
    }

    /// Sets this state as the state of the run. Only the first call sets it: the bin calls it once,
    /// before it starts the other threads.
    pub fn set_for_run(self) {
        // A later call cannot change the state that the code of the run already read.
        let _ = STATE_OF_RUN.set(self);
    }

    /// Whether the records go to the suggestion log.
    #[must_use]
    pub fn is_on(self) -> bool {
        self == Self::On
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_keeps_the_first_state_that_the_bin_sets() {
        SuggestionLog::On.set_for_run();
        SuggestionLog::Off.set_for_run();

        let state = SuggestionLog::of_run();

        assert_eq!(state, SuggestionLog::On);
    }
}
