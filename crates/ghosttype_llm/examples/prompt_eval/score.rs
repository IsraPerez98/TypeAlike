//! Scores the suggestions against the text that the writer typed next.

mod comparison;
mod summary;
#[cfg(test)]
mod test_outcomes;

use serde::{Deserialize, Serialize};

pub use comparison::Comparison;
pub use summary::Summary;

/// Where the caret is when the suggestion shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseKind {
    /// Between two words: the suggestion must start the next word.
    NextWord,
    /// Inside a word: the suggestion must finish it.
    PartialWord,
    /// Inside a word with a typo: the suggestion must not finish it.
    Typo,
    /// After an Enter that did not start a new line in the field: no suggestion must show.
    NoNewLine,
}

impl CaseKind {
    /// Returns the kind of a case where the writer typed `next` after `before`.
    pub fn of(before: &str, next: &str) -> Self {
        let is_inside_word =
            before.ends_with(char::is_alphanumeric) && next.starts_with(char::is_alphanumeric);
        if is_inside_word {
            Self::PartialWord
        } else {
            Self::NextWord
        }
    }

    /// Whether a case of this kind matches when the suggestion starts with the text that the
    /// writer typed next.
    fn has_next_word(self) -> bool {
        matches!(self, Self::NextWord | Self::PartialWord)
    }
}

/// Returns whether a case of `kind` with the ghost text `suggestion` matches. A typo case matches
/// when no suggestion finishes the word with the typo, and a case without a new line when no
/// suggestion shows. Another case matches when the first word of `accepted`, the text that the
/// first Tab types, is the first word of `next`.
pub fn is_match(kind: CaseKind, suggestion: Option<&str>, accepted: &str, next: &str) -> bool {
    match kind {
        CaseKind::Typo => !suggestion.is_some_and(|text| text.starts_with(char::is_alphanumeric)),
        CaseKind::NoNewLine => suggestion.is_none(),
        CaseKind::NextWord | CaseKind::PartialWord => is_next_word(accepted, next),
    }
}

/// Returns whether the first word of `accepted` is the first word of `next`. Signs after the word,
/// such as a period or a comma, do not count.
fn is_next_word(accepted: &str, next: &str) -> bool {
    let word = next_word(accepted);
    !word.trim().is_empty() && word == next_word(next)
}

/// Returns the first word of `next`, with the whitespace before it and without signs after it.
pub fn next_word(next: &str) -> &str {
    without_trailing_signs(first_word(next))
}

/// Returns the words of `text`: a word starts at each whitespace character after a character that
/// is not whitespace. The model runner splits only at ASCII whitespace, so the two agree only on
/// text without other whitespace, such as a no-break space.
pub fn words(text: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut start = 0;
    for ((offset, before), character) in text.char_indices().zip(text.chars().skip(1)) {
        if !before.is_whitespace() && character.is_whitespace() {
            let end = offset + before.len_utf8();
            words.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        words.push(&text[start..]);
    }
    words
}

/// Returns the first word of `text`, with the whitespace before it.
fn first_word(text: &str) -> &str {
    let rest = text
        .trim_start()
        .trim_start_matches(|character: char| !character.is_whitespace());
    &text[..text.len() - rest.len()]
}

/// Returns `word` up to its last letter or digit, or all of `word` when it has none.
fn without_trailing_signs(word: &str) -> &str {
    word.char_indices()
        .rfind(|(_, character)| character.is_alphanumeric())
        .map_or(word, |(start, character)| &word[..start + character.len_utf8()])
}

/// What one case gave.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    /// The id of the case.
    pub id: String,
    /// Where the caret is.
    pub kind: CaseKind,
    /// The ghost text, or `None` when none shows.
    pub suggestion: Option<String>,
    /// Whether the case matches, as [`is_match`] tells.
    pub is_match: bool,
    /// The milliseconds from the key to the ghost text.
    pub millis: u64,
    /// The milliseconds from the key to the ghost text when the app prepared the prompt before the
    /// key, or `None` in results without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warm_millis: Option<u64>,
    /// The tokens of the prompt, or `None` when the case has no prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u64>,
    /// The natural log of the probability of the next word after the prompt, or `None` when the
    /// case has no prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_probability: Option<f64>,
    /// The natural log of the probability that the model gave to the suggestion up to the end of
    /// each shown word, as the model runner gave it. It is empty when no completion of the model
    /// shows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cumulative_log_probabilities: Vec<f64>,
    /// The shown words against the words of the text that the writer typed next.
    #[serde(default)]
    pub words: WordCounts,
    /// The natural log of the probability that the model gave to the typed letters of the last
    /// word, as the model runner gave it, or `None` when no completion came.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed_log_probability: Option<f64>,
    /// The word that the fix of a typo writes, as [`fixed_word`] gives it, or `None` when no fix
    /// shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_word: Option<String>,
    /// Whether `fixed_word` is the word that the writer meant, as [`intended_word`] gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_fix_right: Option<bool>,
}

