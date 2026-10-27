//! Core labels as macOS shows them.

use typealike_core::localization::UiLanguage;

/// Articles, conjunctions and short prepositions, which title case keeps lowercase between the
/// first and the last word.
const MINOR_WORDS: [&str; 20] = [
    "a", "an", "and", "as", "at", "but", "by", "for", "from", "in", "into", "nor", "of", "on", "onto", "or",
    "the", "to", "via", "with",
];

/// Writes a sentence-case core label in `language` as macOS shows it in a menu, a button or a
/// heading: in title case when the language has one, else as it is.
pub(crate) fn menu_label(label: &str, language: UiLanguage) -> String {
    if language.has_title_case() {
        title_case(label)
    } else {
        label.to_owned()
    }
}

/// Turns a sentence-case core label into the title case of macOS menus, buttons and headings.
fn title_case(label: &str) -> String {
    let words: Vec<&str> = label.split(' ').collect();
    let last = words.len().saturating_sub(1);
    words
        .iter()
        .enumerate()
        .map(|(index, word)| {
            let is_inside = index != 0 && index != last;
            if is_inside && MINOR_WORDS.contains(word) {
                (*word).to_owned()
            } else {
                capitalize(word)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Capitalizes the first letter of `word`, unless the word has a capital or a dot, as the name of
/// an app or the host of a website can.
fn capitalize(word: &str) -> String {
    if word.chars().any(char::is_uppercase) || word.contains('.') {
        return word.to_owned();
    }
    let mut characters = word.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().chain(characters).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentence_case_label_becomes_title_case() {
        assert_eq!(title_case("Resume suggestions"), "Resume Suggestions");
    }

    #[test]
    fn short_minor_words_inside_a_title_stay_lowercase() {
        let labels = ["Reset to default", "Sign in", "Back to the list"];

        let titles = labels.map(title_case);

        assert_eq!(titles, ["Reset to Default", "Sign In", "Back to the List"]);
    }

    #[test]
    fn title_case_keeps_inner_capitals() {
        assert_eq!(title_case("Quit TypeAlike"), "Quit TypeAlike");
    }

    #[test]
    fn title_case_keeps_an_app_name_that_starts_in_lowercase() {
        assert_eq!(title_case("Use screen text in iTerm"), "Use Screen Text in iTerm");
    }

    #[test]
    fn title_case_keeps_the_host_of_a_website() {
        assert_eq!(
            title_case("Show suggestions on mail.example.com"),
            "Show Suggestions on mail.example.com"
        );
    }

    #[test]
    fn label_of_english_takes_title_case() {
        assert_eq!(
            menu_label("Resume suggestions", UiLanguage::English),
            "Resume Suggestions"
        );
    }

    #[test]
    fn label_of_a_language_without_title_case_keeps_its_sentence_case() {
        assert_eq!(
            menu_label("Pausar las sugerencias", UiLanguage::Spanish),
            "Pausar las sugerencias"
        );
    }
}
