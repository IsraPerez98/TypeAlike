//! Gives the fake desktop and the helpers that several groups of engine tests share.

use std::cell::Cell;
use std::time::Duration;

use crate::desktop::{CopiedText, Desktop, Field, FieldText, Surroundings};
use crate::engine::{AfterAccept, Engine, Reaction};
use crate::history::HistoryEntry;
use crate::inference::{
    Completion, InferenceError, InferenceRequest, LogProbability, NextWordBias, Prompt, TokenCount,
};
use crate::key::Key;
use crate::prompt;
use crate::settings::{
    HistoryAccess, HistoryApp, HistoryScope, PersonalizationSettings, Settings, WordChoice,
};

#[derive(Default)]
pub(super) struct FakeDesktop {
    pub(super) surroundings: Surroundings,
    pub(super) copied: Option<CopiedText>,
    pub(super) surroundings_reads: Cell<usize>,
    pub(super) clipboard_reads: Cell<usize>,
    pub(super) is_ghost_hidden: bool,
}

impl FakeDesktop {
    pub(super) fn with_field_text(before_caret: &str) -> Self {
        Self::with_text_around(before_caret, "")
    }

    pub(super) fn with_text_around(before_caret: &str, after_caret: &str) -> Self {
        Self {
            surroundings: Surroundings {
                field_text: Some(FieldText {
                    before_caret: before_caret.to_owned(),
                    after_caret: after_caret.to_owned(),
                }),
                ..Surroundings::default()
            },
            ..Self::default()
        }
    }

    pub(super) fn in_app(mut self, app_id: &str) -> Self {
        self.surroundings.app_id = Some(app_id.to_owned());
        self
    }

    pub(super) fn with_field_name(mut self, field_name: &str) -> Self {
        self.surroundings.field_labels.name = Some(field_name.to_owned());
        self
    }

