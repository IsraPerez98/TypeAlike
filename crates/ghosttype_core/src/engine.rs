//! Decides what ghost text to show as the user types, and what Tab inserts.

use std::{fmt, mem};

use crate::inference::{Completion, InferenceError, InferenceRequest, Prompt, TokenCount};
use crate::key::Key;

const MAX_CONTEXT_CHARS: usize = 200;
pub(crate) const MAX_SUGGESTION_TOKENS: TokenCount = TokenCount(16);

/// What the platform must do after a key press, a context change or a completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reaction {
    /// Let the key through and show this ghost text after the caret.
    Show(String),
    /// Let the key through and hide any ghost text.
    Hide,
    /// Swallow the key and type text into the focused app.
    Accept {
        /// The text to type at the caret.
        text: String,
        /// The ghost text to show after the typed text, or `None` to hide the ghost text.
        rest: Option<String>,
    },
    /// Let the key through, hide any ghost text and submit this request to the model runner.
    /// Its completion goes to [`App::on_completion`](crate::app::App::on_completion).
    Request(InferenceRequest),
}

/// Tracks the text typed at the caret and the model's suggestion for it.
#[derive(Default)]
pub(crate) struct Engine {
    context: String,
    offer: Offer,
}

#[derive(Default)]
enum Offer {
    #[default]
    Idle,
    Requested {
        prompt: String,
        is_dismissed: bool,
    },
    Shown(String),
}

impl Engine {
    /// Updates the context for a key pressed in the focused app.
    pub fn on_key(&mut self, key: Key) -> Reaction {
        match key {
            Key::Char(character) if !character.is_control() => self.on_typed(character),
            Key::Backspace => {
                self.context.pop();
                self.request()
            }
            Key::Tab => self.accept(),
            Key::Escape => self.dismiss(),
            Key::Char(_) | Key::Other => self.reset(),
        }
    }

    /// Handles the completion of the last [`Reaction::Request`].
    pub fn on_completion(&mut self, result: Result<Completion, InferenceError>) -> Reaction {
        match mem::take(&mut self.offer) {
            Offer::Requested { prompt, .. } if prompt != self.context => self.request(),
            Offer::Requested {
                is_dismissed: false, ..
            } => result
                .ok()
                .and_then(|completion| suggestion_after(&self.context, &completion.text))
                .map_or(Reaction::Hide, |suggestion| self.show(suggestion)),
            Offer::Requested {
                is_dismissed: true, ..
            } => Reaction::Hide,
            offer @ (Offer::Idle | Offer::Shown(_)) => {
                self.offer = offer;
                Reaction::Hide
            }
        }
    }

    /// Forgets the typed context, for when the caret moved somewhere unknown.
    pub fn reset(&mut self) -> Reaction {
        self.context.clear();
        // A request in flight stays tracked, so that the engine never has two.
        if let Offer::Shown(_) = self.offer {
            self.offer = Offer::Idle;
        }
        Reaction::Hide
    }

    fn on_typed(&mut self, character: char) -> Reaction {
        self.append(&character.to_string());
        let rest = match &self.offer {
            Offer::Shown(suggestion) => suggestion.strip_prefix(character).map(str::to_owned),
            Offer::Idle | Offer::Requested { .. } => None,
        };
        match rest {
            Some(rest) if !rest.is_empty() => self.show(rest),
            _ => self.request(),
        }
    }

    fn accept(&mut self) -> Reaction {
        match mem::take(&mut self.offer) {
            Offer::Shown(suggestion) => {
                let (word, rest) = split_first_word(&suggestion);
                let rest = (!rest.is_empty()).then(|| rest.to_owned());
                self.append(word);
                self.offer = rest.clone().map_or(Offer::Idle, Offer::Shown);
                Reaction::Accept {
                    text: word.to_owned(),
                    rest,
                }
            }
            offer @ (Offer::Idle | Offer::Requested { .. }) => {
                self.offer = offer;
                self.reset()
            }
        }
    }

    fn dismiss(&mut self) -> Reaction {
        if let Offer::Requested { is_dismissed, .. } = &mut self.offer {
            *is_dismissed = true;
        } else {
            self.offer = Offer::Idle;
        }
        Reaction::Hide
    }

    fn request(&mut self) -> Reaction {
        if let Offer::Requested { .. } = self.offer {
            return Reaction::Hide;
        }
        if self.context.trim().is_empty() {
            self.offer = Offer::Idle;
            return Reaction::Hide;
        }
        self.offer = Offer::Requested {
            prompt: self.context.clone(),
            is_dismissed: false,
        };
        Reaction::Request(InferenceRequest::Complete {
            prompt: Prompt(self.context.clone()),
            max_tokens: MAX_SUGGESTION_TOKENS,
        })
    }

    fn show(&mut self, suggestion: String) -> Reaction {
        self.offer = Offer::Shown(suggestion.clone());
        Reaction::Show(suggestion)
    }

    fn append(&mut self, text: &str) {
        self.context.push_str(text);
        let excess_chars = self.context.chars().count().saturating_sub(MAX_CONTEXT_CHARS);
        let kept_start = self
            .context
            .char_indices()
            .nth(excess_chars)
            .map_or(0, |(index, _)| index);
        self.context.drain(..kept_start);
    }
}

