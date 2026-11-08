//! Converts the apps and the websites where suggestions show, and the rule for private windows,
//! to and from the format of the settings file.

use serde::{Deserialize, Serialize};
use typealike_core::settings::{PrivateWindowRule, SuggestionApp, SuggestionPlaces, SuggestionSite};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct StoredSuggestionPlaces {
    apps: Vec<StoredSuggestionApp>,
    sites: Vec<StoredSuggestionSite>,
    private_windows: StoredPrivateWindowRule,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredPrivateWindowRule {
    #[default]
    StopSuggestions,
    KeepSuggestions,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredSuggestionApp {
    app_id: String,
    app_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    app_path: Option<String>,
    is_on: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredSuggestionSite {
    host: String,
    is_on: bool,
}

impl From<SuggestionPlaces> for StoredSuggestionPlaces {
    fn from(places: SuggestionPlaces) -> Self {
        let apps = places
            .apps
            .into_iter()
            .map(|app| StoredSuggestionApp {
                app_id: app.app_id,
                app_name: app.app_name,
                app_path: app.app_path,
                is_on: app.is_on,
            })
            .collect();
        let sites = places
            .sites
            .into_iter()
            .map(|site| StoredSuggestionSite {
                host: site.host,
                is_on: site.is_on,
            })
            .collect();
        let private_windows = match places.private_windows {
            PrivateWindowRule::StopSuggestions => StoredPrivateWindowRule::StopSuggestions,
            PrivateWindowRule::KeepSuggestions => StoredPrivateWindowRule::KeepSuggestions,
        };
        Self {
            apps,
            sites,
            private_windows,
        }
    }
}

impl From<StoredSuggestionPlaces> for SuggestionPlaces {
    fn from(stored: StoredSuggestionPlaces) -> Self {
        let apps = stored
            .apps
            .into_iter()
            .map(|app| SuggestionApp {
                app_id: app.app_id,
                app_name: app.app_name,
                app_path: app.app_path,
                is_on: app.is_on,
            })
            .collect();
        let sites = stored
            .sites
            .into_iter()
            .map(|site| SuggestionSite {
                host: site.host,
                is_on: site.is_on,
            })
            .collect();
        let private_windows = match stored.private_windows {
            StoredPrivateWindowRule::StopSuggestions => PrivateWindowRule::StopSuggestions,
            StoredPrivateWindowRule::KeepSuggestions => PrivateWindowRule::KeepSuggestions,
        };
        Self {
            apps,
            sites,
            private_windows,
        }
    }
}
