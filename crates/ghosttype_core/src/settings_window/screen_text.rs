//! The Screen text pane of the settings window: where suggestions can use the text near the text
//! field on the screen.

use crate::desktop::{OcrAvailability, OcrLevels};
use crate::settings::{OcrLevel, ScreenTextAccess, ScreenTextApp, ScreenTextSettings, SuggestionPlaces};
use crate::settings_window::controls::{
    MenuText, SUGGESTED_APPS_FOOTER, app_list_section, picker, replace_if_changed,
};
use crate::settings_window::view::{
    AppSwitch, Button, ControlId, Message, Page, PageIcon, Picker, Row, RowIcon, Section, SettingsInput,
    Switch,
};

/// Something the user did in the Screen text pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenTextAction {
    /// Turned the main switch on or off.
    SwitchScreenText(ScreenTextAccess),
    /// Turned the switch of the app with this id on or off.
    SwitchApp {
        /// The id of the app.
        app_id: String,
        /// Whether the switch is on now.
        is_on: bool,
    },
    /// Asked for the permission that the OCR needs.
    AskForOcrPermission,
    /// Picked this level of the OCR.
    SelectOcrLevel(OcrLevel),
}

pub(crate) const SCREEN_TEXT: &str = "screen-text";
const APP_SWITCH_PREFIX: &str = "screen-text-app:";
const OCR_PERMISSION: &str = "screen-text-ocr-permission";
const OCR_LEVEL: &str = "screen-text-ocr-level";
const PANE_TITLE: &str = "Screen text";
const PANE_SUMMARY: &str = "Suggestions can use the text near the text field where you type, for example, \
     the last messages of a chat. GhostType reads this text on this computer and does not save the text.";
const SCREEN_TEXT_LABEL: &str = "Use screen text";
const SCREEN_TEXT_DESCRIPTION: &str =
    "When this switch is off, GhostType does not read screen text in any app.";
const OCR_LEVEL_LABEL: &str = "Text recognition";
const OCR_LEVEL_DESCRIPTION: &str = "When GhostType cannot get the text of a window, it reads the text from an \
     image of the window. “Fast” reads English, French, German, Italian, Portuguese and Spanish. “Accurate” \
     reads more languages with fewer errors, but it is slower.";
const OCR_LEVEL_CHOICES: [(OcrLevel, &str); 2] = [
    (OcrLevel::Fast, "Fast (recommended)"),
    (OcrLevel::Accurate, "Accurate"),
];
const OCR_PERMISSION_NOTE: &str = "GhostType reads only part of the screen text. To let GhostType read all screen \
     text, go to System Settings > Privacy & Security > Screen & System Audio Recording. Then set the switch for \
     GhostType to on.";
const OPEN_SYSTEM_SETTINGS: &str = "Open System Settings";

/// State of the Screen text pane: the main switch and the switch of each app.
///
/// ```
/// use ghosttype_core::settings_window::screen_text::{ScreenTextAction, ScreenTextPane};
/// use ghosttype_core::settings::{ScreenTextAccess, ScreenTextSettings};
///
/// let mut pane = ScreenTextPane::new(ScreenTextSettings::default());
///
/// let is_changed = pane.on_action(ScreenTextAction::SwitchScreenText(ScreenTextAccess::Denied));
///
/// assert!(is_changed);
/// assert_eq!(pane.settings().access, ScreenTextAccess::Denied);
/// ```
#[derive(Debug)]
pub struct ScreenTextPane {
    settings: ScreenTextSettings,
}

impl ScreenTextPane {
    /// Creates a pane that shows `settings`.
    #[must_use]
    pub fn new(settings: ScreenTextSettings) -> Self {
        Self { settings }
    }

    /// Returns the settings that the pane shows.
    #[must_use]
    pub fn settings(&self) -> &ScreenTextSettings {
        &self.settings
    }

    /// Adds `app`, where the user typed, to the list when the list does not have it. Returns
    /// whether the settings changed.
    pub fn add_app(&mut self, app: &ScreenTextApp) -> bool {
        self.settings.add(app)
    }

