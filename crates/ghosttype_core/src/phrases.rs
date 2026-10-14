//! Completions from a fixed list of phrases.

use crate::suggestion::SuggestionProvider;

const MIN_TRIGGER_CHARS: usize = 3;

/// Suggests the rest of a known phrase once the last typed words start one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhraseList {
    phrases: Vec<String>,
}

impl PhraseList {
    /// Creates a list that matches `phrases` in order of preference.
    #[must_use]
    pub fn new(phrases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            phrases: phrases.into_iter().map(Into::into).collect(),
        }
    }
}

impl Default for PhraseList {
    /// Common phrases from everyday writing and email.
    fn default() -> Self {
        Self::new([
            "Thank you for your time",
            "Thank you so much",
            "Thanks in advance",
            "Best regards",
            "Kind regards",
            "Looking forward to hearing from you",
            "Please let me know if you have any questions",
            "I hope this message finds you well",
            "Let me know what you think",
            "Have a great day",
            "Have a great weekend",
            "As soon as possible",
            "Sounds good to me",
            "Talk to you soon",
            "Happy to help",
        ])
    }
}

impl SuggestionProvider for PhraseList {
    fn suggest(&self, context: &str) -> Option<String> {
        word_starts(context)
            .map(|start| &context[start..])
            .filter(|fragment| fragment.chars().count() >= MIN_TRIGGER_CHARS)
            .find_map(|fragment| self.complete(fragment))
    }
}

impl PhraseList {
    fn complete(&self, fragment: &str) -> Option<String> {
        self.phrases
            .iter()
            .find_map(|phrase| completion_after(phrase, fragment))
    }
}

fn word_starts(text: &str) -> impl Iterator<Item = usize> {
    let mut is_after_whitespace = true;
    text.char_indices().filter_map(move |(index, character)| {
        let is_word_start = is_after_whitespace && !character.is_whitespace();
        is_after_whitespace = character.is_whitespace();
        is_word_start.then_some(index)
    })
}

fn completion_after(phrase: &str, fragment: &str) -> Option<String> {
    let mut phrase_chars = phrase.chars();
    let starts_with_fragment = fragment.chars().all(|typed| {
        phrase_chars
            .next()
            .is_some_and(|expected| is_same_ignoring_case(typed, expected))
    });
    let rest: String = phrase_chars.collect();
    (starts_with_fragment && !rest.is_empty()).then_some(rest)
}

fn is_same_ignoring_case(first: char, second: char) -> bool {
    first.to_lowercase().eq(second.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phrases() -> PhraseList {
        PhraseList::new(["Thank you for your time", "Thank you so much", "Best regards"])
    }

    #[test]
    fn completes_phrase_started_by_context() {
        let completion = phrases().suggest("Thank y");

        assert_eq!(completion.as_deref(), Some("ou for your time"));
    }

    #[test]
    fn completes_phrase_started_after_other_words() {
        let completion = phrases().suggest("Hi Ana, best re");

        assert_eq!(completion.as_deref(), Some("gards"));
    }

    #[test]
    fn matching_ignores_case_and_keeps_phrase_casing() {
        let completion = phrases().suggest("BEST R");

        assert_eq!(completion.as_deref(), Some("egards"));
    }

    #[test]
    fn earlier_phrase_wins_when_several_match() {
        let completion = phrases().suggest("thank you ");

        assert_eq!(completion.as_deref(), Some("for your time"));
    }

    #[test]
    fn later_phrase_matches_once_context_diverges() {
        let completion = phrases().suggest("thank you s");

        assert_eq!(completion.as_deref(), Some("o much"));
    }

    #[test]
    fn longest_matching_context_wins() {
        let list = PhraseList::new(["you so much", "thank you so much"]);

        let completion = list.suggest("thank you so");

        assert_eq!(completion.as_deref(), Some(" much"));
    }

    #[test]
    fn match_must_start_at_word_boundary() {
        let completion = phrases().suggest("unbest re");

        assert_eq!(completion, None);
    }

    #[test]
    fn too_short_context_suggests_nothing() {
        let completion = phrases().suggest("be");

        assert_eq!(completion, None);
    }

    #[test]
    fn complete_phrase_suggests_nothing() {
        let completion = phrases().suggest("Best regards");

        assert_eq!(completion, None);
    }

    #[test]
    fn unrelated_context_suggests_nothing() {
        let completion = phrases().suggest("see you later");

        assert_eq!(completion, None);
    }

    #[test]
    fn non_ascii_phrase_is_completed_by_character() {
        let list = PhraseList::new(["Él está aquí"]);

        let completion = list.suggest("él es");

        assert_eq!(completion.as_deref(), Some("tá aquí"));
    }

    #[test]
    fn default_list_completes_common_closing() {
        let completion = PhraseList::default().suggest("Kind reg");

        assert_eq!(completion.as_deref(), Some("ards"));
    }
}
