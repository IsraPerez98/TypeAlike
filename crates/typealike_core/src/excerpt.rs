//! The token estimate of a text, the cuts of a text to a count of tokens or characters, and the
//! text on one line.

use crate::inference::TokenCount;

/// Returns `text` with each run of whitespace as one space, and no whitespace at its ends.
pub(crate) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Returns `text` with a plain space for each no-break space. The count of characters stays the
/// same, so the Backspace keys of a replacement still delete the right characters.
pub(crate) fn plain_spaces(text: &str) -> String {
    text.replace(['\u{a0}', '\u{202f}', '\u{2007}'], " ")
}

/// Returns `text` with a plain space for each character of a private-use area of Unicode, which
/// icon fonts draw as icons. The count of characters stays the same, so the Backspace keys of a
/// replacement still delete the right characters.
pub(crate) fn icons_as_spaces(text: &str) -> String {
    text.replace(is_private_use, " ")
}

fn is_private_use(character: char) -> bool {
    matches!(
        character,
        '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}'
    )
}

/// Returns `text` without its zero-width spaces and word joiners. A web editor such as CodeMirror
/// puts them around a widget, such as the mark of a list item, never inside a typed word, so the
/// Backspace keys of the replacement of a word still delete the right characters.
pub(crate) fn without_zero_width_spaces(text: &str) -> String {
    text.replace(['\u{200b}', '\u{2060}', '\u{feff}'], "")
}

/// Returns the last `count` characters of `text`.
pub(crate) fn last_chars(text: &str, count: usize) -> &str {
    let excess = text.chars().count().saturating_sub(count);
    text.char_indices()
        .nth(excess)
        .map_or("", |(start, _)| &text[start..])
}

/// Returns about the most tokens that `text` takes in the models that TypeAlike runs. The real
/// count is lower for most text, so that a prompt within a budget fits in the context.
pub(crate) fn estimated_tokens(text: &str) -> TokenCount {
    let thirds = text.chars().map(token_thirds).sum::<usize>();
    TokenCount(u32::try_from(thirds.div_ceil(3)).unwrap_or(u32::MAX))
}

/// Returns `end`, the end of `text`, from the start of its first word when it cuts a word of
/// `text`.
pub(crate) fn from_word<'text>(text: &str, end: &'text str) -> &'text str {
    if end.len() == text.len() || text[..text.len() - end.len()].ends_with(char::is_whitespace) {
        return end.trim_start();
    }
    end.split_once(char::is_whitespace)
        .map_or("", |(_, rest)| rest.trim_start())
}

/// Returns `start`, the start of `text`, without the part of a word at its end.
pub(crate) fn start_at_word<'text>(text: &str, start: &'text str) -> &'text str {
    if text[start.len()..].starts_with(char::is_whitespace) {
        return start.trim_end();
    }
    start
        .rsplit_once(char::is_whitespace)
        .map_or("", |(whole, _)| whole.trim_end())
}

/// Returns the start of `text` that takes at most `budget` estimated tokens.
pub(crate) fn first_tokens(text: &str, budget: TokenCount) -> &str {
    &text[..fitting_bytes(text.chars(), budget)]
}

/// Returns the end of `text` that takes at most `budget` estimated tokens.
pub(crate) fn last_tokens(text: &str, budget: TokenCount) -> &str {
    &text[text.len() - fitting_bytes(text.chars().rev(), budget)..]
}

/// The thirds of a token that `character` takes at most. Code has about 3 ASCII characters for
/// each token and Chinese about 2 characters, but an emoji can take two tokens.
fn token_thirds(character: char) -> usize {
    match character.len_utf8() {
        1 => 1,
        4 => 6,
        _ => 3,
    }
}

/// Returns the bytes of the first of `characters` that fit in `budget`.
fn fitting_bytes(characters: impl Iterator<Item = char>, budget: TokenCount) -> usize {
    let mut thirds_left = usize::try_from(budget.0).unwrap_or(usize::MAX).saturating_mul(3);
    characters
        .take_while(|&character| {
            let Some(left) = thirds_left.checked_sub(token_thirds(character)) else {
                return false;
            };
            thirds_left = left;
            true
        })
        .map(char::len_utf8)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_counts_three_ascii_characters_one_other_character_or_half_an_emoji_as_a_token() {
        let cases = [
            ("", 0),
            ("abc", 1),
            ("abcd", 2),
            ("你好", 2),
            ("café", 2),
            ("👍", 2),
        ];

        let estimates = cases.map(|(text, _)| estimated_tokens(text));

        assert_eq!(estimates, cases.map(|(_, tokens)| TokenCount(tokens)));
    }

    #[test]
    fn first_tokens_keeps_the_start_that_fits() {
        let cases = [
            ("abcdefg", 2, "abcdef"),
            ("你好世界", 3, "你好世"),
            ("ab你好", 1, "ab"),
        ];

        let kept = cases.map(|(text, budget, _)| first_tokens(text, TokenCount(budget)));

        assert_eq!(kept, cases.map(|(_, _, start)| start));
    }

    #[test]
    fn last_tokens_keeps_the_end_that_fits() {
        let cases = [
            ("abcdefg", 2, "bcdefg"),
            ("你好世界", 3, "好世界"),
            ("你好ab", 1, "ab"),
        ];

        let kept = cases.map(|(text, budget, _)| last_tokens(text, TokenCount(budget)));

        assert_eq!(kept, cases.map(|(_, _, end)| end));
    }

    #[test]
    fn last_chars_keeps_the_end_of_the_text() {
        let cases = [("café au lait", 4, "lait"), ("café", 10, "café"), ("café", 0, "")];

        let kept = cases.map(|(text, count, _)| last_chars(text, count));

        assert_eq!(kept, cases.map(|(_, _, end)| end));
    }
}
