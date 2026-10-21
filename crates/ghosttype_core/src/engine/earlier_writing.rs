//! Keeps the earlier writing of the user and the tables of words and phrases that come from it.

use std::mem;

use crate::desktop::Desktop;
use crate::history::{self, HistoryEntry, HistoryTables};
use crate::settings::{HistoryScope, KeptTexts, PersonalizationSettings};

use super::Engine;
use super::session::Session;

/// Whether the tables of the earlier writing for the personalization `new` differ from the tables
/// for `old`: another word choice, or another scope of an app.
pub(super) fn needs_new_tables(old: &PersonalizationSettings, new: &PersonalizationSettings) -> bool {
    old.word_choice != new.word_choice
        || old.history != new.history
        || old
            .apps
            .iter()
            .chain(&new.apps)
            .any(|app| old.scope_of(&app.app_id) != new.scope_of(&app.app_id))
}

impl Engine {
    /// Uses `entries`, the earlier writing of the user, oldest first, from the next session on.
    pub(crate) fn use_history(&mut self, entries: Vec<HistoryEntry>) {
        self.history = entries;
        self.refresh_tables();
    }

    /// Whether the earlier writing that suggestions may use holds a text of the place of the
    /// focused field of `desktop`.
    #[must_use]
    pub(crate) fn has_earlier_writing_in(&self, desktop: &dyn Desktop) -> bool {
        if self.history.is_empty() {
            return false;
        }
        history::session_texts(&self.history, &desktop.surroundings(), &self.personalization).has_local()
    }

    pub(super) fn keep_text_of(&mut self, session: &Session) {
        let Some(app) = &session.app else {
            return;
        };
        let is_wanted = session.has_accepted || self.personalization.kept_texts == KeptTexts::All;
        if session.is_excluded
            || !is_wanted
            || self.personalization.scope_of(&app.app_id) == HistoryScope::Off
        {
            return;
        }
        let domain = session.domain.as_deref();
        let written = history::own_text(
            &self.history,
            &session.text_before,
            session.field_chars,
            &app.app_id,
            domain,
            &self.personalization,
        );
        let Some(entry) = HistoryEntry::of(written, &app.app_id, domain) else {
            return;
        };
        history::add(&mut self.history, entry, &self.personalization);
        self.has_unsaved_history = true;
    }

    /// Returns the earlier writing of the user, oldest first, when it changed since the last call.
    /// The word bias and the phrases then come from the new history.
    pub(crate) fn history_to_save(&mut self) -> Option<&[HistoryEntry]> {
        if !mem::take(&mut self.has_unsaved_history) {
            return None;
        }
        self.refresh_tables();
        Some(&self.history)
    }

    /// Forgets the earlier writing of the app with `app_id`.
    pub(crate) fn forget_history_of(&mut self, app_id: &str) {
        self.history.retain(|entry| entry.app_id != app_id);
        self.has_unsaved_history = true;
    }

    /// Builds the word bias of each pool, with new versions, and the phrases again from the
    /// history.
    pub(super) fn refresh_tables(&mut self) {
        self.tables = HistoryTables::of(&self.history, &self.personalization, &mut self.word_bias_version);
    }
}
