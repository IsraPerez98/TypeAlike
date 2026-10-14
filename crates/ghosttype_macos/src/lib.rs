//! macOS platform layer of GhostType.
//!
//! Wraps AppKit, Core Graphics and Accessibility behind safe types: the keyboard and mouse event
//! tap, reading the caret, the ghost-text overlay, text injection, the menu bar extra and the
//! settings window.

#![cfg(target_os = "macos")]

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

use ghosttype_core::app::App;
use ghosttype_core::hub::ModelHub;
use ghosttype_core::inference::ModelRunner;

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

/// Runs GhostType in the menu bar until the user quits, offering suggestions from `app`.
///
/// The settings window finds, downloads and deletes models through `hub`, and tries them with
/// `runner`.
///
/// Returns immediately when another instance is already running for this user. Quitting from the
/// menu ends the process.
///
/// # Errors
///
/// Returns [`RunError`] when called off the main thread, when the instance lock can't be checked,
/// or when macOS denies the keyboard event tap.
pub fn run(app: App, hub: Box<dyn ModelHub>, runner: Box<dyn ModelRunner>) -> Result<(), RunError> {
    let Some(_instance_lock) = single_instance::acquire().map_err(RunError::InstanceCheck)? else {
        tracing::info!("another instance is already running");
        return Ok(());
    };
    runtime::run(app, hub, runner)
}
