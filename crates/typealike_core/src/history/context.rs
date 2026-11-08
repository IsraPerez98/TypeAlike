//! The typing context of a text: the contact, channel or document that the window title names.

use super::redaction::redacted;

/// The marks between the parts of a window title.
const TITLE_SEPARATORS: [&str; 5] = [" - ", " | ", " — ", " – ", " · "];
/// The marks at the start of the subject of a reply or a forward, in lowercase.
const REPLY_PREFIXES: [&str; 9] = ["re:", "fwd:", "fw:", "aw:", "wg:", "rv:", "res:", "enc:", "tr:"];
/// The most characters of a file extension at the end of a title part.
const MAX_EXTENSION_CHARS: usize = 4;

/// Returns the typing context of a window with `window_title` in the app named `app_name`: the
/// parts of the title in lowercase, without the parts that name the app, without counts such as
/// "(3)", reply marks such as "Re:", file extensions and secrets. `None` when no part stays.
#[must_use]
pub fn typing_context(app_name: Option<&str>, window_title: Option<&str>) -> Option<String> {
    let mut title = redacted(window_title?).to_lowercase();
    for separator in TITLE_SEPARATORS {
        title = title.replace(separator, "\u{1}");
    }
    let app = app_name.map(str::to_lowercase);
    let parts = title
        .split('\u{1}')
        .filter_map(|part| context_part(part, app.as_deref()))
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join(" | "))
}

fn context_part(part: &str, app: Option<&str>) -> Option<String> {
    let mut text = part
        .split_whitespace()
        .filter(|word| !is_count(word))
        .collect::<Vec<_>>()
        .join(" ");
    while let Some(rest) = REPLY_PREFIXES.iter().find_map(|prefix| text.strip_prefix(prefix)) {
        text = rest.trim_start().to_owned();
    }
    let text = without_extension(&text)
        .trim_matches(|character: char| character == '•' || character == '*' || character.is_whitespace());
    let names_app = app.is_some_and(|app| app.contains(text) || text.contains(app));
    (!text.is_empty() && !names_app).then(|| text.to_owned())
}

fn is_count(word: &str) -> bool {
    word.strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .is_some_and(|inner| {
            !inner.is_empty()
                && inner
                    .chars()
                    .all(|character| character.is_ascii_digit() || character == '+')
        })
}

fn without_extension(text: &str) -> &str {
    match text.rsplit_once('.') {
        Some((stem, extension))
            if !stem.is_empty()
                && (1..=MAX_EXTENSION_CHARS).contains(&extension.chars().count())
                && extension.chars().all(char::is_alphanumeric) =>
        {
            stem
        }
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_context_leaves_out_the_secrets_of_the_title() {
        let context = typing_context(
            Some("Postman"),
            Some("GET https://api.example/v1?key=sk-abcdefgh12345 - My Workspace"),
        );

        assert_eq!(
            context.as_deref(),
            Some("get https://api.example/v1?[redacted] | my workspace")
        );
    }

    #[test]
    fn typing_context_keeps_the_parts_of_the_title_that_do_not_name_the_app() {
        let cases = [
            (
                Some("Slack"),
                "#team-web - Acme - Slack",
                Some("#team-web | acme"),
            ),
            (
                Some("Slack"),
                "Priya Shah (DM) - Acme - Slack",
                Some("priya shah (dm) | acme"),
            ),
            (
                Some("Microsoft Teams"),
                "Chat | Raj Mehta | Microsoft Teams",
                Some("chat | raj mehta"),
            ),
            (Some("WhatsApp"), "WhatsApp", None),
            (Some("WhatsApp"), "(3) WhatsApp", None),
            (Some("WhatsApp"), "Familia Pérez", Some("familia pérez")),
            (Some("Mail"), "Re: Garden party", Some("garden party")),
            (Some("Mail"), "RE: Fwd: Garden party", Some("garden party")),
            (
                Some("Microsoft Word"),
                "Informe_Gestion_2024.docx - Word",
                Some("informe_gestion_2024"),
            ),
            (
                Some("Google Chrome"),
                "Re: Kitchen remodel quote - Gmail - Google Chrome",
                Some("kitchen remodel quote | gmail"),
            ),
            (
                Some("Discord"),
                "(2) #bug-reports - Indie Devs - Discord",
                Some("#bug-reports | indie devs"),
            ),
        ];

        for (app_name, title, context) in cases {
            assert_eq!(
                typing_context(app_name, Some(title)).as_deref(),
                context,
                "{title}"
            );
        }
    }
}
