//! Converts the personalization settings to and from the format of the settings file.

use ghosttype_core::settings::{
    HistoryAccess, HistoryApp, HistoryScope, KeptTexts, PersonalizationSettings, WordChoice,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct StoredPersonalization {
    history: StoredHistoryAccess,
    kept_texts: StoredKeptTexts,
    apps: Vec<StoredHistoryApp>,
    word_choice: StoredWordChoice,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredHistoryApp {
    app_id: String,
    app_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    app_path: Option<String>,
    #[serde(default)]
    scope: Option<StoredHistoryScope>,
    /// The switch of an app in a file of an older version, which had no scope.
    #[serde(default, skip_serializing)]
    is_on: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredHistoryScope {
    Off,
    ThisApp,
    AllApps,
}

impl Default for StoredPersonalization {
    fn default() -> Self {
        PersonalizationSettings::default().into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoredHistoryAccess {
    Off,
    On,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredKeptTexts {
    AcceptedOnly,
    All,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoredWordChoice {
    Off,
    Light,
    Medium,
    Strong,
}

impl From<PersonalizationSettings> for StoredPersonalization {
    fn from(settings: PersonalizationSettings) -> Self {
        let history = match settings.history {
            HistoryAccess::Off => StoredHistoryAccess::Off,
            HistoryAccess::On => StoredHistoryAccess::On,
        };
        let kept_texts = match settings.kept_texts {
            KeptTexts::AcceptedOnly => StoredKeptTexts::AcceptedOnly,
            KeptTexts::All => StoredKeptTexts::All,
        };
        let apps = settings
            .apps
            .into_iter()
            .map(|app| StoredHistoryApp {
                app_id: app.app_id,
                app_name: app.app_name,
                app_path: app.app_path,
                scope: Some(match app.scope {
                    HistoryScope::Off => StoredHistoryScope::Off,
                    HistoryScope::ThisApp => StoredHistoryScope::ThisApp,
                    HistoryScope::AllApps => StoredHistoryScope::AllApps,
                }),
                is_on: None,
            })
            .collect();
        let word_choice = match settings.word_choice {
            WordChoice::Off => StoredWordChoice::Off,
            WordChoice::Light => StoredWordChoice::Light,
            WordChoice::Medium => StoredWordChoice::Medium,
            WordChoice::Strong => StoredWordChoice::Strong,
        };
        Self {
            history,
            kept_texts,
            apps,
            word_choice,
        }
    }
}

impl From<StoredPersonalization> for PersonalizationSettings {
    fn from(stored: StoredPersonalization) -> Self {
        let history = match stored.history {
            StoredHistoryAccess::Off => HistoryAccess::Off,
            StoredHistoryAccess::On => HistoryAccess::On,
        };
        let kept_texts = match stored.kept_texts {
            StoredKeptTexts::AcceptedOnly => KeptTexts::AcceptedOnly,
            StoredKeptTexts::All => KeptTexts::All,
        };
        let apps = stored
            .apps
            .into_iter()
            .map(|app| HistoryApp {
                scope: match (app.scope, app.is_on) {
                    (Some(StoredHistoryScope::Off), _) | (None, Some(false)) => HistoryScope::Off,
                    (Some(StoredHistoryScope::ThisApp), _) => HistoryScope::ThisApp,
                    (Some(StoredHistoryScope::AllApps) | None, _) => HistoryScope::AllApps,
                },
                app_id: app.app_id,
                app_name: app.app_name,
                app_path: app.app_path,
            })
            .collect();
        let word_choice = match stored.word_choice {
            StoredWordChoice::Off => WordChoice::Off,
            StoredWordChoice::Light => WordChoice::Light,
            StoredWordChoice::Medium => WordChoice::Medium,
            StoredWordChoice::Strong => WordChoice::Strong,
        };
        Self {
            history,
            kept_texts,
            apps,
            word_choice,
        }
    }
}
