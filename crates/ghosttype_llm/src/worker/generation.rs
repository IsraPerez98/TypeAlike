//! What a completion takes and gives: its limits, its progress, the healed text and the added
//! tokens, with the text that they add.

use std::str;

use ghosttype_core::inference::{Correction, LogProbability};
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::token_type::{LlamaTokenAttr, LlamaTokenAttrs};
use llama_cpp_2::vocab::LlamaVocab;

use super::pieces::TokenPieces;
use super::words::starts_word;

/// The most text at the end of a prompt that generation takes back and writes again.
const MAX_HEALED_BYTES: usize = 24;

/// Where a completion stops adding tokens.
#[derive(Debug, Clone, Copy)]
pub(super) struct Limits {
    pub(super) max_added: usize,
    /// The least likely that the added text can get after its first word.
    pub(super) min_log_probability: Option<LogProbability>,
    /// The least likely that the healed text can be before the completion adds no text.
    pub(super) min_typed_log_probability: Option<LogProbability>,
    /// Whether the completion fixes a typo that `min_typed_log_probability` finds.
    pub(super) fixes_typos: bool,
}

/// What a running completion asks and tells its job.
pub(super) struct Progress<'job> {
    /// Tells that a newer completion waits, so that this completion must stop.
    pub(super) is_outdated: &'job mut dyn FnMut() -> bool,
    /// Receives the text that the completion added so far.
    pub(super) send_partial: &'job mut dyn FnMut(String),
}

/// The text that the first added tokens write again, and the text of each token, which finds the
/// tokens that agree with it.
pub(super) struct Healing<'text> {
    pub(super) pieces: &'text TokenPieces,
    pub(super) text: &'text [u8],
    /// The token that the first step takes, from the fix of a typo, if any.
    pub(super) first_token: Option<LlamaToken>,
}

/// The tokens that a completion added, how likely the model found each word of them, and how likely
/// it found the healed text.
pub(super) struct AddedTokens {
    pub(super) tokens: Vec<LlamaToken>,
    pub(super) word_log_probabilities: Vec<LogProbability>,
    /// How likely the model found the healed text, up to the stop for a typo.
    pub(super) typed_log_probability: LogProbability,
    /// The fix of a typo in the healed text, which the tokens continue, if any.
    pub(super) correction: Option<Correction>,
}

/// Removes the tokens of the last word from the end of `prompt_tokens`, and returns their text.
/// It stops at the token that starts the word, at markup, at a line break, and before the first
/// token, so the prompt never becomes empty. A line break ends a word, and the model writes too
/// many line breaks when it writes the last ones again.
pub(super) fn take_back_last_word(vocab: &LlamaVocab<'_>, prompt_tokens: &mut Vec<LlamaToken>) -> Vec<u8> {
    let mut text = Vec::new();
    while text.len() < MAX_HEALED_BYTES && prompt_tokens.len() > 1 {
        let Some(&token) = prompt_tokens
            .last()
            .filter(|&&token| !is_markup(vocab.attr(token)))
        else {
            break;
        };
        let piece = vocab.token_to_piece(token, false, None);
        if piece.contains(&b'\n') {
            break;
        }
        prompt_tokens.pop();
        text.splice(0..0, piece.iter().copied());
        if starts_word(&piece) {
            break;
        }
    }
    text
}

/// Returns the index of the token that starts the last word of `prompt_tokens`, as healing finds
/// that word.
pub(super) fn last_word_start(vocab: &LlamaVocab<'_>, prompt_tokens: &[LlamaToken]) -> usize {
    let mut before_last_word = prompt_tokens.to_vec();
    take_back_last_word(vocab, &mut before_last_word);
    before_last_word.len()
}

/// Returns what stays unwritten of `unwritten` after a token with `piece` as text.
pub(super) fn unwritten_after<'text>(unwritten: &'text [u8], piece: &[u8]) -> &'text [u8] {
    unwritten.strip_prefix(piece).unwrap_or_default()
}

/// Whether a token with `piece` as text adds a line break after it writes the `unwritten` text.
pub(super) fn writes_line_break(piece: &[u8], unwritten: &[u8]) -> bool {
    added_after(piece, unwritten).contains(&b'\n')
}

/// Returns the text that a token with `piece` as text adds after it writes the `unwritten` text.
pub(super) fn added_after<'piece>(piece: &'piece [u8], unwritten: &[u8]) -> &'piece [u8] {
    piece.get(unwritten.len()..).unwrap_or_default()
}

