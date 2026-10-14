//! Decides what ghost text to show as the user types, and what Tab inserts.

use std::fmt;

use crate::key::Key;
use crate::suggestion::SuggestionProvider;

const MAX_CONTEXT_CHARS: usize = 200;

/// What the platform must do after a key press or context change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reaction {
    /// Let the key through and show this ghost text after the caret.
    Show(String),
    /// Let the key through and hide any ghost text.
    Hide,
    /// Swallow the key, hide the ghost text and type this text into the focused app.
    Accept(String),
}

/// Tracks the text typed at the caret and the suggestion currently offered for it.
pub(crate) struct Engine {
    provider: Box<dyn SuggestionProvider>,
    context: String,
    suggestion: Option<String>,
}

impl Engine {
    /// Creates an engine with an empty context that asks `provider` for completions.
    #[must_use]
    pub fn new(provider: Box<dyn SuggestionProvider>) -> Self {
        Self {
            provider,
            context: String::new(),
            suggestion: None,
        }
    }

    /// Updates the context for a key pressed in the focused app.
    pub fn on_key(&mut self, key: Key) -> Reaction {
        match key {
            Key::Char(character) if !character.is_control() => {
                self.append(&character.to_string());
                self.refresh()
            }
            Key::Backspace => {
                self.context.pop();
                self.refresh()
            }
            Key::Tab => self.accept(),
            Key::Escape => {
                self.suggestion = None;
                Reaction::Hide
            }
            Key::Char(_) | Key::Other => self.reset(),
        }
    }

    /// Forgets the typed context, for when the caret moved somewhere unknown.
    pub fn reset(&mut self) -> Reaction {
        self.context.clear();
        self.suggestion = None;
        Reaction::Hide
    }

    fn accept(&mut self) -> Reaction {
        match self.suggestion.take() {
            Some(completion) => {
                self.append(&completion);
                Reaction::Accept(completion)
            }
            None => self.reset(),
        }
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

    fn refresh(&mut self) -> Reaction {
        self.suggestion = self.provider.suggest(&self.context);
        self.suggestion.clone().map_or(Reaction::Hide, Reaction::Show)
    }
}

impl fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("context_chars", &self.context.chars().count())
            .field("has_suggestion", &self.suggestion.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    const TRIGGER: &str = "hel";
    const COMPLETION: &str = "lo";

    #[derive(Default)]
    struct FakeProvider {
        last_context: Rc<RefCell<String>>,
    }

    impl SuggestionProvider for FakeProvider {
        fn suggest(&self, context: &str) -> Option<String> {
            context.clone_into(&mut self.last_context.borrow_mut());
            context.ends_with(TRIGGER).then(|| COMPLETION.to_owned())
        }
    }

    fn engine() -> Engine {
        Engine::new(Box::new(FakeProvider::default()))
    }

    fn engine_recording_context() -> (Engine, Rc<RefCell<String>>) {
        let provider = FakeProvider::default();
        let last_context = Rc::clone(&provider.last_context);
        (Engine::new(Box::new(provider)), last_context)
    }

    fn type_text(engine: &mut Engine, text: &str) -> Reaction {
        text.chars()
            .map(|character| engine.on_key(Key::Char(character)))
            .last()
            .expect("text is not empty")
    }

    #[test]
    fn typing_trigger_shows_completion() {
        let mut engine = engine();

        let reaction = type_text(&mut engine, "hel");

        assert_eq!(reaction, Reaction::Show(COMPLETION.to_owned()));
    }

    #[test]
    fn typing_without_match_hides() {
        let mut engine = engine();

        let reaction = type_text(&mut engine, "help");

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn tab_accepts_visible_suggestion() {
        let mut engine = engine();
        type_text(&mut engine, "hel");

        let reaction = engine.on_key(Key::Tab);

        assert_eq!(reaction, Reaction::Accept(COMPLETION.to_owned()));
    }

    #[test]
    fn tab_without_suggestion_passes_through() {
        let mut engine = engine();
        type_text(&mut engine, "help");

        let reaction = engine.on_key(Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn accepted_text_becomes_context() {
        let (mut engine, last_context) = engine_recording_context();
        type_text(&mut engine, "hel");
        engine.on_key(Key::Tab);

        type_text(&mut engine, "!");

        assert_eq!(*last_context.borrow(), "hello!");
    }

    #[test]
    fn suggestion_is_offered_once_after_accept() {
        let mut engine = engine();
        type_text(&mut engine, "hel");
        engine.on_key(Key::Tab);

        let reaction = engine.on_key(Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn backspace_restores_suggestion() {
        let mut engine = engine();
        type_text(&mut engine, "help");

        let reaction = engine.on_key(Key::Backspace);

        assert_eq!(reaction, Reaction::Show(COMPLETION.to_owned()));
    }

    #[test]
    fn backspace_on_empty_context_hides() {
        let mut engine = engine();

        let reaction = engine.on_key(Key::Backspace);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn escape_hides_suggestion() {
        let mut engine = engine();
        type_text(&mut engine, "hel");

        let reaction = engine.on_key(Key::Escape);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn tab_after_escape_passes_through() {
        let mut engine = engine();
        type_text(&mut engine, "hel");
        engine.on_key(Key::Escape);

        let reaction = engine.on_key(Key::Tab);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn other_key_clears_context() {
        let mut engine = engine();
        type_text(&mut engine, "he");
        engine.on_key(Key::Other);

        let reaction = type_text(&mut engine, "l");

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn control_character_clears_context() {
        let mut engine = engine();
        type_text(&mut engine, "he\r");

        let reaction = type_text(&mut engine, "l");

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn reset_hides_and_clears_context() {
        let mut engine = engine();
        type_text(&mut engine, "he");

        let reset_reaction = engine.reset();
        let typed_reaction = type_text(&mut engine, "l");

        assert_eq!((reset_reaction, typed_reaction), (Reaction::Hide, Reaction::Hide));
    }

    #[test]
    fn context_keeps_only_recent_characters() {
        let (mut engine, last_context) = engine_recording_context();
        let typed = format!("x{}", "y".repeat(MAX_CONTEXT_CHARS));

        type_text(&mut engine, &typed);

        assert_eq!(*last_context.borrow(), "y".repeat(MAX_CONTEXT_CHARS));
    }

    #[test]
    fn debug_does_not_reveal_typed_text() {
        let mut engine = engine();
        type_text(&mut engine, "secret");

        let debug = format!("{engine:?}");

        assert!(!debug.contains("secret"));
    }
}
