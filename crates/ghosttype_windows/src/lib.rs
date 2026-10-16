//! Windows platform layer of GhostType.
//!
//! Wraps Win32 behind safe types: keyboard and mouse hooks, reading the caret, the ghost-text
//! overlay, text injection, the tray icon and the settings window.

#![cfg(windows)]

mod clipboard;
mod desktop;
mod focus;
mod gdi;
mod injection;
mod keys;
mod mailbox;
mod menu;
mod overlay;
mod pixels;
mod registry;
mod runtime;
mod settings;
mod single_instance;
mod system_theme;
mod tray;
mod window;

use std::fmt;

use ghosttype_core::app::App;
use ghosttype_core::hub::ModelHub;
use ghosttype_core::inference::ModelRunner;
use ghosttype_core::settings::{Settings, SettingsStore};
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

/// The services that GhostType uses on Windows.
pub struct Services {
    /// Finds, downloads and deletes models for the Models pane.
    pub hub: Box<dyn ModelHub>,
    /// Loads the model that the Playground pane selects, and completes the text the user types.
    pub runner: Box<dyn ModelRunner>,
    /// Keeps the settings that the user changes in the General pane.
    pub settings_store: Box<dyn SettingsStore>,
}

impl fmt::Debug for Services {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Services").finish_non_exhaustive()
    }
}

/// Runs GhostType in the tray until the user quits, offering suggestions from `app`.
///
/// The settings window starts with the saved `settings` and works through `services`.
///
/// Returns immediately when another instance is already running in this session.
///
/// # Errors
///
/// Returns [`RunError`] when a required Windows resource (window, hook, tray icon) can't be created
/// or the message loop fails.
pub fn run(app: App, settings: Settings, services: Services) -> Result<(), RunError> {
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
    runtime::run(app, settings, services)
}
