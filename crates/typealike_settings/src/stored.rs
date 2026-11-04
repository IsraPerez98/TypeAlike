//! Converts the settings to and from the format of the settings file.

mod energy_saving;
mod personalization;
mod screen_text;
mod suggestion_places;

use serde::{Deserialize, Serialize};
use typealike_core::hub::ModelFile;
use typealike_core::inference::Backend;
use typealike_core::localization::UiLanguage;
use typealike_core::settings::{
    ClipboardAccess, PromptPreparation, Settings, Setup, SuggestionDisplay, SuggestionLength, TypoFix,
    TypoRule, WriterName, WriterNotes,
};

use energy_saving::StoredEnergySaving;
use personalization::StoredPersonalization;
use screen_text::StoredScreenText;
use suggestion_places::StoredSuggestionPlaces;

/// The settings as the file stores them. Missing fields take their default values, so a file
/// from an older version still loads.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct StoredSettings {
    backend: StoredBackend,
    suggestion_model: Option<StoredModelFile>,
    suggestion_places: StoredSuggestionPlaces,
    writer_name: String,
    /// `None` when the file has no notes, so that the user gets the default notes.
    #[serde(default)]
    writer_notes: Option<String>,
    /// The text that older versions used in place of the frame of the prompt. It becomes the
    /// writer notes, so that the user keeps the text.
    #[serde(skip_serializing)]
    system_prompt: Option<String>,
    clipboard_access: StoredClipboardAccess,
    screen_text: StoredScreenText,
    personalization: StoredPersonalization,
    energy_saving: StoredEnergySaving,
    prompt_preparation: StoredPromptPreparation,
    suggestion_display: StoredSuggestionDisplay,
    suggestion_length: StoredSuggestionLength,
    typo_rule: StoredTypoRule,
    typo_fix: StoredTypoFix,
    /// `None` follows the language of the OS.
    ui_language: Option<StoredUiLanguage>,
    /// A file of an older version has no setup, and counts as finished.
    setup: StoredSetup,
}

