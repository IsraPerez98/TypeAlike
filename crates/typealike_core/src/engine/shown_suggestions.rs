//! Keeps the suggestions that showed in a session, so that a backspace or a retype shows them again
//! without the model.

use std::collections::VecDeque;

/// The most suggestions that a session keeps. A new suggestion then removes the oldest.
const MAX_SHOWN_SUGGESTIONS: usize = 16;

/// A suggestion and the text before the caret where it showed.
struct ShownSuggestion {
    text_before: String,
    suggestion: String,
}

/// The suggestions that the ghost text showed in a session, each after its text before the caret.
#[derive(Default)]
pub(super) struct ShownSuggestions {
    /// The kept suggestions, the oldest first.
    kept: VecDeque<ShownSuggestion>,
}

impl ShownSuggestions {
    /// Keeps `suggestion`, the ghost text after `text_before`, the text before the caret, in place
    /// of the suggestion that showed after the same text. An empty suggestion changes nothing.
    pub(super) fn record(&mut self, text_before: &str, suggestion: &str) {
        if suggestion.is_empty() {
            return;
        }
        self.kept.retain(|shown| shown.text_before != text_before);
        if self.kept.len() == MAX_SHOWN_SUGGESTIONS {
            self.kept.pop_front();
        }
        self.kept.push_back(ShownSuggestion {
            text_before: text_before.to_owned(),
            suggestion: suggestion.to_owned(),
        });
    }

    /// Returns the rest of the newest kept suggestion that `text_before` continues: its text before
    /// the caret, then a start of the suggestion that is shorter than the suggestion. `None` when
    /// no kept suggestion fits, also when `text_before` holds a whole suggestion.
    #[must_use]
    pub(super) fn rest_after(&self, text_before: &str) -> Option<&str> {
        self.kept
            .iter()
            .rev()
            .find_map(|shown| shown.rest_after(text_before))
    }
}

impl ShownSuggestion {
    fn rest_after(&self, text_before: &str) -> Option<&str> {
        let typed = text_before.strip_prefix(self.text_before.as_str())?;
        self.suggestion
            .strip_prefix(typed)
            .filter(|rest| !rest.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_SHOWN_SUGGESTIONS, ShownSuggestions};

    fn kept(suggestions: &[(&str, &str)]) -> ShownSuggestions {
        let mut shown_suggestions = ShownSuggestions::default();
        for (text_before, suggestion) in suggestions {
            shown_suggestions.record(text_before, suggestion);
        }
        shown_suggestions
    }

    /// Keeps a suggestion after each of `count` other texts before the caret.
    fn record_others(shown_suggestions: &mut ShownSuggestions, count: usize) {
        for index in 0..count {
            shown_suggestions.record(&format!("text {index}"), " more");
        }
    }

    #[test]
    fn rest_follows_the_typed_start_of_the_suggestion() {
        let shown_suggestions = kept(&[("hel", "lo world")]);

        let rests = ["hel", "hell", "hello w"].map(|text_before| shown_suggestions.rest_after(text_before));

        assert_eq!(rests, [Some("lo world"), Some("o world"), Some("orld")]);
    }

    #[test]
    fn text_that_leaves_the_suggestion_has_no_rest() {
        let shown_suggestions = kept(&[("hel", "lo world")]);

        let rests = ["he", "help", "hello world", "hello world!"]
            .map(|text_before| shown_suggestions.rest_after(text_before));

        assert_eq!(rests, [None; 4]);
    }

    #[test]
    fn newest_suggestion_that_fits_gives_the_rest() {
        let shown_suggestions = kept(&[("hel", "lo world"), ("hell", "o there")]);

        let rests = ["hel", "hell", "hello w"].map(|text_before| shown_suggestions.rest_after(text_before));

        assert_eq!(rests, [Some("lo world"), Some("o there"), Some("orld")]);
    }

    #[test]
    fn empty_suggestion_keeps_the_suggestion_before_it() {
        let shown_suggestions = kept(&[("hel", "lo world"), ("hel", "")]);

        let rest = shown_suggestions.rest_after("hel");

        assert_eq!(rest, Some("lo world"));
    }

    #[test]
    fn oldest_suggestion_goes_when_the_memory_is_full() {
        let mut shown_suggestions = kept(&[("hel", "lo world")]);
        record_others(&mut shown_suggestions, MAX_SHOWN_SUGGESTIONS);

        let rests = ["hel", "text 0"].map(|text_before| shown_suggestions.rest_after(text_before));

        assert_eq!(rests, [None, Some(" more")]);
    }

    #[test]
    fn suggestion_that_grows_takes_one_place() {
        let mut shown_suggestions = kept(&[("Hi", " Ana"), ("hel", "lo"), ("hel", "lo world")]);
        record_others(&mut shown_suggestions, MAX_SHOWN_SUGGESTIONS - 2);

        let rest = shown_suggestions.rest_after("Hi");

        assert_eq!(rest, Some(" Ana"));
    }
}
