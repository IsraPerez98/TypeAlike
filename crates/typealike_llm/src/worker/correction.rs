//! The fix of a typo in the typed letters of the last word, from the text of a token that the
//! model expects there.

use std::str;

use llama_cpp_2::token::LlamaToken;
use typealike_core::inference::LogProbability;

use super::logits::{most_likely_tokens, typed_log_probability};
use super::pieces::TokenPieces;

/// The fewest letters of a word, without its leading whitespace, that a fix can change. A shorter
/// word has many other words one edit away.
const MIN_FIXED_LETTERS: usize = 3;
/// The most likely tokens at the start of a word with a typo that a fix looks at.
const FIX_CANDIDATES: usize = 64;

/// Returns how many edits turn `left` into `right`: a swap of two letters next to each other, a
/// change, an extra or a missing letter count one edit each.
fn edit_distance(left: &[char], right: &[char]) -> usize {
    let mut rows = vec![(0..=right.len()).collect::<Vec<_>>()];
    for (row, &left_char) in left.iter().enumerate() {
        let mut current = vec![row + 1; right.len() + 1];
        for (column, &right_char) in right.iter().enumerate() {
            let above = &rows[row];
            let change = above[column] + usize::from(left_char != right_char);
            let is_swap =
                row > 0 && column > 0 && left_char == right[column - 1] && left[row - 1] == right_char;
            let cost = change.min(above[column + 1] + 1).min(current[column] + 1);
            let swap = is_swap.then(|| rows[row - 1][column - 1] + 1);
            current[column + 1] = swap.map_or(cost, |swap| swap.min(cost));
        }
        rows.push(current);
    }
    rows.last()
        .and_then(|row| row.last())
        .copied()
        .unwrap_or_default()
}

/// Returns the typed `letters` with one edit, so that the text of a token, `piece`, writes their
/// start, or all of them with more after. `None` when `piece` already agrees with the letters,
/// is more than one edit away, starts with other whitespace, changes the first letter, or the word
/// or its fix is short.
fn fixed_letters(letters: &str, piece: &str) -> Option<String> {
    let (space, typed) = split_space(letters);
    let (piece_space, written) = split_space(piece);
    if space != piece_space || typed.len() < MIN_FIXED_LETTERS || written.len() < MIN_FIXED_LETTERS {
        return None;
    }
    let fixed = if written.len() >= typed.len() {
        one_edit_away(typed.len(), written.len(), |length| {
            edit_distance(&typed, &written[..length])
        })
        .map(|length| collect_text(&written[..length]))
    } else {
        one_edit_away(written.len(), typed.len(), |length| {
            edit_distance(&typed[..length], &written)
        })
        .map(|length| collect_text(&written) + &collect_text(&typed[length..]))
    };
    fixed
        .filter(|word| word.chars().count() >= MIN_FIXED_LETTERS && keeps_start(&typed, word))
        .map(|word| format!("{space}{word}"))
}

/// Returns the first of `length`, one less and one more, up to `longest`, whose `edits` are one.
/// `None` when one of them has no edit, because then the text needs no fix.
fn one_edit_away(length: usize, longest: usize, edits: impl Fn(usize) -> usize) -> Option<usize> {
    let lengths = [Some(length), length.checked_sub(1), length.checked_add(1)]
        .into_iter()
        .flatten()
        .filter(|&candidate| candidate > 0 && candidate <= longest)
        .map(|candidate| (candidate, edits(candidate)))
        .collect::<Vec<_>>();
    if lengths.iter().any(|&(_, count)| count == 0) {
        return None;
    }
    lengths
        .into_iter()
        .find(|&(_, count)| count == 1)
        .map(|(candidate, _)| candidate)
}

/// Whether the `fixed` word starts with the first letter of the `typed` letters, or with their
/// first two letters swapped. A fix of the first letter more often turns a name into another word.
fn keeps_start(typed: &[char], fixed: &str) -> bool {
    let mut start = fixed.chars();
    let (first, second) = (start.next(), start.next());
    first == typed.first().copied() || (first == typed.get(1).copied() && second == typed.first().copied())
}

/// Splits the whitespace at the start of `text` from the characters of the rest.
fn split_space(text: &str) -> (&str, Vec<char>) {
    let word = text.trim_start();
    (&text[..text.len() - word.len()], word.chars().collect())
}

fn collect_text(characters: &[char]) -> String {
    characters.iter().collect()
}

