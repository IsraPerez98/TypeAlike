//! The earlier writing of the user: the texts that GhostType saves, the secrets that it removes
//! from them, and the texts that a prompt shows as examples.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::ops::Range;

use crate::desktop::{FieldText, Surroundings};
use crate::inference::WordBias;
use crate::prompt::{from_word, last_chars};
use crate::settings::WordChoice;

/// The most texts that GhostType keeps. The oldest text goes first.
const MAX_ENTRIES: usize = 2000;
/// The fewest characters of a text that GhostType saves.
const MIN_KEPT_CHARS: usize = 40;
/// The most characters of a text that GhostType saves, from its end.
const MAX_KEPT_CHARS: usize = 1000;
/// The newest texts of the same place that a prompt shows: the same domain in a browser, else the
/// same app.
const RECENT_IN_PLACE: usize = 1;
/// The longest texts of the same place that a prompt shows.
const LONGEST_IN_PLACE: usize = 1;
/// The newest texts of other places that a prompt shows.
const RECENT_ACROSS_APPS: usize = 0;
/// The most characters of one example, from the end of its text.
const EXAMPLE_CHARS: usize = 320;
const REDACTED: &str = "[redacted]";
/// The starts of the API tokens that GhostType knows. A token needs at least
/// [`MIN_SECRET_TAIL`] more characters.
const SECRET_PREFIXES: [&str; 8] = [
    "sk-",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "AKIA",
];
const MIN_SECRET_TAIL: usize = 8;
/// The fewest characters of a token of letters and digits that GhostType takes for a secret.
const MIN_MIXED_TOKEN_CHARS: usize = 20;
/// The digits of a card number.
const CARD_DIGITS: Range<usize> = 13..20;
/// The most common words of English and Spanish. Each writer writes them often, so a preference
/// for them only moves a suggestion away from the rarer words of the writer.
const COMMON_WORDS: [&str; 119] = [
    "the", "and", "you", "for", "that", "this", "with", "have", "but", "are", "not", "will", "can", "just",
    "about", "what", "was", "were", "they", "them", "their", "there", "from", "your", "our", "has", "had",
    "would", "could", "should", "all", "any", "one", "out", "get", "got", "like", "some", "more", "also",
    "then", "than", "when", "which", "who", "how", "its", "it's", "i'm", "don't", "can't", "we're", "been",
    "into", "over", "only", "other", "here", "now", "very", "much", "does", "did", "let", "know", "que",
    "los", "las", "por", "para", "con", "una", "uno", "pero", "como", "está", "esta", "este", "esto", "del",
    "más", "mas", "muy", "hay", "ser", "son", "fue", "era", "les", "nos", "sus", "mis", "tus", "todo",
    "todos", "también", "cuando", "donde", "porque", "sobre", "entre", "hasta", "desde", "sin", "algo",
    "bien", "hoy", "ahora", "solo", "sólo", "tengo", "tiene", "hace", "eso", "ese", "esa", "qué", "cómo",
    "mucho",
];
/// The fewest times that the writer wrote a word that suggestions prefer.
const MIN_WORD_COUNT: u32 = 3;
/// The fewest letters of a word that suggestions prefer.
const MIN_WORD_CHARS: usize = 3;
/// The times of a word that give it the full strength of the word choice.
const FULL_WEIGHT_COUNT: u16 = 20;

/// One text that the user wrote, which later prompts can show as an example. `Debug` shows only
/// its length and its app, so logs never hold what the user wrote.
#[derive(Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// The text before the caret when the user finished the typing, without its secrets.
    pub text: String,
    /// The app where the user wrote the text, as [`Surroundings::app_id`] gives it.
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

    fn is_in_place_of(&self, surroundings: &Surroundings) -> bool {
        match &surroundings.domain {
            Some(domain) => self.domain.as_ref() == Some(domain),
            None => surroundings.app_id.as_ref() == Some(&self.app_id),
        }
    }

    fn is_same_place(&self, other: &Self) -> bool {
        self.app_id == other.app_id && self.domain == other.domain
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.text.contains(&other.text) || other.text.contains(&self.text)
    }
}

/// Keeps the earlier writing of the user between runs, on this computer only.
pub trait WritingHistory {
    /// Returns the kept entries, oldest first. Returns none when the store has none or cannot read
    /// them. The store reports its own failures.
    fn load(&mut self) -> Vec<HistoryEntry>;

