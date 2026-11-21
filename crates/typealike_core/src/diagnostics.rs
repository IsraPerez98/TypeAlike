//! The numbers of the debug records about the resources that TypeAlike uses. The platform measures
//! and writes the records, and these pure types and functions only count and convert.

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

/// The percent of one CPU core that the `cpu_time` of a window of `wall_time` used, or `None` for a
/// window without time.
#[must_use]
pub fn cpu_percent(cpu_time: Duration, wall_time: Duration) -> Option<f64> {
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
