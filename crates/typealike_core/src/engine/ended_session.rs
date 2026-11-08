//! Waits until the app handles the key that ended a session, so that the history keeps only the
//! text that the field still holds after that key.

use crate::desktop::{Desktop, FieldLabels, FieldText, Surroundings};
use crate::key::Key;

use super::session::Session;
use super::{Engine, Reaction, read_surroundings};

/// A session that a key ended before the app handled the key, with the field where it typed.
pub(super) struct KeyEndedSession {
    session: Session,
    field: FieldIdentity,
}

/// What tells a field apart from the other fields of the desktop.
#[derive(PartialEq, Eq)]
struct FieldIdentity {
    app_id: Option<String>,
    domain: Option<String>,
    window_title: Option<String>,
    labels: FieldLabels,
}

impl FieldIdentity {
    fn of(surroundings: &Surroundings) -> Self {
        Self {
            app_id: surroundings.app_id.clone(),
            domain: surroundings.domain.clone(),
            window_title: surroundings.window_title.clone(),
            labels: surroundings.field_labels.clone(),
        }
    }
}

impl Engine {
    /// Ends the session at a shortcut, a navigation key or a control character, which the app did
    /// not handle yet. Such a key can delete or select text, as Option+Backspace and Command+A do,
    /// so the history keeps the text of the session only at the next key.
    pub(super) fn end_session_at_key(&mut self, desktop: &dyn Desktop) -> Reaction {
        if let Some(session) = self.session.take() {
            self.keep_key_ended_session();
            let field = FieldIdentity::of(&desktop.surroundings());
            self.key_ended_session = Some(KeyEndedSession { session, field });
        }
        self.end_offer();
        Reaction::Hide
    }

    /// Follows the session that a key ended at `key`, which the app did not handle yet: the history
    /// keeps the text that the field of `desktop` still holds. The session waits again when `key`
    /// also ends a session, as a second Option+Backspace does.
    pub(super) fn follow_key_ended_session_at(&mut self, key: Key, desktop: &dyn Desktop) {
        self.follow_key_ended_session(desktop);
        if !ends_session_before_app(key) {
            self.keep_key_ended_session();
        }
    }

    /// Cuts the text of the session that a key ended to the text that the field of `desktop` still
    /// holds, now that the app handled that key. In a field that the platform cannot read, the text
    /// stays whole. In another field, the history keeps the whole text, because the key moved the
    /// focus.
    pub(crate) fn follow_key_ended_session(&mut self, desktop: &dyn Desktop) {
        let Some(ended) = &mut self.key_ended_session else {
            return;
        };
        let surroundings = read_surroundings(desktop);
        if ended.field != FieldIdentity::of(&surroundings) {
            self.keep_key_ended_session();
            return;
        }
        if let Some(field_text) = &surroundings.field_text {
            let held = held_text(&ended.session.text_before, field_text).to_owned();
            ended.session.use_read_text(held);
        }
    }

    /// Keeps the whole text of the session that a key ended, for when the caret moved somewhere
    /// unknown.
    pub(super) fn keep_key_ended_session(&mut self) {
        if let Some(ended) = self.key_ended_session.take() {
            self.keep_text_of(&ended.session);
        }
    }
}

/// Whether `key` ends a session before the app handles it: a shortcut, a navigation key or a
/// control character.
fn ends_session_before_app(key: Key) -> bool {
    match key {
        Key::Char(character) => character.is_control(),
        Key::Other => true,
        Key::Backspace | Key::Tab | Key::Escape | Key::Enter | Key::Dead => false,
    }
}

/// Returns the start of `text`, the text before the caret at the end of a session, that `field`
/// still holds: the longest start of `text` that ends the text before the caret, then the rest of
/// `text` as far as the text after the caret starts with it, because a key can move the caret back
/// without a change to the text.
fn held_text<'text>(text: &'text str, field: &FieldText) -> &'text str {
    let before_end = text
        .char_indices()
        .map(|(index, _)| index)
        .chain([text.len()])
        .rev()
        .find(|&end| field.before_caret.ends_with(&text[..end]))
        .unwrap_or_default();
    let after_len: usize = text[before_end..]
        .chars()
        .zip(field.after_caret.chars())
        .take_while(|(held, after)| held == after)
        .map(|(held, _)| held.len_utf8())
        .sum();
    &text[..before_end + after_len]
}
