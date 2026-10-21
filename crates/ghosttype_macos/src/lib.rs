//! macOS platform layer of GhostType.
//!
//! Wraps AppKit, Core Graphics and Accessibility behind safe types: the keyboard and mouse event
//! tap, reading the caret, the ghost-text overlay, the badge at the caret, text injection, the
//! menu bar extra and the settings window.

#![cfg(target_os = "macos")]

mod accessibility;
mod badge;
mod badge_menu;
mod clipboard;
mod event_tap;
mod focus;
mod geometry;
mod injection;
mod keychain;
mod keys;
mod labels;
mod launch_agent;
mod main_menu;
mod menu_bar;
mod mirror;
mod ocr;
mod overlay;
mod runtime;
mod screen_text;
mod settings;
mod single_instance;

pub use keychain::Keychain;
pub use launch_agent::LaunchAgent;

use ghosttype_core::controller::Services;
use ghosttype_core::settings::Settings;

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

/// Runs GhostType in the menu bar until the user quits.
///
/// GhostType starts with the saved `settings` and works through `services`. The suggestion model
/// of `settings` starts to load at once.
///
/// Returns immediately when another instance is already running for this user. Quitting from the
/// menu ends the process.
///
/// # Errors
///
/// Returns [`RunError`] when called off the main thread, when the instance lock can't be checked,
/// or when macOS denies the keyboard event tap.
pub fn run(settings: Settings, services: Services) -> Result<(), RunError> {
    let Some(_instance_lock) = single_instance::acquire().map_err(RunError::InstanceCheck)? else {
        tracing::info!("another instance is already running");
        return Ok(());
    };
    runtime::run(settings, services)
}
