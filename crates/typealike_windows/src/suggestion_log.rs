//! The records of the suggestion log that Windows writes: the screen text, the ghost text and the
//! text that Tab inserts. They hold what the user wrote, so only a build with the `suggestion-log`
//! feature has them.

use std::time::Duration;

use typealike_core::SUGGESTION_LOG_TARGET;
use typealike_core::controller::{Ghost, GhostView};
use typealike_core::desktop::{ScreenRead, ScreenSource};

/// Writes the lines of the screen read `answer` from `source`, which waited `waited` in the queue
/// of the reader and then took `duration`.
pub(crate) fn screen_read(answer: &ScreenRead, source: ScreenSource, waited: Duration, duration: Duration) {
    tracing::info!(
        target: SUGGESTION_LOG_TARGET,
        read = ?answer.id,
        ?source,
        waited_ms = waited.as_secs_f64() * 1000.0,
        duration_ms = duration.as_secs_f64() * 1000.0,
        lines = answer.text.as_ref().map(|text| {
            tracing::field::debug(text.lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>())
        }),
        "read the screen"
    );
}

/// Writes the ghost text of `view`.
pub(crate) fn ghost(view: &GhostView) {
    let wrapped = view
        .wrapped
        .as_ref()
        .map(|wrapped| tracing::field::debug(&wrapped.lines));
    match &view.ghost {
        Ghost::Text(text) => {
            tracing::info!(target: SUGGESTION_LOG_TARGET, text = text.as_str(), wrapped, "showed the ghost text");
        }
        Ghost::Fix(fix) => tracing::info!(
            target: SUGGESTION_LOG_TARGET,
            typo = fix.typo.as_str(),
            word = fix.word.as_str(),
            rest = fix.rest.as_str(),
            wrapped,
            "showed a fix"
        ),
    }
}

/// Writes the `text` that Tab types in place of the `deleted` characters before the caret.
pub(crate) fn tab(deleted: usize, text: &str) {
    tracing::info!(target: SUGGESTION_LOG_TARGET, deleted, text, "Tab inserted text");
}
