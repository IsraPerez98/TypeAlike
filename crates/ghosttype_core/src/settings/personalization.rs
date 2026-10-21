//! The settings of the Personalization pane: what suggestions learn from the earlier writing.

use crate::settings::ScreenTextApp;
use crate::settings::known_apps::{is_password_manager, is_terminal};

/// Whether GhostType saves the text that the user writes, for the suggestions to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryAccess {
    /// GhostType saves no new text, and suggestions use no earlier writing.
    #[default]
    Off,
    /// GhostType saves the text that the user writes in each app that is on in the list of the
    /// Personalization pane, and suggestions use it.
    On,
}

/// Which texts GhostType saves while the history is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeptTexts {
    /// Only a text where the user accepted a suggestion.
    #[default]
    AcceptedOnly,
    /// Each text.
    All,
}

/// How much suggestions prefer the words that the user often writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordChoice {
    /// Suggestions do not prefer the words of the user.
    Off,
    /// A small preference.
    Light,
    /// A medium preference.
    #[default]
    Medium,
    /// A large preference, which can make a suggestion match the text less well.
    Strong,
}

/// Which earlier writing an app gives to suggestions and gets from them. An app gives its text
/// only to the apps that it gets text from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryScope {
    /// GhostType saves no text of the app, and suggestions in the app use no earlier writing.
    Off,
    /// GhostType saves the text of the app, and suggestions in the app use only the text of the
    /// app.
    ThisApp,
    /// GhostType saves the text of the app, and suggestions in each app with this scope use the
    /// text of all these apps, the text of the same app or web page first.
    AllApps,
}

/// An app of the Personalization pane, and which earlier writing it gives and gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryApp {
    /// The id of the app, as [`Surroundings::app_id`](crate::desktop::Surroundings::app_id)
    /// gives it.
    pub app_id: String,
    /// The name of the app, as the user knows it.
    pub app_name: String,
    /// The full path of the program of the app, as
    /// [`Surroundings::app_path`](crate::desktop::Surroundings::app_path) gives it.
    pub app_path: Option<String>,
    /// Which earlier writing the app gives and gets.
    pub scope: HistoryScope,
}

impl HistoryApp {
    /// The entry of `app`, where the user types, which shares its text with all apps unless the
    /// app is a password manager or a terminal.
    pub(crate) fn of(app: &ScreenTextApp) -> Self {
        Self {
            app_id: app.app_id.clone(),
            app_name: app.app_name.clone(),
            app_path: app.app_path.clone(),
            scope: default_scope(&app.app_id),
        }
    }
}

/// What suggestions learn from the earlier writing of the user.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PersonalizationSettings {
    /// The main switch: whether GhostType saves and uses the text of any app.
    pub history: HistoryAccess,
    /// Which texts GhostType saves.
    pub kept_texts: KeptTexts,
    /// The apps where the user typed, in alphabetical order of their names.
    pub apps: Vec<HistoryApp>,
    /// How much suggestions prefer the words that the user often writes.
    pub word_choice: WordChoice,
}

impl PersonalizationSettings {
    /// Which earlier writing the app with `app_id` gives and gets. Each app is off while the main
    /// switch is off. An app that is not in the list yet shares with all apps, unless it is a
    /// password manager or a terminal.
    ///
    /// ```
    /// use ghosttype_core::settings::{HistoryAccess, HistoryScope, PersonalizationSettings};
    ///
    /// let settings = PersonalizationSettings {
    ///     history: HistoryAccess::On,
    ///     ..PersonalizationSettings::default()
    /// };
    ///
    /// assert_eq!(settings.scope_of("com.apple.mail"), HistoryScope::AllApps);
    /// assert_eq!(settings.scope_of("com.apple.Terminal"), HistoryScope::Off);
    /// ```
    #[must_use]
    pub fn scope_of(&self, app_id: &str) -> HistoryScope {
        if self.history == HistoryAccess::Off {
            return HistoryScope::Off;
        }
        self.apps
            .iter()
            .find(|app| app.app_id == app_id)
            .map_or_else(|| default_scope(app_id), |app| app.scope)
    }

