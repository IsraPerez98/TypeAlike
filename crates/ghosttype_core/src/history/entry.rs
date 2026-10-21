//! One kept text of the user: its trim, its redaction, its cut to the end and its overlap checks.

use std::fmt;

use super::redaction::redacted;
use crate::excerpt::{from_word, last_chars};
use crate::place::Place;

/// The fewest characters of a text that GhostType saves.
pub(super) const MIN_KEPT_CHARS: usize = 40;
/// The most characters of a text that GhostType saves, from its end.
const MAX_KEPT_CHARS: usize = 1000;
/// The characters at the start or at the end of two texts that make them near copies, when they
/// are the same.
const MIN_SHARED_CHARS: usize = 80;

/// One text that the user wrote, which later prompts can show as an example. `Debug` shows only
/// its length and its app, so logs never hold what the user wrote.
#[derive(Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// The text before the caret when the user finished the typing, without its secrets.
    pub text: String,
    /// The app where the user wrote the text, as
    /// [`Surroundings::app_id`](crate::desktop::Surroundings::app_id) gives it.
    pub app_id: String,
    /// The host of the web page where the user wrote the text, if any.
    pub domain: Option<String>,
}

impl fmt::Debug for HistoryEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HistoryEntry")
            .field("chars", &self.text.chars().count())
            .field("app_id", &self.app_id)
            .finish_non_exhaustive()
    }
}

impl HistoryEntry {
    /// The entry of `text`, which the user wrote in the app with `app_id`, on the web page of
    /// `domain` if any, with each secret replaced. `None` when the text is too short.
    pub(crate) fn of(text: &str, app_id: &str, domain: Option<&str>) -> Option<Self> {
        let text = text.trim();
        if text.chars().count() < MIN_KEPT_CHARS {
            return None;
        }
        Some(Self {
            text: end_at_word(&redacted(text), MAX_KEPT_CHARS).to_owned(),
            app_id: app_id.to_owned(),
            domain: domain.map(str::to_owned),
        })
    }

    pub(super) fn place(&self) -> Place<&str> {
        Place::of(&self.app_id, self.domain.as_deref())
    }

    /// Whether the texts start with the same [`MIN_SHARED_CHARS`] characters, or end with them.
    pub(super) fn shares_an_edge_with(&self, other: &Self) -> bool {
        let is_long = |entry: &Self| entry.text.chars().count() >= MIN_SHARED_CHARS;
        is_long(self)
            && is_long(other)
            && (self
                .text
                .chars()
                .take(MIN_SHARED_CHARS)
                .eq(other.text.chars().take(MIN_SHARED_CHARS))
                || last_chars(&self.text, MIN_SHARED_CHARS) == last_chars(&other.text, MIN_SHARED_CHARS))
    }

    pub(super) fn overlaps(&self, other: &Self) -> bool {
        self.text.contains(&other.text) || other.text.contains(&self.text)
    }
}

/// Returns the end of `text` that has at most `count` characters and starts with a word.
fn end_at_word(text: &str, count: usize) -> &str {
    from_word(text, last_chars(text, count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{entry, mail};

    #[test]
    fn debug_of_an_entry_hides_the_text() {
        let debug = format!("{:?}", mail("Best regards, Ana"));

        assert_eq!(
            debug,
            r#"HistoryEntry { chars: 17, app_id: "com.apple.mail", .. }"#
        );
    }

    #[test]
    fn entry_keeps_the_trimmed_text_and_the_place() {
        let kept = HistoryEntry::of(
            "  Thanks for the update, I will send the file today.\n",
            "com.google.Chrome",
            Some("mail.google.com"),
        );

        assert_eq!(
            kept,
            Some(entry(
                "Thanks for the update, I will send the file today.",
                "com.google.Chrome",
                Some("mail.google.com")
            ))
        );
    }

    #[test]
    fn short_text_gives_no_entry() {
        let short = "a".repeat(MIN_KEPT_CHARS - 1);

        let kept = HistoryEntry::of(&format!("  {short}  "), "com.apple.mail", None);

        assert_eq!(kept, None);
    }

    #[test]
    fn long_entry_keeps_the_end_of_its_text_from_a_word() {
        let text = format!("start {}", "word ".repeat(250));

        let kept = HistoryEntry::of(&text, "com.apple.mail", None).unwrap();

        assert_eq!(kept.text, "word ".repeat(200).trim_end());
    }

    #[test]
    fn entry_has_its_secrets_redacted() {
        let text = "Here is the key for the staging bot: sk-proj4Hk2LmQ9xT. Do not share it with anyone.";

        let kept = HistoryEntry::of(text, "com.apple.mail", None).unwrap();

        assert_eq!(
            kept.text,
            "Here is the key for the staging bot: [redacted]. Do not share it with anyone."
        );
    }
}
