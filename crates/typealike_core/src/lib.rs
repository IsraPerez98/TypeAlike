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

/// The `tracing` target of the records that hold what the user wrote, what the screen shows and
/// what the model suggests. Only a build with the `suggestion-log` feature writes them, and only
/// to the suggestion log, which is off until the user makes its file. The other logs never take
/// this target.
pub const SUGGESTION_LOG_TARGET: &str = "typealike_suggestions";
