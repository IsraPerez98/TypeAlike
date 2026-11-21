//! The numbers of the debug records about the resources and the times that TypeAlike uses. The
//! platform measures and writes the records, and these pure types and functions only count, add up
//! and convert.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::controller::Delivery;
use crate::inference::InferenceEvent;

/// The number of the items in each collection in memory that grows while TypeAlike runs. A count
/// that grows from one record to the next without end shows a leak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeptItems {
    /// The texts of the earlier writing.
    pub history_entries: usize,
    /// The words of the earlier writing, which a suggestion may write.
    pub history_words: usize,
    /// The answers and the guesses of the dictionaries, which the engine keeps.
    pub dictionary_answers: usize,
    /// The suggestions that the session keeps, to show them again.
    pub shown_suggestions: usize,
    /// The chat lines that the user scrolled past.
    pub conversation_lines: usize,
}

/// The time above which an input in the keyboard and mouse watch is slow: the app gets the input
/// only after it, so the user feels it as lag.
const SLOW_INPUT: Duration = Duration::from_millis(20);
/// The time above which another operation is slow.
const SLOW_OPERATION: Duration = Duration::from_millis(200);

/// An operation of the platform whose times the suggestion log adds up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Operation {
    /// A key down in the keyboard watch, with all the work for the key in it.
    KeyDown,
    /// A key up, a click or a scroll in the keyboard and mouse watch.
    OtherInput,
    /// A read of the focused field and its app.
    FieldRead,
    /// A read of the caret and the frame of the focused field.
    CaretRead,
    /// A draw of the ghost text and the badge.
    GhostDraw,
    /// A read of the copied text.
    ClipboardRead,
    /// A question to a dictionary about a word or the start of a word.
    WordCheck,
    /// A question to a dictionary for the words in place of a word.
    WordGuesses,
    /// A save of the earlier writing.
    HistorySave,
    /// A save of the settings.
    SettingsSave,
    /// A read of the screen text, from the Accessibility tree or by OCR.
    ScreenRead,
}

impl Operation {
    /// Whether a run of the operation that took `duration` is slow: more than 20 ms for an input,
    /// else more than 200 ms.
    #[must_use]
    pub fn is_slow(self, duration: Duration) -> bool {
        let limit = match self {
            Self::KeyDown | Self::OtherInput => SLOW_INPUT,
            Self::FieldRead
            | Self::CaretRead
            | Self::GhostDraw
            | Self::ClipboardRead
            | Self::WordCheck
            | Self::WordGuesses
            | Self::HistorySave
            | Self::SettingsSave
            | Self::ScreenRead => SLOW_OPERATION,
        };
        duration > limit
    }
}

/// The count, the total time and the longest time of the runs of one operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OperationTime {
    /// The number of the runs.
    pub count: u32,
    /// The sum of the times of the runs.
    pub total: Duration,
    /// The time of the longest run.
    pub longest: Duration,
}

/// Adds up the `runs`, each an operation and the time of one of its runs, for each operation.
#[must_use]
pub fn operation_times(
    runs: impl IntoIterator<Item = (Operation, Duration)>,
) -> BTreeMap<Operation, OperationTime> {
    let mut times = BTreeMap::<Operation, OperationTime>::new();
    for (operation, duration) in runs {
        let time = times.entry(operation).or_default();
        time.count = time.count.saturating_add(1);
        time.total = time.total.saturating_add(duration);
        time.longest = time.longest.max(duration);
    }
    times
}

/// The CPU time of a process between two records of its resources.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CpuWindow {
    /// The time since the start and the CPU time of the process at the last record.
    last: Option<(Duration, Duration)>,
}

impl CpuWindow {
    /// The percent of one CPU core that the process used since the last record, from its `uptime`
    /// and its `cpu_time` now. The first record has none, because the CPU time of the start would
    /// count in a window of almost no time.
    pub fn percent_at(&mut self, uptime: Duration, cpu_time: Duration) -> Option<f64> {
        let (last_uptime, last_cpu_time) = self.last.replace((uptime, cpu_time))?;
        cpu_percent(
            cpu_time.saturating_sub(last_cpu_time),
            uptime.saturating_sub(last_uptime),
        )
    }
}

/// The percent of one CPU core that the `cpu_time` of a window of `wall_time` used, or `None` for a
/// window without time.
fn cpu_percent(cpu_time: Duration, wall_time: Duration) -> Option<f64> {
    (!wall_time.is_zero()).then(|| cpu_time.as_secs_f64() / wall_time.as_secs_f64() * 100.0)
}

