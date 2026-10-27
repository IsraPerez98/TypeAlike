//! The store of kept texts and the pool of each app. It adds a new text, finds the part of a text
//! that the user wrote, and gives the texts of a session.

use super::entry::HistoryEntry;
use crate::desktop::Surroundings;
use crate::excerpt::from_word;
use crate::place::Place;
use crate::settings::{HistoryScope, PersonalizationSettings};

/// The most characters of all the kept texts together. Above it, the oldest texts go first. It
/// keeps the time to rank the examples and to build the tables on the main thread short.
const MAX_STORED_CHARS: usize = 1_000_000;
/// The most texts that TypeAlike keeps, also when their characters are under
/// [`MAX_STORED_CHARS`]. It limits the time to rank the examples and to build the tables of a store
/// of many short texts. Above it, the oldest texts go first.
const MAX_STORED_ENTRIES: usize = 10_000;
/// The share of a text, in percent, that the user must add in a session for TypeAlike to keep the
/// whole text. With a smaller share, TypeAlike keeps only the added part.
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

/// Whether an entry of `entries` in the same place and the same pool of `personalization` as
/// `entry` holds its text.
fn is_known(
    entries: &[HistoryEntry],
    entry: &HistoryEntry,
    personalization: &PersonalizationSettings,
) -> bool {
    entries
        .iter()
        .any(|known| is_same_spot(known, entry, personalization) && known.text.contains(&entry.text))
}

/// Where the text of a new entry starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextOrigin {
    /// The user typed all the text in the session. It is a new text, also when an older text is
    /// the same, so a repeated message counts each time.
    Typed,
    /// The text starts with text that the field held, so it can continue an older text.
    Field,
}

/// Adds `entry` as the newest of `entries`. When the text starts with text of the field
/// ([`TextOrigin::Field`]), an older entry of the same place and the same pool of `personalization`
/// whose text the new text holds goes, and the new entry does not come when such an older one
/// holds its text. A typed text always comes and keeps the older entries. While the entries have
/// more than [`MAX_STORED_CHARS`] characters or are more than [`MAX_STORED_ENTRIES`], the oldest
/// entry goes, in any place and in any pool.
pub(crate) fn add(
    entries: &mut Vec<HistoryEntry>,
    entry: HistoryEntry,
    origin: TextOrigin,
    personalization: &PersonalizationSettings,
) {
    if origin == TextOrigin::Field {
        if is_known(entries, &entry, personalization) {
            return;
        }
        entries.retain(|known| {
            !(is_same_spot(known, &entry, personalization) && entry.text.contains(&known.text))
        });
    }
    entries.push(entry);
    let excess = excess_of(entries);
    entries.drain(..excess);
}

/// Returns how many of the oldest `entries` must go so that the rest have at most
/// [`MAX_STORED_CHARS`] characters and are at most [`MAX_STORED_ENTRIES`].
fn excess_of(entries: &[HistoryEntry]) -> usize {
    let kept = entries
        .iter()
        .rev()
        .take(MAX_STORED_ENTRIES)
        .scan(0, |kept_chars, entry| {
            *kept_chars += entry.text.chars().count();
            Some(*kept_chars)
        })
        .take_while(|&kept_chars| kept_chars <= MAX_STORED_CHARS)
        .count();
    entries.len() - kept
}

/// The text before the caret at the end of a session, with its place.
pub(crate) struct FinishedText<'text> {
    /// The text before the caret.
    pub(crate) text: &'text str,
    /// The characters at the start of the text that came from the field.
    pub(crate) field_chars: usize,
    /// The app where the user wrote the text.
    pub(crate) app_id: &'text str,
    /// The host of the web page where the user wrote the text, if any.
    pub(crate) domain: Option<&'text str>,
}

impl<'text> FinishedText<'text> {
    /// Where the text starts: [`TextOrigin::Typed`] when the characters that came from the field
    /// are only whitespace, else [`TextOrigin::Field`].
    pub(crate) fn origin(&self) -> TextOrigin {
        let (field, _) = self.split();
        if field.trim().is_empty() {
            TextOrigin::Typed
        } else {
            TextOrigin::Field
        }
    }

    /// Returns the characters that came from the field and the characters that the user added.
    fn split(&self) -> (&'text str, &'text str) {
        let added_start = self
            .text
            .char_indices()
            .nth(self.field_chars)
            .map_or(self.text.len(), |(index, _)| index);
        self.text.split_at(added_start)
    }
}

