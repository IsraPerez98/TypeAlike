//! The earlier writing of the user: the texts that GhostType saves, the secrets that it removes
//! from them, and the texts that a prompt shows as examples.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::ops::Range;

use crate::desktop::{FieldText, ScreenText, Surroundings};
use crate::inference::{NextWordBias, TokenCount, WordBias};
use crate::place::{self, Format};
use crate::prompt::{estimated_tokens, first_tokens, from_word, last_chars, last_tokens};
use crate::settings::WordChoice;

/// The most texts that GhostType keeps. The oldest text goes first.
const MAX_ENTRIES: usize = 2000;
/// The fewest characters of a text that GhostType saves.
const MIN_KEPT_CHARS: usize = 40;
/// The most characters of a text that GhostType saves, from its end.
const MAX_KEPT_CHARS: usize = 1000;
/// The examples that the newest texts of a small place fill up to: the same domain in a browser,
/// else the same app.
const RECENT_IN_PLACE: usize = 2;
/// The most texts of a small place. In a larger place, the newest texts are seldom on the topic
/// of the field, so only the matching texts show.
const MAX_SMALL_PLACE_TEXTS: usize = 3;
/// The newest texts of other places with the same format that a prompt shows, when the place has
/// no text.
const RECENT_IN_FORMAT: usize = 1;
/// The most estimated tokens of an example.
const EXAMPLE_TOKENS: TokenCount = TokenCount(160);
/// The estimated tokens of the start of a text longer than [`EXAMPLE_TOKENS`], which its example
/// keeps before its end: the greeting of a long text.
const EXAMPLE_START_TOKENS: TokenCount = TokenCount(33);
/// The share of a text, in percent, that the user must add in a session for GhostType to keep the
/// whole text. With a smaller share, GhostType keeps only the added part.
const MIN_OWN_PERCENT: usize = 50;
/// The characters at the start or at the end of two texts that make them near copies, when they
/// are the same.
const MIN_SHARED_CHARS: usize = 80;
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
/// The words before the next word of a phrase.
const PHRASE_CONTEXT_WORDS: usize = 3;
/// The fewest times that the writer wrote the same next word after the same words, for a
/// suggestion of that word without the model.
const MIN_PHRASE_COUNT: u32 = 3;
/// The smallest share, in percent, of the next word among all the words that the writer wrote
/// after the same words.
const MIN_PHRASE_PERCENT: u32 = 60;
/// The words of the runs before a next word that give a preference for it, longest first.
const RUN_WORDS: [usize; 2] = [PHRASE_CONTEXT_WORDS, 2];
/// The fewest times that the writer wrote a run of words, for a preference for its next words.
const MIN_RUN_COUNT: u32 = 2;
/// The most next words that a completion prefers.
const MAX_NEXT_WORDS: usize = 8;

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

    fn place(&self) -> Place<'_> {
        Place::of(&self.app_id, self.domain.as_deref())
    }

    fn is_in_place_of(&self, surroundings: &Surroundings) -> bool {
        Place::of_field(surroundings) == Some(self.place())
    }

    /// Whether the texts start with the same [`MIN_SHARED_CHARS`] characters, or end with them.
    fn shares_an_edge_with(&self, other: &Self) -> bool {
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

    fn overlaps(&self, other: &Self) -> bool {
        self.text.contains(&other.text) || other.text.contains(&self.text)
    }
}

