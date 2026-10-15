//! Keeps the settings in a JSON file in the app's data folder, so they last between runs.

use std::fs;
use std::io;
use std::path::PathBuf;

use ghosttype_core::hub::ModelFile;
use ghosttype_core::inference::Backend;
use ghosttype_core::settings::{Settings, SettingsStore};
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
        Self {
            backend,
            suggestion_model,
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
        Self {
            backend,
            suggestion_model,
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
