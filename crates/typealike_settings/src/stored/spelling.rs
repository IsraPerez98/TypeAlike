//! Converts the languages of the user and the spell check to and from the format of the settings
//! file.

use serde::{Deserialize, Serialize};
use typealike_core::settings::{SpellCheck, SpellingSettings};

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct StoredSpelling {
    /// `None` follows the languages of the OS.
    languages: Option<Vec<String>>,
    check: StoredSpellCheck,
}

impl Default for StoredSpelling {
    fn default() -> Self {
        SpellingSettings::default().into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredSpellCheck {
    On,
    Off,
}

impl From<SpellingSettings> for StoredSpelling {
    fn from(settings: SpellingSettings) -> Self {
        let check = match settings.check {
            SpellCheck::On => StoredSpellCheck::On,
            SpellCheck::Off => StoredSpellCheck::Off,
        };
        Self {
            languages: settings.languages,
            check,
        }
    }
}

impl From<StoredSpelling> for SpellingSettings {
    fn from(stored: StoredSpelling) -> Self {
        let check = match stored.check {
            StoredSpellCheck::On => SpellCheck::On,
            StoredSpellCheck::Off => SpellCheck::Off,
        };
        Self {
            languages: stored.languages,
            check,
        }
    }
}
