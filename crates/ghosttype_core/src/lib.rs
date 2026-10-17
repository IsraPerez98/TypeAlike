//! Platform-independent core of GhostType.
//!
//! Suggestion matching, acceptance logic and app state live here. This crate must not depend on
//! any OS API, so it builds and tests on every platform.

pub mod app;
mod catalog;
pub mod controller;
pub mod desktop;
pub mod engine;
pub mod general_pane;
pub mod hub;
pub mod inference;
pub mod key;
pub mod models_pane;
mod place;
pub mod playground;
mod prompt;
pub mod settings;
pub mod settings_view;