/// Where the user wrote a text: a web page in any browser, else an app.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place<'text> {
    /// The host of the web page.
    WebPage(&'text str),
    /// The id of the app.
    App(&'text str),
}

impl<'text> Place<'text> {
    fn of(app_id: &'text str, domain: Option<&'text str>) -> Self {
        domain.map_or(Self::App(app_id), Self::WebPage)
    }

    /// The place of the field of `surroundings`, or `None` when it has no app id and no domain.
    fn of_field(surroundings: &'text Surroundings) -> Option<Self> {
        match (&surroundings.domain, &surroundings.app_id) {
            (Some(domain), _) => Some(Self::WebPage(domain)),
            (None, Some(app_id)) => Some(Self::App(app_id)),
            (None, None) => None,
        }
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
/// text holds goes, and the new entry does not come when an older one holds its text. An older
/// entry of the same place that starts or ends like the new text (a near copy) also goes. The
/// oldest entries go when there are more than [`MAX_ENTRIES`].
pub(crate) fn add(entries: &mut Vec<HistoryEntry>, entry: HistoryEntry) {
    let is_known = entries
        .iter()
        .any(|known| known.place() == entry.place() && known.text.contains(&entry.text));
    if is_known {
        return;
    }
    entries.retain(|known| {
        let is_replaced = entry.text.contains(&known.text) || entry.shares_an_edge_with(known);
        !(known.place() == entry.place() && is_replaced)
    });
    entries.push(entry);
    let excess = entries.len().saturating_sub(MAX_ENTRIES);
    entries.drain(..excess);
}

/// Returns the part of `text`, the text before the caret at the end of a session in the place of
/// `app_id` and `domain`, that the user wrote. The first `field_chars` characters came from the
/// field. They count as written by the user when `entries` hold them, because the user continues
/// a draft, or when the user added at least [`MIN_OWN_PERCENT`] of the text. Else only the added
/// part counts, from its first whole word.
pub(crate) fn own_text<'text>(
    entries: &[HistoryEntry],
    text: &'text str,
    field_chars: usize,
    app_id: &str,
    domain: Option<&str>,
) -> &'text str {
    let added_start = text
        .char_indices()
        .nth(field_chars)
        .map_or(text.len(), |(index, _)| index);
    let (field, added) = text.split_at(added_start);
    let is_draft = HistoryEntry::of(field, app_id, domain).is_some_and(|draft| {
        entries
            .iter()
            .any(|known| known.place() == draft.place() && known.text.contains(&draft.text))
    });
    let is_mostly_added = added.chars().count() * 100 >= text.chars().count() * MIN_OWN_PERCENT;
    if field.trim().is_empty() || is_draft || is_mostly_added {
        return text;
    }
    from_word(text, added)
}

/// Returns the texts of `entries`, oldest first, that a prompt in `surroundings` shows as
/// examples: the `matching` best matching texts of the same place. In a place with at most
/// [`MAX_SMALL_PLACE_TEXTS`] texts, its newest texts fill the examples up to [`RECENT_IN_PLACE`].
/// A place without texts gets the newest texts of other places with the same format. A text
/// matches when it shares rare words with the text near the caret, the window, the labels of the
/// field and the `screen`. Each example keeps the start and the end of its text. A text that the
/// field holds now is no example, and two examples never hold the same text.
pub(crate) fn examples(
    entries: &[HistoryEntry],
    surroundings: &Surroundings,
    screen: Option<&ScreenText>,
    matching: usize,
) -> Vec<String> {
    let candidates = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !is_copy_of_field(entry, surroundings.field_text.as_ref()))
        .rev();
    let (in_place, elsewhere): (Vec<_>, Vec<_>) =
        candidates.partition(|(_, entry)| entry.is_in_place_of(surroundings));
    let topics = Topics::of(entries, surroundings, screen);
    let newest_first = in_place.clone();
    let mut scored = in_place
        .iter()
        .map(|&candidate| (topics.score(&candidate.1.text), candidate))
        .filter(|(score, _)| *score > 0.0)
        .collect::<Vec<_>>();
    scored.sort_by(|(left, _), (right, _)| right.total_cmp(left));
    let best_first = scored
        .into_iter()
        .map(|(_, candidate)| candidate)
        .collect::<Vec<_>>();
    let mut picked = Vec::new();
    pick(&mut picked, &best_first, matching);
    if newest_first.len() <= MAX_SMALL_PLACE_TEXTS {
        let missing = RECENT_IN_PLACE.saturating_sub(picked.len());
        pick(&mut picked, &newest_first, missing);
    }
    if newest_first.is_empty() {
        let format = Format::of(surroundings).filter(|format| *format != Format::WebPage);
        let same_format = elsewhere
            .iter()
            .copied()
            .filter(|(_, entry)| {
                format.is_some() && Format::of_place(Some(&entry.app_id), entry.domain.as_deref()) == format
            })
            .collect::<Vec<_>>();
        pick(&mut picked, &same_format, RECENT_IN_FORMAT);
    }
    picked.sort_by_key(|&(index, _)| index);
    picked
        .into_iter()
        .map(|(_, entry)| example_of(&entry.text))
        .collect()
}

