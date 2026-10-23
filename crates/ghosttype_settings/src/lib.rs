//! Keeps the settings in a JSON file in the app's data folder, so they last between runs.

mod stored;

use std::fs;
use std::io;
use std::path::PathBuf;

use ghosttype_core::settings::{Settings, SettingsStore};

use stored::{parse, to_json};

/// The settings file: it gives the saved settings at start, and keeps each change.
#[derive(Debug)]
pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    /// Creates the settings file at `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Reads the saved settings. Gives the default settings when there is no file, or when the
    /// file is damaged.
    #[must_use]
    pub fn load(&self) -> Settings {
        let json = match fs::read_to_string(&self.path) {
            Ok(json) => json,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Settings::default(),
            Err(error) => {
                tracing::warn!(%error, "failed to read the settings file, using the default settings");
                return Settings::default();
            }
        };
        parse(&json).unwrap_or_else(|error| {
            tracing::warn!(%error, "the settings file is damaged, using the default settings");
            Settings::default()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghosttype_core::hub::ModelFile;
    use ghosttype_core::inference::{Backend, SentenceEnd};
    use ghosttype_core::settings::{
        ClipboardAccess, HistoryAccess, HistoryApp, HistoryScope, KeptTexts, OcrLevel,
        PersonalizationSettings, PromptPreparation, ScreenTextAccess, ScreenTextApp, ScreenTextSettings,
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
            .join(format!("ghosttype-settings-{}", std::process::id()))
            .join(name)
    }

    #[test]
    fn saved_settings_load_again() {
        let path = temporary_file("saved.json");
        let mut file = SettingsFile::new(path.clone());

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
            clipboard_access: ClipboardAccess::Allowed,
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
                history: HistoryAccess::On,
                kept_texts: KeptTexts::All,
                apps: vec![HistoryApp {
                    app_id: "slack.exe".to_owned(),
                    app_name: "Slack".to_owned(),
                    app_path: Some("C:\\Slack\\slack.exe".to_owned()),
                    scope: HistoryScope::ThisApp,
                }],
                word_choice: WordChoice::Strong,
            },
            prompt_preparation: PromptPreparation::OnFocus,
            suggestion_display: SuggestionDisplay::WordByWord,
            suggestion_length: SuggestionLength::Long,
            typo_rule: TypoRule::Suggest,
            typo_fix: TypoFix::Off,
            sentence_end: SentenceEnd::Stop,
        };

        file.save(&settings);
        let loaded = file.load();

        fs::remove_file(&path).ok();
        assert_eq!(loaded, settings);
    }

    #[test]
    fn missing_file_gives_the_default_settings() {
        let file = SettingsFile::new(temporary_file("missing.json"));

        assert_eq!(file.load(), Settings::default());
    }

    #[test]
    fn damaged_file_gives_the_default_settings() {
        let path = temporary_file("damaged.json");
        fs::create_dir_all(path.parent().expect("the path has a folder")).expect("temp dir is writable");
        fs::write(&path, "not json").expect("temp dir is writable");

        let loaded = SettingsFile::new(path.clone()).load();

        fs::remove_file(&path).ok();
        assert_eq!(loaded, Settings::default());
    }
}
