//! The removal of secrets from a text: private keys, passwords of web addresses, credentials of an
//! authorization header, values of named secrets, passwords of database commands, JSON web tokens,
//! API tokens, long mixed tokens and card numbers.

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
/// The names of secrets, alone or as the last part of a longer name, as in `DB_PASSWORD`.
const SECRET_NAMES: [&str; 7] = ["password", "passwd", "pwd", "pass", "secret", "token", "apikey"];
/// The last part of a longer name of a secret, as in `API_KEY`. Alone, "key" is a common word.
const KEY_NAME: &str = "key";
/// The flag of a `mysql` password, with the password next to it, as in `-pS3cret`.
const MYSQL_PASSWORD_FLAG: &str = "-p";
/// The start of a JSON web token: `{"` in base64.
const JSON_WEB_TOKEN_START: &str = "eyJ";

/// Returns `text` with each secret replaced by [`REDACTED`]: private keys, passwords of web
/// addresses, credentials of an authorization header, values of named secrets, passwords of
/// database commands, JSON web tokens, API tokens, long tokens that mix letters and digits, and
/// card numbers.
pub(super) fn redacted(text: &str) -> String {
    let text = replaced(text, &private_key_spans(text));
    let text = replaced(&text, &web_address_password_spans(&text));
    let text = replaced(&text, &authorization_spans(&text));
    let text = replaced(&text, &named_secret_spans(&text));
    let text = replaced(&text, &command_password_spans(&text));
    let text = replaced(&text, &json_web_token_spans(&text));
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

/// The spans of the passwords in web addresses, as in `postgres://admin:password@host`.
fn web_address_password_spans(text: &str) -> Vec<Range<usize>> {
    const SCHEME_END: &str = "://";
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find(SCHEME_END) {
        let authority_start = from + offset + SCHEME_END.len();
        let authority_end = text[authority_start..]
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '/' | '?' | '#' | '"' | '\'')
            })
            .map_or(text.len(), |offset| authority_start + offset);
        from = authority_end;
        let authority = &text[authority_start..authority_end];
        let Some(user_end) = authority.rfind('@') else {
            continue;
        };
        let Some(password_start) = authority[..user_end].find(':').map(|colon| colon + 1) else {
            continue;
        };
        if password_start < user_end {
            spans.push(authority_start + password_start..authority_start + user_end);
        }
    }
    spans
}

/// The spans of the credentials after "Authorization:", as in an HTTP header with "Basic" or
/// "Bearer" and a token.
fn authorization_spans(text: &str) -> Vec<Range<usize>> {
    const HEADER: &str = "authorization:";
    let lowercase = text.to_ascii_lowercase();
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(offset) = lowercase[from..].find(HEADER) {
        from += offset + HEADER.len();
        let Some(first) = next_word(text, from) else {
            continue;
        };
        let is_scheme = text[first.clone()]
            .chars()
            .all(|character| character.is_ascii_alphabetic());
        let credential = if is_scheme {
            next_word(text, first.end)
        } else {
            Some(first)
        };
        if let Some(credential) = credential {
            from = credential.end;
            spans.push(credential);
        }
    }
    spans
}

/// The span of the next word of `text` after `from`: the characters up to a space or a quote.
fn next_word(text: &str, from: usize) -> Option<Range<usize>> {
    let start = text[from..]
        .find(|character: char| !character.is_whitespace())
        .map(|offset| from + offset)?;
    let end = text[start..]
        .find(|character: char| character.is_whitespace() || matches!(character, '"' | '\''))
        .map_or(text.len(), |offset| start + offset);
    (end > start).then_some(start..end)
}

/// The spans of the values after the names of secrets: after ":" or "=", as in
/// `DB_PASSWORD=value`, or after "is", as in "my password is hunter2".
fn named_secret_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut from = 0;
    for name in runs_of(text, is_name_character) {
        if name.start < from || !is_secret_name(&text[name.clone()]) {
            continue;
        }
        if let Some(value) = named_value_span(text, name.end) {
            from = value.end;
            spans.push(value);
        }
    }
    spans
}

fn is_name_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

/// Whether `name` is one of the [`SECRET_NAMES`], or ends with one of them or with [`KEY_NAME`]
/// after a dash or an underscore.
fn is_secret_name(name: &str) -> bool {
    let lowercase = name.to_ascii_lowercase();
    let last_part = lowercase.rsplit(['_', '-']).next().unwrap_or_default();
    let has_more_parts = last_part.len() < lowercase.len();
    SECRET_NAMES.contains(&last_part) || (has_more_parts && last_part == KEY_NAME)
}

