//! Converts the screen text settings to and from the format of the settings file.

use ghosttype_core::settings::{OcrLevel, ScreenTextAccess, ScreenTextApp, ScreenTextSettings};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct StoredScreenText {
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
