//! Gives the fake desktop and the helpers that several groups of engine tests share.

use std::cell::Cell;
use std::time::Duration;

use crate::desktop::{CopiedText, Desktop, Field, FieldLabels, FieldText, Surroundings};
use crate::engine::{AfterAccept, Engine, Reaction};
use crate::history::HistoryEntry;
use crate::inference::{
    Completion, InferenceError, InferenceRequest, LogProbability, NextWordBias, TokenCount,
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

    pub(super) fn in_field(app_id: &str, field_name: &str) -> Self {
        Self {
            surroundings: Surroundings {
                app_id: Some(app_id.to_owned()),
                field_labels: FieldLabels {
                    name: Some(field_name.to_owned()),
                    ..FieldLabels::default()
                },
                ..Surroundings::default()
            },
            ..Self::default()
        }
    }

    pub(super) fn with_copy(text: &str, age: Option<Duration>) -> Self {
        Self {
            copied: Some(CopiedText {
                text: text.to_owned(),
                age,
            }),
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

pub(super) fn request_with(lines: &[&str], text_before: &str) -> Reaction {
    Reaction::Request(InferenceRequest::Complete {
        prompt: prompt::framed_prompt(lines, text_before),
        max_tokens: Settings::default().suggestion_length_in_use().max_tokens(),
        word_bias: None,
        next_word_bias: NextWordBias::default(),
        min_log_probability: Some(Settings::default().suggestion_length_in_use().tail_cut()),
        min_typed_log_probability: Settings::default().typo_rule.typed_letters_cut(),
        fixes_typos: true,
    })
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

pub(super) fn mail_entry(text: &str) -> HistoryEntry {
    HistoryEntry {
        text: text.to_owned(),
        app_id: "com.apple.mail".to_owned(),
        domain: None,
    }
}

pub(super) fn mail_app(scope: HistoryScope) -> HistoryApp {
    HistoryApp {
        app_id: "com.apple.mail".to_owned(),
        app_name: "Mail".to_owned(),
        app_path: None,
        scope,
    }
}

pub(super) fn mail_field() -> FakeDesktop {
    FakeDesktop {
        surroundings: Surroundings {
            app_id: Some("com.apple.mail".to_owned()),
            ..Surroundings::default()
        },
        ..FakeDesktop::default()
    }
}

pub(super) const GREETING: &str = "Hi Ana,\n\n";
pub(super) const TYPED_TEXT: &str = "The report is ready, and the numbers look goo";

pub(super) fn field_of(app_id: Option<&str>, field_name: Option<&str>, before_caret: &str) -> FakeDesktop {
    FakeDesktop {
        surroundings: Surroundings {
            app_id: app_id.map(str::to_owned),
            field_labels: FieldLabels {
                name: field_name.map(str::to_owned),
                ..FieldLabels::default()
            },
            field_text: Some(FieldText {
                before_caret: before_caret.to_owned(),
                after_caret: String::new(),
            }),
            ..Surroundings::default()
        },
        ..FakeDesktop::default()
    }
}

/// Types `typed` and the key `d` in `desktop`, accepts a suggestion when `accepts` and ends
/// the session. Returns the history to save.
pub(super) fn finish_typing_text(
    engine: &mut Engine,
    desktop: &FakeDesktop,
    typed: &str,
    accepts: bool,
) -> Option<Vec<HistoryEntry>> {
    for character in typed.chars() {
        if let Reaction::Request(_) = engine.on_key(Key::Char(character), desktop) {
            engine.on_completion(completion(""));
        }
    }
    engine.on_key(Key::Char('d'), desktop);
    engine.on_completion(completion(". Best, Tom"));
    if accepts {
        engine.on_key(Key::Tab, desktop);
    }
    engine.on_key(Key::Other, desktop);
    engine.history_to_save().map(<[HistoryEntry]>::to_vec)
}

pub(super) fn finish_typing(
    engine: &mut Engine,
    desktop: &FakeDesktop,
    accepts: bool,
) -> Option<Vec<HistoryEntry>> {
    finish_typing_text(engine, desktop, TYPED_TEXT, accepts)
}
