//! The earlier texts that a prompt shows as examples: the texts that match the topic of the field.

use std::collections::{HashMap, HashSet};

use super::context::typing_context;
use super::entry::HistoryEntry;
use super::store::SessionTexts;
use super::words::{is_distinctive_word, is_within_edits, small_count, words};
use crate::desktop::{FieldText, ScreenText, Surroundings, Timestamp};
use crate::excerpt::{estimated_tokens, first_tokens, from_word, last_tokens, start_at_word};
use crate::inference::TokenCount;
use crate::place::{self, Format, Place};
use crate::screen::LineScope;

/// The examples that the newest texts of a small place fill up to: the same domain in a browser,
/// else the same app.
const RECENT_IN_PLACE: usize = 2;
/// The most texts of a small place. In a larger place, the newest texts are seldom on the topic
/// of the field, so only the matching texts show.
const MAX_SMALL_PLACE_TEXTS: usize = 3;
/// The newest texts of the typing context of the window that a prompt shows, when no text of the
/// place matches. A text to a contact seldom holds the name of the contact.
const RECENT_IN_CONTEXT: usize = 1;
/// The newest texts of other places with the same format that a prompt shows, when the place has
/// no text.
const RECENT_IN_FORMAT: usize = 1;
/// The fewest topic words that a text of another place shares with the field, for an example. A
/// text of another place often has another topic or style, which harms more than no example.
const MIN_WIDE_WORDS: usize = 3;
/// The fewest characters before the caret that make a text which holds them a copy of the field.
/// Fewer characters, such as a greeting, are also in texts that are not copies.
const MIN_COPIED_CHARS: usize = 40;
/// The most edits of two short near copies. Short texts that differ in so few edits still say the
/// same thing.
const MIN_NEAR_COPY_EDITS: usize = 2;
/// The most edits, as a percent of the characters of the shorter text, of two near copies.
const NEAR_COPY_EDIT_PERCENT: usize = 10;
/// The most estimated tokens of an example.
const EXAMPLE_TOKENS: TokenCount = TokenCount(160);
/// The estimated tokens of the start of a text longer than [`EXAMPLE_TOKENS`], which its example
/// keeps before its end: the greeting of a long text.
const EXAMPLE_START_TOKENS: TokenCount = TokenCount(33);
/// The estimated tokens of the end of a text longer than [`EXAMPLE_TOKENS`], which its example
/// keeps after its start.
const EXAMPLE_END_TOKENS: TokenCount = TokenCount(EXAMPLE_TOKENS.0 - EXAMPLE_START_TOKENS.0);

/// Returns the texts of `texts`, oldest first, that a prompt in `surroundings` shows as examples:
/// the `matching_examples` best matching texts of the same place. When none matches, the newest
/// texts of the typing context of the window fill up to [`RECENT_IN_CONTEXT`]. In a place with at
/// most [`MAX_SMALL_PLACE_TEXTS`] texts, its newest texts fill the examples up to
/// [`RECENT_IN_PLACE`]. The best matching texts of other places with at least [`MIN_WIDE_WORDS`]
/// topic words fill the rest of the `matching_examples`. A place without texts gets the newest
/// texts of other places with the same format in the free examples. A text matches when it shares
/// rare words with the text near the caret, the window, the labels of the field and the lines of
/// the `screen` in the column of the field, or when the message that it answers shares such words.
/// Each example keeps the start and the end of its text. A word of the window title matches only a
/// text of the same typing context or of none, because a text of another conversation that holds
/// the name of a contact is about that contact, not to that contact. A text that the field holds
/// now is no example, and two examples never hold the same text or near copies of one text. Among
/// texts that match the same, the text with more copies in its place comes first, so that the
/// spelling that the writer uses most stays. Each example comes with the time of its text.
pub(crate) fn examples(
    texts: &SessionTexts<'_>,
    surroundings: &Surroundings,
    screen: Option<&ScreenText>,
    matching_examples: usize,
) -> Vec<(String, Option<Timestamp>)> {
    let field = surroundings.field_text.as_ref();
    let in_place = newest_not_in_field(&texts.local, field);
    let elsewhere = newest_not_in_field(&texts.wide, field);
    let pool = texts
        .local
        .iter()
        .chain(&texts.wide)
        .map(|&(_, entry)| entry)
        .collect::<Vec<_>>();
    let topics = Topics::of(&pool, surroundings, screen);
    let mut picked = Vec::new();
    pick(&mut picked, &topics.best_first(&in_place, 1), matching_examples);
    if picked.is_empty() {
        pick(
            &mut picked,
            &topics.same_context_texts(&in_place),
            RECENT_IN_CONTEXT.min(matching_examples),
        );
    }
    if in_place.len() <= MAX_SMALL_PLACE_TEXTS {
        let missing = RECENT_IN_PLACE.saturating_sub(picked.len());
        pick(&mut picked, &in_place, missing);
    }
    let missing = matching_examples.saturating_sub(picked.len());
    pick(
        &mut picked,
        &topics.best_first(&elsewhere, MIN_WIDE_WORDS),
        missing,
    );
    if in_place.is_empty() {
        let missing = matching_examples.saturating_sub(picked.len());
        pick(
            &mut picked,
            &same_format_texts(&elsewhere, surroundings),
            RECENT_IN_FORMAT.min(missing),
        );
    }
    picked.sort_by_key(|&(index, _)| index);
    picked
        .into_iter()
        .map(|(_, entry)| (example_of(&entry.text), entry.written_at))
        .collect()
}