    /// Keeps `entries`, oldest first, in place of all the kept entries. The store reports its own
    /// failures.
    fn save(&mut self, entries: &[HistoryEntry]);
}

/// The secret key that locks the file of the earlier writing. `Debug` does not show it.
#[derive(Clone, PartialEq, Eq)]
pub struct HistoryKey(pub [u8; 32]);

impl fmt::Debug for HistoryKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HistoryKey(..)")
    }
}

/// Keeps the [`HistoryKey`] in the secure store of the OS, apart from the file that it locks.
pub trait KeyVault {
    /// Returns the kept key, or `None` when the vault has no key yet.
    ///
    /// # Errors
    ///
    /// Fails when the vault cannot be read, for example when the user does not allow it.
    fn load(&mut self) -> io::Result<Option<HistoryKey>>;

    /// Keeps `key` as the key of the vault.
    ///
    /// # Errors
    ///
    /// Fails when the vault cannot keep the key.
    fn store(&mut self, key: &HistoryKey) -> io::Result<()>;
}

/// Adds `entry` as the newest of `entries`. An older entry of the same place whose text the new
/// text holds goes, and the new entry does not come when an older one holds its text. The oldest
/// entries go when there are more than [`MAX_ENTRIES`].
pub(crate) fn add(entries: &mut Vec<HistoryEntry>, entry: HistoryEntry) {
    let is_known = entries
        .iter()
        .any(|known| known.is_same_place(&entry) && known.text.contains(&entry.text));
    if is_known {
        return;
    }
    entries.retain(|known| !(known.is_same_place(&entry) && entry.text.contains(&known.text)));
    entries.push(entry);
    let excess = entries.len().saturating_sub(MAX_ENTRIES);
    entries.drain(..excess);
}

/// Returns the texts of `entries`, oldest first, that a prompt in `surroundings` shows as
/// examples: the newest and the longest texts of the same place, then the newest texts of other
/// places. Each example keeps the end of its text, from the start of a word. A text that the field
/// holds now is no example, and two examples never hold the same text.
pub(crate) fn examples<'entries>(
    entries: &'entries [HistoryEntry],
    surroundings: &Surroundings,
) -> Vec<&'entries str> {
    let candidates = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !is_copy_of_field(entry, surroundings.field_text.as_ref()))
        .rev();
    let (mut in_place, elsewhere): (Vec<_>, Vec<_>) =
        candidates.partition(|(_, entry)| entry.is_in_place_of(surroundings));
    let mut picked = Vec::new();
    pick(&mut picked, &in_place, RECENT_IN_PLACE);
    in_place.sort_by_key(|(_, entry)| std::cmp::Reverse(entry.text.chars().count()));
    pick(&mut picked, &in_place, LONGEST_IN_PLACE);
    pick(&mut picked, &elsewhere, RECENT_ACROSS_APPS);
    picked.sort_by_key(|&(index, _)| index);
    picked
        .into_iter()
        .map(|(_, entry)| end_at_word(&entry.text, EXAMPLE_CHARS))
        .collect()
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

/// Returns the end of `text` that has at most `count` characters and starts with a word.
fn end_at_word(text: &str, count: usize) -> &str {
    from_word(text, last_chars(text, count))
}

/// Returns the preference of `choice` for the words of `entries`, with `version`, or `None` when
/// the choice is off. A word is a run of letters and apostrophes in lowercase with at least
/// [`MIN_WORD_CHARS`] letters, which the entries hold at least [`MIN_WORD_COUNT`] times and which
/// is not one of the [`COMMON_WORDS`].
pub(crate) fn word_bias(entries: &[HistoryEntry], choice: WordChoice, version: u64) -> Option<WordBias> {
    let mut counts = HashMap::<String, u32>::new();
    for entry in entries {
        for word in words(&entry.text.replace(REDACTED, " ")) {
            *counts.entry(word).or_default() += 1;
        }
    }
    word_bias_of_counts(counts, choice, version)
}

