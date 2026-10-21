//! The store of kept texts and the pool of each app. It adds a new text, finds the part of a text
//! that the user wrote, and gives the texts of a session.

use std::collections::HashMap;

use super::entry::HistoryEntry;
use crate::desktop::Surroundings;
use crate::excerpt::from_word;
use crate::place::Place;
use crate::settings::{HistoryScope, PersonalizationSettings};

/// The most texts that GhostType keeps. The oldest text of the place with the most texts goes
/// first.
const MAX_ENTRIES: usize = 2000;
/// The share of a text, in percent, that the user must add in a session for GhostType to keep the
/// whole text. With a smaller share, GhostType keeps only the added part.
const MIN_OWN_PERCENT: usize = 50;

/// The earlier writing that a session can use: the texts of the apps whose scope gives them to
/// the app of the session. `Debug` shows only the app id of a pool of one app.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum HistoryPool {
    /// The texts of all the apps with [`HistoryScope::AllApps`].
    Shared,
    /// The texts of the app with this id, which has [`HistoryScope::ThisApp`].
    App(String),
}

impl HistoryPool {
    /// The pool of the app with `app_id`, or `None` when `personalization` turns the app off.
    pub(crate) fn of(app_id: &str, personalization: &PersonalizationSettings) -> Option<Self> {
        match personalization.scope_of(app_id) {
            HistoryScope::Off => None,
            HistoryScope::ThisApp => Some(Self::App(app_id.to_owned())),
            HistoryScope::AllApps => Some(Self::Shared),
        }
    }
}

/// Whether `left` and `right` are in the same place and in the same pool, so that one text can
/// replace or continue the other.
fn is_same_spot(
    left: &HistoryEntry,
    right: &HistoryEntry,
    personalization: &PersonalizationSettings,
) -> bool {
    left.place() == right.place()
        && (left.app_id == right.app_id
            || HistoryPool::of(&left.app_id, personalization)
                == HistoryPool::of(&right.app_id, personalization))
}

/// Adds `entry` as the newest of `entries`. An older entry of the same place and the same pool of
/// `personalization` whose text the new text holds goes, and the new entry does not come when such
/// an older one holds its text. Such an older entry that starts or ends like the new text (a near
/// copy) also goes. While
/// there are more than [`MAX_ENTRIES`], the oldest entry of the place with the most entries goes,
/// so that a busy place does not push out the texts of the rare places.
pub(crate) fn add(
    entries: &mut Vec<HistoryEntry>,
    entry: HistoryEntry,
    personalization: &PersonalizationSettings,
) {
    let is_known = entries
        .iter()
        .any(|known| is_same_spot(known, &entry, personalization) && known.text.contains(&entry.text));
    if is_known {
        return;
    }
    entries.retain(|known| {
        let is_replaced = entry.text.contains(&known.text) || entry.shares_an_edge_with(known);
        !(is_same_spot(known, &entry, personalization) && is_replaced)
    });
    entries.push(entry);
    while entries.len() > MAX_ENTRIES {
        let oldest = oldest_of_largest_place(entries);
        entries.remove(oldest);
    }
}

/// Returns the index of the oldest of `entries` in the place with the most entries. Among places
/// with the same number of entries, the place with the oldest entry goes first.
fn oldest_of_largest_place(entries: &[HistoryEntry]) -> usize {
    let mut counts = HashMap::<Place<&str>, usize>::new();
    for entry in entries {
        *counts.entry(entry.place()).or_default() += 1;
    }
    let largest = counts.values().copied().max().unwrap_or_default();
    entries
        .iter()
        .position(|entry| counts[&entry.place()] == largest)
        .unwrap_or_default()
}

/// Returns the part of `text`, the text before the caret at the end of a session in the place of
/// `app_id` and `domain`, that the user wrote. The first `field_chars` characters came from the
/// field. They count as written by the user when an entry of the same place and the same pool of
/// `personalization` holds them, because the user continues a draft, or when the user added at
/// least [`MIN_OWN_PERCENT`] of the text. Else only the added part counts, from its first whole
/// word.
pub(crate) fn own_text<'text>(
    entries: &[HistoryEntry],
    text: &'text str,
    field_chars: usize,
    app_id: &str,
    domain: Option<&str>,
    personalization: &PersonalizationSettings,
) -> &'text str {
    let added_start = text
        .char_indices()
        .nth(field_chars)
        .map_or(text.len(), |(index, _)| index);
    let (field, added) = text.split_at(added_start);
    let is_draft = HistoryEntry::of(field, app_id, domain).is_some_and(|draft| {
        entries
            .iter()
            .any(|known| is_same_spot(known, &draft, personalization) && known.text.contains(&draft.text))
    });
    let is_mostly_added = added.chars().count() * 100 >= text.chars().count() * MIN_OWN_PERCENT;
    if field.trim().is_empty() || is_draft || is_mostly_added {
        return text;
    }
    from_word(text, added)
}

