//! GhostType: system-wide inline autocomplete for Windows and macOS.
//!
//! The binary only wires things together: configuration, logging and the platform event loop.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod platform;

use ghosttype_core::app::App;
use ghosttype_core::phrases::PhraseList;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    let app = App::new(Box::new(PhraseList::default()));
    platform::run(app)
}
