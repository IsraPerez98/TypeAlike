//! The settings of the Energy saving pane: when TypeAlike stops suggestions and unloads the
//! suggestion model.

use std::time::Duration;

/// The time without keys and clicks after which TypeAlike stops suggestions and unloads the
/// suggestion model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IdleTimeout {
    /// TypeAlike does not stop when the user does not type or click.
    Off,
    /// After 15 minutes.
    FifteenMinutes,
    /// After 30 minutes.
    #[default]
    ThirtyMinutes,
    /// After 60 minutes.
    SixtyMinutes,
}

impl IdleTimeout {
    /// The time without keys and clicks before the stop, or `None` when TypeAlike does not stop.
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use typealike_core::settings::IdleTimeout;
    ///
    /// assert_eq!(IdleTimeout::ThirtyMinutes.duration(), Some(Duration::from_mins(30)));
    /// assert_eq!(IdleTimeout::Off.duration(), None);
    /// ```
    #[must_use]
    pub fn duration(self) -> Option<Duration> {
        let minutes = match self {
            Self::Off => return None,
            Self::FifteenMinutes => 15,
            Self::ThirtyMinutes => 30,
            Self::SixtyMinutes => 60,
        };
        Some(Duration::from_mins(minutes))
    }
}

/// What TypeAlike does while the energy saver of the computer is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnergySaverRule {
    /// TypeAlike stops suggestions and unloads the suggestion model until the energy saver goes
    /// off.
    #[default]
    StopSuggestions,
    /// Suggestions continue as when the energy saver is off.
    KeepSuggestions,
}

/// What TypeAlike does while a game is in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameRule {
    /// TypeAlike stops suggestions and unloads the suggestion model. After the game, the next
    /// request for a suggestion loads the model again.
    #[default]
    StopSuggestions,
    /// Suggestions continue as in any other app.
    KeepSuggestions,
}

/// When TypeAlike stops suggestions and unloads the suggestion model to save energy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EnergySavingSettings {
    /// The time without keys and clicks after which TypeAlike stops.
    pub idle_timeout: IdleTimeout,
    /// What TypeAlike does while the energy saver of the computer is on.
    pub energy_saver: EnergySaverRule,
    /// What TypeAlike does while a game is in front.
    pub game: GameRule,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_idle_timeout_waits_its_own_minutes() {
        let timeouts = [
            IdleTimeout::Off,
            IdleTimeout::FifteenMinutes,
            IdleTimeout::ThirtyMinutes,
            IdleTimeout::SixtyMinutes,
        ];

        let durations = timeouts.map(IdleTimeout::duration);

        assert_eq!(
            durations,
            [
                None,
                Some(Duration::from_mins(15)),
                Some(Duration::from_mins(30)),
                Some(Duration::from_mins(60)),
            ]
        );
    }

    #[test]
    fn default_stops_after_30_minutes_in_energy_saver_and_during_games() {
        let settings = EnergySavingSettings::default();

        assert_eq!(
            settings,
            EnergySavingSettings {
                idle_timeout: IdleTimeout::ThirtyMinutes,
                energy_saver: EnergySaverRule::StopSuggestions,
                game: GameRule::StopSuggestions,
            }
        );
    }
}