/// How likely the model finds the healed text, which holds the typed letters of the last word. It
/// takes the higher of two estimates. The healing path can write a word in pieces that the model
/// seldom saw, such as a lone space and then the word. The tokens that cover the text at the first
/// step miss a word that the model splits in other places.
#[derive(Debug, Default)]
pub(super) struct TypedLetters {
    /// The sum of the log probabilities of the agreeing tokens at each healing step.
    path: f32,
    /// The log probability of the tokens that cover the healed text at the first healing step.
    cover: Option<f32>,
}

impl TypedLetters {
    /// Adds a healing step with `logits`, where the `unwritten` text stays to write.
    pub(super) fn add(&mut self, logits: &[f32], pieces: &TokenPieces, unwritten: &[u8]) {
        self.path += typed_log_probability(logits, pieces.agreeing_with(unwritten));
        self.cover
            .get_or_insert_with(|| typed_log_probability(logits, pieces.covering(unwritten)));
    }

    /// The higher of the two estimates, or zero before a healing step.
    pub(super) fn log_probability(&self) -> LogProbability {
        LogProbability(self.cover.map_or(self.path, |cover| cover.max(self.path)))
    }
}

/// Returns the most likely token among the first [`FIX_CANDIDATES`] of `logits` whose text,
/// `piece_of`, fixes the `typed` letters with one edit, and the fixed letters. The fixed letters
/// must be at least as likely as `min`, so that the fix is not a typo too. `None` when no such
/// token comes.
pub(super) fn typo_fix(
    logits: &[f32],
    pieces: &TokenPieces,
    typed: &[u8],
    min: LogProbability,
    piece_of: impl Fn(LlamaToken) -> Vec<u8>,
) -> Option<(LlamaToken, String)> {
    let typed = str::from_utf8(typed).ok()?;
    most_likely_tokens(logits, FIX_CANDIDATES)
        .into_iter()
        .filter_map(|token| {
            let piece = String::from_utf8(piece_of(token)).ok()?;
            Some((token, fixed_letters(typed, &piece)?))
        })
        .find(|(_, fixed)| typed_log_probability(logits, pieces.covering(fixed.as_bytes())) >= min.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::test_support::assert_near;

    fn edits_between(left: &str, right: &str) -> usize {
        edit_distance(
            &left.chars().collect::<Vec<_>>(),
            &right.chars().collect::<Vec<_>>(),
        )
    }

    #[test]
    fn one_swap_change_extra_or_missing_letter_is_one_edit() {
        let pairs = [
            ("dlei", "deli"),
            ("delu", "deli"),
            ("dleli", "deli"),
            ("del", "deli"),
        ];

        let distances = pairs.map(|(left, right)| edits_between(left, right));

        assert_eq!(distances, [1, 1, 1, 1]);
    }

    #[test]
    fn two_changes_are_two_edits() {
        assert_eq!(edits_between("dxlx", "deli"), 2);
    }

    #[test]
    fn token_that_writes_all_of_the_letters_fixes_them_with_its_start() {
        let fixed = fixed_letters(" dlei", " delivery");

        assert_eq!(fixed.as_deref(), Some(" deli"));
    }

    #[test]
    fn token_that_writes_a_start_of_the_letters_fixes_that_start() {
        let fixed = fixed_letters(" porveed", " prove");

        assert_eq!(fixed.as_deref(), Some(" proveed"));
    }

    #[test]
    fn token_with_a_missing_letter_fixes_the_letters() {
        let fixed = fixed_letters(" delvery", " delivery");

        assert_eq!(fixed.as_deref(), Some(" delivery"));
    }

    #[test]
    fn letters_that_the_token_writes_without_an_edit_need_no_fix() {
        let fixed = fixed_letters(" deli", " delivery");

        assert_eq!(fixed, None);
    }

    #[test]
    fn token_more_than_one_edit_away_does_not_fix_the_letters() {
        let fixed = fixed_letters(" dxlx", " delivery");

        assert_eq!(fixed, None);
    }

    #[test]
    fn word_of_fewer_than_three_letters_gets_no_fix() {
        let fixed = fixed_letters(" Ob", " Oh");

        assert_eq!(fixed, None);
    }

    #[test]
    fn fix_that_changes_the_first_letter_is_left_out() {
        let fixed = fixed_letters(" Car", " parte");

        assert_eq!(fixed, None);
    }

    #[test]
    fn fix_can_swap_the_first_two_letters() {
        let fixed = fixed_letters(" hte", " the");

        assert_eq!(fixed.as_deref(), Some(" the"));
    }

    #[test]
    fn fix_with_fewer_than_three_letters_is_left_out() {
        let fixed = fixed_letters(" Car", " artesan");

        assert_eq!(fixed, None);
    }

    #[test]
    fn token_that_starts_the_word_in_another_way_does_not_fix_the_letters() {
        let cases = [(" dlei", "deli"), ("dlei", " deli"), (" dlei", " de")];

        let fixed = cases.map(|(typed, piece)| fixed_letters(typed, piece));

        assert_eq!(fixed, [None, None, None]);
    }

    fn space_and_cost_pieces() -> TokenPieces {
        TokenPieces::from_texts(&[" ", " cost", " costume", "cost", " cat"])
    }

    #[test]
    fn typed_letters_are_as_likely_as_the_tokens_that_cover_them_when_the_healing_path_is_less_likely() {
        let pieces = space_and_cost_pieces();
        let mut typed = TypedLetters::default();

        typed.add(
            &[
                4.0_f32.ln(),
                1.0_f32.ln(),
                3.0_f32.ln(),
                1.0_f32.ln(),
                1.0_f32.ln(),
            ],
            &pieces,
            b" cost",
        );
        typed.add(
            &[
                1.0_f32.ln(),
                1.0_f32.ln(),
                1.0_f32.ln(),
                1.0_f32.ln(),
                96.0_f32.ln(),
            ],
            &pieces,
            b"cost",
        );

        assert_near(typed.log_probability().0, 0.4_f32.ln());
    }

    #[test]
    fn typed_letters_are_as_likely_as_the_healing_path_when_the_cover_is_less_likely() {
        let pieces = space_and_cost_pieces();
        let mut typed = TypedLetters::default();

        typed.add(
            &[
                8.0_f32.ln(),
                1.0_f32.ln(),
                0.0_f32.ln(),
                0.0_f32.ln(),
                1.0_f32.ln(),
            ],
            &pieces,
            b" cost",
        );
        typed.add(
            &[
                0.0_f32.ln(),
                0.0_f32.ln(),
                0.0_f32.ln(),
                1.0_f32.ln(),
                0.0_f32.ln(),
            ],
            &pieces,
            b"cost",
        );

        assert_near(typed.log_probability().0, 0.9_f32.ln());
    }

    fn delivery_pieces() -> Vec<&'static str> {
        vec![" de", " delivery", " dlei", " details", " deli", " the"]
    }

    fn piece_of(pieces: &[&str], token: LlamaToken) -> Vec<u8> {
        let index = usize::try_from(token.0).expect("a test token is never negative");
        pieces[index].as_bytes().to_vec()
    }

    #[test]
    fn typo_gets_the_fix_of_the_most_likely_token_one_edit_away() {
        let pieces = delivery_pieces();
        let logits = [2.0, 6.0, 0.0, 1.0, 1.0, 9.0];

        let fix = typo_fix(
            &logits,
            &TokenPieces::from_texts(&pieces),
            b" dlei",
            LogProbability(-11.0),
            |token| piece_of(&pieces, token),
        );

        assert_eq!(fix, Some((LlamaToken(1), " deli".to_owned())));
    }

    #[test]
    fn fix_that_also_looks_like_a_typo_is_left_out() {
        let pieces = delivery_pieces();
        let logits = [0.0, 0.0, 0.0, 0.0, 0.0, 20.0];

        let fix = typo_fix(
            &logits,
            &TokenPieces::from_texts(&pieces),
            b" dlei",
            LogProbability(-11.0),
            |token| piece_of(&pieces, token),
        );

        assert_eq!(fix, None);
    }

    #[test]
    fn letters_without_a_close_token_get_no_fix() {
        let pieces = delivery_pieces();
        let logits = [2.0, 6.0, 0.0, 1.0, 1.0, 9.0];

        let fix = typo_fix(
            &logits,
            &TokenPieces::from_texts(&pieces),
            b" qzxw",
            LogProbability(-11.0),
            |token| piece_of(&pieces, token),
        );

        assert_eq!(fix, None);
    }

    #[test]
    fn letters_without_a_healing_step_are_certain() {
        let typed = TypedLetters::default();

        assert_near(typed.log_probability().0, 0.0);
    }
}
