//! The settings of the Apps and websites pane: the apps and the websites where suggestions show.

use crate::settings::ScreenTextApp;
use crate::settings::known_apps::{insert_in_name_order, is_spreadsheet};

/// An app of the Apps and websites pane, and whether suggestions show in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionApp {
    /// The id of the app, as [`Surroundings::app_id`](crate::desktop::Surroundings::app_id)
    /// gives it.
    pub app_id: String,
    /// The name of the app, as the user knows it.
    pub app_name: String,
    /// The full path of the program of the app, as
    /// [`Surroundings::app_path`](crate::desktop::Surroundings::app_path) gives it.
    pub app_path: Option<String>,
    /// Whether suggestions show in the app.
    pub is_on: bool,
}

impl SuggestionApp {
    /// The entry of `app`, where the user types, which is on unless the app is a spreadsheet.
    pub(crate) fn of(app: &ScreenTextApp) -> Self {
        Self {
            app_id: app.app_id.clone(),
            app_name: app.app_name.clone(),
            app_path: app.app_path.clone(),
            is_on: !is_spreadsheet(&app.app_id),
        }
    }
}

/// A website of the Apps and websites pane, and whether suggestions show on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionSite {
    /// The host of the website, as [`Surroundings::domain`](crate::desktop::Surroundings::domain)
    /// gives it.
    pub host: String,
    /// Whether suggestions show on the website.
    pub is_on: bool,
}

/// What TypeAlike does in a private window of a web browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrivateWindowRule {
    /// TypeAlike shows no suggestions, reads no screen text and keeps no earlier writing there.
    #[default]
    StopSuggestions,
    /// Suggestions show as in any other window.
    KeepSuggestions,
}

/// The apps and the websites where the user typed, each with its switch for suggestions, and the
/// rule for private windows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SuggestionPlaces {
    /// The apps, in alphabetical order of their names.
    pub apps: Vec<SuggestionApp>,
    /// The websites, in alphabetical order of their hosts.
    pub sites: Vec<SuggestionSite>,
    /// What TypeAlike does in a private window of a web browser.
    pub private_windows: PrivateWindowRule,
}

impl SuggestionPlaces {
    /// Whether suggestions show in a field of the app with `app_id`, on the website `host` if any.
    /// The app and the website must both be on. An app or a website that is not in its list yet is
    /// on, unless the app is a spreadsheet.
    ///
    /// ```
    /// use typealike_core::settings::{SuggestionPlaces, SuggestionSite};
    ///
    /// let places = SuggestionPlaces {
    ///     sites: vec![SuggestionSite { host: "bank.example".to_owned(), is_on: false }],
    ///     ..SuggestionPlaces::default()
    /// };
    ///
    /// assert!(places.allows(Some("com.apple.Safari"), Some("mail.example")));
    /// assert!(!places.allows(Some("com.apple.Safari"), Some("bank.example")));
    /// assert!(!places.allows(Some("com.microsoft.Excel"), None));
    /// ```
    #[must_use]
    pub fn allows(&self, app_id: Option<&str>, host: Option<&str>) -> bool {
        let is_app_on = app_id.is_none_or(|app_id| {
            self.apps
                .iter()
                .find(|app| app.app_id == app_id)
                .map_or_else(|| !is_spreadsheet(app_id), |app| app.is_on)
        });
        let is_site_on = host.is_none_or(|host| {
            self.sites
                .iter()
                .find(|site| site.host == host)
                .is_none_or(|site| site.is_on)
        });
        is_app_on && is_site_on
    }

    /// Adds `app` to the list in its alphabetical place, unless the list has an app with its id.
    /// Returns whether the list changed.
    pub(crate) fn add_app(&mut self, app: &SuggestionApp) -> bool {
        insert_in_name_order(&mut self.apps, app, |app| &app.app_id, |app| &app.app_name)
    }

