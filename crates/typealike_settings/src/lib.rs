//! Keeps the settings in a JSON file in the app's data folder, so they last between runs.

mod stored;

use std::fs;
use std::io;
use std::path::PathBuf;

use typealike_core::settings::{Settings, SettingsStore};

use stored::{parse, to_json};

/// The settings file: it gives the saved settings at start, and keeps each change. Without a
/// file, it gives the settings of the first start on this computer.
#[derive(Debug)]
pub struct SettingsFile {
    path: PathBuf,
    first_start: Settings,
}

impl SettingsFile {
    /// Creates the settings file at `path`, with the settings of the `first_start` on this
    /// computer.
    #[must_use]
    pub fn new(path: PathBuf, first_start: Settings) -> Self {
        Self { path, first_start }
    }

    /// Reads the saved settings. Gives the settings of the first start when there is no file, or
    /// when the file is damaged.
    #[must_use]
    pub fn load(&self) -> Settings {
        let json = match fs::read_to_string(&self.path) {
            Ok(json) => json,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return self.first_start.clone(),
            Err(error) => {
                tracing::warn!(%error, "failed to read the settings file, using the default settings");
                return self.first_start.clone();
            }
        };
        parse(&json).unwrap_or_else(|error| {
            tracing::warn!(%error, "the settings file is damaged, using the default settings");
            self.first_start.clone()
        })
    }

    /// Writes a new file next to the old one, then replaces the old one with it, so that a failed
    /// write never leaves a damaged file.
    fn write(&self, settings: &Settings) -> io::Result<()> {
        if let Some(folder) = self.path.parent() {
            fs::create_dir_all(folder)?;
        }
        let staged = self.path.with_extension("json.new");
        fs::write(&staged, to_json(settings))?;
        fs::rename(&staged, &self.path)
    }
}

impl SettingsStore for SettingsFile {
    fn save(&mut self, settings: &Settings) {
        if let Err(error) = self.write(settings) {
            tracing::warn!(%error, "failed to save the settings");
        }
    }

    fn first_start_settings(&self) -> Settings {
        self.first_start.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typealike_core::hub::ModelFile;
    use typealike_core::inference::Backend;
    use typealike_core::localization::UiLanguage;
    use typealike_core::settings::{
        ClipboardAccess, EnergySaverRule, EnergySavingSettings, FullScreenRule, GameRule, HistoryAccess,
        HistoryApp, HistoryScope, IdleTimeout, KeptTexts, OcrLevel, PersonalizationSettings,
        PromptPreparation, ReplyWords, ScreenTextAccess, ScreenTextApp, ScreenTextSettings, Setup,
        SuggestionDisplay, SuggestionLength, SuggestionPlaces, SuggestionSite, TypoFix, TypoRule, WordChoice,
        WriterName, WriterNotes,
    };

    fn model_file() -> ModelFile {
        ModelFile {
            repo: "owner/repo".to_owned(),
            name: "model.gguf".to_owned(),
        }
    }

    fn temporary_file(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("typealike-settings-{}", std::process::id()))
            .join(name)
    }

    #[test]
    fn saved_settings_load_again() {
        let path = temporary_file("saved.json");
        let mut file = SettingsFile::new(path.clone(), first_start_settings());

        let settings = Settings {
            backend: Backend::Cuda,
            suggestion_model: Some(model_file()),
            suggestion_places: SuggestionPlaces {
                apps: Vec::new(),
                sites: vec![SuggestionSite {
                    host: "bank.example".to_owned(),
                    is_on: false,
                }],
            },
            writer_name: WriterName("Sara Novak".to_owned()),
            writer_notes: WriterNotes("I am a nurse.".to_owned()),
            clipboard_access: ClipboardAccess::Denied,
            screen_text: ScreenTextSettings {
                access: ScreenTextAccess::Denied,
                apps: vec![ScreenTextApp {
                    app_id: "slack.exe".to_owned(),
                    app_name: "Slack".to_owned(),
                    app_path: Some("C:\\Slack\\slack.exe".to_owned()),
                    is_on: false,
                }],
                ocr_level: OcrLevel::Accurate,
            },
            personalization: PersonalizationSettings {
                history: HistoryAccess::Off,
                kept_texts: KeptTexts::AcceptedOnly,
                apps: vec![HistoryApp {
                    app_id: "slack.exe".to_owned(),
                    app_name: "Slack".to_owned(),
                    app_path: Some("C:\\Slack\\slack.exe".to_owned()),
                    scope: HistoryScope::ThisApp,
                }],
                word_choice: WordChoice::Strong,
                reply_words: ReplyWords::Off,
            },
            energy_saving: EnergySavingSettings {
                idle_timeout: IdleTimeout::FifteenMinutes,
                energy_saver: EnergySaverRule::KeepSuggestions,
                game: GameRule::KeepSuggestions,
                full_screen: FullScreenRule::KeepSuggestions,
            },
            prompt_preparation: PromptPreparation::OnFirstKey,
            suggestion_display: SuggestionDisplay::WhenComplete,
            suggestion_length: SuggestionLength::Long,
            typo_rule: TypoRule::Suggest,
            typo_fix: TypoFix::Off,
            ui_language: Some(UiLanguage::Spanish),
            setup: Setup::Finished,
        };

        file.save(&settings);
        let loaded = file.load();

        fs::remove_file(&path).ok();
        assert_eq!(loaded, settings);
    }

    fn first_start_settings() -> Settings {
        Settings {
            backend: Backend::Vulkan,
            writer_name: WriterName("Sara Novak".to_owned()),
            setup: Setup::Needed,
            ..Settings::default()
        }
    }

    #[test]
    fn missing_file_gives_the_first_start_settings() {
        let file = SettingsFile::new(temporary_file("missing.json"), first_start_settings());

        assert_eq!(file.load(), first_start_settings());
    }

    #[test]
    fn store_gives_the_first_start_settings_for_a_reset() {
        let file = SettingsFile::new(temporary_file("reset.json"), first_start_settings());

        assert_eq!(file.first_start_settings(), first_start_settings());
    }

    #[test]
    fn damaged_file_gives_the_first_start_settings() {
        let path = temporary_file("damaged.json");
        fs::create_dir_all(path.parent().expect("the path has a folder")).expect("temp dir is writable");
        fs::write(&path, "not json").expect("temp dir is writable");

        let loaded = SettingsFile::new(path.clone(), first_start_settings()).load();

        fs::remove_file(&path).ok();
        assert_eq!(loaded, first_start_settings());
    }
}
