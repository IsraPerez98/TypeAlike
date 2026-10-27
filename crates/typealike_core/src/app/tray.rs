//! The tray icon and its menu: what they show and the commands of the menu.

use super::Status;
use crate::localization::{Translations, UiLanguage};

/// An action the user can pick from the tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    /// Pauses suggestions when active, resumes them when paused.
    TogglePause,
    /// Shows the settings window.
    OpenSettings,
    /// Exits TypeAlike.
    Quit,
}

/// One entry of the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayItem {
    /// Text shown for the entry, in sentence case.
    pub label: String,
    /// Command run when the entry is picked.
    pub command: TrayCommand,
}

/// Everything the platform needs to render the tray icon and its menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayView {
    /// Status the icon glyph must reflect.
    pub status: Status,
    /// Tooltip naming the app and its status.
    pub tooltip: String,
    /// Menu entries, top to bottom.
    pub items: Vec<TrayItem>,
    /// The language of the texts.
    pub language: UiLanguage,
}

impl TrayView {
    /// The tray icon and its menu while TypeAlike has `status`, with the texts of `translations`.
    pub(super) fn of(status: Status, translations: &Translations) -> Self {
        let item = |message, command| TrayItem {
            label: translations.text(message),
            command,
        };
        Self {
            status,
            tooltip: translations.text(status.name_message()),
            items: vec![
                item(status.toggle_message(), TrayCommand::TogglePause),
                item("menu-settings", TrayCommand::OpenSettings),
                item("menu-quit", TrayCommand::Quit),
            ],
            language: translations.language(),
        }
    }
}

/// What the controller does after a tray command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayOutcome {
    /// Suggestions stopped: hide any ghost text, show the tray and the badge again, and unload
    /// the suggestion model to free its memory.
    Paused,
    /// Suggestions started again: show the tray and the badge again, and load the suggestion
    /// model again.
    Resumed,
    /// The settings window must show and come to the front.
    OpenSettings,
    /// The app must exit.
    Quit,
}
