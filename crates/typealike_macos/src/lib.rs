//! macOS platform layer of TypeAlike.
//!
//! Wraps AppKit, Core Graphics and Accessibility behind safe types: the keyboard and mouse event
//! tap, reading the caret, the ghost-text overlay, the badge at the caret, text injection, the
//! menu bar extra and the settings window.

#![cfg(target_os = "macos")]

mod accessibility;
mod account;
mod badge;
mod badge_menu;
mod clipboard;
mod energy_saver;
mod event_tap;
mod focus;
mod full_screen;
mod game;
mod geometry;
mod injection;
mod keychain;
mod keys;
mod labels;
mod language;
mod launch_agent;
mod main_menu;
mod menu_bar;
mod mirror;
mod ocr;
mod overlay;
mod panel;
mod permissions;
mod runtime;
mod screen_text;
mod settings;
mod single_instance;
mod spelling;
mod symbol;

pub use account::account_name;
pub use keychain::Keychain;
pub use language::ui_language;
pub use launch_agent::LaunchAgent;
pub use spelling::SpellingDictionary;

use typealike_core::controller::Services;
use typealike_core::settings::Settings;

/// A failure that stops TypeAlike from starting or running.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// AppKit can only run on the process's main thread.
    #[error("typealike must run on the main thread")]
    NotMainThread,
    /// The lock that keeps one TypeAlike per user could not be checked.
    #[error("failed to check for a running instance")]
    InstanceCheck(#[source] std::io::Error),
}

/// Runs TypeAlike in the menu bar until the user quits.
///
/// TypeAlike starts with the saved `settings` and works through `services`. The suggestion model
/// of `settings` starts to load at once.
///
/// Returns immediately when another instance is already running for this user. Quitting from the
/// menu ends the process.
///
/// # Errors
///
/// Returns [`RunError`] when called off the main thread, or when the instance lock cannot be
/// checked. Without the Accessibility permission, TypeAlike runs and waits for it.
pub fn run(settings: Settings, services: Services) -> Result<(), RunError> {
    let Some(_instance_lock) = single_instance::acquire().map_err(RunError::InstanceCheck)? else {
        tracing::info!("another instance is already running");
        return Ok(());
    };
    runtime::run(settings, services)
}
