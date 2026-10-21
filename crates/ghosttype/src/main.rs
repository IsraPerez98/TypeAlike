//! GhostType: system-wide inline autocomplete for Windows and macOS.
//!
//! The binary only does the wiring: the data folders, the settings load, the logging and the
//! platform event loop.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod platform;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    platform::run()
}
