//! Windows platform layer of GhostType.
//!
//! Wraps Win32 behind safe types: keyboard and mouse hooks, reading the caret, the ghost-text
//! overlay, text injection, the tray icon and the settings window.

#![cfg(windows)]

mod focus;
mod gdi;
mod injection;
mod keys;
mod mailbox;
mod menu;
mod overlay;
mod pixels;
mod runtime;
mod settings;
mod single_instance;
mod system_theme;
mod tray;
mod window;

use ghosttype_core::app::App;
use ghosttype_core::hub::ModelHub;
use ghosttype_core::inference::ModelRunner;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};

/// A failure that stops GhostType from starting or running.
#[derive(Debug, thiserror::Error)]
#[error("failed to {operation}")]
pub struct RunError {
    operation: &'static str,
    #[source]
    source: windows::core::Error,
}

impl RunError {
    fn during(operation: &'static str) -> impl FnOnce(windows::core::Error) -> Self {
        move |source| Self { operation, source }
    }
}

/// Runs GhostType in the tray until the user quits, offering suggestions from `app`.
///
/// The settings window finds, downloads and deletes models through `hub`. Its Playground pane
/// loads a model into `runner`, and that model completes the text that the user types.
///
/// Returns immediately when another instance is already running in this session.
///
/// # Errors
///
/// Returns [`RunError`] when a required Windows resource (window, hook, tray icon) can't be created
/// or the message loop fails.
pub fn run(app: App, hub: Box<dyn ModelHub>, runner: Box<dyn ModelRunner>) -> Result<(), RunError> {
    let Some(_instance_lock) =
        single_instance::acquire().map_err(RunError::during("check for a running instance"))?
    else {
        tracing::info!("another instance is already running");
        return Ok(());
    };
    // SAFETY: plain call made before any window exists.
    if let Err(error) = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } {
        tracing::warn!(%error, "per-monitor dpi awareness unavailable");
    }
    runtime::run(app, hub, runner)
}
