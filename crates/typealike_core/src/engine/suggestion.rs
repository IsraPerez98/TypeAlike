//! Filters the text of a completion and splits a suggestion into the parts that Tab types.

use crate::history::is_apostrophe;
use crate::inference::Correction;
use crate::prompt;

use super::WordFix;

/// The last words before the caret that a suggestion copies when it holds them in a row. Fewer
/// words are often the correct phrase again.
const COPIED_WORDS: usize = 4;
const MAX_PATTERN_WORDS: usize = 3;
const PATTERN_REPEATS: usize = 3;
const CLOSING_PUNCTUATION: [char; 7] = [',', '.', ';', ':', '!', '?', '…'];
/// The words and signs in a row that a suggestion shares with the text after the caret, from where
/// it writes that text again. One shared word often only starts another sentence.
const SHARED_UNITS: usize = 2;
/// The signs that can close a word: closing punctuation, quotes and brackets. The apostrophe is not
/// one, because it also ends a possessive, as in "the students'".
const CLOSING_SIGNS: [char; 13] = [',', '.', ';', ':', '!', '?', '…', '"', '”', ')', ']', '}', '»'];
/// The signs that can follow a word with a typo, before at most one space, and keep its fix.
const FIX_KEEPING_SIGNS: [char; 5] = [',', ';', ':', '!', '?'];

/// The words of a line of the prompt frame in a row that make a copy of the line. Fewer words are
/// often writing, such as "The part of my".
const FRAME_COPY_WORDS: usize = 5;
/// The fewest words of a shorter frame line that count as a copy of it.
const MIN_FRAME_COPY_WORDS: usize = 3;
/// The signs that end a sentence.
const SENTENCE_ENDS: [char; 7] = ['.', '!', '?', '…', '。', '！', '？'];
/// The brackets that open a note about a text, such as its translation, after the end of a sentence.
const NOTE_BRACKETS: [char; 2] = ['(', '（'];

/// Returns the first line of `completion` up to a placeholder, and up to where it writes
/// `text_after` again, as ghost text after `context`, or `None` when it has no text. Spaces that
/// would double the space at the end of `context`, or trail, are dropped.
pub(super) fn suggestion_after(context: &str, completion: &str, text_after: &str) -> Option<String> {
    let first_line = completion.lines().next().unwrap_or_default();
    let line = if context.ends_with(char::is_whitespace) {
        first_line.trim_start()
    } else {
        first_line
    };
    let suggestion = before_text_after(before_placeholder(line), text_after).trim_end();
    (!suggestion.is_empty()).then(|| suggestion.to_owned())
}

/// Returns `text` up to where it writes the line of `text_after` again, which the model often does
/// after the words that fill the gap before it. The text on the next lines does not continue the
/// line of the caret, so it does not count.
fn before_text_after<'text>(text: &'text str, text_after: &str) -> &'text str {
    let line_after = text_after.lines().next().unwrap_or_default().trim_start();
    start_of_line_after(text, line_after).map_or(text, |end| &text[..end])
}

/// Returns the index of the first word of `text`, or of the closing punctuation that starts
/// `line_after`, from where `text` writes `line_after` again: their words agree up to the end of
/// the shorter, or in at least [`SHARED_UNITS`] words and signs in a row.
fn start_of_line_after(text: &str, line_after: &str) -> Option<usize> {
    let words_after = words(line_after).collect::<Vec<_>>();
    let first = line_after.chars().next()?;
    let starts_with_sign = CLOSING_PUNCTUATION.contains(&first);
    let writes_line_after_from = |start: usize| {
        let rest = words(&text[start..]).collect::<Vec<_>>();
        let shared_words = rest
            .iter()
            .zip(&words_after)
            .take_while(|(word, word_after)| word == word_after)
            .count();
        let shared_units = shared_words + usize::from(starts_with_sign);
        shared_units > 0
            && (shared_words == rest.len().min(words_after.len()) || shared_units >= SHARED_UNITS)
    };
    if starts_with_sign {
        return text
            .match_indices(first)
            .map(|(index, _)| index)
            .filter(|&index| {
                text[index + first.len_utf8()..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace)
            })
            .find(|&index| writes_line_after_from(index));
    }
    word_starts(text).find(|&start| writes_line_after_from(start))
}

/// Returns the byte index of each run of characters without whitespace in `text`.
pub(super) fn word_starts(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.char_indices()
        .filter(|&(index, character)| {
            !character.is_whitespace() && text[..index].chars().next_back().is_none_or(char::is_whitespace)
        })
        .map(|(index, _)| index)
}

