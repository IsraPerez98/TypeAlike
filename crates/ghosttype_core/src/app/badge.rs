//! The badge at the caret and its menu: what they show and the commands of the menu.

use std::slice;

use super::Status;
use crate::settings::{
    HistoryAccess, HistoryScope, PersonalizationSettings, ScreenTextAccess, ScreenTextApp,
    ScreenTextSettings, SuggestionPlaces,
};

/// The badge at the caret of the field where the user types: a button that opens the
/// [`BadgeMenu`]. It shows above the caret, on the side opposite to the ghost text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadgeView {
    /// Status the glyph must reflect: paused also in a place where suggestions are off.
    pub status: Status,
    /// The name of the app and its status, for assistive technology.
    pub label: &'static str,
}

/// An action the user can pick from the menu of the badge at the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BadgeCommand {
    /// Pauses suggestions when active, resumes them when paused.
    TogglePause,
    /// Turns the screen text of the app with this id on or off.
    SwitchScreenText {
        /// The id of the app.
        app_id: String,
        /// Whether GhostType reads the screen text in the app after the command.
        is_on: bool,
    },
    /// Turns suggestions in the app with this id on or off.
    SwitchAppSuggestions {
        /// The id of the app.
        app_id: String,
        /// Whether suggestions show in the app after the command.
        is_on: bool,
    },
    /// Turns suggestions on the website with this host on or off.
    SwitchSiteSuggestions {
        /// The host of the website.
        host: String,
        /// Whether suggestions show on the website after the command.
        is_on: bool,
    },
    /// Selects which earlier writing the app with this id gives and gets.
    SelectHistoryScope {
        /// The id of the app.
        app_id: String,
        /// The scope of the app after the command.
        scope: HistoryScope,
    },
}

/// An entry of the badge menu that runs a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeItem {
    /// Text shown for the entry, in sentence case.
    pub label: String,
    /// Command run when the entry is picked.
    pub command: BadgeCommand,
    /// Whether the entry shows a check mark.
    pub is_checked: bool,
    /// Whether the user can pick the entry.
    pub is_enabled: bool,
}

/// A line of the menu of the badge at the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BadgeEntry {
    /// An entry that runs a command.
    Item(BadgeItem),
    /// A line between two groups of entries.
    Separator,
    /// An entry that opens a submenu.
    Submenu {
        /// Text shown for the entry, in sentence case.
        label: String,
        /// The entries of the submenu, top to bottom.
        items: Vec<BadgeItem>,
        /// Whether the user can open the submenu.
        is_enabled: bool,
    },
}

/// The menu that a click on the badge at the caret shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeMenu {
    /// The lines of the menu, top to bottom.
    pub entries: Vec<BadgeEntry>,
}

impl BadgeMenu {
    /// The entries that run a command, top to bottom, with the entries of each submenu in the
    /// place of the submenu.
    pub fn items(&self) -> impl Iterator<Item = &BadgeItem> {
        self.entries.iter().flat_map(|entry| match entry {
            BadgeEntry::Item(item) => slice::from_ref(item),
            BadgeEntry::Separator => &[],
            BadgeEntry::Submenu { items, .. } => items.as_slice(),
        })
    }

    /// The command of the entry at `index` in [`items`](Self::items).
    ///
    /// ```
    /// use ghosttype_core::app::{BadgeCommand, BadgeEntry, BadgeItem, BadgeMenu};
    ///
    /// let pause = BadgeItem {
    ///     label: "Pause suggestions".to_owned(),
    ///     command: BadgeCommand::TogglePause,
    ///     is_checked: false,
    ///     is_enabled: true,
    /// };
    /// let menu = BadgeMenu {
    ///     entries: vec![BadgeEntry::Separator, BadgeEntry::Item(pause)],
    /// };
    ///
    /// assert_eq!(menu.command(0), Some(&BadgeCommand::TogglePause));
    /// assert_eq!(menu.command(1), None);
    /// ```
    #[must_use]
    pub fn command(&self, index: usize) -> Option<&BadgeCommand> {
        self.items().nth(index).map(|item| &item.command)
    }
}

