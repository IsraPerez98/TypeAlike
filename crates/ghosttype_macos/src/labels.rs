//! Core labels as macOS shows them.

/// Articles, conjunctions and short prepositions, which title case keeps lowercase between the
/// first and the last word.
const MINOR_WORDS: [&str; 20] = [
    "a", "an", "and", "as", "at", "but", "by", "for", "from", "in", "into", "nor", "of", "on", "onto", "or",
    "the", "to", "via", "with",
];

/// Turns a sentence-case core label into the title case of macOS menus, buttons and headings.
pub(crate) fn title_case(label: &str) -> String {
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

fn capitalize(word: &str) -> String {
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
        assert_eq!(title_case("Quit GhostType"), "Quit GhostType");
    }
}