/// Returns `text` up to the first word after an opening square bracket, such as "[Your Name]",
/// which a model writes in place of a fact that it does not know. The token limit can cut the
/// placeholder before its closing bracket.
fn before_placeholder(text: &str) -> &str {
    text.match_indices('[')
        .find(|&(start, _)| text[start + 1..].starts_with(char::is_alphanumeric))
        .map_or(text, |(start, _)| &text[..start])
}

/// Returns the ghost text that `completion` gives after the `typed` text, which is empty for a
/// bad suggestion, or `None` when it adds nothing after `typed`. `text_before`, the text before
/// the caret, ends with `typed`, and the request continued the text before `typed` after
/// `preface`.
pub(super) fn suggestion_after_typed(
    text_before: &str,
    text_after: &str,
    typed: &str,
    completion: &str,
    preface: &str,
) -> Option<String> {
    let context = text_before.strip_suffix(typed)?;
    let suggestion = suggestion_after(context, completion, text_after)?;
    if is_bad_suggestion(&suggestion, context, text_after, preface) {
        return Some(String::new());
    }
    let rest = suggestion.strip_prefix(typed).filter(|rest| !rest.is_empty())?;
    Some(rest.to_owned())
}

/// Whether `suggestion` after `context` has a shape that is wrong almost each time: no letter or
/// digit, closing punctuation after a space, the start of `text_after` again, a copy of the last
/// words of `context`, a loop of a few words, a copy of a line of the frame of `preface`, or a
/// bracket after the end of a sentence, which opens a note about the text and not the text.
pub(super) fn is_bad_suggestion(suggestion: &str, context: &str, text_after: &str, preface: &str) -> bool {
    let has_no_word = !suggestion.chars().any(char::is_alphanumeric);
    let closes_after_space =
        context.ends_with(char::is_whitespace) && suggestion.starts_with(CLOSING_PUNCTUATION);
    let suggested = words(suggestion).collect::<Vec<_>>();
    has_no_word
        || closes_after_space
        || repeats_text_after(&suggested, text_after)
        || copies_last_words(&suggested, context)
        || repeats_a_pattern(&suggested)
        || copies_frame_line(&suggested, suggestion, context, preface)
        || opens_note_after_sentence(suggestion, context)
}

fn opens_note_after_sentence(suggestion: &str, context: &str) -> bool {
    context.trim_end().ends_with(SENTENCE_ENDS) && suggestion.trim_start().starts_with(NOTE_BRACKETS)
}

/// Whether `suggestion` after `context` copies a line of the frame of `preface`: it holds
/// [`FRAME_COPY_WORDS`] words of the line in a row, or it starts the line of the caret as the line
/// starts, with its first [`FRAME_COPY_WORDS`] words or with all the words of a shorter line of at
/// least [`MIN_FRAME_COPY_WORDS`]. In the second case, the suggestion must add one of these words.
fn copies_frame_line(suggested: &[String], suggestion: &str, context: &str, preface: &str) -> bool {
    let caret_line = context.rsplit('\n').next().unwrap_or_default();
    let typed_words = words(caret_line).count();
    let line = words(&format!("{caret_line}{suggestion}")).collect::<Vec<_>>();
    prompt::frame_lines(preface).iter().any(|frame_line| {
        let frame_words = words(frame_line).collect::<Vec<_>>();
        let start = frame_words.len().min(FRAME_COPY_WORDS);
        let starts_as_frame_line =
            start >= MIN_FRAME_COPY_WORDS && typed_words < start && line.starts_with(&frame_words[..start]);
        starts_as_frame_line
            || frame_words
                .windows(FRAME_COPY_WORDS)
                .any(|run| suggested.windows(FRAME_COPY_WORDS).any(|copy| copy == run))
    })
}

/// Returns the words of `text` in lowercase: its runs of letters, digits and apostrophes.
pub(super) fn words(text: &str) -> impl DoubleEndedIterator<Item = String> + '_ {
    text.split(|character: char| !(character.is_alphanumeric() || is_apostrophe(character)))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

/// Whether `suggested`, the words of a suggestion, and the words of `text_after` are the same up to
/// the end of the shorter.
fn repeats_text_after(suggested: &[String], text_after: &str) -> bool {
    let after = words(text_after).take(suggested.len()).collect::<Vec<_>>();
    !after.is_empty() && suggested.starts_with(&after)
}