/// Whether `entries` hold a text of the place of `surroundings`.
pub(crate) fn has_text_in_place(entries: &[HistoryEntry], surroundings: &Surroundings) -> bool {
    entries.iter().any(|entry| entry.is_in_place_of(surroundings))
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

/// Whether `lowercase` tells the topic of a text: it has at least [`MIN_WORD_CHARS`] letters and
/// is not one of the [`COMMON_WORDS`].
fn is_topic_word(lowercase: &str) -> bool {
    lowercase.chars().count() >= MIN_WORD_CHARS && !COMMON_WORDS.contains(&lowercase)
}

/// The topic words of the text near the caret, each with the number of entries that hold it.
struct Topics {
    entries: usize,
    query: HashMap<String, u32>,
}

impl Topics {
    fn of(entries: &[HistoryEntry], surroundings: &Surroundings, screen: Option<&ScreenText>) -> Self {
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
        for entry in entries {
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
            entries: entries.len(),
            query,
        }
    }

    /// The sum of the weights of the query words that `text` holds. A word that few entries hold
    /// weighs more than a word that many entries hold, but each word weighs something.
    fn score(&self, text: &str) -> f32 {
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
        total
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

/// Returns `start`, the start of `text`, without the part of a word at its end.
fn start_at_word<'text>(text: &str, start: &'text str) -> &'text str {
    if text[start.len()..].starts_with(char::is_whitespace) {
        return start.trim_end();
    }
    start
        .rsplit_once(char::is_whitespace)
        .map_or("", |(whole, _)| whole.trim_end())
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
/// the choice is off. A word is a run of letters and apostrophes with at least
/// [`MIN_WORD_CHARS`] letters, which the entries hold at least [`MIN_WORD_COUNT`] times in any
/// case and which is not one of the [`COMMON_WORDS`]. The preference has the spelling that the
/// entries use most.
pub(crate) fn word_bias(entries: &[HistoryEntry], choice: WordChoice, version: u64) -> Option<WordBias> {
    let mut counts = HashMap::<String, u32>::new();
    for entry in entries {
        for word in words(&entry.text.replace(REDACTED, " ")) {
            *counts.entry(word.to_owned()).or_default() += 1;
        }
    }
    word_bias_of_counts(counts, choice, version)
}

/// Returns the preference of `choice` for the words of `counts`, each with the number of times
/// that the writer wrote it, or `None` when the choice is off. Spellings that differ only in case
/// count as one word, and the preference has the spelling that comes most, and the lowercase one
/// when two come the same number of times. A word needs at least 3 letters and 3 times, and the
/// most common words of English and Spanish get no preference. A word that comes 20 times or more
/// gets the full strength of the choice.
#[must_use]
pub fn word_bias_of_counts(
    counts: impl IntoIterator<Item = (String, u32)>,
    choice: WordChoice,
    version: u64,
) -> Option<WordBias> {
    let strength = strength_of(choice)?;
    let full_weight = f32::from(FULL_WEIGHT_COUNT).ln();
    let mut spellings = HashMap::<String, Vec<(String, u32)>>::new();
    for (word, count) in counts {
        spellings
            .entry(word.to_lowercase())
            .or_default()
            .push((word, count));
    }
    let mut words = spellings
        .into_iter()
        .filter_map(|(lowercase, spellings)| {
            let count = spellings.iter().map(|(_, count)| count).sum::<u32>();
            let is_wanted = count >= MIN_WORD_COUNT
                && lowercase.chars().count() >= MIN_WORD_CHARS
                && !COMMON_WORDS.contains(&lowercase.as_str());
            let (usual, _) = spellings.into_iter().max_by_key(|(spelling, count)| {
                (
                    *count,
                    *spelling == lowercase,
                    std::cmp::Reverse(spelling.clone()),
                )
            })?;
            let capped = u16::try_from(count).unwrap_or(u16::MAX).min(FULL_WEIGHT_COUNT);
            is_wanted.then(|| (usual, strength * f32::from(capped).ln() / full_weight))
        })
        .collect::<Vec<_>>();
    words.sort_by(|left, right| left.0.cmp(&right.0));
    Some(WordBias {
        version,
        words: words.into(),
    })
}

/// Returns the most logits that `choice` adds to a word, or `None` when the choice is off.
fn strength_of(choice: WordChoice) -> Option<f32> {
    match choice {
        WordChoice::Off => None,
        WordChoice::Light => Some(0.5),
        WordChoice::Medium => Some(1.0),
        WordChoice::Strong => Some(2.0),
    }
}

/// What the writer wrote after each run of [`RUN_WORDS`] words in any case, for the runs that the
/// writer wrote at least [`MIN_RUN_COUNT`] times: the phrases that the writer repeats, and the
/// words that the writer wrote after the last words before the caret. `Debug` shows only the
/// number of runs, so logs never hold what the user wrote.
#[derive(Default)]
pub(crate) struct Phrases {
    /// The logits for a word that always came after its run.
    strength: f32,
    /// For each number of words of a run, longest first, the texts after each run up to the end
    /// of the next word, with the signs before that word, and their counts.
    runs: Vec<(usize, Followers)>,
}

type Followers = HashMap<String, Vec<(String, u32)>>;

impl fmt::Debug for Phrases {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let runs = self
            .runs
            .iter()
            .map(|(_, followers)| followers.len())
            .sum::<usize>();
        formatter
            .debug_struct("Phrases")
            .field("runs", &runs)
            .finish_non_exhaustive()
    }
}

impl Phrases {
    /// Returns the rest of a phrase after `text_before`, the text before the caret, up to the end
    /// of its next word, or `None`. The text can end inside that word. The rest never goes past
    /// the end of the line. A phrase needs the same next word, with the same signs before it, at
    /// least [`MIN_PHRASE_COUNT`] times and in at least [`MIN_PHRASE_PERCENT`] of the times that
    /// the writer wrote the [`PHRASE_CONTEXT_WORDS`] words before it.
    pub(crate) fn continuation_of(&self, text_before: &str) -> Option<&str> {
        let followers = self.followers_of(PHRASE_CONTEXT_WORDS)?;
        let spans = runs_of(text_before, is_phrase_character);
        let ends_inside_word = text_before.ends_with(is_phrase_character);
        let partial_word = ends_inside_word.then(|| spans.len().checked_sub(1)).flatten();
        [partial_word, Some(spans.len())]
            .into_iter()
            .flatten()
            .find_map(|context_end| {
                let context = spans.get(context_end.checked_sub(PHRASE_CONTEXT_WORDS)?..context_end)?;
                let follower = repeated_follower(followers.get(&context_key(text_before, context))?)?;
                let typed = &text_before[context.last()?.end..];
                let rest = follower.strip_prefix(typed)?;
                (!rest.is_empty() && !rest.contains(['\r', '\n'])).then_some(rest)
            })
    }

    /// Returns the preference for the words that the writer wrote after the last words of
    /// `text_before`, the text before the caret: the words after the longest run that has some.
    /// Each word gets the strength times its share of the words after the run. When the text ends
    /// inside a word, the run ends before that word and the words start with its letters. A word
    /// gets no preference when a line break or no space comes before it.
    pub(crate) fn next_word_bias(&self, text_before: &str) -> NextWordBias {
        let spans = runs_of(text_before, is_phrase_character);
        let partial_words = usize::from(text_before.ends_with(is_phrase_character));
        let context_end = spans.len().saturating_sub(partial_words);
        let words = self.runs.iter().find_map(|(context_words, followers)| {
            let context = spans.get(context_end.checked_sub(*context_words)?..context_end)?;
            let counts = followers.get(&context_key(text_before, context))?;
            let typed = &text_before[context.last()?.end..];
            let words = preferred_words(counts, typed, self.strength);
            (!words.is_empty()).then_some(words)
        });
        NextWordBias(words.unwrap_or_default())
    }

    fn followers_of(&self, context_words: usize) -> Option<&Followers> {
        self.runs
            .iter()
            .find(|(words, _)| *words == context_words)
            .map(|(_, followers)| followers)
    }
}

/// Returns the text of `counts` that the writer wrote at least [`MIN_PHRASE_COUNT`] times and in
/// at least [`MIN_PHRASE_PERCENT`] of the times, if any.
fn repeated_follower(counts: &[(String, u32)]) -> Option<&str> {
    let total = counts.iter().map(|(_, count)| count).sum::<u32>();
    let (follower, count) = counts.iter().max_by_key(|(_, count)| count)?;
    let is_repeated = *count >= MIN_PHRASE_COUNT && count * 100 >= total * MIN_PHRASE_PERCENT;
    is_repeated.then_some(follower.as_str())
}

/// Returns the words of `counts` after `typed` with their share of `strength`, most common first.
/// A word needs a space before it and no line break.
fn preferred_words(counts: &[(String, u32)], typed: &str, strength: f32) -> Vec<(String, f32)> {
    let total = small_count(counts.iter().map(|(_, count)| count).sum());
    let mut words = counts
        .iter()
        .filter(|(follower, _)| follower.starts_with(typed))
        .filter_map(|(follower, count)| {
            let word = follower.trim_start_matches(|character| !is_phrase_character(character));
            let signs = &follower[..follower.len() - word.len()];
            let is_after_space = signs.ends_with(char::is_whitespace) && !signs.contains(['\r', '\n']);
            is_after_space.then_some((word, *count))
        })
        .collect::<Vec<_>>();
    words.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(right.0)));
    words
        .into_iter()
        .take(MAX_NEXT_WORDS)
        .map(|(word, count)| (word.to_owned(), strength * small_count(count) / total))
        .collect()
}