/// Tells if a token with `attrs` is markup, such as `<think>`, that never belongs in typed text.
pub(super) fn is_markup(attrs: LlamaTokenAttrs) -> bool {
    attrs.intersects(LlamaTokenAttr::Control | LlamaTokenAttr::UserDefined)
}

/// Returns the text `added` puts after `prompt_tokens`, with the spacing it has after them, without
/// the `healed_text` that the prompt already ends with.
pub(super) fn added_text(
    vocab: &LlamaVocab<'_>,
    prompt_tokens: &[LlamaToken],
    added: &[LlamaToken],
    healed_text: &[u8],
) -> String {
    let prompt_bytes = vocab.detokenize(prompt_tokens, false, false);
    let all_bytes = vocab.detokenize(&[prompt_tokens, added].concat(), false, false);
    let added_bytes = all_bytes
        .strip_prefix(prompt_bytes.as_slice())
        .map_or_else(|| vocab.detokenize(added, false, false), <[u8]>::to_vec);
    text_without_cut_character(text_after_prefix(&added_bytes, healed_text))
}

/// Returns `bytes` after `prefix`, or nothing when they do not start with it, so that a completion
/// never types again text that is already there.
fn text_after_prefix<'bytes>(bytes: &'bytes [u8], prefix: &[u8]) -> &'bytes [u8] {
    bytes.strip_prefix(prefix).unwrap_or_default()
}

/// Returns `bytes` as text, without a character that the token limit cut at the end.
fn text_without_cut_character(bytes: &[u8]) -> String {
    let end = match str::from_utf8(bytes) {
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        _ => bytes.len(),
    };
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_piece_leaves_the_rest_unwritten() {
        let cases = [
            (&b" beauti"[..], &b" beaut"[..], &b"i"[..]),
            (b" beauti", b" beautiful", b""),
        ];

        let rests = cases.map(|(unwritten, piece, _)| unwritten_after(unwritten, piece));

        assert_eq!(rests, cases.map(|(_, _, rest)| rest));
    }

    #[test]
    fn text_after_the_healed_text_is_what_a_token_adds() {
        let cases = [
            (&b" world"[..], &b" wor"[..], &b"ld"[..]),
            (b" wo", b" wor", b""),
            (b" world", b"", b" world"),
        ];

        let added = cases.map(|(piece, unwritten, _)| added_after(piece, unwritten));

        assert_eq!(added, cases.map(|(_, _, added)| added));
    }

    #[test]
    fn only_a_line_break_after_the_healed_text_ends_the_line() {
        let cases = [
            (&b"\n"[..], &b""[..], true),
            (b"ld\nHi", b"ld", true),
            (b" world", b"", false),
            (b"\nHi", b"\nHi", false),
            (b"\nHi", b"\nHi there", false),
        ];

        let ends = cases.map(|(piece, unwritten, _)| writes_line_break(piece, unwritten));

        assert_eq!(ends, cases.map(|(_, _, ends)| ends));
    }

    #[test]
    fn text_after_the_healed_prefix_is_kept() {
        let text = text_after_prefix(b" world, hi", b" wor");

        assert_eq!(text, b"ld, hi");
    }

    #[test]
    fn text_that_does_not_start_with_the_healed_prefix_is_dropped() {
        let text = text_after_prefix(b" planet", b" wor");

        assert_eq!(text, b"");
    }

    #[test]
    fn complete_text_is_kept() {
        let text = text_without_cut_character("café".as_bytes());

        assert_eq!(text, "café");
    }

    #[test]
    fn character_cut_at_the_end_is_dropped() {
        let bytes = "café".as_bytes();

        let text = text_without_cut_character(&bytes[..bytes.len() - 1]);

        assert_eq!(text, "caf");
    }

    #[test]
    fn invalid_bytes_inside_become_replacement_characters() {
        let text = text_without_cut_character(b"a\xffb");

        assert_eq!(text, "a\u{FFFD}b");
    }

    #[test]
    fn only_control_and_user_defined_tokens_are_markup() {
        let cases = [
            (LlamaTokenAttr::Normal, false),
            (LlamaTokenAttr::Byte, false),
            (LlamaTokenAttr::Control, true),
            (LlamaTokenAttr::UserDefined, true),
        ];

        let markup = cases.map(|(attr, _)| is_markup(LlamaTokenAttrs(attr.into())));

        assert_eq!(markup, cases.map(|(_, expected)| expected));
    }
}
