//! Keeps the earlier writing of the user and the tables of words and phrases that come from it.

use std::mem;

use crate::desktop::Desktop;
use crate::history::{self, HistoryEntry, HistoryTables};
use crate::settings::{HistoryScope, KeptTexts, PersonalizationSettings, ReplyWords};

use super::Engine;

/// The most seconds between two builds of the tables while sessions start, so that a text leaves
/// the last day of the tables at most this late.
const TABLES_MAX_AGE_SECONDS: u64 = 60 * 60;
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
        self.drop_reply_words_when_off();
        self.refresh_tables();
    }

    /// Removes the words of the answered messages from the earlier writing while the reply words
    /// are off. The history then needs a save when it had such words.
    pub(super) fn drop_reply_words_when_off(&mut self) {
        if self.personalization.reply_words == ReplyWords::On {
            return;
        }
        for entry in self.history.iter_mut().filter(|entry| !entry.reply_to.is_empty()) {
            entry.reply_to.clear();
            self.has_unsaved_history = true;
        }
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
        let Some(app) = &session.field.app else {
            return;
        };
        let is_wanted = session.has_accepted || self.personalization.kept_texts == KeptTexts::All;
        if session.field.is_excluded
            || !session.field.can_keep_text()
            || !is_wanted
            || self.personalization.scope_of(&app.app_id) == HistoryScope::Off
        {
            return;
        }
        let domain = session.field.domain.as_deref();
        let finished = history::FinishedText {
            text: &session.text_before,
            field_chars: session.field_chars,
            app_id: &app.app_id,
            domain,
        };
        let written = history::own_text(&self.history, &finished, &self.personalization);
        let Some(entry) = HistoryEntry::of(written, &app.app_id, domain, session.started_at) else {
            return;
        };
        let entry = HistoryEntry {
            reply_to: session.reply_to.clone(),
            typing_context: session.field.typing_context.clone(),
            ..entry
        };
        history::add(&mut self.history, entry, finished.origin(), &self.personalization);
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

    /// Reads the clock of `desktop`, and builds the tables again when they are older than
    /// [`TABLES_MAX_AGE_SECONDS`], because the texts of the last day count each time and an
    /// earlier day counts at most two times.
    pub(super) fn follow_clock(&mut self, desktop: &dyn Desktop) {
        self.clock = desktop.now();
        let is_stale = match (self.clock, self.tables_clock) {
            (Some(now), Some(built)) => now.0.saturating_sub(built.0) >= TABLES_MAX_AGE_SECONDS,
            (now, built) => now != built,
        };
        if is_stale {
            self.refresh_tables();
        }
    }

    /// Builds the word bias of each pool, with new versions, and the phrases again from the
    /// history.
    pub(super) fn refresh_tables(&mut self) {
        self.tables_clock = self.clock;
        self.tables = HistoryTables::of(
            &self.history,
            &self.personalization,
            &mut self.word_bias_version,
            self.clock,
        );
    }
}