/// Returns `count` as a float, at most [`u16::MAX`].
fn small_count(count: u32) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

/// Returns the phrases of `entries` for `choice`, which have none when the choice is off.
pub(crate) fn phrases(entries: &[HistoryEntry], choice: WordChoice) -> Phrases {
    let Some(strength) = strength_of(choice) else {
        return Phrases::default();
    };
    let runs = RUN_WORDS
        .iter()
        .map(|&context_words| (context_words, repeated_runs(entries, context_words)))
        .collect();
    Phrases { strength, runs }
}

/// Returns, for each run of `context_words` words of `entries` in any case that the writer wrote
/// at least [`MIN_RUN_COUNT`] times, the texts that came after it up to the end of the next word,
/// with the signs before that word, and their counts.
fn repeated_runs(entries: &[HistoryEntry], context_words: usize) -> Followers {
    let mut followers = HashMap::<String, Vec<(&str, u32)>>::new();
    for part in entries.iter().flat_map(|entry| entry.text.split(REDACTED)) {
        let spans = runs_of(part, is_phrase_character);
        let lowercase = spans
            .iter()
            .map(|span| {
                part[span.clone()]
                    .chars()
                    .flat_map(char::to_lowercase)
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        for (start, window) in spans.windows(context_words + 1).enumerate() {
            let follower = &part[window[context_words - 1].end..window[context_words].end];
            let counts = followers
                .entry(lowercase[start..start + context_words].join(" "))
                .or_default();
            match counts.iter_mut().find(|(known, _)| *known == follower) {
                Some((_, count)) => *count += 1,
                None => counts.push((follower, 1)),
            }
        }
    }
    followers
        .into_iter()
        .filter(|(_, counts)| counts.iter().map(|(_, count)| count).sum::<u32>() >= MIN_RUN_COUNT)
        .map(|(context, counts)| {
            let owned = counts
                .into_iter()
                .map(|(follower, count)| (follower.to_owned(), count))
                .collect();
            (context, owned)
        })
        .collect()
}

/// Returns the words of `context`, spans of `text`, in lowercase with one space between them.
fn context_key(text: &str, context: &[Range<usize>]) -> String {
    let mut key = String::new();
    for span in context {
        if !key.is_empty() {
            key.push(' ');
        }
        key.extend(text[span.clone()].chars().flat_map(char::to_lowercase));
    }
    key
}

/// The characters of a word of a phrase: letters, digits and apostrophes.
fn is_phrase_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '\'' | '’')
}

