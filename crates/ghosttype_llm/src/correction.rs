//! The fix of a typo in the typed letters of the last word, from the text of a token that the
//! model expects there.

/// The fewest letters of a word, without its leading whitespace, that a fix can change. A shorter
/// word has many other words one edit away.
const MIN_FIXED_LETTERS: usize = 3;

/// Returns the edits that turn `left` into `right`: a swap of two letters next to each other, a
/// change, an extra or a missing letter count one edit each.
fn edits(left: &[char], right: &[char]) -> usize {
    let mut rows = vec![(0..=right.len()).collect::<Vec<_>>()];
    for (row, &left_char) in left.iter().enumerate() {
        let mut current = vec![row + 1; right.len() + 1];
        for (column, &right_char) in right.iter().enumerate() {
            let above = &rows[row];
            let change = above[column] + usize::from(left_char != right_char);
            current[column + 1] = change.min(above[column + 1] + 1).min(current[column] + 1);
            let is_swap =
                row > 0 && column > 0 && left_char == right[column - 1] && left[row - 1] == right_char;
            if is_swap {
                current[column + 1] = current[column + 1].min(rows[row - 1][column - 1] + 1);
            }
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
pub(crate) fn fixed_letters(letters: &str, piece: &str) -> Option<String> {
    let (space, typed) = split_space(letters);
    let (piece_space, written) = split_space(piece);
    if space != piece_space || typed.len() < MIN_FIXED_LETTERS || written.len() < MIN_FIXED_LETTERS {
        return None;
    }
    let fixed = if written.len() >= typed.len() {
        one_edit_away(typed.len(), written.len(), |length| {
            edits(&typed, &written[..length])
        })
        .map(|length| text(&written[..length]))
    } else {
        one_edit_away(written.len(), typed.len(), |length| {
            edits(&typed[..length], &written)
        })
        .map(|length| text(&written) + &text(&typed[length..]))
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

fn text(characters: &[char]) -> String {
    characters.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edits_between(left: &str, right: &str) -> usize {
        edits(
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
}
