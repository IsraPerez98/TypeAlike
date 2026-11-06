//! Continues a session on the new line after Enter.

use crate::desktop::{Desktop, is_line_break};

use super::session::recent_text;
use super::{Engine, Reaction, read_surroundings};

impl Engine {
    /// Reads the field of `desktop` again after Enter, and returns the suggestion for the new line
    /// when the session continues there. `None` when the user pressed no Enter since the last read,
    /// or when the session ends.
    pub(crate) fn suggest_on_new_line(&mut self, desktop: &dyn Desktop) -> Option<Reaction> {
        self.read_field_after_enter(desktop).then(|| self.suggest())
    }

    /// Hides the ghost text, keeps the text of the field in `desktop`, and lets the session wait
    /// for a read of its field. A second Enter keeps the text of the first. A session in a field
    /// that the platform cannot read or that gets no suggestions ends.
    pub(super) fn wait_for_new_line(&mut self, desktop: &dyn Desktop) -> Reaction {
        let Some(session) = self
            .session
            .as_mut()
            .filter(|session| session.field.is_text_field() && !session.field.is_excluded)
        else {
            return self.reset();
        };
        if session.field_at_enter.is_none() {
            let field_text = read_surroundings(desktop).field_text;
            let text = field_text
                .as_ref()
                .map_or(session.text_before.as_str(), |field_text| {
                    recent_text(
                        &field_text.before_caret,
                        session.limits,
                        session.max_tokens_before,
                    )
                });
            session.field_at_enter = Some(text.to_owned());
        }
        self.dismiss()
    }

    /// Reads the field of `desktop` when the session waits for a read after Enter. The session
    /// continues with the text that Enter added when the field shows the text of the session, or
    /// the text of the field at Enter, and a new line after it. Else it ends. Returns whether the
    /// session continues on a new line.
    pub(super) fn read_field_after_enter(&mut self, desktop: &dyn Desktop) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        let Some(field_at_enter) = session.field_at_enter.take() else {
            return false;
        };
        let field_text = read_surroundings(desktop).field_text.unwrap_or_default();
        let Some(text_before) =
            text_after_enter(&session.text_before, &field_at_enter, &field_text.before_caret)
        else {
            self.reset();
            return false;
        };
        session.use_read_text(text_before);
        true
    }
}

/// Returns the text before the caret of a session after Enter: `typed`, the text of the session,
/// else `field_at_enter`, the text of the field at Enter, with what Enter added in `field_text`.
/// The field text counts when the app changed the typed text, as a dead key or an autocorrect
/// does. `None` when `field_text` shows neither with a new line.
fn text_after_enter(typed: &str, field_at_enter: &str, field_text: &str) -> Option<String> {
    [typed, field_at_enter]
        .into_iter()
        .find_map(|text| text_on_new_line(text, field_text).map(|added| format!("{text}{added}")))
}

/// Returns what Enter added after `text` in `field_text`, the text before the caret after Enter:
/// line breaks, then signs without letters, such as an indent or a bullet. `None` when
/// `field_text` is not `text` with such an addition.
fn text_on_new_line<'field>(text: &str, field_text: &'field str) -> Option<&'field str> {
    let line_start = field_text
        .trim_end_matches(|character| !is_line_break(character))
        .len();
    let breaks_start = field_text[..line_start].trim_end().len();
    let added_start = field_text[breaks_start..line_start]
        .char_indices()
        .filter(|&(_, character)| is_line_break(character))
        .map(|(offset, _)| breaks_start + offset)
        .find(|&end| field_text[..end].ends_with(text))?;
    let new_line = &field_text[line_start..];
    (!new_line.contains(char::is_alphabetic)).then(|| &field_text[added_start..])
}