    pub(super) fn with_copy(mut self, text: &str, age: Option<Duration>) -> Self {
        self.copied = Some(CopiedText {
            text: text.to_owned(),
            age,
        });
        self
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

    fn copied_text(&self) -> Option<CopiedText> {
        self.clipboard_reads.set(self.clipboard_reads.get() + 1);
        self.copied.clone()
    }

    fn is_ghost_shown(&self) -> bool {
        !self.is_ghost_hidden
    }
}

pub(super) fn press(engine: &mut Engine, key: Key) -> Reaction {
    engine.on_key(key, &FakeDesktop::default())
}

pub(super) fn engine_with(settings: &Settings) -> Engine {
    let mut engine = Engine::default();
    engine.use_settings(settings);
    engine
}

pub(super) fn engine_with_history(settings: &Settings, entries: Vec<HistoryEntry>) -> Engine {
    let mut engine = engine_with(settings);
    engine.use_history(entries);
    engine
}

/// The request of a completion without word bias, which a test changes where its settings differ
/// from the default settings.
pub(super) struct ExpectedRequest {
    pub(super) prompt: Prompt,
    pub(super) max_tokens: TokenCount,
    pub(super) min_log_probability: Option<LogProbability>,
    pub(super) min_typed_log_probability: Option<LogProbability>,
    pub(super) fixes_typos: bool,
}

impl ExpectedRequest {
    /// The request that the default settings give for `text_before` after the frame and `lines`.
    pub(super) fn framed(lines: &[&str], text_before: &str) -> Self {
        let settings = Settings::default();
        Self {
            prompt: prompt::framed_prompt(lines, text_before),
            max_tokens: settings.suggestion_length_in_use().max_tokens(),
            min_log_probability: Some(settings.suggestion_length_in_use().tail_cut()),
            min_typed_log_probability: settings.typo_rule.typed_letters_cut(),
            fixes_typos: true,
        }
    }
}

impl From<ExpectedRequest> for InferenceRequest {
    fn from(expected: ExpectedRequest) -> Self {
        Self::Complete {
            prompt: expected.prompt,
            max_tokens: expected.max_tokens,
            word_bias: None,
            next_word_bias: NextWordBias::default(),
            min_log_probability: expected.min_log_probability,
            min_typed_log_probability: expected.min_typed_log_probability,
            fixes_typos: expected.fixes_typos,
        }
    }
}

impl From<ExpectedRequest> for Reaction {
    fn from(expected: ExpectedRequest) -> Self {
        Self::Request(expected.into())
    }
}

pub(super) fn request_with(lines: &[&str], text_before: &str) -> Reaction {
    ExpectedRequest::framed(lines, text_before).into()
}

pub(super) fn request(text_before: &str) -> Reaction {
    request_with(&[], text_before)
}

#[expect(clippy::unnecessary_wraps, reason = "on_completion takes a Result")]
pub(super) fn completion(text: &str) -> Result<Completion, InferenceError> {
    Ok(Completion {
        text: text.to_owned(),
        tokens: TokenCount(1),
        duration: Duration::ZERO,
        word_log_probabilities: Vec::new(),
        typed_log_probability: LogProbability::default(),
        correction: None,
    })
}

pub(super) fn show(text: &str) -> Reaction {
    Reaction::Show(text.to_owned())
}

pub(super) fn accept(text: &str, rest: &str) -> Reaction {
    Reaction::Accept {
        text: text.to_owned(),
        then: AfterAccept::Rest(rest.to_owned()),
    }
}

pub(super) fn type_text(engine: &mut Engine, text: &str) -> Reaction {
    text.chars()
        .map(|character| press(engine, Key::Char(character)))
        .last()
        .expect("text is not empty")
}

pub(super) fn engine_waiting_for(context: &str) -> Engine {
    let mut engine = Engine::default();
    for character in context.chars() {
        engine.on_completion(completion(""));
        press(&mut engine, Key::Char(character));
    }
    engine
}

pub(super) fn engine_showing(context: &str, suggestion: &str) -> Engine {
    let mut engine = engine_waiting_for(context);
    engine.on_completion(completion(suggestion));
    engine
}

pub(super) fn history_settings(history: HistoryAccess, apps: Vec<HistoryApp>) -> Settings {
    Settings {
        personalization: PersonalizationSettings {
            history,
            apps,
            word_choice: WordChoice::Off,
            ..PersonalizationSettings::default()
        },
        ..Settings::default()
    }
}

pub(super) const MAIL_APP: &str = "com.apple.mail";

pub(super) fn mail_entry(text: &str) -> HistoryEntry {
    HistoryEntry {
        text: text.to_owned(),
        app_id: MAIL_APP.to_owned(),
        domain: None,
    }
}

pub(super) fn mail_app(scope: HistoryScope) -> HistoryApp {
    HistoryApp {
        app_id: MAIL_APP.to_owned(),
        app_name: "Mail".to_owned(),
        app_path: None,
        scope,
    }
}

/// A field of the mail app that the platform cannot read.
pub(super) fn mail_field() -> FakeDesktop {
    FakeDesktop::default().in_app(MAIL_APP)
}

pub(super) const GREETING: &str = "Hi Ana,\n\n";
pub(super) const TYPED_TEXT: &str = "The report is ready, and the numbers look goo";

/// What the user does with the suggestion at the end of a typed text.
#[derive(Clone, Copy)]
pub(super) enum Suggestion {
    Accepted,
    Ignored,
}

/// Types `typed` and the key `d` in `desktop`, accepts the suggestion when `suggestion` is
/// [`Suggestion::Accepted`] and ends the session. Returns the history to save.
pub(super) fn finish_typing_text(
    engine: &mut Engine,
    desktop: &FakeDesktop,
    typed: &str,
    suggestion: Suggestion,
) -> Option<Vec<HistoryEntry>> {
    for character in typed.chars() {
        if let Reaction::Request(_) = engine.on_key(Key::Char(character), desktop) {
            engine.on_completion(completion(""));
        }
    }
    engine.on_key(Key::Char('d'), desktop);
    engine.on_completion(completion(". Best, Tom"));
    if matches!(suggestion, Suggestion::Accepted) {
        engine.on_key(Key::Tab, desktop);
    }
    engine.on_key(Key::Other, desktop);
    engine.history_to_save().map(<[HistoryEntry]>::to_vec)
}

pub(super) fn finish_typing(
    engine: &mut Engine,
    desktop: &FakeDesktop,
    suggestion: Suggestion,
) -> Option<Vec<HistoryEntry>> {
    finish_typing_text(engine, desktop, TYPED_TEXT, suggestion)
}
