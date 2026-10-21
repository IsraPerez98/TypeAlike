//! The earlier texts that a prompt shows as examples: the texts that match the topic of the field.

use std::collections::HashMap;

use super::entry::{HistoryEntry, MIN_KEPT_CHARS};
use super::store::SessionTexts;
use super::words::{is_topic_word, words};
use crate::desktop::{FieldText, ScreenText, Surroundings};
use crate::excerpt::{estimated_tokens, first_tokens, from_word, last_tokens, start_at_word};
use crate::inference::TokenCount;
use crate::place::{self, Format};

/// The examples that the newest texts of a small place fill up to: the same domain in a browser,
/// else the same app.
const RECENT_IN_PLACE: usize = 2;
/// The most texts of a small place. In a larger place, the newest texts are seldom on the topic
/// of the field, so only the matching texts show.
const MAX_SMALL_PLACE_TEXTS: usize = 3;
/// The newest texts of other places with the same format that a prompt shows, when the place has
/// no text.
const RECENT_IN_FORMAT: usize = 1;
/// The fewest topic words that a text of another place shares with the field, for an example. A
/// text of another place often has another topic or style, which harms more than no example.
const MIN_WIDE_WORDS: usize = 3;
/// The most estimated tokens of an example.
const EXAMPLE_TOKENS: TokenCount = TokenCount(160);
/// The estimated tokens of the start of a text longer than [`EXAMPLE_TOKENS`], which its example
/// keeps before its end: the greeting of a long text.
const EXAMPLE_START_TOKENS: TokenCount = TokenCount(33);

/// Returns the texts of `texts`, oldest first, that a prompt in `surroundings` shows as examples:
/// the `matching` best matching texts of the same place. In a place with at most
/// [`MAX_SMALL_PLACE_TEXTS`] texts, its newest texts fill the examples up to [`RECENT_IN_PLACE`].
/// The best matching texts of other places with at least [`MIN_WIDE_WORDS`] topic words fill the
/// rest of the `matching` examples. A place without texts gets the newest texts of other places
/// with the same format in the free examples. A text matches when it shares rare words with the
/// text near the caret, the window, the labels of the field and the `screen`. Each example keeps
/// the start and the end of its text. A text that the field holds now is no example, and two
/// examples never hold the same text.
pub(crate) fn examples(
    texts: &SessionTexts<'_>,
    surroundings: &Surroundings,
    screen: Option<&ScreenText>,
    matching: usize,
) -> Vec<String> {
    let field = surroundings.field_text.as_ref();
    let in_place = newest_not_in_field(&texts.local, field);
    let elsewhere = newest_not_in_field(&texts.wide, field);
    let pool = texts.local.iter().chain(&texts.wide).map(|&(_, entry)| entry);
    let topics = Topics::of(pool, surroundings, screen);
    let mut picked = Vec::new();
    pick(&mut picked, &topics.best_first(&in_place, 1), matching);
    if in_place.len() <= MAX_SMALL_PLACE_TEXTS {
        let missing = RECENT_IN_PLACE.saturating_sub(picked.len());
        pick(&mut picked, &in_place, missing);
    }
    let missing = matching.saturating_sub(picked.len());
    pick(
        &mut picked,
        &topics.best_first(&elsewhere, MIN_WIDE_WORDS),
        missing,
    );
    if in_place.is_empty() {
        let format = Format::of(surroundings).filter(|format| *format != Format::WebPage);
        let same_format = elsewhere
            .iter()
            .copied()
            .filter(|(_, entry)| {
                format.is_some() && Format::of_place(Some(&entry.app_id), entry.domain.as_deref()) == format
            })
            .collect::<Vec<_>>();
        let missing = matching.saturating_sub(picked.len());
        pick(&mut picked, &same_format, RECENT_IN_FORMAT.min(missing));
    }
    picked.sort_by_key(|&(index, _)| index);
    picked
        .into_iter()
        .map(|(_, entry)| example_of(&entry.text))
        .collect()
}

