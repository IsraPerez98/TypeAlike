//! Decides what ghost text to show as the user types, and what Tab inserts.

use std::{fmt, mem};

use crate::desktop::{Desktop, MAX_CHARS_BEFORE_CARET};
use crate::inference::{Completion, InferenceError, InferenceRequest, Prompt, SystemPrompt, TokenCount};
use crate::key::Key;
use crate::prompt::{self, last_chars};
use crate::settings::{ClipboardAccess, Settings};

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

/// Tracks the text at the caret and the model's suggestion for it.
#[derive(Default)]
pub(crate) struct Engine {
    session: Option<Session>,
    offer: Offer,
    system_prompt: SystemPrompt,
    clipboard_access: ClipboardAccess,
}

/// The typing in one place: what the model reads before the text, and the text before the caret.
/// It starts at the first key after a reset, when the key has not reached the app yet.
struct Session {
    preface: String,
    text_before: String,
}

#[derive(Default)]
enum Offer {
    #[default]
    Idle,
    Requested {
        prompt: Prompt,
        is_dismissed: bool,
    },
    Shown(String),
}

impl Engine {
    /// Uses the system prompt and the clipboard access of `settings` from the next session on.
    pub fn use_settings(&mut self, settings: &Settings) {
        self.system_prompt = settings.system_prompt();
        self.clipboard_access = settings.clipboard_access;
    }

