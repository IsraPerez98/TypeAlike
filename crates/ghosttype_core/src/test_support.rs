//! The builders and the lookups that the tests of several modules share.

use crate::hub::ModelFile;
use crate::settings::{HistoryApp, HistoryScope, ScreenTextApp};
use crate::settings_window::view::{ControlId, Page, Row, SettingsInput};

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
