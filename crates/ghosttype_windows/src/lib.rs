//! Windows platform layer of GhostType.
//!
//! It wraps Win32 behind safe types:
//!
//! - the keyboard, mouse and focus hooks
//! - the caret (the system caret, else the caret object, else UI Automation) and the field text
//! - the screen text, from UI Automation or from OCR
//! - the clipboard
//! - the ghost text overlay and the badge at the caret
//! - text injection
//! - the tray icon and the settings window
//! - the startup entry, the key file of the earlier writing and the CPU cores

#![cfg(windows)]

mod badge;
mod caret;
mod caret_reader;
mod clipboard;
mod cpu_cores;
mod display;
mod energy_saver;
mod focus;
mod game_watch;
mod gdi;
mod injection;
mod key_file;
mod keys;
mod mailbox;
mod menu;
mod ocr;
mod overlay;
mod pixels;
mod registry;
mod runtime;
mod screen_text;
mod settings;
mod single_instance;
mod startup;
mod system_theme;
mod tray;
mod uia;
mod utf16;
mod window;

use ghosttype_core::controller::Services;
use ghosttype_core::settings::Settings;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};

pub use crate::cpu_cores::cpu_cores;
pub use crate::key_file::ProtectedKeyFile;
pub use crate::startup::StartupEntry;

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

/// The size of the Win32 structure `T`, for the field or the argument that tells Win32 its size.
fn struct_size<T>() -> u32 {
    u32::try_from(size_of::<T>()).expect("a Win32 structure is smaller than 4 GiB")
}

/// Runs GhostType in the tray until the user quits.
///
/// GhostType starts with the saved `settings` and works through `services`. The suggestion model
/// of `settings` starts to load at once.
///
/// Returns immediately when another instance is already running in this session.
///
/// # Errors
///
/// Returns [`RunError`] when a required Windows resource (window, hook, tray icon) can't be created
/// or the message loop fails.
pub fn run(settings: Settings, services: Services) -> Result<(), RunError> {
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
    runtime::run(settings, services)
}