    /// Handles a user action. Returns whether the settings changed.
    pub fn on_action(&mut self, action: ScreenTextAction) -> bool {
        match action {
            ScreenTextAction::SwitchScreenText(access) => {
                replace_if_changed(&mut self.settings.access, access)
            }
            ScreenTextAction::SwitchApp { app_id, is_on } => {
                let Some(app) = self.settings.apps.iter_mut().find(|app| app.app_id == app_id) else {
                    return false;
                };
                replace_if_changed(&mut app.is_on, is_on)
            }
            ScreenTextAction::AskForOcrPermission => false,
            ScreenTextAction::SelectOcrLevel(level) => {
                replace_if_changed(&mut self.settings.ocr_level, level)
            }
        }
    }

    /// Returns the action of `input`, when `input` is for a control of the pane.
    #[must_use]
    pub fn action_for(&self, input: &SettingsInput) -> Option<ScreenTextAction> {
        match input {
            SettingsInput::Pressed(id) if id.as_str() == OCR_PERMISSION => {
                Some(ScreenTextAction::AskForOcrPermission)
            }
            SettingsInput::Picked { id, index } if id.as_str() == OCR_LEVEL => OCR_LEVEL_CHOICES
                .get(*index)
                .map(|&(level, _)| ScreenTextAction::SelectOcrLevel(level)),
            SettingsInput::Toggled { id, is_on } if id.as_str() == SCREEN_TEXT => {
                Some(ScreenTextAction::SwitchScreenText(if *is_on {
                    ScreenTextAccess::Allowed
                } else {
                    ScreenTextAccess::Denied
                }))
            }
            SettingsInput::Toggled { id, is_on } => {
                let app_id = id.as_str().strip_prefix(APP_SWITCH_PREFIX)?;
                Some(ScreenTextAction::SwitchApp {
                    app_id: app_id.to_owned(),
                    is_on: *is_on,
                })
            }
            _ => None,
        }
    }

    /// Returns the page that the pane shows now, with the menu of the OCR level when the platform
    /// has an OCR whose `levels` the user picks, and a note that tells how to give the permission
    /// when `ocr` needs it. The switch of an app is dimmed while `suggestion_places` turns its
    /// suggestions off.
    #[must_use]
    pub fn page(
        &self,
        ocr: OcrAvailability,
        levels: OcrLevels,
        suggestion_places: &SuggestionPlaces,
    ) -> Page {
        let is_allowed = self.settings.access == ScreenTextAccess::Allowed;
        let main_switch = Row::Switch(Switch {
            id: ControlId::new(SCREEN_TEXT),
            icon: RowIcon::Monitor,
            label: SCREEN_TEXT_LABEL,
            description: SCREEN_TEXT_DESCRIPTION,
            is_on: is_allowed,
        });
        let has_level_menu = ocr != OcrAvailability::Unsupported && levels == OcrLevels::Selectable;
        let ocr_level = has_level_menu.then(|| Row::Picker(self.ocr_level_picker()));
        let main_section = Section::new([Some(main_switch), ocr_level].into_iter().flatten().collect());
        let needs_permission = is_allowed && ocr == OcrAvailability::NotPermitted;
        let permission_note = needs_permission.then(|| {
            Section::new(vec![Row::Message(Message {
                text: OCR_PERMISSION_NOTE.to_owned(),
                button: Some(Button {
                    id: ControlId::new(OCR_PERMISSION),
                    label: OPEN_SYSTEM_SETTINGS,
                    is_enabled: true,
                }),
            })])
        });
        let app_list = app_list_section(
            self.app_rows(is_allowed, suggestion_places),
            SUGGESTED_APPS_FOOTER,
        );
        Page {
            title: PANE_TITLE,
            icon: PageIcon::TextViewfinder,
            summary: PANE_SUMMARY,
            search: None,
            sections: [Some(main_section), permission_note, Some(app_list)]
                .into_iter()
                .flatten()
                .collect(),
        }
    }

    fn ocr_level_picker(&self) -> Picker {
        let text = MenuText {
            id: OCR_LEVEL,
            icon: RowIcon::Gauge,
            label: OCR_LEVEL_LABEL,
            description: OCR_LEVEL_DESCRIPTION,
        };
        let is_allowed = self.settings.access == ScreenTextAccess::Allowed;
        picker(text, &OCR_LEVEL_CHOICES, self.settings.ocr_level, is_allowed)
    }