/// Returns the first line of `completion` as ghost text after `context`, or `None` when it has no
/// text. Spaces that would double the space at the end of `context`, or trail, are dropped.
fn suggestion_after(context: &str, completion: &str) -> Option<String> {
    let first_line = completion.lines().next().unwrap_or_default();
    let line = if context.ends_with(char::is_whitespace) {
        first_line.trim_start()
    } else {
        first_line
    };
    let suggestion = line.trim_end();
    (!suggestion.is_empty()).then(|| suggestion.to_owned())
}

/// Splits `suggestion` into its first word, with the whitespace before that word, and the rest.
fn split_first_word(suggestion: &str) -> (&str, &str) {
    let rest = suggestion
        .trim_start()
        .trim_start_matches(|character: char| !character.is_whitespace());
    suggestion.split_at(suggestion.len() - rest.len())
}

impl fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("context_chars", &self.context.chars().count())
            .field("is_waiting", &matches!(self.offer, Offer::Requested { .. }))
            .field("has_suggestion", &matches!(self.offer, Offer::Shown(_)))
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn request(context: &str) -> Reaction {
        Reaction::Request(InferenceRequest::Complete {
            prompt: Prompt(context.to_owned()),
            max_tokens: MAX_SUGGESTION_TOKENS,
        })
    }

    #[expect(clippy::unnecessary_wraps, reason = "on_completion takes a Result")]
    fn completion(text: &str) -> Result<Completion, InferenceError> {
        Ok(Completion {
            text: text.to_owned(),
            tokens: TokenCount(1),
            duration: Duration::ZERO,
        })
    }

    fn show(text: &str) -> Reaction {
        Reaction::Show(text.to_owned())
    }

    fn accept(text: &str, rest: Option<&str>) -> Reaction {
        Reaction::Accept {
            text: text.to_owned(),
            rest: rest.map(str::to_owned),
        }
    }

    fn type_text(engine: &mut Engine, text: &str) -> Reaction {
        text.chars()
            .map(|character| engine.on_key(Key::Char(character)))
            .last()
            .expect("text is not empty")
    }

    fn engine_waiting_for(context: &str) -> Engine {
        let mut engine = Engine::default();
        for character in context.chars() {
            engine.on_completion(completion(""));
            engine.on_key(Key::Char(character));
        }
        engine
    }

    fn engine_showing(context: &str, suggestion: &str) -> Engine {
        let mut engine = engine_waiting_for(context);
        engine.on_completion(completion(suggestion));
        engine
    }

    #[test]
    fn typing_requests_completion_of_context() {
        let mut engine = Engine::default();

        let reaction = engine.on_key(Key::Char('h'));

        assert_eq!(reaction, request("h"));
    }

    #[test]
    fn typing_while_waiting_hides() {
        let mut engine = engine_waiting_for("h");

        let reaction = engine.on_key(Key::Char('e'));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn whitespace_context_requests_nothing() {
        let mut engine = Engine::default();

        let reaction = engine.on_key(Key::Char(' '));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn completion_for_current_context_shows() {
        let mut engine = engine_waiting_for("hel");

        let reaction = engine.on_completion(completion("lo"));

        assert_eq!(reaction, show("lo"));
    }

    #[test]
    fn stale_completion_requests_current_context() {
        let mut engine = engine_waiting_for("he");
        engine.on_key(Key::Char('l'));

        let reaction = engine.on_completion(completion("llo"));

        assert_eq!(reaction, request("hel"));
    }

    #[test]
    fn failed_completion_hides() {
        let mut engine = engine_waiting_for("hel");

        let reaction = engine.on_completion(Err(InferenceError::NoModel));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn completion_without_text_on_first_line_hides() {
        let cases = ["", "  ", "\nnext line"];

        let reactions = cases.map(|text| engine_waiting_for("hel").on_completion(completion(text)));

        assert_eq!(reactions, cases.map(|_| Reaction::Hide));
    }

    #[test]
    fn completion_stops_at_line_break() {
        let mut engine = engine_waiting_for("hel");

        let reaction = engine.on_completion(completion("lo there\nNext line"));

        assert_eq!(reaction, show("lo there"));
    }

    #[test]
    fn completion_drops_trailing_whitespace() {
        let mut engine = engine_waiting_for("hel");

        let reaction = engine.on_completion(completion("lo  "));

        assert_eq!(reaction, show("lo"));
    }

    #[test]
    fn completion_after_whitespace_drops_leading_whitespace() {
        let mut engine = engine_waiting_for("Hi ");

        let reaction = engine.on_completion(completion("  there"));

        assert_eq!(reaction, show("there"));
    }

    #[test]
    fn completion_after_word_keeps_leading_space() {
        let mut engine = engine_waiting_for("Hi");

        let reaction = engine.on_completion(completion(" there"));

        assert_eq!(reaction, show(" there"));
    }

    #[test]
    fn typing_next_suggested_character_shows_rest() {
        let mut engine = engine_showing("hel", "lo world");

        let reaction = engine.on_key(Key::Char('l'));

        assert_eq!(reaction, show("o world"));
    }

    #[test]
    fn typing_other_character_requests_new_completion() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = engine.on_key(Key::Char('p'));

        assert_eq!(reaction, request("help"));
    }

    #[test]
    fn typing_whole_suggestion_requests_continuation() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = type_text(&mut engine, "lo");

        assert_eq!(reaction, request("hello"));
    }

    #[test]
    fn tab_accepts_next_word_of_suggestion() {
        let mut engine = engine_showing("hel", "lo big world");

        let reaction = engine.on_key(Key::Tab);

        assert_eq!(reaction, accept("lo", Some(" big world")));
    }

    #[test]
    fn tab_accepts_word_with_space_before_it() {
        let mut engine = engine_showing("hello", " big world");

        let reaction = engine.on_key(Key::Tab);

        assert_eq!(reaction, accept(" big", Some(" world")));
    }

    #[test]
    fn tab_after_accepted_word_accepts_following_word() {
        let mut engine = engine_showing("hel", "lo big world");
        engine.on_key(Key::Tab);

        let reaction = engine.on_key(Key::Tab);

        assert_eq!(reaction, accept(" big", Some(" world")));
    }

    #[test]
    fn tab_accepts_last_word_and_hides() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = engine.on_key(Key::Tab);

        assert_eq!(reaction, accept("lo", None));
    }

    #[test]
    fn typing_after_accepted_word_follows_rest() {
        let mut engine = engine_showing("hel", "lo big world");
        engine.on_key(Key::Tab);

        let reaction = engine.on_key(Key::Char(' '));

        assert_eq!(reaction, show("big world"));
    }

    #[test]
    fn tab_without_suggestion_passes_through() {
        let mut engine = engine_waiting_for("hel");

        let reaction = engine.on_key(Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn accepted_word_becomes_context() {
        let mut engine = engine_showing("hel", "lo big world");
        engine.on_key(Key::Tab);

        let reaction = engine.on_key(Key::Char('!'));

        assert_eq!(reaction, request("hello!"));
    }

    #[test]
    fn suggestion_is_offered_once_after_accept() {
        let mut engine = engine_showing("hel", "lo");
        engine.on_key(Key::Tab);

        let reaction = engine.on_key(Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn backspace_requests_completion_of_shorter_context() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = engine.on_key(Key::Backspace);

        assert_eq!(reaction, request("he"));
    }

    #[test]
    fn backspace_on_empty_context_hides() {
        let mut engine = Engine::default();

        let reaction = engine.on_key(Key::Backspace);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn escape_hides_suggestion() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = engine.on_key(Key::Escape);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn tab_after_escape_passes_through() {
        let mut engine = engine_showing("hel", "lo");
        engine.on_key(Key::Escape);

        let reaction = engine.on_key(Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn escape_while_waiting_drops_completion() {
        let mut engine = engine_waiting_for("hel");
        engine.on_key(Key::Escape);

        let reaction = engine.on_completion(completion("lo"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn typing_after_escape_while_waiting_requests_again() {
        let mut engine = engine_waiting_for("hel");
        engine.on_key(Key::Escape);
        engine.on_key(Key::Char('p'));

        let reaction = engine.on_completion(completion("lo"));

        assert_eq!(reaction, request("help"));
    }

    #[test]
    fn other_key_clears_context() {
        let mut engine = engine_showing("he", "llo");
        engine.on_key(Key::Other);

        let reaction = engine.on_key(Key::Char('l'));

        assert_eq!(reaction, request("l"));
    }

    #[test]
    fn control_character_clears_context() {
        let mut engine = engine_showing("he", "llo");
        engine.on_key(Key::Char('\r'));

        let reaction = engine.on_key(Key::Char('l'));

        assert_eq!(reaction, request("l"));
    }

    #[test]
    fn reset_hides_and_clears_context() {
        let mut engine = engine_showing("he", "llo");

        let reset_reaction = engine.reset();
        let typed_reaction = engine.on_key(Key::Char('l'));

        assert_eq!((reset_reaction, typed_reaction), (Reaction::Hide, request("l")));
    }

    #[test]
    fn completion_after_reset_is_dropped() {
        let mut engine = engine_waiting_for("hel");
        engine.reset();

        let reaction = engine.on_completion(completion("lo"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn reset_keeps_one_request_in_flight() {
        let mut engine = engine_waiting_for("hel");
        engine.reset();

        let reaction = engine.on_key(Key::Char('x'));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn context_keeps_only_recent_characters() {
        let mut engine = engine_waiting_for(&format!("x{}", "y".repeat(MAX_CONTEXT_CHARS - 1)));
        engine.on_completion(completion(""));

        let reaction = engine.on_key(Key::Char('y'));

        assert_eq!(reaction, request(&"y".repeat(MAX_CONTEXT_CHARS)));
    }

    #[test]
    fn debug_does_not_reveal_typed_or_suggested_text() {
        let engine = engine_showing("secret", "word");

        let debug = format!("{engine:?}");

        assert!(!debug.contains("secret") && !debug.contains("word"));
    }
}
