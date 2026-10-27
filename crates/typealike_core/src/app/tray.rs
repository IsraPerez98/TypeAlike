//! The tray icon and its menu: what they show and the commands of the menu.

use super::Status;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayItem {
    /// Text shown for the entry, in sentence case.
    pub label: &'static str,
    /// Command run when the entry is picked.
    pub command: TrayCommand,
}

/// Everything the platform needs to render the tray icon and its menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayView {
    /// Status the icon glyph must reflect.
    pub status: Status,
    /// Tooltip naming the app and its status.
    pub tooltip: &'static str,
    /// Menu entries, top to bottom.
    pub items: Vec<TrayItem>,
}

impl TrayView {
    /// The tray icon and its menu while TypeAlike has `status`.
    pub(super) fn of(status: Status) -> Self {
        Self {
            status,
            tooltip: status.name(),
            items: vec![
                TrayItem {
                    label: status.toggle_label(),
                    command: TrayCommand::TogglePause,
                },
                TrayItem {
                    label: "Settings…",
                    command: TrayCommand::OpenSettings,
                },
                TrayItem {
                    label: "Quit TypeAlike",
                    command: TrayCommand::Quit,
                },
            ],
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
