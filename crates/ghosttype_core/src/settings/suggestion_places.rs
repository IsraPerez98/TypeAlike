//! The settings of the Apps and websites pane: the apps and the websites where suggestions show.

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

/// A website of the Apps and websites pane, and whether suggestions show on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionSite {
    /// The host of the website, as [`Surroundings::domain`](crate::desktop::Surroundings::domain)
    /// gives it.
    pub host: String,
    /// Whether suggestions show on the website.
    pub is_on: bool,
}

/// The apps and the websites where the user typed, each with its switch for suggestions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SuggestionPlaces {
    /// The apps, in alphabetical order of their names.
    pub apps: Vec<SuggestionApp>,
    /// The websites, in alphabetical order of their hosts.
    pub sites: Vec<SuggestionSite>,
}

impl SuggestionPlaces {
    /// Whether suggestions show in a field of the app with `app_id`, on the website `host` if any.
    /// The app and the website must both be on. An app or a website that is not in its list yet is
    /// on.
    ///
    /// ```
    /// use ghosttype_core::settings::{SuggestionPlaces, SuggestionSite};
    ///
    /// let places = SuggestionPlaces {
    ///     sites: vec![SuggestionSite { host: "bank.example".to_owned(), is_on: false }],
    ///     ..SuggestionPlaces::default()
    /// };
    ///
    /// assert!(places.allows(Some("com.apple.Safari"), Some("mail.example")));
    /// assert!(!places.allows(Some("com.apple.Safari"), Some("bank.example")));
    /// ```
    #[must_use]
    pub fn allows(&self, app_id: Option<&str>, host: Option<&str>) -> bool {
        let is_app_on = app_id.is_none_or(|app_id| {
            self.apps
                .iter()
                .find(|app| app.app_id == app_id)
                .is_none_or(|app| app.is_on)
        });
        let is_site_on = host.is_none_or(|host| {
            self.sites
                .iter()
                .find(|site| site.host == host)
                .is_none_or(|site| site.is_on)
        });
        is_app_on && is_site_on
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{suggestion_app, suggestion_site};

    fn places(apps: Vec<SuggestionApp>, sites: Vec<SuggestionSite>) -> SuggestionPlaces {
        SuggestionPlaces { apps, sites }
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
}
