//! Core labels as macOS shows them.

/// Turns a sentence-case core label into the title case of macOS menus, buttons and headings.
pub(crate) fn title_case(label: &str) -> String {
    label.split(' ').map(capitalize).collect::<Vec<_>>().join(" ")
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
    fn title_case_keeps_inner_capitals() {
        assert_eq!(title_case("Quit GhostType"), "Quit GhostType");
    }
}
