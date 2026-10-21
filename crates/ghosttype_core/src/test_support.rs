//! The requests, completions, builders and lookups that the tests of several modules share.

use std::time::Duration;

use crate::app::{BadgeView, Status};
use crate::hub::ModelFile;
use crate::inference::{
    Completion, InferenceError, InferenceRequest, LogProbability, NextWordBias, TokenCount,
};
use crate::prompt::framed_prompt;
use crate::settings::{HistoryApp, HistoryScope, ScreenTextApp, Settings};
use crate::settings_window::view::{ControlId, Page, Row, SettingsInput};

pub(crate) const ACTIVE_BADGE: BadgeView = BadgeView {
    status: Status::Active,
    label: "GhostType",
};

pub(crate) const PAUSED_BADGE: BadgeView = BadgeView {
    status: Status::Paused,
    label: "GhostType (paused)",
};

/// The request for a suggestion with the default settings, after the `lines` of the preface and
/// `text_before`.
pub(crate) fn suggestion_request(lines: &[&str], text_before: &str) -> InferenceRequest {
    InferenceRequest::Complete {
        prompt: framed_prompt(lines, text_before),
        max_tokens: Settings::default().suggestion_length_in_use().max_tokens(),
        word_bias: None,
        next_word_bias: NextWordBias::default(),
        min_log_probability: Some(Settings::default().suggestion_length_in_use().tail_cut()),
        min_typed_log_probability: Settings::default().typo_rule.typed_letters_cut(),
        fixes_typos: true,
    }
}

/// The request that only decodes the prompt with the `lines` of the preface and `text_before`.
pub(crate) fn preparation_request(lines: &[&str], text_before: &str) -> InferenceRequest {
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

#[expect(clippy::unnecessary_wraps, reason = "a completion answers with a Result")]
pub(crate) fn completion(text: &str) -> Result<Completion, InferenceError> {
    Ok(Completion {
        text: text.to_owned(),
        tokens: TokenCount(1),
        duration: Duration::ZERO,
        word_log_probabilities: Vec::new(),
        typed_log_probability: LogProbability::default(),
        correction: None,
    })
}

pub(crate) fn mail_app(is_on: bool) -> ScreenTextApp {
    screen_text_app("com.apple.mail", "Mail", is_on)
}

pub(crate) fn mail_history_app(scope: HistoryScope) -> HistoryApp {
    history_app("com.apple.mail", "Mail", scope)
}

impl Page {
    /// The row whose control is `id`.
    pub(crate) fn row(&self, id: &str) -> Option<&Row> {
        self.sections
            .iter()
            .flat_map(|section| &section.rows)
            .find(|row| row.id().is_some_and(|row_id| row_id.as_str() == id))
    }
}

pub(crate) fn toggled(id: &str, is_on: bool) -> SettingsInput {
    SettingsInput::Toggled {
        id: ControlId::from(id.to_owned()),
        is_on,
    }
}

pub(crate) fn picked(id: &'static str, index: usize) -> SettingsInput {
    SettingsInput::Picked {
        id: ControlId::new(id),
        index,
    }
}

pub(crate) fn model_file(name: &str) -> ModelFile {
    ModelFile {
        repo: "owner/repo".to_owned(),
        name: name.to_owned(),
    }
}

pub(crate) fn screen_text_app(app_id: &str, app_name: &str, is_on: bool) -> ScreenTextApp {
    ScreenTextApp {
        app_id: app_id.to_owned(),
        app_name: app_name.to_owned(),
        app_path: None,
        is_on,
    }
}

pub(crate) fn history_app(app_id: &str, app_name: &str, scope: HistoryScope) -> HistoryApp {
    HistoryApp {
        app_id: app_id.to_owned(),
        app_name: app_name.to_owned(),
        app_path: None,
        scope,
    }
}