impl Default for StoredSettings {
    fn default() -> Self {
        Settings::default().into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoredBackend {
    Cpu,
    Vulkan,
    Cuda,
    Metal,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoredClipboardAccess {
    Denied,
    Allowed,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredPromptPreparation {
    OnFirstKey,
    OnFocus,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredSuggestionDisplay {
    WhenComplete,
    WordByWord,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredTypoRule {
    HideSuggestions,
    Suggest,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredTypoFix {
    Offer,
    Off,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredUiLanguage {
    English,
    Spanish,
    Portuguese,
    French,
    German,
    Italian,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredSetup {
    Needed,
    Finished,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredSuggestionLength {
    VeryShort,
    Short,
    Medium,
    Long,
    VeryLong,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredModelFile {
    repo: String,
    name: String,
}

impl From<Settings> for StoredSettings {
    fn from(settings: Settings) -> Self {
        let backend = match settings.backend {
            Backend::Cpu => StoredBackend::Cpu,
            Backend::Vulkan => StoredBackend::Vulkan,
            Backend::Cuda => StoredBackend::Cuda,
            Backend::Metal => StoredBackend::Metal,
        };
        let suggestion_model = settings.suggestion_model.map(|file| StoredModelFile {
            repo: file.repo,
            name: file.name,
        });
        let clipboard_access = match settings.clipboard_access {
            ClipboardAccess::Denied => StoredClipboardAccess::Denied,
            ClipboardAccess::Allowed => StoredClipboardAccess::Allowed,
        };
        let prompt_preparation = match settings.prompt_preparation {
            PromptPreparation::OnFirstKey => StoredPromptPreparation::OnFirstKey,
            PromptPreparation::OnFocus => StoredPromptPreparation::OnFocus,
        };
        let suggestion_display = match settings.suggestion_display {
            SuggestionDisplay::WhenComplete => StoredSuggestionDisplay::WhenComplete,
            SuggestionDisplay::WordByWord => StoredSuggestionDisplay::WordByWord,
        };
        let suggestion_length = match settings.suggestion_length {
            SuggestionLength::VeryShort => StoredSuggestionLength::VeryShort,
            SuggestionLength::Short => StoredSuggestionLength::Short,
            SuggestionLength::Medium => StoredSuggestionLength::Medium,
            SuggestionLength::Long => StoredSuggestionLength::Long,
            SuggestionLength::VeryLong => StoredSuggestionLength::VeryLong,
        };
        let typo_rule = match settings.typo_rule {
            TypoRule::HideSuggestions => StoredTypoRule::HideSuggestions,
            TypoRule::Suggest => StoredTypoRule::Suggest,
        };
        let typo_fix = match settings.typo_fix {
            TypoFix::Offer => StoredTypoFix::Offer,
            TypoFix::Off => StoredTypoFix::Off,
        };
        let ui_language = settings.ui_language.map(|language| match language {
            UiLanguage::English => StoredUiLanguage::English,
            UiLanguage::Spanish => StoredUiLanguage::Spanish,
            UiLanguage::Portuguese => StoredUiLanguage::Portuguese,
            UiLanguage::French => StoredUiLanguage::French,
            UiLanguage::German => StoredUiLanguage::German,
            UiLanguage::Italian => StoredUiLanguage::Italian,
        });
        let setup = match settings.setup {
            Setup::Needed => StoredSetup::Needed,
            Setup::Finished => StoredSetup::Finished,
        };
        Self {
            backend,
            suggestion_model,
            suggestion_places: settings.suggestion_places.into(),
            writer_name: settings.writer_name.0,
            writer_notes: Some(settings.writer_notes.0),
            system_prompt: None,
            clipboard_access,
            screen_text: settings.screen_text.into(),
            personalization: settings.personalization.into(),
            energy_saving: settings.energy_saving.into(),
            prompt_preparation,
            suggestion_display,
            suggestion_length,
            typo_rule,
            typo_fix,
            ui_language,
            setup,
        }
    }
}

impl From<StoredSettings> for Settings {
    fn from(stored: StoredSettings) -> Self {
        let backend = match stored.backend {
            StoredBackend::Cpu => Backend::Cpu,
            StoredBackend::Vulkan => Backend::Vulkan,
            StoredBackend::Cuda => Backend::Cuda,
            StoredBackend::Metal => Backend::Metal,
        };
        let suggestion_model = stored.suggestion_model.map(|file| ModelFile {
            repo: file.repo,
            name: file.name,
        });
        let clipboard_access = match stored.clipboard_access {
            StoredClipboardAccess::Denied => ClipboardAccess::Denied,
            StoredClipboardAccess::Allowed => ClipboardAccess::Allowed,
        };
        let prompt_preparation = match stored.prompt_preparation {
            StoredPromptPreparation::OnFirstKey => PromptPreparation::OnFirstKey,
            StoredPromptPreparation::OnFocus => PromptPreparation::OnFocus,
        };
        let suggestion_display = match stored.suggestion_display {
            StoredSuggestionDisplay::WhenComplete => SuggestionDisplay::WhenComplete,
            StoredSuggestionDisplay::WordByWord => SuggestionDisplay::WordByWord,
        };
        let suggestion_length = match stored.suggestion_length {
            StoredSuggestionLength::VeryShort => SuggestionLength::VeryShort,
            StoredSuggestionLength::Short => SuggestionLength::Short,
            StoredSuggestionLength::Medium => SuggestionLength::Medium,
            StoredSuggestionLength::Long => SuggestionLength::Long,
            StoredSuggestionLength::VeryLong => SuggestionLength::VeryLong,
        };
        let typo_rule = match stored.typo_rule {
            StoredTypoRule::HideSuggestions => TypoRule::HideSuggestions,
            StoredTypoRule::Suggest => TypoRule::Suggest,
        };
        let typo_fix = match stored.typo_fix {
            StoredTypoFix::Offer => TypoFix::Offer,
            StoredTypoFix::Off => TypoFix::Off,
        };
        let ui_language = stored.ui_language.map(|language| match language {
            StoredUiLanguage::English => UiLanguage::English,
            StoredUiLanguage::Spanish => UiLanguage::Spanish,
            StoredUiLanguage::Portuguese => UiLanguage::Portuguese,
            StoredUiLanguage::French => UiLanguage::French,
            StoredUiLanguage::German => UiLanguage::German,
            StoredUiLanguage::Italian => UiLanguage::Italian,
        });
        let setup = match stored.setup {
            StoredSetup::Needed => Setup::Needed,
            StoredSetup::Finished => Setup::Finished,
        };
        let writer_notes = match (stored.writer_notes, stored.system_prompt) {
            (Some(notes), _) if !notes.is_empty() => WriterNotes(notes),
            (_, Some(system_prompt)) => WriterNotes(system_prompt),
            (Some(notes), None) => WriterNotes(notes),
            (None, None) => WriterNotes::default(),
        };
        Self {
            backend,
            suggestion_model,
            suggestion_places: stored.suggestion_places.into(),
            writer_name: WriterName(stored.writer_name),
            writer_notes,
            clipboard_access,
            screen_text: stored.screen_text.into(),
            personalization: stored.personalization.into(),
            energy_saving: stored.energy_saving.into(),
            prompt_preparation,
            suggestion_display,
            suggestion_length,
            typo_rule,
            typo_fix,
            ui_language,
            setup,
        }
    }
}

pub(super) fn parse(json: &str) -> Result<Settings, serde_json::Error> {
    serde_json::from_str::<StoredSettings>(json).map(Settings::from)
}

pub(super) fn to_json(settings: &Settings) -> String {
    serde_json::to_string_pretty(&StoredSettings::from(settings.clone()))
        .expect("settings of plain enums and strings always serialize")
}

#[cfg(test)]
mod tests;
