//! The badge at the caret and its menu: what they show and the commands of the menu.

use std::slice;

use super::Status;
use super::field_place::FieldPlace;
use crate::desktop::ScreenRect;
use crate::localization::{MessageArgument, Translations, UiLanguage};
use crate::settings::{
    HistoryAccess, HistoryScope, PersonalizationSettings, PrivateWindowRule, ScreenTextAccess,
    ScreenTextSettings, SuggestionPlaces,
};

/// The badge at the caret of the field where the user types: a button that opens the
/// [`BadgeMenu`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadgeView {
    /// Status the glyph must reflect: paused also in a place where suggestions are off.
    pub status: Status,
    /// The name of the app and its status, for assistive technology.
    pub label: String,
    /// Where the badge shows: at the end of the line of the caret, inside the field. Only a shell
    /// that gives the side of its badge
    /// ([`Shell::badge_side`](crate::controller::Shell::badge_side)) gets a frame. `None` for a
    /// shell that puts the badge at the caret by itself.
    pub frame: Option<ScreenRect>,
}

/// The settings that the entries of the badge menu show and change.
#[derive(Debug, Clone, Copy)]
pub struct BadgeSettings<'settings> {
    /// The apps and the websites where suggestions show.
    pub suggestion_places: &'settings SuggestionPlaces,
    /// The apps where TypeAlike reads the screen text.
    pub screen_text: &'settings ScreenTextSettings,
    /// The earlier writing of each app.
    pub personalization: &'settings PersonalizationSettings,
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
        /// Whether TypeAlike reads the screen text in the app after the command.
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
    /// Selects whether suggestions stop in private windows.
    SwitchPrivateWindows(PrivateWindowRule),
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
    /// The language of the texts.
    pub language: UiLanguage,
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
    /// use typealike_core::app::{BadgeCommand, BadgeEntry, BadgeItem, BadgeMenu};
    /// use typealike_core::localization::UiLanguage;
    ///
    /// let pause = BadgeItem {
    ///     label: "Pause suggestions".to_owned(),
    ///     command: BadgeCommand::TogglePause,
    ///     is_checked: false,
    ///     is_enabled: true,
    /// };
    /// let menu = BadgeMenu {
    ///     entries: vec![BadgeEntry::Separator, BadgeEntry::Item(pause)],
    ///     language: UiLanguage::English,
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

/// The entries of the badge menu for `place`, with the texts of `translations`: the switch that
/// stops suggestions in private windows when `place` is a private window, the suggestion switches
/// of its app and of its website, then the screen text switch and the menu of the scope of its app,
/// each when the list of `settings` has the app or the website. The entries of the app are dimmed
/// while suggestions are off in `place`.
pub(super) fn place_entries(
    place: &FieldPlace,
    settings: &BadgeSettings<'_>,
    translations: &Translations,
) -> Vec<BadgeEntry> {
    let suggestion_places = settings.suggestion_places;
    let is_stopped_as_private = place.is_stopped_as_private(suggestion_places);
    let private_window_switch = place.is_private_window.then(|| {
        let picked_rule = if is_stopped_as_private {
            PrivateWindowRule::KeepSuggestions
        } else {
            PrivateWindowRule::StopSuggestions
        };
        suggestion_switch(
            translations.text("badge-private-windows"),
            BadgeCommand::SwitchPrivateWindows(picked_rule),
            is_stopped_as_private,
        )
    });
    let app_switch = place
        .app
        .as_ref()
        .and_then(|typed| {
            suggestion_places
                .apps
                .iter()
                .find(|app| app.app_id == typed.app_id)
        })
        .map(|app| {
            suggestion_switch(
                translations.text_with(
                    "badge-app-suggestions",
                    &[("app", MessageArgument::Text(&app.app_name))],
                ),
                BadgeCommand::SwitchAppSuggestions {
                    app_id: app.app_id.clone(),
                    is_on: !app.is_on,
                },
                app.is_on,
            )
        });
    let site_switch = place
        .host
        .as_deref()
        .and_then(|host| suggestion_places.sites.iter().find(|site| site.host == host))
        .map(|site| {
            suggestion_switch(
                translations.text_with(
                    "badge-site-suggestions",
                    &[("host", MessageArgument::Text(&site.host))],
                ),
                BadgeCommand::SwitchSiteSuggestions {
                    host: site.host.clone(),
                    is_on: !site.is_on,
                },
                site.is_on,
            )
        });
    let is_place_on = place.is_on_in(suggestion_places);
    let app_entries = place
        .app
        .as_ref()
        .map(|app| app_entries(&app.app_id, settings, is_place_on, translations))
        .unwrap_or_default();
    [private_window_switch, app_switch, site_switch]
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
/// of its scope, each when the list of `settings` has the app. An entry is dimmed while its main
/// switch is off or `is_place_on` is false.
fn app_entries(
    app_id: &str,
    settings: &BadgeSettings<'_>,
    is_place_on: bool,
    translations: &Translations,
) -> Vec<BadgeEntry> {
    let BadgeSettings {
        screen_text,
        personalization,
        ..
    } = *settings;
    let screen_switch = screen_text
        .apps
        .iter()
        .find(|app| app.app_id == app_id)
        .map(|app| {
            BadgeEntry::Item(BadgeItem {
                label: translations.text_with(
                    "badge-app-screen-text",
                    &[("app", MessageArgument::Text(&app.app_name))],
                ),
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
            label: translations.text_with(
                "badge-app-earlier-writing",
                &[("app", MessageArgument::Text(&app.app_name))],
            ),
            items: HistoryScope::CHOICES
                .iter()
                .map(|&(scope, message)| BadgeItem {
                    label: translations.text(message),
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
