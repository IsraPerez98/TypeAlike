//! Platform-independent core of TypeAlike.
//!
//! Suggestion matching, acceptance logic and app state live here. This crate must not depend on
//! any OS API, so it builds and tests on every platform.

pub mod app;
pub mod controller;
pub mod desktop;
pub mod engine;
mod excerpt;
pub mod history;
pub mod hub;
pub mod inference;
pub mod key;
pub mod language;
pub mod localization;
mod place;
mod private_window;
mod prompt;
pub mod release;
mod screen;
pub mod settings;
pub mod settings_window;
pub mod setup;
pub mod spelling;
#[cfg(test)]
mod test_support;
