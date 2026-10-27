//! The Energy saving pane of the settings window: when TypeAlike stops suggestions and unloads the
//! suggestion model.

use crate::desktop::EnergySaver;
use crate::localization::Translations;
use crate::settings::{EnergySaverRule, EnergySavingSettings, GameRule, IdleTimeout};
use crate::settings_window::controls::{MenuText, picker, replace_if_changed};
use crate::settings_window::view::{
    ControlId, Note, NoteTone, Page, PageIcon, Row, RowIcon, Section, SettingsInput, Switch,
};

/// Something the user did in the Energy saving pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnergySavingAction {
    /// Picked this time without keys and clicks before TypeAlike stops.
    SelectIdleTimeout(IdleTimeout),
    /// Turned on or off the switch that stops suggestions while the energy saver is on.
    SwitchEnergySaver(EnergySaverRule),
    /// Turned on or off the switch that stops suggestions while a game is in front.
    SwitchGame(GameRule),
}

pub(crate) const IDLE_TIMEOUT: &str = "idle-timeout";
pub(crate) const ENERGY_SAVER: &str = "energy-saver";
pub(crate) const GAME: &str = "game";
const IDLE_TIMEOUT_CHOICES: [(IdleTimeout, &str); 4] = [
    (IdleTimeout::Off, "energy-saving-idle-timeout-off"),
    (IdleTimeout::FifteenMinutes, "energy-saving-idle-timeout-15"),
    (IdleTimeout::ThirtyMinutes, "energy-saving-idle-timeout-30"),
    (IdleTimeout::SixtyMinutes, "energy-saving-idle-timeout-60"),
];

/// State of the Energy saving pane: when TypeAlike stops suggestions to save energy.
///
/// ```
/// use typealike_core::settings::{EnergySavingSettings, IdleTimeout};
/// use typealike_core::settings_window::energy_saving::{EnergySavingAction, EnergySavingPane};
///
/// let mut pane = EnergySavingPane::new(EnergySavingSettings::default());
///
/// let is_changed = pane.on_action(EnergySavingAction::SelectIdleTimeout(IdleTimeout::Off));
///
/// assert!(is_changed);
/// assert_eq!(pane.settings().idle_timeout, IdleTimeout::Off);
/// ```
#[derive(Debug)]
pub struct EnergySavingPane {
    settings: EnergySavingSettings,
}

impl EnergySavingPane {
    /// Creates a pane that shows `settings`.
    #[must_use]
    pub fn new(settings: EnergySavingSettings) -> Self {
        Self { settings }
    }

    /// Returns the settings that the pane shows.
    #[must_use]
    pub fn settings(&self) -> &EnergySavingSettings {
        &self.settings
    }

    /// Handles a user action. Returns whether the settings changed.
    pub fn on_action(&mut self, action: EnergySavingAction) -> bool {
        match action {
            EnergySavingAction::SelectIdleTimeout(timeout) => {
                replace_if_changed(&mut self.settings.idle_timeout, timeout)
            }
            EnergySavingAction::SwitchEnergySaver(rule) => {
                replace_if_changed(&mut self.settings.energy_saver, rule)
            }
            EnergySavingAction::SwitchGame(rule) => replace_if_changed(&mut self.settings.game, rule),
        }
    }

    /// Returns the action of `input`, when `input` is for a control of the pane.
    #[must_use]
    pub fn action_for(&self, input: &SettingsInput) -> Option<EnergySavingAction> {
        match input {
            SettingsInput::Picked { id, index } if id.as_str() == IDLE_TIMEOUT => IDLE_TIMEOUT_CHOICES
                .get(*index)
                .map(|&(timeout, _)| EnergySavingAction::SelectIdleTimeout(timeout)),
            SettingsInput::Toggled { id, is_on } if id.as_str() == ENERGY_SAVER => {
                Some(EnergySavingAction::SwitchEnergySaver(if *is_on {
                    EnergySaverRule::StopSuggestions
                } else {
                    EnergySaverRule::KeepSuggestions
                }))
            }
            SettingsInput::Toggled { id, is_on } if id.as_str() == GAME => {
                Some(EnergySavingAction::SwitchGame(if *is_on {
                    GameRule::StopSuggestions
                } else {
                    GameRule::KeepSuggestions
                }))
            }
            _ => None,
        }
    }

