//! Converts the energy saving settings to and from the format of the settings file.

use serde::{Deserialize, Serialize};
use typealike_core::settings::{
    EnergySaverRule, EnergySavingSettings, FullScreenRule, GameRule, IdleTimeout,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct StoredEnergySaving {
    idle_timeout: StoredIdleTimeout,
    energy_saver: StoredEnergySaverRule,
    game: StoredGameRule,
    full_screen: StoredFullScreenRule,
}

impl Default for StoredEnergySaving {
    fn default() -> Self {
        EnergySavingSettings::default().into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredIdleTimeout {
    Off,
    FifteenMinutes,
    ThirtyMinutes,
    SixtyMinutes,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredEnergySaverRule {
    StopSuggestions,
    KeepSuggestions,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredGameRule {
    StopSuggestions,
    KeepSuggestions,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredFullScreenRule {
    StopSuggestions,
    KeepSuggestions,
}

impl From<EnergySavingSettings> for StoredEnergySaving {
    fn from(settings: EnergySavingSettings) -> Self {
        let idle_timeout = match settings.idle_timeout {
            IdleTimeout::Off => StoredIdleTimeout::Off,
            IdleTimeout::FifteenMinutes => StoredIdleTimeout::FifteenMinutes,
            IdleTimeout::ThirtyMinutes => StoredIdleTimeout::ThirtyMinutes,
            IdleTimeout::SixtyMinutes => StoredIdleTimeout::SixtyMinutes,
        };
        let energy_saver = match settings.energy_saver {
            EnergySaverRule::StopSuggestions => StoredEnergySaverRule::StopSuggestions,
            EnergySaverRule::KeepSuggestions => StoredEnergySaverRule::KeepSuggestions,
        };
        let game = match settings.game {
            GameRule::StopSuggestions => StoredGameRule::StopSuggestions,
            GameRule::KeepSuggestions => StoredGameRule::KeepSuggestions,
        };
        let full_screen = match settings.full_screen {
            FullScreenRule::StopSuggestions => StoredFullScreenRule::StopSuggestions,
            FullScreenRule::KeepSuggestions => StoredFullScreenRule::KeepSuggestions,
        };
        Self {
            idle_timeout,
            energy_saver,
            game,
            full_screen,
        }
    }
}

impl From<StoredEnergySaving> for EnergySavingSettings {
    fn from(stored: StoredEnergySaving) -> Self {
        let idle_timeout = match stored.idle_timeout {
            StoredIdleTimeout::Off => IdleTimeout::Off,
            StoredIdleTimeout::FifteenMinutes => IdleTimeout::FifteenMinutes,
            StoredIdleTimeout::ThirtyMinutes => IdleTimeout::ThirtyMinutes,
            StoredIdleTimeout::SixtyMinutes => IdleTimeout::SixtyMinutes,
        };
        let energy_saver = match stored.energy_saver {
            StoredEnergySaverRule::StopSuggestions => EnergySaverRule::StopSuggestions,
            StoredEnergySaverRule::KeepSuggestions => EnergySaverRule::KeepSuggestions,
        };
        let game = match stored.game {
            StoredGameRule::StopSuggestions => GameRule::StopSuggestions,
            StoredGameRule::KeepSuggestions => GameRule::KeepSuggestions,
        };
        let full_screen = match stored.full_screen {
            StoredFullScreenRule::StopSuggestions => FullScreenRule::StopSuggestions,
            StoredFullScreenRule::KeepSuggestions => FullScreenRule::KeepSuggestions,
        };
        Self {
            idle_timeout,
            energy_saver,
            game,
            full_screen,
        }
    }
}
