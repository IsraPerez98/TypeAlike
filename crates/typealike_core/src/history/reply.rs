//! The words of the message that a text answers: the messages above the field of a chat or an
//! email app. An earlier text whose message shares words with the screen now can be an example.

use super::entry::HistoryEntry;
use super::redaction::{REDACTED, redacted};
use super::words::{is_distinctive_word, words};
use crate::desktop::{ScreenText, Surroundings};
use crate::excerpt::one_line;
use crate::place::Place;
use crate::screen::{self, LineScope};

/// The most words of the message that a text answers. The words nearest the reply tell most
/// about it.
const MAX_REPLY_WORDS: usize = 10;
/// The fewest characters of a screen line that counts as a part of an earlier text of the user. A
/// shorter line counts only as a whole text, because a short message such as "see you" is also a
/// part of the texts of other people.
const MIN_OWN_PART_CHARS: usize = 20;

/// Returns the distinctive words of `message` in lowercase, each one time, the nearest to its end
/// first. It keeps at most 10 words, and no word of a secret.
#[must_use]
pub fn reply_words_of(message: &str) -> Vec<String> {
    let text = redacted(message).replace(REDACTED, " ");
    let mut lowercase_words = words(&text)
        .map(|word| word.chars().flat_map(char::to_lowercase).collect::<String>())
        .filter(|word| is_distinctive_word(word))
        .collect::<Vec<_>>();
    lowercase_words.reverse();
    let mut kept = Vec::new();
    for word in lowercase_words {
        if kept.len() == MAX_REPLY_WORDS {
            break;
        }
        if !kept.contains(&word) {
            kept.push(word);
        }
    }
    kept
}

/// Returns the words of the message that the user answers in the field of `surroundings`: the
/// lines of `screen` above the field after the last line of an earlier text of the same place in
/// `entries`. A field outside of a chat or an email app answers no message.
pub(crate) fn reply_words(
    screen: &ScreenText,
    surroundings: &Surroundings,
    entries: &[HistoryEntry],
) -> Vec<String> {
    if LineScope::of(surroundings) != LineScope::Column {
        return Vec::new();
    }
    let place = Place::of_field(surroundings);
    let own_texts = entries
        .iter()
        .filter(|entry| Some(entry.place()) == place)
        .map(|entry| one_line(&entry.text))
        .collect::<Vec<_>>();
    let lines = screen::message_lines_above(screen, surroundings.field_text.as_ref());
    let message_start = lines
        .iter()
        .rposition(|line| is_own_line(line, &own_texts))
        .map_or(0, |own_line| own_line + 1);
    reply_words_of(&lines[message_start..].join("\n"))
}

/// Whether `line` is one of `own_texts`, or a long part of one.
fn is_own_line(line: &str, own_texts: &[String]) -> bool {
    let is_long = line.chars().count() >= MIN_OWN_PART_CHARS;
    own_texts
        .iter()
        .any(|text| text == line || (is_long && text.contains(line)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::{Confidence, FieldText, ScreenLine, ScreenRect};
    use crate::history::test_support::entry;

    const CHAT_APP: &str = "net.whatsapp.WhatsApp";

    fn words(expected: &[&str]) -> Vec<String> {
        expected.iter().map(|&word| word.to_owned()).collect()
    }

    /// A screen of a chat with `lines` in the column of the field, the last one just above it.
    fn chat_screen(lines: &[&str]) -> ScreenText {
        let rows = lines.len();
        ScreenText {
            lines: lines
                .iter()
                .zip((1..=rows).rev())
                .map(|(&text, row)| {
                    let top = 900 - 40 * i32::try_from(row).expect("few rows");
                    ScreenLine {
                        text: text.to_owned(),
                        confidence: Confidence::CERTAIN,
                        frame: ScreenRect {
                            left: 300,
                            top,
                            right: 900,
                            bottom: top + 30,
                        },
                    }
                })
                .collect(),
            caret: Some(ScreenRect {
                left: 320,
                top: 920,
                right: 321,
                bottom: 940,
            }),
            field: ScreenRect {
                left: 300,
                top: 900,
                right: 1400,
                bottom: 960,
            },
        }
    }

    fn field_of(app_id: &str) -> Surroundings {
        Surroundings {
            app_id: Some(app_id.to_owned()),
            field_text: Some(FieldText::default()),
            ..Surroundings::default()
        }
    }

    #[test]
    fn reply_words_are_the_distinctive_words_of_the_message_nearest_its_end_first() {
        let reply_words = reply_words_of("Standup in 5? Same room as yesterday, same time");

        assert_eq!(
            reply_words,
            words(&["time", "same", "yesterday", "room", "standup"])
        );
    }

    #[test]
    fn reply_words_keep_at_most_ten_words() {
        let message = "alpha bravo charlie delta echo foxtrot golf hotel india juliett kilo lima";

        let reply_words = reply_words_of(message);

        assert_eq!(
            reply_words,
            words(&[
                "lima", "kilo", "juliett", "india", "hotel", "golf", "foxtrot", "echo", "delta", "charlie"
            ])
        );
    }

    #[test]
    fn reply_words_leave_out_secrets() {
        let reply_words = reply_words_of("Use the key sk-abcdefghijklmnop please");

        assert_eq!(reply_words, words(&["please", "key", "use"]));
    }

    #[test]
    fn reply_words_of_a_chat_come_from_the_messages_after_the_last_line_of_the_user() {
        let own = entry("I will bring the cake and the balloons tomorrow", CHAT_APP, None);
        let screen = chat_screen(&[
            "Can you bring napkins?",
            "I will bring the cake and the balloons tomorrow",
            "Pili 15:02",
            "Who picks up the cake on Saturday?",
        ]);

        let reply_words = reply_words(&screen, &field_of(CHAT_APP), &[own]);

        assert_eq!(reply_words, words(&["saturday", "cake", "picks"]));
    }

    #[test]
    fn short_line_is_a_line_of_the_user_only_as_a_whole_text() {
        let own = entry("Sure, see you", CHAT_APP, None);
        let screen = chat_screen(&["Lunch at noon?", "see you"]);

        let reply_words = reply_words(&screen, &field_of(CHAT_APP), &[own]);

        assert_eq!(reply_words, words(&["see", "noon", "lunch"]));
    }

    #[test]
    fn line_of_the_user_in_another_app_does_not_end_the_message() {
        let own = entry(
            "I will bring the cake and the balloons tomorrow",
            "com.apple.Notes",
            None,
        );
        let screen = chat_screen(&["I will bring the cake and the balloons tomorrow", "Great"]);

        let reply_words = reply_words(&screen, &field_of(CHAT_APP), &[own]);

        assert_eq!(
            reply_words,
            words(&["great", "tomorrow", "balloons", "cake", "bring"])
        );
    }

    #[test]
    fn field_outside_of_a_chat_or_an_email_app_has_no_reply_words() {
        let screen = chat_screen(&["Standup in 5?"]);

        let reply_words = reply_words(&screen, &field_of("com.apple.Notes"), &[]);

        assert_eq!(reply_words, Vec::<String>::new());
    }
}
