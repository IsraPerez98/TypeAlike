//! macOS platform layer of GhostType.
//!
//! Wraps AppKit, Core Graphics and Accessibility behind safe types: the keyboard and mouse event
//! tap, reading the caret, the ghost-text overlay, text injection and the menu bar extra.

#![cfg(target_os = "macos")]

mod event_tap;
mod focus;
mod injection;
mod keys;
mod labels;
mod menu_bar;
mod overlay;
mod runtime;
mod single_instance;

use ghosttype_core::app::App;

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
    #[error("failed to watch the keyboard; grant accessibility access and start ghosttype again")]
    KeyboardAccess,
}

/// Runs GhostType in the menu bar until the user quits, offering suggestions from `app`.
///
/// Returns immediately when another instance is already running for this user. Quitting from the
/// menu ends the process.
///
/// # Errors
///
/// Returns [`RunError`] when called off the main thread, when the instance lock can't be checked,
/// or when macOS denies the keyboard event tap.
pub fn run(app: App) -> Result<(), RunError> {
    let Some(_instance_lock) = single_instance::acquire().map_err(RunError::InstanceCheck)? else {
        tracing::info!("another instance is already running");
        return Ok(());
    };
    runtime::run(app)
}