/// Returns the preference of `choice` for the words of `counts`, each with the number of times
/// that the writer wrote it, or `None` when the choice is off. A word needs at least 3 letters and
/// 3 times, and the most common words of English and Spanish get no preference. A word that comes
/// 20 times or more gets the full strength of the choice.
#[must_use]
pub fn word_bias_of_counts(
    counts: impl IntoIterator<Item = (String, u32)>,
    choice: WordChoice,
    version: u64,
) -> Option<WordBias> {
    let strength = match choice {
        WordChoice::Off => return None,
        WordChoice::Light => 0.5,
        WordChoice::Medium => 1.0,
        WordChoice::Strong => 2.0,
    };
    let full_weight = f32::from(FULL_WEIGHT_COUNT).ln();
    let mut words = counts
        .into_iter()
        .filter(|(word, count)| {
            *count >= MIN_WORD_COUNT
                && word.chars().count() >= MIN_WORD_CHARS
                && !COMMON_WORDS.contains(&word.as_str())
        })
        .map(|(word, count)| {
            let capped = u16::try_from(count).unwrap_or(u16::MAX).min(FULL_WEIGHT_COUNT);
            (word, strength * f32::from(capped).ln() / full_weight)
        })
        .collect::<Vec<_>>();
    words.sort_by(|left, right| left.0.cmp(&right.0));
    Some(WordBias {
        version,
        words: words.into(),
    })
}

/// Returns the words of `text` in lowercase: its runs of letters and apostrophes.
fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| !(character.is_alphabetic() || matches!(character, '\'' | '’')))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

/// Returns `text` with each secret replaced by [`REDACTED`]: private keys, API tokens, long tokens
/// that mix letters and digits, and card numbers.
pub(crate) fn redacted(text: &str) -> String {
    let text = replaced(text, &private_key_spans(text));
    let secret_tokens = token_spans(&text)
        .into_iter()
        .filter(|span| is_secret_token(&text[span.clone()]))
        .collect::<Vec<_>>();
    let text = replaced(&text, &secret_tokens);
    replaced(&text, &card_number_spans(&text))
}

/// Returns `text` with each of `spans`, which do not overlap and go from the start to the end,
/// replaced by [`REDACTED`].
fn replaced(text: &str, spans: &[Range<usize>]) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut from = 0;
    for span in spans {
        kept.push_str(&text[from..span.start]);
        kept.push_str(REDACTED);
        from = span.end;
    }
    kept.push_str(&text[from..]);
    kept
}

/// The spans of the private keys in PEM form. A key without its end line goes to the end of the
/// text.
fn private_key_spans(text: &str) -> Vec<Range<usize>> {
    const BEGIN: &str = "-----BEGIN ";
    const END: &str = "-----END ";
    const DASHES: &str = "-----";
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find(BEGIN) {
        let start = from + offset;
        let after_begin = start + BEGIN.len();
        let header_end = text[after_begin..]
            .find(DASHES)
            .map_or(text.len(), |offset| after_begin + offset + DASHES.len());
        from = header_end;
        if !text[start..header_end].contains("PRIVATE KEY") {
            continue;
        }
        let end = text[header_end..]
            .find(END)
            .map(|offset| header_end + offset + END.len())
            .and_then(|after_end| {
                text[after_end..]
                    .find(DASHES)
                    .map(|offset| after_end + offset + DASHES.len())
            })
            .unwrap_or(text.len());
        spans.push(start..end);
        from = end;
    }
    spans
}

/// The spans of the runs of characters that a token can hold.
fn token_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut start = None;
    for (index, character) in text.char_indices() {
        match (is_token_character(character), start) {
            (true, None) => start = Some(index),
            (false, Some(begin)) => {
                spans.push(begin..index);
                start = None;
            }
            _ => {}
        }
    }
    spans.extend(start.map(|begin| begin..text.len()));
    spans
}

fn is_token_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '+' | '=')
}

fn is_secret_token(token: &str) -> bool {
    let has_secret_prefix = SECRET_PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix) && token.len() >= prefix.len() + MIN_SECRET_TAIL);
    let is_slack_token = token.len() >= 5 + MIN_SECRET_TAIL
        && token.starts_with("xox")
        && token[3..].starts_with(|character: char| character.is_ascii_alphabetic())
        && token[4..].starts_with('-');
    let is_mixed = token.len() >= MIN_MIXED_TOKEN_CHARS
        && token.contains(|character: char| character.is_ascii_alphabetic())
        && token.contains(|character: char| character.is_ascii_digit());
    has_secret_prefix || is_slack_token || is_mixed
}