/// Returns the `candidates` with the format of the field of `surroundings`. It has none when that
/// format is unknown or a web page.
fn same_format_texts<'entries>(
    candidates: &[(usize, &'entries HistoryEntry)],
    surroundings: &Surroundings,
) -> Vec<(usize, &'entries HistoryEntry)> {
    let Some(format) = Format::of(surroundings).filter(|format| *format != Format::WebPage) else {
        return Vec::new();
    };
    candidates
        .iter()
        .copied()
        .filter(|(_, entry)| Format::of_place(Some(&entry.app_id), entry.domain.as_deref()) == Some(format))
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
    entry_count: usize,
    holders_by_word: HashMap<String, u32>,
    /// The topic words that only the window title gives.
    title_words: HashSet<String>,
    /// The typing context of the window.
    context: Option<String>,
}

/// How well a text matches the topic words: the sum of the weights of the topic words that it
/// holds, and the number of these words.
struct TopicMatch {
    weight: f32,
    words: usize,
}

impl Topics {
    fn of(entries: &[&HistoryEntry], surroundings: &Surroundings, screen: Option<&ScreenText>) -> Self {
        let (mut holders_by_word, title_words) = query_words(surroundings, screen);
        for entry in entries {
            for word in matched_words(&holders_by_word, entry) {
                *holders_by_word.entry(word).or_default() += 1;
            }
        }
        Self {
            entry_count: entries.len(),
            holders_by_word,
            title_words,
            context: typing_context(
                surroundings.app_name.as_deref(),
                surroundings.window_title.as_deref(),
            ),
        }
    }

    /// Returns the `candidates` with the typing context of the window. It has none when the window
    /// title names no typing context.
    fn same_context_texts<'entries>(
        &self,
        candidates: &[(usize, &'entries HistoryEntry)],
    ) -> Vec<(usize, &'entries HistoryEntry)> {
        let Some(context) = &self.context else {
            return Vec::new();
        };
        candidates
            .iter()
            .copied()
            .filter(|(_, entry)| entry.typing_context.as_ref() == Some(context))
            .collect()
    }

    /// Whether `entry` is a text of another typing context than the window.
    fn is_other_context(&self, entry: &HistoryEntry) -> bool {
        entry
            .typing_context
            .as_ref()
            .is_some_and(|context| self.context.as_ref() != Some(context))
    }

    /// Returns the `candidates` that hold at least `min_words` query words, best match first. Among
    /// the same matches, the text with more copies in its place comes first, then the newest.
    fn best_first<'entries>(
        &self,
        candidates: &[(usize, &'entries HistoryEntry)],
        min_words: usize,
    ) -> Vec<(usize, &'entries HistoryEntry)> {
        let mut copies = HashMap::<_, usize>::new();
        for (_, entry) in candidates {
            *copies.entry(copy_key(entry)).or_default() += 1;
        }
        let mut scored = candidates
            .iter()
            .map(|&candidate| (self.score(candidate.1), copies[&copy_key(candidate.1)], candidate))
            .filter(|(topic_match, _, _)| topic_match.words >= min_words)
            .collect::<Vec<_>>();
        scored.sort_by(|(left, left_copies, _), (right, right_copies, _)| {
            right
                .weight
                .total_cmp(&left.weight)
                .then(right_copies.cmp(left_copies))
        });
        scored.into_iter().map(|(_, _, candidate)| candidate).collect()
    }

    /// Returns how well `entry` matches. A word that few entries hold weighs more than a word that
    /// many entries hold, but each word weighs something.
    fn score(&self, entry: &HistoryEntry) -> TopicMatch {
        let entry_count = small_count(self.entry_count);
        let mut words = matched_words(&self.holders_by_word, entry);
        if self.is_other_context(entry) {
            words.retain(|word| !self.title_words.contains(word));
        }
        let mut weight = 0.0;
        for &holders in words.iter().filter_map(|word| self.holders_by_word.get(word)) {
            weight += (entry_count / small_count(holders).max(1.0)).ln_1p();
        }
        TopicMatch {
            weight,
            words: words.len(),
        }
    }
}

/// Returns what the copies of the text of `entry` share: its place and its trimmed text.
fn copy_key(entry: &HistoryEntry) -> (Place<&str>, &str) {
    (entry.place(), entry.text.trim())
}

/// Returns the topic words of the text near the caret, the window title, the labels of the field
/// and the lines of the `screen` in the column of the field, each with no holders yet, and the
/// words of them that only the window title gives.
fn query_words(
    surroundings: &Surroundings,
    screen: Option<&ScreenText>,
) -> (HashMap<String, u32>, HashSet<String>) {
    let labels = place::field_labels(surroundings);
    let mut texts = vec![];
    if let Some(field) = &surroundings.field_text {
        texts.extend([field.before_caret.as_str(), field.after_caret.as_str()]);
    }
    texts.extend(labels.iter().map(String::as_str));
    if let Some(screen) = screen {
        texts.extend(
            LineScope::of(surroundings)
                .lines(screen)
                .map(|line| line.text.as_str()),
        );
    }
    let mut query = HashMap::new();
    let mut add_distinctive = |word: &str| {
        if is_distinctive_word(word) {
            query.entry(word.to_owned()).or_insert(0);
        }
    };
    for text in texts {
        each_lowercase_word(text, &mut add_distinctive);
    }
    let mut title_words = HashSet::new();
    each_lowercase_word(surroundings.window_title.as_deref().unwrap_or_default(), |word| {
        if is_distinctive_word(word) && !query.contains_key(word) {
            title_words.insert(word.to_owned());
        }
    });
    for word in &title_words {
        query.insert(word.clone(), 0);
    }
    (query, title_words)
}

/// Returns the words of `query` that the text of `entry` or the message that it answers holds, each
/// one time.
fn matched_words(query: &HashMap<String, u32>, entry: &HistoryEntry) -> Vec<String> {
    let mut words = distinct_query_words(query, &entry.text);
    for word in &entry.reply_to {
        if query.contains_key(word) && !words.contains(word) {
            words.push(word.clone());
        }
    }
    words
}

/// Returns the words of `query` that `text` holds, each one time, in the order of their first
/// place in `text`.
fn distinct_query_words(query: &HashMap<String, u32>, text: &str) -> Vec<String> {
    let mut seen = Vec::<String>::new();
    each_lowercase_word(text, |word| {
        if query.contains_key(word) && !seen.iter().any(|known| known == word) {
            seen.push(word.to_owned());
        }
    });
    seen
}

/// Returns the example that `text` gives: all of it when it fits in [`EXAMPLE_TOKENS`], else its
/// start and its end with a mark between them, so that the example fits.
fn example_of(text: &str) -> String {
    if estimated_tokens(text) <= EXAMPLE_TOKENS {
        return text.to_owned();
    }
    let end = from_word(text, last_tokens(text, EXAMPLE_END_TOKENS));
    format!(
        "{} … {end}",
        start_at_word(text, first_tokens(text, EXAMPLE_START_TOKENS))
    )
}

/// Adds to `picked` the first `count` of `candidates` that do not overlap a picked entry and are no
/// near copy of one.
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
        let is_known = picked
            .iter()
            .any(|(_, entry)| entry.overlaps(candidate.1) || is_near_copy(&entry.text, &candidate.1.text));
        if is_known {
            continue;
        }
        picked.push(candidate);
        added += 1;
    }
}

/// Whether `left` and `right` differ in at most [`MIN_NEAR_COPY_EDITS`] edits, or in at most
/// [`NEAR_COPY_EDIT_PERCENT`] percent of the characters of the shorter one.
fn is_near_copy(left: &str, right: &str) -> bool {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    let shorter = left.len().min(right.len());
    let limit = MIN_NEAR_COPY_EDITS.max(shorter * NEAR_COPY_EDIT_PERCENT / 100);
    is_within_edits(&left, &right, limit)
}

/// Whether the field holds the text of `entry`, or the text of `entry` holds the text before the
/// caret of a field with at least [`MIN_COPIED_CHARS`] characters.
fn is_copy_of_field(entry: &HistoryEntry, field: Option<&FieldText>) -> bool {
    let Some(field) = field else {
        return false;
    };
    let before = field.before_caret.trim();
    let whole = format!("{}{}", field.before_caret, field.after_caret);
    whole.contains(entry.text.trim())
        || (before.chars().count() >= MIN_COPIED_CHARS && entry.text.contains(before))
}

#[cfg(test)]
mod tests;