    /// Adds `app` to the list in its alphabetical place, unless the list has an app with its id.
    /// Returns whether the list changed.
    pub(crate) fn add(&mut self, app: &HistoryApp) -> bool {
        if self.apps.iter().any(|known| known.app_id == app.app_id) {
            return false;
        }
        let name = app.app_name.to_lowercase();
        let place = self
            .apps
            .partition_point(|known| known.app_name.to_lowercase() <= name);
        self.apps.insert(place, app.clone());
        true
    }
}

fn default_scope(app_id: &str) -> HistoryScope {
    if is_password_manager(app_id) || is_terminal(app_id) {
        HistoryScope::Off
    } else {
        HistoryScope::AllApps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::tests::app;

    #[test]
    fn word_choice_is_medium_by_default() {
        let settings = PersonalizationSettings::default();

        assert_eq!(settings.word_choice, WordChoice::Medium);
    }

    fn history_app(app_id: &str, app_name: &str, scope: HistoryScope) -> HistoryApp {
        HistoryApp {
            app_id: app_id.to_owned(),
            app_name: app_name.to_owned(),
            app_path: None,
            scope,
        }
    }

    fn personalization(history: HistoryAccess, apps: Vec<HistoryApp>) -> PersonalizationSettings {
        PersonalizationSettings {
            history,
            apps,
            ..PersonalizationSettings::default()
        }
    }

    #[test]
    fn new_history_app_shares_with_all_apps_unless_it_is_a_password_manager_or_a_terminal() {
        let cases = [
            ("com.apple.mail", HistoryScope::AllApps),
            ("com.1password.1password", HistoryScope::Off),
            ("com.apple.Terminal", HistoryScope::Off),
            ("WindowsTerminal.exe", HistoryScope::Off),
            ("pwsh.exe", HistoryScope::Off),
        ];

        let scopes = cases.map(|(app_id, _)| HistoryApp::of(&app(app_id, "App", true)).scope);

        assert_eq!(scopes, cases.map(|(_, scope)| scope));
    }

    #[test]
    fn scope_of_an_app_in_the_list_is_its_own() {
        let settings = personalization(
            HistoryAccess::On,
            vec![
                history_app("com.apple.mail", "Mail", HistoryScope::Off),
                history_app("com.apple.Notes", "Notes", HistoryScope::ThisApp),
                history_app("com.apple.Terminal", "Terminal", HistoryScope::AllApps),
            ],
        );

        let scopes = ["com.apple.mail", "com.apple.Notes", "com.apple.Terminal"]
            .map(|app_id| settings.scope_of(app_id));

        assert_eq!(
            scopes,
            [HistoryScope::Off, HistoryScope::ThisApp, HistoryScope::AllApps]
        );
    }

    #[test]
    fn scope_of_an_app_that_is_not_in_the_list_is_all_apps_unless_it_is_a_password_manager_or_a_terminal() {
        let settings = personalization(HistoryAccess::On, Vec::new());

        let scopes =
            ["com.apple.notes", "com.1password.1password", "cmd.exe"].map(|app_id| settings.scope_of(app_id));

        assert_eq!(
            scopes,
            [HistoryScope::AllApps, HistoryScope::Off, HistoryScope::Off]
        );
    }

    #[test]
    fn history_off_turns_off_each_app() {
        let settings = personalization(
            HistoryAccess::Off,
            vec![history_app("com.apple.mail", "Mail", HistoryScope::AllApps)],
        );

        let scopes = ["com.apple.mail", "com.apple.notes"].map(|app_id| settings.scope_of(app_id));

        assert_eq!(scopes, [HistoryScope::Off, HistoryScope::Off]);
    }

    #[test]
    fn history_apps_go_in_alphabetical_order_and_once() {
        let mut settings = personalization(HistoryAccess::On, Vec::new());

        let added = [
            history_app("slack.exe", "Slack", HistoryScope::AllApps),
            history_app("com.apple.mail", "mail", HistoryScope::AllApps),
            history_app("slack.exe", "Slack", HistoryScope::Off),
        ]
        .map(|entry| settings.add(&entry));

        assert_eq!(added, [true, true, false]);
        assert_eq!(
            settings.apps,
            [
                history_app("com.apple.mail", "mail", HistoryScope::AllApps),
                history_app("slack.exe", "Slack", HistoryScope::AllApps)
            ]
        );
    }
}
