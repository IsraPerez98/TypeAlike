//! The Apps and websites pane of the settings window: the apps and the websites where suggestions
//! show.

use crate::settings::{SuggestionApp, SuggestionPlaces};
use crate::settings_window::controls::{
    TYPED_APPS_FOOTER, app_list_section, list_section, replace_if_changed,
};
use crate::settings_window::view::{AppSwitch, ControlId, Page, PageIcon, Row, SettingsInput, SiteSwitch};

/// Something the user did in the Apps and websites pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionPlacesAction {
    /// Turned the switch of the app with this id on or off.
    SwitchApp {
        /// The id of the app.
        app_id: String,
        /// Whether the switch is on now.
        is_on: bool,
    },
    /// Turned the switch of the website with this host on or off.
    SwitchSite {
        /// The host of the website.
        host: String,
        /// Whether the switch is on now.
        is_on: bool,
    },
}

const APP_SWITCH_PREFIX: &str = "suggestions-app:";
const SITE_SWITCH_PREFIX: &str = "suggestions-site:";
const PANE_TITLE: &str = "Apps and websites";
const PANE_SUMMARY: &str = "Suggestions show in each app and on each website where you type. To stop suggestions \
     in an app or on a website, set its switch to off.";
const SITES_HEADING: &str = "Websites";
const NO_SITES: &str = "This list has no websites.";
const SITES_FOOTER: &str =
    "When you type on a website for the first time, GhostType adds the website to this list.";

/// State of the Apps and websites pane: the switch of each app and of each website.
///
/// ```
/// use ghosttype_core::settings::SuggestionPlaces;
/// use ghosttype_core::settings_window::suggestion_places::{
///     SuggestionPlacesAction, SuggestionPlacesPane,
/// };
///
/// let mut pane = SuggestionPlacesPane::new(SuggestionPlaces::default());
/// pane.add_site("bank.example");
///
/// let is_changed = pane.on_action(SuggestionPlacesAction::SwitchSite {
///     host: "bank.example".to_owned(),
///     is_on: false,
/// });
///
/// assert!(is_changed);
/// assert!(!pane.settings().allows(None, Some("bank.example")));
/// ```
#[derive(Debug)]
pub struct SuggestionPlacesPane {
    settings: SuggestionPlaces,
}

impl SuggestionPlacesPane {
    /// Creates a pane that shows `settings`.
    #[must_use]
    pub fn new(settings: SuggestionPlaces) -> Self {
        Self { settings }
    }

    /// Returns the settings that the pane shows.
    #[must_use]
    pub fn settings(&self) -> &SuggestionPlaces {
        &self.settings
    }

    /// Adds `app`, where the user typed, to the list when the list does not have it. Returns
    /// whether the settings changed.
    pub fn add_app(&mut self, app: &SuggestionApp) -> bool {
        self.settings.add_app(app)
    }

    /// Adds the website `host`, where the user typed, to the list when the list does not have it.
    /// Returns whether the settings changed.
    pub fn add_site(&mut self, host: &str) -> bool {
        self.settings.add_site(host)
    }

    /// Handles a user action. Returns whether the settings changed.
    pub fn on_action(&mut self, action: SuggestionPlacesAction) -> bool {
        let switch = match action {
            SuggestionPlacesAction::SwitchApp { app_id, is_on } => self
                .settings
                .apps
                .iter_mut()
                .find(|app| app.app_id == app_id)
                .map(|app| (&mut app.is_on, is_on)),
            SuggestionPlacesAction::SwitchSite { host, is_on } => self
                .settings
                .sites
                .iter_mut()
                .find(|site| site.host == host)
                .map(|site| (&mut site.is_on, is_on)),
        };
        switch.is_some_and(|(slot, is_on)| replace_if_changed(slot, is_on))
    }

    /// Returns the action of `input`, when `input` is for a control of the pane.
    #[must_use]
    pub fn action_for(&self, input: &SettingsInput) -> Option<SuggestionPlacesAction> {
        let SettingsInput::Toggled { id, is_on } = input else {
            return None;
        };
        let id = id.as_str();
        let is_on = *is_on;
        if let Some(app_id) = id.strip_prefix(APP_SWITCH_PREFIX) {
            return Some(SuggestionPlacesAction::SwitchApp {
                app_id: app_id.to_owned(),
                is_on,
            });
        }
        id.strip_prefix(SITE_SWITCH_PREFIX)
            .map(|host| SuggestionPlacesAction::SwitchSite {
                host: host.to_owned(),
                is_on,
            })
    }

    /// Returns the page that the pane shows now.
    #[must_use]
    pub fn page(&self) -> Page {
        Page {
            title: PANE_TITLE,
            icon: PageIcon::AppGrid,
            summary: PANE_SUMMARY,
            search: None,
            sections: vec![
                app_list_section(self.app_rows(), TYPED_APPS_FOOTER),
                list_section(SITES_HEADING, NO_SITES, self.site_rows(), SITES_FOOTER),
            ],
        }
    }