/// Returns the word that the fix of a typo writes, from the `word` that the ghost text shows: its
/// letters and digits, without the signs after them.
pub fn fixed_word(word: &str) -> &str {
    word_start(word.trim_start())
}

/// Returns the word that the writer meant: the letters and digits at the end of `before`, the
/// text before the caret without the typo, and at the start of `next`.
pub fn intended_word(before: &str, next: &str) -> String {
    let end = before
        .char_indices()
        .rev()
        .take_while(|(_, character)| character.is_alphanumeric())
        .last()
        .map_or(before.len(), |(start, _)| start);
    format!("{}{}", &before[end..], word_start(next))
}

/// Returns the letters and digits at the start of `text`.
fn word_start(text: &str) -> &str {
    let end = text
        .find(|character: char| !character.is_alphanumeric())
        .unwrap_or(text.len());
    &text[..end]
}

/// The words of a suggestion against the words of the text that the writer typed next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WordCounts {
    /// The words before the first word that differs from the word of the text at its place.
    pub right: usize,
    /// The words from the first word that differs on. A word after the end of the text counts only
    /// after a wrong word.
    pub wrong: usize,
}

impl WordCounts {
    /// Compares the words of `suggestion` with the words of `next`. Signs after a word do not
    /// count.
    pub fn of(suggestion: &str, next: &str) -> Self {
        let shown = words(suggestion);
        let typed = words(next);
        let right = shown
            .iter()
            .zip(&typed)
            .take_while(|(shown, typed)| without_trailing_signs(shown) == without_trailing_signs(typed))
            .count();
        let has_wrong_word = right < shown.len() && right < typed.len();
        Self {
            right,
            wrong: if has_wrong_word { shown.len() - right } else { 0 },
        }
    }

    /// As [`WordCounts::of`], for a `gap` that text of the field follows on its line: each word of
    /// `suggestion` after the end of `gap` is wrong.
    pub fn in_gap(suggestion: &str, gap: &str) -> Self {
        let right = Self::of(suggestion, gap).right;
        Self {
            right,
            wrong: words(suggestion).len() - right,
        }
    }

    /// Returns the counts when only the first `shown` words of the suggestion show.
    fn of_first(self, shown: usize) -> Self {
        Self {
            right: self.right.min(shown),
            wrong: if self.wrong > 0 {
                shown.saturating_sub(self.right)
            } else {
                0
            },
        }
    }
}

