//! TypeAlike: system-wide inline autocomplete for Windows and macOS.
//!
//! The binary only does the wiring: the data folders, the settings load, the logging and the
//! platform event loop.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod logging;
mod platform;

fn main() -> anyhow::Result<()> {
    logging::start();
    platform::run()
}