    fn app_rows(&self) -> Vec<Row> {
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
                    is_enabled: true,
                })
            })
            .collect()
    }

    fn site_rows(&self) -> Vec<Row> {
        self.settings
            .sites
            .iter()
            .map(|site| {
                Row::SiteSwitch(SiteSwitch {
                    id: ControlId::from(format!("{SITE_SWITCH_PREFIX}{}", site.host)),
                    host: site.host.clone(),
                    is_on: site.is_on,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SuggestionSite;
    use crate::settings_window::view::Message;
    use crate::test_support::{suggestion_app, suggestion_site, toggled};

    fn pane_with(apps: Vec<SuggestionApp>, sites: Vec<SuggestionSite>) -> SuggestionPlacesPane {
        SuggestionPlacesPane::new(SuggestionPlaces { apps, sites })
    }

    #[test]
    fn page_shows_a_switch_for_each_app_and_each_website() {
        let pane = pane_with(
            vec![suggestion_app("com.apple.mail", "Mail", false)],
            vec![suggestion_site("bank.example", false)],
        );

        let shown = pane.page();

        assert_eq!(
            shown
                .sections
                .iter()
                .map(|section| (section.heading.as_deref(), section.rows.clone()))
                .collect::<Vec<_>>(),
            [
                (
                    Some("Apps"),
                    vec![Row::AppSwitch(AppSwitch {
                        id: ControlId::from("suggestions-app:com.apple.mail".to_owned()),
                        app_id: "com.apple.mail".to_owned(),
                        app_path: None,
                        app_name: "Mail".to_owned(),
                        is_on: false,
                        is_enabled: true,
                    })]
                ),
                (
                    Some("Websites"),
                    vec![Row::SiteSwitch(SiteSwitch {
                        id: ControlId::from("suggestions-site:bank.example".to_owned()),
                        host: "bank.example".to_owned(),
                        is_on: false,
                    })]
                ),
            ]
        );
    }

    #[test]
    fn empty_lists_say_that_they_have_no_apps_and_no_websites() {
        let pane = pane_with(Vec::new(), Vec::new());

        let shown = pane.page();

        assert_eq!(
            shown
                .sections
                .iter()
                .map(|section| section.rows.clone())
                .collect::<Vec<_>>(),
            [
                vec![Row::Message(Message {
                    text: "This list has no apps.".to_owned(),
                    button: None,
                })],
                vec![Row::Message(Message {
                    text: "This list has no websites.".to_owned(),
                    button: None,
                })],
            ]
        );
    }

    #[test]
    fn switches_give_their_actions() {
        let pane = pane_with(Vec::new(), Vec::new());
        let inputs = [
            toggled("suggestions-app:com.apple.mail", false),
            toggled("suggestions-site:bank.example", true),
            toggled("screen-text-app:com.apple.mail", false),
        ];

        let actions = inputs.map(|input| pane.action_for(&input));

        assert_eq!(
            actions,
            [
                Some(SuggestionPlacesAction::SwitchApp {
                    app_id: "com.apple.mail".to_owned(),
                    is_on: false,
                }),
                Some(SuggestionPlacesAction::SwitchSite {
                    host: "bank.example".to_owned(),
                    is_on: true,
                }),
                None,
            ]
        );
    }

    #[test]
    fn app_switch_changes_its_app() {
        let mut pane = pane_with(
            vec![
                suggestion_app("com.apple.mail", "Mail", true),
                suggestion_app("slack.exe", "Slack", true),
            ],
            Vec::new(),
        );

        let is_changed = pane.on_action(SuggestionPlacesAction::SwitchApp {
            app_id: "slack.exe".to_owned(),
            is_on: false,
        });

        assert_eq!(
            (is_changed, &pane.settings().apps),
            (
                true,
                &vec![
                    suggestion_app("com.apple.mail", "Mail", true),
                    suggestion_app("slack.exe", "Slack", false)
                ]
            )
        );
    }

    #[test]
    fn website_switch_changes_its_website() {
        let mut pane = pane_with(
            Vec::new(),
            vec![
                suggestion_site("bank.example", true),
                suggestion_site("mail.example", true),
            ],
        );

        let is_changed = pane.on_action(SuggestionPlacesAction::SwitchSite {
            host: "bank.example".to_owned(),
            is_on: false,
        });

        assert_eq!(
            (is_changed, &pane.settings().sites),
            (
                true,
                &vec![
                    suggestion_site("bank.example", false),
                    suggestion_site("mail.example", true)
                ]
            )
        );
    }

    #[test]
    fn action_that_changes_nothing_reports_no_change() {
        let mut pane = pane_with(
            vec![suggestion_app("com.apple.mail", "Mail", true)],
            vec![suggestion_site("bank.example", false)],
        );
        let actions = [
            SuggestionPlacesAction::SwitchApp {
                app_id: "com.apple.mail".to_owned(),
                is_on: true,
            },
            SuggestionPlacesAction::SwitchApp {
                app_id: "unknown.exe".to_owned(),
                is_on: false,
            },
            SuggestionPlacesAction::SwitchSite {
                host: "bank.example".to_owned(),
                is_on: false,
            },
            SuggestionPlacesAction::SwitchSite {
                host: "unknown.example".to_owned(),
                is_on: false,
            },
        ];

        let changes = actions.map(|action| pane.on_action(action));

        assert_eq!(changes, [false; 4]);
    }

    #[test]
    fn typed_app_and_website_are_added_one_time() {
        let mut pane = pane_with(Vec::new(), Vec::new());

        let changes = [
            pane.add_app(&suggestion_app("slack.exe", "Slack", true)),
            pane.add_app(&suggestion_app("slack.exe", "Slack", true)),
            pane.add_site("mail.example"),
            pane.add_site("mail.example"),
        ];

        assert_eq!(changes, [true, false, true, false]);
    }
}