/// Returns the sum of the first `log_probabilities` up to each of the first `shown` words.
pub fn cumulative_log_probabilities(log_probabilities: &[f64], shown: usize) -> Vec<f64> {
    log_probabilities
        .iter()
        .take(shown)
        .scan(0.0, |total, log_probability| {
            *total += log_probability;
            Some(*total)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::test_outcomes::scored;

    #[test]
    fn caret_after_letters_and_before_letters_is_inside_a_word() {
        let cases = [
            ("the quick re", "ply and"),
            ("la cotiza", "ción que"),
            ("in 20", "26 we"),
        ];

        let kinds = cases.map(|(before, next)| CaseKind::of(before, next));

        assert_eq!(kinds, cases.map(|_| CaseKind::PartialWord));
    }

    #[test]
    fn caret_next_to_a_space_or_a_sign_is_between_words() {
        let cases = [
            ("the quick ", "reply and"),
            ("Thanks for the", " meeting"),
            ("How are you", "? Let me"),
            ("Hi Ana,", " thanks"),
        ];

        let kinds = cases.map(|(before, next)| CaseKind::of(before, next));

        assert_eq!(kinds, cases.map(|_| CaseKind::NextWord));
    }

    #[test]
    fn accepted_word_that_is_the_next_word_matches() {
        let cases = [
            ("ply", "ply and the files"),
            (" meeting", " meeting at noon"),
            ("?", "? Let me know"),
        ];

        let matches = cases.map(|(accepted, next)| is_next_word(accepted, next));

        assert_eq!(matches, cases.map(|_| true));
    }

    #[test]
    fn accepted_signs_with_the_word_after_them_match_on_the_signs() {
        let cases = [(", I", ","), ("? Let", "? Let me know")];

        let matches = cases.map(|(accepted, next)| is_next_word(accepted, next));

        assert_eq!(matches, cases.map(|_| true));
    }

    #[test]
    fn signs_after_the_word_do_not_count() {
        let cases = [
            ("rt.", "rt or some cookies"),
            ("tomorrow", "tomorrow."),
            ("do\".", "do\" y no me deja"),
        ];

        let matches = cases.map(|(accepted, next)| is_next_word(accepted, next));

        assert_eq!(matches, cases.map(|_| true));
    }

    #[test]
    fn accepted_word_that_differs_from_the_next_word_does_not_match() {
        let cases = [
            ("plied", "ply and the files"),
            ("meeting", " meeting at noon"),
            ("the", "then we"),
            ("1Z999AA1", "1Z999AA10123456784. It"),
            (".", "? Let me know"),
            ("", "the"),
            (" ", " the"),
        ];

        let matches = cases.map(|(accepted, next)| is_next_word(accepted, next));

        assert_eq!(matches, cases.map(|_| false));
    }

    #[test]
    fn next_word_keeps_the_space_before_it_and_drops_the_signs_after_it() {
        let cases = [
            (" meeting at noon", " meeting"),
            ("ción que", "ción"),
            ("tomorrow.", "tomorrow"),
            ("? Let", "?"),
        ];

        let words = cases.map(|(next, _)| next_word(next));

        assert_eq!(words, cases.map(|(_, word)| word));
    }

    #[test]
    fn typo_case_matches_without_a_suggestion_that_finishes_the_word() {
        let cases = [
            (Some("cia antes del miércoles"), false),
            (Some(" antes"), true),
            (Some("?"), true),
            (None, true),
        ];

        let matches = cases.map(|(suggestion, _)| is_match(CaseKind::Typo, suggestion, "cia", "cia antes"));

        assert_eq!(matches, cases.map(|(_, is_right)| is_right));
    }

    #[test]
    fn case_without_a_new_line_matches_without_a_suggestion() {
        let suggestions = [None, Some(" Sara")];

        let matches = suggestions.map(|suggestion| is_match(CaseKind::NoNewLine, suggestion, "", ""));

        assert_eq!(matches, [true, false]);
    }

    #[test]
    fn other_cases_match_when_the_accepted_word_is_the_next_word() {
        let kinds = [CaseKind::NextWord, CaseKind::PartialWord];

        let matches = kinds.map(|kind| is_match(kind, Some(" meeting at"), " meeting", " meeting at noon"));

        assert_eq!(matches, [true, true]);
    }

    #[test]
    fn word_of_a_fix_leaves_out_the_signs_after_it() {
        let word = fixed_word("delivery,");

        assert_eq!(word, "delivery");
    }

    #[test]
    fn intended_word_is_the_last_word_without_the_typo_and_its_rest() {
        let word = intended_word("the new deli", "very, soon");

        assert_eq!(word, "delivery");
    }

    #[test]
    fn log_probability_reads_back_from_json_as_written() {
        let outcome = scored("a", true, -1.142_062_436_409_791_3);

        let read_back = serde_json::from_str::<Outcome>(&serde_json::to_string(&outcome).unwrap()).unwrap();

        assert_eq!(read_back, outcome);
    }

    #[test]
    fn word_starts_at_whitespace_after_a_word() {
        let cases: [(&str, &[&str]); 4] = [
            ("very soon, thanks", &["very", " soon,", " thanks"]),
            (" meeting at", &[" meeting", " at"]),
            ("Hi  there", &["Hi", "  there"]),
            ("", &[]),
        ];

        let split = cases.map(|(text, _)| words(text));

        assert_eq!(split, cases.map(|(_, words)| words.to_vec()));
    }

    #[test]
    fn word_also_starts_at_whitespace_that_is_not_ascii() {
        let split = words("see\u{a0}you\u{3000}soon");

        assert_eq!(split, ["see", "\u{a0}you", "\u{3000}soon"]);
    }

    #[test]
    fn words_count_as_right_up_to_the_first_wrong_word_and_wrong_from_it() {
        let cases = [
            ("very soon, thanks", "very soon. Thanks a lot", 2, 1),
            (" the meeting today", " the report today", 1, 2),
            ("ply and the files", "ply and", 2, 0),
            ("plied", "ply and", 0, 1),
        ];

        let counts = cases.map(|(suggestion, next, _, _)| WordCounts::of(suggestion, next));

        assert_eq!(
            counts,
            cases.map(|(_, _, right, wrong)| WordCounts { right, wrong })
        );
    }

    #[test]
    fn word_after_the_end_of_a_gap_counts_as_wrong() {
        let cases = [
            ("dimensions of the shade?", "height and the width", 0, 4),
            ("jueves 21 de noviembre", "jueves 21", 2, 2),
            ("jueves 21", "jueves 21", 2, 0),
            ("jueves", "jueves 21", 1, 0),
        ];

        let counts = cases.map(|(suggestion, gap, _, _)| WordCounts::in_gap(suggestion, gap));

        assert_eq!(
            counts,
            cases.map(|(_, _, right, wrong)| WordCounts { right, wrong })
        );
    }

    #[test]
    fn cumulative_log_probability_follows_each_shown_word() {
        let cumulative = cumulative_log_probabilities(&[-1.0, -0.5, -2.0], 2);

        assert_eq!(cumulative, [-1.0, -1.5]);
    }
}
