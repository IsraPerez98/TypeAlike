//! The records of the suggestion log that macOS writes: the fields that it reads, the screen text,
//! the ghost text and the text that Tab inserts. They hold what the user wrote, so only a build
//! with the `suggestion-log` feature has them.

use objc2_application_services::AXUIElement;
use typealike_core::SUGGESTION_LOG_TARGET;
use typealike_core::controller::{Ghost, GhostView};
use typealike_core::desktop::{ScreenReadId, ScreenSource, ScreenText, Surroundings};

use crate::accessibility::{ROLE_ATTRIBUTE, SUBROLE_ATTRIBUTE, copy_string, frame};

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

/// Writes the lines of the screen `text` that the read `read` from `source` gave.
pub(crate) fn screen_read(read: ScreenReadId, source: ScreenSource, text: Option<&ScreenText>) {
    tracing::info!(
        target: SUGGESTION_LOG_TARGET,
        ?read,
        ?source,
        lines = text.map(|text| {
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
