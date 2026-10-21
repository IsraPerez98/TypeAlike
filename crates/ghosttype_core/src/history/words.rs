//! The words of a text, the runs of characters, the distinctive words and the common words.

use std::ops::Range;

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
/// The fewest letters of a distinctive word.
const MIN_WORD_CHARS: usize = 3;

/// Returns the words of `text` in their spelling: its runs of letters and apostrophes.
pub(super) fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|character: char| !(character.is_alphabetic() || is_apostrophe(character)))
        .filter(|word| !word.is_empty())
}

/// Whether `character` is a straight or a curly apostrophe.
pub(crate) fn is_apostrophe(character: char) -> bool {
    matches!(character, '\'' | '’')
}

/// The spans of the runs of the characters of `text` that `is_part` accepts.
pub(super) fn runs_of(text: &str, is_part: fn(char) -> bool) -> Vec<Range<usize>> {
    let mut spans = Vec::<Range<usize>>::new();
    for (index, character) in text.char_indices().filter(|&(_, character)| is_part(character)) {
        let end = index + character.len_utf8();
        match spans.last_mut() {
            Some(span) if span.end == index => span.end = end,
            _ => spans.push(index..end),
        }
    }
    spans
}

/// Whether `lowercase` is a distinctive word: it has at least [`MIN_WORD_CHARS`] letters and is
/// not one of the [`COMMON_WORDS`].
pub(super) fn is_distinctive_word(lowercase: &str) -> bool {
    lowercase.chars().count() >= MIN_WORD_CHARS && !COMMON_WORDS.contains(&lowercase)
}

/// Returns `count` as a float, at most [`u16::MAX`].
pub(super) fn small_count(count: impl TryInto<u16>) -> f32 {
    f32::from(count.try_into().unwrap_or(u16::MAX))
}