/// The span of the value after the name of a secret that ends at `name_end`. A value after "is"
/// must not be a plain word, because "the password is wrong" names no secret.
fn named_value_span(text: &str, name_end: usize) -> Option<Range<usize>> {
    let after_name = &text[name_end..];
    let rest = after_name
        .strip_prefix(['"', '\''])
        .unwrap_or(after_name)
        .trim_start_matches([' ', '\t']);
    if let Some(value) = rest.strip_prefix([':', '=']) {
        let is_operator = value.starts_with([':', '=', '>']);
        return (!is_operator)
            .then(|| value_span(text, text.len() - value.len()))
            .flatten();
    }
    let value = rest.strip_prefix("is ")?;
    value_span(text, text.len() - value.len()).filter(|span| !is_plain_word(&text[span.clone()]))
}

/// The span of the value at `from` on the same line: the text between quotes, else the next word.
fn value_span(text: &str, from: usize) -> Option<Range<usize>> {
    let start = text.len() - text[from..].trim_start_matches([' ', '\t']).len();
    let Some(quote) = text[start..]
        .chars()
        .next()
        .filter(|character| matches!(character, '"' | '\''))
    else {
        return next_word(text, start).filter(|word| word.start == start);
    };
    let value_start = start + quote.len_utf8();
    let value_end = text[value_start..]
        .find([quote, '\n'])
        .map_or(text.len(), |offset| value_start + offset);
    (value_end > value_start).then_some(value_start..value_end)
}

/// Whether `word`, without the punctuation at its end, has only letters, dashes and apostrophes.
fn is_plain_word(word: &str) -> bool {
    word.trim_end_matches(['.', ',', '!', '?', ';', ':'])
        .chars()
        .all(|character| character.is_alphabetic() || matches!(character, '-' | '\''))
}

/// The spans of the passwords of database commands: the value next to `-p` after `mysql` or
/// `mariadb`, the word after `-a` after `redis-cli`, and the last word of a Redis `AUTH` with one
/// or two words after it. Each command ends at the end of its line.
fn command_password_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let word_spans = runs_of(line, |character| !character.is_whitespace());
        let words = word_spans
            .iter()
            .map(|span| &line[span.clone()])
            .collect::<Vec<_>>();
        for (index, span) in word_spans.iter().enumerate() {
            let is_last = index + 1 == words.len();
            if let Some(offset) = command_password_offset(&words[..index], words[index], is_last) {
                spans.push(line_start + span.start + offset..line_start + span.end);
            }
        }
        line_start += line.len();
    }
    spans
}

/// The byte of `word` where its password starts, when `word` follows the words `before` of a
/// database command.
fn command_password_offset(before: &[&str], word: &str, is_last: bool) -> Option<usize> {
    let is_after_mysql = before
        .iter()
        .any(|earlier| earlier.starts_with("mysql") || earlier.starts_with("mariadb"));
    let is_mysql_password =
        is_after_mysql && word.len() > MYSQL_PASSWORD_FLAG.len() && word.starts_with(MYSQL_PASSWORD_FLAG);
    let is_redis_cli_password = before.last() == Some(&"-a") && before.contains(&"redis-cli");
    let is_auth_password = is_last && before.iter().rev().take(2).any(|earlier| *earlier == "AUTH");
    if is_mysql_password {
        Some(MYSQL_PASSWORD_FLAG.len())
    } else {
        (is_redis_cli_password || is_auth_password).then_some(0)
    }
}

