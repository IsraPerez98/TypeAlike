//! The removal of secrets from a text: private keys, API tokens, long mixed tokens and card numbers.

use std::ops::Range;

use super::words::runs_of;

pub(super) const REDACTED: &str = "[redacted]";
/// The starts of the API tokens that TypeAlike knows. A token needs at least
/// [`MIN_SECRET_TAIL`] more characters.
const SECRET_PREFIXES: [&str; 8] = [
    "sk-",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "AKIA",
];
const MIN_SECRET_TAIL: usize = 8;
/// The start of a Slack token, before its letter and its dash, as in "xoxb-".
const SLACK_PREFIX: &str = "xox";
/// The fewest characters of a token of letters and digits that TypeAlike takes for a secret.
const MIN_MIXED_TOKEN_CHARS: usize = 20;
/// The digits of a card number.
const CARD_DIGITS: Range<usize> = 13..20;

/// Returns `text` with each secret replaced by [`REDACTED`]: private keys, API tokens, long tokens
/// that mix letters and digits, and card numbers.
pub(super) fn redacted(text: &str) -> String {
    let text = replaced(text, &private_key_spans(text));
    let secret_tokens = runs_of(&text, is_token_character)
        .into_iter()
        .filter(|span| is_secret_token(&text[span.clone()]))
        .collect::<Vec<_>>();
    let text = replaced(&text, &secret_tokens);
    replaced(&text, &card_number_spans(&text))
}

/// Returns `text` with each of `spans`, which do not overlap and go from the start to the end,
/// replaced by [`REDACTED`].
fn replaced(text: &str, spans: &[Range<usize>]) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut from = 0;
    for span in spans {
        kept.push_str(&text[from..span.start]);
        kept.push_str(REDACTED);
        from = span.end;
    }
    kept.push_str(&text[from..]);
    kept
}

/// The spans of the private keys in PEM form. A key without its end line goes to the end of the
/// text.
fn private_key_spans(text: &str) -> Vec<Range<usize>> {
    const BEGIN: &str = "-----BEGIN ";
    const END: &str = "-----END ";
    const DASHES: &str = "-----";
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find(BEGIN) {
        let start = from + offset;
        let after_begin = start + BEGIN.len();
        let header_end = text[after_begin..]
            .find(DASHES)
            .map_or(text.len(), |offset| after_begin + offset + DASHES.len());
        from = header_end;
        if !text[start..header_end].contains("PRIVATE KEY") {
            continue;
        }
        let end = text[header_end..]
            .find(END)
            .map(|offset| header_end + offset + END.len())
            .and_then(|after_end| {
                text[after_end..]
                    .find(DASHES)
                    .map(|offset| after_end + offset + DASHES.len())
            })
            .unwrap_or(text.len());
        spans.push(start..end);
        from = end;
    }
    spans
}

fn is_token_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '+' | '=')
}

fn is_secret_token(token: &str) -> bool {
    let has_secret_prefix = SECRET_PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix) && token.len() >= prefix.len() + MIN_SECRET_TAIL);
    let is_slack_token = token.len() >= "xoxb-".len() + MIN_SECRET_TAIL
        && token
            .strip_prefix(SLACK_PREFIX)
            .and_then(|rest| rest.strip_prefix(|character: char| character.is_ascii_alphabetic()))
            .is_some_and(|rest| rest.starts_with('-'));
    let is_mixed = token.len() >= MIN_MIXED_TOKEN_CHARS
        && token.contains(|character: char| character.is_ascii_alphabetic())
        && token.contains(|character: char| character.is_ascii_digit())
        && !is_name_of_plain_parts(token);
    has_secret_prefix || is_slack_token || is_mixed
}

/// Whether each part of `token` between its dashes and underscores has only letters or only
/// digits, as a file name such as `Report_Q_2024` has. A part of a secret mixes them.
fn is_name_of_plain_parts(token: &str) -> bool {
    token.split(['-', '_']).all(|part| {
        part.chars().all(|character| character.is_ascii_alphabetic())
            || part.chars().all(|character| character.is_ascii_digit())
    })
}

/// The spans of the runs of 13 to 19 digits, in groups that one space or dash divides.
fn card_number_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut run: Option<DigitRun> = None;
    let mut characters = text.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        let next_is_digit = characters.peek().is_some_and(|&(_, next)| next.is_ascii_digit());
        run = match (character.is_ascii_digit(), run) {
            (true, None) => Some(DigitRun {
                start: index,
                end: index + 1,
                digits: 1,
            }),
            (true, Some(open)) => Some(DigitRun {
                end: index + 1,
                digits: open.digits + 1,
                ..open
            }),
            (false, Some(open)) if matches!(character, ' ' | '-') && next_is_digit => Some(open),
            (false, open) => {
                spans.extend(open.and_then(DigitRun::card_span));
                None
            }
        };
    }
    spans.extend(run.and_then(DigitRun::card_span));
    spans
}

/// A run of digits, in groups that one space or dash divides, from the byte `start` to the byte
/// `end` of its text.
#[derive(Clone, Copy)]
struct DigitRun {
    start: usize,
    end: usize,
    digits: usize,
}

impl DigitRun {
    /// The span of the run when it has the [`CARD_DIGITS`] of a card number.
    fn card_span(self) -> Option<Range<usize>> {
        CARD_DIGITS.contains(&self.digits).then_some(self.start..self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_replaces_api_tokens() {
        let cases = [
            ("key sk-abcdefgh1 ok", "key [redacted] ok"),
            ("ghp_A1b2C3d4E5f6", "[redacted]"),
            ("github_pat_11ABCDEFG", "[redacted]"),
            ("token xoxb-1234-abcd-5678", "token [redacted]"),
            ("AKIAIOSFODNN7EXAMPLE.", "[redacted]."),
            ("sk-short and ghp_ and xoxb-", "sk-short and ghp_ and xoxb-"),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn redaction_replaces_long_tokens_that_mix_letters_and_digits() {
        let cases = [
            ("id 3f9a8b7c6d5e4f3a2b1c end", "id [redacted] end"),
            ("eyJhbGci.eyJzdWIiOiIxMjM0NTY3ODkw", "eyJhbGci.[redacted]"),
            (
                "internationalization and end-to-end-encryption",
                "internationalization and end-to-end-encryption",
            ),
            ("order 1Z999AA10123456 shipped", "order 1Z999AA10123456 shipped"),
            ("file Informe_Gestion_2024.docx", "file Informe_Gestion_2024.docx"),
            ("key abc123-def456-ghi789-jkl0", "key [redacted]"),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn redaction_replaces_card_numbers() {
        let cases = [
            ("card 4111111111111111.", "card [redacted]."),
            ("card 4111 1111 1111 1111 exp", "card [redacted] exp"),
            ("card 4111-1111-1111-1111", "card [redacted]"),
            (
                "call 415-555-0187 or 22 345 6789",
                "call 415-555-0187 or 22 345 6789",
            ),
            ("id 12345678901234567890", "id 12345678901234567890"),
            (
                "in 2024 and 2025  2026 2027 2028",
                "in 2024 and 2025  2026 2027 2028",
            ),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn redaction_replaces_private_keys() {
        let cases = [
            (
                "key:\n-----BEGIN RSA PRIVATE KEY-----\nMIIEow\nIBAAK\n-----END RSA PRIVATE KEY-----\nthanks",
                "key:\n[redacted]\nthanks",
            ),
            ("-----BEGIN PRIVATE KEY-----\nMIIEow", "[redacted]"),
            (
                "-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----",
                "-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----",
            ),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }
}
