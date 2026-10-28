//! The Permissions pane of the settings window: the permissions that the OS asks the user to give
//! TypeAlike, and whether TypeAlike has them.

use crate::desktop::{Permission, PermissionState};
use crate::localization::Translations;
use crate::settings_window::view::{
    Access, Button, ControlId, Page, PageIcon, PermissionRow, Row, RowIcon, Section, SettingsInput,
};

const ACCESSIBILITY: &str = "permission-accessibility";
const SCREEN_RECORDING: &str = "permission-screen-recording";

/// Returns the page of the `permissions` of the OS, each with its state, with the texts of
/// `translations`. Returns `None` when the OS asks for no permission.
#[must_use]
pub fn page(permissions: &[(Permission, PermissionState)], translations: &Translations) -> Option<Page> {
    if permissions.is_empty() {
        return None;
    }
    let rows = permissions
        .iter()
        .map(|&(permission, state)| permission_row(permission, state, translations))
        .collect();
    Some(Page {
        title: translations.text("permissions-title"),
        icon: PageIcon::Hand,
        summary: translations.text("permissions-summary"),
        search: None,
        sections: vec![Section {
            footer: Some(translations.text("permissions-footer")),
            ..Section::new(rows)
        }],
    })
}

/// Returns the permission that `input` asks for, when `input` is a press of a button of the pane.
#[must_use]
pub fn action_for(input: &SettingsInput) -> Option<Permission> {
    let SettingsInput::Pressed(id) = input else {
        return None;
    };
    match id.as_str() {
        ACCESSIBILITY => Some(Permission::Accessibility),
        SCREEN_RECORDING => Some(Permission::ScreenRecording),
        _ => None,
    }
}

pub(crate) fn permission_row(
    permission: Permission,
    state: PermissionState,
    translations: &Translations,
) -> Row {
    let (id, icon, label, description) = match permission {
        Permission::Accessibility => (
            ACCESSIBILITY,
            RowIcon::Accessibility,
            "permissions-accessibility",
            "permissions-accessibility-description",
        ),
        Permission::ScreenRecording => (
            SCREEN_RECORDING,
            RowIcon::Monitor,
            "permissions-screen-recording",
            "permissions-screen-recording-description",
        ),
    };
    let access = match state {
        PermissionState::Granted => Access::Granted(translations.text("permissions-granted")),
        PermissionState::Missing => Access::Missing(Button {
            id: ControlId::new(id),
            label: translations.text("permissions-give-access"),
            is_enabled: true,
        }),
    };
    Row::Permission(PermissionRow {
        icon,
        label: translations.text(label),
        description: translations.text(description),
        access,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::localization::UiLanguage;
    use crate::test_support::{english, writing_rule_breaks};

    const BOTH_STATES: [(Permission, PermissionState); 2] = [
        (Permission::Accessibility, PermissionState::Granted),
        (Permission::ScreenRecording, PermissionState::Missing),
    ];

    #[test]
    fn page_shows_each_permission_with_its_state() {
        let shown = page(&BOTH_STATES, &english());

        assert_eq!(
            shown,
            Some(Page {
                title: "Permissions".to_owned(),
                icon: PageIcon::Hand,
                summary: "TypeAlike uses these permissions to show suggestions in other apps. You give them in System Settings.".to_owned(),
                search: None,
                sections: vec![Section {
                    footer: Some("When System Settings asks you to quit and reopen TypeAlike, do it, so that TypeAlike can use the new permission.".to_owned()),
                    ..Section::new(vec![
                        Row::Permission(PermissionRow {
                            icon: RowIcon::Accessibility,
                            label: "Accessibility".to_owned(),
                            description: "TypeAlike needs this permission to read the text where you type and to type a suggestion when you press Tab. Without it, no suggestion shows.".to_owned(),
                            access: Access::Granted("Allowed".to_owned()),
                        }),
                        Row::Permission(PermissionRow {
                            icon: RowIcon::Monitor,
                            label: "Screen & System Audio Recording".to_owned(),
                            description: "Some apps do not give their text to TypeAlike. With this permission, TypeAlike reads their text from an image of the window. Suggestions also work without it.".to_owned(),
                            access: Access::Missing(Button {
                                id: ControlId::new(SCREEN_RECORDING),
                                label: "Give access".to_owned(),
                                is_enabled: true,
                            }),
                        }),
                    ])
                }],
            })
        );
    }

    #[test]
    fn os_that_asks_for_no_permission_has_no_page() {
        let shown = page(&[], &english());

        assert_eq!(shown, None);
    }

    #[test]
    fn give_access_button_asks_for_its_permission() {
        let cases = [
            (ACCESSIBILITY, Some(Permission::Accessibility)),
            (SCREEN_RECORDING, Some(Permission::ScreenRecording)),
            ("download", None),
        ];

        let actions = cases.map(|(id, _)| action_for(&SettingsInput::Pressed(ControlId::new(id))));

        assert_eq!(actions, cases.map(|(_, permission)| permission));
    }

    #[test]
    fn switch_with_the_id_of_a_button_asks_for_nothing() {
        let input = SettingsInput::Toggled {
            id: ControlId::new(ACCESSIBILITY),
            is_on: true,
        };

        assert_eq!(action_for(&input), None);
    }

    #[test]
    fn page_follows_the_writing_rules() {
        let missing = BOTH_STATES.map(|(permission, _)| (permission, PermissionState::Missing));
        let shown = [page(&BOTH_STATES, &english()), page(&missing, &english())];

        let texts: Vec<_> = shown.iter().flatten().flat_map(Page::texts).collect();

        assert_eq!(writing_rule_breaks(&texts), Vec::<String>::new());
    }

    #[test]
    fn page_shows_in_spanish() {
        let translations = Translations::new(UiLanguage::Spanish);

        let title = page(&BOTH_STATES, &translations).map(|shown| shown.title);

        assert_eq!(title.as_deref(), Some("Permisos"));
    }
}
