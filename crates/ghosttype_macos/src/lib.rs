//! macOS platform layer of GhostType.
//!
//! Wraps AppKit, Core Graphics and Accessibility behind safe types: the keyboard and mouse event
//! tap, reading the caret, the ghost-text overlay, text injection, the menu bar extra and the
//! settings window.

#![cfg(target_os = "macos")]

mod clipboard;
mod event_tap;
mod focus;
mod injection;
mod keys;
mod labels;
mod main_menu;
mod menu_bar;
mod overlay;
mod runtime;
mod settings;
mod single_instance;

use std::fmt;

use ghosttype_core::app::App;
use ghosttype_core::hub::ModelHub;
use ghosttype_core::inference::ModelRunner;
use ghosttype_core::settings::{Autostart, Settings, SettingsStore};

/// A failure that stops GhostType from starting or running.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// AppKit can only run on the process's main thread.
    #[error("ghosttype must run on the main thread")]
    NotMainThread,
    /// The lock that keeps one GhostType per user could not be checked.
    #[error("failed to check for a running instance")]
    InstanceCheck(#[source] std::io::Error),
    /// macOS refused the keyboard event tap, almost always for lack of Accessibility access.
    #[error("ghosttype needs accessibility access to watch the keyboard")]
    KeyboardAccess,
}

/// The services that GhostType uses on macOS.
pub struct Services {
    /// Finds, downloads and deletes models for the Models pane.
    pub hub: Box<dyn ModelHub>,
    /// Loads the model that the Playground pane selects, and completes the text the user types.
    pub runner: Box<dyn ModelRunner>,
    /// Keeps the settings that the user changes in the General pane.
    pub settings_store: Box<dyn SettingsStore>,
    /// Starts GhostType at login when the user turns it on in the General pane.
    pub autostart: Box<dyn Autostart>,
}

impl fmt::Debug for Services {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Services").finish_non_exhaustive()
    }
}

/// Runs GhostType in the menu bar until the user quits, offering suggestions from `app`.
///
/// The settings window starts with the saved `settings` and works through `services`. The
/// suggestion model of `settings` starts to load at once.
///
/// Returns immediately when another instance is already running for this user. Quitting from the
/// menu ends the process.
///
/// # Errors
///
/// Returns [`RunError`] when called off the main thread, when the instance lock can't be checked,
/// or when macOS denies the keyboard event tap.
pub fn run(app: App, settings: Settings, services: Services) -> Result<(), RunError> {
    let Some(_instance_lock) = single_instance::acquire().map_err(RunError::InstanceCheck)? else {
        tracing::info!("another instance is already running");
        return Ok(());
    };
    runtime::run(app, settings, services)
}
