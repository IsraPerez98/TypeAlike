//! One kept text of the user: its trim, its redaction, its cut to the end and its overlap checks.

use std::fmt;

use super::redaction::redacted;
use crate::excerpt::{from_word, last_chars};
use crate::place::Place;

/// The fewest characters of a text that GhostType saves. The text also needs a space between two
/// words.
const MIN_KEPT_CHARS: usize = 5;
/// The most characters of a text that GhostType saves, from its end.
const MAX_KEPT_CHARS: usize = 1000;

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
    /// `domain` if any, with each secret replaced. It keeps at most the last [`MAX_KEPT_CHARS`]
    /// characters, from the start of a word. `None` when the text is shorter than
    /// [`MIN_KEPT_CHARS`] or has only one word.
    pub(crate) fn of(text: &str, app_id: &str, domain: Option<&str>) -> Option<Self> {
        let text = text.trim();
        let is_long_enough = text.chars().count() >= MIN_KEPT_CHARS && text.contains(char::is_whitespace);
        if !is_long_enough {
            return None;
        }
        let without_secrets = redacted(text);
        Some(Self {
            text: from_word(&without_secrets, last_chars(&without_secrets, MAX_KEPT_CHARS)).to_owned(),
            app_id: app_id.to_owned(),
            domain: domain.map(str::to_owned),
        })
    }

    pub(super) fn place(&self) -> Place<&str> {
        Place::of(&self.app_id, self.domain.as_deref())
    }

    pub(super) fn overlaps(&self, other: &Self) -> bool {
        self.text.contains(&other.text) || other.text.contains(&self.text)
    }
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
    fn text_of_one_word_or_of_less_than_five_characters_gives_no_entry() {
        let cases = ["  Wonderful  ", "Hi A", " ok "];

        let kept = cases.map(|text| HistoryEntry::of(text, "com.apple.mail", None));

        assert_eq!(kept, [None, None, None]);
    }

    #[test]
    fn text_of_two_words_and_five_characters_gives_an_entry() {
        let cases = ["  Hi Al  ", "Hi\nAl"];

        let kept = cases.map(|text| HistoryEntry::of(text, "com.apple.mail", None));

        assert_eq!(kept, [Some(mail("Hi Al")), Some(mail("Hi\nAl"))]);
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