/// The spans of the runs of 13 to 19 digits, in groups that one space or dash divides.
fn card_number_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut run: Option<(usize, usize, usize)> = None;
    let mut characters = text.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        let next_is_digit = characters.peek().is_some_and(|&(_, next)| next.is_ascii_digit());
        run = match (character.is_ascii_digit(), run) {
            (true, None) => Some((index, index + 1, 1)),
            (true, Some((start, _, digits))) => Some((start, index + 1, digits + 1)),
            (false, Some(open)) if matches!(character, ' ' | '-') && next_is_digit => Some(open),
            (false, open) => {
                spans.extend(
                    open.filter(|&(_, _, digits)| CARD_DIGITS.contains(&digits))
                        .map(|(start, end, _)| start..end),
                );
                None
            }
        };
    }
    spans.extend(
        run.filter(|&(_, _, digits)| CARD_DIGITS.contains(&digits))
            .map(|(start, end, _)| start..end),
    );
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str, app_id: &str, domain: Option<&str>) -> HistoryEntry {
        HistoryEntry {
            text: text.to_owned(),
            app_id: app_id.to_owned(),
            domain: domain.map(str::to_owned),
        }
    }

    fn mail(text: &str) -> HistoryEntry {
        entry(text, "com.apple.mail", None)
    }

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

    fn words_of(bias: Option<WordBias>) -> Option<Vec<(String, f32)>> {
        bias.map(|bias| bias.words.to_vec())
    }

    #[test]
    fn word_bias_holds_the_words_of_three_letters_that_come_three_times() {
        let entries = [mail("Cheers mate, ok ok ok. Cheers"), mail("cheers MATE! Mate's")];

        let bias = word_bias(&entries, WordChoice::Medium, 1);

        assert_eq!(
            words_of(bias),
            Some(vec![("cheers".to_owned(), 3.0_f32.ln() / 20.0_f32.ln())])
        );
    }

    #[test]
    fn word_that_comes_often_gets_the_full_strength_of_the_choice() {
        let entries = [mail(&"reckon ".repeat(25))];
        let choices = [WordChoice::Light, WordChoice::Medium, WordChoice::Strong];

        let biases = choices.map(|choice| words_of(word_bias(&entries, choice, 1)));

        assert_eq!(
            biases,
            [0.5, 1.0, 2.0].map(|strength| Some(vec![("reckon".to_owned(), strength)]))
        );
    }

    #[test]
    fn common_words_get_no_preference() {
        let entries = [mail(&"the reckon que ".repeat(25))];

        let bias = word_bias(&entries, WordChoice::Medium, 1);

        assert_eq!(words_of(bias), Some(vec![("reckon".to_owned(), 1.0)]));
    }

    #[test]
    fn word_choice_off_gives_no_bias() {
        let entries = [mail(&"reckon ".repeat(25))];

        let bias = word_bias(&entries, WordChoice::Off, 1);

        assert_eq!(bias, None);
    }

    #[test]
    fn redacted_secrets_are_not_words_of_the_writer() {
        let entries = [mail("key [redacted] and [redacted] or [redacted]")];

        let bias = word_bias(&entries, WordChoice::Medium, 1);

        assert_eq!(words_of(bias), Some(Vec::new()));
    }

    #[test]
    fn debug_of_a_key_hides_the_key() {
        let debug = format!("{:?}", HistoryKey([7; 32]));

        assert_eq!(debug, "HistoryKey(..)");
    }

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

    #[test]
    fn redaction_replaces_api_tokens() {
        let cases = [
            ("key sk-abcdefgh1 ok", "key [redacted] ok"),
            ("ghp_A1b2C3d4E5f6", "[redacted]"),
            ("github_pat_11ABCDEFG", "[redacted]"),
            ("token xoxb-1234-abcd-5678", "token [redacted]"),
            ("AKIAIOSFODNN7EXAMPLE.", "[redacted]."),
            ("sk-short and ghp_ and xoxb-", "sk-short and ghp_ and xoxb-"),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn redaction_replaces_long_tokens_that_mix_letters_and_digits() {
        let cases = [
            ("id 3f9a8b7c6d5e4f3a2b1c end", "id [redacted] end"),
            ("eyJhbGci.eyJzdWIiOiIxMjM0NTY3ODkw", "eyJhbGci.[redacted]"),
            (
                "internationalization and end-to-end-encryption",
                "internationalization and end-to-end-encryption",
            ),
            ("order 1Z999AA10123456 shipped", "order 1Z999AA10123456 shipped"),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn redaction_replaces_card_numbers() {
        let cases = [
            ("card 4111111111111111.", "card [redacted]."),
            ("card 4111 1111 1111 1111 exp", "card [redacted] exp"),
            ("card 4111-1111-1111-1111", "card [redacted]"),
            (
                "call 415-555-0187 or 22 345 6789",
                "call 415-555-0187 or 22 345 6789",
            ),
            ("id 12345678901234567890", "id 12345678901234567890"),
            (
                "in 2024 and 2025  2026 2027 2028",
                "in 2024 and 2025  2026 2027 2028",
            ),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn redaction_replaces_private_keys() {
        let cases = [
            (
                "key:\n-----BEGIN RSA PRIVATE KEY-----\nMIIEow\nIBAAK\n-----END RSA PRIVATE KEY-----\nthanks",
                "key:\n[redacted]\nthanks",
            ),
            ("-----BEGIN PRIVATE KEY-----\nMIIEow", "[redacted]"),
            (
                "-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----",
                "-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----",
            ),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn added_entry_is_the_newest() {
        let mut entries = vec![mail("Old text of the writer")];

        add(&mut entries, mail("New text of the writer"));

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

        add(&mut entries, mail("Dear Ana, the report is ready."));

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

        add(&mut entries, mail("Dear Ana, the report"));

        assert_eq!(entries, [mail("Dear Ana, the report is ready.")]);
    }

    #[test]
    fn oldest_entries_go_after_the_most_entries() {
        let mut entries = (0..MAX_ENTRIES)
            .map(|index| mail(&format!("text {index}.")))
            .collect::<Vec<_>>();

        add(&mut entries, mail("newest"));

        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries.first(), Some(&mail("text 1.")));
        assert_eq!(entries.last(), Some(&mail("newest")));
    }

    #[test]
    fn examples_are_the_newest_and_the_longest_texts_of_the_app_oldest_first() {
        let entries = [
            mail("The longest text of the writer in this app."),
            entry("Text of another app", "com.apple.Notes", None),
            mail("Short one."),
            mail("Newest one."),
        ];

        let examples = examples(&entries, &place("com.apple.mail", None, "Hi "));

        assert_eq!(
            examples,
            ["The longest text of the writer in this app.", "Newest one."]
        );
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

        let examples = examples(&entries, &place("com.google.Chrome", Some("github.com"), "LGTM"));

        assert_eq!(examples, ["Review from Safari"]);
    }

    #[test]
    fn example_keeps_the_end_of_a_long_text_from_a_word() {
        let entries = [mail(format!("start {}", "word ".repeat(80)).trim_end())];

        let examples = examples(&entries, &place("com.apple.mail", None, ""));

        assert_eq!(examples, ["word ".repeat(64).trim_end()]);
    }

    #[test]
    fn text_that_the_field_holds_is_no_example() {
        let earlier = "Dear Ana,\n\nThe report is ready and the numbers look good.";
        let entries = [mail(earlier), mail("Other text.")];

        let examples = examples(
            &entries,
            &place("com.apple.mail", None, &format!("{earlier}\n\nBest,")),
        );

        assert_eq!(examples, ["Other text."]);
    }

    #[test]
    fn text_that_holds_a_long_field_is_no_example() {
        let field = "Dear Ana,\n\nThe report is ready and the numbers look";
        let entries = [mail(&format!("{field} good. Best, Tom")), mail("Other text.")];

        let examples = examples(&entries, &place("com.apple.mail", None, field));

        assert_eq!(examples, ["Other text."]);
    }

    #[test]
    fn two_examples_never_hold_the_same_text() {
        let entries = [
            mail("Dear Ana, the report is ready."),
            mail("Dear Ana, the report"),
        ];

        let examples = examples(&entries, &place("com.apple.mail", None, "Hi"));

        assert_eq!(examples, ["Dear Ana, the report"]);
    }

    #[test]
    fn place_without_entries_has_no_examples() {
        let entries = [entry("Text of another app", "com.apple.Notes", None)];

        let examples = examples(&entries, &place("com.apple.mail", None, "Hi"));

        assert_eq!(examples, Vec::<&str>::new());
    }
}
