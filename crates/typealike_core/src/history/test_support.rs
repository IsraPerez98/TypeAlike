//! The helpers that the tests of two or more parts of the earlier writing share.

use super::entry::HistoryEntry;
use crate::settings::{HistoryAccess, PersonalizationSettings};

pub(super) fn entry(text: &str, app_id: &str, domain: Option<&str>) -> HistoryEntry {
    HistoryEntry {
        text: text.to_owned(),
        app_id: app_id.to_owned(),
        domain: domain.map(str::to_owned),
    }
}

pub(super) fn mail(text: &str) -> HistoryEntry {
    entry(text, "com.apple.mail", None)
}

pub(super) fn shared() -> PersonalizationSettings {
    PersonalizationSettings {
        history: HistoryAccess::On,
        ..PersonalizationSettings::default()
    }
}

pub(super) fn refs(entries: &[HistoryEntry]) -> Vec<&HistoryEntry> {
    entries.iter().collect()
}

pub(super) fn word(word: &str, weight: f32) -> (String, f32) {
    (word.to_owned(), weight)
}