/// Returns the words of `text` in their spelling: its runs of letters and apostrophes.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|character: char| !(character.is_alphabetic() || matches!(character, '\'' | '’')))
        .filter(|word| !word.is_empty())
}

/// Returns `text` with each secret replaced by [`REDACTED`]: private keys, API tokens, long tokens
/// that mix letters and digits, and card numbers.
pub(crate) fn redacted(text: &str) -> String {
    let text = replaced(text, &private_key_spans(text));
    let secret_tokens = runs_of(&text, is_token_character)
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

/// The spans of the runs of the characters of `text` that `is_part` accepts.
fn runs_of(text: &str, is_part: fn(char) -> bool) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut start = None;
    for (index, character) in text.char_indices() {
        match (is_part(character), start) {
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
    use crate::desktop::{Confidence, ScreenLine, ScreenRect};

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

    fn examples_in(entries: &[HistoryEntry], surroundings: &Surroundings) -> Vec<String> {
        examples(entries, surroundings, None, 2)
    }

    fn words_of(bias: Option<WordBias>) -> Option<Vec<(String, f32)>> {
        bias.map(|bias| bias.words.to_vec())
    }

    #[test]
    fn word_bias_holds_the_words_of_three_letters_that_come_three_times() {
        let entries = [mail("Cheers mate, ok ok ok. Cheers"), mail("Cheers MATE! Mate's")];

        let bias = word_bias(&entries, WordChoice::Medium, 1);

        assert_eq!(
            words_of(bias),
            Some(vec![("Cheers".to_owned(), 3.0_f32.ln() / 20.0_f32.ln())])
        );
    }

    #[test]
    fn word_bias_has_the_spelling_that_the_writer_uses_most() {
        let entries = [mail("Bluefin bluefin Bluefin. reckon Reckon reckon")];

        let bias = word_bias(&entries, WordChoice::Medium, 1);

        let spellings =
            words_of(bias).map(|words| words.into_iter().map(|(word, _)| word).collect::<Vec<_>>());
        assert_eq!(spellings, Some(vec!["Bluefin".to_owned(), "reckon".to_owned()]));
    }

    #[test]
    fn word_bias_has_the_lowercase_spelling_when_two_spellings_come_the_same_times() {
        let entries = [mail("Cheers cheers Cheers cheers")];

        let bias = word_bias(&entries, WordChoice::Medium, 1);

        let spellings =
            words_of(bias).map(|words| words.into_iter().map(|(word, _)| word).collect::<Vec<_>>());
        assert_eq!(spellings, Some(vec!["cheers".to_owned()]));
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

    fn phrase_entries() -> [HistoryEntry; 3] {
        [
            mail("Please let me know if it works."),
            mail("Let me know if you can come."),
            mail("So let me KNOW if that helps."),
        ]
    }

    fn continuation_in(entries: &[HistoryEntry], text_before: &str) -> Option<String> {
        phrases(entries, WordChoice::Medium)
            .continuation_of(text_before)
            .map(str::to_owned)
    }

    #[test]
    fn phrase_continues_the_text_with_the_next_word_of_the_writer() {
        let cases = [
            ("Can you let me know ", Some("if")),
            ("Can you Let Me know", Some(" if")),
            ("Can you let me know i", Some("f")),
            ("Can you let me know if", None),
            ("Can you let us know ", None),
        ];

        let continuations = cases.map(|(text_before, _)| continuation_in(&phrase_entries(), text_before));

        assert_eq!(continuations, cases.map(|(_, next)| next.map(str::to_owned)));
    }

    #[test]
    fn phrase_needs_three_times_and_a_share_of_sixty_percent() {
        let cases: [(&[&str], Option<&str>); 4] = [
            (&["if", "if"], None),
            (&["if", "if", "if"], Some("if")),
            (&["if", "if", "if", "when", "when"], Some("if")),
            (&["if", "if", "if", "when", "when", "when"], None),
        ];

        let continuations = cases.map(|(next_words, _)| {
            let entries = next_words
                .iter()
                .map(|word| mail(&format!("let me know {word} it works")))
                .collect::<Vec<_>>();
            continuation_in(&entries, "Can you let me know ")
        });

        assert_eq!(continuations, cases.map(|(_, next)| next.map(str::to_owned)));
    }

    #[test]
    fn phrase_keeps_the_signs_and_the_spelling_of_the_writer() {
        let entries = vec![mail("Thanks a lot.\nBest regards,\nJordan"); 3];

        let continuation = continuation_in(&entries, "Thanks a lot.\nBest regards,\nJ");

        assert_eq!(continuation.as_deref(), Some("ordan"));
    }

    #[test]
    fn phrase_ends_at_the_end_of_the_line() {
        let entries = vec![mail("Thanks, kind regards,\nMateo"); 3];

        let continuation = continuation_in(&entries, "Thanks, kind regards");

        assert_eq!(continuation, None);
    }

    #[test]
    fn redacted_secret_is_no_phrase() {
        let entries = vec![mail("the code is [redacted] today"); 3];

        let continuation = continuation_in(&entries, "So the code is ");

        assert_eq!(continuation, None);
    }

    #[test]
    fn word_choice_off_gives_no_phrases() {
        let continuation = phrases(&phrase_entries(), WordChoice::Off)
            .continuation_of("Can you let me know ")
            .map(str::to_owned);

        assert_eq!(continuation, None);
    }

    #[test]
    fn debug_of_the_phrases_hides_the_text() {
        let entries = [mail("let me know if"), mail("let me know if")];

        let debug = format!("{:?}", phrases(&entries, WordChoice::Medium));

        assert_eq!(debug, "Phrases { runs: 3, .. }");
    }

    fn next_word_entries() -> Vec<HistoryEntry> {
        ["if it works", "if you can", "when you can", "if that helps"]
            .map(|rest| mail(&format!("Please let me know {rest}.")))
            .to_vec()
    }

    fn next_words_in(entries: &[HistoryEntry], choice: WordChoice, text_before: &str) -> Vec<(String, f32)> {
        phrases(entries, choice).next_word_bias(text_before).0
    }

    fn word(word: &str, weight: f32) -> (String, f32) {
        (word.to_owned(), weight)
    }

    #[test]
    fn next_word_gets_the_share_of_the_strength_that_it_has_after_the_last_three_words() {
        let next = next_words_in(&next_word_entries(), WordChoice::Medium, "Can you let me know ");

        assert_eq!(next, [word("if", 0.75), word("when", 0.25)]);
    }

    #[test]
    fn next_words_inside_a_word_start_with_the_typed_letters() {
        let next = next_words_in(&next_word_entries(), WordChoice::Medium, "Can you let me know w");

        assert_eq!(next, [word("when", 0.25)]);
    }

    #[test]
    fn next_words_come_after_the_last_two_words_when_the_last_three_have_none() {
        let next = next_words_in(&next_word_entries(), WordChoice::Medium, "Then tell me know ");

        assert_eq!(next, [word("if", 0.75), word("when", 0.25)]);
    }

    #[test]
    fn strength_of_the_choice_scales_the_next_words() {
        let choices = [WordChoice::Off, WordChoice::Light, WordChoice::Strong];

        let next = choices.map(|choice| next_words_in(&next_word_entries(), choice, "let me know "));

        assert_eq!(
            next,
            [
                vec![],
                vec![word("if", 0.375), word("when", 0.125)],
                vec![word("if", 1.5), word("when", 0.5)]
            ]
        );
    }

    #[test]
    fn run_of_words_that_comes_one_time_gives_no_next_words() {
        let entries = [mail("We meet at noon today.")];

        let next = next_words_in(&entries, WordChoice::Medium, "So we meet at ");

        assert_eq!(next, []);
    }

    #[test]
    fn next_word_after_a_line_break_or_without_a_space_gets_no_preference() {
        let entries = ["Thanks a lot.\nJordan", "Thanks a lot, Jordan", "send the e-mail"]
            .repeat(2)
            .into_iter()
            .map(mail)
            .collect::<Vec<_>>();
        let cases = [
            ("Thanks a lot, ", vec![word("Jordan", 0.5)]),
            ("Thanks a lot.\n", vec![]),
            ("Please send the e-", vec![]),
        ];

        let next = cases
            .clone()
            .map(|(text_before, _)| next_words_in(&entries, WordChoice::Medium, text_before));

        assert_eq!(next, cases.map(|(_, words)| words));
    }

    #[test]
    fn next_words_are_at_most_the_eight_most_common() {
        let entries =
            ["a", "b", "c", "d", "e", "f", "g", "h", "i", "a"].map(|next| mail(&format!("we go to {next}x")));

        let next = next_words_in(&entries, WordChoice::Medium, "So we go to ");

        assert_eq!(next.first(), Some(&word("ax", 0.2)));
        assert_eq!(next.len(), 8);
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

    const DRAFT: &str = "Dear Ana, I send you the report of the third quarter with all the numbers.";

    fn own(entries: &[HistoryEntry], text: &str, field_chars: usize) -> String {
        own_text(entries, text, field_chars, "com.apple.mail", None).to_owned()
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
    fn added_entry_replaces_a_near_copy_with_an_edit_in_the_middle() {
        let start = "Dear Ana, I send you the report of the third quarter with all the numbers.";
        let old = format!("{start} The costs went down by two percent. Best, Tom");
        let new = format!("{start} The costs went down by four percent. Best, Tom");
        let mut entries = vec![mail(&old)];

        add(&mut entries, mail(&new));

        assert_eq!(entries, [mail(&new)]);
    }

    #[test]
    fn added_entry_replaces_a_near_copy_with_an_edit_at_its_start() {
        let end = "the numbers of the third quarter are ready and the costs went down. Best, Tom";
        let mut entries = vec![mail(&format!("Hi Ana, {end}"))];

        add(&mut entries, mail(&format!("Hello Anna, {end}")));

        assert_eq!(entries, [mail(&format!("Hello Anna, {end}"))]);
    }

    #[test]
    fn short_texts_with_the_same_greeting_stay_two_entries() {
        let mut entries = vec![mail("Dear Ana, the report is ready for you.")];

        add(&mut entries, mail("Dear Ana, the meeting moved to Friday."));

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

        add(&mut entries, continued.clone());

        assert_eq!(entries, [continued]);
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

    #[test]
    fn near_copy_in_another_place_stays() {
        let text = "Dear Ana, I send you the report of the third quarter with all the numbers. Best, Tom";
        let mut entries = vec![entry(text, "com.apple.Notes", None)];

        add(&mut entries, mail(&format!("{text} PS")));

        assert_eq!(entries.len(), 2);
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

        let examples = examples(&large_mail_place(), &surroundings, None, 1);

        assert_eq!(examples, ["Bluefin budget review with Dana tomorrow morning."]);
    }

    #[test]
    fn newest_texts_of_a_small_place_fill_the_second_example() {
        let surroundings = place("com.apple.mail", None, "Update on the Bluefin budget");

        let examples = examples(&bluefin_entries(), &surroundings, None, 1);

        assert_eq!(
            examples,
            [
                "Weekend trip to the lake with the kids, back on Sunday.",
                "Bluefin budget review with Dana tomorrow morning."
            ]
        );
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

        let examples = examples(
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
