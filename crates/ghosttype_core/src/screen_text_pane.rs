//! The Screen text pane of the settings window: where suggestions can use the text near the text
//! field on the screen.

use crate::desktop::OcrAvailability;
use crate::settings::{ScreenTextAccess, ScreenTextApp, ScreenTextSettings};
use crate::settings_view::{
    AppSwitch, Button, ControlId, Message, Page, PageIcon, Row, Section, SettingsInput, Switch,
};

/// Something the user did in the Screen text pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenTextAction {
    /// Turned on the main switch.
    Allow,
    /// Turned off the main switch.
    Deny,
    /// Turned the switch of the app with this id on or off.
    SwitchApp {
        /// The id of the app.
        app_id: String,
        /// Whether the switch is on now.
        is_on: bool,
    },
    /// Asked for the permission that the OCR needs.
    AskForOcrPermission,
}

pub(crate) const SCREEN_TEXT: &str = "screen-text";
const APP_SWITCH_PREFIX: &str = "screen-text-app:";
const OCR_PERMISSION: &str = "screen-text-ocr-permission";
const PANE_TITLE: &str = "Screen text";
const PANE_SUMMARY: &str = "Suggestions can use the text near the text field where you type, for example, \
     the last messages of a chat. GhostType reads this text on this computer and does not save the text.";
const SCREEN_TEXT_LABEL: &str = "Use screen text";
const SCREEN_TEXT_DESCRIPTION: &str =
    "When this switch is off, GhostType does not read screen text in any app.";
const APPS_HEADING: &str = "Apps";
const APPS_FOOTER: &str = "When you type in an app for the first time, GhostType adds the app to this list.";
const NO_APPS: &str = "This list has no apps.";
const OCR_PERMISSION_NOTE: &str = "GhostType reads only part of the screen text. To let GhostType read all screen \
     text, go to System Settings > Privacy & Security > Screen & System Audio Recording. Then set the switch for \
     GhostType to on.";
const OPEN_SYSTEM_SETTINGS: &str = "Open System Settings";