/// Returns the part of the `finished` text that the user wrote. The characters that came from the
/// field count as written by the user when an entry of the same place and the same pool of
/// `personalization` holds them, because the user continues a draft, or when the user added at
/// least [`MIN_OWN_PERCENT`] of the text. Else only the added part counts, from its first whole
/// word.
pub(crate) fn own_text<'text>(
    entries: &[HistoryEntry],
    finished: &FinishedText<'text>,
    personalization: &PersonalizationSettings,
) -> &'text str {
    let text = finished.text;
    let (field, added) = finished.split();
    let is_draft = HistoryEntry::of(field, finished.app_id, finished.domain)
        .is_some_and(|draft| is_known(entries, &draft, personalization));
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
        let finished = FinishedText {
            text,
            field_chars,
            app_id: "com.apple.mail",
            domain: None,
        };
        own_text(entries, &finished, &shared()).to_owned()
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
        let finished = FinishedText {
            text: &text,
            field_chars: DRAFT.chars().count(),
            app_id: "com.google.Chrome",
            domain: Some("mail.example.com"),
        };

        let own = own_text(&entries, &finished, &shared());

        assert_eq!(own, text);
    }

    #[test]
    fn own_text_is_only_the_added_words_when_the_user_adds_a_few_words_to_a_template() {
        let text = format!("{DRAFT} Thanks.");

        let own = own(&[], &text, DRAFT.chars().count());

        assert_eq!(own, "Thanks.");
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
    fn own_text_is_only_the_added_part_when_the_draft_is_in_another_pool() {
        let entries = [entry(DRAFT, "com.apple.Safari", Some("mail.example.com"))];
        let text = format!("{DRAFT} PS: call me.");
        let finished = FinishedText {
            text: &text,
            field_chars: DRAFT.chars().count(),
            app_id: "com.google.Chrome",
            domain: Some("mail.example.com"),
        };

        let own = own_text(&entries, &finished, &safari_on_its_own());

        assert_eq!(own, "PS: call me.");
    }

    #[test]
    fn added_entry_is_the_newest() {
        let mut entries = vec![mail("Old text of the writer")];

        add(
            &mut entries,
            mail("New text of the writer"),
            TextOrigin::Typed,
            &shared(),
        );

        assert_eq!(
            entries,
            [mail("Old text of the writer"), mail("New text of the writer")]
        );
    }

    #[test]
    fn continued_text_replaces_the_older_text_of_its_place_that_it_holds() {
        let mut entries = vec![
            mail("Dear Ana, the report"),
            entry("Dear Ana, the report", "notes", None),
        ];

        add(
            &mut entries,
            mail("Dear Ana, the report is ready."),
            TextOrigin::Field,
            &shared(),
        );

        assert_eq!(
            entries,
            [
                entry("Dear Ana, the report", "notes", None),
                mail("Dear Ana, the report is ready.")
            ]
        );
    }

    #[test]
    fn continued_text_that_an_older_text_of_its_place_holds_is_not_added() {
        let mut entries = vec![mail("Dear Ana, the report is ready.")];

        add(
            &mut entries,
            mail("Dear Ana, the report"),
            TextOrigin::Field,
            &shared(),
        );

        assert_eq!(entries, [mail("Dear Ana, the report is ready.")]);
    }

    #[test]
    fn continued_text_with_the_same_closing_keeps_the_older_text_of_its_place() {
        let closing =
            "I look forward to your comments.\nBest regards,\nCamila Aranguiz\nAcademic Coordinator";
        let older = mail(&format!(
            "Dear Mr. Poblete, I confirm the date of the visit.\n\n{closing}"
        ));
        let newer = mail(&format!(
            "Dear Ms. Tamara, here are the costs of the quarter.\n\n{closing}"
        ));
        let mut entries = vec![older.clone()];

        add(&mut entries, newer.clone(), TextOrigin::Field, &shared());

        assert_eq!(entries, [older, newer]);
    }

    #[test]
    fn continued_text_with_the_same_opening_keeps_the_older_text_of_its_place() {
        let opening = "Hello, thank you for your message to the support team. We received your ticket and";
        let older = mail(&format!("{opening} we will call you about the printer today."));
        let newer = mail(&format!("{opening} the new password is in a separate mail."));
        let mut entries = vec![older.clone()];

        add(&mut entries, newer.clone(), TextOrigin::Field, &shared());

        assert_eq!(entries, [older, newer]);
    }

    #[test]
    fn continued_text_replaces_the_draft_of_the_same_page_in_another_browser() {
        let mut entries = vec![entry(DRAFT, "com.apple.Safari", Some("mail.example.com"))];
        let continued = entry(
            &format!("{DRAFT} PS: call me."),
            "com.google.Chrome",
            Some("mail.example.com"),
        );

        add(&mut entries, continued.clone(), TextOrigin::Field, &shared());

        assert_eq!(entries, [continued]);
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

        add(
            &mut entries,
            continued.clone(),
            TextOrigin::Field,
            &safari_on_its_own(),
        );

        assert_eq!(entries, [draft, continued]);
    }

    #[test]
    fn typed_text_comes_also_when_an_older_text_of_its_place_holds_it() {
        let mut entries = vec![chat("hola como estas?")];

        add(
            &mut entries,
            chat("hola como estas?"),
            TextOrigin::Typed,
            &shared(),
        );

        assert_eq!(entries, [chat("hola como estas?"), chat("hola como estas?")]);
    }

    #[test]
    fn typed_text_keeps_the_older_text_of_its_place_that_it_holds() {
        let mut entries = vec![chat("hola como estas?")];

        add(
            &mut entries,
            chat("hola como estas? todo bien?"),
            TextOrigin::Typed,
            &shared(),
        );

        assert_eq!(
            entries,
            [chat("hola como estas?"), chat("hola como estas? todo bien?")]
        );
    }

    #[test]
    fn finished_text_is_typed_when_the_field_gave_only_whitespace() {
        let texts = [
            ("hola como estas?", 0),
            (
                "
hola como estas?",
                1,
            ),
            ("hola como estas?", 5),
        ];

        let origins = texts.map(|(text, field_chars)| {
            FinishedText {
                text,
                field_chars,
                app_id: "net.whatsapp.WhatsApp",
                domain: None,
            }
            .origin()
        });

        assert_eq!(origins, [TextOrigin::Typed, TextOrigin::Typed, TextOrigin::Field]);
    }

    fn chat(text: &str) -> HistoryEntry {
        entry(text, "net.whatsapp.WhatsApp", None)
    }

    fn text_of(letter: char, chars: usize) -> String {
        std::iter::repeat_n(letter, chars).collect()
    }

    fn numbered_mails(count: usize) -> Vec<HistoryEntry> {
        (0..count).map(|index| mail(&format!("text {index}."))).collect()
    }

    #[test]
    fn short_texts_under_the_character_limit_all_stay() {
        let mut entries = numbered_mails(MAX_STORED_ENTRIES - 1);

        add(&mut entries, mail("newest"), TextOrigin::Typed, &shared());

        assert_eq!(entries.len(), MAX_STORED_ENTRIES);
        assert_eq!(entries.first(), Some(&mail("text 0.")));
    }

    #[test]
    fn new_text_pushes_out_the_oldest_text_of_the_store_in_another_place() {
        let document = entry(&text_of('d', MAX_STORED_CHARS / 2), "md.obsidian", None);
        let old_chat = chat(&text_of('c', MAX_STORED_CHARS / 2));
        let mut entries = vec![document, old_chat.clone()];

        add(&mut entries, chat("newest chat."), TextOrigin::Typed, &shared());

        assert_eq!(entries, [old_chat, chat("newest chat.")]);
    }

    #[test]
    fn store_at_the_character_limit_keeps_all_its_texts() {
        let newest = chat("newest chat.");
        let old_chat = chat(&text_of('c', MAX_STORED_CHARS - newest.text.chars().count()));
        let mut entries = vec![old_chat.clone()];

        add(&mut entries, newest.clone(), TextOrigin::Typed, &shared());

        assert_eq!(entries, [old_chat, newest]);
    }

    #[test]
    fn count_guard_pushes_out_the_oldest_text_under_the_character_limit() {
        let mut entries = numbered_mails(MAX_STORED_ENTRIES);

        add(&mut entries, mail("newest"), TextOrigin::Typed, &shared());

        assert_eq!(entries.len(), MAX_STORED_ENTRIES);
        assert_eq!(entries.first(), Some(&mail("text 1.")));
        assert_eq!(entries.last(), Some(&mail("newest")));
    }

    #[test]
    fn store_over_the_character_limit_drops_its_oldest_texts_until_it_is_under_it() {
        let quarter = MAX_STORED_CHARS / 4;
        let kept = [mail(&text_of('c', 2 * quarter)), mail(&text_of('d', quarter))];
        let mut entries = [mail(&text_of('a', quarter)), mail(&text_of('b', quarter))]
            .into_iter()
            .chain(kept.clone())
            .collect::<Vec<_>>();

        add(&mut entries, chat("newest chat."), TextOrigin::Typed, &shared());

        assert_eq!(entries, [kept[0].clone(), kept[1].clone(), chat("newest chat.")]);
    }
}