fn copies_last_words(suggested: &[String], context: &str) -> bool {
    let mut last_words = words(context).rev().take(COPIED_WORDS).collect::<Vec<_>>();
    last_words.reverse();
    last_words.len() == COPIED_WORDS
        && suggested
            .windows(COPIED_WORDS)
            .any(|run| run == last_words.as_slice())
}

/// Whether `suggested`, the words of a suggestion, has a pattern of one to [`MAX_PATTERN_WORDS`]
/// words [`PATTERN_REPEATS`] times in a row.
fn repeats_a_pattern(suggested: &[String]) -> bool {
    (1..=MAX_PATTERN_WORDS).any(|pattern_words| {
        suggested
            .windows(pattern_words * PATTERN_REPEATS)
            .any(|run| run[pattern_words..] == run[..run.len() - pattern_words])
    })
}

/// Returns `text` up to its last whitespace, without the word after it, which can still grow.
pub(super) fn complete_words(text: &str) -> &str {
    text.rfind(char::is_whitespace).map_or("", |end| &text[..end])
}

/// Returns the fix of the typo that `correction` gives, with `suggestion`, the ghost text after the
/// fixed letters. The fixed word ends at the first whitespace of `suggestion`, before the
/// [`CLOSING_SIGNS`] at its end.
pub(super) fn word_fix(correction: &Correction, suggestion: &str) -> WordFix {
    let word_end = suggestion.find(char::is_whitespace).unwrap_or(suggestion.len());
    let unclosed_end = suggestion[..word_end].trim_end_matches(CLOSING_SIGNS).len();
    let (word_rest, rest) = suggestion.split_at(unclosed_end);
    WordFix {
        typo: correction.typed.trim_start().to_owned(),
        word: format!("{}{word_rest}", correction.fixed.trim_start()),
        rest: rest.to_owned(),
    }
}

/// Returns `fix` after the user typed `character` after its typo, or `None` when `character` does
/// not keep it. A space or one of [`FIX_KEEPING_SIGNS`] keeps it, but only one sign and then one
/// space. The typo and the word then end with `character`, so that Tab deletes it and types it
/// again. The rest keeps its text after `character`, and a rest that does not start with it goes.
pub(super) fn fix_after_delimiter(fix: &WordFix, character: char) -> Option<WordFix> {
    let is_sign = FIX_KEEPING_SIGNS.contains(&character);
    let is_open = !fix.typo.ends_with(' ');
    let is_kept = is_open && (character == ' ' || (is_sign && !fix.typo.ends_with(FIX_KEEPING_SIGNS)));
    is_kept.then(|| WordFix {
        typo: format!("{}{character}", fix.typo),
        word: format!("{}{character}", fix.word),
        rest: fix.rest.strip_prefix(character).unwrap_or_default().to_owned(),
    })
}

/// Returns the space or the sign at the end of the typo of `fix` that the user typed after its
/// letters, which is empty when the fix kept none.
pub(super) fn kept_delimiter(fix: &WordFix) -> &str {
    let letters = fix.typo.trim_end_matches(' ').trim_end_matches(FIX_KEEPING_SIGNS);
    &fix.typo[letters.len()..]
}

/// Whether `before_caret`, a read of the field text before the caret, ends with the typo of `fix`.
/// A read from before the kept space or sign ends with less of them, and still counts.
pub(super) fn ends_with_typo(before_caret: &str, fix: &WordFix) -> bool {
    let letters_end = fix.typo.len() - kept_delimiter(fix).len();
    fix.typo[letters_end..]
        .char_indices()
        .map(|(index, _)| letters_end + index)
        .chain([fix.typo.len()])
        .any(|end| before_caret.ends_with(&fix.typo[..end]))
}

/// Splits `suggestion` into the part that Tab types and the rest. The part is the first word with
/// the whitespace before it, without the [`CLOSING_SIGNS`] at its end, which the next Tab types.
/// A word of only signs is one part.
pub(crate) fn split_next_part(suggestion: &str) -> (&str, &str) {
    let rest = suggestion
        .trim_start()
        .trim_start_matches(|character: char| !character.is_whitespace());
    let word = &suggestion[..suggestion.len() - rest.len()];
    let unclosed_word = word.trim_end_matches(CLOSING_SIGNS);
    let part = if unclosed_word.trim().is_empty() {
        word
    } else {
        unclosed_word
    };
    suggestion.split_at(part.len())
}
