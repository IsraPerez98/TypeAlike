//! The records of the suggestion log that macOS writes: the fields that it reads, the screen text,
//! the ghost text and the text that Tab inserts. They hold what the user wrote, so only a build
//! with the `suggestion-log` feature has them.

use std::time::{Duration, Instant};

use objc2_application_services::AXUIElement;
use tracing::Span;
use typealike_core::SUGGESTION_LOG_TARGET;
use typealike_core::controller::{Delivery, Ghost, GhostView};
use typealike_core::desktop::{ScreenRead, ScreenReadId, ScreenSource, Surroundings};
use typealike_core::inference::InferenceEvent;

use crate::accessibility::{ROLE_ATTRIBUTE, SUBROLE_ATTRIBUTE, copy_string, frame};
use crate::mirror::Mirror;
use crate::overlay::Overlay;

/// Writes what the app knows about the focused field: the `surroundings` of `element` and its
/// Accessibility role.
pub(crate) fn field(
    element: Option<&AXUIElement>,
    surroundings: &Surroundings,
    shows_text_only_on_screen: bool,
) {
    if !tracing::enabled!(target: SUGGESTION_LOG_TARGET, tracing::Level::INFO) {
        return;
    }
    let role = element.and_then(|element| copy_string(element, ROLE_ATTRIBUTE));
    let subrole = element.and_then(|element| copy_string(element, SUBROLE_ATTRIBUTE));
    let labels = &surroundings.field_labels;
    tracing::info!(
        target: SUGGESTION_LOG_TARGET,
        app_id = surroundings.app_id.as_deref(),
        app_name = surroundings.app_name.as_deref(),
        window_title = surroundings.window_title.as_deref(),
        domain = surroundings.domain.as_deref(),
        is_outside_web_pages = surroundings.is_outside_web_pages,
        caption = labels.caption.as_deref(),
        name = labels.name.as_deref(),
        placeholder = labels.placeholder.as_deref(),
        help = labels.help.as_deref(),
        role = role.as_deref(),
        subrole = subrole.as_deref(),
        frame = ?element.and_then(frame),
        chars_before_caret = surroundings.field_text.as_ref().map(|text| text.before_caret.chars().count()),
        chars_after_caret = surroundings.field_text.as_ref().map(|text| text.after_caret.chars().count()),
        is_search_field = surroundings.is_search_field,
        is_caret_line_hidden = surroundings.is_caret_line_hidden,
        has_unreadable_text = surroundings.has_unreadable_text,
        shows_text_only_on_screen,
        "read the field"
    );
}

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

/// Where a ghost text shows after a draw.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Drawn {
    /// The overlay at the caret.
    Overlay,
    /// The mirror next to the field, or the card below the caret.
    Mirror,
    /// Nothing: the field gave no caret and no frame, or the overlay could not show the text.
    Nothing,
}

impl Drawn {
    /// Where the ghost text of `overlay` and `mirror` shows now.
    pub(crate) fn of(overlay: &Overlay, mirror: &Mirror) -> Self {
        if overlay.is_shown() {
            Self::Overlay
        } else if mirror.is_shown() {
            Self::Mirror
        } else {
            Self::Nothing
        }
    }
}

/// Whether the suggestion `event` gives text that a ghost text can show: a part of a completion, or
/// a completion with text or a fix.
fn gives_text(event: &InferenceEvent) -> bool {
    match event {
        InferenceEvent::Partial(_) => true,
        InferenceEvent::Completed(Ok(completion)) => {
            !completion.text.is_empty() || completion.correction.is_some()
        }
        InferenceEvent::Completed(Err(_))
        | InferenceEvent::Loaded { .. }
        | InferenceEvent::GpuMemory { .. } => false,
    }
}

/// Writes the records of the shell: the ghost text and the text that Tab inserts, each with the
/// completion and the screen read that came last before it.
#[derive(Debug)]
pub(crate) struct ShellRecords {
    /// The span of the completion of the newest suggestion event.
    completion: Span,
    /// The newest screen read that the shell gave to core.
    screen_read: Option<ScreenReadId>,
    /// The time of the last key down that no ghost text showed after yet.
    key_down: Option<Instant>,
}

impl Default for ShellRecords {
    fn default() -> Self {
        Self {
            completion: Span::none(),
            screen_read: None,
            key_down: None,
        }
    }
}