/// The `bytes` in megabytes of 1024 × 1024 bytes, to the kilobyte. The count stops at 4 TB.
#[must_use]
pub fn megabytes(bytes: u64) -> f64 {
    let kilobytes = u32::try_from(bytes / 1024).unwrap_or(u32::MAX);
    f64::from(kilobytes) / 1024.0
}

/// Whether `delivery` tells that a model loaded, so that the memory of the model shows now.
#[must_use]
pub fn is_model_load(delivery: &Delivery) -> bool {
    let (Delivery::Suggestion(event) | Delivery::Playground(event) | Delivery::Recommendation(event)) =
        delivery
    else {
        return false;
    };
    matches!(event, InferenceEvent::Loaded { result: Ok(()), .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::{ScreenRead, ScreenReadId};
    use crate::inference::InferenceError;
    use crate::test_support::model_file;

    const fn milliseconds(count: u64) -> Duration {
        Duration::from_millis(count)
    }

    #[test]
    fn operation_times_add_the_count_the_total_and_the_longest_run_of_each_operation() {
        let runs = [
            (Operation::CaretRead, milliseconds(3)),
            (Operation::KeyDown, milliseconds(2)),
            (Operation::CaretRead, milliseconds(9)),
            (Operation::CaretRead, milliseconds(4)),
        ];

        let times = operation_times(runs);

        assert_eq!(
            times.into_iter().collect::<Vec<_>>(),
            [
                (
                    Operation::KeyDown,
                    OperationTime {
                        count: 1,
                        total: milliseconds(2),
                        longest: milliseconds(2),
                    }
                ),
                (
                    Operation::CaretRead,
                    OperationTime {
                        count: 3,
                        total: milliseconds(16),
                        longest: milliseconds(9),
                    }
                ),
            ]
        );
    }

    #[test]
    fn only_an_input_above_20_ms_or_another_operation_above_200_ms_is_slow() {
        let runs = [
            (Operation::KeyDown, milliseconds(20)),
            (Operation::KeyDown, milliseconds(21)),
            (Operation::OtherInput, milliseconds(21)),
            (Operation::CaretRead, milliseconds(200)),
            (Operation::HistorySave, milliseconds(201)),
        ];

        let slow = runs.map(|(operation, duration)| operation.is_slow(duration));

        assert_eq!(slow, [false, true, true, false, true]);
    }

    #[test]
    fn cpu_percent_is_the_cpu_time_of_one_core_in_the_window() {
        let windows = [
            (Duration::from_secs(15), Duration::from_mins(1)),
            (Duration::from_secs(90), Duration::from_mins(1)),
            (Duration::ZERO, Duration::from_mins(1)),
            (Duration::from_secs(1), Duration::ZERO),
        ];

        let percents = windows.map(|(cpu_time, wall_time)| cpu_percent(cpu_time, wall_time));

        assert_eq!(percents, [Some(25.0), Some(150.0), Some(0.0), None]);
    }

    #[test]
    fn cpu_window_gives_no_percent_at_the_first_record_and_then_the_percent_since_the_last() {
        let mut window = CpuWindow::default();

        let percents = [
            window.percent_at(Duration::from_secs(1), Duration::from_secs(2)),
            window.percent_at(Duration::from_secs(61), Duration::from_secs(17)),
        ];

        assert_eq!(percents, [None, Some(25.0)]);
    }

    #[test]
    fn megabytes_count_whole_kilobytes_up_to_four_terabytes() {
        let sizes = [0, 1_572_864, 1_573_375, u64::MAX];

        let megabytes = sizes.map(megabytes);

        assert_eq!(megabytes, [0.0, 1.5, 1.5, f64::from(u32::MAX) / 1024.0]);
    }

    fn loaded(result: Result<(), InferenceError>) -> InferenceEvent {
        InferenceEvent::Loaded {
            file: model_file("model.gguf"),
            result,
        }
    }

    #[test]
    fn only_a_model_that_loaded_is_a_model_load() {
        let deliveries = [
            Delivery::Suggestion(loaded(Ok(()))),
            Delivery::Playground(loaded(Ok(()))),
            Delivery::Recommendation(loaded(Ok(()))),
            Delivery::Suggestion(loaded(Err(InferenceError::ModelMissing))),
            Delivery::Suggestion(InferenceEvent::Partial("Dear".to_owned())),
            Delivery::Screen(ScreenRead {
                id: ScreenReadId::default(),
                text: None,
            }),
        ];

        let loads = deliveries.each_ref().map(is_model_load);

        assert_eq!(loads, [true, true, true, false, false, false]);
    }
}
