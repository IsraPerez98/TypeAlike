//! The earlier texts that a prompt shows as examples: the texts that match the topic of the field.

use std::collections::HashMap;
use std::mem;

use super::entry::HistoryEntry;
use super::store::SessionTexts;
use super::words::{is_distinctive_word, small_count, words};
use crate::desktop::{FieldText, ScreenText, Surroundings};
use crate::excerpt::{estimated_tokens, first_tokens, from_word, last_tokens, start_at_word};
use crate::inference::TokenCount;
use crate::place::{self, Format, Place};

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
/// the `matching_examples` best matching texts of the same place. In a place with at most
/// [`MAX_SMALL_PLACE_TEXTS`] texts, its newest texts fill the examples up to [`RECENT_IN_PLACE`].
/// The best matching texts of other places with at least [`MIN_WIDE_WORDS`] topic words fill the
/// rest of the `matching_examples`. A place without texts gets the newest texts of other places
/// with the same format in the free examples. A text matches when it shares rare words with the
/// text near the caret, the window, the labels of the field and the `screen`. Each example keeps
/// the start and the end of its text. A text that the field holds now is no example, and two
/// examples never hold the same text or near copies of one text. Among texts that match the same,
/// the text with more copies in its place comes first, so that the spelling that the writer uses
/// most stays.
pub(crate) fn examples(
    texts: &SessionTexts<'_>,
    surroundings: &Surroundings,
    screen: Option<&ScreenText>,
    matching_examples: usize,
) -> Vec<String> {
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
        .map(|(_, entry)| example_of(&entry.text))
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
}

/// How well a text matches the topic words: the sum of the weights of the topic words that it
/// holds, and the number of these words.
struct TopicMatch {
    weight: f32,
    words: usize,
}

impl Topics {
    fn of(entries: &[&HistoryEntry], surroundings: &Surroundings, screen: Option<&ScreenText>) -> Self {
        let mut holders_by_word = query_words(surroundings, screen);
        for entry in entries {
            for word in distinct_query_words(&holders_by_word, &entry.text) {
                *holders_by_word.entry(word).or_default() += 1;
            }
        }
        Self {
            entry_count: entries.len(),
            holders_by_word,
        }
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
            .map(|&candidate| {
                (
                    self.score(&candidate.1.text),
                    copies[&copy_key(candidate.1)],
                    candidate,
                )
            })
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

    /// Returns how well `text` matches. A word that few entries hold weighs more than a word that
    /// many entries hold, but each word weighs something.
    fn score(&self, text: &str) -> TopicMatch {
        let entry_count = small_count(self.entry_count);
        let words = distinct_query_words(&self.holders_by_word, text);
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
/// and the `screen`, each with no holders yet.
fn query_words(surroundings: &Surroundings, screen: Option<&ScreenText>) -> HashMap<String, u32> {
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
    let mut add_distinctive = |word: &str| {
        if is_distinctive_word(word) {
            query.entry(word.to_owned()).or_insert(0);
        }
    };
    for text in texts {
        each_lowercase_word(text, &mut add_distinctive);
    }
    query
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

/// Whether at most `limit` inserted, deleted or changed characters turn `left` into `right`.
fn is_within_edits(left: &[char], right: &[char], limit: usize) -> bool {
    if left.len().abs_diff(right.len()) > limit {
        return false;
    }
    // A cell outside the band keeps `over`, so the band never reads a stale count.
    let over = limit + 1;
    let mut previous = (0..=right.len()).map(|edits| edits.min(over)).collect::<Vec<_>>();
    let mut current = vec![over; right.len() + 1];
    for (row, &left_char) in left.iter().enumerate() {
        let first = row.saturating_sub(limit);
        let end = (row + limit + 1).min(right.len());
        current[first] = if first == 0 { (row + 1).min(over) } else { over };
        for column in first..end {
            let change = previous[column] + usize::from(left_char != right[column]);
            let edits = change.min(previous[column + 1] + 1).min(current[column] + 1);
            current[column + 1] = edits.min(over);
        }
        if current[first..=end].iter().all(|&edits| edits == over) {
            return false;
        }
        mem::swap(&mut previous, &mut current);
    }
    previous[right.len()] <= limit
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
