//! Tells a private window of a browser from the title that the browser gives the window.

use crate::desktop::Surroundings;

/// The titles that a browser gives its private windows, in English, Spanish, Portuguese, French,
/// German and Italian.
struct PrivateTitles {
    /// The starts of the app ids of the browser in lowercase: the bundle identifier on macOS and
    /// the program file name on Windows.
    app_ids: &'static [&'static str],
    /// The ends of the title of a private window in lowercase, with the signs that divide them from
    /// the title of the page. A private window without a page has the end without these signs as
    /// its title.
    title_ends: &'static [&'static str],
    /// The texts in lowercase that mark a private window anywhere in its title.
    title_marks: &'static [&'static str],
}

/// A closing quote can end the title, as in the German title of Safari.
const CLOSING_QUOTES: [char; 4] = ['“', '”', '"', '»'];

const BROWSERS: [PrivateTitles; 6] = [
    PrivateTitles {
        app_ids: &[
            "com.google.chrome",
            "org.chromium.chromium",
            "com.vivaldi.vivaldi",
            "chrome.exe",
            "chromium.exe",
            "vivaldi.exe",
        ],
        title_ends: &[
            " (incognito)",
            " (incógnito)",
            " (modo anônimo)",
            " (navigation privée)",
            " (anonym)",
            " (in incognito)",
        ],
        title_marks: &[],
    },
    PrivateTitles {
        app_ids: &["com.brave.browser", "brave.exe"],
        title_ends: &[
            " (private)",
            " (privado)",
            " (incógnito)",
            " (navigation privée)",
            " (privat)",
            " (privato)",
        ],
        title_marks: &[],
    },
    PrivateTitles {
        app_ids: &["com.operasoftware.opera", "opera.exe"],
        title_ends: &[
            " (private)",
            " (privada)",
            " (privado)",
            " (privé)",
            " (privat)",
            " (riservata)",
        ],
        title_marks: &[],
    },
    PrivateTitles {
        app_ids: &["com.microsoft.edgemac", "msedge.exe"],
        title_ends: &[" (inprivate)"],
        title_marks: &["[inprivate]"],
    },
    PrivateTitles {
        app_ids: &["org.mozilla.firefox", "firefox.exe"],
        title_ends: &[
            " — private browsing",
            " — navegación privada",
            " — navegação privativa",
            " — navigation privée",
            " — privater modus",
            " – privater modus",
            " — navigazione anonima",
            " — mozilla firefox private browsing",
            " — mozilla firefox navegación privada",
            " — navegación privada de mozilla firefox",
            " — navigation privée de mozilla firefox",
            " — mozilla firefox privater modus",
            " — mozilla firefox navigazione anonima",
        ],
        title_marks: &[],
    },
    PrivateTitles {
        app_ids: &["com.apple.safari"],
        title_ends: &[
            ", private browsing",
            ", navegación privada",
            " con navegación privada",
            ", navegação privada",
            ", navigation privée",
            ", „privates surfen",
            ", navigazione privata",
        ],
        title_marks: &[],
    },
];

/// Whether the focused field of `surroundings` is in a private window of a browser. No browser
/// tells it in a way that does not depend on the language, so the title must match a title of a
/// private window in a language of TypeAlike.
pub(crate) fn is_private_window(surroundings: &Surroundings) -> bool {
    let (Some(app_id), Some(title)) = (&surroundings.app_id, &surroundings.window_title) else {
        return false;
    };
    let app_id = app_id.to_lowercase();
    let Some(browser) = BROWSERS
        .iter()
        .find(|browser| browser.app_ids.iter().any(|start| app_id.starts_with(start)))
    else {
        return false;
    };
    let title = title.to_lowercase();
    let title = title
        .trim_end_matches(|character: char| character.is_whitespace() || CLOSING_QUOTES.contains(&character));
    browser.title_marks.iter().any(|mark| title.contains(mark))
        || browser.title_ends.iter().any(|end| {
            title.ends_with(end)
                || title == end.trim_start_matches(|character: char| !character.is_alphanumeric())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(app_id: &str, title: Option<&str>) -> Surroundings {
        Surroundings {
            app_id: Some(app_id.to_owned()),
            window_title: title.map(str::to_owned),
            ..Surroundings::default()
        }
    }

    #[test]
    fn window_title_tells_a_private_window_of_a_browser() {
        let cases = [
            (
                "com.google.Chrome",
                Some("Trip notes - Google Chrome (Incognito)"),
                true,
            ),
            (
                "com.google.Chrome",
                Some("Notas - Google Chrome (incógnito)"),
                true,
            ),
            (
                "com.google.Chrome",
                Some("Notas - Google Chrome (Modo anônimo)"),
                true,
            ),
            (
                "com.google.Chrome",
                Some("Notizen - Google Chrome (anonym)"),
                true,
            ),
            ("com.google.Chrome", Some("Trip notes - Google Chrome"), false),
            (
                "com.google.Chrome",
                Some("Trip notes - Google Chrome - Work"),
                false,
            ),
            ("com.brave.Browser", Some("Trip notes - Brave (Private)"), true),
            (
                "com.microsoft.edgemac",
                Some("Trip notes - Microsoft Edge (InPrivate)"),
                true,
            ),
            (
                "msedge.exe",
                Some("Trip notes - [InPrivate] - Microsoft\u{200b} Edge"),
                true,
            ),
            ("com.apple.Safari", Some("Trip notes, Private Browsing"), true),
            ("com.apple.Safari", Some("Private Browsing"), true),
            ("com.apple.Safari", Some("Notas con navegación privada"), true),
            ("com.apple.Safari", Some("Notizen, „Privates Surfen“"), true),
            ("com.apple.Safari", Some("Tips for private browsing"), false),
            ("org.mozilla.firefox", Some("Trip notes — Private Browsing"), true),
            (
                "firefox.exe",
                Some("Trip notes — Mozilla Firefox Private Browsing"),
                true,
            ),
            ("firefox.exe", Some("Mozilla Firefox Private Browsing"), true),
            ("firefox.exe", Some("Trip notes — Mozilla Firefox"), false),
            ("com.apple.Notes", Some("Trip notes, Private Browsing"), false),
            ("com.google.Chrome", None, false),
        ];

        let found = cases.map(|(app_id, title, _)| is_private_window(&window(app_id, title)));

        assert_eq!(found, cases.map(|(_, _, is_private)| is_private));
    }
}
