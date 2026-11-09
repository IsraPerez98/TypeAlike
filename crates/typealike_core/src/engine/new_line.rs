//! Continues a session on the new line after Enter.

use crate::desktop::{Desktop, is_line_break};
use crate::key::Press;

use super::session::recent_text;
use super::{Engine, Reaction, read_surroundings};

/// What a session keeps from Enter until the read of its field after Enter.
pub(super) struct EnterWait {
    /// The text of the session at Enter.
    typed: String,
    /// The text before the caret in the field at Enter. It is `typed` when the platform gave no
    /// field text.
    field: String,
    /// The text before the caret that the read at the settled caret took, if it found a new line.
    new_line: Option<String>,
    /// Whether the settled caret reads the field: the user pressed the last Enter, the OS did not
    /// repeat it, no read at the settled caret came after it, and the field is not a terminal. A
    /// terminal shows the new line at once, but the output of the command only later.
    is_caret_read_due: bool,
}

impl EnterWait {
    /// Returns the text before the caret after Enter that `field_text` shows ([`text_after_enter`]).
    fn text_shown_in(&self, field_text: &str) -> Option<String> {
        text_after_enter(&self.typed, &self.field, field_text)
    }
}

/// What the read of the field after Enter did to the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NewLineRead {
    /// The session continues on a new line that it did not have before the read.
    NewLine,
    /// The session did not change: no read after Enter waited, or the field shows the new line
    /// that the read at the settled caret took.
    Same,
    /// The field shows no new line after the text of the session, so the session ended.
    Ended,
}

impl Engine {
    /// Reads the field of `desktop` again after Enter, and returns the suggestion for the new line
    /// when the session continues on a line that the read at the settled caret did not take. When
    /// the session ends after that read took a new line, it hides the suggestion of that line.
    /// `None` when the user pressed no Enter since the last read, when the session keeps the new
    /// line of the settled caret, or when the session ends with no suggestion.
    pub(crate) fn suggest_on_new_line(&mut self, desktop: &dyn Desktop) -> Option<Reaction> {
        let had_offer = self.has_offer();
        match self.read_field_after_enter(desktop) {
            NewLineRead::NewLine => Some(self.suggest()),
            NewLineRead::Ended if had_offer => Some(Reaction::Hide),
            NewLineRead::Ended | NewLineRead::Same => None,
        }
    }

    /// Reads the field of `desktop` once after Enter, when the caret settled, so that the
    /// suggestion for the new line does not wait for the input to settle. Returns that suggestion
    /// when the field shows a new line that the session did not take yet. The session still waits
    /// for the read when the input settles or at the next key, because the app can still change
    /// the field, as a chat that sends the message.
    pub(crate) fn suggest_on_settled_caret(&mut self, desktop: &dyn Desktop) -> Option<Reaction> {
        let session = self.session.as_mut()?;
        let wait = session
            .enter_wait
            .as_mut()
            .filter(|wait| wait.is_caret_read_due)?;
        wait.is_caret_read_due = false;
        let field_text = read_surroundings(desktop).field_text?;
        let text_before = wait
            .text_shown_in(&field_text.before_caret)
            .filter(|text| wait.new_line.as_ref() != Some(text))?;
        wait.new_line = Some(text_before.clone());
        session.use_read_text(text_before);
        Some(self.suggest())
    }

    /// Hides the ghost text, keeps the text of the field in `desktop`, and lets the session wait
    /// for a read of its field. A second Enter keeps the text of the first. A session in a field
    /// that the platform cannot read or that gets no suggestions ends.
    pub(super) fn wait_for_new_line(&mut self, desktop: &dyn Desktop, press: Press) -> Reaction {
        let Some(session) = self
            .session
            .as_mut()
            .filter(|session| session.field.is_text_field() && !session.field.is_excluded)
        else {
            return self.reset();
        };
        let wait = session.enter_wait.get_or_insert_with(|| {
            let field_text = read_surroundings(desktop).field_text;
            let field = field_text
                .as_ref()
                .map_or(session.text_before.as_str(), |field_text| {
                    recent_text(
                        &field_text.before_caret,
                        session.limits,
                        session.max_tokens_before,
                    )
                });
            EnterWait {
                typed: session.text_before.clone(),
                field: field.to_owned(),
                new_line: None,
                is_caret_read_due: false,
            }
        });
        wait.is_caret_read_due = press == Press::First && !session.field.is_terminal();
        self.dismiss()
    }

    /// Reads the field of `desktop` when the session waits for a read after Enter. The session
    /// continues with the text that Enter added when the field shows the text of the session, or
    /// the text of the field at Enter, and a new line after it. Else it ends.
    pub(super) fn read_field_after_enter(&mut self, desktop: &dyn Desktop) -> NewLineRead {
        let Some(session) = self.session.as_mut() else {
            return NewLineRead::Same;
        };
        let Some(wait) = session.enter_wait.take() else {
            return NewLineRead::Same;
        };
        let field_text = read_surroundings(desktop).field_text.unwrap_or_default();
        match wait.text_shown_in(&field_text.before_caret) {
            None => {
                self.reset();
                NewLineRead::Ended
            }
            Some(text_before) if wait.new_line.as_ref() == Some(&text_before) => NewLineRead::Same,
            Some(text_before) => {
                session.use_read_text(text_before);
                NewLineRead::NewLine
            }
        }
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
/// `field_text` is not `text` with such an addition. The field can show more text before a
/// `text` that is the end of a longer text, but not before an empty `text`.
fn text_on_new_line<'field>(text: &str, field_text: &'field str) -> Option<&'field str> {
    let line_start = field_text
        .trim_end_matches(|character| !is_line_break(character))
        .len();
    let breaks_start = field_text[..line_start].trim_end().len();
    let added_start = field_text[breaks_start..line_start]
        .char_indices()
        .filter(|&(_, character)| is_line_break(character))
        .map(|(offset, _)| breaks_start + offset)
        .find(|&end| field_text[..end].ends_with(text) && (end == 0 || !text.is_empty()))?;
    let new_line = &field_text[line_start..];
    (!new_line.contains(char::is_alphabetic)).then(|| &field_text[added_start..])
}
