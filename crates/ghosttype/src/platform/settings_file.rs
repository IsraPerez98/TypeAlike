//! Keeps the settings in a JSON file in the app's data folder, so they last between runs.

use std::fs;
use std::io;
use std::path::PathBuf;

use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::{Backend, SystemPrompt};
use ghosttype_core::settings::{
    ClipboardAccess, PromptPreparation, ScreenTextAccess, ScreenTextApp, ScreenTextSettings, Settings,
    SettingsStore, SuggestionDisplay, SuggestionLength,
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
    system_prompt: Option<String>,
    clipboard_access: StoredClipboardAccess,
    screen_text: StoredScreenText,
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
        Self { access, apps }
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
        Self { access, apps }
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
    Short,
    Medium,
    Long,
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
            SuggestionLength::Short => StoredSuggestionLength::Short,
            SuggestionLength::Medium => StoredSuggestionLength::Medium,
            SuggestionLength::Long => StoredSuggestionLength::Long,
        };
        Self {
            backend,
            suggestion_model,
            system_prompt: settings.custom_system_prompt.map(|system_prompt| system_prompt.0),
            clipboard_access,
            screen_text: settings.screen_text.into(),
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
            StoredSuggestionLength::Short => SuggestionLength::Short,
            StoredSuggestionLength::Medium => SuggestionLength::Medium,
            StoredSuggestionLength::Long => SuggestionLength::Long,
        };
        Self {
            backend,
            suggestion_model,
            custom_system_prompt: stored.system_prompt.map(SystemPrompt),
            clipboard_access,
            screen_text: stored.screen_text.into(),
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
        ];

        let backends = files.map(|json| parse(json).map(|settings| settings.backend).ok());

        assert_eq!(
            backends,
            [Some(Backend::Cpu), Some(Backend::Vulkan), Some(Backend::Cuda)]
        );
    }

    #[test]
    fn suggestion_model_reads_from_its_repo_and_name() {
        let json = r#"{ "suggestion_model": { "repo": "owner/repo", "name": "model.gguf" } }"#;

        let suggestion_model = parse(json).ok().and_then(|settings| settings.suggestion_model);

        assert_eq!(suggestion_model, Some(model_file()));
    }

    #[test]
    fn system_prompt_reads_as_the_custom_system_prompt() {
        let json = r#"{ "system_prompt": "Formal Spanish." }"#;

        let system_prompt = parse(json)
            .ok()
            .and_then(|settings| settings.custom_system_prompt);

        assert_eq!(system_prompt, Some(SystemPrompt("Formal Spanish.".to_owned())));
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
            r#"{ "suggestion_length": "short" }"#,
            r#"{ "suggestion_length": "medium" }"#,
            r#"{ "suggestion_length": "long" }"#,
        ];

        let lengths = files.map(|json| parse(json).map(|settings| settings.suggestion_length).ok());

        assert_eq!(
            lengths,
            [
                Some(SuggestionLength::Short),
                Some(SuggestionLength::Medium),
                Some(SuggestionLength::Long)
            ]
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
            custom_system_prompt: Some(SystemPrompt("Formal Spanish.".to_owned())),
            clipboard_access: ClipboardAccess::Allowed,
            screen_text: ScreenTextSettings {
                access: ScreenTextAccess::Denied,
                apps: vec![ScreenTextApp {
                    app_id: "slack.exe".to_owned(),
                    app_name: "Slack".to_owned(),
                    app_path: Some("C:\\Slack\\slack.exe".to_owned()),
                    is_on: false,
                }],
            },
            prompt_preparation: PromptPreparation::OnFocus,
            suggestion_display: SuggestionDisplay::WordByWord,
            suggestion_length: SuggestionLength::Medium,
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