/// The app and the website of the field where the user types.
pub(super) struct TypedPlace<'place> {
    /// The app, if it has an id.
    pub(super) app: Option<&'place ScreenTextApp>,
    /// The host of the website, if any.
    pub(super) host: Option<&'place str>,
}

/// The entries of the badge menu for `place`: the suggestion switches of its app and of its
/// website, then the screen text switch and the menu of the scope of its app, each when its list
/// has the app or the website. The entries of the app are dimmed while suggestions are off in
/// `place`.
pub(super) fn place_entries(
    place: &TypedPlace<'_>,
    suggestion_places: &SuggestionPlaces,
    screen_text: &ScreenTextSettings,
    personalization: &PersonalizationSettings,
) -> Vec<BadgeEntry> {
    let app_switch = place
        .app
        .and_then(|typed| {
            suggestion_places
                .apps
                .iter()
                .find(|app| app.app_id == typed.app_id)
        })
        .map(|app| {
            suggestion_switch(
                format!("Show suggestions in {}", app.app_name),
                BadgeCommand::SwitchAppSuggestions {
                    app_id: app.app_id.clone(),
                    is_on: !app.is_on,
                },
                app.is_on,
            )
        });
    let site_switch = place
        .host
        .and_then(|host| suggestion_places.sites.iter().find(|site| site.host == host))
        .map(|site| {
            suggestion_switch(
                format!("Show suggestions on {}", site.host),
                BadgeCommand::SwitchSiteSuggestions {
                    host: site.host.clone(),
                    is_on: !site.is_on,
                },
                site.is_on,
            )
        });
    let app_id = place.app.map(|app| app.app_id.as_str());
    let is_place_on = suggestion_places.allows(app_id, place.host);
    let app_entries = app_id
        .map(|app_id| app_entries(app_id, screen_text, personalization, is_place_on))
        .unwrap_or_default();
    [app_switch, site_switch]
        .into_iter()
        .flatten()
        .chain(app_entries)
        .collect()
}

/// An entry that turns suggestions on or off with `command`, checked while they are on.
fn suggestion_switch(label: String, command: BadgeCommand, is_on: bool) -> BadgeEntry {
    BadgeEntry::Item(BadgeItem {
        label,
        command,
        is_checked: is_on,
        is_enabled: true,
    })
}

/// The entries of the badge menu for the app with `app_id`: its screen text switch and the menu
/// of its scope, each when its list has the app. An entry is dimmed while its main switch is off
/// or `is_place_on` is false.
fn app_entries(
    app_id: &str,
    screen_text: &ScreenTextSettings,
    personalization: &PersonalizationSettings,
    is_place_on: bool,
) -> Vec<BadgeEntry> {
    let screen_switch = screen_text
        .apps
        .iter()
        .find(|app| app.app_id == app_id)
        .map(|app| {
            BadgeEntry::Item(BadgeItem {
                label: format!("Use screen text in {}", app.app_name),
                command: BadgeCommand::SwitchScreenText {
                    app_id: app.app_id.clone(),
                    is_on: !app.is_on,
                },
                is_checked: app.is_on,
                is_enabled: is_place_on && screen_text.access == ScreenTextAccess::Allowed,
            })
        });
    let is_history_on = is_place_on && personalization.history == HistoryAccess::On;
    let scope_menu = personalization
        .apps
        .iter()
        .find(|app| app.app_id == app_id)
        .map(|app| BadgeEntry::Submenu {
            label: format!("Earlier writing in {}", app.app_name),
            items: HistoryScope::CHOICES
                .iter()
                .map(|&(scope, title)| BadgeItem {
                    label: title.to_owned(),
                    command: BadgeCommand::SelectHistoryScope {
                        app_id: app.app_id.clone(),
                        scope,
                    },
                    is_checked: scope == app.scope,
                    is_enabled: is_history_on,
                })
                .collect(),
            is_enabled: is_history_on,
        });
    [screen_switch, scope_menu].into_iter().flatten().collect()
}