    /// Returns the page that the pane shows now, with the texts of `translations`. While
    /// `energy_saver` is on and the switch stops suggestions then, a note below the switch tells
    /// that suggestions are off.
    #[must_use]
    pub fn page(&self, energy_saver: EnergySaver, translations: &Translations) -> Page {
        let idle_timeout = picker(
            MenuText {
                id: IDLE_TIMEOUT,
                icon: RowIcon::Moon,
                label: "energy-saving-idle-timeout",
                description: "energy-saving-idle-timeout-description",
            },
            &IDLE_TIMEOUT_CHOICES,
            self.settings.idle_timeout,
            translations,
        );
        let stops_in_energy_saver = self.settings.energy_saver == EnergySaverRule::StopSuggestions;
        let energy_saver_switch = Switch {
            id: ControlId::new(ENERGY_SAVER),
            icon: RowIcon::Battery,
            label: translations.text("energy-saving-energy-saver"),
            description: translations.text("energy-saving-energy-saver-description"),
            is_on: stops_in_energy_saver,
            note: (stops_in_energy_saver && energy_saver == EnergySaver::On).then(|| Note {
                text: translations.text("energy-saving-energy-saver-stopped"),
                tone: NoteTone::Plain,
            }),
        };
        let game_switch = Switch {
            id: ControlId::new(GAME),
            icon: RowIcon::Game,
            label: translations.text("energy-saving-game"),
            description: translations.text("energy-saving-game-description"),
            is_on: self.settings.game == GameRule::StopSuggestions,
            note: None,
        };
        Page {
            title: translations.text("energy-saving-title"),
            icon: PageIcon::Lightbulb,
            summary: translations.text("energy-saving-summary"),
            search: None,
            sections: vec![Section::new(vec![
                Row::Picker(idle_timeout),
                Row::Switch(energy_saver_switch),
                Row::Switch(game_switch),
            ])],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_window::view::{ChoiceText, Picker};
    use crate::test_support::{english, picked, toggled};

    fn pane_with(idle_timeout: IdleTimeout, energy_saver: EnergySaverRule) -> EnergySavingPane {
        EnergySavingPane::new(EnergySavingSettings {
            idle_timeout,
            energy_saver,
            ..EnergySavingSettings::default()
        })
    }

    fn energy_saver_note(page: &Page) -> Option<Note> {
        match page.row(ENERGY_SAVER) {
            Some(Row::Switch(switch)) => switch.note.clone(),
            _ => None,
        }
    }

    #[test]
    fn page_shows_the_idle_timeout_menu_and_the_energy_saver_switch() {
        let pane = pane_with(IdleTimeout::ThirtyMinutes, EnergySaverRule::StopSuggestions);

        let shown = pane.page(EnergySaver::Off, &english());

        assert_eq!(
            (shown.row(IDLE_TIMEOUT), shown.row(ENERGY_SAVER)),
            (
                Some(&Row::Picker(Picker {
                    id: ControlId::new(IDLE_TIMEOUT),
                    icon: RowIcon::Moon,
                    label: "Stop after a time without use".to_owned(),
                    description: "When you do not type or click for this time, suggestions stop to free \
                         memory. They start again when you type in a text field."
                        .to_owned(),
                    choices: ["Off", "15 minutes", "30 minutes", "60 minutes"]
                        .map(str::to_owned)
                        .to_vec(),
                    choice_text: ChoiceText::Label,
                    selected: 2,
                    is_enabled: true,
                    note: None,
                })),
                Some(&Row::Switch(Switch {
                    id: ControlId::new(ENERGY_SAVER),
                    icon: RowIcon::Battery,
                    label: "Stop in energy saver".to_owned(),
                    description:
                        "While energy saver or Low Power Mode is on, suggestions stop to save power. \
                         They start again when it goes off."
                            .to_owned(),
                    is_on: true,
                    note: None,
                })),
            )
        );
    }

    #[test]
    fn page_shows_the_game_switch_below_the_energy_saver_switch() {
        let pane = EnergySavingPane::new(EnergySavingSettings::default());

        let shown = pane.page(EnergySaver::Off, &english());

        assert_eq!(
            shown.sections[0].rows.get(2),
            Some(&Row::Switch(Switch {
                id: ControlId::new(GAME),
                icon: RowIcon::Game,
                label: "Stop during games".to_owned(),
                description:
                    "While you play a game, suggestions stop to free memory for the game. They start \
                     again when you type in a text field."
                        .to_owned(),
                is_on: true,
                note: None,
            }))
        );
    }

    #[test]
    fn game_switch_is_off_when_suggestions_continue_during_games() {
        let pane = EnergySavingPane::new(EnergySavingSettings {
            game: GameRule::KeepSuggestions,
            ..EnergySavingSettings::default()
        });

        let shown = pane.page(EnergySaver::Off, &english());

        assert!(matches!(shown.row(GAME), Some(Row::Switch(switch)) if !switch.is_on));
    }

    #[test]
    fn page_has_the_title_and_the_icon_of_the_pane() {
        let pane = EnergySavingPane::new(EnergySavingSettings::default());

        let shown = pane.page(EnergySaver::Off, &english());

        assert_eq!(
            (shown.title.as_str(), shown.icon),
            ("Energy saving", PageIcon::Lightbulb)
        );
    }

    #[test]
    fn energy_saver_that_stops_suggestions_shows_a_note() {
        let pane = pane_with(IdleTimeout::ThirtyMinutes, EnergySaverRule::StopSuggestions);

        let shown = pane.page(EnergySaver::On, &english());

        assert_eq!(
            energy_saver_note(&shown),
            Some(Note {
                text: "Suggestions are off while the computer saves energy.".to_owned(),
                tone: NoteTone::Plain,
            })
        );
    }

    #[test]
    fn energy_saver_that_keeps_suggestions_shows_no_note() {
        let pane = pane_with(IdleTimeout::ThirtyMinutes, EnergySaverRule::KeepSuggestions);

        let shown = pane.page(EnergySaver::On, &english());

        assert_eq!(energy_saver_note(&shown), None);
    }

    #[test]
    fn each_idle_timeout_entry_gives_its_timeout() {
        let pane = EnergySavingPane::new(EnergySavingSettings::default());

        let actions = [0, 1, 2, 3, 4].map(|index| pane.action_for(&picked(IDLE_TIMEOUT, index)));

        assert_eq!(
            actions,
            [
                Some(EnergySavingAction::SelectIdleTimeout(IdleTimeout::Off)),
                Some(EnergySavingAction::SelectIdleTimeout(IdleTimeout::FifteenMinutes)),
                Some(EnergySavingAction::SelectIdleTimeout(IdleTimeout::ThirtyMinutes)),
                Some(EnergySavingAction::SelectIdleTimeout(IdleTimeout::SixtyMinutes)),
                None,
            ]
        );
    }

    #[test]
    fn energy_saver_switch_gives_its_rule() {
        let pane = EnergySavingPane::new(EnergySavingSettings::default());

        let actions = [true, false].map(|is_on| pane.action_for(&toggled(ENERGY_SAVER, is_on)));

        assert_eq!(
            actions,
            [
                Some(EnergySavingAction::SwitchEnergySaver(
                    EnergySaverRule::StopSuggestions
                )),
                Some(EnergySavingAction::SwitchEnergySaver(
                    EnergySaverRule::KeepSuggestions
                )),
            ]
        );
    }

    #[test]
    fn game_switch_gives_its_rule() {
        let pane = EnergySavingPane::new(EnergySavingSettings::default());

        let actions = [true, false].map(|is_on| pane.action_for(&toggled(GAME, is_on)));

        assert_eq!(
            actions,
            [
                Some(EnergySavingAction::SwitchGame(GameRule::StopSuggestions)),
                Some(EnergySavingAction::SwitchGame(GameRule::KeepSuggestions)),
            ]
        );
    }

    #[test]
    fn input_of_another_pane_gives_no_action() {
        let pane = EnergySavingPane::new(EnergySavingSettings::default());

        let action = pane.action_for(&toggled("screen-text", true));

        assert_eq!(action, None);
    }

    #[test]
    fn same_value_does_not_change_the_settings() {
        let mut pane = pane_with(IdleTimeout::ThirtyMinutes, EnergySaverRule::StopSuggestions);

        let changes = [
            pane.on_action(EnergySavingAction::SelectIdleTimeout(IdleTimeout::ThirtyMinutes)),
            pane.on_action(EnergySavingAction::SwitchEnergySaver(
                EnergySaverRule::StopSuggestions,
            )),
            pane.on_action(EnergySavingAction::SwitchGame(GameRule::StopSuggestions)),
        ];

        assert_eq!(changes, [false, false, false]);
    }

    #[test]
    fn energy_saver_switch_changes_the_rule() {
        let mut pane = pane_with(IdleTimeout::ThirtyMinutes, EnergySaverRule::StopSuggestions);

        pane.on_action(EnergySavingAction::SwitchEnergySaver(
            EnergySaverRule::KeepSuggestions,
        ));

        assert_eq!(pane.settings().energy_saver, EnergySaverRule::KeepSuggestions);
    }

    #[test]
    fn game_switch_changes_the_rule() {
        let mut pane = EnergySavingPane::new(EnergySavingSettings::default());

        pane.on_action(EnergySavingAction::SwitchGame(GameRule::KeepSuggestions));

        assert_eq!(pane.settings().game, GameRule::KeepSuggestions);
    }
}
