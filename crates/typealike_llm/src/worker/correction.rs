//! The fix of a typo in the typed letters of the last word, from the text of a token that the
//! model expects there.

use std::str;

use llama_cpp_2::token::LlamaToken;
use typealike_core::inference::{LogProbability, TypoCheck};

use super::logits::{most_likely_tokens, typed_log_probability};
use super::pieces::TokenPieces;

/// The fewest letters of a word, without its leading whitespace, that a fix can change. A shorter
/// word has many other words one edit away.
const MIN_FIXED_LETTERS: usize = 3;
/// The most likely tokens at the start of a word with a typo that a fix looks at.
const FIX_CANDIDATES: usize = 64;
/// The most that the typed letters can be less likely than the letters of their best fix, in
/// natural log units, before they count as a typo: the fix is then about 20 times as likely.
const MAX_GAP_TO_FIX: f32 = 3.0;
/// The fewest typed letters, without the whitespace before them, that the gap to their best fix
/// can take for a typo. Fewer letters, such as the start of a word, have many words one edit away.
const MIN_GAP_LETTERS: usize = 5;

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

/// The fix of the typed letters of the last word with one edit.
#[derive(Debug, PartialEq)]
pub(super) struct TypoFix {
    /// The token that starts the fixed letters.
    pub(super) token: LlamaToken,
    /// The fixed letters, with the whitespace before them.
    pub(super) letters: String,
    /// How likely the model finds the fixed letters, as the tokens that cover them.
    pub(super) log_probability: LogProbability,
}

/// Returns the fix of the `typed` letters with one edit whose fixed letters the model finds most
/// likely, among the fixes that the texts (`piece_of`) of the first [`FIX_CANDIDATES`] tokens of
/// `logits` give. A short token can fix the start of the letters and keep a typo after it, and
/// such fixed letters are less likely than the letters of a longer token. Of two fixes with the
/// same fixed letters, the fix of the more likely token comes. The fixed letters must be at least
/// as likely as `min`, so that the fix is not a typo too. `None` when no such token comes.
pub(super) fn typo_fix(
    logits: &[f32],
    pieces: &TokenPieces,
    typed: &[u8],
    min: LogProbability,
    piece_of: impl Fn(LlamaToken) -> Vec<u8>,
) -> Option<TypoFix> {
    let typed = str::from_utf8(typed).ok()?;
    if typed.trim_start().chars().count() < MIN_FIXED_LETTERS {
        return None;
    }
    most_likely_tokens(logits, FIX_CANDIDATES)
        .into_iter()
        .filter_map(|token| {
            let piece = String::from_utf8(piece_of(token)).ok()?;
            let letters = fixed_letters(typed, &piece)?;
            let log_probability =
                LogProbability(typed_log_probability(logits, pieces.covering(letters.as_bytes())));
            Some(TypoFix {
                token,
                letters,
                log_probability,
            })
        })
        .filter(|fix| fix.log_probability >= min)
        .reduce(|best, fix| {
            if fix.log_probability > best.log_probability {
                fix
            } else {
                best
            }
        })
}

/// Returns `check` for the `typed` letters with their best `fix`. With [`TypoCheck::Fix`] and at
/// least [`MIN_GAP_LETTERS`] typed letters, the letters must also be at most [`MAX_GAP_TO_FIX`]
/// less likely than the fixed letters, when that minimum is higher. The fix is then much more
/// likely than the typed letters, so they hold a typo, although the model can find a typo after
/// the first token of a word fairly likely.
pub(super) fn with_fix_gap(check: TypoCheck, typed: &[u8], fix: Option<&TypoFix>) -> TypoCheck {
    let letters = str::from_utf8(typed).map_or(0, |text| text.trim_start().chars().count());
    match (check, fix) {
        (TypoCheck::Fix(min), Some(fix)) if letters >= MIN_GAP_LETTERS => {
            TypoCheck::Fix(LogProbability(min.0.max(fix.log_probability.0 - MAX_GAP_TO_FIX)))
        }
        _ => check,
    }
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

        assert_eq!(
            fix.map(|fix| (fix.token, fix.letters)),
            Some((LlamaToken(1), " deli".to_owned()))
        );
    }

    #[test]
    fn fix_with_the_most_likely_fixed_letters_wins_over_the_fix_of_a_more_likely_token() {
        let pieces = [" par", " paro", " problema"];
        let logits = [3.0, -5.0, 2.0];

        let fix = typo_fix(
            &logits,
            &TokenPieces::from_texts(&pieces),
            b" probelma",
            LogProbability(-11.0),
            |token| piece_of(&pieces, token),
        );

        assert_eq!(fix.map(|fix| fix.letters), Some(" problema".to_owned()));
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

    fn fix_with(log_probability: f32) -> TypoFix {
        TypoFix {
            token: LlamaToken(1),
            letters: " delivery".to_owned(),
            log_probability: LogProbability(log_probability),
        }
    }

    const FIX_CHECK: TypoCheck = TypoCheck::Fix(LogProbability(-11.0));

    #[test]
    fn fix_check_with_a_much_more_likely_fix_takes_letters_far_below_it_for_a_typo() {
        let check = with_fix_gap(FIX_CHECK, b" delvery", Some(&fix_with(-1.0)));

        assert_eq!(check, TypoCheck::Fix(LogProbability(-1.0 - MAX_GAP_TO_FIX)));
    }

    #[test]
    fn fix_check_keeps_its_minimum_when_the_fix_is_less_likely() {
        let check = with_fix_gap(FIX_CHECK, b" delvery", Some(&fix_with(-10.0)));

        assert_eq!(check, FIX_CHECK);
    }

    #[test]
    fn fix_check_of_fewer_than_five_typed_letters_keeps_its_minimum() {
        let check = with_fix_gap(FIX_CHECK, b" dlei", Some(&fix_with(-1.0)));

        assert_eq!(check, FIX_CHECK);
    }

    #[test]
    fn check_without_a_fix_or_that_fixes_nothing_keeps_its_minimum() {
        let cases = [
            (FIX_CHECK, None),
            (TypoCheck::Hide(LogProbability(-11.0)), Some(fix_with(-1.0))),
            (TypoCheck::Off, Some(fix_with(-1.0))),
        ];

        let checks = cases
            .each_ref()
            .map(|(check, fix)| with_fix_gap(*check, b" delvery", fix.as_ref()));

        assert_eq!(checks, cases.map(|(check, _)| check));
    }

    #[test]
    fn letters_without_a_healing_step_are_certain() {
        let typed = TypedLetters::default();

        assert_near(typed.log_probability().0, 0.0);
    }
}