impl ShellRecords {
    /// Notes `delivery` before the shell gives it to core. `span` is the span of the thread that
    /// sent it, which for a suggestion event names its completion. Only an event with text names
    /// the completion of the ghost text: the empty answer of a prompt that core only prepares can
    /// come while an earlier ghost text shows.
    pub(crate) fn note_delivery(&mut self, delivery: &Delivery, span: Span) {
        match delivery {
            Delivery::Suggestion(event) if gives_text(event) => self.completion = span,
            Delivery::Screen(read) => self.screen_read = Some(read.id),
            _ => {}
        }
    }

    /// Notes a key down, so that the next ghost text that shows tells the time since it.
    pub(crate) fn note_key_down(&mut self) {
        self.key_down = Some(Instant::now());
    }

    /// Writes the ghost text of `view`, and where it shows after the draw. The first ghost text
    /// that shows after a key down also tells the time since that key.
    pub(crate) fn ghost(&mut self, view: &GhostView, drawn: Drawn) {
        let key_to_ghost = match drawn {
            Drawn::Overlay | Drawn::Mirror => self.key_down.take().map(|key_down| key_down.elapsed()),
            Drawn::Nothing => None,
        };
        let key_to_ghost_ms = key_to_ghost.map(|time| time.as_secs_f64() * 1000.0);
        let wrapped = view
            .wrapped
            .as_ref()
            .map(|wrapped| tracing::field::debug(&wrapped.lines));
        let screen_read = self.screen_read.map(tracing::field::debug);
        let placement = tracing::field::debug(view.placement);
        let drawn = tracing::field::debug(drawn);
        match &view.ghost {
            Ghost::Text(text) => tracing::info!(
                target: SUGGESTION_LOG_TARGET,
                parent: &self.completion,
                text = text.as_str(),
                wrapped,
                placement,
                drawn,
                screen_read,
                key_to_ghost_ms,
                "showed the ghost text"
            ),
            Ghost::Fix(fix) => tracing::info!(
                target: SUGGESTION_LOG_TARGET,
                parent: &self.completion,
                typo = fix.typo.as_str(),
                word = fix.word.as_str(),
                rest = fix.rest.as_str(),
                wrapped,
                placement,
                drawn,
                screen_read,
                key_to_ghost_ms,
                "showed a fix"
            ),
        }
    }

    /// Writes the `text` that Tab types in place of the `deleted` characters before the caret.
    pub(crate) fn tab(&self, deleted: usize, text: &str) {
        tracing::info!(
            target: SUGGESTION_LOG_TARGET,
            parent: &self.completion,
            deleted,
            text,
            "Tab inserted text"
        );
    }
}

#[cfg(test)]
mod tests {
    use typealike_core::controller::GhostPlacement;
    use typealike_core::inference::{Completion, InferenceError, LogProbability, TokenCount};

    use super::*;

    fn completed(text: &str) -> InferenceEvent {
        InferenceEvent::Completed(Ok(Completion {
            text: text.to_owned(),
            tokens: TokenCount(0),
            duration: Duration::ZERO,
            word_log_probabilities: Vec::new(),
            typed_log_probability: LogProbability(0.0),
            correction: None,
        }))
    }

    #[test]
    fn only_an_event_with_text_names_the_completion_of_the_ghost_text() {
        let events = [
            InferenceEvent::Partial(" soon".to_owned()),
            completed(" soon"),
            completed(""),
            InferenceEvent::Completed(Err(InferenceError::Superseded)),
        ];

        let names_completion = events.each_ref().map(gives_text);

        assert_eq!(names_completion, [true, true, false, false]);
    }

    #[test]
    fn records_name_the_newest_screen_read_that_core_got() {
        let mut records = ShellRecords::default();
        let screen = Delivery::Screen(ScreenRead {
            id: ScreenReadId::default(),
            text: None,
        });
        let suggestion = Delivery::Suggestion(InferenceEvent::Partial("Dear".to_owned()));

        records.note_delivery(&screen, Span::none());
        records.note_delivery(&suggestion, Span::none());

        assert_eq!(records.screen_read, Some(ScreenReadId::default()));
    }

    #[test]
    fn key_time_waits_for_a_ghost_text_that_shows() {
        let mut records = ShellRecords::default();
        let view = GhostView::new(
            Ghost::Text(" soon".to_owned()),
            "See you",
            GhostPlacement::AfterCaret,
        );

        records.note_key_down();
        records.ghost(&view, Drawn::Nothing);
        let is_waiting_after_nothing = records.key_down.is_some();
        records.ghost(&view, Drawn::Overlay);

        assert_eq!(
            (is_waiting_after_nothing, records.key_down.is_some()),
            (true, false)
        );
    }
}