    /// Updates the text at the caret for a key pressed in the focused field of `desktop`.
    pub fn on_key(&mut self, key: Key, desktop: &dyn Desktop) -> Reaction {
        match key {
            Key::Char(character) if !character.is_control() => {
                self.start_session(desktop).text_before.push(character);
                self.on_typed(character)
            }
            Key::Backspace => {
                self.start_session(desktop).text_before.pop();
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
            Offer::Requested { prompt, .. } if self.prompt().as_ref() != Some(&prompt) => self.request(),
            Offer::Requested {
                is_dismissed: false, ..
            } => result
                .ok()
                .and_then(|completion| suggestion_after(self.text_before(), &completion.text))
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

    /// Ends the session, for when the caret moved somewhere unknown.
    pub fn reset(&mut self) -> Reaction {
        self.session = None;
        // A request in flight stays tracked, so that the engine never has two.
        if let Offer::Shown(_) = self.offer {
            self.offer = Offer::Idle;
        }
        Reaction::Hide
    }

    /// Returns the session, and starts it with what surrounds the caret in `desktop` when there
    /// is none.
    fn start_session(&mut self, desktop: &dyn Desktop) -> &mut Session {
        let system_prompt = &self.system_prompt;
        let clipboard_access = self.clipboard_access;
        self.session.get_or_insert_with(|| {
            let surroundings = desktop.surroundings();
            let clipboard = match clipboard_access {
                ClipboardAccess::Allowed => desktop.clipboard_text(),
                ClipboardAccess::Denied => None,
            };
            let text_before = surroundings
                .field_text
                .as_ref()
                .map(|field_text| last_chars(&field_text.before_caret, MAX_CHARS_BEFORE_CARET).to_owned())
                .unwrap_or_default();
            Session {
                preface: prompt::preface(system_prompt, &surroundings, clipboard.as_deref()),
                text_before,
            }
        })
    }

    fn on_typed(&mut self, character: char) -> Reaction {
        self.keep_recent_text();
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
        match (mem::take(&mut self.offer), &mut self.session) {
            (Offer::Shown(suggestion), Some(session)) => {
                let (word, rest) = split_first_word(&suggestion);
                let rest = (!rest.is_empty()).then(|| rest.to_owned());
                session.text_before.push_str(word);
                self.keep_recent_text();
                self.offer = rest.clone().map_or(Offer::Idle, Offer::Shown);
                Reaction::Accept {
                    text: word.to_owned(),
                    rest,
                }
            }
            (offer, _) => {
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
        let Some(prompt) = self.prompt() else {
            self.offer = Offer::Idle;
            return Reaction::Hide;
        };
        self.offer = Offer::Requested {
            prompt: prompt.clone(),
            is_dismissed: false,
        };
        Reaction::Request(InferenceRequest::Complete {
            prompt,
            max_tokens: MAX_SUGGESTION_TOKENS,
        })
    }

    /// Returns the prompt for the text before the caret, or `None` when that text has no words.
    fn prompt(&self) -> Option<Prompt> {
        let session = self.session.as_ref()?;
        let has_words = !session.text_before.trim().is_empty();
        has_words.then(|| prompt::prompt(&session.preface, &session.text_before))
    }

    fn show(&mut self, suggestion: String) -> Reaction {
        self.offer = Offer::Shown(suggestion.clone());
        Reaction::Show(suggestion)
    }

    fn text_before(&self) -> &str {
        self.session
            .as_ref()
            .map_or("", |session| session.text_before.as_str())
    }

    fn keep_recent_text(&mut self) {
        if let Some(session) = &mut self.session {
            let kept = last_chars(&session.text_before, MAX_CHARS_BEFORE_CARET).len();
            session.text_before.drain(..session.text_before.len() - kept);
        }
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
            .field("text_before_chars", &self.text_before().chars().count())
            .field("is_waiting", &matches!(self.offer, Offer::Requested { .. }))
            .field("has_suggestion", &matches!(self.offer, Offer::Shown(_)))
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::Duration;

    use super::*;
    use crate::app::Field;
    use crate::desktop::{FieldText, Surroundings};

    #[derive(Default)]
    struct FakeDesktop {
        surroundings: Surroundings,
        clipboard: Option<String>,
        surroundings_reads: Cell<usize>,
        clipboard_reads: Cell<usize>,
    }

    impl FakeDesktop {
        fn with_field_text(before_caret: &str) -> Self {
            Self {
                surroundings: Surroundings {
                    field_text: Some(FieldText {
                        before_caret: before_caret.to_owned(),
                        after_caret: String::new(),
                    }),
                    ..Surroundings::default()
                },
                ..Self::default()
            }
        }

        fn with_clipboard(text: &str) -> Self {
            Self {
                clipboard: Some(text.to_owned()),
                ..Self::default()
            }
        }
    }

    impl Desktop for FakeDesktop {
        fn field(&self) -> Field {
            Field::Open
        }

        fn surroundings(&self) -> Surroundings {
            self.surroundings_reads.set(self.surroundings_reads.get() + 1);
            self.surroundings.clone()
        }

        fn clipboard_text(&self) -> Option<String> {
            self.clipboard_reads.set(self.clipboard_reads.get() + 1);
            self.clipboard.clone()
        }
    }

    fn press(engine: &mut Engine, key: Key) -> Reaction {
        engine.on_key(key, &FakeDesktop::default())
    }

    fn engine_with(settings: &Settings) -> Engine {
        let mut engine = Engine::default();
        engine.use_settings(settings);
        engine
    }

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
            .map(|character| press(engine, Key::Char(character)))
            .last()
            .expect("text is not empty")
    }

    fn engine_waiting_for(context: &str) -> Engine {
        let mut engine = Engine::default();
        for character in context.chars() {
            engine.on_completion(completion(""));
            press(&mut engine, Key::Char(character));
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

        let reaction = press(&mut engine, Key::Char('h'));

        assert_eq!(reaction, request("h"));
    }

    #[test]
    fn typing_while_waiting_hides() {
        let mut engine = engine_waiting_for("h");

        let reaction = press(&mut engine, Key::Char('e'));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn whitespace_context_requests_nothing() {
        let mut engine = Engine::default();

        let reaction = press(&mut engine, Key::Char(' '));

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
        press(&mut engine, Key::Char('l'));

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

        let reaction = press(&mut engine, Key::Char('l'));

        assert_eq!(reaction, show("o world"));
    }

    #[test]
    fn typing_other_character_requests_new_completion() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = press(&mut engine, Key::Char('p'));

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

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, accept("lo", Some(" big world")));
    }

    #[test]
    fn tab_accepts_word_with_space_before_it() {
        let mut engine = engine_showing("hello", " big world");

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, accept(" big", Some(" world")));
    }

    #[test]
    fn tab_after_accepted_word_accepts_following_word() {
        let mut engine = engine_showing("hel", "lo big world");
        press(&mut engine, Key::Tab);

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, accept(" big", Some(" world")));
    }

    #[test]
    fn tab_accepts_last_word_and_hides() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, accept("lo", None));
    }

    #[test]
    fn typing_after_accepted_word_follows_rest() {
        let mut engine = engine_showing("hel", "lo big world");
        press(&mut engine, Key::Tab);

        let reaction = press(&mut engine, Key::Char(' '));

        assert_eq!(reaction, show("big world"));
    }

    #[test]
    fn tab_without_suggestion_passes_through() {
        let mut engine = engine_waiting_for("hel");

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn accepted_word_becomes_context() {
        let mut engine = engine_showing("hel", "lo big world");
        press(&mut engine, Key::Tab);

        let reaction = press(&mut engine, Key::Char('!'));

        assert_eq!(reaction, request("hello!"));
    }

    #[test]
    fn suggestion_is_offered_once_after_accept() {
        let mut engine = engine_showing("hel", "lo");
        press(&mut engine, Key::Tab);

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn backspace_requests_completion_of_shorter_context() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = press(&mut engine, Key::Backspace);

        assert_eq!(reaction, request("he"));
    }

    #[test]
    fn backspace_on_empty_context_hides() {
        let mut engine = Engine::default();

        let reaction = press(&mut engine, Key::Backspace);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn escape_hides_suggestion() {
        let mut engine = engine_showing("hel", "lo");

        let reaction = press(&mut engine, Key::Escape);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn tab_after_escape_passes_through() {
        let mut engine = engine_showing("hel", "lo");
        press(&mut engine, Key::Escape);

        let reaction = press(&mut engine, Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn escape_while_waiting_drops_completion() {
        let mut engine = engine_waiting_for("hel");
        press(&mut engine, Key::Escape);

        let reaction = engine.on_completion(completion("lo"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn typing_after_escape_while_waiting_requests_again() {
        let mut engine = engine_waiting_for("hel");
        press(&mut engine, Key::Escape);
        press(&mut engine, Key::Char('p'));

        let reaction = engine.on_completion(completion("lo"));

        assert_eq!(reaction, request("help"));
    }

    #[test]
    fn other_key_clears_context() {
        let mut engine = engine_showing("he", "llo");
        press(&mut engine, Key::Other);

        let reaction = press(&mut engine, Key::Char('l'));

        assert_eq!(reaction, request("l"));
    }

    #[test]
    fn control_character_clears_context() {
        let mut engine = engine_showing("he", "llo");
        press(&mut engine, Key::Char('\r'));

        let reaction = press(&mut engine, Key::Char('l'));

        assert_eq!(reaction, request("l"));
    }

    #[test]
    fn reset_hides_and_clears_context() {
        let mut engine = engine_showing("he", "llo");

        let reset_reaction = engine.reset();
        let typed_reaction = press(&mut engine, Key::Char('l'));

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

        let reaction = press(&mut engine, Key::Char('x'));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn context_keeps_only_recent_characters() {
        let mut engine = engine_waiting_for(&format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 1)));
        engine.on_completion(completion(""));

        let reaction = press(&mut engine, Key::Char('y'));

        assert_eq!(reaction, request(&"y".repeat(MAX_CHARS_BEFORE_CARET)));
    }

    #[test]
    fn first_key_continues_the_field_text_before_the_caret() {
        let mut engine = Engine::default();

        let reaction = engine.on_key(Key::Char('l'), &FakeDesktop::with_field_text("Hello wor"));

        assert_eq!(reaction, request("Hello worl"));
    }

    #[test]
    fn field_text_is_read_once_per_session() {
        let mut engine = Engine::default();
        let desktop = FakeDesktop::with_field_text("Hello ");
        engine.on_key(Key::Char('w'), &desktop);
        engine.on_completion(completion(""));

        let reaction = engine.on_key(Key::Char('o'), &desktop);

        assert_eq!(
            (reaction, desktop.surroundings_reads.get()),
            (request("Hello wo"), 1)
        );
    }

    #[test]
    fn reset_reads_the_field_again() {
        let mut engine = Engine::default();
        engine.on_key(Key::Char('x'), &FakeDesktop::with_field_text("Old "));
        engine.on_completion(completion(""));
        engine.reset();

        let reaction = engine.on_key(Key::Char('d'), &FakeDesktop::with_field_text("New wor"));

        assert_eq!(reaction, request("New word"));
    }

    #[test]
    fn backspace_deletes_from_the_field_text() {
        let mut engine = Engine::default();

        let reaction = engine.on_key(Key::Backspace, &FakeDesktop::with_field_text("Helloo"));

        assert_eq!(reaction, request("Hello"));
    }

    #[test]
    fn long_field_text_keeps_its_end() {
        let mut engine = Engine::default();
        let before_caret = format!("x{}", "y".repeat(MAX_CHARS_BEFORE_CARET - 1));

        let reaction = engine.on_key(Key::Char('y'), &FakeDesktop::with_field_text(&before_caret));

        assert_eq!(reaction, request(&"y".repeat(MAX_CHARS_BEFORE_CARET)));
    }

    #[test]
    fn system_prompt_comes_before_the_text() {
        let mut engine = engine_with(&Settings {
            custom_system_prompt: Some(SystemPrompt("Formal.".to_owned())),
            ..Settings::default()
        });

        let reaction = press(&mut engine, Key::Char('h'));

        assert_eq!(reaction, request("Formal.\n\nh"));
    }

    #[test]
    fn allowed_clipboard_text_comes_before_the_text() {
        let mut engine = engine_with(&Settings {
            custom_system_prompt: Some(SystemPrompt(String::new())),
            clipboard_access: ClipboardAccess::Allowed,
            ..Settings::default()
        });

        let reaction = engine.on_key(Key::Char('h'), &FakeDesktop::with_clipboard("1Z999"));

        assert_eq!(reaction, request("Text on the clipboard: “1Z999”\n\nh"));
    }

    #[test]
    fn denied_clipboard_is_never_read() {
        let mut engine = engine_with(&Settings {
            custom_system_prompt: Some(SystemPrompt(String::new())),
            ..Settings::default()
        });
        let desktop = FakeDesktop::with_clipboard("1Z999");

        let reaction = engine.on_key(Key::Char('h'), &desktop);

        assert_eq!((reaction, desktop.clipboard_reads.get()), (request("h"), 0));
    }

    #[test]
    fn new_settings_apply_from_the_next_session() {
        let mut engine = Engine::default();
        press(&mut engine, Key::Char('h'));
        engine.on_completion(completion(""));
        engine.use_settings(&Settings::default());

        let reaction = press(&mut engine, Key::Char('i'));

        assert_eq!(reaction, request("hi"));
    }

    #[test]
    fn debug_does_not_reveal_typed_or_suggested_text() {
        let engine = engine_showing("secret", "word");

        let debug = format!("{engine:?}");

        assert!(!debug.contains("secret") && !debug.contains("word"));
    }
}