/// State of the Screen text pane: the main switch and the switch of each app.
///
/// ```
/// use ghosttype_core::screen_text_pane::{ScreenTextAction, ScreenTextPane};
/// use ghosttype_core::settings::{ScreenTextAccess, ScreenTextSettings};
///
/// let mut pane = ScreenTextPane::new(ScreenTextSettings::default());
///
/// let is_changed = pane.on_action(ScreenTextAction::Deny);
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
            ScreenTextAction::Allow => self.change_access(ScreenTextAccess::Allowed),
            ScreenTextAction::Deny => self.change_access(ScreenTextAccess::Denied),
            ScreenTextAction::SwitchApp { app_id, is_on } => {
                let Some(app) = self.settings.apps.iter_mut().find(|app| app.app_id == app_id) else {
                    return false;
                };
                let is_changed = app.is_on != is_on;
                app.is_on = is_on;
                is_changed
            }
            ScreenTextAction::AskForOcrPermission => false,
        }
    }

    /// Returns the action of `input`, when `input` is for a control of the pane.
    #[must_use]
    pub fn action_for(&self, input: &SettingsInput) -> Option<ScreenTextAction> {
        let (id, is_on) = match input {
            SettingsInput::Pressed(id) if id.as_str() == OCR_PERMISSION => {
                return Some(ScreenTextAction::AskForOcrPermission);
            }
            SettingsInput::Toggled { id, is_on } => (id, is_on),
            _ => return None,
        };
        if id.as_str() == SCREEN_TEXT {
            return Some(if *is_on {
                ScreenTextAction::Allow
            } else {
                ScreenTextAction::Deny
            });
        }
        let app_id = id.as_str().strip_prefix(APP_SWITCH_PREFIX)?;
        Some(ScreenTextAction::SwitchApp {
            app_id: app_id.to_owned(),
            is_on: *is_on,
        })
    }

    /// Returns the page that the pane shows now, with a note that tells how to give the permission
    /// when `ocr` needs it.
    #[must_use]
    pub fn page(&self, ocr: OcrAvailability) -> Page {
        let is_allowed = self.settings.access == ScreenTextAccess::Allowed;
        let apps = self
            .settings
            .apps
            .iter()
            .map(|app| {
                Row::AppSwitch(AppSwitch {
                    id: ControlId::from(format!("{APP_SWITCH_PREFIX}{}", app.app_id)),
                    app_id: app.app_id.clone(),
                    app_path: app.app_path.clone(),
                    app_name: app.app_name.clone(),
                    is_on: app.is_on,
                    is_enabled: is_allowed,
                })
            })
            .collect::<Vec<_>>();
        let app_rows = if apps.is_empty() {
            vec![Row::Message(Message {
                text: NO_APPS.to_owned(),
                button: None,
            })]
        } else {
            apps
        };
        let main_switch = Section::new(vec![Row::Switch(Switch {
            id: ControlId::new(SCREEN_TEXT),
            label: SCREEN_TEXT_LABEL,
            description: SCREEN_TEXT_DESCRIPTION,
            is_on: is_allowed,
        })]);
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
        let app_list = Section {
            heading: Some(APPS_HEADING.to_owned()),
            back: None,
            rows: app_rows,
            footer: Some(APPS_FOOTER),
        };
        Page {
            title: PANE_TITLE,
            icon: PageIcon::TextViewfinder,
            summary: PANE_SUMMARY,
            search: None,
            sections: [Some(main_switch), permission_note, Some(app_list)]
                .into_iter()
                .flatten()
                .collect(),
        }
    }

    fn change_access(&mut self, access: ScreenTextAccess) -> bool {
        let is_changed = self.settings.access != access;
        self.settings.access = access;
        is_changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(app_id: &str, app_name: &str, is_on: bool) -> ScreenTextApp {
        ScreenTextApp {
            app_id: app_id.to_owned(),
            app_name: app_name.to_owned(),
            app_path: None,
            is_on,
        }
    }

    fn pane_with(access: ScreenTextAccess, apps: Vec<ScreenTextApp>) -> ScreenTextPane {
        ScreenTextPane::new(ScreenTextSettings { access, apps })
    }

    fn toggled(id: &str, is_on: bool) -> SettingsInput {
        SettingsInput::Toggled {
            id: ControlId::from(id.to_owned()),
            is_on,
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
            vec![app("com.apple.mail", "Mail", false)],
        );

        let shown = pane.page(OcrAvailability::Ready);

        assert_eq!(
            (shown.row(SCREEN_TEXT), app_switches(&shown)),
            (
                Some(&Row::Switch(Switch {
                    id: ControlId::new(SCREEN_TEXT),
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
            vec![app("com.apple.mail", "Mail", true)],
        );

        let shown = pane.page(OcrAvailability::Ready);

        assert_eq!(
            app_switches(&shown)
                .iter()
                .map(|switch| (switch.is_on, switch.is_enabled))
                .collect::<Vec<_>>(),
            [(true, false)]
        );
    }

    #[test]
    fn empty_list_says_that_it_has_no_apps() {
        let pane = pane_with(ScreenTextAccess::Allowed, Vec::new());

        let shown = pane.page(OcrAvailability::Ready);

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

        let shown = pane.page(OcrAvailability::NotPermitted);

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
            ready.page(OcrAvailability::Ready),
            ready.page(OcrAvailability::Unsupported),
            off.page(OcrAvailability::NotPermitted),
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
                Some(ScreenTextAction::Allow),
                Some(ScreenTextAction::Deny),
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
                app("com.apple.mail", "Mail", true),
                app("slack.exe", "Slack", true),
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
                    app("com.apple.mail", "Mail", true),
                    app("slack.exe", "Slack", false)
                ]
            )
        );
    }

    #[test]
    fn action_that_changes_nothing_reports_no_change() {
        let mut pane = pane_with(
            ScreenTextAccess::Allowed,
            vec![app("com.apple.mail", "Mail", true)],
        );
        let actions = [
            ScreenTextAction::Allow,
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
    fn typed_app_is_added_one_time() {
        let mut pane = pane_with(ScreenTextAccess::Allowed, Vec::new());

        let changes = [
            pane.add_app(&app("slack.exe", "Slack", true)),
            pane.add_app(&app("slack.exe", "Slack", true)),
        ];

        assert_eq!(changes, [true, false]);
    }
}