    /// Adds the website `host`, which is on, to the list in its alphabetical place, unless the
    /// list has it. Returns whether the list changed.
    pub(crate) fn add_site(&mut self, host: &str) -> bool {
        let site = SuggestionSite {
            host: host.to_owned(),
            is_on: true,
        };
        insert_in_name_order(&mut self.sites, &site, |site| &site.host, |site| &site.host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{screen_text_app, suggestion_app, suggestion_site};

    fn places(apps: Vec<SuggestionApp>, sites: Vec<SuggestionSite>) -> SuggestionPlaces {
        SuggestionPlaces {
            apps,
            sites,
            ..SuggestionPlaces::default()
        }
    }

    #[test]
    fn suggestions_stop_in_private_windows_by_default() {
        let places = SuggestionPlaces::default();

        assert_eq!(places.private_windows, PrivateWindowRule::StopSuggestions);
    }

    #[test]
    fn new_app_entry_is_on_and_keeps_the_name_and_the_path_of_the_app() {
        let app = ScreenTextApp {
            app_path: Some("C:\\Program Files\\App\\app.exe".to_owned()),
            ..screen_text_app("com.1password.1password", "1Password", false)
        };

        let entry = SuggestionApp::of(&app);

        assert_eq!(
            entry,
            SuggestionApp {
                app_path: Some("C:\\Program Files\\App\\app.exe".to_owned()),
                ..suggestion_app("com.1password.1password", "1Password", true)
            }
        );
    }

    #[test]
    fn new_app_entry_is_on_unless_the_app_is_a_spreadsheet() {
        let apps = [
            ("com.microsoft.Excel", "Microsoft Excel"),
            ("EXCEL.EXE", "Excel"),
            ("com.apple.iWork.Numbers", "Numbers"),
            ("com.apple.mail", "Mail"),
        ];

        let is_on =
            apps.map(|(app_id, app_name)| SuggestionApp::of(&screen_text_app(app_id, app_name, true)).is_on);

        assert_eq!(is_on, [false, false, false, true]);
    }

    #[test]
    fn app_without_an_entry_is_on_unless_it_is_a_spreadsheet() {
        let places = SuggestionPlaces::default();

        let allowed = [
            "com.microsoft.Excel",
            "EXCEL.EXE",
            "com.apple.iWork.Numbers",
            "com.apple.mail",
        ]
        .map(|app_id| places.allows(Some(app_id), None));

        assert_eq!(allowed, [false, false, false, true]);
    }

    #[test]
    fn app_entry_that_is_on_turns_on_a_spreadsheet() {
        let places = places(
            vec![suggestion_app("com.microsoft.Excel", "Microsoft Excel", true)],
            Vec::new(),
        );

        let is_allowed = places.allows(Some("com.microsoft.Excel"), None);

        assert!(is_allowed);
    }

    #[test]
    fn app_switch_decides_for_its_app() {
        let places = places(
            vec![
                suggestion_app("com.apple.mail", "Mail", false),
                suggestion_app("slack.exe", "Slack", true),
            ],
            Vec::new(),
        );

        let allowed = ["com.apple.mail", "slack.exe", "com.apple.notes"]
            .map(|app_id| places.allows(Some(app_id), None));

        assert_eq!(allowed, [false, true, true]);
    }

    #[test]
    fn field_gets_suggestions_only_when_its_app_and_its_website_are_on() {
        let places = places(
            vec![
                suggestion_app("com.apple.mail", "Mail", false),
                suggestion_app("com.apple.Safari", "Safari", true),
            ],
            vec![
                suggestion_site("bank.example", false),
                suggestion_site("mail.example", true),
            ],
        );
        let cases = [
            (Some("com.apple.Safari"), Some("mail.example"), true),
            (Some("com.apple.Safari"), Some("bank.example"), false),
            (Some("com.apple.mail"), None, false),
            (Some("com.apple.mail"), Some("mail.example"), false),
            (Some("com.apple.Safari"), Some("news.example"), true),
            (None, Some("bank.example"), false),
            (None, None, true),
        ];

        let allowed = cases.map(|(app_id, host, _)| places.allows(app_id, host));

        assert_eq!(allowed, cases.map(|(_, _, is_allowed)| is_allowed));
    }

    #[test]
    fn website_switch_decides_only_for_its_own_host() {
        let places = places(Vec::new(), vec![suggestion_site("example.com", false)]);

        let allowed =
            ["mail.example.com", "badexample.com", "example.com"].map(|host| places.allows(None, Some(host)));

        assert_eq!(allowed, [true, true, false]);
    }

    #[test]
    fn suggestion_apps_go_in_alphabetical_order_of_their_names() {
        let mut places = SuggestionPlaces::default();

        let added = [
            suggestion_app("slack.exe", "Slack", true),
            suggestion_app("com.apple.mail", "mail", true),
        ]
        .map(|app| places.add_app(&app));

        assert_eq!(added, [true, true]);
        assert_eq!(
            places.apps,
            [
                suggestion_app("com.apple.mail", "mail", true),
                suggestion_app("slack.exe", "Slack", true)
            ]
        );
    }

    #[test]
    fn known_suggestion_app_is_not_added_again() {
        let mut places = places(vec![suggestion_app("slack.exe", "Slack", false)], Vec::new());

        let added = places.add_app(&suggestion_app("slack.exe", "Slack", true));

        assert!(!added);
        assert_eq!(places.apps, [suggestion_app("slack.exe", "Slack", false)]);
    }

    #[test]
    fn new_websites_are_on_in_alphabetical_order_of_their_hosts() {
        let mut places = SuggestionPlaces::default();

        let added = ["mail.example", "docs.example", "mail.example"].map(|host| places.add_site(host));

        assert_eq!(added, [true, true, false]);
        assert_eq!(
            places.sites,
            [
                suggestion_site("docs.example", true),
                suggestion_site("mail.example", true)
            ]
        );
    }

    #[test]
    fn known_website_keeps_its_switch() {
        let mut places = places(Vec::new(), vec![suggestion_site("bank.example", false)]);

        let added = places.add_site("bank.example");

        assert!(!added);
        assert_eq!(places.sites, [suggestion_site("bank.example", false)]);
    }
}