/// The spans of the JSON web tokens: three parts or more between dots, and the first starts with
/// [`JSON_WEB_TOKEN_START`].
fn json_web_token_spans(text: &str) -> Vec<Range<usize>> {
    runs_of(text, |character| {
        character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
    })
    .into_iter()
    .map(|run| run.start..run.start + text[run].trim_end_matches('.').len())
    .filter(|run| {
        let token = &text[run.clone()];
        token.starts_with(JSON_WEB_TOKEN_START) && token.matches('.').count() >= 2
    })
    .collect()
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
    fn redaction_replaces_the_passwords_of_web_addresses() {
        let cases = [
            (
                "db postgres://admin:S3cretPass@db.local:5432/app",
                "db postgres://admin:[redacted]@db.local:5432/app",
            ),
            (
                "redis://:hunter2@cache:6379 ok",
                "redis://:[redacted]@cache:6379 ok",
            ),
            ("https://ana@example.com/a", "https://ana@example.com/a"),
            ("https://example.com/a:b@c", "https://example.com/a:b@c"),
            ("mail ana@example.com: hi", "mail ana@example.com: hi"),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn redaction_replaces_the_credentials_of_an_authorization_header() {
        let cases = [
            (
                "Authorization: Basic YWRtaW46czNjcmV0",
                "Authorization: Basic [redacted]",
            ),
            (
                "-H 'authorization: Bearer abc.def' -X",
                "-H 'authorization: Bearer [redacted]' -X",
            ),
            ("the basic idea works", "the basic idea works"),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn redaction_replaces_the_values_after_the_names_of_secrets() {
        let cases = [
            ("export PASSWORD=S3cretPass", "export PASSWORD=[redacted]"),
            (
                "DB_PASSWORD=S3cretPass\nDB_HOST=db",
                "DB_PASSWORD=[redacted]\nDB_HOST=db",
            ),
            ("API_KEY=abc123xyz", "API_KEY=[redacted]"),
            ("password: Summer2024", "password: [redacted]"),
            ("pwd=Summer2024", "pwd=[redacted]"),
            ("Secret: blue-falcon", "Secret: [redacted]"),
            ("token = abc123xyz ok", "token = [redacted] ok"),
            ("mysql --password=S3cretPass", "mysql --password=[redacted]"),
            ("X-Api-Key: abc123xyz", "X-Api-Key: [redacted]"),
            (
                r#"{"password": "S3cret Pass", "user": "ana"}"#,
                r#"{"password": "[redacted]", "user": "ana"}"#,
            ),
            ("pass the salt", "pass the salt"),
            ("the token of appreciation", "the token of appreciation"),
            ("password reset email sent", "password reset email sent"),
            ("Key: blue line", "Key: blue line"),
            ("max_tokens=100", "max_tokens=100"),
            ("if password == stored:", "if password == stored:"),
            ("Password:\nUse eight letters", "Password:\nUse eight letters"),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn redaction_replaces_a_password_that_is_not_a_plain_word_after_is() {
        let cases = [
            ("my password is hunter2", "my password is [redacted]"),
            ("Your API_KEY is abc123xyz now", "Your API_KEY is [redacted] now"),
            ("the password is wrong", "the password is wrong"),
            (
                "your password is case-sensitive",
                "your password is case-sensitive",
            ),
            (
                "the new password is in a separate mail.",
                "the new password is in a separate mail.",
            ),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn redaction_replaces_the_passwords_of_database_commands() {
        let cases = [
            ("mysql -u root -pS3cretPass app", "mysql -u root -p[redacted] app"),
            (
                "mysqldump -uroot -pS3cretPass db > db.sql",
                "mysqldump -uroot -p[redacted] db > db.sql",
            ),
            (
                "redis-cli -h cache -a S3cretPass ping",
                "redis-cli -h cache -a [redacted] ping",
            ),
            ("AUTH default S3cretPass", "AUTH default [redacted]"),
            (
                "127.0.0.1:6379> AUTH S3cretPass\nOK",
                "127.0.0.1:6379> AUTH [redacted]\nOK",
            ),
            ("psql -h db -p 5432 -U app", "psql -h db -p 5432 -U app"),
            ("mysql -u root -p app", "mysql -u root -p app"),
            ("cp -pR old new", "cp -pR old new"),
            (
                "ls -a and AUTH settings are in the docs",
                "ls -a and AUTH settings are in the docs",
            ),
        ];

        let redactions = cases.map(|(text, _)| redacted(text));

        assert_eq!(redactions, cases.map(|(_, kept)| kept.to_owned()));
    }

    #[test]
    fn redaction_replaces_json_web_tokens() {
        let cases = [
            (
                "jwt eyJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJhYmMifQ.c2ln ok",
                "jwt [redacted] ok",
            ),
            ("eyJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJhYmMifQ.c2ln.", "[redacted]."),
            ("see eyJhbGci.eyJzdWIi for more", "see eyJhbGci.eyJzdWIi for more"),
            ("v1.2.3 and a.b.c", "v1.2.3 and a.b.c"),
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
