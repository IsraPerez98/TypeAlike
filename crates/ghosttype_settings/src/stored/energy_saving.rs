//! Converts the energy saving settings to and from the format of the settings file.

use ghosttype_core::settings::{EnergySaverRule, EnergySavingSettings, IdleTimeout};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct StoredEnergySaving {
    idle_timeout: StoredIdleTimeout,
    energy_saver: StoredEnergySaverRule,
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
        Self {
            idle_timeout,
            energy_saver,
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
        Self {
            idle_timeout,
            energy_saver,
        }
    }
}