    /// The switch of each app, dimmed while the main switch is off or `suggestion_places` turns
    /// the suggestions of the app off.
    fn app_rows(&self, is_allowed: bool, suggestion_places: &SuggestionPlaces) -> Vec<Row> {
        self.settings
            .apps
            .iter()
            .map(|app| {
                Row::AppSwitch(AppSwitch {
                    id: ControlId::from(format!("{APP_SWITCH_PREFIX}{}", app.app_id)),
                    app_id: app.app_id.clone(),
                    app_path: app.app_path.clone(),
                    app_name: app.app_name.clone(),
                    is_on: app.is_on,
                    is_enabled: is_allowed && suggestion_places.allows(Some(&app.app_id), None),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_window::view::ChoiceText;
    use crate::test_support::{screen_text_app, suggestion_app, toggled};

    fn pane_with(access: ScreenTextAccess, apps: Vec<ScreenTextApp>) -> ScreenTextPane {
        ScreenTextPane::new(ScreenTextSettings {
            access,
            apps,
            ..ScreenTextSettings::default()
        })
    }

    fn ocr_level_picker(page: &Page) -> Option<&Picker> {
        match page.row(OCR_LEVEL) {
            Some(Row::Picker(picker)) => Some(picker),
            _ => None,
        }
    }

    fn app_switches(page: &Page) -> Vec<&AppSwitch> {
        page.sections
            .iter()
            .flat_map(|section| &section.rows)
            .filter_map(|row| match row {
                Row::AppSwitch(switch) => Some(switch),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn page_shows_the_main_switch_and_a_switch_for_each_app() {
        let pane = pane_with(
            ScreenTextAccess::Allowed,
            vec![screen_text_app("com.apple.mail", "Mail", false)],
        );

        let shown = pane.page(
            OcrAvailability::Ready,
            OcrLevels::Selectable,
            &SuggestionPlaces::default(),
        );

        assert_eq!(
            (shown.row(SCREEN_TEXT), app_switches(&shown)),
            (
                Some(&Row::Switch(Switch {
                    id: ControlId::new(SCREEN_TEXT),
                    icon: RowIcon::Monitor,
                    label: "Use screen text",
                    description: "When this switch is off, GhostType does not read screen text in any app.",
                    is_on: true,
                })),
                vec![&AppSwitch {
                    id: ControlId::from("screen-text-app:com.apple.mail".to_owned()),
                    app_id: "com.apple.mail".to_owned(),
                    app_path: None,
                    app_name: "Mail".to_owned(),
                    is_on: false,
                    is_enabled: true,
                }]
            )
        );
    }

    #[test]
    fn main_switch_off_dims_the_app_switches_and_keeps_their_values() {
        let pane = pane_with(
            ScreenTextAccess::Denied,
            vec![screen_text_app("com.apple.mail", "Mail", true)],
        );

        let shown = pane.page(
            OcrAvailability::Ready,
            OcrLevels::Selectable,
            &SuggestionPlaces::default(),
        );

        assert_eq!(
            app_switches(&shown)
                .iter()
                .map(|switch| (switch.is_on, switch.is_enabled))
                .collect::<Vec<_>>(),
            [(true, false)]
        );
    }

    #[test]
    fn switch_of_an_app_with_suggestions_off_is_dimmed() {
        let pane = pane_with(
            ScreenTextAccess::Allowed,
            vec![
                screen_text_app("com.apple.mail", "Mail", true),
                screen_text_app("slack.exe", "Slack", true),
            ],
        );
        let places = SuggestionPlaces {
            apps: vec![suggestion_app("com.apple.mail", "Mail", false)],
            ..SuggestionPlaces::default()
        };

        let shown = pane.page(OcrAvailability::Ready, OcrLevels::Selectable, &places);

        assert_eq!(
            app_switches(&shown)
                .iter()
                .map(|switch| (switch.app_id.as_str(), switch.is_on, switch.is_enabled))
                .collect::<Vec<_>>(),
            [("com.apple.mail", true, false), ("slack.exe", true, true)]
        );
    }

    #[test]
    fn empty_list_says_that_it_has_no_apps() {
        let pane = pane_with(ScreenTextAccess::Allowed, Vec::new());

        let shown = pane.page(
            OcrAvailability::Ready,
            OcrLevels::Selectable,
            &SuggestionPlaces::default(),
        );

        assert_eq!(
            shown.sections[1].rows,
            [Row::Message(Message {
                text: "This list has no apps.".to_owned(),
                button: None,
            })]
        );
    }

    fn permission_notes(page: &Page) -> Vec<&Message> {
        page.sections
            .iter()
            .flat_map(|section| &section.rows)
            .filter_map(|row| match row {
                Row::Message(message) if message.button.is_some() => Some(message),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn page_tells_how_to_give_the_ocr_permission() {
        let pane = pane_with(ScreenTextAccess::Allowed, Vec::new());

        let shown = pane.page(
            OcrAvailability::NotPermitted,
            OcrLevels::Selectable,
            &SuggestionPlaces::default(),
        );

        assert_eq!(
            permission_notes(&shown),
            [&Message {
                text: "GhostType reads only part of the screen text. To let GhostType read all screen text, go \
                       to System Settings > Privacy & Security > Screen & System Audio Recording. Then set the \
                       switch for GhostType to on."
                    .to_owned(),
                button: Some(Button {
                    id: ControlId::new(OCR_PERMISSION),
                    label: "Open System Settings",
                    is_enabled: true,
                }),
            }]
        );
    }

    #[test]
    fn page_has_no_permission_note_when_the_note_cannot_help() {
        let ready = pane_with(ScreenTextAccess::Allowed, Vec::new());
        let off = pane_with(ScreenTextAccess::Denied, Vec::new());

        let pages = [
            ready.page(
                OcrAvailability::Ready,
                OcrLevels::Selectable,
                &SuggestionPlaces::default(),
            ),
            ready.page(
                OcrAvailability::Unsupported,
                OcrLevels::Selectable,
                &SuggestionPlaces::default(),
            ),
            off.page(
                OcrAvailability::NotPermitted,
                OcrLevels::Selectable,
                &SuggestionPlaces::default(),
            ),
        ];

        assert!(pages.iter().all(|page| permission_notes(page).is_empty()));
    }

    #[test]
    fn permission_button_asks_for_the_ocr_permission() {
        let pane = pane_with(ScreenTextAccess::Allowed, Vec::new());

        let action = pane.action_for(&SettingsInput::Pressed(ControlId::new(OCR_PERMISSION)));

        assert_eq!(action, Some(ScreenTextAction::AskForOcrPermission));
    }

    #[test]
    fn asking_for_the_ocr_permission_changes_no_setting() {
        let mut pane = pane_with(ScreenTextAccess::Allowed, Vec::new());

        let is_changed = pane.on_action(ScreenTextAction::AskForOcrPermission);

        assert!(!is_changed);
    }

    #[test]
    fn switches_give_their_actions() {
        let pane = pane_with(ScreenTextAccess::Allowed, Vec::new());
        let inputs = [
            toggled(SCREEN_TEXT, true),
            toggled(SCREEN_TEXT, false),
            toggled("screen-text-app:com.apple.mail", false),
            toggled("clipboard", true),
        ];

        let actions = inputs.map(|input| pane.action_for(&input));

        assert_eq!(
            actions,
            [
                Some(ScreenTextAction::SwitchScreenText(ScreenTextAccess::Allowed)),
                Some(ScreenTextAction::SwitchScreenText(ScreenTextAccess::Denied)),
                Some(ScreenTextAction::SwitchApp {
                    app_id: "com.apple.mail".to_owned(),
                    is_on: false,
                }),
                None,
            ]
        );
    }

    #[test]
    fn app_switch_changes_its_app() {
        let mut pane = pane_with(
            ScreenTextAccess::Allowed,
            vec![
                screen_text_app("com.apple.mail", "Mail", true),
                screen_text_app("slack.exe", "Slack", true),
            ],
        );

        let is_changed = pane.on_action(ScreenTextAction::SwitchApp {
            app_id: "slack.exe".to_owned(),
            is_on: false,
        });

        assert_eq!(
            (is_changed, &pane.settings().apps),
            (
                true,
                &vec![
                    screen_text_app("com.apple.mail", "Mail", true),
                    screen_text_app("slack.exe", "Slack", false)
                ]
            )
        );
    }

    #[test]
    fn action_that_changes_nothing_reports_no_change() {
        let mut pane = pane_with(
            ScreenTextAccess::Allowed,
            vec![screen_text_app("com.apple.mail", "Mail", true)],
        );
        let actions = [
            ScreenTextAction::SwitchScreenText(ScreenTextAccess::Allowed),
            ScreenTextAction::SwitchApp {
                app_id: "com.apple.mail".to_owned(),
                is_on: true,
            },
            ScreenTextAction::SwitchApp {
                app_id: "unknown.exe".to_owned(),
                is_on: false,
            },
        ];

        let changes = actions.map(|action| pane.on_action(action));

        assert_eq!(changes, [false; 3]);
    }

    #[test]
    fn ocr_level_menu_shows_fast_as_the_recommended_default() {
        let pane = pane_with(ScreenTextAccess::Allowed, Vec::new());

        let shown = pane.page(
            OcrAvailability::Ready,
            OcrLevels::Selectable,
            &SuggestionPlaces::default(),
        );

        assert_eq!(
            shown.sections[0].rows.get(1),
            Some(&Row::Picker(Picker {
                id: ControlId::new(OCR_LEVEL),
                icon: RowIcon::Gauge,
                label: "Text recognition",
                description: "When GhostType cannot get the text of a window, it reads the text from an image \
                     of the window. “Fast” reads English, French, German, Italian, Portuguese and Spanish. \
                     “Accurate” reads more languages with fewer errors, but it is slower.",
                choices: vec!["Fast (recommended)".to_owned(), "Accurate".to_owned()],
                choice_text: ChoiceText::Label,
                selected: 0,
                is_enabled: true,
                note: None,
            }))
        );
    }

    #[test]
    fn picking_accurate_selects_the_accurate_level() {
        let mut pane = pane_with(ScreenTextAccess::Allowed, Vec::new());
        let action = pane
            .action_for(&SettingsInput::Picked {
                id: ControlId::new(OCR_LEVEL),
                index: 1,
            })
            .expect("the menu has a second entry");

        let is_changed = pane.on_action(action);

        assert_eq!(
            (is_changed, pane.settings().ocr_level),
            (true, OcrLevel::Accurate)
        );
    }

    #[test]
    fn ocr_level_menu_is_dimmed_when_screen_text_is_off() {
        let pane = pane_with(ScreenTextAccess::Denied, Vec::new());

        let shown = pane.page(
            OcrAvailability::Ready,
            OcrLevels::Selectable,
            &SuggestionPlaces::default(),
        );

        assert_eq!(
            ocr_level_picker(&shown).map(|picker| picker.is_enabled),
            Some(false)
        );
    }

    #[test]
    fn page_without_ocr_has_no_ocr_level_menu() {
        let pane = pane_with(ScreenTextAccess::Allowed, Vec::new());

        let shown = pane.page(
            OcrAvailability::Unsupported,
            OcrLevels::Selectable,
            &SuggestionPlaces::default(),
        );

        assert_eq!(ocr_level_picker(&shown), None);
    }

    #[test]
    fn ocr_with_one_level_has_no_ocr_level_menu() {
        let pane = pane_with(ScreenTextAccess::Allowed, Vec::new());

        let shown = pane.page(
            OcrAvailability::Ready,
            OcrLevels::One,
            &SuggestionPlaces::default(),
        );

        assert_eq!(ocr_level_picker(&shown), None);
    }

    #[test]
    fn typed_app_is_added_one_time() {
        let mut pane = pane_with(ScreenTextAccess::Allowed, Vec::new());

        let changes = [
            pane.add_app(&screen_text_app("slack.exe", "Slack", true)),
            pane.add_app(&screen_text_app("slack.exe", "Slack", true)),
        ];

        assert_eq!(changes, [true, false]);
    }
}