/// Returns `candidates`, newest first, without the texts that are copies of `field`.
fn newest_not_in_field<'entries>(
    candidates: &[(usize, &'entries HistoryEntry)],
    field: Option<&FieldText>,
) -> Vec<(usize, &'entries HistoryEntry)> {
    candidates
        .iter()
        .rev()
        .copied()
        .filter(|(_, entry)| !is_copy_of_field(entry, field))
        .collect()
}

/// Calls `visit` with each word of `text` in lowercase.
fn each_lowercase_word(text: &str, mut visit: impl FnMut(&str)) {
    let mut lowercase = String::new();
    for word in words(text) {
        lowercase.clear();
        lowercase.extend(word.chars().flat_map(char::to_lowercase));
        visit(&lowercase);
    }
}

/// The topic words of the text near the caret, each with the number of entries that hold it.
struct Topics {
    entries: usize,
    query: HashMap<String, u32>,
}

impl Topics {
    fn of<'entries>(
        entries: impl IntoIterator<Item = &'entries HistoryEntry>,
        surroundings: &Surroundings,
        screen: Option<&ScreenText>,
    ) -> Self {
        let labels = place::field_labels(surroundings);
        let mut texts = vec![];
        if let Some(field) = &surroundings.field_text {
            texts.extend([field.before_caret.as_str(), field.after_caret.as_str()]);
        }
        texts.extend(surroundings.window_title.as_deref());
        texts.extend(labels.iter().map(String::as_str));
        if let Some(screen) = screen {
            texts.extend(screen.lines.iter().map(|line| line.text.as_str()));
        }
        let mut query = HashMap::new();
        for text in texts {
            each_lowercase_word(text, |word| {
                if is_topic_word(word) {
                    query.entry(word.to_owned()).or_insert(0);
                }
            });
        }
        let mut count = 0;
        for entry in entries {
            count += 1;
            let mut seen = Vec::<String>::new();
            each_lowercase_word(&entry.text, |word| {
                let Some(holding) = query.get_mut(word) else {
                    return;
                };
                if !seen.iter().any(|known| known == word) {
                    seen.push(word.to_owned());
                    *holding += 1;
                }
            });
        }
        Self {
            entries: count,
            query,
        }
    }

    /// Returns the `candidates` that hold at least `min_words` query words, best match first, from
    /// the newest among the same matches.
    fn best_first<'entries>(
        &self,
        candidates: &[(usize, &'entries HistoryEntry)],
        min_words: usize,
    ) -> Vec<(usize, &'entries HistoryEntry)> {
        let mut scored = candidates
            .iter()
            .map(|&candidate| (self.score(&candidate.1.text), candidate))
            .filter(|((_, words), _)| *words >= min_words)
            .collect::<Vec<_>>();
        scored.sort_by(|((left, _), _), ((right, _), _)| right.total_cmp(left));
        scored.into_iter().map(|(_, candidate)| candidate).collect()
    }

    /// The sum of the weights of the query words that `text` holds, and the number of these words.
    /// A word that few entries hold weighs more than a word that many entries hold, but each word
    /// weighs something.
    fn score(&self, text: &str) -> (f32, usize) {
        let mut seen = Vec::<String>::new();
        let mut total = 0.0;
        each_lowercase_word(text, |word| {
            let Some(&holding) = self.query.get(word) else {
                return;
            };
            if seen.iter().any(|known| known == word) {
                return;
            }
            seen.push(word.to_owned());
            let entries = f32::from(u16::try_from(self.entries).unwrap_or(u16::MAX));
            total += (entries / f32::from(u16::try_from(holding).unwrap_or(u16::MAX).max(1))).ln_1p();
        });
        (total, seen.len())
    }
}

/// Returns the example that `text` gives: all of it when it fits in [`EXAMPLE_TOKENS`], else its
/// start and its end with a mark between them, so that the example fits.
fn example_of(text: &str) -> String {
    if estimated_tokens(text) <= EXAMPLE_TOKENS {
        return text.to_owned();
    }
    let end_tokens = TokenCount(EXAMPLE_TOKENS.0 - EXAMPLE_START_TOKENS.0);
    let end = from_word(text, last_tokens(text, end_tokens));
    format!(
        "{} … {end}",
        start_at_word(text, first_tokens(text, EXAMPLE_START_TOKENS))
    )
}

/// Adds to `picked` the first `count` of `candidates` that do not overlap a picked entry.
fn pick<'entries>(
    picked: &mut Vec<(usize, &'entries HistoryEntry)>,
    candidates: &[(usize, &'entries HistoryEntry)],
    count: usize,
) {
    let mut added = 0;
    for &candidate in candidates {
        if added == count {
            return;
        }
        if picked.iter().any(|(_, entry)| entry.overlaps(candidate.1)) {
            continue;
        }
        picked.push(candidate);
        added += 1;
    }
}

/// Whether the field holds the text of `entry`, or the text of `entry` holds the text before the
/// caret of a field that has enough text to save.
fn is_copy_of_field(entry: &HistoryEntry, field: Option<&FieldText>) -> bool {
    let Some(field) = field else {
        return false;
    };
    let before = field.before_caret.trim();
    let whole = format!("{}{}", field.before_caret, field.after_caret);
    whole.contains(entry.text.trim())
        || (before.chars().count() >= MIN_KEPT_CHARS && entry.text.contains(before))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::{Confidence, ScreenLine, ScreenRect};
    use crate::history::store::session_texts;
    use crate::history::test_support::{entry, mail, shared};

    fn place(app_id: &str, domain: Option<&str>, before_caret: &str) -> Surroundings {
        Surroundings {
            app_id: Some(app_id.to_owned()),
            domain: domain.map(str::to_owned),
            field_text: Some(FieldText {
                before_caret: before_caret.to_owned(),
                after_caret: String::new(),
            }),
            ..Surroundings::default()
        }
    }

    fn examples_with(
        entries: &[HistoryEntry],
        surroundings: &Surroundings,
        screen: Option<&ScreenText>,
        matching: usize,
    ) -> Vec<String> {
        let texts = session_texts(entries, surroundings, &shared());
        examples(&texts, surroundings, screen, matching)
    }

    fn examples_in(entries: &[HistoryEntry], surroundings: &Surroundings) -> Vec<String> {
        examples_with(entries, surroundings, None, 2)
    }

    #[test]
    fn text_of_a_web_page_is_no_example_in_its_browser_outside_of_a_page() {
        let entries = [entry(
            "Review of the pull request",
            "com.google.Chrome",
            Some("github.com"),
        )];

        let examples = examples_in(&entries, &place("com.google.Chrome", None, "Hi"));

        assert_eq!(examples, Vec::<&str>::new());
    }

    fn place_with(app_id: &str, before_caret: &str, window_title: Option<&str>) -> Surroundings {
        Surroundings {
            window_title: window_title.map(str::to_owned),
            ..place(app_id, None, before_caret)
        }
    }

    fn bluefin_entries() -> Vec<HistoryEntry> {
        vec![
            mail("Bluefin launch plan: the budget goes to the demo."),
            entry("Text of another app", "com.apple.Notes", None),
            mail("Weekend trip to the lake with the kids, back on Sunday."),
            mail("Bluefin budget review with Dana tomorrow morning."),
        ]
    }

    #[test]
    fn examples_are_the_two_texts_of_the_app_that_match_the_text_near_the_caret_oldest_first() {
        let examples = examples_in(
            &bluefin_entries(),
            &place("com.apple.mail", None, "Update on the Bluefin budget"),
        );

        assert_eq!(
            examples,
            [
                "Bluefin launch plan: the budget goes to the demo.",
                "Bluefin budget review with Dana tomorrow morning."
            ]
        );
    }

    #[test]
    fn examples_are_the_two_newest_texts_of_the_app_when_no_text_matches() {
        let examples = examples_in(&bluefin_entries(), &place("com.apple.mail", None, "Hi Sam"));

        assert_eq!(
            examples,
            [
                "Weekend trip to the lake with the kids, back on Sunday.",
                "Bluefin budget review with Dana tomorrow morning."
            ]
        );
    }

    fn large_mail_place() -> Vec<HistoryEntry> {
        let mut entries = bluefin_entries();
        entries.extend([
            mail("Dinner with Ana and Leo on Friday at eight."),
            mail("The printer on the second floor works again."),
            entry("Text of another mail app", "OUTLOOK.EXE", None),
        ]);
        entries
    }

    #[test]
    fn one_wanted_match_gives_only_the_best_matching_text_of_a_large_place() {
        let surroundings = place("com.apple.mail", None, "Update on the Bluefin budget");

        let examples = examples_with(&large_mail_place(), &surroundings, None, 1);

        assert_eq!(examples, ["Bluefin budget review with Dana tomorrow morning."]);
    }

    #[test]
    fn newest_texts_of_a_small_place_fill_the_second_example() {
        let surroundings = place("com.apple.mail", None, "Update on the Bluefin budget");

        let examples = examples_with(&bluefin_entries(), &surroundings, None, 1);

        assert_eq!(
            examples,
            [
                "Weekend trip to the lake with the kids, back on Sunday.",
                "Bluefin budget review with Dana tomorrow morning."
            ]
        );
    }

    fn notes(text: &str) -> HistoryEntry {
        entry(text, "com.apple.Notes", None)
    }

    const KESTREL_NOTE: &str = "Kestrel portal: carrier sign-in goes through the Pinwheel gateway.";

    #[test]
    fn matching_text_of_another_app_fills_a_free_example() {
        let mut entries = large_mail_place();
        entries.push(notes(KESTREL_NOTE));
        let surroundings = place(
            "com.apple.mail",
            None,
            "Update on the Kestrel portal: carrier sign-in",
        );

        let examples = examples_with(&entries, &surroundings, None, 1);

        assert_eq!(examples, [KESTREL_NOTE]);
    }

    #[test]
    fn matching_text_of_another_app_never_takes_the_place_of_a_text_of_the_place() {
        let mut entries = large_mail_place();
        entries.push(notes(
            "Update: Bluefin budget review, the Bluefin budget goes to the launch demo.",
        ));
        let surroundings = place("com.apple.mail", None, "Update on the Bluefin budget");

        let examples = examples_with(&entries, &surroundings, None, 1);

        assert_eq!(examples, ["Bluefin budget review with Dana tomorrow morning."]);
    }

    #[test]
    fn text_of_another_app_needs_three_topic_words_of_the_field() {
        let mut entries = large_mail_place();
        entries.push(notes("Kestrel portal ideas for the lake trip this week."));
        let surroundings = place(
            "com.apple.mail",
            None,
            "Update on the Kestrel portal: carrier sign-in",
        );

        let examples = examples_with(&entries, &surroundings, None, 1);

        assert_eq!(examples, Vec::<&str>::new());
    }

    #[test]
    fn large_place_without_matching_texts_has_no_examples() {
        let examples = examples_in(&large_mail_place(), &place("com.apple.mail", None, "Hi Sam"));

        assert_eq!(examples, Vec::<&str>::new());
    }

    #[test]
    fn title_of_the_window_tells_the_topic_and_the_newest_text_fills_the_second_place() {
        let surroundings = place_with("com.apple.mail", "Hi Sam", Some("Launch"));

        let examples = examples_in(&bluefin_entries(), &surroundings);

        assert_eq!(
            examples,
            [
                "Bluefin launch plan: the budget goes to the demo.",
                "Bluefin budget review with Dana tomorrow morning."
            ]
        );
    }

    #[test]
    fn text_on_the_screen_tells_the_topic_and_the_newest_text_fills_the_second_place() {
        let screen = ScreenText {
            lines: vec![ScreenLine {
                text: "Launch".to_owned(),
                confidence: Confidence::CERTAIN,
                frame: ScreenRect::default(),
            }],
            ..ScreenText::default()
        };

        let examples = examples_with(
            &bluefin_entries(),
            &place("com.apple.mail", None, "Hi Sam"),
            Some(&screen),
            2,
        );

        assert_eq!(
            examples,
            [
                "Bluefin launch plan: the budget goes to the demo.",
                "Bluefin budget review with Dana tomorrow morning."
            ]
        );
    }

    #[test]
    fn rare_word_counts_more_than_a_word_of_many_texts() {
        let entries = [
            mail("Meeting notes on the roof"),
            mail("Meeting notes on the garden"),
            mail("Meeting notes on the garage"),
            mail("A zebra at the zoo"),
            entry("Text of another app", "com.apple.Notes", None),
        ];

        let examples = examples_in(&entries, &place("com.apple.mail", None, "meeting zebra"));

        assert_eq!(examples, ["Meeting notes on the garage", "A zebra at the zoo"]);
    }

    #[test]
    fn examples_in_a_browser_come_from_the_same_domain_in_any_browser() {
        let entries = [
            entry("Review from Safari", "com.apple.Safari", Some("github.com")),
            entry(
                "Mail in the same browser",
                "com.google.Chrome",
                Some("mail.google.com"),
            ),
        ];

        let examples = examples_in(&entries, &place("com.google.Chrome", Some("github.com"), "LGTM"));

        assert_eq!(examples, ["Review from Safari"]);
    }

    #[test]
    fn example_keeps_the_start_and_the_end_of_a_long_text_from_a_word() {
        let entries = [mail(&format!("Hola equipo, {}fin", "word ".repeat(120)))];

        let examples = examples_in(&entries, &place("com.apple.mail", None, ""));

        assert_eq!(
            examples,
            [format!(
                "Hola equipo, {} … {}fin",
                "word ".repeat(17).trim_end(),
                "word ".repeat(75)
            )]
        );
    }

    #[test]
    fn example_of_a_text_with_wide_characters_keeps_fewer_characters() {
        let entries = [mail(&format!("Hola equipo, {}fin", "ñu ".repeat(200)))];

        let examples = examples_in(&entries, &place("com.apple.mail", None, ""));

        assert_eq!(
            examples,
            [format!(
                "Hola equipo, {} … {}fin",
                "ñu ".repeat(17).trim_end(),
                "ñu ".repeat(75)
            )]
        );
    }

    #[test]
    fn example_keeps_all_of_a_text_that_fits() {
        let text = format!("Hola equipo, {}fin", "word ".repeat(90));
        let entries = [mail(&text)];

        let examples = examples_in(&entries, &place("com.apple.mail", None, ""));

        assert_eq!(examples, [text]);
    }

    #[test]
    fn place_without_texts_gets_the_newest_text_of_another_app_with_the_same_format() {
        let entries = [
            entry("Chat text of the writer", "net.whatsapp.WhatsApp", None),
            entry("Mail text of the writer", "com.apple.mail", None),
            entry("Newest chat text of the writer", "com.hnc.Discord", None),
            entry("Newest mail text of the writer", "OUTLOOK.EXE", None),
        ];

        let examples = examples_in(&entries, &place("com.tinyspeck.slackmacgap", None, "Hi"));

        assert_eq!(examples, ["Newest chat text of the writer"]);
    }

    #[test]
    fn place_without_texts_gets_no_text_of_another_format() {
        let entries = [entry("Mail text of the writer", "com.apple.mail", None)];

        let examples = examples_in(&entries, &place("com.tinyspeck.slackmacgap", None, "Hi"));

        assert_eq!(examples, Vec::<&str>::new());
    }

    #[test]
    fn web_page_without_texts_gets_no_text_of_another_web_page() {
        let entries = [entry(
            "Review from Safari",
            "com.apple.Safari",
            Some("github.com"),
        )];

        let examples = examples_in(
            &entries,
            &place("com.google.Chrome", Some("news.example.com"), "Hi"),
        );

        assert_eq!(examples, Vec::<&str>::new());
    }

    #[test]
    fn text_that_the_field_holds_is_no_example() {
        let earlier = "Dear Ana,\n\nThe report is ready and the numbers look good.";
        let entries = [mail(earlier), mail("Other text.")];

        let examples = examples_in(
            &entries,
            &place("com.apple.mail", None, &format!("{earlier}\n\nBest,")),
        );

        assert_eq!(examples, ["Other text."]);
    }

    #[test]
    fn text_that_holds_a_long_field_is_no_example() {
        let field = "Dear Ana,\n\nThe report is ready and the numbers look";
        let entries = [mail(&format!("{field} good. Best, Tom")), mail("Other text.")];

        let examples = examples_in(&entries, &place("com.apple.mail", None, field));

        assert_eq!(examples, ["Other text."]);
    }

    #[test]
    fn two_examples_never_hold_the_same_text() {
        let entries = [
            mail("Dear Ana, the report is ready."),
            mail("Dear Ana, the report"),
        ];

        let examples = examples_in(&entries, &place("com.apple.mail", None, "Hi"));

        assert_eq!(examples, ["Dear Ana, the report"]);
    }

    #[test]
    fn place_without_entries_has_no_examples() {
        let entries = [entry("Text of another app", "com.apple.Notes", None)];

        let examples = examples_in(&entries, &place("com.apple.mail", None, "Hi"));

        assert_eq!(examples, Vec::<&str>::new());
    }
}
