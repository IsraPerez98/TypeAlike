//! The settings of the Screen text pane: where TypeAlike reads the text near the text field.

use crate::desktop::Surroundings;
use crate::settings::known_apps::{insert_in_name_order, is_password_manager};

/// Whether suggestions may use the text near the text field on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenTextAccess {
    /// TypeAlike reads the screen text in each app that is on in the list of the Screen text pane.
    #[default]
    Allowed,
    /// TypeAlike reads no screen text.
    Denied,
}

/// An app of the Screen text pane, and whether TypeAlike reads the screen text in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenTextApp {
    /// The id of the app, as [`Surroundings::app_id`] gives it.
    pub app_id: String,
    /// The name of the app, as the user knows it.
    pub app_name: String,
    /// The full path of the program of the app, as [`Surroundings::app_path`] gives it.
    pub app_path: Option<String>,
    /// Whether TypeAlike reads the screen text in the app.
    pub is_on: bool,
}

impl ScreenTextApp {
    /// The entry of the app of `surroundings`, which is on unless the app is a password manager.
    /// `None` when the app has no id.
    pub(crate) fn of(surroundings: &Surroundings) -> Option<Self> {
        let app_id = surroundings.app_id.clone()?;
        Some(Self {
            app_name: surroundings.app_name.clone().unwrap_or_else(|| app_id.clone()),
            app_path: surroundings.app_path.clone(),
            is_on: !is_password_manager(&app_id),
            app_id,
        })
    }
}

/// How the OCR recognizes the text in a capture of a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OcrLevel {
    /// Fast, for clean text in a few languages.
    #[default]
    Fast,
    /// Slower, with fewer errors and more languages.
    Accurate,
}

/// Where TypeAlike reads the text near the text field on the screen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScreenTextSettings {
    /// The main switch: whether TypeAlike reads screen text in any app.
    pub access: ScreenTextAccess,
    /// The apps where the user typed, in alphabetical order of their names.
    pub apps: Vec<ScreenTextApp>,
    /// How the OCR recognizes the text of a window that gives no text.
    pub ocr_level: OcrLevel,
}

impl ScreenTextSettings {
    /// Whether TypeAlike reads the screen text in the app with `app_id`. An app that is not in the
    /// list yet is on, unless it is a password manager.
    ///
    /// ```
    /// use typealike_core::settings::ScreenTextSettings;
    ///
    /// let settings = ScreenTextSettings::default();
    ///
    /// assert!(settings.allows("com.apple.mail"));
    /// assert!(!settings.allows("com.1password.1password"));
    /// ```
    #[must_use]
    pub fn allows(&self, app_id: &str) -> bool {
        let entry = self.apps.iter().find(|app| app.app_id == app_id);
        self.access == ScreenTextAccess::Allowed
            && entry.map_or_else(|| !is_password_manager(app_id), |app| app.is_on)
    }

    /// Adds `app` to the list in its alphabetical place, unless the list has an app with its id.
    /// Returns whether the list changed.
    pub(crate) fn add(&mut self, app: &ScreenTextApp) -> bool {
        insert_in_name_order(&mut self.apps, app, |app| &app.app_id, |app| &app.app_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::screen_text_app;

    fn surroundings_of(app_id: Option<&str>, app_name: Option<&str>) -> Surroundings {
        Surroundings {
            app_id: app_id.map(str::to_owned),
            app_name: app_name.map(str::to_owned),
            app_path: Some("C:\\Program Files\\App\\app.exe".to_owned()),
            ..Surroundings::default()
        }
    }

    #[test]
    fn new_app_entry_is_on_unless_the_app_is_a_password_manager() {
        let cases = [
            ("com.apple.mail", true),
            ("slack.exe", true),
            ("com.1password.1password", false),
            ("KeePassXC.exe", false),
        ];

        let switches = cases.map(|(app_id, _)| {
            ScreenTextApp::of(&surroundings_of(Some(app_id), Some("App"))).map(|entry| entry.is_on)
        });

        assert_eq!(switches, cases.map(|(_, is_on)| Some(is_on)));
    }

    #[test]
    fn app_entry_keeps_the_path_of_the_app() {
        let entry = ScreenTextApp::of(&surroundings_of(Some("notepad.exe"), Some("Notepad")));

        assert_eq!(
            entry,
            Some(ScreenTextApp {
                app_path: Some("C:\\Program Files\\App\\app.exe".to_owned()),
                ..screen_text_app("notepad.exe", "Notepad", true)
            })
        );
    }

    #[test]
    fn app_without_a_name_is_named_by_its_id() {
        let entry = ScreenTextApp::of(&surroundings_of(Some("notepad.exe"), None));

        assert_eq!(
            entry,
            Some(ScreenTextApp {
                app_path: Some("C:\\Program Files\\App\\app.exe".to_owned()),
                ..screen_text_app("notepad.exe", "notepad.exe", true)
            })
        );
    }

    #[test]
    fn app_without_an_id_has_no_entry() {
        let entry = ScreenTextApp::of(&surroundings_of(None, Some("Mail")));

        assert_eq!(entry, None);
    }

    #[test]
    fn app_switch_decides_for_its_app() {
        let settings = ScreenTextSettings {
            access: ScreenTextAccess::Allowed,
            apps: vec![
                screen_text_app("com.apple.mail", "Mail", false),
                screen_text_app("com.1password.1password", "1Password", true),
            ],
            ..ScreenTextSettings::default()
        };

        let allowed = ["com.apple.mail", "com.1password.1password", "com.apple.notes"]
            .map(|app_id| settings.allows(app_id));

        assert_eq!(allowed, [false, true, true]);
    }

    #[test]
    fn main_switch_off_allows_no_app() {
        let settings = ScreenTextSettings {
            access: ScreenTextAccess::Denied,
            apps: vec![screen_text_app("com.apple.mail", "Mail", true)],
            ..ScreenTextSettings::default()
        };

        let allowed = ["com.apple.mail", "com.apple.notes"].map(|app_id| settings.allows(app_id));

        assert_eq!(allowed, [false, false]);
    }

    #[test]
    fn apps_go_in_alphabetical_order_of_their_names() {
        let mut settings = ScreenTextSettings::default();

        let added = [
            screen_text_app("slack.exe", "Slack", true),
            screen_text_app("com.apple.mail", "mail", true),
            screen_text_app("b.exe", "Bear", true),
        ]
        .map(|entry| settings.add(&entry));

        assert_eq!(added, [true; 3]);
        assert_eq!(
            settings
                .apps
                .iter()
                .map(|entry| entry.app_name.as_str())
                .collect::<Vec<_>>(),
            ["Bear", "mail", "Slack"]
        );
    }

    #[test]
    fn known_app_is_not_added_again() {
        let mut settings = ScreenTextSettings {
            access: ScreenTextAccess::Allowed,
            apps: vec![screen_text_app("com.apple.mail", "Mail", false)],
            ..ScreenTextSettings::default()
        };

        let added = settings.add(&screen_text_app("com.apple.mail", "Mail", true));

        assert!(!added);
        assert_eq!(settings.apps, [screen_text_app("com.apple.mail", "Mail", false)]);
    }
}
