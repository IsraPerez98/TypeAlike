//! Keeps the settings in a JSON file in the app's data folder, so they last between runs.

use std::fs;
use std::io;
use std::path::PathBuf;

use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::Backend;
use ghosttype_core::settings::{
    ClipboardAccess, HistoryAccess, HistoryApp, KeptTexts, OcrLevel, PersonalizationSettings,
    PromptPreparation, ScreenTextAccess, ScreenTextApp, ScreenTextSettings, Settings, SettingsStore,
    SuggestionDisplay, SuggestionLength, WordChoice, WriterName, WriterNotes,
};
use serde::{Deserialize, Serialize};

/// The settings file: it gives the saved settings at start, and keeps each change.
#[derive(Debug)]
pub(crate) struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Reads the saved settings. Gives the default settings when there is no file, or when the
    /// file is damaged.
    pub(crate) fn load(&self) -> Settings {
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

/// The settings as the file stores them. Missing fields take their default values, so a file
/// from an older version still loads.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct StoredSettings {
    backend: StoredBackend,
    suggestion_model: Option<StoredModelFile>,
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
    prompt_preparation: StoredPromptPreparation,
    suggestion_display: StoredSuggestionDisplay,
    suggestion_length: StoredSuggestionLength,
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
#[serde(default)]
struct StoredScreenText {
    access: StoredScreenTextAccess,
    apps: Vec<StoredScreenTextApp>,
    ocr_level: StoredOcrLevel,
}

impl Default for StoredScreenText {
    fn default() -> Self {
        ScreenTextSettings::default().into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoredScreenTextAccess {
    Allowed,
    Denied,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StoredOcrLevel {
    Fast,
    Accurate,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredScreenTextApp {
    app_id: String,
    app_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    app_path: Option<String>,
    is_on: bool,
}

impl From<ScreenTextSettings> for StoredScreenText {
    fn from(settings: ScreenTextSettings) -> Self {
        let access = match settings.access {
            ScreenTextAccess::Allowed => StoredScreenTextAccess::Allowed,
            ScreenTextAccess::Denied => StoredScreenTextAccess::Denied,
        };
        let apps = settings
            .apps
            .into_iter()
            .map(|app| StoredScreenTextApp {
                app_id: app.app_id,
                app_name: app.app_name,
                app_path: app.app_path,
                is_on: app.is_on,
            })
            .collect();
        let ocr_level = match settings.ocr_level {
            OcrLevel::Fast => StoredOcrLevel::Fast,
            OcrLevel::Accurate => StoredOcrLevel::Accurate,
        };
        Self {
            access,
            apps,
            ocr_level,
        }
    }
}

impl From<StoredScreenText> for ScreenTextSettings {
    fn from(stored: StoredScreenText) -> Self {
        let access = match stored.access {
            StoredScreenTextAccess::Allowed => ScreenTextAccess::Allowed,
            StoredScreenTextAccess::Denied => ScreenTextAccess::Denied,
        };
        let apps = stored
            .apps
            .into_iter()
            .map(|app| ScreenTextApp {
                app_id: app.app_id,
                app_name: app.app_name,
                app_path: app.app_path,
                is_on: app.is_on,
            })
            .collect();
        let ocr_level = match stored.ocr_level {
            StoredOcrLevel::Fast => OcrLevel::Fast,
            StoredOcrLevel::Accurate => OcrLevel::Accurate,
        };
        Self {
            access,
            apps,
            ocr_level,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct StoredPersonalization {
    history: StoredHistoryAccess,
    kept_texts: StoredKeptTexts,
    apps: Vec<StoredScreenTextApp>,
    word_choice: StoredWordChoice,
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
            .map(|app| StoredScreenTextApp {
                app_id: app.app_id,
                app_name: app.app_name,
                app_path: app.app_path,
                is_on: app.is_on,
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
                app_id: app.app_id,
                app_name: app.app_name,
                app_path: app.app_path,
                is_on: app.is_on,
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
        Self {
            backend,
            suggestion_model,
            writer_name: settings.writer_name.0,
            writer_notes: Some(settings.writer_notes.0),
            system_prompt: None,
            clipboard_access,
            screen_text: settings.screen_text.into(),
            personalization: settings.personalization.into(),
            prompt_preparation,
            suggestion_display,
            suggestion_length,
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
        let writer_notes = match (stored.writer_notes, stored.system_prompt) {
            (Some(notes), _) if !notes.is_empty() => WriterNotes(notes),
            (_, Some(system_prompt)) => WriterNotes(system_prompt),
            (Some(notes), None) => WriterNotes(notes),
            (None, None) => WriterNotes::default(),
        };
        Self {
            backend,
            suggestion_model,
            writer_name: WriterName(stored.writer_name),
            writer_notes,
            clipboard_access,
            screen_text: stored.screen_text.into(),
            personalization: stored.personalization.into(),
            prompt_preparation,
            suggestion_display,
            suggestion_length,
        }
    }
}

fn parse(json: &str) -> Result<Settings, serde_json::Error> {
    serde_json::from_str::<StoredSettings>(json).map(Settings::from)
}

fn to_json(settings: &Settings) -> String {
    serde_json::to_string_pretty(&StoredSettings::from(settings.clone()))
        .expect("settings of plain enums and strings always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn backend_reads_from_its_name() {
        let files = [
            r#"{ "backend": "cpu" }"#,
            r#"{ "backend": "vulkan" }"#,
            r#"{ "backend": "cuda" }"#,
            r#"{ "backend": "metal" }"#,
        ];

        let backends = files.map(|json| parse(json).map(|settings| settings.backend).ok());

        assert_eq!(
            backends,
            [
                Some(Backend::Cpu),
                Some(Backend::Vulkan),
                Some(Backend::Cuda),
                Some(Backend::Metal)
            ]
        );
    }

    #[test]
    fn suggestion_model_reads_from_its_repo_and_name() {
        let json = r#"{ "suggestion_model": { "repo": "owner/repo", "name": "model.gguf" } }"#;

        let suggestion_model = parse(json).ok().and_then(|settings| settings.suggestion_model);

        assert_eq!(suggestion_model, Some(model_file()));
    }

    #[test]
    fn writer_name_reads_from_its_text() {
        let json = r#"{ "writer_name": "Sara Novak" }"#;

        let name = parse(json).map(|settings| settings.writer_name).ok();

        assert_eq!(name, Some(WriterName("Sara Novak".to_owned())));
    }

    #[test]
    fn writer_notes_read_from_their_text() {
        let json = r#"{ "writer_notes": "I am a nurse." }"#;

        let notes = parse(json).map(|settings| settings.writer_notes).ok();

        assert_eq!(notes, Some(WriterNotes("I am a nurse.".to_owned())));
    }

    #[test]
    fn missing_writer_notes_are_the_default_notes() {
        let notes = parse("{}").map(|settings| settings.writer_notes).ok();

        assert_eq!(notes, Some(WriterNotes::default()));
    }

    #[test]
    fn empty_writer_notes_stay_empty() {
        let json = r#"{ "writer_notes": "" }"#;

        let notes = parse(json).map(|settings| settings.writer_notes).ok();

        assert_eq!(notes, Some(WriterNotes(String::new())));
    }

    #[test]
    fn system_prompt_of_an_older_version_becomes_the_writer_notes() {
        let files = [
            r#"{ "system_prompt": "Formal Spanish." }"#,
            r#"{ "system_prompt": "Formal Spanish.", "writer_notes": "" }"#,
        ];

        let notes = files.map(|json| parse(json).ok().map(|settings| settings.writer_notes.0));

        assert_eq!(
            notes.each_ref().map(Option::as_deref),
            [Some("Formal Spanish."); 2]
        );
    }

    #[test]
    fn writer_notes_win_over_an_older_system_prompt() {
        let json = r#"{ "system_prompt": "Formal Spanish.", "writer_notes": "I am a nurse." }"#;

        let notes = parse(json).map(|settings| settings.writer_notes).ok();

        assert_eq!(notes, Some(WriterNotes("I am a nurse.".to_owned())));
    }

    #[test]
    fn saved_file_has_no_system_prompt() {
        let json = to_json(&Settings {
            writer_notes: WriterNotes("Formal Spanish.".to_owned()),
            ..Settings::default()
        });

        assert!(!json.contains("system_prompt"), "{json}");
    }

    #[test]
    fn clipboard_access_reads_from_its_name() {
        let files = [
            r#"{ "clipboard_access": "denied" }"#,
            r#"{ "clipboard_access": "allowed" }"#,
        ];

        let accesses = files.map(|json| parse(json).map(|settings| settings.clipboard_access).ok());

        assert_eq!(
            accesses,
            [Some(ClipboardAccess::Denied), Some(ClipboardAccess::Allowed)]
        );
    }

    #[test]
    fn screen_text_reads_its_switch_and_its_apps() {
        let json = r#"{ "screen_text": { "access": "denied", "apps": [
            { "app_id": "slack.exe", "app_name": "Slack", "app_path": "C:\\Slack\\slack.exe", "is_on": false },
            { "app_id": "com.apple.mail", "app_name": "Mail", "is_on": true }
        ] } }"#;

        let screen_text = parse(json).map(|settings| settings.screen_text).ok();

        assert_eq!(
            screen_text,
            Some(ScreenTextSettings {
                access: ScreenTextAccess::Denied,
                apps: vec![
                    ScreenTextApp {
                        app_id: "slack.exe".to_owned(),
                        app_name: "Slack".to_owned(),
                        app_path: Some("C:\\Slack\\slack.exe".to_owned()),
                        is_on: false,
                    },
                    ScreenTextApp {
                        app_id: "com.apple.mail".to_owned(),
                        app_name: "Mail".to_owned(),
                        app_path: None,
                        is_on: true,
                    },
                ],
                ..ScreenTextSettings::default()
            })
        );
    }

    #[test]
    fn settings_file_without_an_ocr_level_reads_as_fast() {
        let json = r#"{ "screen_text": { "access": "allowed", "apps": [] } }"#;

        let level = parse(json).map(|settings| settings.screen_text.ocr_level).ok();

        assert_eq!(level, Some(OcrLevel::Fast));
    }

    #[test]
    fn ocr_level_survives_a_save_and_a_load() {
        let settings = Settings {
            screen_text: ScreenTextSettings {
                ocr_level: OcrLevel::Accurate,
                ..ScreenTextSettings::default()
            },
            ..Settings::default()
        };

        let loaded = parse(&to_json(&settings))
            .map(|loaded| loaded.screen_text.ocr_level)
            .ok();

        assert_eq!(loaded, Some(OcrLevel::Accurate));
    }

    #[test]
    fn settings_file_without_personalization_reads_with_the_history_off() {
        let json = r#"{ "clipboard_access": "allowed" }"#;

        let personalization = parse(json).map(|settings| settings.personalization).ok();

        assert_eq!(personalization, Some(PersonalizationSettings::default()));
    }

    #[test]
    fn personalization_reads_from_its_names() {
        let json = r#"{ "personalization": { "history": "on", "kept_texts": "accepted_only", "word_choice": "light" } }"#;

        let personalization = parse(json).map(|settings| settings.personalization).ok();

        assert_eq!(
            personalization,
            Some(PersonalizationSettings {
                history: HistoryAccess::On,
                kept_texts: KeptTexts::AcceptedOnly,
                apps: Vec::new(),
                word_choice: WordChoice::Light,
            })
        );
    }

    #[test]
    fn prompt_preparation_reads_from_its_name() {
        let files = [
            r#"{ "prompt_preparation": "on_first_key" }"#,
            r#"{ "prompt_preparation": "on_focus" }"#,
        ];

        let preparations = files.map(|json| parse(json).map(|settings| settings.prompt_preparation).ok());

        assert_eq!(
            preparations,
            [
                Some(PromptPreparation::OnFirstKey),
                Some(PromptPreparation::OnFocus)
            ]
        );
    }

    #[test]
    fn suggestion_display_reads_from_its_name() {
        let files = [
            r#"{ "suggestion_display": "when_complete" }"#,
            r#"{ "suggestion_display": "word_by_word" }"#,
        ];

        let displays = files.map(|json| parse(json).map(|settings| settings.suggestion_display).ok());

        assert_eq!(
            displays,
            [
                Some(SuggestionDisplay::WhenComplete),
                Some(SuggestionDisplay::WordByWord)
            ]
        );
    }

    #[test]
    fn suggestion_length_reads_from_its_name() {
        let files = [
            r#"{ "suggestion_length": "very_short" }"#,
            r#"{ "suggestion_length": "short" }"#,
            r#"{ "suggestion_length": "medium" }"#,
            r#"{ "suggestion_length": "long" }"#,
            r#"{ "suggestion_length": "very_long" }"#,
        ];

        let lengths = files.map(|json| parse(json).map(|settings| settings.suggestion_length).ok());

        assert_eq!(
            lengths,
            [
                SuggestionLength::VeryShort,
                SuggestionLength::Short,
                SuggestionLength::Medium,
                SuggestionLength::Long,
                SuggestionLength::VeryLong,
            ]
            .map(Some)
        );
    }

    #[test]
    fn missing_field_takes_its_default() {
        let settings = parse("{}");

        assert_eq!(settings.ok(), Some(Settings::default()));
    }

    #[test]
    fn unknown_backend_is_an_error() {
        let settings = parse(r#"{ "backend": "quantum" }"#);

        assert!(settings.is_err());
    }

    #[test]
    fn saved_settings_load_again() {
        let path = temporary_file("saved.json");
        let mut file = SettingsFile::new(path.clone());

        let settings = Settings {
            backend: Backend::Cuda,
            suggestion_model: Some(model_file()),
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
                    is_on: false,
                }],
                word_choice: WordChoice::Strong,
            },
            prompt_preparation: PromptPreparation::OnFocus,
            suggestion_display: SuggestionDisplay::WordByWord,
            suggestion_length: SuggestionLength::Long,
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
