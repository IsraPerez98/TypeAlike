//! The fake desktops, requests and completions that the app tests share.

use std::time::Duration;

use crate::app::App;
use crate::desktop::{CopiedText, Desktop, Field, FieldText, OcrAvailability, Surroundings};
use crate::engine::Reaction;
use crate::inference::{
    Completion, InferenceError, InferenceRequest, LogProbability, NextWordBias, TokenCount,
};
use crate::key::Key;
use crate::prompt::framed_prompt;
use crate::settings::{PromptPreparation, Settings};

pub(super) struct FakeDesktop(pub(super) Field);

impl Desktop for FakeDesktop {
    fn field(&self) -> Field {
        self.0
    }

    fn surroundings(&self) -> Surroundings {
        Surroundings::default()
    }

    fn copied_text(&self) -> Option<CopiedText> {
        None
    }
}

/// A text field of the app `app_id`.
pub(super) struct TextField {
    pub(super) app_id: &'static str,
    pub(super) field: Field,
    pub(super) ocr: OcrAvailability,
    pub(super) has_readable_text: bool,
}

impl Desktop for TextField {
    fn field(&self) -> Field {
        self.field
    }

    fn surroundings(&self) -> Surroundings {
        Surroundings {
            app_id: Some(self.app_id.to_owned()),
            field_text: self.has_readable_text.then(FieldText::default),
            ..Surroundings::default()
        }
    }

    fn copied_text(&self) -> Option<CopiedText> {
        None
    }

    fn ocr_availability(&self) -> OcrAvailability {
        self.ocr
    }
}

pub(super) const MAIL: TextField = TextField {
    app_id: "com.apple.mail",
    field: Field::Open,
    ocr: OcrAvailability::Unsupported,
    has_readable_text: true,
};

pub(super) fn preparation(lines: &[&str], text_before: &str) -> InferenceRequest {
    InferenceRequest::Complete {
        prompt: framed_prompt(lines, text_before),
        max_tokens: TokenCount(0),
        word_bias: None,
        next_word_bias: NextWordBias::default(),
        min_log_probability: None,
        min_typed_log_probability: None,
        fixes_typos: false,
    }
}

fn request_with(lines: &[&str], text_before: &str) -> Reaction {
    Reaction::Request(InferenceRequest::Complete {
        prompt: framed_prompt(lines, text_before),
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

pub(super) fn type_key(app: &mut App, character: char) -> Reaction {
    app.on_key(Key::Char(character), &FakeDesktop(Field::Open))
}

pub(super) fn app_preparing_on_focus() -> App {
    let mut app = App::new();
    app.use_settings(&Settings {
        prompt_preparation: PromptPreparation::OnFocus,
        ..Settings::default()
    });
    app
}
