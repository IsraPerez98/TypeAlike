//! Converts the languages of the user to and from the format of the settings file.

use serde::{Deserialize, Serialize};
use typealike_core::settings::SpellingSettings;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct StoredSpelling {
    /// `None` follows the languages of the OS.
    languages: Option<Vec<String>>,
}

impl From<SpellingSettings> for StoredSpelling {
    fn from(settings: SpellingSettings) -> Self {
        Self {
            languages: settings.languages,
        }
    }
}

impl From<StoredSpelling> for SpellingSettings {
    fn from(stored: StoredSpelling) -> Self {
        Self {
            languages: stored.languages,
        }
    }
}