/// The texts of the pool of a session, each with its index in the history, oldest first.
pub(crate) struct SessionTexts<'entries> {
    /// The texts in the place of the session.
    pub(super) local: Vec<(usize, &'entries HistoryEntry)>,
    /// The texts in the other places.
    pub(super) wide: Vec<(usize, &'entries HistoryEntry)>,
}

impl SessionTexts<'_> {
    /// Whether the session has a text of its place.
    pub(crate) fn has_local(&self) -> bool {
        !self.local.is_empty()
    }
}

/// Returns the texts of `entries` that a session in the field of `surroundings` can use: the texts
/// of the pool that `personalization` gives its app. It has none when the app is off or has no id.
pub(crate) fn session_texts<'entries>(
    entries: &'entries [HistoryEntry],
    surroundings: &Surroundings,
    personalization: &PersonalizationSettings,
) -> SessionTexts<'entries> {
    let pool = surroundings
        .app_id
        .as_deref()
        .and_then(|app_id| HistoryPool::of(app_id, personalization));
    let place = Place::of_field(surroundings);
    let (local, wide) = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| pool.is_some() && HistoryPool::of(&entry.app_id, personalization) == pool)
        .partition(|(_, entry)| Some(entry.place()) == place);
    SessionTexts { local, wide }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{entry, mail, shared};
    use crate::settings::HistoryApp;

    const DRAFT: &str = "Dear Ana, I send you the report of the third quarter with all the numbers.";

    fn own(entries: &[HistoryEntry], text: &str, field_chars: usize) -> String {
        own_text(entries, text, field_chars, "com.apple.mail", None, &shared()).to_owned()
    }

    #[test]
    fn own_text_is_all_the_text_when_the_field_was_empty() {
        let own = own(&[], DRAFT, 0);

        assert_eq!(own, DRAFT);
    }

    #[test]
    fn own_text_is_all_the_text_when_the_user_adds_half_of_it_or_more() {
        let text =
            format!("{DRAFT} The costs went down by two percent in the quarter and the team is very happy.");
        let field_chars = DRAFT.chars().count();

        let own = own(&[], &text, field_chars);

        assert_eq!(own, text);
    }

    #[test]
    fn own_text_is_only_the_added_paragraph_for_a_long_shared_text() {
        let shared = format!("{} ", "The terms of the lease are in force. ".repeat(10));
        let paragraph = "My comment: the deposit must come back in thirty days.";
        let text = format!("{shared}{paragraph}");

        let own = own(&[], &text, shared.chars().count());

        assert_eq!(own, paragraph);
    }

    #[test]
    fn own_text_starts_at_a_whole_word() {
        let shared =
            "Template text that the user did not write, which goes on for a while to be long".to_owned();
        let text = format!("{shared}ing and then the user wrote a long enough sentence of their own.");

        let own = own(&[], &text, shared.chars().count());

        assert_eq!(
            own,
            "and then the user wrote a long enough sentence of their own."
        );
    }

    #[test]
    fn own_text_is_all_the_text_when_an_entry_of_the_place_holds_the_field() {
        let entries = [mail(DRAFT)];
        let text = format!("{DRAFT} PS: call me.");

        let own = own(&entries, &text, DRAFT.chars().count());

        assert_eq!(own, text);
    }

    #[test]
    fn own_text_is_only_the_added_part_when_an_entry_of_another_place_holds_the_field() {
        let entries = [entry(DRAFT, "com.apple.Notes", None)];
        let text = format!("{DRAFT} {}", "PS: call me when you can, it is long.");

        let own = own(&entries, &text, DRAFT.chars().count());

        assert_eq!(own, "PS: call me when you can, it is long.");
    }

    #[test]
    fn own_text_is_all_the_text_when_an_entry_of_the_same_page_in_another_browser_holds_the_field() {
        let entries = [entry(DRAFT, "com.apple.Safari", Some("mail.example.com"))];
        let text = format!("{DRAFT} PS: call me.");

        let own = own_text(
            &entries,
            &text,
            DRAFT.chars().count(),
            "com.google.Chrome",
            Some("mail.example.com"),
            &shared(),
        );

        assert_eq!(own, text);
    }

    #[test]
    fn own_text_is_only_the_added_words_when_the_user_adds_a_few_words_to_a_template() {
        let text = format!("{DRAFT} Thanks.");

        let own = own(&[], &text, DRAFT.chars().count());

        assert_eq!(own, "Thanks.");
    }

    #[test]
    fn added_entry_is_the_newest() {
        let mut entries = vec![mail("Old text of the writer")];

        add(&mut entries, mail("New text of the writer"), &shared());

        assert_eq!(
            entries,
            [mail("Old text of the writer"), mail("New text of the writer")]
        );
    }

    #[test]
    fn added_entry_replaces_the_older_text_of_its_place_that_it_holds() {
        let mut entries = vec![
            mail("Dear Ana, the report"),
            entry("Dear Ana, the report", "notes", None),
        ];

        add(&mut entries, mail("Dear Ana, the report is ready."), &shared());

        assert_eq!(
            entries,
            [
                entry("Dear Ana, the report", "notes", None),
                mail("Dear Ana, the report is ready.")
            ]
        );
    }

    #[test]
    fn entry_that_an_older_text_of_its_place_holds_is_not_added() {
        let mut entries = vec![mail("Dear Ana, the report is ready.")];

        add(&mut entries, mail("Dear Ana, the report"), &shared());

        assert_eq!(entries, [mail("Dear Ana, the report is ready.")]);
    }

    #[test]
    fn added_entry_replaces_a_near_copy_with_an_edit_in_the_middle() {
        let start = "Dear Ana, I send you the report of the third quarter with all the numbers.";
        let old = format!("{start} The costs went down by two percent. Best, Tom");
        let new = format!("{start} The costs went down by four percent. Best, Tom");
        let mut entries = vec![mail(&old)];

        add(&mut entries, mail(&new), &shared());

        assert_eq!(entries, [mail(&new)]);
    }

    #[test]
    fn added_entry_replaces_a_near_copy_with_an_edit_at_its_start() {
        let end = "the numbers of the third quarter are ready and the costs went down. Best, Tom";
        let mut entries = vec![mail(&format!("Hi Ana, {end}"))];

        add(&mut entries, mail(&format!("Hello Anna, {end}")), &shared());

        assert_eq!(entries, [mail(&format!("Hello Anna, {end}"))]);
    }

    #[test]
    fn short_texts_with_the_same_greeting_stay_two_entries() {
        let mut entries = vec![mail("Dear Ana, the report is ready for you.")];

        add(
            &mut entries,
            mail("Dear Ana, the meeting moved to Friday."),
            &shared(),
        );

        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn added_entry_replaces_the_draft_of_the_same_page_in_another_browser() {
        let mut entries = vec![entry(DRAFT, "com.apple.Safari", Some("mail.example.com"))];
        let continued = entry(
            &format!("{DRAFT} PS: call me."),
            "com.google.Chrome",
            Some("mail.example.com"),
        );

        add(&mut entries, continued.clone(), &shared());

        assert_eq!(entries, [continued]);
    }

    fn safari_on_its_own() -> PersonalizationSettings {
        PersonalizationSettings {
            apps: vec![HistoryApp {
                app_id: "com.apple.Safari".to_owned(),
                app_name: "Safari".to_owned(),
                app_path: None,
                scope: HistoryScope::ThisApp,
            }],
            ..shared()
        }
    }

    #[test]
    fn draft_of_an_app_with_only_this_app_stays_when_another_browser_continues_it() {
        let draft = entry(DRAFT, "com.apple.Safari", Some("mail.example.com"));
        let mut entries = vec![draft.clone()];
        let continued = entry(
            &format!("{DRAFT} PS: call me."),
            "com.google.Chrome",
            Some("mail.example.com"),
        );

        add(&mut entries, continued.clone(), &safari_on_its_own());

        assert_eq!(entries, [draft, continued]);
    }

    #[test]
    fn own_text_is_only_the_added_part_when_the_draft_is_in_another_pool() {
        let entries = [entry(DRAFT, "com.apple.Safari", Some("mail.example.com"))];
        let text = format!("{DRAFT} PS: call me.");

        let own = own_text(
            &entries,
            &text,
            DRAFT.chars().count(),
            "com.google.Chrome",
            Some("mail.example.com"),
            &safari_on_its_own(),
        );

        assert_eq!(own, "PS: call me.");
    }

    #[test]
    fn near_copy_in_another_place_stays() {
        let text = "Dear Ana, I send you the report of the third quarter with all the numbers. Best, Tom";
        let mut entries = vec![entry(text, "com.apple.Notes", None)];

        add(&mut entries, mail(&format!("{text} PS")), &shared());

        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn oldest_entries_go_after_the_most_entries() {
        let mut entries = (0..MAX_ENTRIES)
            .map(|index| mail(&format!("text {index}.")))
            .collect::<Vec<_>>();

        add(&mut entries, mail("newest"), &shared());

        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries.first(), Some(&mail("text 1.")));
        assert_eq!(entries.last(), Some(&mail("newest")));
    }

    #[test]
    fn full_store_drops_the_oldest_text_of_the_place_with_the_most_texts() {
        let chat = |text: &str| entry(text, "net.whatsapp.WhatsApp", None);
        let document = entry("Notes on the roof repair.", "md.obsidian", None);
        let mut entries = std::iter::once(document.clone())
            .chain((1..MAX_ENTRIES).map(|index| chat(&format!("chat {index}."))))
            .collect::<Vec<_>>();

        add(&mut entries, chat("newest chat."), &shared());

        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[..2], [document, chat("chat 2.")]);
        assert_eq!(entries.last(), Some(&chat("newest chat.")));
    }
}
